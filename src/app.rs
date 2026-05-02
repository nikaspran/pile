use std::time::Duration;

use crossbeam_channel::{Sender, bounded};
use eframe::egui;
use tracing::{info, warn};

use crate::{
    model::{AppState, DocumentId, SessionSnapshot},
    persistence::{
        SaveMsg, SaveWorker, default_session_path, load_session, quarantine_corrupt_session,
    },
    syntax::{LanguageDetection, LanguageRegistry},
};

const LINE_GUTTER_MIN_WIDTH: f32 = 44.0;
const LINE_GUTTER_PADDING: f32 = 10.0;

pub struct PileApp {
    state: AppState,
    save_tx: Sender<SaveMsg>,
    save_worker: Option<SaveWorker>,
    syntax: LanguageRegistry,
    editor_text: String,
    last_loaded_document: DocumentId,
    last_detection: LanguageDetection,
    renaming_document: Option<DocumentId>,
    rename_text: String,
    rename_focus_pending: bool,
    editor_focus_pending: bool,
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
        let editor_text = state
            .active_document()
            .map(|document| document.text())
            .unwrap_or_default();
        let last_loaded_document = state.active_document;
        let syntax = LanguageRegistry;
        let last_detection = syntax.detect(&editor_text);

        info!(documents = state.documents.len(), "pile started");

        Self {
            state,
            save_tx,
            save_worker: Some(save_worker),
            syntax,
            editor_text,
            last_loaded_document,
            last_detection,
            renaming_document: None,
            rename_text: String::new(),
            rename_focus_pending: false,
            editor_focus_pending: true,
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

    fn sync_editor_text_from_active_document(&mut self) {
        if self.last_loaded_document == self.state.active_document {
            return;
        }

        self.editor_text = self
            .state
            .active_document()
            .map(|document| document.text())
            .unwrap_or_default();
        self.last_loaded_document = self.state.active_document;
        self.last_detection = self.syntax.detect(&self.editor_text);
    }

    fn commit_editor_text(&mut self) {
        if let Some(document) = self.state.active_document_mut()
            && document.text() != self.editor_text
        {
            document.replace_text(&self.editor_text);
            self.last_detection = self.syntax.detect(&self.editor_text);
            self.mark_changed();
        }
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
            self.commit_editor_text();
            self.state.set_active(document_id);
            self.mark_changed();
            self.sync_editor_text_from_active_document();
            self.editor_focus_pending = true;
        }

        if response.double_clicked() {
            self.begin_rename(document_id);
        }
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("+").on_hover_text("New scratch").clicked() {
                self.new_scratch();
            }

            if ui.button("x").on_hover_text("Close scratch").clicked() {
                self.close_active_scratch();
            }

            ui.separator();

            if ui.button("R").on_hover_text("Rename tab").clicked() {
                self.begin_rename(self.state.active_document);
            }

            ui.add_enabled(false, egui::Button::new("Search").small())
                .on_disabled_hover_text("Search is planned");
            ui.add_enabled(false, egui::Button::new("Replace").small())
                .on_disabled_hover_text("Replace is planned");
        });
    }

    fn new_scratch(&mut self) {
        self.commit_rename();
        self.commit_editor_text();
        self.state.open_untitled();
        self.mark_changed();
        self.sync_editor_text_from_active_document();
        self.editor_focus_pending = true;
    }

    fn close_active_scratch(&mut self) {
        self.commit_rename();
        self.commit_editor_text();
        self.state.close_active();
        self.mark_changed();
        self.sync_editor_text_from_active_document();
        self.editor_focus_pending = true;
    }

    fn render_editor(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

        let line_count = line_count(&self.editor_text);
        let line_digits = decimal_digits(line_count);
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let rows_for_height = (ui.available_height() / row_height).ceil() as usize;
        let gutter_width = (line_digits as f32 * 8.0 + LINE_GUTTER_PADDING * 2.0)
            .max(LINE_GUTTER_MIN_WIDTH);

        egui::ScrollArea::both()
            .id_salt("editor-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(gutter_width);
                        for line in 1..=line_count {
                            ui.add_sized(
                                [gutter_width, row_height],
                                egui::Label::new(
                                    egui::RichText::new(line.to_string())
                                        .monospace()
                                        .color(ui.visuals().weak_text_color()),
                                )
                                .selectable(false),
                            );
                        }
                    });

                    ui.separator();

                    let available_width = ui.available_width().max(320.0);
                    let response = egui::TextEdit::multiline(&mut self.editor_text)
                        .desired_width(available_width)
                        .desired_rows(line_count.max(rows_for_height).max(1))
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .frame(false)
                        .show(ui);

                    if self.editor_focus_pending {
                        response.response.request_focus();
                        self.editor_focus_pending = false;
                    }

                    if response.response.changed() {
                        self.commit_editor_text();
                    }
                });
            });
    }
}

impl eframe::App for PileApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_editor_text_from_active_document();

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            self.render_toolbar(ui);
        });

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                let tabs = self.state.tab_order.clone();
                for document_id in tabs {
                    self.render_tab(ui, document_id);
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{:?} ({:.0}%)",
                    self.last_detection.language,
                    self.last_detection.confidence * 100.0
                ));
                ui.separator();
                ui.label(format!("{} bytes", self.editor_text.len()));
            });
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0))
            .show(ctx, |ui| {
                self.render_editor(ui);
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.commit_editor_text();
        self.flush_session();
        if let Some(worker) = self.save_worker.take() {
            worker.shutdown();
        }
    }
}

fn line_count(text: &str) -> usize {
    text.lines().count().max(1)
}

fn decimal_digits(value: usize) -> usize {
    value.checked_ilog10().map_or(1, |digits| digits as usize + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_count_has_a_minimum_of_one() {
        assert_eq!(line_count(""), 1);
        assert_eq!(line_count("one"), 1);
        assert_eq!(line_count("one\ntwo"), 2);
    }

    #[test]
    fn decimal_digits_tracks_gutter_growth() {
        assert_eq!(decimal_digits(1), 1);
        assert_eq!(decimal_digits(9), 1);
        assert_eq!(decimal_digits(10), 2);
        assert_eq!(decimal_digits(100), 3);
    }
}
