use crop::{Rope, RopeSlice};
use eframe::egui;

/// Cached text layout metrics for the editor.
///
/// This struct encapsulates all measurements needed to render editor content
/// without recalculating font metrics on every frame. It provides stable line
/// heights and fast viewport-based lookups.
#[derive(Clone, Debug)]
pub struct TextLayoutPipeline {
    pub row_height: f32,
    pub char_width: f32,
    pub font_id: egui::FontId,
    /// Gutter width in points.
    #[allow(dead_code)]
    pub gutter_width: f32,
    pub text_origin_x: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub line_count: usize,
    wrap_mode: WrapMode,
    /// Width available for text wrapping, in characters.
    #[allow(dead_code)]
    wrap_width_chars: usize,
    visual_line_map: Vec<(usize, usize, usize)>,
}

/// Line wrapping mode (internal to layout).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WrapMode {
    NoWrap,
    ViewportWrap,
    RulerWrap,
}

impl TextLayoutPipeline {
    /// Build a new layout pipeline from the current UI state and document.
    pub fn new(
        ui: &egui::Ui,
        rope: &Rope,
        available_width: f32,
        available_height: f32,
        wrap_mode: crate::settings::WrapMode,
        rulers: &[usize],
        font_family: &crate::settings::FontFamily,
        font_size: f32,
        line_height_scale: f32,
    ) -> Self {
        let line_count = visual_line_count(rope);
        let line_digits = decimal_digits(line_count);
        let base_row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let font_id = egui::FontId::new(font_size, font_family.to_egui());
        let char_width = monospace_char_width(ui, font_id.clone());
        let row_height = base_row_height * line_height_scale;
        let gutter_width = (line_digits as f32 * 8.0 + super::LINE_GUTTER_PADDING * 2.0)
            .max(super::LINE_GUTTER_MIN_WIDTH);
        let text_origin_x = gutter_width + super::LINE_GUTTER_PADDING;

        let wrap_mode_enum = match wrap_mode {
            crate::settings::WrapMode::NoWrap => WrapMode::NoWrap,
            crate::settings::WrapMode::ViewportWrap => WrapMode::ViewportWrap,
            crate::settings::WrapMode::RulerWrap => WrapMode::RulerWrap,
        };

        let text_width = available_width - text_origin_x;
        let (wrap_width_chars, visual_line_map) =
            build_wrap_map(rope, wrap_mode_enum, char_width, text_width, rulers);
        let wrapped_line_count = if wrap_mode_enum == WrapMode::NoWrap {
            line_count
        } else {
            visual_line_map.len().max(1)
        };

        let content_width = match wrap_mode_enum {
            WrapMode::NoWrap => {
                let longest_line_width = longest_visual_line_chars(rope) as f32 * char_width;
                available_width
                    .max(text_origin_x + longest_line_width + super::LINE_GUTTER_PADDING)
                    .max(text_origin_x + super::EDITOR_MIN_WIDTH)
            }
            WrapMode::ViewportWrap | WrapMode::RulerWrap => {
                let wrap_px = wrap_width_chars as f32 * char_width;
                (text_origin_x + wrap_px).max(text_origin_x + super::EDITOR_MIN_WIDTH)
            }
        };
        let content_height = (wrapped_line_count as f32 * row_height).max(available_height);

        Self {
            row_height,
            char_width,
            font_id,
            gutter_width,
            text_origin_x,
            content_width,
            content_height,
            line_count: wrapped_line_count,
            wrap_mode: wrap_mode_enum,
            wrap_width_chars,
            visual_line_map,
        }
    }

    /// Create a TextLayoutPipeline for testing purposes.
    /// This bypasses the normal construction that requires UI context.
    #[cfg(test)]
    pub fn for_test(
        row_height: f32,
        char_width: f32,
        font_id: egui::FontId,
        gutter_width: f32,
        text_origin_x: f32,
        content_width: f32,
        content_height: f32,
        line_count: usize,
    ) -> Self {
        Self {
            row_height,
            char_width,
            font_id,
            gutter_width,
            text_origin_x,
            content_width,
            content_height,
            line_count,
            wrap_mode: WrapMode::NoWrap,
            wrap_width_chars: usize::MAX,
            visual_line_map: vec![],
        }
    }

    /// Return the vertical range of visible line indices for a given viewport.
    pub fn visible_line_range(&self, viewport: &egui::Rect) -> (usize, usize) {
        let first_line = (viewport.min.y / self.row_height).floor().max(0.0) as usize;
        let last_line =
            ((viewport.max.y / self.row_height).ceil() as usize + 1).min(self.line_count);
        (first_line, last_line)
    }

    /// Return the number of fully visible rows in the viewport.
    pub fn visible_row_count(&self, viewport: &egui::Rect) -> usize {
        ((viewport.max.y - viewport.min.y) / self.row_height)
            .floor()
            .max(1.0) as usize
    }

    /// Compute the Y coordinate for a given wrapped line index.
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
    pub fn caret_position(&self, rope: &Rope, offset: usize, content_top: f32) -> egui::Pos2 {
        let (wrapped_line, column) = if self.wrap_mode != WrapMode::NoWrap {
            wrapped_line_and_column(rope, offset, &self.visual_line_map)
        } else {
            let line = line_index_of_byte(rope, offset);
            let col = column_of_byte(rope, offset);
            (line, col)
        };
        egui::pos2(
            self.column_x(column),
            self.line_y(wrapped_line, content_top),
        )
    }

    /// Convert a pointer position to a byte offset in the rope.
    pub fn offset_at_pointer(&self, rope: &Rope, pos: egui::Pos2, rect: egui::Rect) -> usize {
        if self.wrap_mode == WrapMode::NoWrap {
            let line = ((pos.y - rect.top()).max(0.0) / self.row_height) as usize;
            let line = line.min(self.line_count.saturating_sub(1));
            let column = ((pos.x - (rect.left() + self.text_origin_x)) / self.char_width)
                .round()
                .max(0.0) as usize;
            byte_for_line_column(rope, line, column)
        } else {
            let wrapped_line = ((pos.y - rect.top()).max(0.0) / self.row_height) as usize;
            let wrapped_line = wrapped_line.min(self.line_count.saturating_sub(1));
            let column = ((pos.x - (rect.left() + self.text_origin_x)) / self.char_width)
                .round()
                .max(0.0) as usize;
            byte_for_wrapped_line_column(rope, wrapped_line, column, &self.visual_line_map)
        }
    }

    /// Return the total content size as a `egui::Vec2`.
    pub fn content_size(&self) -> egui::Vec2 {
        egui::vec2(self.content_width, self.content_height)
    }

    /// Get the text for a wrapped line.
    pub fn wrapped_line_text<'a>(
        &self,
        rope: &'a Rope,
        wrapped_line_index: usize,
    ) -> RopeSlice<'a> {
        if self.wrap_mode == WrapMode::NoWrap {
            visual_line_text(rope, wrapped_line_index)
        } else {
            let Some(&(doc_line, start_col, end_col)) =
                self.visual_line_map.get(wrapped_line_index)
            else {
                return rope.byte_slice(..0);
            };
            let line_slice = visual_line_text(rope, doc_line);
            let start_byte = char_index_to_byte_offset(line_slice, start_col);
            let end_byte = char_index_to_byte_offset(line_slice, end_col);
            line_slice.byte_slice(start_byte..end_byte)
        }
    }

    /// Get the byte offset where a wrapped line starts in the original rope.
    pub fn wrapped_line_byte_start(&self, rope: &Rope, wrapped_line_index: usize) -> usize {
        if self.wrap_mode == WrapMode::NoWrap {
            byte_of_visual_line(rope, wrapped_line_index)
        } else {
            let Some(&(doc_line, start_col, _)) = self.visual_line_map.get(wrapped_line_index)
            else {
                return 0;
            };
            let line_start = byte_of_visual_line(rope, doc_line);
            let line_slice = visual_line_text(rope, doc_line);
            let byte_offset_within_line = char_index_to_byte_offset(line_slice, start_col);
            line_start + byte_offset_within_line
        }
    }
}

fn build_wrap_map(
    rope: &Rope,
    wrap_mode: WrapMode,
    char_width: f32,
    available_text_width: f32,
    rulers: &[usize],
) -> (usize, Vec<(usize, usize, usize)>) {
    if wrap_mode == WrapMode::NoWrap {
        return (usize::MAX, vec![]);
    }

    let wrap_cols = match wrap_mode {
        WrapMode::NoWrap => usize::MAX,
        WrapMode::ViewportWrap => {
            let cols = (available_text_width / char_width).floor().max(1.0) as usize;
            cols
        }
        WrapMode::RulerWrap => rulers.first().copied().unwrap_or(80),
    };

    let mut map = Vec::new();
    for doc_line in 0..visual_line_count(rope) {
        let line_slice = visual_line_text(rope, doc_line);
        let line_len_chars = line_slice.chars().count();

        if line_len_chars == 0 {
            map.push((doc_line, 0, 0));
        } else if line_len_chars <= wrap_cols {
            map.push((doc_line, 0, line_len_chars));
        } else {
            let mut col = 0;
            while col < line_len_chars {
                let end = (col + wrap_cols).min(line_len_chars);
                map.push((doc_line, col, end));
                col = end;
            }
        }
    }

    (wrap_cols, map)
}

pub(crate) fn longest_visual_line_chars(rope: &Rope) -> usize {
    (0..visual_line_count(rope))
        .map(|line| visual_line_text(rope, line).chars().count())
        .max()
        .unwrap_or(0)
}

fn wrapped_line_and_column(
    rope: &Rope,
    offset: usize,
    visual_line_map: &[(usize, usize, usize)],
) -> (usize, usize) {
    let doc_line = line_index_of_byte(rope, offset);
    let doc_col = column_of_byte(rope, offset);

    for (i, &(line, start_col, _)) in visual_line_map.iter().enumerate() {
        if line == doc_line && doc_col >= start_col {
            return (i, doc_col - start_col);
        }
    }

    (visual_line_map.len().saturating_sub(1), 0)
}

fn byte_for_wrapped_line_column(
    rope: &Rope,
    wrapped_line: usize,
    column: usize,
    visual_line_map: &[(usize, usize, usize)],
) -> usize {
    let Some(&(doc_line, start_col, _)) = visual_line_map.get(wrapped_line) else {
        return rope.byte_len();
    };

    let target_col = start_col + column;
    let line_start_byte = byte_of_visual_line(rope, doc_line);
    let line_slice = visual_line_text(rope, doc_line);
    let mut byte_offset = line_start_byte;
    let mut char_count = 0usize;

    for c in line_slice.chars() {
        if char_count >= target_col {
            break;
        }
        byte_offset += c.len_utf8();
        char_count += 1;
    }

    byte_offset
}

// Re-export helpers from geometry so the pipeline is self-contained for callers
use super::{
    byte_for_line_column, byte_of_visual_line, char_index_to_byte_offset, column_of_byte,
    decimal_digits, line_index_of_byte, monospace_char_width, visual_line_count, visual_line_text,
};
