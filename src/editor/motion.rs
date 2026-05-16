use crop::Rope;

use crate::model::{Document, Selection};
use crate::syntax_highlighting::DocumentSyntaxState;

use super::{
    EditorViewState, byte_for_line_column, byte_of_visual_line, clamp_primary_selection,
    clamp_selection_to_rope, column_of_byte, line_index_of_byte, next_grapheme_boundary,
    next_word_boundary, previous_grapheme_boundary, previous_word_boundary, primary_selection,
    selection_range, set_primary_selection, visual_line_bounds, visual_line_count,
};

pub fn move_left(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |rope, selection| {
        if !extend && selection.anchor != selection.head {
            selection_range(selection).0
        } else {
            previous_grapheme_boundary(rope, selection.head)
        }
    });
}

pub fn move_right(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |rope, selection| {
        if !extend && selection.anchor != selection.head {
            selection_range(selection).1
        } else {
            next_grapheme_boundary(rope, selection.head)
        }
    });
}

pub fn move_word_left(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |rope, selection| {
        let from = if !extend && selection.anchor != selection.head {
            selection_range(selection).0
        } else {
            selection.head
        };
        previous_word_boundary(rope, from)
    });
}

pub fn move_word_right(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |rope, selection| {
        let from = if !extend && selection.anchor != selection.head {
            selection_range(selection).1
        } else {
            selection.head
        };
        next_word_boundary(rope, from)
    });
}

pub fn move_home(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |rope, selection| {
        let line = line_index_of_byte(rope, selection.head);
        byte_of_visual_line(rope, line)
    });
}

pub fn move_end(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |rope, selection| {
        let line = line_index_of_byte(rope, selection.head);
        let (_, end) = visual_line_bounds(rope, line);
        end
    });
}

pub fn move_vertical(
    document: &mut Document,
    view_state: &mut EditorViewState,
    delta: isize,
    extend: bool,
) {
    if document.selections.len() <= 1 {
        clamp_primary_selection(document);
        let selection = primary_selection(document);
        let current_line = line_index_of_byte(&document.rope, selection.head);
        let line_count = visual_line_count(&document.rope);
        let target_line =
            (current_line as isize + delta).clamp(0, line_count as isize - 1) as usize;
        let column = view_state
            .preferred_column
            .unwrap_or_else(|| column_of_byte(&document.rope, selection.head));
        view_state.preferred_column = Some(column);
        let target = byte_for_line_column(&document.rope, target_line, column);
        apply_motion(document, target, extend);
        return;
    }

    view_state.preferred_column = None;
    apply_motion_to_all(document, extend, |rope, selection| {
        let current_line = line_index_of_byte(rope, selection.head);
        let line_count = visual_line_count(rope);
        let target_line =
            (current_line as isize + delta).clamp(0, line_count as isize - 1) as usize;
        let column = column_of_byte(rope, selection.head);
        byte_for_line_column(rope, target_line, column)
    });
}

pub fn move_document_start(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |_rope, _selection| 0);
}

pub fn move_document_end(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |rope, _selection| rope.byte_len());
}

pub fn move_paragraph_up(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |rope, selection| {
        let current_line = line_index_of_byte(rope, selection.head);
        (0..current_line)
            .rev()
            .find(|line| is_blank_line(rope, *line))
            .map(|line| byte_of_visual_line(rope, line))
            .unwrap_or(0)
    });
}

pub fn move_paragraph_down(document: &mut Document, extend: bool) {
    apply_motion_to_all(document, extend, |rope, selection| {
        let current_line = line_index_of_byte(rope, selection.head);
        let line_count = visual_line_count(rope);
        ((current_line + 1)..line_count)
            .find(|line| is_blank_line(rope, *line))
            .map(|line| byte_of_visual_line(rope, line))
            .unwrap_or_else(|| rope.byte_len())
    });
}

pub fn move_page(
    document: &mut Document,
    view_state: &mut EditorViewState,
    delta_pages: isize,
    extend: bool,
) {
    let visible_rows = view_state.visible_rows.unwrap_or(1).max(1);
    let step = visible_rows.saturating_sub(1).max(1) as isize;

    if document.selections.len() <= 1 {
        clamp_primary_selection(document);
        let selection = primary_selection(document);
        let current_line = line_index_of_byte(&document.rope, selection.head);
        let line_count = visual_line_count(&document.rope);
        let target_line =
            (current_line as isize + delta_pages * step).clamp(0, line_count as isize - 1) as usize;
        let column = view_state
            .preferred_column
            .unwrap_or_else(|| column_of_byte(&document.rope, selection.head));
        view_state.preferred_column = Some(column);
        let target = byte_for_line_column(&document.rope, target_line, column);
        apply_motion(document, target, extend);
        return;
    }

    view_state.preferred_column = None;
    apply_motion_to_all(document, extend, |rope, selection| {
        let current_line = line_index_of_byte(rope, selection.head);
        let line_count = visual_line_count(rope);
        let target_line =
            (current_line as isize + delta_pages * step).clamp(0, line_count as isize - 1) as usize;
        let column = column_of_byte(rope, selection.head);
        byte_for_line_column(rope, target_line, column)
    });
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

fn apply_motion_to_all<F>(document: &mut Document, extend: bool, mut target_for_selection: F)
where
    F: FnMut(&Rope, Selection) -> usize,
{
    if document.selections.is_empty() {
        document.selections.push(Selection::caret(0));
    }

    let rope = &document.rope;
    document.selections = document
        .selections
        .iter()
        .copied()
        .map(|selection| {
            let selection = clamp_selection_to_rope(rope, selection);
            let target = target_for_selection(rope, selection);
            let new = if extend {
                Selection {
                    anchor: selection.anchor,
                    head: target,
                }
            } else {
                Selection::caret(target)
            };
            clamp_selection_to_rope(rope, new)
        })
        .collect();
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
    // First try direct match at cursor position
    if let Some(result) = find_matching_bracket_at(rope, offset) {
        return Some(result);
    }

    // Search backward from cursor to find the innermost enclosing bracket pair
    let mut best_match: Option<(usize, usize)> = None;

    // Iterate through characters before the cursor (going backward)
    let mut search_offset = offset;
    while search_offset > 0 {
        let ch = rope.byte_slice(..search_offset).chars().next_back()?;
        let byte_offset = search_offset - ch.len_utf8();

        if is_opening_bracket(ch) {
            // Check if this opening bracket has a matching closing bracket
            if let Some((open, close)) = find_matching_forward(rope, byte_offset, ch) {
                // Make sure cursor is between the brackets and this is the innermost pair
                if open <= offset && offset <= close {
                    // Keep the innermost pair (smallest range)
                    if best_match.map_or(true, |(o, c)| open > o || (open == o && close < c)) {
                        best_match = Some((open, close));
                    }
                }
            }
        }

        search_offset = byte_offset;
    }

    best_match
}

fn is_opening_bracket(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | '<')
}

fn is_closing_bracket(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | '>')
}

/// Returns the matching bracket position if the character at `offset` (or just before it) is a bracket.
/// Checks both the character at offset and the character before offset to handle caret between chars.
/// Returns `Some((bracket_offset, matching_offset))` if a match is found.
pub fn find_matching_bracket_at(rope: &Rope, offset: usize) -> Option<(usize, usize)> {
    let total = rope.byte_len();

    // Try tree-sitter based matching first if we have syntax state
    // (This will be called from a context where syntax_state is available)
    // For now, fall back to character-based matching

    // Check character at offset (if not at end)
    if offset < total {
        let ch = rope.byte_slice(offset..).chars().next()?;
        if is_opening_bracket(ch) {
            return find_matching_forward(rope, offset, ch);
        }
        if is_closing_bracket(ch) {
            return find_matching_backward(rope, offset, ch);
        }
    }

    // Check character before offset (if not at start)
    if offset > 0 {
        let mut pos = offset;
        let ch = rope.byte_slice(..offset).chars().next_back()?;
        pos -= ch.len_utf8();
        if is_opening_bracket(ch) {
            return find_matching_forward(rope, pos, ch);
        }
        if is_closing_bracket(ch) {
            return find_matching_backward(rope, pos, ch);
        }
    }

    None
}

/// Syntax-aware bracket matching using tree-sitter.
/// Returns `Some((open_offset, close_offset))` if a matching pair is found.
#[allow(dead_code)]
pub fn find_matching_bracket_at_syntax(
    rope: &Rope,
    offset: usize,
    syntax_state: &DocumentSyntaxState,
) -> Option<(usize, usize)> {
    let tree = syntax_state.tree()?;

    // Find the leaf node at the offset
    let root = tree.root_node();
    let leaf = find_leaf_node_at_offset(root, offset)?;

    // Check if the leaf or its parent is a bracket/parenthesis node
    let mut current: Option<tree_sitter::Node> = Some(leaf);
    while let Some(node) = current {
        let kind = node.kind();

        // Check if this node is a bracket or parenthesis
        if kind == "{"
            || kind == "}"
            || kind == "("
            || kind == ")"
            || kind == "["
            || kind == "]"
            || kind == "<"
            || kind == ">"
            || kind == "\""
            || kind == "'"
        {
            // Found a bracket node, now find its pair
            let node_start = node.start_byte();
            let node_end = node.end_byte();

            // Determine if it's opening or closing
            let ch = rope.byte_slice(node_start..node_end).chars().next()?;

            if is_opening_bracket(ch) {
                // Find matching closing bracket using tree structure
                return find_matching_bracket_in_tree(rope, node_start, ch);
            } else if is_closing_bracket(ch) {
                // Find matching opening bracket
                if let Some(open_ch) = match ch {
                    ')' => Some('('),
                    ']' => Some('['),
                    '}' => Some('{'),
                    '>' => Some('<'),
                    _ => None,
                } {
                    return find_matching_bracket_in_tree_backward(rope, node_start, open_ch);
                }
            }
        }

        current = node.parent();
    }

    None
}

/// Find matching bracket using tree-sitter tree structure.
#[allow(dead_code)]
fn find_matching_bracket_in_tree(
    rope: &Rope,
    open_offset: usize,
    open_char: char,
) -> Option<(usize, usize)> {
    let close_char = match open_char {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        _ => return None,
    };

    // Use character-based search but skip strings and comments
    // This is a simplified version - a full implementation would walk the tree
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

/// Find matching opening bracket using tree-sitter tree structure (backward search).
#[allow(dead_code)]
fn find_matching_bracket_in_tree_backward(
    rope: &Rope,
    close_offset: usize,
    open_char: char,
) -> Option<(usize, usize)> {
    let mut depth = 1usize;
    let mut current_pos = close_offset;
    for c in rope.byte_slice(..close_offset).chars().rev() {
        current_pos -= c.len_utf8();
        if c == open_char {
            depth -= 1;
            if depth == 0 {
                return Some((current_pos, close_offset + open_char.len_utf8()));
            }
        } else if c
            == match open_char {
                '(' => ')',
                '[' => ']',
                '{' => '}',
                '<' => '>',
                _ => c,
            }
        {
            depth += 1;
        }
    }

    None
}

/// Find the leaf node at the given byte offset.
#[allow(dead_code)]
fn find_leaf_node_at_offset(node: tree_sitter::Node, offset: usize) -> Option<tree_sitter::Node> {
    if node.start_byte() > offset || offset >= node.end_byte() {
        return None;
    }

    let mut current = node;
    loop {
        let child_count = current.child_count();
        if child_count == 0 {
            return Some(current);
        }

        let mut found = None;
        for i in 0..child_count {
            if let Some(child) = current.child(i as u32) {
                if child.start_byte() <= offset && offset < child.end_byte() {
                    found = Some(child);
                    break;
                }
            }
        }

        match found {
            Some(child) => current = child,
            None => return Some(current),
        }
    }
}

/// Find matching closing bracket starting from an opening bracket position.
fn find_matching_forward(
    rope: &Rope,
    open_offset: usize,
    open_char: char,
) -> Option<(usize, usize)> {
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

/// Find matching opening bracket starting from a closing bracket position.
fn find_matching_backward(
    rope: &Rope,
    close_offset: usize,
    close_char: char,
) -> Option<(usize, usize)> {
    let open_char = match close_char {
        ')' => '(',
        ']' => '[',
        '}' => '{',
        '>' => '<',
        _ => return None,
    };

    let mut depth = 1usize;
    let mut current_pos = close_offset;
    for c in rope.byte_slice(..close_offset).chars().rev() {
        current_pos -= c.len_utf8();
        if c == close_char {
            depth += 1;
        } else if c == open_char {
            depth -= 1;
            if depth == 0 {
                return Some((current_pos, close_offset + close_char.len_utf8()));
            }
        }
    }

    None
}

pub fn move_to_line(document: &mut Document, line_number: usize) {
    clamp_primary_selection(document);
    let line_count = visual_line_count(&document.rope);
    let target_line = line_number
        .saturating_sub(1)
        .min(line_count.saturating_sub(1));
    let target = byte_of_visual_line(&document.rope, target_line);
    apply_motion(document, target, false);
}

pub fn line_indent_level(rope: &Rope, line_index: usize) -> usize {
    if line_index >= rope.line_len() {
        return 0;
    }
    let line = rope.line(line_index).to_string();
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}
