use std::time::Duration;

use crossbeam_channel::{Sender, bounded};
use eframe::egui;
use tracing::{info, warn};

use crate::{
    editor::{EditorViewState, show_editor},
    model::{AppState, DocumentId, SessionSnapshot},
    native_menu::{NativeMenu, NativeMenuCommand},
    persistence::{
        SaveMsg, SaveWorker, default_session_path, load_session, quarantine_corrupt_session,
    },
    syntax::{LanguageDetection, LanguageRegistry},
};

#[derive(Clone, Debug, Default)]
struct SearchState {
    visible: bool,
    query: String,
    case_sensitive: bool,
    whole_word: bool,
    matches: Vec<SearchMatch>,
    current_match: Option<usize>,
    focus_pending: bool,
    selection_pending: bool,
}

impl SearchState {
    fn recompute(&mut self, text: &str) {
        let old_range = self
            .current_match
            .and_then(|index| self.matches.get(index).copied());
        let options = SearchOptions {
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
        };

        self.matches = find_matches(text, &self.query, options);
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

    fn next_match(&mut self) {
        self.current_match = advance_match(self.current_match, self.matches.len(), 1);
        self.selection_pending = true;
    }

    fn previous_match(&mut self) {
        self.current_match = advance_match(self.current_match, self.matches.len(), -1);
        self.selection_pending = true;
    }

    fn current_label(&self) -> String {
        match (self.current_match, self.matches.len()) {
            (_, 0) => "0 / 0".to_owned(),
            (Some(index), total) => format!("{} / {total}", index + 1),
            (None, total) => format!("0 / {total}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SearchOptions {
    case_sensitive: bool,
    whole_word: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SearchMatch {
    start: usize,
    end: usize,
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
        self.search.recompute(&text);
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
        let text = self.active_document_text();
        self.search.recompute(&text);
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

        ui.horizontal(|ui| {
            ui.label("Find");
            let response = ui.add_sized(
                [240.0, ui.spacing().interact_size.y],
                egui::TextEdit::singleline(&mut self.search.query)
                    .hint_text("Search current scratch")
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

            if ui.button("x").on_hover_text("Close search").clicked() {
                self.close_search();
            }
        });

        if changed {
            let text = self.active_document_text();
            self.search.recompute(&text);
        }
        if go_next {
            self.search.next_match();
        }
        if go_previous {
            self.search.previous_match();
        }
    }

    fn render_editor(&mut self, ui: &mut egui::Ui) {
        let reveal_selection = if self.search.selection_pending {
            self.search
                .current_match
                .and_then(|index| self.search.matches.get(index))
                .map(|search_match| crate::model::Selection {
                    anchor: search_match.start,
                    head: search_match.end,
                })
        } else {
            None
        };
        self.search.selection_pending = false;

        let Some(document) = self.state.active_document_mut() else {
            return;
        };

        let response = show_editor(
            ui,
            document,
            &mut self.editor_view,
            &mut self.editor_focus_pending,
            reveal_selection,
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

fn find_matches(text: &str, query: &str, options: SearchOptions) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let (haystack, needle) = if options.case_sensitive {
        (text.to_owned(), query.to_owned())
    } else {
        (text.to_ascii_lowercase(), query.to_ascii_lowercase())
    };

    let mut matches = Vec::new();
    let mut search_from = 0;
    while let Some(relative_start) = haystack[search_from..].find(&needle) {
        let start = search_from + relative_start;
        let end = start + needle.len();
        if !options.whole_word || is_whole_word_match(text, start, end) {
            matches.push(SearchMatch { start, end });
        }
        search_from = end;
    }

    matches
}

fn is_whole_word_match(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();

    !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
}

fn is_word_char(char: char) -> bool {
    char.is_alphanumeric() || char == '_'
}

fn advance_match(current: Option<usize>, total: usize, delta: isize) -> Option<usize> {
    if total == 0 {
        return None;
    }

    let Some(current) = current else {
        return Some(if delta < 0 { total - 1 } else { 0 });
    };

    let current = current as isize;
    let total = total as isize;
    Some((current + delta).rem_euclid(total) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_returns_non_overlapping_matches() {
        let matches = find_matches(
            "aaaa",
            "aa",
            SearchOptions {
                case_sensitive: true,
                whole_word: false,
            },
        );

        assert_eq!(
            matches,
            vec![
                SearchMatch { start: 0, end: 2 },
                SearchMatch { start: 2, end: 4 }
            ]
        );
    }

    #[test]
    fn search_handles_case_sensitivity() {
        assert_eq!(
            find_matches(
                "Hello hello",
                "hello",
                SearchOptions {
                    case_sensitive: true,
                    whole_word: false,
                },
            )
            .len(),
            1
        );
        assert_eq!(
            find_matches(
                "Hello hello",
                "hello",
                SearchOptions {
                    case_sensitive: false,
                    whole_word: false,
                },
            )
            .len(),
            2
        );
    }

    #[test]
    fn search_can_restrict_to_whole_words() {
        let matches = find_matches(
            "cat concatenate cat_ cat",
            "cat",
            SearchOptions {
                case_sensitive: true,
                whole_word: true,
            },
        );

        assert_eq!(
            matches,
            vec![
                SearchMatch { start: 0, end: 3 },
                SearchMatch { start: 21, end: 24 }
            ]
        );
    }

    #[test]
    fn search_navigation_wraps() {
        assert_eq!(advance_match(None, 3, 1), Some(0));
        assert_eq!(advance_match(None, 3, -1), Some(2));
        assert_eq!(advance_match(Some(2), 3, 1), Some(0));
        assert_eq!(advance_match(Some(0), 3, -1), Some(2));
        assert_eq!(advance_match(Some(0), 0, 1), None);
    }
}
