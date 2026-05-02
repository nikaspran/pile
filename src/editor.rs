use crop::Rope;
use eframe::egui;

use crate::model::{Document, Selection};

const LINE_GUTTER_MIN_WIDTH: f32 = 44.0;
const LINE_GUTTER_PADDING: f32 = 10.0;
const EDITOR_MIN_WIDTH: f32 = 320.0;

#[derive(Debug, Default)]
pub struct EditorViewState {
    preferred_column: Option<usize>,
}

#[derive(Debug)]
pub struct EditorResponse {
    pub changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchHighlight {
    pub start: usize,
    pub end: usize,
    pub is_current: bool,
}

pub fn show_editor(
    ui: &mut egui::Ui,
    document: &mut Document,
    view_state: &mut EditorViewState,
    focus_pending: &mut bool,
    reveal_selection: Option<Selection>,
    search_highlights: &[SearchHighlight],
) -> EditorResponse {
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    clamp_primary_selection(document);

    if let Some(selection) = reveal_selection {
        set_primary_selection(document, selection);
        *focus_pending = true;
    }

    let line_count = visual_line_count(&document.rope);
    let line_digits = decimal_digits(line_count);
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let gutter_width =
        (line_digits as f32 * 8.0 + LINE_GUTTER_PADDING * 2.0).max(LINE_GUTTER_MIN_WIDTH);
    let available_width = ui.available_width().max(EDITOR_MIN_WIDTH);
    let text_origin_x = gutter_width + LINE_GUTTER_PADDING;
    let content_width = available_width.max(text_origin_x + EDITOR_MIN_WIDTH);
    let content_height = (line_count as f32 * row_height).max(ui.available_height());
    let font_id = egui::TextStyle::Monospace.resolve(ui.style());
    let char_width = monospace_char_width(ui, font_id.clone());

    let mut changed = false;

    egui::ScrollArea::both()
        .id_salt("editor-scroll")
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(content_width, content_height),
                egui::Sense::click(),
            );

            if *focus_pending {
                response.request_focus();
                *focus_pending = false;
            }
            if response.clicked() {
                response.request_focus();
                if let Some(pointer_position) = response.interact_pointer_pos() {
                    let line = ((pointer_position.y - rect.top()).max(0.0) / row_height) as usize;
                    let line = line.min(line_count.saturating_sub(1));
                    let column = ((pointer_position.x - (rect.left() + text_origin_x)) / char_width)
                        .round()
                        .max(0.0) as usize;
                    let offset = byte_for_line_column(&document.rope, line, column);
                    set_primary_selection(document, Selection::caret(offset));
                    view_state.preferred_column = None;
                }
            }

            if response.has_focus() {
                changed |= handle_input(ui, document, view_state);
                if changed {
                    ui.memory_mut(|memory| memory.request_focus(response.id));
                }
            }

            let painter = ui.painter_at(rect);
            let first_line = (viewport.min.y / row_height).floor().max(0.0) as usize;
            let last_line = ((viewport.max.y / row_height).ceil() as usize + 1).min(line_count);
            let selection = primary_selection(document);
            let caret_line = line_index_of_byte(&document.rope, selection.head);

            for line_index in first_line..last_line {
                let y = rect.top() + line_index as f32 * row_height;
                let row_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), y),
                    egui::vec2(content_width, row_height),
                );

                if line_index == caret_line {
                    painter.rect_filled(
                        row_rect,
                        0.0,
                        ui.visuals().selection.bg_fill.gamma_multiply(0.25),
                    );
                }

                let line_number_pos = egui::pos2(rect.left() + LINE_GUTTER_PADDING, y);
                painter.text(
                    line_number_pos,
                    egui::Align2::LEFT_TOP,
                    (line_index + 1).to_string(),
                    font_id.clone(),
                    ui.visuals().weak_text_color(),
                );

                let text_pos = egui::pos2(rect.left() + text_origin_x, y);
                paint_search_highlights_for_line(
                    &painter,
                    &document.rope,
                    search_highlights,
                    line_index,
                    false,
                    text_pos,
                    row_height,
                    char_width,
                    ui.visuals(),
                );
                paint_selection_for_line(
                    &painter,
                    document,
                    selection,
                    line_index,
                    text_pos,
                    row_height,
                    char_width,
                    ui.visuals().selection.bg_fill,
                );
                paint_search_highlights_for_line(
                    &painter,
                    &document.rope,
                    search_highlights,
                    line_index,
                    true,
                    text_pos,
                    row_height,
                    char_width,
                    ui.visuals(),
                );

                let line_text = visual_line_text(&document.rope, line_index);
                painter.text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    line_text,
                    font_id.clone(),
                    ui.visuals().text_color(),
                );
            }

            let caret_position = caret_position(
                &document.rope,
                selection.head,
                rect.left() + text_origin_x,
                rect.top(),
                row_height,
                char_width,
            );
            let current_caret_rect =
                egui::Rect::from_min_size(caret_position, egui::vec2(1.0, row_height));

            if reveal_selection.is_some() {
                ui.scroll_to_rect(current_caret_rect.expand(24.0), Some(egui::Align::Center));
            }

            if response.has_focus() {
                painter.line_segment(
                    [
                        current_caret_rect.left_top(),
                        current_caret_rect.left_bottom(),
                    ],
                    egui::Stroke::new(1.5, ui.visuals().text_color()),
                );
            }
        });

    EditorResponse { changed }
}

fn handle_input(ui: &egui::Ui, document: &mut Document, view_state: &mut EditorViewState) -> bool {
    let events = ui.input(|input| input.events.clone());
    let mut changed = false;

    for event in events {
        match event {
            egui::Event::Paste(text) if !text.is_empty() => {
                changed |= replace_selection_with(document, &text);
                view_state.preferred_column = None;
            }
            egui::Event::Text(text) if !text.is_empty() && text != "\n" && text != "\r" => {
                changed |= replace_selection_with(document, &text);
                view_state.preferred_column = None;
            }
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.command && !modifiers.ctrl && !modifiers.alt => match key {
                egui::Key::Backspace => {
                    changed |= backspace(document);
                    view_state.preferred_column = None;
                }
                egui::Key::Delete => {
                    changed |= delete(document);
                    view_state.preferred_column = None;
                }
                egui::Key::Enter => {
                    changed |= replace_selection_with(document, "\n");
                    view_state.preferred_column = None;
                }
                egui::Key::ArrowLeft => {
                    move_left(document);
                    view_state.preferred_column = None;
                }
                egui::Key::ArrowRight => {
                    move_right(document);
                    view_state.preferred_column = None;
                }
                egui::Key::ArrowUp => {
                    move_vertical(document, view_state, -1);
                }
                egui::Key::ArrowDown => {
                    move_vertical(document, view_state, 1);
                }
                egui::Key::Home => {
                    move_home(document);
                    view_state.preferred_column = None;
                }
                egui::Key::End => {
                    move_end(document);
                    view_state.preferred_column = None;
                }
                _ => {}
            },
            _ => {}
        }
    }

    changed
}

pub fn replace_selection_with(document: &mut Document, text: &str) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    if start == end && text.is_empty() {
        return false;
    }

    if start != end {
        document.rope.delete(start..end);
    }
    if !text.is_empty() {
        document.rope.insert(start, text);
    }

    let caret = start + text.len();
    set_primary_selection(document, Selection::caret(caret));
    document.revision += 1;
    true
}

pub fn backspace(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);
    if start != end {
        return replace_selection_with(document, "");
    }
    if start == 0 {
        return false;
    }

    let delete_start = previous_char_boundary(&document.rope, start);
    document.rope.delete(delete_start..start);
    set_primary_selection(document, Selection::caret(delete_start));
    document.revision += 1;
    true
}

pub fn delete(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);
    if start != end {
        return replace_selection_with(document, "");
    }
    if start >= document.rope.byte_len() {
        return false;
    }

    let delete_end = next_char_boundary(&document.rope, start);
    document.rope.delete(start..delete_end);
    set_primary_selection(document, Selection::caret(start));
    document.revision += 1;
    true
}

pub fn move_left(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let offset = if selection.anchor != selection.head {
        selection_range(selection).0
    } else {
        previous_char_boundary(&document.rope, selection.head)
    };
    set_primary_selection(document, Selection::caret(offset));
}

pub fn move_right(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let offset = if selection.anchor != selection.head {
        selection_range(selection).1
    } else {
        next_char_boundary(&document.rope, selection.head)
    };
    set_primary_selection(document, Selection::caret(offset));
}

pub fn move_home(document: &mut Document) {
    clamp_primary_selection(document);
    let line = line_index_of_byte(&document.rope, primary_selection(document).head);
    set_primary_selection(
        document,
        Selection::caret(byte_of_visual_line(&document.rope, line)),
    );
}

pub fn move_end(document: &mut Document) {
    clamp_primary_selection(document);
    let line = line_index_of_byte(&document.rope, primary_selection(document).head);
    let (_, end) = visual_line_bounds(&document.rope, line);
    set_primary_selection(document, Selection::caret(end));
}

pub fn move_vertical(document: &mut Document, view_state: &mut EditorViewState, delta: isize) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let current_line = line_index_of_byte(&document.rope, selection.head);
    let line_count = visual_line_count(&document.rope);
    let target_line = (current_line as isize + delta).clamp(0, line_count as isize - 1) as usize;
    let column = view_state
        .preferred_column
        .unwrap_or_else(|| column_of_byte(&document.rope, selection.head));
    view_state.preferred_column = Some(column);
    let target = byte_for_line_column(&document.rope, target_line, column);
    set_primary_selection(document, Selection::caret(target));
}

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

fn clamp_primary_selection(document: &mut Document) {
    let selection = primary_selection(document);
    set_primary_selection(document, selection);
}

fn clamp_selection_to_rope(rope: &Rope, selection: Selection) -> Selection {
    Selection {
        anchor: clamp_to_char_boundary(rope, selection.anchor.min(rope.byte_len())),
        head: clamp_to_char_boundary(rope, selection.head.min(rope.byte_len())),
    }
}

fn clamp_to_char_boundary(rope: &Rope, mut offset: usize) -> usize {
    while offset > 0 && !rope.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn selection_range(selection: Selection) -> (usize, usize) {
    if selection.anchor <= selection.head {
        (selection.anchor, selection.head)
    } else {
        (selection.head, selection.anchor)
    }
}

fn previous_char_boundary(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let offset = clamp_to_char_boundary(rope, offset.min(rope.byte_len()));
    rope.byte_slice(..offset)
        .chars()
        .next_back()
        .map_or(0, |char| offset - char.len_utf8())
}

fn next_char_boundary(rope: &Rope, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(rope, offset.min(rope.byte_len()));
    if offset >= rope.byte_len() {
        return rope.byte_len();
    }
    rope.byte_slice(offset..)
        .chars()
        .next()
        .map_or(rope.byte_len(), |char| offset + char.len_utf8())
}

fn has_trailing_newline(rope: &Rope) -> bool {
    rope.byte_len() > 0 && rope.byte(rope.byte_len() - 1) == b'\n'
}

fn byte_of_visual_line(rope: &Rope, line_index: usize) -> usize {
    if rope.line_len() == 0 || line_index >= rope.line_len() {
        rope.byte_len()
    } else {
        rope.byte_of_line(line_index)
    }
}

fn visual_line_bounds(rope: &Rope, line_index: usize) -> (usize, usize) {
    if rope.line_len() == 0 || line_index >= rope.line_len() {
        let end = rope.byte_len();
        return (end, end);
    }

    let start = rope.byte_of_line(line_index);
    let end = start + rope.line(line_index).byte_len();
    (start, end)
}

fn visual_line_text(rope: &Rope, line_index: usize) -> String {
    if rope.line_len() == 0 || line_index >= rope.line_len() {
        String::new()
    } else {
        rope.line(line_index).to_string()
    }
}

fn line_index_of_byte(rope: &Rope, offset: usize) -> usize {
    let offset = offset.min(rope.byte_len());
    let line = if rope.byte_len() == 0 {
        0
    } else {
        rope.line_of_byte(offset)
    };
    line.min(visual_line_count(rope).saturating_sub(1))
}

fn column_of_byte(rope: &Rope, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(rope, offset.min(rope.byte_len()));
    let line = line_index_of_byte(rope, offset);
    let line_start = byte_of_visual_line(rope, line);
    rope.byte_slice(line_start..offset).chars().count()
}

fn byte_for_line_column(rope: &Rope, line_index: usize, column: usize) -> usize {
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

fn caret_position(
    rope: &Rope,
    offset: usize,
    text_left: f32,
    content_top: f32,
    row_height: f32,
    char_width: f32,
) -> egui::Pos2 {
    let line = line_index_of_byte(rope, offset);
    let column = column_of_byte(rope, offset);
    egui::pos2(
        text_left + column as f32 * char_width,
        content_top + line as f32 * row_height,
    )
}

fn monospace_char_width(ui: &egui::Ui, font_id: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap("m".to_owned(), font_id, ui.visuals().text_color())
        .size()
        .x
        .max(1.0)
}

fn paint_selection_for_line(
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
    painter.rect_filled(egui::Rect::from_min_max(min, max), 0.0, color);
}

fn paint_search_highlights_for_line(
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HighlightColumns {
    start_column: usize,
    end_column: usize,
}

fn highlight_columns_for_line(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        let mut document = Document::new_untitled(1);
        document.replace_text(text);
        document.selections = vec![Selection::caret(0)];
        document.revision = 0;
        document
    }

    #[test]
    fn typing_and_paste_insert_at_caret() {
        let mut document = document("");

        assert!(replace_selection_with(&mut document, "hi"));
        assert_eq!(document.text(), "hi");
        assert_eq!(primary_selection(&document), Selection::caret(2));

        assert!(replace_selection_with(&mut document, "\nthere"));
        assert_eq!(document.text(), "hi\nthere");
        assert_eq!(primary_selection(&document), Selection::caret(8));
    }

    #[test]
    fn typing_replaces_selected_range() {
        let mut document = document("hello world");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 6,
                head: 11,
            },
        );

        assert!(replace_selection_with(&mut document, "pile"));

        assert_eq!(document.text(), "hello pile");
        assert_eq!(primary_selection(&document), Selection::caret(10));
    }

    #[test]
    fn backspace_and_delete_handle_boundaries_and_lines() {
        let mut document = document("ab\ncd");
        set_primary_selection(&mut document, Selection::caret(3));

        assert!(backspace(&mut document));
        assert_eq!(document.text(), "abcd");
        assert_eq!(primary_selection(&document), Selection::caret(2));

        assert!(delete(&mut document));
        assert_eq!(document.text(), "abd");
        assert_eq!(primary_selection(&document), Selection::caret(2));

        set_primary_selection(&mut document, Selection::caret(0));
        assert!(!backspace(&mut document));
        let end = document.rope.byte_len();
        set_primary_selection(&mut document, Selection::caret(end));
        assert!(!delete(&mut document));
    }

    #[test]
    fn movement_respects_multibyte_char_boundaries() {
        let mut document = document("aé日");
        let end = document.rope.byte_len();
        set_primary_selection(&mut document, Selection::caret(end));

        move_left(&mut document);
        assert_eq!(primary_selection(&document), Selection::caret(3));
        move_left(&mut document);
        assert_eq!(primary_selection(&document), Selection::caret(1));
        move_right(&mut document);
        assert_eq!(primary_selection(&document), Selection::caret(3));
    }

    #[test]
    fn vertical_and_line_boundary_movement_tracks_columns() {
        let mut document = document("abc\nde\nfghi");
        let mut view_state = EditorViewState::default();
        set_primary_selection(&mut document, Selection::caret(3));

        move_vertical(&mut document, &mut view_state, 1);
        assert_eq!(primary_selection(&document), Selection::caret(6));

        move_vertical(&mut document, &mut view_state, 1);
        assert_eq!(primary_selection(&document), Selection::caret(10));

        move_home(&mut document);
        assert_eq!(primary_selection(&document), Selection::caret(7));

        move_end(&mut document);
        assert_eq!(primary_selection(&document), Selection::caret(11));
    }

    #[test]
    fn visual_lines_include_empty_document_and_trailing_newline() {
        assert_eq!(visual_line_count(&Rope::from("")), 1);
        assert_eq!(visual_line_count(&Rope::from("a\n")), 2);
        assert_eq!(byte_for_line_column(&Rope::from("a\n"), 1, 0), 2);
    }

    #[test]
    fn search_highlight_columns_clip_to_visual_line() {
        let rope = Rope::from("abc\ndef");

        assert_eq!(
            highlight_columns_for_line(
                &rope,
                SearchHighlight {
                    start: 1,
                    end: 3,
                    is_current: false,
                },
                0,
            ),
            Some(HighlightColumns {
                start_column: 1,
                end_column: 3,
            })
        );
        assert_eq!(
            highlight_columns_for_line(
                &rope,
                SearchHighlight {
                    start: 1,
                    end: 3,
                    is_current: false,
                },
                1,
            ),
            None
        );
    }

    #[test]
    fn search_highlight_columns_split_multiline_matches() {
        let rope = Rope::from("abc\ndef");
        let highlight = SearchHighlight {
            start: 2,
            end: 6,
            is_current: true,
        };

        assert_eq!(
            highlight_columns_for_line(&rope, highlight, 0),
            Some(HighlightColumns {
                start_column: 2,
                end_column: 3,
            })
        );
        assert_eq!(
            highlight_columns_for_line(&rope, highlight, 1),
            Some(HighlightColumns {
                start_column: 0,
                end_column: 2,
            })
        );
    }

    #[test]
    fn search_highlight_columns_count_multibyte_characters() {
        let rope = Rope::from("aé日z");
        let highlight = SearchHighlight {
            start: 1,
            end: "aé日".len(),
            is_current: false,
        };

        assert_eq!(
            highlight_columns_for_line(&rope, highlight, 0),
            Some(HighlightColumns {
                start_column: 1,
                end_column: 3,
            })
        );
    }

    #[test]
    fn search_selection_is_clamped_to_valid_byte_offsets() {
        let mut document = document("aé日");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 2,
                head: 999,
            },
        );

        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 1,
                head: document.rope.byte_len(),
            }
        );
    }
}
