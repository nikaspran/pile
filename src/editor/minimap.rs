use crop::Rope;
use eframe::egui;

use crate::settings::Theme;

/// Minimap rendering configuration.
pub struct MinimapConfig {
    pub width: f32,
    pub line_height: f32,
    pub viewport_color: egui::Color32,
    pub viewport_border: egui::Color32,
    pub background_color: egui::Color32,
}

impl MinimapConfig {
    pub fn new(theme: Theme) -> Self {
        Self {
            width: 80.0,
            line_height: 2.0,
            viewport_color: theme.minimap_viewport(),
            viewport_border: theme.minimap_viewport_border(),
            background_color: egui::Color32::from_rgba_premultiplied(0, 0, 0, 0),
        }
    }
}

impl Default for MinimapConfig {
    fn default() -> Self {
        Self::new(Theme::Dark)
    }
}

/// Result of minimap interaction.
pub struct MinimapResult {
    /// Whether the user interacted with the minimap.
    pub interacted: bool,
    /// The target scroll Y position if the user clicked/dragged.
    pub target_scroll_y: Option<f32>,
}

/// Computes the total height needed for the minimap.
pub fn minimap_total_height(line_count: usize, line_height: f32) -> f32 {
    line_count as f32 * line_height
}

/// Renders a minimap for the given document.
///
/// Returns a `MinimapResult` indicating if the user interacted with the minimap
/// and optionally the target scroll position.
pub fn show_minimap(
    ui: &mut egui::Ui,
    rope: &Rope,
    scroll_y: f32,
    viewport_height: f32,
    content_height: f32,
    config: &MinimapConfig,
    theme: Theme,
) -> MinimapResult {
    let line_count = visual_line_count(rope).max(1);
    let total_height = minimap_total_height(line_count, config.line_height);
    let available_height = ui.available_height();

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(config.width, available_height),
        egui::Sense::click_and_drag(),
    );

    let painter = ui.painter_at(rect);

    // Draw background
    painter.rect_filled(rect, 0.0, config.background_color);

    // Draw minimap content - show lines as colored blocks
    let text_color = theme.minimap_text();

    let comment_color = theme.minimap_comment();

    let keyword_color = theme.minimap_keyword();

    // Simple line rendering - iterate through wrapped lines
    for line_idx in 0..line_count {
        let y = rect.top() + line_idx as f32 * config.line_height;

        // Skip if line is outside visible minimap area
        if y + config.line_height < rect.top() || y > rect.bottom() {
            continue;
        }

        let line_text = visual_line_text(rope, line_idx);
        let line_text_str: String = line_text.chars().collect();
        let trimmed = line_text_str.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Choose color based on content
        let color =
            if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with("/*") {
                comment_color
            } else if trimmed.starts_with("fn")
                || trimmed.starts_with("func")
                || trimmed.starts_with("def")
                || trimmed.starts_with("struct")
                || trimmed.starts_with("class")
                || trimmed.starts_with("enum")
                || trimmed.starts_with("impl")
                || trimmed.starts_with("trait")
            {
                keyword_color
            } else {
                text_color
            };

        let line_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 2.0, y),
            egui::vec2(rect.width() - 4.0, config.line_height - 0.5),
        );
        painter.rect_filled(line_rect, 0.0, color);
    }

    // Draw viewport indicator
    if content_height > 0.0 {
        let viewport_top_ratio = (scroll_y / content_height).clamp(0.0, 1.0);
        let viewport_height_ratio = (viewport_height / content_height).min(1.0);

        let indicator_top = rect.top() + viewport_top_ratio * total_height;
        let indicator_height = (viewport_height_ratio * total_height).max(config.line_height * 3.0);

        let indicator_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left(), indicator_top),
            egui::vec2(rect.width(), indicator_height),
        );

        // Draw viewport indicator with border
        painter.rect(
            indicator_rect,
            0.0,
            config.viewport_color,
            egui::Stroke::new(1.0, config.viewport_border),
            egui::StrokeKind::Inside,
        );
    }

    // Handle click/drag to scroll
    let mut result = MinimapResult {
        interacted: false,
        target_scroll_y: None,
    };

    if response.clicked() || response.dragged() {
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            result.interacted = true;
            // Calculate the target scroll position based on click position
            // Center the viewport on the clicked position
            let click_ratio = ((pointer_pos.y - rect.top()) / total_height).clamp(0.0, 1.0);
            let target_scroll_y = click_ratio * content_height - viewport_height * 0.5;
            result.target_scroll_y = Some(target_scroll_y.max(0.0));
        }
    }

    result
}

/// Calculate the target scroll position from a click on the minimap.
#[allow(dead_code)]
pub fn minimap_scroll_target(
    click_y: f32,
    minimap_rect_top: f32,
    minimap_total_height: f32,
    content_height: f32,
) -> f32 {
    let click_ratio = ((click_y - minimap_rect_top) / minimap_total_height).clamp(0.0, 1.0);
    click_ratio * content_height
}

/// Check if a document has enough content to warrant showing a minimap.
pub fn should_show_minimap(rope: &Rope) -> bool {
    visual_line_count(rope) > 10
}

// Import helpers from geometry
use crate::editor::geometry::{visual_line_count, visual_line_text};
