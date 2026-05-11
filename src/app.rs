use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};
use eframe::egui;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use tracing::{info, warn};

use crate::{
    command::Command,
    command_palette::CommandPalette,
    editor::{
        EditorViewState, SearchHighlight,
        minimap::{self, MinimapConfig},
        replace_all_matches, replace_match, show_editor,
    },
    grammar_registry::GrammarRegistry,
    model::{AppState, Document, DocumentId, PaneSnapshot, Selection, SessionSnapshot},
    native_menu::NativeMenu,
    parse_worker::{ParseEvent, ParseWorker},
    persistence::{
        RecoveryEvent, RecoveryEventKind, SaveMsg, SaveTelemetry, SaveWorker, WorkerEvent,
        default_session_path, default_settings_path, load_session, load_settings,
        quarantine_corrupt_session, save_settings,
    },
    preferences::PreferencesState,
    search::{SearchMatch, SearchState},
    settings::Settings,
    syntax::LanguageDetection,
    tab_switcher::TabSwitcher,
    theme::apply_theme,
};

mod commands;
use commands::AppCommand;

struct GotoLineState {
    visible: bool,
    query: String,
    focus_pending: bool,
}

impl GotoLineState {
    fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            focus_pending: false,
        }
    }
}

#[derive(Clone, Debug)]
struct EditorPane {
    document_id: DocumentId,
    view_state: EditorViewState,
}

impl EditorPane {
    fn new(document_id: DocumentId) -> Self {
        Self {
            document_id,
            view_state: EditorViewState::default(),
        }
    }
}

pub struct PileApp {
    ctx: egui::Context,
    state: AppState,
    settings: Settings,
    save_tx: Sender<SaveMsg>,
    save_worker: Option<SaveWorker>,
    syntax: GrammarRegistry,
    last_detection: LanguageDetection,
    renaming_document: Option<DocumentId>,
    rename_text: String,
    rename_focus_pending: bool,
    editor_focus_pending: bool,
    clipboard_text: Option<String>,
    native_menu: Option<NativeMenu>,
    search: SearchState,
    command_palette: CommandPalette,
    tab_switcher: TabSwitcher,
    preferences: PreferencesState,
    panes: Vec<EditorPane>,
    active_pane: usize,
    goto_line: GotoLineState,
    pending_delete_confirmation: Option<DocumentId>,
    telemetry: SaveTelemetry,
    recovery_events: Vec<RecoveryEvent>,
    worker_event_rx: Option<Receiver<WorkerEvent>>,
    shutdown_flag: Arc<AtomicBool>,
    parse_worker: Option<ParseWorker>,
    frame_time_ms: f64,
}

impl PileApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();

        // Set up system shutdown signal handling
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        #[cfg(unix)]
        {
            let _ = flag::register(SIGTERM, Arc::clone(&shutdown_flag));
            let _ = flag::register(SIGINT, Arc::clone(&shutdown_flag));
        }

        let session_path = default_session_path();

        // Load settings first so we can use them during state initialization
        let settings_path = default_settings_path();
        let settings = load_settings(&settings_path);

        let mut telemetry = SaveTelemetry::default();
        let (mut state, saved_panes) = match load_session(&session_path, &mut telemetry) {
            Ok(Some(mut snapshot)) => {
                snapshot.state.validate();
                let panes = if snapshot.schema_version >= 2 {
                    Some((snapshot.panes, snapshot.active_pane))
                } else {
                    None
                };
                (snapshot.state, panes)
            }
            Ok(None) => (AppState::empty(), None),
            Err(err) => {
                warn!(error = %err, path = %session_path.display(), "failed to restore session");
                quarantine_corrupt_session(&session_path, &mut telemetry);
                (AppState::empty(), None)
            }
        };

        let (event_tx, event_rx) = bounded(128);
        let save_worker = SaveWorker::spawn_with_events(session_path, event_tx);
        let save_tx = save_worker.sender();
        let syntax = GrammarRegistry::default();
        let last_detection = state
            .active_document()
            .map(|document| syntax.detect_rope(&document.rope))
            .unwrap_or_else(|| syntax.detect(""));

        let (panes, active_pane) = if let Some((saved_panes, saved_active_pane)) = saved_panes {
            let valid_panes: Vec<_> = saved_panes
                .into_iter()
                .filter_map(|pane_snap| {
                    if state.document(pane_snap.document_id).is_some() {
                        Some(pane_snap)
                    } else {
                        None
                    }
                })
                .collect();

            if valid_panes.is_empty() {
                // All saved panes were invalid, create a new document
                state.open_untitled(settings.default_tab_width, settings.default_soft_tabs);
                (vec![EditorPane::new(state.active_document)], 0)
            } else {
                let panes: Vec<EditorPane> = valid_panes
                    .into_iter()
                    .map(|pane_snap| EditorPane {
                        document_id: pane_snap.document_id,
                        view_state: EditorViewState {
                            preferred_column: pane_snap.preferred_column,
                            visible_rows: pane_snap.visible_rows,
                            last_click_time: None,
                            click_count: 0,
                            column_selection: pane_snap.column_selection,
                            column_selection_anchor_col: pane_snap.column_selection_anchor_col,
                            scroll_animation: None,
                            cached_layout: None,
                        },
                    })
                    .collect();
                let active_pane = saved_active_pane.min(panes.len() - 1);
                (panes, active_pane)
            }
        } else {
            // No saved panes - ensure we have at least one document
            if state.documents.is_empty() {
                state.open_untitled(settings.default_tab_width, settings.default_soft_tabs);
            }
            (vec![EditorPane::new(state.active_document)], 0)
        };

        // Apply the loaded theme and font settings
        apply_theme(&ctx, settings.theme);
        crate::settings::apply_font_settings(
            &ctx,
            &settings.font_family,
            settings.font_size,
            settings.line_height_scale,
        );

        info!(
            documents = state.documents.len(),
            panes = panes.len(),
            "pile started"
        );

        // Spawn the background parse worker
        let parse_worker = ParseWorker::spawn();

        Self {
            ctx,
            state,
            settings,
            save_tx,
            save_worker: Some(save_worker),
            syntax,
            last_detection,
            renaming_document: None,
            rename_text: String::new(),
            rename_focus_pending: false,
            editor_focus_pending: true,
            clipboard_text: None,
            native_menu: NativeMenu::install(),
            search: SearchState::default(),
            command_palette: CommandPalette::new(),
            tab_switcher: TabSwitcher::new(),
            preferences: PreferencesState::new(),
            panes,
            active_pane,
            goto_line: GotoLineState::new(),
            pending_delete_confirmation: None,
            telemetry,
            recovery_events: Vec::new(),
            worker_event_rx: Some(event_rx),
            shutdown_flag: shutdown_flag.clone(),
            parse_worker: Some(parse_worker),
            frame_time_ms: 0.0,
        }
    }

    fn extract_selected_text(document: &crate::model::Document) -> String {
        use crate::editor::selection_range;
        let mut parts = Vec::new();
        for selection in &document.selections {
            let (start, end) = selection_range(*selection);
            if start < end {
                parts.push(document.rope.byte_slice(start..end).to_string());
            }
        }
        parts.join("\n")
    }

    fn handle_clipboard_events(&mut self, ctx: &egui::Context) {
        let (cut, copy) = ctx.input(|input| {
            let cut = input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Cut));
            let copy = input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Copy));
            (cut, copy)
        });

        if cut {
            self.execute_command(AppCommand::Cut);
        } else if copy {
            self.execute_command(AppCommand::Copy);
        }
    }

    fn mark_changed(&self) {
        let snapshot = create_snapshot(&self.state, &self.panes, self.active_pane);
        let _ = self.save_tx.send(SaveMsg::Changed(snapshot));
    }

    fn flush_session(&mut self) {
        let (ack_tx, ack_rx) = bounded(1);
        let snapshot = create_snapshot(&self.state, &self.panes, self.active_pane);
        let _ = self.save_tx.send(SaveMsg::Flush(snapshot, ack_tx));
        if let Ok(Err(err)) = ack_rx.recv_timeout(Duration::from_secs(2)) {
            self.recovery_events.push(RecoveryEvent {
                timestamp: std::time::SystemTime::now(),
                kind: RecoveryEventKind::SaveFailed,
                message: format!("Flush save failed: {}", err),
            });
        }
    }

    fn refresh_active_document_detection(&mut self) {
        let mut detection = self
            .state
            .active_document()
            .map(|document| self.syntax.detect_rope(&document.rope))
            .unwrap_or_else(|| self.syntax.detect(""));

        // Override detection if language is in ignored list
        let lang_name = match detection.language {
            crate::syntax::LanguageId::PlainText => "PlainText",
            crate::syntax::LanguageId::Markdown => "Markdown",
            crate::syntax::LanguageId::Rust => "Rust",
            crate::syntax::LanguageId::JavaScript => "JavaScript",
            crate::syntax::LanguageId::TypeScript => "TypeScript",
            crate::syntax::LanguageId::Python => "Python",
            crate::syntax::LanguageId::Json => "Json",
            crate::syntax::LanguageId::Toml => "Toml",
            crate::syntax::LanguageId::Yaml => "Yaml",
            crate::syntax::LanguageId::Bash => "Bash",
        };
        if self
            .settings
            .ignored_languages
            .contains(&lang_name.to_string())
        {
            detection.language = crate::syntax::LanguageId::PlainText;
            detection.confidence = 0.0;
        }

        self.last_detection = detection;
    }

    fn refresh_active_document_metadata(&mut self) {
        self.refresh_active_document_detection();
        self.recompute_search();
    }

    fn document_edited(&mut self) {
        self.refresh_active_document_metadata();
        self.mark_changed();
        self.request_background_parse_for_active();
    }

    /// Request a background parse for the active document if needed.
    fn request_background_parse_for_active(&mut self) {
        let Some(worker) = self.parse_worker.as_ref() else {
            return;
        };

        let Some(document) = self.state.active_document() else {
            return;
        };

        let Some(detection) = document.detect_syntax() else {
            return;
        };

        if detection.language == crate::syntax::LanguageId::PlainText {
            return;
        }

        // Check if we need to parse
        if !document
            .syntax_state
            .needs_parse(detection.language, document.revision)
        {
            return;
        }

        // Get visible range from the active pane
        let (visible_start, visible_end) = self
            .panes
            .get(self.active_pane)
            .map(|_pane| {
                // Use a reasonable default visible range if not available
                (0, document.rope.byte_len().min(4096))
            })
            .unwrap_or((0, 0));

        let request = crate::parse_worker::ParseRequest {
            document_id: document.id,
            revision: document.revision,
            language: detection.language,
            text: document
                .rope
                .byte_slice(visible_start..visible_end)
                .to_string(),
            visible_start,
            visible_end,
        };

        worker.request_parse(request);
    }

    /// Handle parse events from the background worker.
    fn handle_parse_events(&mut self) {
        let Some(worker) = self.parse_worker.as_ref() else {
            return;
        };

        while let Some(event) = worker.try_recv() {
            match event {
                ParseEvent::Result(result) => {
                    // Update the document's syntax state
                    if let Some(document) = self.state.document_mut(result.document_id) {
                        // Only update if this result is still relevant
                        if should_accept_parse_result(document, &result) {
                            document.syntax_state.update_from_parse_result(
                                result.tree,
                                result.spans,
                                result.language,
                                result.revision,
                                result.visible_start,
                                result.visible_end,
                            );
                        }
                    }
                }
            }
        }
    }

    fn set_active_document(&mut self, document_id: DocumentId) {
        if self.state.active_document == document_id {
            return;
        }
        if self.state.set_active(document_id) {
            // Update the active pane to point to the new document
            if let Some(pane) = self.panes.get_mut(self.active_pane) {
                pane.document_id = document_id;
            }
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
            // Update the active pane to point to the new document
            if let Some(pane) = self.panes.get_mut(self.active_pane) {
                pane.document_id = document_id;
            }
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
        let pinnd = document.pinned;

        ui.horizontal(|ui| {
            // Pin button (if pinned)
            if pinnd {
                if ui.small_button("📌").on_hover_text("Unpin tab").clicked() {
                    self.toggle_pin_tab(document_id);
                }
            }

            let response = ui
                .selectable_label(selected, title)
                .on_hover_text("Double-click to rename");

            if response.clicked() {
                self.set_active_document(document_id);
            }

            if response.double_clicked() {
                self.begin_rename(document_id);
            }

            // Close button (not shown for pinned tabs)
            if !pinnd && ui.small_button("×").on_hover_text("Close tab").clicked() {
                self.close_document(document_id);
            }
        });
    }

    fn new_scratch(&mut self) {
        self.commit_rename();
        self.state.open_untitled(
            self.settings.default_tab_width,
            self.settings.default_soft_tabs,
        );
        // Update the active pane to point to the new document
        if let Some(pane) = self.panes.get_mut(self.active_pane) {
            pane.document_id = self.state.active_document;
        }
        self.mark_changed();
        self.refresh_active_document_metadata();
        self.editor_focus_pending = true;
    }

    fn toggle_pin_tab(&mut self, document_id: DocumentId) {
        if let Some(document) = self.state.document_mut(document_id) {
            document.pinned = !document.pinned;
            self.mark_changed();
        }
    }

    fn close_document(&mut self, document_id: DocumentId) {
        // Don't close pinned tabs
        if let Some(document) = self.state.document(document_id) {
            if document.pinned {
                return;
            }
        }

        if self.state.active_document == document_id {
            self.close_active_scratch();
        } else {
            self.state.close_document_by_id(document_id);
            // Update any panes that were pointing to the closed document
            for pane in &mut self.panes {
                if pane.document_id == document_id {
                    pane.document_id = self.state.active_document;
                }
            }
            self.mark_changed();
        }
    }

    fn move_tab_left(&mut self, document_id: DocumentId) {
        if let Some(pos) = self
            .state
            .tab_order
            .iter()
            .position(|id| *id == document_id)
        {
            if pos > 0 {
                self.state.tab_order.swap(pos, pos - 1);
                self.mark_changed();
            }
        }
    }

    fn move_tab_right(&mut self, document_id: DocumentId) {
        if let Some(pos) = self
            .state
            .tab_order
            .iter()
            .position(|id| *id == document_id)
        {
            if pos < self.state.tab_order.len() - 1 {
                self.state.tab_order.swap(pos, pos + 1);
                self.mark_changed();
            }
        }
    }

    fn split_pane_horizontal(&mut self) {
        let active_doc = self.state.active_document;
        self.panes.push(EditorPane::new(active_doc));
        self.active_pane = self.panes.len() - 1;
        self.mark_changed();
    }

    fn split_pane_vertical(&mut self) {
        // For now, vertical split is the same as horizontal (UI limitation)
        self.split_pane_horizontal();
    }

    fn close_pane(&mut self) {
        if self.panes.len() <= 1 {
            return;
        }
        if self.active_pane < self.panes.len() {
            self.panes.remove(self.active_pane);
            if self.active_pane >= self.panes.len() {
                self.active_pane = self.panes.len() - 1;
            }
            self.mark_changed();
        }
    }

    fn execute_command(&mut self, command: AppCommand) {
        match command {
            AppCommand::NewScratch => self.new_scratch(),
            AppCommand::CloseScratch => self.close_active_scratch(),
            AppCommand::RenameTab => self.begin_rename(self.state.active_document),
            AppCommand::Quit => self.ctx.send_viewport_cmd(egui::ViewportCommand::Close),
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
            AppCommand::Cut => {
                if let Some(document) = self.state.active_document_mut() {
                    let text = Self::extract_selected_text(document);
                    if !text.is_empty() {
                        self.clipboard_text = Some(text);
                        crate::editor::delete_all(document);
                        self.document_edited();
                    }
                }
                self.editor_focus_pending = true;
            }
            AppCommand::Copy => {
                if let Some(document) = self.state.active_document() {
                    let text = Self::extract_selected_text(document);
                    if !text.is_empty() {
                        self.clipboard_text = Some(text);
                    }
                }
                self.editor_focus_pending = true;
            }
            AppCommand::Paste => {
                self.ctx
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                self.editor_focus_pending = true;
            }
            AppCommand::SelectAll => {
                if let Some(document) = self.state.active_document_mut() {
                    let len = document.rope.byte_len();
                    document.selections = vec![crate::model::Selection::caret(len)];
                    // For select all, we need to select from 0 to len
                    if let Some(sel) = document.selections.last_mut() {
                        sel.anchor = 0;
                    }
                    self.editor_focus_pending = true;
                }
            }
            AppCommand::ToggleComments => {
                if let Some(document) = self.state.active_document_mut() {
                    let comment_prefix = document
                        .detect_syntax()
                        .and_then(|d| d.language.comment_prefix())
                        .unwrap_or("//");
                    crate::editor::toggle_comments(document, comment_prefix);
                    self.document_edited();
                }
            }
            AppCommand::UpperCase => {
                if let Some(document) = self.state.active_document_mut() {
                    if document.selections.len() > 1 {
                        crate::editor::convert_case_all_selections(
                            document,
                            crate::editor::CaseType::Upper,
                        );
                    } else {
                        crate::editor::convert_case_selection(
                            document,
                            crate::editor::CaseType::Upper,
                        );
                    }
                    self.document_edited();
                }
            }
            AppCommand::LowerCase => {
                if let Some(document) = self.state.active_document_mut() {
                    if document.selections.len() > 1 {
                        crate::editor::convert_case_all_selections(
                            document,
                            crate::editor::CaseType::Lower,
                        );
                    } else {
                        crate::editor::convert_case_selection(
                            document,
                            crate::editor::CaseType::Lower,
                        );
                    }
                    self.document_edited();
                }
            }
            AppCommand::TitleCase => {
                if let Some(document) = self.state.active_document_mut() {
                    if document.selections.len() > 1 {
                        crate::editor::convert_case_all_selections(
                            document,
                            crate::editor::CaseType::Title,
                        );
                    } else {
                        crate::editor::convert_case_selection(
                            document,
                            crate::editor::CaseType::Title,
                        );
                    }
                    self.document_edited();
                }
            }
            AppCommand::ExpandWord => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::expand_selection_by_word(document);
                    self.editor_focus_pending = true;
                }
            }
            AppCommand::ContractWord => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::contract_selection_by_word(document);
                    self.editor_focus_pending = true;
                }
            }
            AppCommand::ExpandLine => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::expand_selection_by_line(document);
                    self.editor_focus_pending = true;
                }
            }
            AppCommand::ContractLine => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::contract_selection_by_line(document);
                    self.editor_focus_pending = true;
                }
            }
            AppCommand::ExpandBracketPair => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::expand_selection_by_bracket_pair(document);
                    self.editor_focus_pending = true;
                }
            }
            AppCommand::ContractBracketPair => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::contract_selection_by_bracket_pair(document);
                    self.editor_focus_pending = true;
                }
            }
            AppCommand::ExpandIndentBlock => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::expand_selection_by_indent_block(document);
                    self.editor_focus_pending = true;
                }
            }
            AppCommand::ContractIndentBlock => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::contract_selection_by_indent_block(document);
                    self.editor_focus_pending = true;
                }
            }
            AppCommand::Indent => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::indent_selection(document);
                    self.document_edited();
                }
            }
            AppCommand::Outdent => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::outdent_selection(document);
                    self.document_edited();
                }
            }
            AppCommand::DuplicateLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::duplicate_selected_lines(document);
                    self.document_edited();
                }
            }
            AppCommand::DeleteLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::delete_selected_lines(document);
                    self.document_edited();
                }
            }
            AppCommand::MoveLinesUp => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::move_selected_lines_up(document);
                    self.document_edited();
                }
            }
            AppCommand::MoveLinesDown => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::move_selected_lines_down(document);
                    self.document_edited();
                }
            }
            AppCommand::JoinLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::join_selected_lines(document);
                    self.document_edited();
                }
            }
            AppCommand::SortLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::sort_selected_lines(document);
                    self.document_edited();
                }
            }
            AppCommand::ReverseLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::reverse_selected_lines(document);
                    self.document_edited();
                }
            }
            AppCommand::TrimTrailingWhitespace => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::trim_trailing_whitespace(document);
                    self.document_edited();
                }
            }
            AppCommand::AddNextMatch => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::add_next_match(document);
                    self.document_edited();
                }
            }
            AppCommand::AddAllMatches => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::add_all_matches(document);
                    self.document_edited();
                }
            }
            AppCommand::SplitSelectionIntoLines => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::split_selection_into_lines(document);
                    self.document_edited();
                }
            }
            AppCommand::ClearSecondaryCursors => {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::clear_secondary_cursors(document);
                    self.document_edited();
                }
            }
            AppCommand::Find => self.handle_command(crate::command::Command::Find),
            AppCommand::FindReplace => self.handle_command(crate::command::Command::FindReplace),
            AppCommand::FindUnderCursor => {
                self.handle_command(crate::command::Command::FindUnderCursor)
            }
            AppCommand::SearchInTabs => self.handle_command(crate::command::Command::SearchInTabs),
            AppCommand::CommandPalette => {
                self.handle_command(crate::command::Command::CommandPalette)
            }
            AppCommand::ToggleWrapMode => {
                self.handle_command(crate::command::Command::ToggleWrapMode)
            }
            AppCommand::ToggleVisibleWhitespace => {
                self.handle_command(crate::command::Command::ToggleVisibleWhitespace)
            }
            AppCommand::ToggleIndentGuides => {
                self.handle_command(crate::command::Command::ToggleIndentGuides)
            }
            AppCommand::ToggleMinimap => {
                self.handle_command(crate::command::Command::ToggleMinimap)
            }
            AppCommand::ToggleStatusBar => {
                self.handle_command(crate::command::Command::ToggleStatusBar)
            }
            AppCommand::ToggleTheme => self.handle_command(crate::command::Command::ToggleTheme),
            AppCommand::GoToLine => {
                self.goto_line.visible = true;
                self.goto_line.focus_pending = true;
            }
            AppCommand::ToggleBookmark => self.toggle_bookmark(),
            AppCommand::JumpToNextBookmark => self.jump_to_next_bookmark(),
            AppCommand::ClearBookmarks => self.clear_bookmarks(),
            AppCommand::SplitPaneHorizontal => self.split_pane_horizontal(),
            AppCommand::SplitPaneVertical => self.split_pane_vertical(),
            AppCommand::ClosePane => self.close_pane(),
            AppCommand::PinTab => {
                let active = self.state.active_document;
                self.toggle_pin_tab(active);
            }
            AppCommand::ImportFile => self.import_file(),
            AppCommand::ExportFile => self.export_file(),
            AppCommand::Preferences => self.preferences.toggle(),
            AppCommand::MoveTabLeft => {
                let active = self.state.active_document;
                self.move_tab_left(active);
            }
            AppCommand::MoveTabRight => {
                let active = self.state.active_document;
                self.move_tab_right(active);
            }
            AppCommand::ReopenLastClosed => {
                if let Some(closed) = self.state.last_closed_document() {
                    let id = closed.document.id;
                    self.state.reopen_document(id);
                    if let Some(pane) = self.panes.get_mut(self.active_pane) {
                        pane.document_id = id;
                    }
                    self.mark_changed();
                    self.refresh_active_document_metadata();
                    self.editor_focus_pending = true;
                }
            }
        }
    }

    fn handle_command(&mut self, command: Command) {
        if let Some(app_command) = AppCommand::from_command(command) {
            self.execute_command(app_command);
            return;
        }

        use Command::*;
        match command {
            NewScratch => self.execute_command(AppCommand::NewScratch),
            CloseScratch => self.execute_command(AppCommand::CloseScratch),
            RenameTab => self.execute_command(AppCommand::RenameTab),
            Cut => self.execute_command(AppCommand::Cut),
            Copy => self.execute_command(AppCommand::Copy),
            Paste => self.execute_command(AppCommand::Paste),
            SelectAll => self.execute_command(AppCommand::SelectAll),
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
                        crate::editor::convert_case_all_selections(
                            document,
                            crate::editor::CaseType::Upper,
                        );
                    } else {
                        crate::editor::convert_case_selection(
                            document,
                            crate::editor::CaseType::Upper,
                        );
                    }
                    self.document_edited();
                }
            }
            LowerCase => {
                if let Some(document) = self.state.active_document_mut() {
                    if document.selections.len() > 1 {
                        crate::editor::convert_case_all_selections(
                            document,
                            crate::editor::CaseType::Lower,
                        );
                    } else {
                        crate::editor::convert_case_selection(
                            document,
                            crate::editor::CaseType::Lower,
                        );
                    }
                    self.document_edited();
                }
            }
            TitleCase => {
                if let Some(document) = self.state.active_document_mut() {
                    if document.selections.len() > 1 {
                        crate::editor::convert_case_all_selections(
                            document,
                            crate::editor::CaseType::Title,
                        );
                    } else {
                        crate::editor::convert_case_selection(
                            document,
                            crate::editor::CaseType::Title,
                        );
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
            GoToLine => self.execute_command(AppCommand::GoToLine),

            // View
            CommandPalette => self.command_palette.toggle(),
            QuickSwitchTabs => self.tab_switcher.toggle(&self.state),
            ToggleWrapMode => {
                self.settings.wrap_mode = self.settings.wrap_mode.cycle();
                let settings_path = default_settings_path();
                save_settings(&settings_path, &self.settings);
            }
            ToggleVisibleWhitespace => {
                self.settings.show_visible_whitespace = !self.settings.show_visible_whitespace;
                let settings_path = default_settings_path();
                save_settings(&settings_path, &self.settings);
            }
            ToggleIndentGuides => {
                self.settings.show_indentation_guides = !self.settings.show_indentation_guides;
                let settings_path = default_settings_path();
                save_settings(&settings_path, &self.settings);
            }
            ToggleMinimap => {
                self.settings.show_minimap = !self.settings.show_minimap;
                let settings_path = default_settings_path();
                save_settings(&settings_path, &self.settings);
            }
            ToggleStatusBar => {
                self.settings.show_status_bar = !self.settings.show_status_bar;
                let settings_path = default_settings_path();
                save_settings(&settings_path, &self.settings);
            }
            ToggleTheme => {
                self.settings.theme = self.settings.theme.cycle();
                apply_theme(&self.ctx, self.settings.theme);
                let settings_path = default_settings_path();
                save_settings(&settings_path, &self.settings);
            }

            // File operations
            ImportFile => self.import_file(),
            ExportFile => self.export_file(),
            Preferences => self.preferences.toggle(),
            ToggleBookmark => self.toggle_bookmark(),
            JumpToNextBookmark => self.jump_to_next_bookmark(),
            ClearBookmarks => self.clear_bookmarks(),
            SplitPaneHorizontal => self.split_pane_horizontal(),
            SplitPaneVertical => self.split_pane_vertical(),
            ClosePane => self.close_pane(),
            PinTab => {
                let active = self.state.active_document;
                self.toggle_pin_tab(active);
            }
            MoveTabLeft => {
                let active = self.state.active_document;
                self.move_tab_left(active);
            }
            MoveTabRight => {
                let active = self.state.active_document;
                self.move_tab_right(active);
            }
            ReopenLastClosed => self.execute_command(AppCommand::ReopenLastClosed),
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
        use crate::command::KEYBOARD_COMMANDS;

        let shortcuts = crate::command::default_shortcuts();
        let mut pressed: Vec<Command> = Vec::new();

        for binding in shortcuts {
            if KEYBOARD_COMMANDS.contains(&binding.command) {
                let consumed = ctx.input_mut(|input| input.consume_shortcut(&binding.shortcut));
                if consumed {
                    pressed.push(binding.command);
                }
            }
        }

        for command in pressed {
            self.handle_command(command);
        }

        // Raw key handlers (not shortcuts, just single-key presses)

        let rename_tab =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F2));
        if rename_tab {
            self.execute_command(AppCommand::RenameTab);
        }

        let find_under_cursor =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F3));
        if find_under_cursor {
            self.find_under_cursor();
        }

        let next_bookmark =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F4));
        if next_bookmark {
            self.jump_to_next_bookmark();
        }

        // Special-case: Cmd+D calls select_next_occurrence (not raw add_next_match)
        let select_next = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND,
                logical_key: egui::Key::D,
            })
        });
        if select_next {
            self.select_next_occurrence();
        }

        // Special-case: Cmd+Shift+L chooses between split_selection and add_all_matches
        let add_all_or_split_lines = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut {
                modifiers: egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                logical_key: egui::Key::L,
            })
        });
        if add_all_or_split_lines {
            let split_multiline_selection = self
                .state
                .active_document()
                .and_then(|document| {
                    document.selections.first().map(|selection| {
                        let (start, end) = crate::editor::selection_range(*selection);
                        start < end
                            && document
                                .rope
                                .byte_slice(start..end)
                                .chars()
                                .any(|ch| ch == '\n')
                    })
                })
                .unwrap_or(false);
            if split_multiline_selection {
                self.execute_command(AppCommand::SplitSelectionIntoLines);
            } else {
                self.execute_command(AppCommand::AddAllMatches);
            }
        }
    }

    fn close_active_scratch(&mut self) {
        self.commit_rename();
        let old_active = self.state.active_document;
        self.state.close_active(
            self.settings.default_tab_width,
            self.settings.default_soft_tabs,
        );
        // Update any panes that were pointing to the closed document
        for pane in &mut self.panes {
            if pane.document_id == old_active {
                pane.document_id = self.state.active_document;
            }
        }
        self.mark_changed();
        self.refresh_active_document_metadata();
        self.editor_focus_pending = true;
    }

    fn import_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new().set_title("Import File").pick_file() {
            self.import_dropped_file(&path);
        }
    }

    fn import_dropped_file(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                // Create a new document for the dropped file
                let index = self.state.next_untitled_index;
                let mut doc = Document::new_untitled(
                    index,
                    self.settings.default_tab_width,
                    self.settings.default_soft_tabs,
                );
                self.state.next_untitled_index += 1;
                doc.rope = crop::Rope::from(content);
                doc.revision += 1;
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    doc.rename(file_name);
                }
                let doc_id = doc.id;
                self.state.documents.push(doc);
                self.state.tab_order.push(doc_id);
                self.state.set_active(doc_id);
                self.document_edited();
            }
            Err(err) => {
                tracing::error!(error = %err, path = %path.display(), "failed to import dropped file");
            }
        }
    }

    fn import_dropped_text(&mut self, text: &str) {
        if let Some(document) = self.state.active_document_mut() {
            let old_title = document.title_hint.clone();
            document.replace_text(text);
            if document.title_hint != old_title {
                self.mark_changed();
            }
            self.document_edited();
        }
    }

    fn export_file(&mut self) {
        if let Some(document) = self.state.active_document() {
            let content = document.text();
            let suggested_name = document
                .title_hint
                .trim()
                .strip_prefix("Scratch ")
                .unwrap_or(&document.title_hint)
                .to_owned();

            if let Some(path) = rfd::FileDialog::new()
                .set_title("Export File")
                .set_file_name(if suggested_name.is_empty() {
                    "untitled"
                } else {
                    &suggested_name
                })
                .save_file()
            {
                match std::fs::write(&path, content) {
                    Ok(()) => {
                        tracing::info!(path = %path.display(), "file exported successfully");
                    }
                    Err(err) => {
                        tracing::error!(error = %err, path = %path.display(), "failed to export file");
                    }
                }
            }
        }
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

    fn render_goto_line_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Go to line:");
            let response = ui.add_sized(
                [100.0, ui.spacing().interact_size.y],
                egui::TextEdit::singleline(&mut self.goto_line.query)
                    .hint_text("Line number")
                    .desired_width(100.0),
            );

            if self.goto_line.focus_pending {
                response.request_focus();
                self.goto_line.focus_pending = false;
            }

            let pressed_enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
            let pressed_escape = ui.input(|input| input.key_pressed(egui::Key::Escape));

            if response.lost_focus() && pressed_enter {
                self.execute_goto_line();
            }

            if pressed_escape {
                self.goto_line.visible = false;
                self.goto_line.query.clear();
            }

            if ui.button("Go").clicked() {
                self.execute_goto_line();
            }
        });
    }

    fn execute_goto_line(&mut self) {
        if let Ok(line_num) = self.goto_line.query.parse::<usize>() {
            if line_num > 0 {
                if let Some(document) = self.state.active_document_mut() {
                    crate::editor::move_to_line(document, line_num);
                    self.document_edited();
                }
            }
        }
        self.goto_line.visible = false;
        self.goto_line.query.clear();
    }

    fn toggle_bookmark(&mut self) {
        if let Some(document) = self.state.active_document_mut() {
            let caret = crate::editor::primary_selection(document).head;
            if document.bookmarks.contains(&caret) {
                document.bookmarks.remove(&caret);
            } else {
                document.bookmarks.insert(caret);
            }
            self.mark_changed();
        }
    }

    fn jump_to_next_bookmark(&mut self) {
        if let Some(document) = self.state.active_document_mut() {
            let caret = crate::editor::primary_selection(document).head;
            if let Some(&next) = document.bookmarks.iter().find(|&&bm| bm > caret) {
                crate::editor::set_primary_selection(
                    document,
                    crate::model::Selection::caret(next),
                );
            } else if let Some(&first) = document.bookmarks.first() {
                // Wrap around
                crate::editor::set_primary_selection(
                    document,
                    crate::model::Selection::caret(first),
                );
            }
            self.editor_focus_pending = true;
        }
    }

    fn clear_bookmarks(&mut self) {
        if let Some(document) = self.state.active_document_mut() {
            document.bookmarks.clear();
            self.mark_changed();
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
                        ui.painter().rect_filled(ui.max_rect(), 0.0, bg);
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
                                egui::Label::new(egui::RichText::new(title).monospace().weak()),
                            );
                        }

                        ui.label(egui::RichText::new(before).monospace().weak());
                        ui.label(
                            egui::RichText::new(matched)
                                .monospace()
                                .strong()
                                .background_color(ui.style().visuals.selection.bg_fill),
                        );
                        ui.label(egui::RichText::new(after).monospace().weak());
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

    fn render_pane(&mut self, ui: &mut egui::Ui, pane_index: usize) {
        let Some(pane) = self.panes.get_mut(pane_index) else {
            ui.label("Invalid pane index");
            return;
        };

        // Ensure the document exists; if not, create a new one
        if self.state.document(pane.document_id).is_none() {
            tracing::warn!(?pane.document_id, pane_index, "document not found, creating new one");
            self.state.open_untitled(
                self.settings.default_tab_width,
                self.settings.default_soft_tabs,
            );
            pane.document_id = self.state.active_document;
        }

        let document_id = pane.document_id;
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

        // Get pane view state before any borrowing
        let mut view_state = self.panes[pane_index].view_state.clone();

        // Check if minimap should be shown (before borrowing document mutably)
        let show_minimap = self.settings.show_minimap && {
            let doc = self
                .state
                .document(document_id)
                .expect("document must exist");
            minimap::should_show_minimap(&doc.rope)
        };

        if show_minimap {
            // Render editor and minimap side by side
            ui.horizontal(|ui| {
                // Editor takes most of the space
                let config = MinimapConfig::default();
                let editor_width = ui.available_width() - config.width - 4.0;

                // Render editor
                let editor_response = {
                    let document = self
                        .state
                        .document_mut(document_id)
                        .expect("document must exist");
                    ui.allocate_ui_with_layout(
                        egui::vec2(editor_width, ui.available_height()),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            show_editor(
                                ui,
                                document,
                                &mut view_state,
                                &mut self.editor_focus_pending,
                                reveal_selection,
                                &search_highlights,
                                &extra_selections,
                                self.settings.wrap_mode,
                                &self.settings.rulers,
                                self.settings.show_visible_whitespace,
                                self.settings.show_indentation_guides,
                                self.settings.theme,
                                &self.settings.font_family,
                                self.settings.font_size,
                                self.settings.line_height_scale,
                            )
                        },
                    )
                };

                if editor_response.inner.changed {
                    self.document_edited();
                }

                ui.separator();

                // Render minimap
                let (scroll_y, content_height) = {
                    let doc = self
                        .state
                        .document(document_id)
                        .expect("document must exist");
                    (
                        doc.scroll.y,
                        doc.rope.lines().count() as f32
                            * ui.text_style_height(&egui::TextStyle::Monospace),
                    )
                };

                let config = MinimapConfig::new(self.settings.theme);
                let viewport_height = ui.available_height();

                let doc = self
                    .state
                    .document(document_id)
                    .expect("document must exist");
                let minimap_result = minimap::show_minimap(
                    ui,
                    &doc.rope,
                    scroll_y,
                    viewport_height,
                    content_height,
                    &config,
                    self.settings.theme,
                );
                let _ = doc; // Drop immutable borrow

                if let Some(target_scroll_y) = minimap_result.target_scroll_y {
                    let doc = self
                        .state
                        .document_mut(document_id)
                        .expect("document must exist");
                    doc.scroll.y = target_scroll_y;
                    ui.ctx().request_repaint();
                }
            });
        } else {
            let document = self
                .state
                .document_mut(document_id)
                .expect("document must exist");
            let response = show_editor(
                ui,
                document,
                &mut view_state,
                &mut self.editor_focus_pending,
                reveal_selection,
                &search_highlights,
                &extra_selections,
                self.settings.wrap_mode,
                &self.settings.rulers,
                self.settings.show_visible_whitespace,
                self.settings.show_indentation_guides,
                self.settings.theme,
                &self.settings.font_family,
                self.settings.font_size,
                self.settings.line_height_scale,
            );

            if response.changed {
                self.document_edited();
            }
        }

        // Save back the view state
        self.panes[pane_index].view_state = view_state;
    }
}

impl eframe::App for PileApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let frame_start = std::time::Instant::now();

        // Check for system shutdown signal
        if self.shutdown_flag.load(Ordering::Relaxed) {
            self.flush_session();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.handle_native_menu_commands();
        self.handle_clipboard_events(ctx);
        self.handle_keyboard_shortcuts(ctx);
        self.handle_parse_events();

        // Drain worker events
        if let Some(rx) = &self.worker_event_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    WorkerEvent::Recovery(recovery) => {
                        self.recovery_events.push(recovery);
                    }
                    WorkerEvent::Telemetry(tel) => {
                        self.telemetry = tel;
                    }
                }
            }
        }

        // Handle drag-and-drop files/text
        let dropped_files: Vec<egui::DroppedFile> = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped_files.is_empty() {
            for file in &dropped_files {
                if let Some(path) = &file.path {
                    self.import_dropped_file(path);
                } else if let Some(bytes) = &file.bytes
                    && let Ok(text) = String::from_utf8(bytes.to_vec())
                {
                    self.import_dropped_text(&text);
                }
            }
        }

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("+").on_hover_text("New scratch").clicked() {
                    self.execute_command(AppCommand::NewScratch);
                }

                // Horizontal tab list with scrolling
                let tab_ids: Vec<_> = self.state.tab_order.iter().copied().collect();
                if !tab_ids.is_empty() {
                    egui::ScrollArea::horizontal()
                        .id_salt("tab-scroll")
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for document_id in tab_ids {
                                    self.render_tab(ui, document_id);
                                }
                            });
                        });
                }
            });
        });

        if self.settings.show_status_bar {
            egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
                if self.search.visible {
                    self.render_search_bar(ui);
                    if self.search.preview_visible && !self.search.preview_items.is_empty() {
                        ui.separator();
                        self.render_search_preview(ui);
                    }
                    ui.separator();
                }

                if self.goto_line.visible {
                    self.render_goto_line_bar(ui);
                    ui.separator();
                }

                // Show recovery events as dismissable toasts
                let mut dismissed = None;
                for (idx, event) in self.recovery_events.iter().enumerate() {
                    let color = match event.kind {
                        RecoveryEventKind::SaveFailed
                        | RecoveryEventKind::QuarantineFailed
                        | RecoveryEventKind::BackupFailed => egui::Color32::from_rgb(255, 120, 120),
                        RecoveryEventKind::SessionCorrupt => egui::Color32::from_rgb(255, 180, 50),
                        _ => egui::Color32::from_rgb(120, 200, 120),
                    };
                    ui.horizontal(|ui| {
                        ui.colored_label(color, &event.message);
                        if ui.small_button("×").clicked() {
                            dismissed = Some(idx);
                        }
                    });
                }
                if let Some(idx) = dismissed {
                    self.recovery_events.remove(idx);
                }

                ui.horizontal(|ui| {
                    let (lang, confidence) =
                        (self.last_detection.language, self.last_detection.confidence);
                    let has_parse_errors = self
                        .state
                        .active_document()
                        .map_or(false, |doc| doc.syntax_state.has_parse_errors());
                    let low_confidence = confidence < 0.5;

                    let lang_text = format!("{lang:?} ({confidence:.0}%)");
                    let response = ui.label(lang_text);
                    if low_confidence {
                        response.highlight();
                    }
                    if has_parse_errors {
                        ui.label(
                            egui::RichText::new("⚠ parse issues")
                                .color(egui::Color32::from_rgb(255, 180, 50)),
                        );
                    }

                    ui.separator();
                    let byte_len = self
                        .state
                        .active_document()
                        .map(|document| document.rope.byte_len())
                        .unwrap_or_default();
                    ui.label(format!("{byte_len} bytes"));

                    // Telemetry summary
                    ui.separator();
                    ui.label(format!(
                        "saves: {}/{}",
                        self.telemetry.successful_saves, self.telemetry.total_saves
                    ));
                    if let Some(dur) = self.telemetry.last_save_duration_ms {
                        ui.label(format!("last: {}ms", dur));
                    }
                    if let Some(median) = self.telemetry.median_save_duration_ms() {
                        ui.label(format!("median: {}ms", median));
                    }
                    ui.separator();
                    ui.label(format!("frame: {:.1}ms", self.frame_time_ms));
                });
            });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0))
            .show(ctx, |ui| {
                let pane_count = self.panes.len();
                if pane_count == 1 {
                    self.render_pane(ui, 0);
                } else {
                    // Horizontal split for 2+ panes
                    let available = ui.available_rect_before_wrap();
                    let pane_width = available.width() / pane_count as f32;

                    ui.horizontal(|ui| {
                        let pane_data: Vec<(usize, DocumentId)> = self
                            .panes
                            .iter()
                            .enumerate()
                            .map(|(idx, pane)| (idx, pane.document_id))
                            .collect();

                        for (idx, _document_id) in pane_data {
                            let pane_rect = egui::Rect::from_min_size(
                                egui::pos2(
                                    available.left() + pane_width * idx as f32,
                                    available.top(),
                                ),
                                egui::vec2(pane_width, available.height()),
                            );

                            ui.scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(pane_rect)
                                    .layout(*ui.layout()),
                                |ui| {
                                    self.render_pane(ui, idx);
                                },
                            );

                            if idx < pane_count - 1 {
                                ui.separator();
                            }
                        }
                    });
                }
            });

        let mut cmd = None;
        self.command_palette.show(ctx, &mut |c| cmd = Some(c));
        if let Some(command) = cmd {
            self.handle_command(command);
        }

        let mut switch_to = None;
        let mut delete_doc = None;
        self.tab_switcher.show(
            ctx,
            &self.state,
            &mut |id| switch_to = Some(id),
            &mut |id| delete_doc = Some(id),
        );

        if let Some(document_id) = switch_to {
            if self.state.document(document_id).is_none() {
                // Closed document: reopen and explicitly update the pane
                self.state.reopen_document(document_id);
                if let Some(pane) = self.panes.get_mut(self.active_pane) {
                    pane.document_id = document_id;
                }
                self.mark_changed();
                self.refresh_active_document_metadata();
                self.editor_focus_pending = true;
            } else {
                self.set_active_document(document_id);
            }
        }

        if let Some(document_id) = delete_doc {
            self.pending_delete_confirmation = Some(document_id);
        }

        // Show confirmation dialog for permanent deletion
        if let Some(doc_id) = self.pending_delete_confirmation {
            let doc_title = self
                .state
                .closed_documents()
                .iter()
                .find(|cd| cd.document.id == doc_id)
                .map(|cd| cd.document.display_title())
                .unwrap_or_default();

            let mut dialog_open = true;
            egui::Window::new("Delete forever")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .collapsible(false)
                .resizable(false)
                .open(&mut dialog_open)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Permanently delete \"{doc_title}\"? This cannot be undone."
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.pending_delete_confirmation = None;
                        }
                        if ui.button("Delete").clicked() {
                            self.state.permanently_delete_document(doc_id);
                            self.mark_changed();
                            self.pending_delete_confirmation = None;
                        }
                    });
                });

            if !dialog_open {
                self.pending_delete_confirmation = None;
            }
        }

        // Show preferences window
        self.preferences.show(ctx, &mut self.settings);

        // Apply pending clipboard text
        if let Some(text) = self.clipboard_text.take() {
            ctx.copy_text(text);
        }

        // Record frame time
        self.frame_time_ms = frame_start.elapsed().as_secs_f64() * 1000.0;
        tracing::debug!(target: "pile::frame_time", ms = self.frame_time_ms);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Capture window state before exit
        self.ctx.input(|i| {
            let vp = i.viewport();
            self.settings.window_state = crate::settings::WindowState {
                size: vp.inner_rect.map(|r| [r.width(), r.height()]),
                position: vp.outer_rect.map(|r| [r.min.x, r.min.y]),
                fullscreen: vp.fullscreen,
                maximized: vp.maximized,
            };
        });
        let settings_path = crate::persistence::default_settings_path();
        crate::persistence::save_settings(&settings_path, &self.settings);

        self.flush_session();
        self.worker_event_rx = None; // Drop receiver so worker can finish sending
        if let Some(worker) = self.save_worker.take() {
            worker.shutdown();
        }
    }
}

fn create_snapshot(state: &AppState, panes: &[EditorPane], active_pane: usize) -> SessionSnapshot {
    SessionSnapshot {
        schema_version: 3,
        state: state.clone(),
        panes: panes
            .iter()
            .map(|pane| PaneSnapshot {
                document_id: pane.document_id,
                preferred_column: pane.view_state.preferred_column,
                visible_rows: pane.view_state.visible_rows,
                column_selection: pane.view_state.column_selection,
                column_selection_anchor_col: pane.view_state.column_selection_anchor_col,
            })
            .collect(),
        active_pane,
    }
}

fn should_accept_parse_result(
    document: &Document,
    result: &crate::parse_worker::ParseResult,
) -> bool {
    document.revision == result.revision
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::LanguageId;

    #[test]
    fn fresh_document_accepts_first_parse_result_for_current_revision() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("fn main() {}");

        let result = crate::parse_worker::ParseResult {
            document_id: document.id,
            revision: document.revision,
            tree: None,
            spans: Vec::new(),
            language: LanguageId::Rust,
            visible_start: 0,
            visible_end: document.rope.byte_len(),
        };

        assert_eq!(document.syntax_state.parsed_as(), None);
        assert!(should_accept_parse_result(&document, &result));
    }

    #[test]
    fn stale_parse_result_is_rejected() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("fn main() {}");

        let result = crate::parse_worker::ParseResult {
            document_id: document.id,
            revision: document.revision.saturating_sub(1),
            tree: None,
            spans: Vec::new(),
            language: LanguageId::Rust,
            visible_start: 0,
            visible_end: document.rope.byte_len(),
        };

        assert!(!should_accept_parse_result(&document, &result));
    }
}
