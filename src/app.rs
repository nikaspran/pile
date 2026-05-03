use std::time::Duration;

use crop::Rope;
use crossbeam_channel::{Sender, bounded};
use eframe::egui;
use regex::Regex;
use tracing::{info, warn};

use crate::{
    editor::{EditorViewState, SearchHighlight, replace_all_matches, replace_match, show_editor},
    model::{AppState, DocumentId, Selection, SessionSnapshot},
    native_menu::{NativeMenu, NativeMenuCommand},
    persistence::{
        SaveMsg, SaveWorker, default_session_path, load_session, quarantine_corrupt_session,
    },
    search::{SearchMatch, SearchOptions, GlobalSearchResult, advance_match, find_matches, find_matches_in_documents},
    syntax::{LanguageDetection, LanguageRegistry},
};

#[derive(Clone, Debug, Default)]
struct SearchState {
    visible: bool,
    replace_visible: bool,
    query: String,
    replacement: String,
    case_sensitive: bool,
    whole_word: bool,
    use_regex: bool,
    search_all_tabs: bool,
    matches: Vec<SearchMatch>,
    current_match: Option<usize>,
    focus_pending: bool,
    selection_pending: bool,
    occurrence_selections: Vec<Selection>,
    global_results: Vec<GlobalSearchResult>,
    global_index: Option<usize>,
}

impl SearchState {
    fn recompute(&mut self, rope: &Rope, documents: &[crate::model::Document]) {
        let old_range = self
            .current_match
            .and_then(|index| self.matches.get(index).copied());
        let options = SearchOptions {
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            use_regex: self.use_regex,
        };

        if self.search_all_tabs {
            self.global_results = find_matches_in_documents(documents, &self.query, options);
            self.matches.clear();
            self.current_match = None;
            self.global_index = if self.global_results.is_empty() {
                None
            } else if let Some(old) = old_range {
                self.global_results
                    .iter()
                    .position(|r| r.match_start == old.start && r.match_end == old.end)
                    .or(Some(0))
            } else {
                Some(0)
            };
        } else {
            self.matches = find_matches(rope, &self.query, options);
            self.global_results.clear();
            self.global_index = None;
            self.current_match = if self.matches.is_empty() {
                None
            } else if let Some(old_range) = old_range {
                self.matches
                    .iter()
                    .position(|range| *range == old_range)
                    .or(Some(0))
            } else {
                Some(0)
            };
        }
    }

    fn next_match(&mut self) {
        if self.search_all_tabs {
            self.global_index = advance_match(self.global_index, self.global_results.len(), 1);
        } else {
            self.current_match = advance_match(self.current_match, self.matches.len(), 1);
        }
        self.selection_pending = true;
    }

    fn previous_match(&mut self) {
        if self.search_all_tabs {
            self.global_index = advance_match(self.global_index, self.global_results.len(), -1);
        } else {
            self.current_match = advance_match(self.current_match, self.matches.len(), -1);
        }
        self.selection_pending = true;
    }

    fn current_label(&self) -> String {
        if self.search_all_tabs {
            match (self.global_index, self.global_results.len()) {
                (_, 0) => "0 / 0".to_owned(),
                (Some(index), total) => format!("{} / {total}", index + 1),
                (None, total) => format!("0 / {total}"),
            }
        } else {
            match (self.current_match, self.matches.len()) {
                (_, 0) => "0 / 0".to_owned(),
                (Some(index), total) => format!("{} / {total}", index + 1),
                (None, total) => format!("0 / {total}"),
            }
        }
    }

    fn select_next_occurrence(&mut self, rope: &Rope, primary: Selection) {
        let (query, _) = if let Some((start, end)) =
            crate::editor::word_at_selection(rope, primary)
        {
            let text = rope.byte_slice(start..end).to_string();
            if text.is_empty() {
                return;
            }
            (text, Selection { anchor: start, head: end })
        } else {
            return;
        };

        if self.occurrence_selections.is_empty() {
            self.occurrence_selections.push(primary);
            self.query = query.clone();
        }

        let options = SearchOptions {
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            use_regex: self.use_regex,
        };
        let matches = find_matches(rope, &query, options);

        let all_selected: std::collections::HashSet<_> = self
            .occurrence_selections
            .iter()
            .map(|s| (s.anchor.min(s.head), s.anchor.max(s.head)))
            .collect();

        let next = matches
            .iter()
            .find(|m| !all_selected.contains(&(m.start, m.end)))
            .copied();

        if let Some(m) = next {
            let sel = Selection {
                anchor: m.start,
                head: m.end,
            };
            self.occurrence_selections.push(sel);
        }
    }

    fn find_under_cursor(&mut self, rope: &Rope, primary: Selection) {
        self.occurrence_selections.clear();
        let (start, end) = if let Some((s, e)) =
            crate::editor::word_at_selection(rope, primary)
        {
            (s, e)
        } else {
            return;
        };
        if start == end {
            return;
        }
        let text = rope.byte_slice(start..end).to_string();
        if text.is_empty() {
            return;
        }
        self.query = text;
        self.recompute(rope, &[]);
        self.occurrence_selections
            .push(Selection { anchor: start, head: end });
    }
}

pub struct PileApp {
    state: AppState,
    save_tx: Sender<SaveMsg>,
    save_worker: Option<SaveWorker>,
    syntax: LanguageRegistry,
    editor_view: EditorViewState,
    last_detection: LanguageDetection,
    renaming_document: Option<DocumentId>,
    rename_text: String,
    rename_focus_pending: bool,
    editor_focus_pending: bool,
    native_menu: Option<NativeMenu>,
    search: SearchState,
}

impl PileApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let session_path = default_session_path();
        let state = match load_session(&session_path) {
            Ok(Some(snapshot)) => snapshot.state,
            Ok(None) => AppState::empty(),
            Err(err) => {
                warn!(error = %err, path = %session_path.display(), "failed to restore session");
                quarantine_corrupt_session(&session_path);
                AppState::empty()
            }
        };

        let save_worker = SaveWorker::spawn(session_path);
        let save_tx = save_worker.sender();
        let active_text = state
            .active_document()
            .map(|document| document.text())
            .unwrap_or_default();
        let syntax = LanguageRegistry;
        let last_detection = syntax.detect(&active_text);

        info!(documents = state.documents.len(), "pile started");

        Self {
            state,
            save_tx,
            save_worker: Some(save_worker),
            syntax,
            editor_view: EditorViewState::default(),
            last_detection,
            renaming_document: None,
            rename_text: String::new(),
            rename_focus_pending: false,
            editor_focus_pending: true,
            native_menu: NativeMenu::install(),
            search: SearchState::default(),
        }
    }

    fn mark_changed(&self) {
        let snapshot = SessionSnapshot::from(&self.state);
        let _ = self.save_tx.send(SaveMsg::Changed(snapshot));
    }

    fn flush_session(&self) {
        let (ack_tx, ack_rx) = bounded(1);
        let snapshot = SessionSnapshot::from(&self.state);
        let _ = self.save_tx.send(SaveMsg::Flush(snapshot, ack_tx));
        let _ = ack_rx.recv_timeout(Duration::from_secs(2));
    }

    fn active_document_text(&self) -> String {
        self.state
            .active_document()
            .map(|document| document.text())
            .unwrap_or_default()
    }

    fn refresh_active_document_metadata(&mut self) {
        let text = self.active_document_text();
        self.last_detection = self.syntax.detect(&text);
        self.recompute_search();
    }

    fn document_edited(&mut self) {
        self.refresh_active_document_metadata();
        self.mark_changed();
    }

    fn begin_rename(&mut self, document_id: DocumentId) {
        self.renaming_document = Some(document_id);
        self.rename_focus_pending = true;
        self.rename_text = self
            .state
            .documents
            .iter()
            .find(|document| document.id == document_id)
            .map(|document| {
                if document.has_manual_title() {
                    document.title_hint.clone()
                } else {
                    document.display_title()
                }
            })
            .unwrap_or_default();
    }

    fn commit_rename(&mut self) {
        let Some(document_id) = self.renaming_document.take() else {
            return;
        };

        if let Some(document) = self
            .state
            .documents
            .iter_mut()
            .find(|document| document.id == document_id)
        {
            let old_title = document.title_hint.clone();
            document.rename(&self.rename_text);
            if document.title_hint != old_title {
                self.mark_changed();
            }
        }

        self.rename_text.clear();
        self.rename_focus_pending = false;
    }

    fn render_tab(&mut self, ui: &mut egui::Ui, document_id: DocumentId) {
        let Some(document_index) = self
            .state
            .documents
            .iter()
            .position(|document| document.id == document_id)
        else {
            return;
        };

        if self.renaming_document == Some(document_id) {
            let response = ui.add_sized(
                [180.0, ui.spacing().interact_size.y],
                egui::TextEdit::singleline(&mut self.rename_text)
                    .font(egui::TextStyle::Button)
                    .desired_width(180.0),
            );
            if self.rename_focus_pending {
                response.request_focus();
                self.rename_focus_pending = false;
            }

            let pressed_enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
            if response.lost_focus() || pressed_enter {
                self.commit_rename();
            }

            return;
        }

        let selected = document_id == self.state.active_document;
        let title = self.state.documents[document_index].display_title();
        let response = ui
            .selectable_label(selected, title)
            .on_hover_text("Double-click to rename");

        if response.clicked() {
            self.state.set_active(document_id);
            self.mark_changed();
            self.refresh_active_document_metadata();
            self.editor_focus_pending = true;
        }

        if response.double_clicked() {
            self.begin_rename(document_id);
        }
    }

    fn new_scratch(&mut self) {
        self.commit_rename();
        self.state.open_untitled();
        self.mark_changed();
        self.refresh_active_document_metadata();
        self.editor_focus_pending = true;
    }

    fn handle_native_menu_commands(&mut self) {
        let mut commands = Vec::new();
        if let Some(native_menu) = &self.native_menu {
            while let Some(command) = native_menu.next_command() {
                commands.push(command);
            }
        }

        for command in commands {
            match command {
                NativeMenuCommand::NewScratch => self.new_scratch(),
                NativeMenuCommand::CloseScratch => self.close_active_scratch(),
                NativeMenuCommand::RenameTab => self.begin_rename(self.state.active_document),
                NativeMenuCommand::Undo => {
                    if let Some(document) = self.state.active_document_mut() {
                        document.undo();
                    }
                }
                NativeMenuCommand::Redo => {
                    if let Some(document) = self.state.active_document_mut() {
                        document.redo();
                    }
                }
            }
        }
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        let new_scratch = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND,
                logical_key: egui::Key::N,
            })
        });
        if new_scratch {
            self.new_scratch();
        }

        let close_scratch = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND,
                logical_key: egui::Key::W,
            })
        });
        if close_scratch {
            self.close_active_scratch();
        }

        let undo = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND,
                logical_key: egui::Key::Z,
            })
        });
        if undo {
            if let Some(document) = self.state.active_document_mut() {
                document.undo();
                self.document_edited();
            }
        }

        let redo = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                logical_key: egui::Key::Z,
            })
        });
        if redo {
            if let Some(document) = self.state.active_document_mut() {
                document.redo();
                self.document_edited();
            }
        }

        let open_replace = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND.plus(egui::Modifiers::ALT),
                logical_key: egui::Key::F,
            })
        });
        if open_replace {
            self.open_search();
            self.search.replace_visible = true;
        }

        let open_search = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND,
                logical_key: egui::Key::F,
            })
        });
        if open_search {
            self.open_search();
        }

        let rename_tab =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F2));
        if rename_tab {
            self.begin_rename(self.state.active_document);
        }

        let select_next = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND,
                logical_key: egui::Key::D,
            })
        });
        if select_next {
            self.select_next_occurrence();
        }

        let find_under_cursor = ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::F3)
        });
        if find_under_cursor {
            self.find_under_cursor();
        }
    }

    fn close_active_scratch(&mut self) {
        self.commit_rename();
        self.state.close_active();
        self.mark_changed();
        self.refresh_active_document_metadata();
        self.editor_focus_pending = true;
    }

    fn open_search(&mut self) {
        self.search.visible = true;
        self.search.focus_pending = true;
        self.recompute_search();
    }

    fn select_next_occurrence(&mut self) {
        let Some(document) = self.state.active_document() else {
            return;
        };
        let primary = crate::editor::primary_selection(document);
        let rope = document.rope.clone();
        self.search.select_next_occurrence(&rope, primary);
        self.document_edited();
    }

    fn find_under_cursor(&mut self) {
        let Some(document) = self.state.active_document() else {
            return;
        };
        let primary = crate::editor::primary_selection(document);
        let rope = document.rope.clone();
        self.search.find_under_cursor(&rope, primary);
        self.document_edited();
    }

    fn recompute_search(&mut self) {
        if let Some(document) = self.state.active_document() {
            let rope = document.rope.clone();
            self.search.recompute(&rope, &self.state.documents);
        }
    }

    fn close_search(&mut self) {
        self.search.visible = false;
        self.search.focus_pending = false;
        self.editor_focus_pending = true;
    }

    fn render_search_bar(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut go_next = false;
        let mut go_previous = false;
        let mut do_replace_one = false;
        let mut do_replace_all = false;

        ui.horizontal(|ui| {
            ui.label("Find");
            let hint = if self.search.search_all_tabs {
                "Search all scratches"
            } else {
                "Search current scratch"
            };
            let response = ui.add_sized(
                [240.0, ui.spacing().interact_size.y],
                egui::TextEdit::singleline(&mut self.search.query)
                    .hint_text(hint)
                    .desired_width(240.0),
            );

            if self.search.focus_pending {
                response.request_focus();
                self.search.focus_pending = false;
            }

            changed |= response.changed();

            let pressed_enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
            let pressed_escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
            let shift_down = ui.input(|input| input.modifiers.shift);
            if response.has_focus() && pressed_enter {
                if shift_down {
                    go_previous = true;
                } else {
                    go_next = true;
                }
            }
            if response.has_focus() && pressed_escape {
                self.close_search();
            }

            if ui.button("<").on_hover_text("Previous match").clicked() {
                go_previous = true;
            }
            if ui.button(">").on_hover_text("Next match").clicked() {
                go_next = true;
            }

            ui.label(self.search.current_label());

            changed |= ui
                .checkbox(&mut self.search.case_sensitive, "Aa")
                .on_hover_text("Case sensitive")
                .changed();
            changed |= ui
                .checkbox(&mut self.search.whole_word, "Word")
                .on_hover_text("Whole word")
                .changed();
            changed |= ui
                .checkbox(&mut self.search.use_regex, ".*")
                .on_hover_text("Regular expression")
                .changed();
            changed |= ui
                .checkbox(&mut self.search.search_all_tabs, "All")
                .on_hover_text("Search all tabs")
                .changed();

            let replace_label = if self.search.replace_visible {
                "v"
            } else {
                ">"
            };
            if ui
                .button(replace_label)
                .on_hover_text("Toggle replace")
                .clicked()
            {
                self.search.replace_visible = !self.search.replace_visible;
            }

            if ui.button("x").on_hover_text("Close search").clicked() {
                self.close_search();
            }
        });

        if self.search.replace_visible {
            ui.horizontal(|ui| {
                ui.label("Replace");
                let response = ui.add_sized(
                    [240.0, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut self.search.replacement)
                        .hint_text("Replacement text")
                        .desired_width(240.0),
                );

                let pressed_enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                let pressed_escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
                if response.has_focus() && pressed_enter {
                    do_replace_one = true;
                }
                if response.has_focus() && pressed_escape {
                    self.close_search();
                }

                let has_matches = !self.search.matches.is_empty();
                if ui
                    .add_enabled(has_matches, egui::Button::new("Replace"))
                    .on_hover_text("Replace current match")
                    .clicked()
                {
                    do_replace_one = true;
                }
                if ui
                    .add_enabled(has_matches, egui::Button::new("Replace all"))
                    .on_hover_text("Replace every match in this scratch")
                    .clicked()
                {
                    do_replace_all = true;
                }
            });
        }

        if changed {
            self.recompute_search();
        }
        if go_next {
            self.search.next_match();
        }
        if go_previous {
            self.search.previous_match();
        }
        if do_replace_one {
            self.replace_current_match();
        }
        if do_replace_all {
            self.replace_all_in_active_document();
        }
    }

    fn replace_current_match(&mut self) {
        if self.search.search_all_tabs {
            // For global search, only replace in active document
            let Some(index) = self.search.global_index else {
                return;
            };
            let Some(result) = self.search.global_results.get(index) else {
                return;
            };
            if result.document_id != self.state.active_document {
                // Switch to the document
                self.state.set_active(result.document_id);
                self.search.selection_pending = true;
                return;
            }
            let search_match = SearchMatch {
                start: result.match_start,
                end: result.match_end,
            };
            let replacement = self.search.replacement.clone();

            let regex = if self.search.use_regex {
                Regex::new(&self.search.query).ok()
            } else {
                None
            };

            let Some(document) = self.state.active_document_mut() else {
                return;
            };
            replace_match(document, search_match, &replacement, regex.as_ref());

            self.recompute_search();
            if !self.search.global_results.is_empty() {
                let next = if index < self.search.global_results.len() {
                    index
                } else {
                    0
                };
                self.search.global_index = Some(next);
                self.search.selection_pending = true;
            }
            self.document_edited();
        } else {
            let Some(index) = self.search.current_match else {
                return;
            };
            let Some(search_match) = self.search.matches.get(index).copied() else {
                return;
            };
            let replacement = self.search.replacement.clone();

            let regex = if self.search.use_regex {
                Regex::new(&self.search.query).ok()
            } else {
                None
            };

            let Some(document) = self.state.active_document_mut() else {
                return;
            };
            replace_match(document, search_match, &replacement, regex.as_ref());

            let rope = document.rope.clone();
            self.search.recompute(&rope, &self.state.documents);
            if !self.search.matches.is_empty() {
                let next = if index < self.search.matches.len() {
                    index
                } else {
                    0
                };
                self.search.current_match = Some(next);
                self.search.selection_pending = true;
            }
            self.document_edited();
        }
    }

    fn replace_all_in_active_document(&mut self) {
        if self.search.search_all_tabs {
            // Collect matches only from active document
            let active_id = self.state.active_document;
            let matches: Vec<SearchMatch> = self
                .search
                .global_results
                .iter()
                .filter(|r| r.document_id == active_id)
                .map(|r| SearchMatch {
                    start: r.match_start,
                    end: r.match_end,
                })
                .collect();
            let replacement = self.search.replacement.clone();

            let regex = if self.search.use_regex {
                Regex::new(&self.search.query).ok()
            } else {
                None
            };

            let Some(document) = self.state.active_document_mut() else {
                return;
            };
            let count = replace_all_matches(document, &matches, &replacement, regex.as_ref());
            if count == 0 {
                return;
            }

            self.recompute_search();
            self.document_edited();
        } else {
            if self.search.matches.is_empty() {
                return;
            }
            let matches = self.search.matches.clone();
            let replacement = self.search.replacement.clone();

            let regex = if self.search.use_regex {
                Regex::new(&self.search.query).ok()
            } else {
                None
            };

            let Some(document) = self.state.active_document_mut() else {
                return;
            };
            let count = replace_all_matches(document, &matches, &replacement, regex.as_ref());
            if count == 0 {
                return;
            }

            self.recompute_search();
            self.document_edited();
        }
    }

    fn render_editor(&mut self, ui: &mut egui::Ui) {
        let reveal_selection = if self.search.selection_pending {
            if self.search.search_all_tabs {
                if let Some(index) = self.search.global_index {
                    self.search.global_results.get(index).and_then(|result| {
                        // Switch to the correct tab
                        self.state.set_active(result.document_id);
                        Some(crate::model::Selection {
                            anchor: result.match_start,
                            head: result.match_end,
                        })
                    })
                } else {
                    None
                }
            } else {
                self.search
                    .current_match
                    .and_then(|index| self.search.matches.get(index))
                    .map(|search_match| crate::model::Selection {
                        anchor: search_match.start,
                        head: search_match.end,
                    })
            }
        } else {
            None
        };
        self.search.selection_pending = false;

        let search_highlights = if self.search.visible && !self.search.search_all_tabs {
            self.search
                .matches
                .iter()
                .enumerate()
                .map(|(index, search_match)| SearchHighlight {
                    start: search_match.start,
                    end: search_match.end,
                    is_current: Some(index) == self.search.current_match,
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let extra_selections: Vec<Selection> = self
            .search
            .occurrence_selections
            .iter()
            .filter(|s| {
                let (start, end) = if s.anchor <= s.head {
                    (s.anchor, s.head)
                } else {
                    (s.head, s.anchor)
                };
                start != end
            })
            .copied()
            .collect();

        let Some(document) = self.state.active_document_mut() else {
            return;
        };

        let response = show_editor(
            ui,
            document,
            &mut self.editor_view,
            &mut self.editor_focus_pending,
            reveal_selection,
            &search_highlights,
            &extra_selections,
        );

        if response.changed {
            self.document_edited();
        }
    }
}

impl eframe::App for PileApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_native_menu_commands();
        self.handle_keyboard_shortcuts(ctx);

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("+").on_hover_text("New scratch").clicked() {
                    self.new_scratch();
                }

                if ui.button("x").on_hover_text("Close scratch").clicked() {
                    self.close_active_scratch();
                }

                let tabs = self.state.tab_order.clone();
                for document_id in tabs {
                    self.render_tab(ui, document_id);
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            if self.search.visible {
                self.render_search_bar(ui);
                ui.separator();
            }

            ui.horizontal(|ui| {
                ui.label(format!(
                    "{:?} ({:.0}%)",
                    self.last_detection.language,
                    self.last_detection.confidence * 100.0
                ));
                ui.separator();
                let byte_len = self
                    .state
                    .active_document()
                    .map(|document| document.rope.byte_len())
                    .unwrap_or_default();
                ui.label(format!("{byte_len} bytes"));
            });
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0))
            .show(ctx, |ui| {
                self.render_editor(ui);
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_session();
        if let Some(worker) = self.save_worker.take() {
            worker.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use crop::Rope;
    use crate::model::Selection;

    #[test]
    fn select_next_occurrence_adds_first_word() {
        let rope = Rope::from("hello world hello");
        let primary = Selection {
            anchor: 0,
            head: 5,
        };
        let mut state = super::SearchState {
            visible: false,
            replace_visible: false,
            query: String::new(),
            replacement: String::new(),
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            search_all_tabs: false,
            matches: vec![],
            current_match: None,
            focus_pending: false,
            selection_pending: false,
            occurrence_selections: vec![],
            global_results: vec![],
            global_index: None,
        };

        state.select_next_occurrence(&rope, primary);

        assert_eq!(state.occurrence_selections.len(), 2);
        assert_eq!(state.occurrence_selections[0], primary);
        assert_eq!(state.occurrence_selections[1].anchor, 12);
        assert_eq!(state.occurrence_selections[1].head, 17);
    }

    #[test]
    fn find_under_cursor_selects_word() {
        let rope = Rope::from("hello world hello");
        let primary = Selection::caret(0);
        let mut state = super::SearchState {
            visible: false,
            replace_visible: false,
            query: String::new(),
            replacement: String::new(),
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            search_all_tabs: false,
            matches: vec![],
            current_match: None,
            focus_pending: false,
            selection_pending: false,
            occurrence_selections: vec![],
            global_results: vec![],
            global_index: None,
        };

        state.find_under_cursor(&rope, primary);

        assert_eq!(state.occurrence_selections.len(), 1);
        let sel = state.occurrence_selections[0];
        assert_eq!((sel.anchor, sel.head), (0, 5));
        assert_eq!(state.query, "hello");
    }

    #[test]
    fn find_under_cursor_clears_previous() {
        let rope = Rope::from("hello world");
        let primary = Selection::caret(6);
        let mut state = super::SearchState {
            visible: false,
            replace_visible: false,
            query: "previous".to_owned(),
            replacement: String::new(),
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            search_all_tabs: false,
            matches: vec![],
            current_match: None,
            focus_pending: false,
            selection_pending: false,
            occurrence_selections: vec![Selection { anchor: 0, head: 5 }],
            global_results: vec![],
            global_index: None,
        };

        state.find_under_cursor(&rope, primary);

        assert_eq!(state.occurrence_selections.len(), 1);
        let sel = state.occurrence_selections[0];
        assert_eq!((sel.anchor, sel.head), (6, 11));
        assert_eq!(state.query, "world");
    }
}
