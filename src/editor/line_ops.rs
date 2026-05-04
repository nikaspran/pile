use crop::Rope;

use crate::model::{Document, DocumentEdit, EditTransaction, Selection};
use crate::syntax_highlighting::DocumentSyntaxState;

use super::{
    byte_of_visual_line, clamp_primary_selection, line_index_of_byte, primary_selection,
    record_full_document_undo, selection_range, set_primary_selection, visual_line_bounds,
};

pub fn indent_selection(document: &mut Document) -> bool {
    change_line_indentation(document, IndentChange::Indent)
}

pub fn outdent_selection(document: &mut Document) -> bool {
    change_line_indentation(document, IndentChange::Outdent)
}

pub fn duplicate_selected_lines(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        let before_text = String::new();
        let before_selection = primary_selection(document);
        document.rope.insert(0, "\n");
        set_primary_selection(document, Selection::caret(1));
        document.commit_and_start_new_undo_group();
        document.push_undo(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: before_text,
            inserted_text: "\n".to_owned(),
            selections_before: vec![before_selection],
        });
        document.commit_undo_group();
        document.revision += 1;
        return true;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (line_start, line_end) =
        selected_full_line_bounds(&document.rope, selection_start, selection_end);
    let text = document.rope.byte_slice(line_start..line_end).to_string();
    if text.is_empty() {
        return false;
    }

    let prefix = if text.ends_with('\n') { "" } else { "\n" };
    let insert_at = line_end;
    let original_text = document.text();

    document.rope.insert(insert_at, &format!("{prefix}{text}"));

    let duplicate_start = insert_at + prefix.len();
    let anchor = duplicate_start + selection.anchor.saturating_sub(line_start);
    let head = duplicate_start + selection.head.saturating_sub(line_start);
    let new_selection = Selection {
        anchor: anchor.min(duplicate_start + text.len()),
        head: head.min(duplicate_start + text.len()),
    };

    record_full_document_undo(document, original_text, selection);

    set_primary_selection(document, new_selection);
    document.revision += 1;
    true
}

pub fn delete_selected_lines(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (mut delete_start, delete_end) =
        selected_full_line_bounds(&document.rope, selection_start, selection_end);

    if delete_start == delete_end {
        return false;
    }

    if delete_end == document.rope.byte_len() && delete_start > 0 {
        delete_start = previous_line_break_offset(&document.rope, delete_start)
            .map_or(delete_start, |offset| offset + 1);
        if delete_start > 0 {
            delete_start -= 1;
        }
    }

    let original_text = document.text();

    document.rope.delete(delete_start..delete_end);
    let caret = delete_start.min(document.rope.byte_len());
    record_full_document_undo(document, original_text, selection);

    set_primary_selection(document, Selection::caret(caret));
    document.revision += 1;
    true
}

pub fn move_selected_lines_up(document: &mut Document) -> bool {
    move_selected_lines(document, LineMoveDirection::Up)
}

pub fn move_selected_lines_down(document: &mut Document) -> bool {
    move_selected_lines(document, LineMoveDirection::Down)
}

pub fn join_selected_lines(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (first_line, selected_last_line) =
        selected_line_range(&document.rope, selection_start, selection_end);
    let last_line = if first_line == selected_last_line {
        first_line + 1
    } else {
        selected_last_line
    };

    if last_line >= document.rope.line_len() {
        return false;
    }

    let start = byte_of_visual_line(&document.rope, first_line);
    let end = if last_line + 1 < document.rope.line_len() {
        byte_of_visual_line(&document.rope, last_line + 1)
    } else {
        document.rope.byte_len()
    };
    let original = document.rope.byte_slice(start..end).to_string();
    let (body, suffix) = original
        .strip_suffix('\n')
        .map_or((original.as_str(), ""), |body| (body, "\n"));
    let mut joined = join_lines_text(body);
    joined.push_str(suffix);
    if joined == original {
        return false;
    }

    let caret = start + joined.len() - suffix.len();
    document.apply_grouped_edit(DocumentEdit {
        range: start..end,
        inserted_text: joined,
        selections_before: vec![selection],
        selections_after: vec![Selection::caret(caret)],
    });
    true
}

pub fn sort_selected_lines(document: &mut Document) -> bool {
    transform_selected_lines(document, |lines| {
        lines.sort_by(|left, right| left.cmp(right))
    })
}

pub fn reverse_selected_lines(document: &mut Document) -> bool {
    transform_selected_lines(document, |lines| {
        lines.reverse();
    })
}

pub fn trim_trailing_whitespace(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (start, end) = selected_full_line_bounds(&document.rope, selection_start, selection_end);

    let original = document.rope.byte_slice(start..end).to_string();
    let has_trailing_newline = original.ends_with('\n');
    let body = original.strip_suffix('\n').unwrap_or(&original);

    let lines: Vec<&str> = body.split('\n').collect();
    let trimmed_lines: Vec<String> = lines
        .iter()
        .map(|line| line.trim_end().to_string())
        .collect();

    let changed = lines
        .iter()
        .zip(trimmed_lines.iter())
        .any(|(orig, trimmed)| *orig != trimmed);

    if !changed {
        return false;
    }

    let mut replacement = trimmed_lines.join("\n");
    if has_trailing_newline {
        replacement.push('\n');
    }

    let replacement_len = replacement.len();
    document.apply_grouped_edit(DocumentEdit {
        range: start..end,
        inserted_text: replacement.clone(),
        selections_before: vec![selection],
        selections_after: vec![Selection {
            anchor: start,
            head: start + replacement_len,
        }],
    });
    true
}

pub fn normalize_whitespace(document: &mut Document) -> bool {
    if document.rope.byte_len() == 0 {
        return false;
    }

    let original = document.text();
    let tab_width = document.tab_width;
    let use_soft_tabs = document.use_soft_tabs;

    let lines: Vec<&str> = original.split('\n').collect();
    let mut changed = false;
    let mut result = String::new();

    for (i, line) in lines.iter().enumerate() {
        let mut new_line = String::new();
        let mut chars = line.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\t' && use_soft_tabs {
                // Convert tab to spaces
                let spaces = " ".repeat(tab_width);
                new_line.push_str(&spaces);
                changed = true;
            } else if ch == ' ' && !use_soft_tabs {
                // Check if this is a tab-width sequence of spaces
                let mut space_count = 1;
                while chars.peek() == Some(&' ') {
                    space_count += 1;
                    chars.next();
                }
                if space_count >= tab_width {
                    // Convert spaces to tab
                    new_line.push('\t');
                    changed = true;
                } else {
                    // Keep the spaces
                    for _ in 0..space_count {
                        new_line.push(' ');
                    }
                }
            } else {
                new_line.push(ch);
            }
        }

        result.push_str(&new_line);
        if i < lines.len() - 1 {
            result.push('\n');
        }
    }

    if !changed {
        return false;
    }

    let selection = document.selections[0];
    let original_text = document.text();
    document.rope = Rope::from(result);
    document.revision += 1;

    record_full_document_undo(document, original_text, selection);
    true
}

pub fn toggle_comments(document: &mut Document, comment_prefix: &str) -> bool {
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (sel_start, sel_end) = selection_range(selection);
    let (start, end) = selected_full_line_bounds(&document.rope, sel_start, sel_end);

    let original = document.rope.byte_slice(start..end).to_string();
    let has_trailing_newline = original.ends_with('\n');
    let body = original.strip_suffix('\n').unwrap_or(&original);

    let lines: Vec<&str> = body.split('\n').collect();
    let mut result = String::new();
    let mut toggled = false;
    let mut any_commented = false;
    let mut any_uncommented = false;

    // Get syntax state for syntax-aware toggling
    let syntax_state = &document.syntax_state;
    let text = document.rope.byte_slice(..).to_string();

    for (i, line) in lines.iter().enumerate() {
        let line_start = start + body[..body.find(line).unwrap_or(0)].chars().count();
        
        // Skip lines that are inside string literals
        if syntax_state.is_inside_string(&text, line_start) {
            result.push_str(line);
            if i < lines.len() - 1 {
                result.push('\n');
            }
            continue;
        }

        // Check if this line is actually a comment (using tree-sitter if available)
        let is_commented = if syntax_state.parsed_as().map_or(false, |l| l.has_tree_sitter()) {
            // Use tree-sitter to check if the content after whitespace is a comment
            let trimmed = line.trim_start();
            if trimmed.starts_with(comment_prefix) {
                // Verify it's actually in a comment node (not a string containing the prefix)
                let first_char_offset = line_start + line.len() - trimmed.len();
                syntax_state.is_inside_comment(&text, first_char_offset)
            } else {
                false
            }
        } else {
            // Fallback to simple prefix check
            line.starts_with(comment_prefix)
        };

        if is_commented {
            // Remove comment prefix
            let after_prefix = line[comment_prefix.len()..].to_string();
            // Also remove a single trailing space if present (common style)
            let after_prefix = after_prefix.strip_prefix(' ').unwrap_or(&after_prefix);
            result.push_str(after_prefix);
            any_commented = true;
        } else {
            // Add comment prefix
            result.push_str(comment_prefix);
            // Add a space after prefix if line is not empty and doesn't already start with space
            if !line.is_empty() && !line.starts_with(' ') {
                result.push(' ');
            }
            result.push_str(line);
            any_uncommented = true;
        }

        if i < lines.len() - 1 {
            result.push('\n');
        }
    }

    // If some lines were commented and some uncommented, we need a consistent action
    // Default to commenting all uncommented lines
    if any_commented && any_uncommented {
        // Re-process: comment all uncommented lines
        result.clear();
        for (i, line) in lines.iter().enumerate() {
            let line_start = start + body[..body.find(line).unwrap_or(0)].chars().count();
            
            if syntax_state.is_inside_string(&text, line_start) {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            let is_commented = if syntax_state.parsed_as().map_or(false, |l| l.has_tree_sitter()) {
                let trimmed = line.trim_start();
                if trimmed.starts_with(comment_prefix) {
                    let first_char_offset = line_start + line.len() - trimmed.len();
                    syntax_state.is_inside_comment(&text, first_char_offset)
                } else {
                    false
                }
            } else {
                line.starts_with(comment_prefix)
            };

            if is_commented {
                // Already commented, keep as-is but remove prefix
                let after_prefix = line[comment_prefix.len()..].to_string();
                let after_prefix = after_prefix.strip_prefix(' ').unwrap_or(&after_prefix);
                result.push_str(after_prefix);
            } else {
                // Comment it
                result.push_str(comment_prefix);
                if !line.is_empty() && !line.starts_with(' ') {
                    result.push(' ');
                }
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }
    }

    toggled = any_commented || any_uncommented;

    if !toggled {
        return false;
    }

    if has_trailing_newline {
        result.push('\n');
    }

    if result == original {
        return false;
    }

    let replacement_len = result.len();
    document.apply_grouped_edit(DocumentEdit {
        range: start..end,
        inserted_text: result,
        selections_before: vec![selection],
        selections_after: vec![Selection {
            anchor: start,
            head: start + replacement_len,
        }],
    });
    true
}

pub(super) fn transform_selected_lines<F>(document: &mut Document, transform: F) -> bool
where
    F: FnOnce(&mut Vec<String>),
{
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (first_line, last_line) =
        selected_line_range(&document.rope, selection_start, selection_end);
    if first_line == last_line {
        return false;
    }

    let (start, end) = selected_full_line_bounds(&document.rope, selection_start, selection_end);
    let original = document.rope.byte_slice(start..end).to_string();
    let has_trailing_newline = original.ends_with('\n');
    let body = original.strip_suffix('\n').unwrap_or(&original);
    let mut lines = body.split('\n').map(ToOwned::to_owned).collect::<Vec<_>>();
    if lines.len() <= 1 {
        return false;
    }

    transform(&mut lines);

    let mut replacement = lines.join("\n");
    if has_trailing_newline {
        replacement.push('\n');
    }
    if replacement == original {
        return false;
    }

    let replacement_len = replacement.len();
    document.apply_grouped_edit(DocumentEdit {
        range: start..end,
        inserted_text: replacement.clone(),
        selections_before: vec![selection],
        selections_after: vec![Selection {
            anchor: start,
            head: start + replacement_len,
        }],
    });
    true
}

pub(super) fn join_lines_text(text: &str) -> String {
    let mut lines = text.split('\n');
    let Some(first) = lines.next() else {
        return String::new();
    };

    let mut joined = first.trim_end_matches([' ', '\t']).to_owned();
    for line in lines {
        let next = line.trim_start_matches([' ', '\t']);
        if joined.is_empty()
            || next.is_empty()
            || joined.chars().next_back().is_some_and(char::is_whitespace)
        {
            joined.push_str(next);
        } else {
            joined.push(' ');
            joined.push_str(next);
        }
    }

    joined
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LineMoveDirection {
    Up,
    Down,
}

pub(super) fn move_selected_lines(document: &mut Document, direction: LineMoveDirection) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (first_line, last_line) =
        selected_line_range(&document.rope, selection_start, selection_end);
    let (selected_start, selected_end) =
        selected_full_line_bounds(&document.rope, selection_start, selection_end);

    let original_text = document.text();

    match direction {
        LineMoveDirection::Up => {
            if first_line == 0 {
                return false;
            }

            let previous_start = byte_of_visual_line(&document.rope, first_line - 1);
            let previous_text = document
                .rope
                .byte_slice(previous_start..selected_start)
                .to_string();
            let selected_text = document
                .rope
                .byte_slice(selected_start..selected_end)
                .to_string();
            let replacement = swap_line_text_up(&selected_text, &previous_text);
            let shift = selected_start - previous_start;

            document.rope.delete(previous_start..selected_end);
            document.rope.insert(previous_start, &replacement);
            set_primary_selection(
                document,
                Selection {
                    anchor: selection.anchor.saturating_sub(shift),
                    head: selection.head.saturating_sub(shift),
                },
            );
        }
        LineMoveDirection::Down => {
            let next_line = last_line + 1;
            if next_line >= document.rope.line_len() {
                return false;
            }

            let next_end = if next_line + 1 < document.rope.line_len() {
                byte_of_visual_line(&document.rope, next_line + 1)
            } else {
                document.rope.byte_len()
            };
            let selected_text = document
                .rope
                .byte_slice(selected_start..selected_end)
                .to_string();
            let next_text = document.rope.byte_slice(selected_end..next_end).to_string();
            let shift = moved_down_selection_shift(&selected_text, &next_text);
            let replacement = swap_line_text_down(&selected_text, &next_text);

            document.rope.delete(selected_start..next_end);
            document.rope.insert(selected_start, &replacement);
            set_primary_selection(
                document,
                Selection {
                    anchor: selection.anchor + shift,
                    head: selection.head + shift,
                },
            );
        }
    }

    record_full_document_undo(document, original_text, selection);

    document.revision += 1;
    true
}

pub(super) fn swap_line_text_up(selected_text: &str, previous_text: &str) -> String {
    if !selected_text.ends_with('\n') && previous_text.ends_with('\n') {
        let previous_without_break = previous_text.strip_suffix('\n').unwrap_or(previous_text);
        format!("{selected_text}\n{previous_without_break}")
    } else {
        format!("{selected_text}{previous_text}")
    }
}

pub(super) fn swap_line_text_down(selected_text: &str, next_text: &str) -> String {
    if selected_text.ends_with('\n') && !next_text.ends_with('\n') {
        let selected_without_break = selected_text.strip_suffix('\n').unwrap_or(selected_text);
        format!("{next_text}\n{selected_without_break}")
    } else {
        format!("{next_text}{selected_text}")
    }
}

pub(super) fn moved_down_selection_shift(selected_text: &str, next_text: &str) -> usize {
    if selected_text.ends_with('\n') && !next_text.ends_with('\n') {
        next_text.len() + 1
    } else {
        next_text.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IndentChange {
    Indent,
    Outdent,
}

pub(super) fn change_line_indentation(document: &mut Document, change: IndentChange) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (sel_start, sel_end) = selection_range(selection);
    let (first_line, last_line) = selected_line_range(&document.rope, sel_start, sel_end);

    match change {
        IndentChange::Indent => {
            let line_starts = (first_line..=last_line)
                .map(|line| byte_of_visual_line(&document.rope, line))
                .collect::<Vec<_>>();
            if line_starts.is_empty() {
                return false;
            }

            let indent = indent_string(document.tab_width, document.use_soft_tabs);
            let indent_len = indent.len();

            let original_text = document.text();
            let mut adjusted_selection = selection;
            let mut shift = 0;
            for line_start in line_starts {
                let insert_at = line_start + shift;
                document.rope.insert(insert_at, &indent);
                adjusted_selection.anchor =
                    adjust_offset_after_insert(adjusted_selection.anchor, insert_at, indent_len);
                adjusted_selection.head =
                    adjust_offset_after_insert(adjusted_selection.head, insert_at, indent_len);
                shift += indent_len;
            }

            record_full_document_undo(document, original_text, selection);

            set_primary_selection(document, adjusted_selection);
            document.revision += 1;
            true
        }
        IndentChange::Outdent => {
            let deletions = (first_line..=last_line)
                .filter_map(|line| outdent_range_for_line(&document.rope, line))
                .collect::<Vec<_>>();
            if deletions.is_empty() {
                return false;
            }

            let original_text = document.text();
            let mut adjusted_selection = selection;
            for (delete_start, delete_end) in deletions.into_iter().rev() {
                document.rope.delete(delete_start..delete_end);
                adjusted_selection.anchor =
                    adjust_offset_after_delete(adjusted_selection.anchor, delete_start, delete_end);
                adjusted_selection.head =
                    adjust_offset_after_delete(adjusted_selection.head, delete_start, delete_end);
            }

            record_full_document_undo(document, original_text, selection);

            set_primary_selection(document, adjusted_selection);
            document.revision += 1;
            true
        }
    }
}

fn indent_string(tab_width: usize, use_soft_tabs: bool) -> String {
    if use_soft_tabs {
        " ".repeat(tab_width)
    } else {
        "\t".to_owned()
    }
}

pub(super) fn selected_line_range(rope: &Rope, start: usize, end: usize) -> (usize, usize) {
    let first_line = line_index_of_byte(rope, start);
    let mut last_line = line_index_of_byte(rope, end);
    if end > start && end == byte_of_visual_line(rope, last_line) {
        last_line = last_line.saturating_sub(1);
    }
    (first_line, last_line.max(first_line))
}

pub(super) fn selected_full_line_bounds(rope: &Rope, start: usize, end: usize) -> (usize, usize) {
    let (first_line, last_line) = selected_line_range(rope, start, end);
    let line_start = byte_of_visual_line(rope, first_line);
    let next_line = last_line + 1;
    let line_end = if next_line < rope.line_len() {
        byte_of_visual_line(rope, next_line)
    } else {
        rope.byte_len()
    };
    (line_start, line_end)
}

pub(super) fn previous_line_break_offset(rope: &Rope, before: usize) -> Option<usize> {
    let before = before.min(rope.byte_len());
    rope.byte_slice(..before)
        .chars()
        .rev()
        .scan(before, |offset, char| {
            *offset -= char.len_utf8();
            Some((*offset, char))
        })
        .find_map(|(offset, char)| (char == '\n').then_some(offset))
}

pub(super) fn outdent_range_for_line(rope: &Rope, line_index: usize) -> Option<(usize, usize)> {
    let line_start = byte_of_visual_line(rope, line_index);
    let (_, line_end) = visual_line_bounds(rope, line_index);
    let mut delete_end = line_start;
    let mut spaces = 0;

    for char in rope.byte_slice(line_start..line_end).chars() {
        if char == '\t' {
            return Some((line_start, line_start + char.len_utf8()));
        }
        if char != ' ' || spaces == 4 {
            break;
        }
        spaces += 1;
        delete_end += char.len_utf8();
    }

    (delete_end > line_start).then_some((line_start, delete_end))
}

pub(super) fn adjust_offset_after_insert(
    offset: usize,
    insert_at: usize,
    inserted_len: usize,
) -> usize {
    if offset >= insert_at {
        offset + inserted_len
    } else {
        offset
    }
}

pub(super) fn adjust_offset_after_delete(
    offset: usize,
    delete_start: usize,
    delete_end: usize,
) -> usize {
    if offset <= delete_start {
        offset
    } else if offset >= delete_end {
        offset - (delete_end - delete_start)
    } else {
        delete_start
    }
}
