use crop::Rope;
use eframe::egui;

/// Cached text layout metrics for the editor.
///
/// This struct encapsulates all measurements needed to render editor content
/// without recalculating font metrics on every frame. It provides stable line
/// heights and fast viewport-based lookups.
pub struct TextLayoutPipeline {
    pub row_height: f32,
    pub char_width: f32,
    pub font_id: egui::FontId,
    pub gutter_width: f32,
    pub text_origin_x: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub line_count: usize,
}

impl TextLayoutPipeline {
    /// Build a new layout pipeline from the current UI state and document.
    pub fn new(
        ui: &egui::Ui,
        rope: &Rope,
        available_width: f32,
        available_height: f32,
    ) -> Self {
        let line_count = visual_line_count(rope);
        let line_digits = decimal_digits(line_count);
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let font_id = egui::TextStyle::Monospace.resolve(ui.style());
        let char_width = monospace_char_width(ui, font_id.clone());
        let gutter_width =
            (line_digits as f32 * 8.0 + super::LINE_GUTTER_PADDING * 2.0).max(super::LINE_GUTTER_MIN_WIDTH);
        let text_origin_x = gutter_width + super::LINE_GUTTER_PADDING;
        let content_width = available_width.max(text_origin_x + super::EDITOR_MIN_WIDTH);
        let content_height = (line_count as f32 * row_height).max(available_height);

        Self {
            row_height,
            char_width,
            font_id,
            gutter_width,
            text_origin_x,
            content_width,
            content_height,
            line_count,
        }
    }

    /// Return the vertical range of visible line indices for a given viewport.
    pub fn visible_line_range(&self, viewport: &egui::Rect) -> (usize, usize) {
        let first_line = (viewport.min.y / self.row_height).floor().max(0.0) as usize;
        let last_line = ((viewport.max.y / self.row_height).ceil() as usize + 1).min(self.line_count);
        (first_line, last_line)
    }

    /// Return the number of fully visible rows in the viewport.
    pub fn visible_row_count(&self, viewport: &egui::Rect) -> usize {
        ((viewport.max.y - viewport.min.y) / self.row_height)
            .floor()
            .max(1.0) as usize
    }

    /// Compute the Y coordinate for a given line index.
    #[inline]
    pub fn line_y(&self, line_index: usize, content_top: f32) -> f32 {
        content_top + line_index as f32 * self.row_height
    }

    /// Compute the X coordinate for a given column.
    #[inline]
    pub fn column_x(&self, column: usize) -> f32 {
        self.text_origin_x + column as f32 * self.char_width
    }

    /// Convert a byte offset to a caret position on screen.
    pub fn caret_position(
        &self,
        rope: &Rope,
        offset: usize,
        content_top: f32,
    ) -> egui::Pos2 {
        let line = line_index_of_byte(rope, offset);
        let column = column_of_byte(rope, offset);
        egui::pos2(self.column_x(column), self.line_y(line, content_top))
    }

    /// Convert a pointer position to a byte offset in the rope.
    pub fn offset_at_pointer(
        &self,
        rope: &Rope,
        pos: egui::Pos2,
        rect: egui::Rect,
    ) -> usize {
        let line = ((pos.y - rect.top()).max(0.0) / self.row_height) as usize;
        let line = line.min(self.line_count.saturating_sub(1));
        let column = ((pos.x - (rect.left() + self.text_origin_x)) / self.char_width)
            .round()
            .max(0.0) as usize;
        byte_for_line_column(rope, line, column)
    }

    /// Return the total content size as a `egui::Vec2`.
    pub fn content_size(&self) -> egui::Vec2 {
        egui::vec2(self.content_width, self.content_height)
    }
}

// Re-export helpers from geometry so the pipeline is self-contained for callers
use super::{
    byte_for_line_column, column_of_byte, decimal_digits, line_index_of_byte,
    monospace_char_width, visual_line_count,
};
