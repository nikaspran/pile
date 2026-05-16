use eframe::egui;
use uuid::Uuid;

use crate::command::fuzzy_match;
use crate::model::{AppState, Document};
use crate::search::{SearchOptions, build_preview_items, find_matches};

pub struct TabSwitcher {
    pub visible: bool,
    pub query: String,
    pub selected_index: usize,
    focus_pending: bool,
    tabs: Vec<TabItem>,
}

#[derive(Clone)]
struct TabItem {
    id: Uuid,
    title: String,
    closed: bool,
    content_preview: Option<String>,
}

impl TabSwitcher {
    pub fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            selected_index: 0,
            focus_pending: false,
            tabs: Vec::new(),
        }
    }

    pub fn toggle(&mut self, state: &AppState) {
        if self.visible {
            self.visible = false;
        } else {
            self.visible = true;
            self.query.clear();
            self.build_tab_list(state);
            self.selected_index = 0;
            self.focus_pending = true;
        }
    }

    fn build_tab_list(&mut self, state: &AppState) {
        self.tabs.clear();
        let query = self.query.trim();

        // Open tabs from recent_order (most recent first)
        let order = if !state.recent_order().is_empty() {
            state.recent_order()
        } else {
            &state.tab_order
        };

        for &doc_id in order {
            if let Some(doc) = state.document(doc_id) {
                self.tabs.push(Self::tab_item(doc, false, query));
            }
        }

        // Closed documents appended below, most recently closed first
        let closed = state.closed_documents();
        for cd in closed.iter().rev() {
            self.tabs.push(Self::tab_item(&cd.document, true, query));
        }
    }

    fn tab_item(document: &Document, closed: bool, query: &str) -> TabItem {
        TabItem {
            id: document.id,
            title: document.display_title(),
            closed,
            content_preview: content_preview(document, query),
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        state: &AppState,
        on_switch: &mut dyn FnMut(Uuid),
        on_delete: &mut dyn FnMut(Uuid),
    ) {
        if !self.visible {
            return;
        }

        // Rebuild tab list in case tabs changed
        self.build_tab_list(state);

        let mut open = true;
        egui::Window::new("Quick Tab Switcher")
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 40.0))
            .max_width(500.0)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    let response = ui.text_edit_singleline(&mut self.query);
                    if self.focus_pending {
                        response.request_focus();
                        self.focus_pending = false;
                    }
                    if response.changed() {
                        self.selected_index = 0;
                    }
                });

                ui.separator();

                let filtered: Vec<(usize, &TabItem)> = self
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.matches_query(&self.query))
                    .collect();

                if filtered.is_empty() {
                    ui.label("No documents found");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            let mut delete_id: Option<Uuid> = None;
                            for (list_idx, (_orig_idx, item)) in filtered.iter().enumerate() {
                                let is_selected = list_idx == self.selected_index;

                                ui.horizontal(|ui| {
                                    let prefix = if item.closed { "○ " } else { "" };
                                    let label_text = format!("{}{}", prefix, item.title);

                                    let response = if item.closed {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&label_text)
                                                    .color(egui::Color32::GRAY),
                                            )
                                            .sense(egui::Sense::click()),
                                        )
                                    } else {
                                        ui.selectable_label(is_selected, &label_text)
                                    };
                                    if let Some(preview) = &item.content_preview {
                                        ui.label(egui::RichText::new(preview).weak().small());
                                    }

                                    if is_selected {
                                        response.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    if response.clicked() {
                                        self.selected_index = list_idx;
                                        on_switch(item.id);
                                        self.visible = false;
                                    }

                                    // Delete button for closed docs on hover or when selected
                                    if item.closed {
                                        if is_selected || ui.rect_contains_pointer(response.rect) {
                                            if ui
                                                .small_button("✕")
                                                .on_hover_text("Delete forever")
                                                .clicked()
                                            {
                                                delete_id = Some(item.id);
                                                self.visible = false;
                                            }
                                        }
                                    }
                                });
                            }

                            if let Some(id) = delete_id {
                                on_delete(id);
                            }
                        });
                }
            });

        if !open {
            self.visible = false;
        }

        // Handle keyboard events
        if self.visible {
            ctx.input_mut(|input| {
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    self.visible = false;
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    let count = self
                        .tabs
                        .iter()
                        .filter(|item| item.matches_query(&self.query))
                        .count();
                    self.selected_index = (self.selected_index + 1).min(count.saturating_sub(1));
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                    let filtered: Vec<&TabItem> = self
                        .tabs
                        .iter()
                        .filter(|item| item.matches_query(&self.query))
                        .collect();
                    if let Some(item) = filtered.get(self.selected_index) {
                        on_switch(item.id);
                        self.visible = false;
                    }
                }
            });
        }
    }
}

impl TabItem {
    fn matches_query(&self, query: &str) -> bool {
        query.is_empty() || fuzzy_match(query, &self.title) || self.content_preview.is_some()
    }
}

fn content_preview(document: &Document, query: &str) -> Option<String> {
    if query.is_empty() {
        return None;
    }

    let search_match = find_matches(
        &document.rope,
        query,
        SearchOptions {
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
        },
    )
    .into_iter()
    .next()?;

    let preview = build_preview_items(&document.rope, &[search_match], 32)
        .into_iter()
        .next()?;
    let context = format!(
        "{}{}{}",
        preview.context_before, preview.matched_text, preview.context_after
    )
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ");
    Some(format!("L{}: {context}", preview.line_number))
}

#[cfg(test)]
mod tests {
    use crop::Rope;

    use super::*;

    #[test]
    fn tab_switcher_matches_document_content() {
        let mut state = AppState::empty();
        let first_id = state.active_document;
        state.document_mut(first_id).unwrap().rope = Rope::from("meeting notes");

        let second_id = state.open_untitled(4, true);
        state.document_mut(second_id).unwrap().rope = Rope::from("find the hidden needle here");

        let mut switcher = TabSwitcher::new();
        switcher.query = "needle".to_owned();
        switcher.build_tab_list(&state);

        let matches: Vec<_> = switcher
            .tabs
            .iter()
            .filter(|item| item.matches_query(&switcher.query))
            .collect();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, second_id);
        assert_eq!(
            matches[0].content_preview.as_deref(),
            Some("L1: find the hidden needle here")
        );
    }

    #[test]
    fn tab_switcher_requests_focus_when_opened() {
        let state = AppState::empty();
        let mut switcher = TabSwitcher::new();

        switcher.toggle(&state);

        assert!(switcher.visible);
        assert!(switcher.focus_pending);
    }
}
