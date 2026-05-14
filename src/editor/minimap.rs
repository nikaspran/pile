use crop::Rope;
use eframe::egui;

use crate::settings::Theme;

const MINIMAP_LINE_WIDTH_CAP_CHARS: usize = 120;
const MINIMAP_MIN_LINE_WIDTH: f32 = 4.0;

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
            line_height: 1.0,
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
    content_line_count: usize,
    config: &MinimapConfig,
    theme: Theme,
) -> MinimapResult {
    let line_count = content_line_count.max(1);
    let source_line_count = visual_line_count(rope).max(1);
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

    let visible_rows = ((rect.height() / config.line_height).ceil() as usize).max(1);
    let rendered_rows = line_count.min(visible_rows);
    let compressed = line_count > visible_rows;

    // Simple line rendering: draw one row per visual line when it fits, otherwise
    // sample the document into fixed-height rows so very large notes stay bounded.
    for row in 0..rendered_rows {
        let line_idx = if compressed {
            row.saturating_mul(line_count) / rendered_rows
        } else {
            row
        };
        let source_line_idx = line_idx.saturating_mul(source_line_count) / line_count;
        let y = if compressed {
            rect.top() + row as f32 * config.line_height
        } else {
            rect.top() + line_idx as f32 * config.line_height
        };

        let line_text = visual_line_text(rope, source_line_idx.min(source_line_count - 1));
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

        let line_width = minimap_line_width(rect.width() - 4.0, trimmed.chars().count());
        let line_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 2.0, y),
            egui::vec2(line_width, (config.line_height * 0.65).max(0.5)),
        );
        painter.rect_filled(line_rect, 0.0, color);
    }

    // Draw viewport indicator
    if let Some(indicator_rect) = minimap_viewport_rect(
        rect,
        scroll_y,
        viewport_height,
        content_height,
        config.line_height,
    ) {
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
            result.target_scroll_y = Some(minimap_scroll_target(
                pointer_pos.y,
                rect.top(),
                rect.height(),
                content_height,
                viewport_height,
            ));
        }
    }

    result
}

/// Calculate the target scroll position from a click on the minimap.
pub fn minimap_scroll_target(
    click_y: f32,
    minimap_rect_top: f32,
    minimap_height: f32,
    content_height: f32,
    viewport_height: f32,
) -> f32 {
    if minimap_height <= 0.0 || content_height <= viewport_height {
        return 0.0;
    }
    let click_ratio = ((click_y - minimap_rect_top) / minimap_height).clamp(0.0, 1.0);
    let target = click_ratio * content_height - viewport_height * 0.5;
    target.clamp(0.0, (content_height - viewport_height).max(0.0))
}

/// Calculate the viewport indicator rectangle within the minimap.
pub fn minimap_viewport_rect(
    minimap_rect: egui::Rect,
    scroll_y: f32,
    viewport_height: f32,
    content_height: f32,
    min_height: f32,
) -> Option<egui::Rect> {
    if minimap_rect.height() <= 0.0 || content_height <= 0.0 {
        return None;
    }

    let max_scroll_y = (content_height - viewport_height).max(0.0);
    let scroll_y = scroll_y.clamp(0.0, max_scroll_y);
    let viewport_ratio = (viewport_height / content_height).clamp(0.0, 1.0);
    let indicator_height = (viewport_ratio * minimap_rect.height())
        .max(min_height * 3.0)
        .min(minimap_rect.height());
    let travel = (minimap_rect.height() - indicator_height).max(0.0);
    let top_ratio = if max_scroll_y > 0.0 {
        scroll_y / max_scroll_y
    } else {
        0.0
    };
    let indicator_top = minimap_rect.top() + top_ratio * travel;

    Some(egui::Rect::from_min_size(
        egui::pos2(minimap_rect.left(), indicator_top),
        egui::vec2(minimap_rect.width(), indicator_height),
    ))
}

fn minimap_line_width(available_width: f32, char_count: usize) -> f32 {
    if available_width <= 0.0 || char_count == 0 {
        return 0.0;
    }

    let ratio = (char_count.min(MINIMAP_LINE_WIDTH_CAP_CHARS) as f32
        / MINIMAP_LINE_WIDTH_CAP_CHARS as f32)
        .clamp(0.0, 1.0);
    (available_width * ratio)
        .max(MINIMAP_MIN_LINE_WIDTH.min(available_width))
        .min(available_width)
}

/// Check if a document has enough content to warrant showing a minimap.
pub fn should_show_minimap(rope: &Rope) -> bool {
    visual_line_count(rope) > 10
}

// Import helpers from geometry
use crate::editor::geometry::{visual_line_count, visual_line_text};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_target_centers_click_and_clamps() {
        assert_eq!(minimap_scroll_target(0.0, 0.0, 100.0, 1000.0, 200.0), 0.0);
        assert_eq!(
            minimap_scroll_target(100.0, 0.0, 100.0, 1000.0, 200.0),
            800.0
        );
        assert_eq!(
            minimap_scroll_target(50.0, 0.0, 100.0, 1000.0, 200.0),
            400.0
        );
    }

    #[test]
    fn scroll_target_is_zero_when_content_fits() {
        assert_eq!(minimap_scroll_target(50.0, 0.0, 100.0, 200.0, 200.0), 0.0);
        assert_eq!(minimap_scroll_target(50.0, 0.0, 0.0, 1000.0, 200.0), 0.0);
    }

    #[test]
    fn viewport_rect_stays_inside_minimap() {
        let minimap_rect =
            egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(80.0, 100.0));

        let top = minimap_viewport_rect(minimap_rect, 0.0, 200.0, 1000.0, 2.0).unwrap();
        assert_eq!(top.top(), 20.0);
        assert_eq!(top.height(), 20.0);

        let bottom = minimap_viewport_rect(minimap_rect, 800.0, 200.0, 1000.0, 2.0).unwrap();
        assert_eq!(bottom.bottom(), 120.0);
        assert_eq!(bottom.height(), 20.0);
    }

    #[test]
    fn line_width_tracks_content_length_with_cap() {
        assert_eq!(minimap_line_width(80.0, 0), 0.0);
        assert_eq!(minimap_line_width(80.0, 6), 4.0);

        let medium = minimap_line_width(80.0, 60);
        assert_eq!(medium, 40.0);

        assert_eq!(minimap_line_width(80.0, 240), 80.0);
    }

    #[test]
    fn short_documents_do_not_show_minimap() {
        let short = Rope::from("one\ntwo\nthree");
        let long = Rope::from("1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11");

        assert!(!should_show_minimap(&short));
        assert!(should_show_minimap(&long));
    }
}
