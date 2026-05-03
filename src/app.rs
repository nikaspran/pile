use std::time::Duration;

use crossbeam_channel::{Sender, bounded};
use eframe::egui;
use tracing::{info, warn};

use crate::{
    command::Command,
    command_palette::CommandPalette,
    editor::{EditorViewState, SearchHighlight, replace_all_matches, replace_match, show_editor},
    model::{AppState, DocumentId, Selection, SessionSnapshot},
    native_menu::{NativeMenu, NativeMenuCommand},
    persistence::{
        SaveMsg, SaveWorker, default_session_path, load_session, quarantine_corrupt_session,
    },
    search::{SearchMatch, SearchState},
    syntax::{LanguageDetection, LanguageRegistry},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppCommand {
    NewScratch,
    CloseScratch,
    RenameTab,
    Undo,
    Redo,
}

impl From<NativeMenuCommand> for AppCommand {
    fn from(command: NativeMenuCommand) -> Self {
        match command {
            NativeMenuCommand::NewScratch => Self::NewScratch,
            NativeMenuCommand::CloseScratch => Self::CloseScratch,
            NativeMenuCommand::RenameTab => Self::RenameTab,
            NativeMenuCommand::Undo => Self::Undo,
            NativeMenuCommand::Redo => Self::Redo,
        }
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
    command_palette: CommandPalette,
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
        let syntax = LanguageRegistry;
        let last_detection = state
            .active_document()
            .map(|document| syntax.detect_rope(&document.rope))
            .unwrap_or_else(|| syntax.detect(""));

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
            command_palette: CommandPalette::new(),
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

    fn refresh_active_document_detection(&mut self) {
        self.last_detection = self
            .state
            .active_document()
            .map(|document| self.syntax.detect_rope(&document.rope))
            .unwrap_or_else(|| self.syntax.detect(""));
    }

    fn refresh_active_document_metadata(&mut self) {
        self.refresh_active_document_detection();
        self.recompute_search();
    }

    fn document_edited(&mut self) {
        self.refresh_active_document_metadata();
        self.mark_changed();
    }

    fn set_active_document(&mut self, document_id: DocumentId) {
        if self.state.active_document == document_id {
            return;
        }
        if self.state.set_active(document_id) {
            self.refresh_active_document_metadata();
            self.mark_changed();
            self.editor_focus_pending = true;
        }
    }

    fn set_active_document_from_global_search(&mut self, document_id: DocumentId) {
        if self.state.active_document == document_id {
            return;
        }
        if self.state.set_active(document_id) {
            self.refresh_active_document_detection();
            self.mark_changed();
            self.editor_focus_pending = true;
        }
    }

    fn begin_rename(&mut self, document_id: DocumentId) {
        self.renaming_document = Some(document_id);
        self.rename_focus_pending = true;
        self.rename_text = self
            .state
            .document(document_id)
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

        if let Some(document) = self.state.document_mut(document_id) {
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
        let Some(document) = self.state.document(document_id) else {
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
        let title = document.display_title();
        let response = ui
            .selectable_label(selected, title)
            .on_hover_text("Double-click to rename");

        if response.clicked() {
            self.set_active_document(document_id);
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

    fn execute_command(&mut self, command: AppCommand) {
        match command {
            AppCommand::NewScratch => self.new_scratch(),
            AppCommand::CloseScratch => self.close_active_scratch(),
            AppCommand::RenameTab => self.begin_rename(self.state.active_document),
            AppCommand::Undo => {
                if let Some(document) = self.state.active_document_mut()
                    && document.can_undo()
                    && document.undo()
                {
                    self.document_edited();
                }
            }
            AppCommand::Redo => {
                if let Some(document) = self.state.active_document_mut()
                    && document.redo()
                {
                    self.document_edited();
                }
            }
        }
    }

    fn handle_command(&mut self, command: Command) {
        use Command::*;
        match command {
            NewScratch => self.execute_command(AppCommand::NewScratch),
            CloseScratch => self.execute_command(AppCommand::CloseScratch),
            RenameTab => self.execute_command(AppCommand::RenameTab),
            Undo => self.execute_command(AppCommand::Undo),
            Redo => self.execute_command(AppCommand::Redo),

            // Motion commands - these are handled by editor input
            MoveLeft | MoveRight | MoveWordLeft | MoveWordRight | MoveUp | MoveDown
            | MoveDocumentStart | MoveDocumentEnd | MoveLineStart | MoveLineEnd
            | MoveParagraphUp | MoveParagraphDown | PageUp | PageDown => {
                // Motion commands are handled by editor input
            }

            // Selection commands - handled by editor input
            SelectLeft | SelectRight | SelectWordLeft | SelectWordRight | SelectUp | SelectDown
            | SelectDocumentStart | SelectDocumentEnd | SelectLineStart | SelectLineEnd
            | SelectParagraphUp | SelectParagraphDown | SelectPageUp | SelectPageDown => {
                // Selection commands are handled by editor input
            }

            // Selection expansion
            ExpandWord | ContractWord | ExpandLine | ContractLine | ExpandBracketPair
            | ContractBracketPair | ExpandIndentBlock | ContractIndentBlock => {
                // These are handled by editor input
            }

            // Line operations
            Indent => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::indent_selection(document);
                    self.document_edited();
                }
            }
            Outdent => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::outdent_selection(document);
                    self.document_edited();
                }
            }
            DuplicateLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::duplicate_selected_lines(document);
                    self.document_edited();
                }
            }
            DeleteLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::delete_selected_lines(document);
                    self.document_edited();
                }
            }
            MoveLinesUp => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::move_selected_lines_up(document);
                    self.document_edited();
                }
            }
            MoveLinesDown => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::move_selected_lines_down(document);
                    self.document_edited();
                }
            }
            JoinLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::join_selected_lines(document);
                    self.document_edited();
                }
            }
            SortLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::sort_selected_lines(document);
                    self.document_edited();
                }
            }
            ReverseLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::reverse_selected_lines(document);
                    self.document_edited();
                }
            }
            TrimTrailingWhitespace => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::trim_trailing_whitespace(document);
                    self.document_edited();
                }
            }
            NormalizeWhitespace => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::normalize_whitespace(document);
                    self.document_edited();
                }
            }

            // Multi-cursor
            AddNextMatch => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::add_next_match(document);
                    self.document_edited();
                }
            }
            AddAllMatches => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::add_all_matches(document);
                    self.document_edited();
                }
            }
            SplitSelectionIntoLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::split_selection_into_lines(document);
                    self.document_edited();
                }
            }
            ClearSecondaryCursors => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::clear_secondary_cursors(document);
                    self.document_edited();
                }
            }

            // Editing
            ToggleComments => {
                if let Some(document) = self.state.active_document_mut() {
                    let comment_prefix = document
                        .detect_syntax()
                        .and_then(|d| d.language.comment_prefix())
                        .unwrap_or("//");
                    crate::editor::toggle_comments(document, comment_prefix);
                    self.document_edited();
                }
            }
            UpperCase => {
                if let Some(document) = self.state.active_document_mut() {
                    if document.selections.len() > 1 {
                        crate::editor::convert_case_all_selections(document, crate::editor::CaseType::Upper);
                    } else {
                        crate::editor::convert_case_selection(document, crate::editor::CaseType::Upper);
                    }
                    self.document_edited();
                }
            }
            LowerCase => {
                if let Some(document) = self.state.active_document_mut() {
                    if document.selections.len() > 1 {
                        crate::editor::convert_case_all_selections(document, crate::editor::CaseType::Lower);
                    } else {
                        crate::editor::convert_case_selection(document, crate::editor::CaseType::Lower);
                    }
                    self.document_edited();
                }
            }
            TitleCase => {
                if let Some(document) = self.state.active_document_mut() {
                    if document.selections.len() > 1 {
                        crate::editor::convert_case_all_selections(document, crate::editor::CaseType::Title);
                    } else {
                        crate::editor::convert_case_selection(document, crate::editor::CaseType::Title);
                    }
                    self.document_edited();
                }
            }

            // Search
            Find => self.open_search(),
            FindReplace => {
                self.open_search();
                self.search.replace_visible = true;
            }
            FindUnderCursor => self.find_under_cursor(),
            SelectNextOccurrence => self.select_next_occurrence(),
            SearchInTabs => {
                self.search.search_all_tabs = true;
                self.open_search();
            }

            // View
            CommandPalette => self.command_palette.toggle(),
        }
    }

    fn handle_native_menu_commands(&mut self) {
        let mut commands = Vec::new();
        if let Some(native_menu) = &self.native_menu {
            while let Some(command) = native_menu.next_command() {
                commands.push(command);
            }
        }

        for command in commands {
            self.execute_command(AppCommand::from(command));
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
            self.execute_command(AppCommand::NewScratch);
        }

        let close_scratch = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND,
                logical_key: egui::Key::W,
            })
        });
        if close_scratch {
            self.execute_command(AppCommand::CloseScratch);
        }

        let undo = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND,
                logical_key: egui::Key::Z,
            })
        });
        if undo {
            self.execute_command(AppCommand::Undo);
        }

        let redo = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                logical_key: egui::Key::Z,
            })
        });
        if redo {
            self.execute_command(AppCommand::Redo);
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
            self.execute_command(AppCommand::RenameTab);
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

        let find_under_cursor =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F3));
        if find_under_cursor {
            self.find_under_cursor();
        }

        let toggle_palette = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                logical_key: egui::Key::P,
            })
        });
        if toggle_palette {
            self.command_palette.toggle();
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
            let pressed_up = ui.input(|input| input.key_pressed(egui::Key::ArrowUp));
            let pressed_down = ui.input(|input| input.key_pressed(egui::Key::ArrowDown));

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

            if self.search.preview_visible && !self.search.preview_items.is_empty() {
                if pressed_up {
                    let len = self.search.preview_items.len();
                    if len > 0 {
                        let new_index = self
                            .search
                            .preview_index
                            .map(|i| (i + len - 1) % len)
                            .unwrap_or(0);
                        self.jump_to_preview_item(new_index);
                    }
                }
                if pressed_down {
                    let len = self.search.preview_items.len();
                    if len > 0 {
                        let new_index = self
                            .search
                            .preview_index
                            .map(|i| (i + 1) % len)
                            .unwrap_or(0);
                        self.jump_to_preview_item(new_index);
                    }
                }
            }

            if ui.button("<").on_hover_text("Previous match").clicked() {
                go_previous = true;
            }
            if ui.button(">").on_hover_text("Next match").clicked() {
                go_next = true;
            }

            ui.label(self.search.current_label());
            if self.search.search_all_tabs
                && let Some(title) = self.search.current_result_title()
            {
                ui.label(title);
            }

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

            let preview_label = if self.search.preview_visible {
                "▾"
            } else {
                "▸"
            };
            if ui
                .add_enabled(
                    !self.search.query.is_empty() && self.search.has_matches(),
                    egui::Button::new(preview_label),
                )
                .on_hover_text("Toggle result previews")
                .clicked()
            {
                self.search.preview_visible = !self.search.preview_visible;
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

                let has_matches = self.search.has_matches();
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

    fn render_search_preview(&mut self, ui: &mut egui::Ui) {
        let preview_items: Vec<_> = self
            .search
            .preview_items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                (
                    i,
                    item.line_number,
                    item.document_title.clone(),
                    item.context_before.clone(),
                    item.matched_text.clone(),
                    item.context_after.clone(),
                )
            })
            .collect();
        let preview_index = self.search.preview_index;
        let search_all_tabs = self.search.search_all_tabs;

        let mut clicked_index: Option<usize> = None;

        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for (i, line_number, doc_title, before, matched, after) in &preview_items {
                    let is_current = preview_index == Some(*i);

                    let bg = if is_current {
                        ui.style().visuals.selection.bg_fill
                    } else {
                        ui.style().visuals.widgets.inactive.bg_fill
                    };

                    let response = ui.horizontal(|ui| {
                        ui.painter().rect_filled(
                            ui.max_rect(),
                            0.0,
                            bg,
                        );
                        ui.add_sized(
                            [16.0, ui.spacing().interact_size.y],
                            egui::Label::new(
                                egui::RichText::new(format!("{:>4}", i + 1))
                                    .monospace()
                                    .weak(),
                            ),
                        );

                        ui.add_sized(
                            [60.0, ui.spacing().interact_size.y],
                            egui::Label::new(
                                egui::RichText::new(format!("L{}", line_number + 1))
                                    .monospace()
                                    .weak(),
                            ),
                        );

                        if search_all_tabs {
                            let title = doc_title.as_deref().unwrap_or("Unknown");
                            ui.add_sized(
                                [120.0, ui.spacing().interact_size.y],
                                egui::Label::new(
                                    egui::RichText::new(title)
                                        .monospace()
                                        .weak(),
                                ),
                            );
                        }

                        ui.label(
                            egui::RichText::new(before)
                                .monospace()
                                .weak(),
                        );
                        ui.label(
                            egui::RichText::new(matched)
                                .monospace()
                                .strong()
                                .background_color(ui.style().visuals.selection.bg_fill),
                        );
                        ui.label(
                            egui::RichText::new(after)
                                .monospace()
                                .weak(),
                        );
                    });

                    if response.response.clicked() {
                        clicked_index = Some(*i);
                    }
                }
            });

        if let Some(index) = clicked_index {
            self.jump_to_preview_item(index);
        }
    }

    fn jump_to_preview_item(&mut self, index: usize) {
        if index >= self.search.preview_items.len() {
            return;
        }
        let item = &self.search.preview_items[index];
        if let Some(doc_id) = item.document_id {
            self.set_active_document_from_global_search(doc_id);
        }
        if self.search.search_all_tabs {
            self.search.global_index = Some(index);
        } else {
            self.search.current_match = Some(index);
        }
        self.search.preview_index = Some(index);
        self.search.selection_pending = true;
    }

    fn replace_current_match(&mut self) {
        if self.search.search_all_tabs {
            // For global search, only replace in active document
            let Some(index) = self.search.global_index else {
                return;
            };
            let Some(result) = self.search.current_global_result() else {
                return;
            };
            if result.document_id != self.state.active_document {
                // Switch to the document
                self.set_active_document_from_global_search(result.document_id);
                self.search.selection_pending = true;
                return;
            }
            let search_match = SearchMatch {
                start: result.match_start,
                end: result.match_end,
            };
            let replacement = self.search.replacement.clone();
            let regex = self.search.replacement_regex();

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
            let Some(search_match) = self.search.current_match() else {
                return;
            };
            let replacement = self.search.replacement.clone();
            let regex = self.search.replacement_regex();

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
            let matches = self.search.matches_in_document(active_id);
            let replacement = self.search.replacement.clone();
            let regex = self.search.replacement_regex();

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
            let regex = self.search.replacement_regex();

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
                self.search.current_global_result().cloned().map(|result| {
                    self.set_active_document_from_global_search(result.document_id);
                    crate::model::Selection {
                        anchor: result.match_start,
                        head: result.match_end,
                    }
                })
            } else {
                self.search
                    .current_match()
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
                    self.execute_command(AppCommand::NewScratch);
                }

                if ui.button("x").on_hover_text("Close scratch").clicked() {
                    self.execute_command(AppCommand::CloseScratch);
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
                if self.search.preview_visible && !self.search.preview_items.is_empty() {
                    ui.separator();
                    self.render_search_preview(ui);
                }
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

        let mut cmd = None;
        self.command_palette.show(ctx, &mut |c| cmd = Some(c));
        if let Some(command) = cmd {
            self.handle_command(command);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_session();
        if let Some(worker) = self.save_worker.take() {
            worker.shutdown();
        }
    }
}
