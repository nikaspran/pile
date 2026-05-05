use crop::Rope;
use eframe::egui;
use unicode_segmentation::UnicodeSegmentation;

use crate::model::{Document, Selection};

use super::SearchHighlight;

pub fn set_primary_selection(document: &mut Document, selection: Selection) {
    let selection = clamp_selection_to_rope(&document.rope, selection);
    if let Some(primary) = document.selections.first_mut() {
        *primary = selection;
    } else {
        document.selections.push(selection);
    }
}

pub fn primary_selection(document: &Document) -> Selection {
    document
        .selections
        .first()
        .copied()
        .unwrap_or_else(|| Selection::caret(0))
}

pub fn visual_line_count(rope: &Rope) -> usize {
    let base = rope.line_len().max(1);
    if has_trailing_newline(rope) {
        base + 1
    } else {
        base
    }
}

pub fn decimal_digits(value: usize) -> usize {
    value
        .checked_ilog10()
        .map_or(1, |digits| digits as usize + 1)
}

pub(super) fn clamp_primary_selection(document: &mut Document) {
    let selection = primary_selection(document);
    set_primary_selection(document, selection);
}

pub(super) fn clamp_selection_to_rope(rope: &Rope, selection: Selection) -> Selection {
    Selection {
        anchor: clamp_to_char_boundary(rope, selection.anchor.min(rope.byte_len())),
        head: clamp_to_char_boundary(rope, selection.head.min(rope.byte_len())),
    }
}

pub(super) fn clamp_to_char_boundary(rope: &Rope, mut offset: usize) -> usize {
    while offset > 0 && !rope.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub fn selection_range(selection: Selection) -> (usize, usize) {
    if selection.anchor <= selection.head {
        (selection.anchor, selection.head)
    } else {
        (selection.head, selection.anchor)
    }
}

pub fn word_at_selection(rope: &Rope, selection: Selection) -> Option<(usize, usize)> {
    let (start, end) = selection_range(selection);
    if start != end {
        return Some((start, end));
    }

    let offset = start;
    if offset >= rope.byte_len() {
        return None;
    }

    let char_at_caret = rope.byte_slice(offset..).chars().next();
    let Some(char_at_caret) = char_at_caret else {
        return None;
    };

    if classify_char(char_at_caret) == CharClass::NonWord {
        return None;
    }

    let mut word_start = offset;
    let mut search_offset = offset;
    loop {
        if search_offset == 0 {
            break;
        }
        let prev_char = rope.byte_slice(..search_offset).chars().next_back();
        match prev_char {
            Some(c) if classify_char(c) == CharClass::Word => {
                search_offset -= c.len_utf8();
                word_start = search_offset;
            }
            _ => break,
        }
    }

    let mut word_end = offset;
    let mut chars_after = rope.byte_slice(offset..).chars();
    if let Some(c) = chars_after.next() {
        word_end += c.len_utf8();
    }
    for c in chars_after {
        if classify_char(c) == CharClass::Word {
            word_end += c.len_utf8();
        } else {
            break;
        }
    }

    if word_start < word_end {
        Some((word_start, word_end))
    } else {
        None
    }
}

pub(super) fn previous_grapheme_boundary(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let offset = offset.min(rope.byte_len());
    let prefix = rope.byte_slice(..offset).to_string();
    prefix
        .graphemes(true)
        .last()
        .map_or(0, |g| offset - g.len())
}

pub(super) fn next_grapheme_boundary(rope: &Rope, offset: usize) -> usize {
    let offset = offset.min(rope.byte_len());
    if offset >= rope.byte_len() {
        return rope.byte_len();
    }
    let suffix = rope.byte_slice(offset..).to_string();
    suffix
        .graphemes(true)
        .next()
        .map_or(rope.byte_len(), |g| offset + g.len())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CharClass {
    Word,
    NonWord,
}

fn classify_char(c: char) -> CharClass {
    if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::NonWord
    }
}

pub(super) fn next_word_boundary(rope: &Rope, start: usize) -> usize {
    let total = rope.byte_len();
    let mut offset = clamp_to_char_boundary(rope, start.min(total));
    if offset >= total {
        return total;
    }

    let mut chars = rope.byte_slice(offset..).chars();
    let lead = loop {
        match chars.next() {
            Some(c) if c.is_whitespace() => offset += c.len_utf8(),
            Some(c) => break c,
            None => return total,
        }
    };
    let class = classify_char(lead);
    offset += lead.len_utf8();
    for c in chars {
        if c.is_whitespace() || classify_char(c) != class {
            break;
        }
        offset += c.len_utf8();
    }
    offset
}

pub(super) fn previous_word_boundary(rope: &Rope, start: usize) -> usize {
    let mut offset = clamp_to_char_boundary(rope, start.min(rope.byte_len()));
    if offset == 0 {
        return 0;
    }

    let mut chars = rope.byte_slice(..offset).chars();
    let lead = loop {
        match chars.next_back() {
            Some(c) if c.is_whitespace() => offset -= c.len_utf8(),
            Some(c) => break c,
            None => return 0,
        }
    };
    let class = classify_char(lead);
    offset -= lead.len_utf8();
    while let Some(c) = chars.next_back() {
        if c.is_whitespace() || classify_char(c) != class {
            break;
        }
        offset -= c.len_utf8();
    }
    offset
}

pub(super) fn has_trailing_newline(rope: &Rope) -> bool {
    rope.byte_len() > 0 && rope.byte(rope.byte_len() - 1) == b'\n'
}

pub(super) fn byte_of_visual_line(rope: &Rope, line_index: usize) -> usize {
    if rope.line_len() == 0 || line_index >= rope.line_len() {
        rope.byte_len()
    } else {
        rope.byte_of_line(line_index)
    }
}

pub fn visual_line_bounds(rope: &Rope, line_index: usize) -> (usize, usize) {
    if rope.line_len() == 0 || line_index >= rope.line_len() {
        let end = rope.byte_len();
        return (end, end);
    }

    let start = rope.byte_of_line(line_index);
    let end = start + rope.line(line_index).byte_len();
    (start, end)
}

pub(super) fn visual_line_text(rope: &Rope, line_index: usize) -> String {
    if rope.line_len() == 0 || line_index >= rope.line_len() {
        String::new()
    } else {
        rope.line(line_index).to_string()
    }
}

/// Get the text for a wrapped line given the visual line map.
#[allow(dead_code)]
pub fn wrapped_line_text(
    rope: &Rope,
    visual_line_map: &[(usize, usize, usize)],
    wrapped_line_index: usize,
) -> String {
    let Some(&(doc_line, start_col, end_col)) = visual_line_map.get(wrapped_line_index) else {
        return String::new();
    };
    let line_text = visual_line_text(rope, doc_line);
    line_text
        .chars()
        .skip(start_col)
        .take(end_col - start_col)
        .collect()
}

pub(super) fn line_index_of_byte(rope: &Rope, offset: usize) -> usize {
    let offset = offset.min(rope.byte_len());
    let line = if rope.byte_len() == 0 {
        0
    } else {
        rope.line_of_byte(offset)
    };
    line.min(visual_line_count(rope).saturating_sub(1))
}

pub(super) fn column_of_byte(rope: &Rope, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(rope, offset.min(rope.byte_len()));
    let line = line_index_of_byte(rope, offset);
    let line_start = byte_of_visual_line(rope, line);
    rope.byte_slice(line_start..offset).chars().count()
}

pub(super) fn byte_for_line_column(rope: &Rope, line_index: usize, column: usize) -> usize {
    let (start, end) = visual_line_bounds(rope, line_index);
    let mut offset = start;
    for (current_column, char) in rope.byte_slice(start..end).chars().enumerate() {
        if current_column >= column {
            break;
        }
        offset += char.len_utf8();
    }
    offset
}

pub(super) fn monospace_char_width(ui: &egui::Ui, font_id: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap("m".to_owned(), font_id, ui.visuals().text_color())
        .size()
        .x
        .max(1.0)
}

pub(super) fn paint_selection_for_line(
    painter: &egui::Painter,
    document: &Document,
    selection: Selection,
    line_index: usize,
    text_pos: egui::Pos2,
    row_height: f32,
    char_width: f32,
    color: egui::Color32,
) {
    let (selection_start, selection_end) = selection_range(selection);
    if selection_start == selection_end {
        return;
    }

    let (line_start, line_end) = visual_line_bounds(&document.rope, line_index);
    let start = selection_start.max(line_start);
    let end = selection_end.min(line_end);
    if start >= end {
        return;
    }

    let start_column = document.rope.byte_slice(line_start..start).chars().count();
    let end_column = document.rope.byte_slice(line_start..end).chars().count();
    let min = egui::pos2(text_pos.x + start_column as f32 * char_width, text_pos.y);
    let max = egui::pos2(
        text_pos.x + end_column as f32 * char_width,
        text_pos.y + row_height,
    );
    let rect = egui::Rect::from_min_max(min, max);
    let rounding = egui::CornerRadius::same(2);
    painter.rect_filled(rect, rounding, color);
    painter.rect_stroke(rect, rounding, egui::Stroke::new(1.0, color.gamma_multiply(1.3)), egui::StrokeKind::Inside);
}

pub fn select_word_at_offset(rope: &Rope, offset: usize) -> Selection {
    let offset = clamp_to_char_boundary(rope, offset.min(rope.byte_len()));
    if offset >= rope.byte_len() {
        return Selection::caret(offset);
    }
    let char_at_caret = rope.byte_slice(offset..).chars().next();
    let Some(char_at_caret) = char_at_caret else {
        return Selection::caret(offset);
    };
    if classify_char(char_at_caret) == CharClass::NonWord {
        return Selection::caret(offset);
    }
    let mut word_start = offset;
    let mut search_offset = offset;
    loop {
        if search_offset == 0 {
            break;
        }
        let prev_char = rope.byte_slice(..search_offset).chars().next_back();
        match prev_char {
            Some(c) if classify_char(c) == CharClass::Word => {
                search_offset -= c.len_utf8();
                word_start = search_offset;
            }
            _ => break,
        }
    }
    let mut word_end = offset;
    let mut chars_after = rope.byte_slice(offset..).chars();
    if let Some(c) = chars_after.next() {
        word_end += c.len_utf8();
    }
    for c in chars_after {
        if classify_char(c) == CharClass::Word {
            word_end += c.len_utf8();
        } else {
            break;
        }
    }
    Selection {
        anchor: word_start,
        head: word_end,
    }
}

pub fn select_line_at_offset(rope: &Rope, offset: usize) -> Selection {
    let line_index = line_index_of_byte(rope, offset);
    let (line_start, line_end) = visual_line_bounds(rope, line_index);
    Selection {
        anchor: line_start,
        head: line_end,
    }
}

pub(super) fn paint_search_highlights_for_line(
    painter: &egui::Painter,
    rope: &Rope,
    highlights: &[SearchHighlight],
    line_index: usize,
    current_only: bool,
    text_pos: egui::Pos2,
    row_height: f32,
    char_width: f32,
    visuals: &egui::Visuals,
) {
    let normal_fill = visuals.warn_fg_color.gamma_multiply(0.30);
    let current_fill = visuals.warn_fg_color.gamma_multiply(0.55);
    let current_stroke = egui::Stroke::new(1.0, visuals.warn_fg_color);

    for highlight in highlights {
        if highlight.is_current != current_only {
            continue;
        }

        let Some(span) = highlight_columns_for_line(rope, *highlight, line_index) else {
            continue;
        };

        let min = egui::pos2(
            text_pos.x + span.start_column as f32 * char_width,
            text_pos.y,
        );
        let max = egui::pos2(
            text_pos.x + span.end_column as f32 * char_width,
            text_pos.y + row_height,
        );
        let rect = egui::Rect::from_min_max(min, max);
        if highlight.is_current {
            painter.rect_filled(rect, 0.0, current_fill);
            painter.rect_stroke(rect, 0.0, current_stroke, egui::StrokeKind::Inside);
        } else {
            painter.rect_filled(rect, 0.0, normal_fill);
        }
    }
}

/// Creates rectangular (column) selections between two column positions across lines.
/// This creates a separate selection for each line between the anchor line and current line.
pub fn create_column_selection(
    document: &mut Document,
    anchor_col: usize,
    current_col: usize,
    anchor_line: usize,
    current_line: usize,
) {
    let rope = &document.rope;
    let min_line = anchor_line.min(current_line);
    let max_line = anchor_line.max(current_line);
    let min_col = anchor_col.min(current_col);
    let max_col = anchor_col.max(current_col);

    let mut new_selections = Vec::new();

    for line in min_line..=max_line {
        let line_start = byte_of_visual_line(rope, line);
        let (_, line_end) = visual_line_bounds(rope, line);
        let line_text = rope.byte_slice(line_start..line_end).to_string();

        // Count characters to find byte positions for the column range
        let mut char_count = 0usize;
        let mut sel_start = line_start;
        let mut sel_end = line_start;

        for _char in line_text.chars() {
            if char_count == min_col {
                sel_start = line_start + line_text.chars().take(min_col).map(|c| c.len_utf8()).sum::<usize>();
            }
            if char_count == max_col {
                sel_end = line_start + line_text.chars().take(max_col).map(|c| c.len_utf8()).sum::<usize>();
                break;
            }
            char_count += 1;
        }
        if char_count < max_col {
            // If we didn't reach max_col, use end of line
            sel_end = line_end;
        }

        new_selections.push(Selection {
            anchor: sel_start,
            head: sel_end,
        });
    }

    if !new_selections.is_empty() {
        document.selections = new_selections;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HighlightColumns {
    pub(super) start_column: usize,
    pub(super) end_column: usize,
}

pub(super) fn highlight_columns_for_line(
    rope: &Rope,
    highlight: SearchHighlight,
    line_index: usize,
) -> Option<HighlightColumns> {
    if highlight.start >= highlight.end {
        return None;
    }

    let (line_start, line_end) = visual_line_bounds(rope, line_index);
    let start = highlight.start.max(line_start).min(line_end);
    let end = highlight.end.min(line_end).max(line_start);
    if start >= end {
        return None;
    }

    Some(HighlightColumns {
        start_column: rope.byte_slice(line_start..start).chars().count(),
        end_column: rope.byte_slice(line_start..end).chars().count(),
    })
}
