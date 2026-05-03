use eframe::egui;
use uuid::Uuid;

use crate::command::fuzzy_match;
use crate::model::AppState;

pub struct TabSwitcher {
    pub visible: bool,
    pub query: String,
    pub selected_index: usize,
    tabs: Vec<TabItem>,
}

#[derive(Clone)]
struct TabItem {
    id: Uuid,
    title: String,
}

impl TabSwitcher {
    pub fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            selected_index: 0,
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
        }
    }

    fn build_tab_list(&mut self, state: &AppState) {
        self.tabs.clear();
        for &doc_id in &state.tab_order {
            if let Some(doc) = state.document(doc_id) {
                self.tabs.push(TabItem {
                    id: doc_id,
                    title: doc.display_title(),
                });
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &AppState, on_switch: &mut dyn FnMut(Uuid)) {
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
                    if response.changed() {
                        self.selected_index = 0;
                    }
                });

                ui.separator();

                let filtered: Vec<(usize, &TabItem)> = self.tabs
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| fuzzy_match(&self.query, &item.title))
                    .collect();

                if filtered.is_empty() {
                    ui.label("No tabs found");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for (list_idx, (_, item)) in filtered.iter().enumerate() {
                                let is_selected = list_idx == self.selected_index;
                                let response = ui.selectable_label(is_selected, &item.title);
                                if is_selected {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                if response.clicked() {
                                    self.selected_index = list_idx;
                                    on_switch(item.id);
                                    self.visible = false;
                                }
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
                    let count = self.tabs.iter().filter(|item| fuzzy_match(&self.query, &item.title)).count();
                    self.selected_index = (self.selected_index + 1).min(count.saturating_sub(1));
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                    let filtered: Vec<Uuid> = self.tabs
                        .iter()
                        .filter(|item| fuzzy_match(&self.query, &item.title))
                        .map(|item| item.id)
                        .collect();
                    if let Some(&id) = filtered.get(self.selected_index) {
                        on_switch(id);
                        self.visible = false;
                    }
                }
            });
        }
    }
}
