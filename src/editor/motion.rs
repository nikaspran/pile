use crop::Rope;

use crate::model::{Document, Selection};

use super::{
    EditorViewState, byte_for_line_column, byte_of_visual_line, clamp_primary_selection,
    column_of_byte, line_index_of_byte, next_grapheme_boundary, next_word_boundary,
    previous_grapheme_boundary, previous_word_boundary, primary_selection, selection_range,
    set_primary_selection, visual_line_bounds, visual_line_count,
};

pub fn move_left(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let target = if !extend && selection.anchor != selection.head {
        selection_range(selection).0
    } else {
        previous_grapheme_boundary(&document.rope, selection.head)
    };
    apply_motion(document, target, extend);
}

pub fn move_right(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let target = if !extend && selection.anchor != selection.head {
        selection_range(selection).1
    } else {
        next_grapheme_boundary(&document.rope, selection.head)
    };
    apply_motion(document, target, extend);
}

pub fn move_word_left(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let from = if !extend && selection.anchor != selection.head {
        selection_range(selection).0
    } else {
        selection.head
    };
    let target = previous_word_boundary(&document.rope, from);
    apply_motion(document, target, extend);
}

pub fn move_word_right(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let from = if !extend && selection.anchor != selection.head {
        selection_range(selection).1
    } else {
        selection.head
    };
    let target = next_word_boundary(&document.rope, from);
    apply_motion(document, target, extend);
}

pub fn move_home(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let line = line_index_of_byte(&document.rope, primary_selection(document).head);
    let target = byte_of_visual_line(&document.rope, line);
    apply_motion(document, target, extend);
}

pub fn move_end(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let line = line_index_of_byte(&document.rope, primary_selection(document).head);
    let (_, end) = visual_line_bounds(&document.rope, line);
    apply_motion(document, end, extend);
}

pub fn move_vertical(
    document: &mut Document,
    view_state: &mut EditorViewState,
    delta: isize,
    extend: bool,
) {
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
    apply_motion(document, target, extend);
}

pub fn move_document_start(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    apply_motion(document, 0, extend);
}

pub fn move_document_end(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    apply_motion(document, document.rope.byte_len(), extend);
}

pub fn move_paragraph_up(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let head = primary_selection(document).head;
    let current_line = line_index_of_byte(&document.rope, head);
    let target = (0..current_line)
        .rev()
        .find(|line| is_blank_line(&document.rope, *line))
        .map(|line| byte_of_visual_line(&document.rope, line))
        .unwrap_or(0);
    apply_motion(document, target, extend);
}

pub fn move_paragraph_down(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let head = primary_selection(document).head;
    let current_line = line_index_of_byte(&document.rope, head);
    let line_count = visual_line_count(&document.rope);
    let target = ((current_line + 1)..line_count)
        .find(|line| is_blank_line(&document.rope, *line))
        .map(|line| byte_of_visual_line(&document.rope, line))
        .unwrap_or_else(|| document.rope.byte_len());
    apply_motion(document, target, extend);
}

pub fn move_page(
    document: &mut Document,
    view_state: &mut EditorViewState,
    delta_pages: isize,
    extend: bool,
) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let current_line = line_index_of_byte(&document.rope, selection.head);
    let line_count = visual_line_count(&document.rope);
    let visible_rows = view_state.visible_rows.unwrap_or(1).max(1);
    let step = visible_rows.saturating_sub(1).max(1) as isize;
    let target_line =
        (current_line as isize + delta_pages * step).clamp(0, line_count as isize - 1) as usize;
    let column = view_state
        .preferred_column
        .unwrap_or_else(|| column_of_byte(&document.rope, selection.head));
    view_state.preferred_column = Some(column);
    let target = byte_for_line_column(&document.rope, target_line, column);
    apply_motion(document, target, extend);
}

pub(super) fn is_blank_line(rope: &Rope, line_index: usize) -> bool {
    if line_index >= rope.line_len() {
        return true;
    }
    rope.line(line_index).chars().all(|c| c.is_whitespace())
}

pub(super) fn apply_motion(document: &mut Document, target: usize, extend: bool) {
    let selection = primary_selection(document);
    let new = if extend {
        Selection {
            anchor: selection.anchor,
            head: target,
        }
    } else {
        Selection::caret(target)
    };
    set_primary_selection(document, new);
}
