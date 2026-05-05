use eframe::egui;

use crate::command::{Command, CommandMetadata, all_commands};

pub struct CommandPalette {
    pub visible: bool,
    pub query: String,
    pub selected_index: usize,
    commands: Vec<CommandMetadata>,
    filtered_indices: Vec<usize>,
}

impl CommandPalette {
    pub fn new() -> Self {
        let commands = all_commands();
        Self {
            visible: false,
            query: String::new(),
            selected_index: 0,
            commands,
            filtered_indices: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.query.clear();
            self.selected_index = 0;
            self.update_filter();
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, on_command: &mut dyn FnMut(Command)) {
        if !self.visible {
            return;
        }

        let mut open = true;
        egui::Window::new("Command Palette")
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
                        self.update_filter();
                    }
                    if response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        self.execute_selected(on_command);
                    }
                });

                ui.separator();

                self.render_list(ui);
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
                    self.selected_index = (self.selected_index + 1).min(self.filtered_indices.len().saturating_sub(1));
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                    self.execute_selected(on_command);
                }
            });
        }
    }

    fn update_filter(&mut self) {
        self.filtered_indices.clear();
        for (idx, cmd) in self.commands.iter().enumerate() {
            if cmd.matches_query(&self.query) {
                self.filtered_indices.push(idx);
            }
        }
        self.selected_index = self.selected_index.min(self.filtered_indices.len().saturating_sub(1));
    }

    fn render_list(&mut self, ui: &mut egui::Ui) {
        let filtered_count = self.filtered_indices.len();
        if filtered_count == 0 {
            ui.label("No commands found");
            return;
        }

        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for (list_idx, &cmd_idx) in self.filtered_indices.iter().enumerate() {
                    let cmd = &self.commands[cmd_idx];
                    let is_selected = list_idx == self.selected_index;

                    let response = ui.selectable_label(is_selected, cmd.name);
                    if is_selected {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                    if response.clicked() {
                        self.selected_index = list_idx;
                        self.execute_command(cmd.command, &mut |_c| {});
                    }
                }
            });
    }

    fn execute_selected(&mut self, on_command: &mut dyn FnMut(Command)) {
        if let Some(&cmd_idx) = self.filtered_indices.get(self.selected_index) {
            let cmd = self.commands[cmd_idx].command;
            self.visible = false;
            self.query.clear();
            on_command(cmd);
        }
    }

    fn execute_command(&self, command: Command, on_command: &mut dyn FnMut(Command)) {
        on_command(command);
    }
}
