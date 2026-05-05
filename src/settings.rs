use serde::{Deserialize, Serialize};

pub use crate::theme::Theme;

/// Line wrapping mode for the editor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum WrapMode {
    /// No wrapping; lines extend horizontally with scrolling.
    #[default]
    NoWrap,
    /// Wrap lines at the viewport edge.
    ViewportWrap,
    /// Wrap lines at a configured ruler column.
    RulerWrap,
}

impl WrapMode {
    pub fn cycle(self) -> Self {
        match self {
            WrapMode::NoWrap => WrapMode::ViewportWrap,
            WrapMode::ViewportWrap => WrapMode::RulerWrap,
            WrapMode::RulerWrap => WrapMode::NoWrap,
        }
    }

    #[allow(dead_code)]
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            WrapMode::NoWrap => "No Wrap",
            WrapMode::ViewportWrap => "Viewport Wrap",
            WrapMode::RulerWrap => "Ruler Wrap",
        }
    }
}

/// Window state for persistence across sessions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowState {
    /// Window inner size (width, height) in logical points.
    pub size: Option<[f32; 2]>,
    /// Window outer position (x, y) in logical points.
    pub position: Option<[f32; 2]>,
    /// Whether the window is in fullscreen mode.
    pub fullscreen: Option<bool>,
    /// Whether the window is maximized.
    pub maximized: Option<bool>,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            size: None,
            position: None,
            fullscreen: None,
            maximized: None,
        }
    }
}

/// Font family selection for the editor.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum FontFamily {
    /// Default monospace font for the platform.
    #[default]
    Default,
    /// Specific font family name.
    Named(String),
}

impl FontFamily {
    /// Convert to egui font family.
    pub fn to_egui(&self) -> egui::FontFamily {
        match self {
            FontFamily::Default => egui::FontFamily::Monospace,
            FontFamily::Named(name) => egui::FontFamily::Name(std::sync::Arc::from(name.as_str())),
        }
    }
}

/// Apply font settings to the egui context.
pub fn apply_font_settings(ctx: &egui::Context, font_family: &FontFamily, font_size: f32, line_height_scale: f32) {
    ctx.style_mut(|style| {
        // Update the monospace text style with the configured font family and size
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(font_size, font_family.to_egui()),
        );

        // Apply line height scale by adjusting the spacing
        style.spacing.item_spacing.y = style.spacing.item_spacing.y * line_height_scale;
    });
}

/// Application-wide settings that persist separately from session state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// Active theme.
    pub theme: Theme,
    /// Line wrapping mode.
    pub wrap_mode: WrapMode,
    /// Ruler column positions for ruler wrap and visual indicators.
    pub rulers: Vec<usize>,
    /// Show visible whitespace characters (spaces as middle dots, tabs as arrows).
    pub show_visible_whitespace: bool,
    /// Show indentation guides at multiples of tab width.
    pub show_indentation_guides: bool,
    /// Show minimap with viewport indicator.
    pub show_minimap: bool,
    /// Show status bar at the bottom of the window.
    pub show_status_bar: bool,
    /// Font family for the editor.
    pub font_family: FontFamily,
    /// Font size in points.
    pub font_size: f32,
    /// Line height scale factor (1.0 = normal).
    pub line_height_scale: f32,
    /// Default tab width in spaces for new documents.
    pub default_tab_width: usize,
    /// Default soft tabs setting for new documents.
    pub default_soft_tabs: bool,
    /// Ignored grammar/language names for content detection.
    pub ignored_languages: Vec<String>,
    /// Window state for restore on startup.
    pub window_state: WindowState,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            wrap_mode: WrapMode::default(),
            rulers: vec![80],
            show_visible_whitespace: false,
            show_indentation_guides: true,
            show_minimap: false,
            show_status_bar: true,
            font_family: FontFamily::default(),
            font_size: 14.0,
            line_height_scale: 1.0,
            default_tab_width: 4,
            default_soft_tabs: true,
            ignored_languages: Vec::new(),
            window_state: WindowState::default(),
        }
    }
}
