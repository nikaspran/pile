use eframe::egui;

/// Available bundled themes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark",
            Theme::Light => "Light",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }

    /// Returns the egui Style for this theme.
    pub fn egui_style(self) -> egui::Style {
        let mut style = egui::Style::default();
        style.visuals = match self {
            Theme::Dark => dark_visuals(),
            Theme::Light => light_visuals(),
        };
        style
    }

    /// Bracket highlight color for this theme.
    pub fn bracket_highlight(self) -> egui::Color32 {
        match self {
            Theme::Dark => egui::Color32::from_rgba_premultiplied(255, 255, 255, 60),
            Theme::Light => egui::Color32::from_rgba_premultiplied(0, 0, 0, 60),
        }
    }

    /// Current line highlight color for this theme.
    pub fn current_line_highlight(self) -> egui::Color32 {
        match self {
            Theme::Dark => egui::Color32::from_rgba_premultiplied(255, 255, 255, 8),
            Theme::Light => egui::Color32::from_rgba_premultiplied(0, 0, 0, 8),
        }
    }

    /// Indentation guide color for this theme.
    pub fn indent_guide(self) -> egui::Color32 {
        match self {
            Theme::Dark => egui::Color32::from_rgba_premultiplied(255, 255, 255, 20),
            Theme::Light => egui::Color32::from_rgba_premultiplied(0, 0, 0, 20),
        }
    }

    /// Bookmark indicator color (same in both themes).
    pub fn bookmark(self) -> egui::Color32 {
        egui::Color32::from_rgb(255, 200, 0)
    }

    /// Minimap text color for this theme.
    pub fn minimap_text(self) -> egui::Color32 {
        match self {
            Theme::Dark => egui::Color32::from_rgba_premultiplied(180, 180, 180, 30),
            Theme::Light => egui::Color32::from_rgba_premultiplied(100, 100, 100, 30),
        }
    }

    /// Minimap comment color for this theme.
    pub fn minimap_comment(self) -> egui::Color32 {
        match self {
            Theme::Dark => egui::Color32::from_rgba_premultiplied(100, 100, 100, 40),
            Theme::Light => egui::Color32::from_rgba_premultiplied(150, 150, 150, 40),
        }
    }

    /// Minimap keyword color for this theme.
    pub fn minimap_keyword(self) -> egui::Color32 {
        match self {
            Theme::Dark => egui::Color32::from_rgba_premultiplied(150, 200, 255, 60),
            Theme::Light => egui::Color32::from_rgba_premultiplied(100, 150, 200, 60),
        }
    }

    /// Minimap viewport color (same in both themes).
    pub fn minimap_viewport(self) -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(100, 100, 255, 40)
    }

    /// Minimap viewport border color (same in both themes).
    pub fn minimap_viewport_border(self) -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(150, 150, 255, 100)
    }
}

fn dark_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(30, 30, 30);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(35, 35, 35);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 45, 45);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(50, 50, 50);
    visuals.window_fill = egui::Color32::from_rgb(25, 25, 25);
    visuals.panel_fill = egui::Color32::from_rgb(30, 30, 30);
    visuals.selection.bg_fill = egui::Color32::from_rgba_premultiplied(100, 150, 255, 100);
    visuals.selection.stroke.color = egui::Color32::from_rgb(255, 255, 255);
    visuals.override_text_color = Some(egui::Color32::from_rgb(220, 220, 220));
    visuals.warn_fg_color = egui::Color32::from_rgb(255, 200, 50);
    visuals
}

fn light_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::light();
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(240, 240, 240);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(235, 235, 235);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(225, 225, 225);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(215, 215, 215);
    visuals.window_fill = egui::Color32::from_rgb(245, 245, 245);
    visuals.panel_fill = egui::Color32::from_rgb(240, 240, 240);
    visuals.selection.bg_fill = egui::Color32::from_rgba_premultiplied(100, 150, 255, 100);
    visuals.selection.stroke.color = egui::Color32::from_rgb(0, 0, 0);
    visuals.override_text_color = Some(egui::Color32::from_rgb(30, 30, 30));
    visuals.warn_fg_color = egui::Color32::from_rgb(200, 150, 0);
    visuals
}

/// Apply the given theme to the egui context.
pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    ctx.set_style(theme.egui_style());
}
