use serde::{Deserialize, Serialize};

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

    pub fn label(self) -> &'static str {
        match self {
            WrapMode::NoWrap => "No Wrap",
            WrapMode::ViewportWrap => "Viewport Wrap",
            WrapMode::RulerWrap => "Ruler Wrap",
        }
    }
}

/// Application-wide settings that persist separately from session state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// Line wrapping mode.
    pub wrap_mode: WrapMode,
    /// Ruler column positions for ruler wrap and visual indicators.
    pub rulers: Vec<usize>,
    /// Show visible whitespace characters (spaces as middle dots, tabs as arrows).
    pub show_visible_whitespace: bool,
    /// Show indentation guides at multiples of tab width.
    pub show_indentation_guides: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            wrap_mode: WrapMode::default(),
            rulers: vec![80],
            show_visible_whitespace: false,
            show_indentation_guides: true,
        }
    }
}
