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

pub fn expand_selection_by_word(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    if start == end {
        // Caret mode: select word at caret
        if let Some((word_start, word_end)) = super::word_at_selection(&document.rope, selection) {
            set_primary_selection(
                document,
                Selection {
                    anchor: word_start,
                    head: word_end,
                },
            );
        }
    } else {
        // Selection mode: expand to include full words at both ends
        let new_start = previous_word_boundary(&document.rope, start);
        let new_end = next_word_boundary(&document.rope, end);
        set_primary_selection(
            document,
            Selection {
                anchor: new_start,
                head: new_end,
            },
        );
    }
}

pub fn contract_selection_by_word(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    if start == end {
        return;
    }

    let new_start = next_word_boundary(&document.rope, start);
    let new_end = previous_word_boundary(&document.rope, end);

    if new_start >= new_end {
        let mid = (new_start + new_end) / 2;
        set_primary_selection(document, Selection::caret(mid));
    } else {
        set_primary_selection(
            document,
            Selection {
                anchor: new_start.min(new_end),
                head: new_start.max(new_end),
            },
        );
    }
}

pub fn expand_selection_by_line(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    let start_line = line_index_of_byte(&document.rope, start);
    let end_line = line_index_of_byte(&document.rope, end);

    let new_start = byte_of_visual_line(&document.rope, start_line);
    let (_, new_end) = visual_line_bounds(&document.rope, end_line);

    set_primary_selection(
        document,
        Selection {
            anchor: new_start,
            head: new_end,
        },
    );
}

pub fn contract_selection_by_line(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    if start == end {
        return;
    }

    let start_line = line_index_of_byte(&document.rope, start);
    let end_line = line_index_of_byte(&document.rope, end);

    if start_line >= end_line {
        set_primary_selection(document, Selection::caret(start));
        return;
    }

    let new_start_line = (start_line + 1).min(end_line);
    let new_end_line = (end_line - 1).max(start_line);

    if new_start_line > new_end_line {
        let mid_line = (start_line + end_line) / 2;
        let mid_offset = byte_of_visual_line(&document.rope, mid_line);
        set_primary_selection(document, Selection::caret(mid_offset));
    } else {
        let new_start = byte_of_visual_line(&document.rope, new_start_line);
        let (_, new_end) = visual_line_bounds(&document.rope, new_end_line);
        set_primary_selection(
            document,
            Selection {
                anchor: new_start,
                head: new_end,
            },
        );
    }
}

pub fn expand_selection_by_bracket_pair(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let offset = selection.head;

    if let Some((open, close)) = find_matching_bracket_pair(&document.rope, offset) {
        set_primary_selection(
            document,
            Selection {
                anchor: open,
                head: close,
            },
        );
    }
}

pub fn contract_selection_by_bracket_pair(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    if start == end {
        return;
    }

    let mut chars_from_start = document.rope.byte_slice(start..end).chars();
    if let Some(first_char) = chars_from_start.next() {
        if is_opening_bracket(first_char) {
            let new_start = start + first_char.len_utf8();
            if new_start >= end {
                set_primary_selection(document, Selection::caret(start));
            } else {
                set_primary_selection(
                    document,
                    Selection {
                        anchor: new_start,
                        head: end,
                    },
                );
            }
            return;
        }
    }

    let chars_from_end = document.rope.byte_slice(start..end).chars();
    if let Some(last_char) = chars_from_end.last() {
        if is_closing_bracket(last_char) {
            let new_end = end - last_char.len_utf8();
            if new_end <= start {
                set_primary_selection(document, Selection::caret(end));
            } else {
                set_primary_selection(
                    document,
                    Selection {
                        anchor: start,
                        head: new_end,
                    },
                );
            }
        }
    }
}

pub fn expand_selection_by_indent_block(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    let start_line = line_index_of_byte(&document.rope, start);
    let end_line = line_index_of_byte(&document.rope, end);

    let base_indent = line_indent_level(&document.rope, start_line);

    let mut new_start_line = start_line;
    while new_start_line > 0 {
        let prev_line = new_start_line - 1;
        if line_indent_level(&document.rope, prev_line) >= base_indent
            && !is_blank_line(&document.rope, prev_line)
        {
            new_start_line = prev_line;
        } else {
            break;
        }
    }

    let line_count = visual_line_count(&document.rope);
    let mut new_end_line = end_line;
    while new_end_line + 1 < line_count {
        let next_line = new_end_line + 1;
        if line_indent_level(&document.rope, next_line) >= base_indent
            && !is_blank_line(&document.rope, next_line)
        {
            new_end_line = next_line;
        } else {
            break;
        }
    }

    let new_start = byte_of_visual_line(&document.rope, new_start_line);
    let (_, new_end) = if new_end_line >= document.rope.line_len() {
        (0, document.rope.byte_len())
    } else {
        visual_line_bounds(&document.rope, new_end_line)
    };

    set_primary_selection(
        document,
        Selection {
            anchor: new_start,
            head: new_end,
        },
    );
}

pub fn contract_selection_by_indent_block(document: &mut Document) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    if start == end {
        return;
    }

    let start_line = line_index_of_byte(&document.rope, start);
    let end_line = line_index_of_byte(&document.rope, end);

    if start_line >= end_line {
        set_primary_selection(document, Selection::caret(start));
        return;
    }

    let new_start_line = (start_line + 1).min(end_line);
    let new_end_line = (end_line - 1).max(start_line);

    if new_start_line > new_end_line {
        let mid_line = (start_line + end_line) / 2;
        let mid_offset = byte_of_visual_line(&document.rope, mid_line);
        set_primary_selection(document, Selection::caret(mid_offset));
    } else {
        let new_start = byte_of_visual_line(&document.rope, new_start_line);
        let (_, new_end) = if new_end_line >= document.rope.line_len() {
            (0, document.rope.byte_len())
        } else {
            visual_line_bounds(&document.rope, new_end_line)
        };
        set_primary_selection(
            document,
            Selection {
                anchor: new_start,
                head: new_end,
            },
        );
    }
}

fn find_matching_bracket_pair(rope: &Rope, offset: usize) -> Option<(usize, usize)> {
    let total = rope.byte_len();
    let offset = offset.min(total);

    // Search backwards from offset to find the nearest unmatched opening bracket
    let mut found_open = None;
    let mut current_pos = offset;
    for c in rope.byte_slice(..offset).chars().rev() {
        current_pos -= c.len_utf8();
        if is_opening_bracket(c) {
            found_open = Some((current_pos, c));
            break;
        }
    }

    let (open_offset, open_char) = found_open?;

    let close_char = match open_char {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        _ => return None,
    };

    let mut depth = 1usize;
    let mut pos = open_offset;
    for c in rope.byte_slice(open_offset..).chars() {
        if pos == open_offset {
            // Skip the opening bracket itself
            pos += c.len_utf8();
            continue;
        }
        if c == open_char {
            depth += 1;
        } else if c == close_char {
            depth -= 1;
            if depth == 0 {
                return Some((open_offset, pos + c.len_utf8()));
            }
        }
        pos += c.len_utf8();
    }

    None
}

fn is_opening_bracket(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | '<')
}

fn is_closing_bracket(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | '>')
}

pub fn move_to_line(document: &mut Document, line_number: usize) {
    clamp_primary_selection(document);
    let line_count = visual_line_count(&document.rope);
    let target_line = line_number.saturating_sub(1).min(line_count.saturating_sub(1));
    let target = byte_of_visual_line(&document.rope, target_line);
    apply_motion(document, target, false);
}

fn line_indent_level(rope: &Rope, line_index: usize) -> usize {
    if line_index >= rope.line_len() {
        return 0;
    }
    let line = rope.line(line_index).to_string();
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}
