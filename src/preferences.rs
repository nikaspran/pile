use eframe::egui;
use crate::settings::{Settings, Theme, WrapMode};
use crate::theme::apply_theme;
use crate::persistence::{default_settings_path, save_settings};

pub struct PreferencesState {
    pub visible: bool,
    theme_changed: bool,
    settings_path: std::path::PathBuf,
}

impl PreferencesState {
    pub fn new() -> Self {
        Self {
            visible: false,
            theme_changed: false,
            settings_path: default_settings_path(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.theme_changed = false;
        }
    }

    fn save(&self, settings: &Settings) {
        save_settings(&self.settings_path, settings);
    }

    pub fn show(&mut self, ctx: &egui::Context, settings: &mut Settings) {
        if !self.visible {
            return;
        }

        let mut open = true;
        egui::Window::new("Preferences")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.heading("Editor Settings");
                ui.add_space(8.0);

                // Theme selection
                ui.horizontal(|ui| {
                    ui.label("Theme:");
                    let old_theme = settings.theme;
                    if ui.radio_value(&mut settings.theme, Theme::Dark, "Dark").clicked() {
                        if settings.theme == Theme::Dark && old_theme != Theme::Dark {
                            self.theme_changed = true;
                        }
                    }
                    if ui.radio_value(&mut settings.theme, Theme::Light, "Light").clicked() {
                        if settings.theme == Theme::Light && old_theme != Theme::Light {
                            self.theme_changed = true;
                        }
                    }
                });

                // Wrap mode selection
                ui.horizontal(|ui| {
                    ui.label("Wrap mode:");
                    let old_wrap = settings.wrap_mode;
                    ui.radio_value(&mut settings.wrap_mode, WrapMode::NoWrap, "No Wrap");
                    ui.radio_value(&mut settings.wrap_mode, WrapMode::ViewportWrap, "Viewport");
                    ui.radio_value(&mut settings.wrap_mode, WrapMode::RulerWrap, "Ruler");
                    if settings.wrap_mode != old_wrap {
                        self.save(settings);
                    }
                });

                ui.separator();

                // Toggle options
                let mut show_whitespace = settings.show_visible_whitespace;
                if ui.checkbox(&mut show_whitespace, "Show visible whitespace").clicked() {
                    settings.show_visible_whitespace = show_whitespace;
                    self.save(settings);
                }

                let mut show_indent = settings.show_indentation_guides;
                if ui.checkbox(&mut show_indent, "Show indentation guides").clicked() {
                    settings.show_indentation_guides = show_indent;
                    self.save(settings);
                }

                let mut show_minimap = settings.show_minimap;
                if ui.checkbox(&mut show_minimap, "Show minimap").clicked() {
                    settings.show_minimap = show_minimap;
                    self.save(settings);
                }

                // Apply theme immediately if changed
                if self.theme_changed {
                    apply_theme(ctx, settings.theme);
                    self.theme_changed = false;
                    self.save(settings);
                }

                ui.separator();
                if ui.button("Close").clicked() {
                    self.visible = false;
                }
            });

        if !open {
            self.visible = false;
        }
    }
}
