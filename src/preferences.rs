use eframe::egui;
use crate::settings::{FontFamily, Settings, Theme, WrapMode};
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
            .resizable(true)
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

                ui.separator();

                // Font settings
                ui.collapsing("Font", |ui| {
                    // Font family
                    ui.horizontal(|ui| {
                        ui.label("Family:");
                        let mut use_custom = matches!(settings.font_family, FontFamily::Named(_));
                        let old_use_custom = use_custom;
                        if ui.radio_value(&mut use_custom, false, "Default Monospace").clicked() {
                            if use_custom != old_use_custom {
                                settings.font_family = FontFamily::Default;
                                self.save(settings);
                                crate::settings::apply_font_settings(ctx, &settings.font_family, settings.font_size, settings.line_height_scale);
                            }
                        }
                        if ui.radio_value(&mut use_custom, true, "Custom:").clicked() {
                            if use_custom != old_use_custom {
                                if !matches!(settings.font_family, FontFamily::Named(_)) {
                                    settings.font_family = FontFamily::Named("Courier New".to_owned());
                                }
                                self.save(settings);
                                crate::settings::apply_font_settings(ctx, &settings.font_family, settings.font_size, settings.line_height_scale);
                            }
                        }
                        if use_custom {
                            let mut font_name = match &settings.font_family {
                                FontFamily::Default => "Courier New".to_owned(),
                                FontFamily::Named(name) => name.clone(),
                            };
                            if ui.text_edit_singleline(&mut font_name).changed() {
                                settings.font_family = FontFamily::Named(font_name);
                                self.save(settings);
                                crate::settings::apply_font_settings(ctx, &settings.font_family, settings.font_size, settings.line_height_scale);
                            }
                        }
                    });

                    // Font size
                    ui.horizontal(|ui| {
                        ui.label("Size:");
                        let mut font_size = settings.font_size;
                        if ui.add(egui::DragValue::new(&mut font_size).speed(0.5).range(8.0..=32.0)).changed() {
                            settings.font_size = font_size;
                            self.save(settings);
                            crate::settings::apply_font_settings(ctx, &settings.font_family, settings.font_size, settings.line_height_scale);
                        }
                        ui.label("pt");
                    });

                    // Line height scale
                    ui.horizontal(|ui| {
                        ui.label("Line height:");
                        let mut line_height = settings.line_height_scale;
                        if ui.add(egui::DragValue::new(&mut line_height).speed(0.05).range(0.5..=3.0).fixed_decimals(2)).changed() {
                            settings.line_height_scale = line_height;
                            self.save(settings);
                            crate::settings::apply_font_settings(ctx, &settings.font_family, settings.font_size, settings.line_height_scale);
                        }
                    });
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

                // Default tab settings
                ui.collapsing("Tab Settings", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Default tab width:");
                        let mut tab_width = settings.default_tab_width;
                        if ui.add(egui::DragValue::new(&mut tab_width).range(1..=16)).changed() {
                            settings.default_tab_width = tab_width;
                            self.save(settings);
                        }
                        ui.label("spaces");
                    });

                    let mut soft_tabs = settings.default_soft_tabs;
                    if ui.checkbox(&mut soft_tabs, "Use soft tabs (spaces)").clicked() {
                        settings.default_soft_tabs = soft_tabs;
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

                let mut show_status = settings.show_status_bar;
                if ui.checkbox(&mut show_status, "Show status bar").clicked() {
                    settings.show_status_bar = show_status;
                    self.save(settings);
                }

                ui.separator();

                // Ignored languages
                ui.collapsing("Ignored Languages", |ui| {
                    ui.label("Enter language names to ignore (comma-separated):");
                    let mut ignored_text = settings.ignored_languages.join(", ");
                    if ui.text_edit_singleline(&mut ignored_text).changed() {
                        settings.ignored_languages = ignored_text
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        self.save(settings);
                    }
                    if !settings.ignored_languages.is_empty() {
                        ui.label(format!("Currently ignoring: {}", settings.ignored_languages.join(", ")));
                    }
                });

        // Apply theme and font settings immediately if changed
        if self.theme_changed {
            apply_theme(ctx, settings.theme);
            crate::settings::apply_font_settings(ctx, &settings.font_family, settings.font_size, settings.line_height_scale);
            self.theme_changed = false;
            self.save(settings);
        }

        // Apply font settings if font family or size changed
        // We track this with a simple approach - apply on every change
        ctx.request_repaint();

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
