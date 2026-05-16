use crop::Rope;

use crate::model::{Document, DocumentEdit, Selection};
use crate::search;

use super::{
    byte_for_line_column, byte_of_visual_line, column_of_byte, line_index_of_byte,
    next_grapheme_boundary, next_word_boundary, previous_grapheme_boundary, previous_word_boundary,
    primary_selection, visual_line_count,
};

/// Adds the next occurrence of the word under the primary cursor as a new selection.
/// If no occurrence selections exist yet, starts with the primary selection.
pub fn add_next_match(document: &mut Document) {
    let primary = primary_selection(document);
    let rope = &document.rope;

    // Determine the query from the primary selection or word under cursor
    let query_range = word_at_selection(rope, primary);
    let query = if let Some((start, end)) = query_range {
        let text = rope.byte_slice(start..end).to_string();
        if text.is_empty() {
            return;
        }
        text
    } else {
        return;
    };

    // Initialize occurrence selections if empty
    if document.occurrence_selections.is_empty() {
        if let Some((start, end)) = query_range {
            document.occurrence_selections.push(Selection {
                anchor: start,
                head: end,
            });
        }
        document.multi_cursor_query = Some(query.clone());
    }

    // Find all matches for the query
    let options = search::SearchOptions {
        case_sensitive: false,
        whole_word: true,
        use_regex: false,
    };
    let matches = search::find_matches(rope, &query, options);

    // Collect all currently selected ranges
    let all_selected: std::collections::HashSet<_> = document
        .selections
        .iter()
        .chain(document.occurrence_selections.iter())
        .map(|s| {
            let (start, end) = if s.anchor <= s.head {
                (s.anchor, s.head)
            } else {
                (s.head, s.anchor)
            };
            (start, end)
        })
        .collect();

    // Find the next unselected match
    let next = matches
        .iter()
        .find(|m| !all_selected.contains(&(m.start, m.end)));

    if let Some(m) = next {
        let new_selection = Selection {
            anchor: m.start,
            head: m.end,
        };
        document.selections.push(new_selection);
        document.occurrence_selections.push(Selection {
            anchor: m.start,
            head: m.end,
        });
    }
}

/// Adds all occurrences of the word under the primary cursor as selections.
pub fn add_all_matches(document: &mut Document) {
    let primary = primary_selection(document);
    let rope = &document.rope;

    // Determine the query from the word under cursor
    let query_range = word_at_selection(rope, primary);
    let query = if let Some((start, end)) = query_range {
        let text = rope.byte_slice(start..end).to_string();
        if text.is_empty() {
            return;
        }
        text
    } else {
        return;
    };

    // Clear existing additional selections, keep primary
    document.selections = vec![primary];
    document.occurrence_selections.clear();

    // Initialize with primary word
    if let Some((start, end)) = query_range {
        document.occurrence_selections.push(Selection {
            anchor: start,
            head: end,
        });
        document.multi_cursor_query = Some(query.clone());
    }

    // Find all matches
    let options = search::SearchOptions {
        case_sensitive: false,
        whole_word: true,
        use_regex: false,
    };
    let matches = search::find_matches(rope, &query, options);

    let primary_range = query_range.unwrap();

    // Add all matches as selections, excluding the primary occurrence already present.
    for m in &matches {
        if (m.start, m.end) == primary_range {
            continue;
        }
        let sel = Selection {
            anchor: m.start,
            head: m.end,
        };
        document.selections.push(sel);
        document.occurrence_selections.push(sel);
    }
}

/// Splits the current selections into lines, creating a cursor at the start of each line.
pub fn split_selection_into_lines(document: &mut Document) {
    let selections = document.selections.clone();
    let rope = &document.rope;
    let mut new_selections = Vec::new();

    for selection in &selections {
        let (start, end) = if selection.anchor <= selection.head {
            (selection.anchor, selection.head)
        } else {
            (selection.head, selection.anchor)
        };

        let first_line = line_index_of_byte(rope, start);
        let mut last_line = line_index_of_byte(rope, end);
        if end > start && end == byte_of_visual_line(rope, last_line) {
            last_line = last_line.saturating_sub(1);
        }

        for line in first_line..=last_line {
            let line_start = byte_of_visual_line(rope, line);
            new_selections.push(Selection::caret(line_start));
        }
    }

    if !new_selections.is_empty() {
        document.selections = new_selections;
    }
}

/// Adds a cursor on the adjacent line at the same visual column.
pub fn add_cursor_vertical(document: &mut Document, delta: isize) -> bool {
    if document.selections.is_empty() || delta == 0 {
        return false;
    }

    let rope = &document.rope;
    let line_count = visual_line_count(rope);
    let edge_selection = document
        .selections
        .iter()
        .copied()
        .max_by_key(|selection| {
            let line = line_index_of_byte(rope, selection.head);
            if delta > 0 {
                line
            } else {
                line_count.saturating_sub(line)
            }
        })
        .unwrap_or_else(|| primary_selection(document));

    let current_line = line_index_of_byte(rope, edge_selection.head);
    let Some(target_line) = current_line.checked_add_signed(delta) else {
        return false;
    };
    if target_line >= line_count {
        return false;
    }

    let column = column_of_byte(rope, edge_selection.head);
    let target = byte_for_line_column(rope, target_line, column);
    let selection = Selection::caret(target);
    if document.selections.contains(&selection) {
        return false;
    }

    document.selections.push(selection);
    document
        .selections
        .sort_by_key(|selection| (line_index_of_byte(rope, selection.head), selection.head));
    document.occurrence_selections.clear();
    document.multi_cursor_query = None;
    true
}

/// Selects all occurrences of the word under the cursor for multi-cursor editing.
#[allow(dead_code)]
pub fn select_all_occurrences(document: &mut Document) {
    let primary = primary_selection(document);
    let rope = &document.rope;

    let (start, end) = if let Some((s, e)) = word_at_selection(rope, primary) {
        (s, e)
    } else {
        return;
    };

    let query = rope.byte_slice(start..end).to_string();
    if query.is_empty() {
        return;
    }

    document.occurrence_selections.clear();
    document.multi_cursor_query = Some(query.clone());

    let options = search::SearchOptions {
        case_sensitive: false,
        whole_word: true,
        use_regex: false,
    };
    let matches = search::find_matches(rope, &query, options);

    // Keep primary selection and add all matches
    document.selections = vec![primary];
    document
        .selections
        .extend(matches.iter().map(|m| Selection {
            anchor: m.start,
            head: m.end,
        }));
    document
        .occurrence_selections
        .extend(matches.iter().map(|m| Selection {
            anchor: m.start,
            head: m.end,
        }));
}

/// Clears selected text ranges before falling back to clearing secondary cursors.
pub fn clear_secondary_cursors(document: &mut Document) -> bool {
    if document
        .selections
        .iter()
        .any(|selection| selection.anchor != selection.head)
    {
        for selection in &mut document.selections {
            if selection.anchor != selection.head {
                *selection = Selection::caret(selection.head);
            }
        }
        document.occurrence_selections.clear();
        document.multi_cursor_query = None;
        return true;
    }

    let changed = document.selections.len() > 1
        || !document.occurrence_selections.is_empty()
        || document.multi_cursor_query.is_some();

    if document.selections.len() > 1 {
        let primary = document.selections[0];
        document.selections = vec![primary];
    }
    document.occurrence_selections.clear();
    document.multi_cursor_query = None;

    changed
}

/// Applies a text replacement to all selections (for multi-cursor editing).
/// Returns true if any changes were made.
pub fn replace_selection_all(document: &mut Document, text: &str) -> bool {
    if document.selections.is_empty() {
        return false;
    }
    document.validate();

    let mut sorted_selections = document.selections.clone();
    sorted_selections.sort_by_key(|s| s.anchor.min(s.head));

    let mut edits = Vec::new();

    for selection in &sorted_selections {
        let (start, end) = if selection.anchor <= selection.head {
            (selection.anchor, selection.head)
        } else {
            (selection.head, selection.anchor)
        };

        if start == end && text.is_empty() {
            continue;
        }

        edits.push(DocumentEdit {
            range: start..end,
            inserted_text: text.to_owned(),
            selections_before: vec![*selection],
            selections_after: vec![Selection::caret(start + text.len())],
        });
    }

    if !edits.is_empty() {
        document.apply_multi_edit(edits);
        true
    } else {
        false
    }
}

/// Handles backspace across all selections.
pub fn backspace_all(document: &mut Document) -> bool {
    if document.selections.is_empty() {
        return false;
    }
    document.validate();

    let mut sorted_selections = document.selections.clone();
    sorted_selections.sort_by_key(|s| s.anchor.min(s.head));

    let mut edits = Vec::new();

    for selection in &sorted_selections {
        let (start, end) = if selection.anchor <= selection.head {
            (selection.anchor, selection.head)
        } else {
            (selection.head, selection.anchor)
        };

        if start != end {
            // Delete selection
            edits.push(DocumentEdit {
                range: start..end,
                inserted_text: String::new(),
                selections_before: vec![*selection],
                selections_after: vec![Selection::caret(start)],
            });
        } else if start > 0 {
            // Delete previous grapheme
            let delete_start = previous_grapheme_boundary(&document.rope, start);
            edits.push(DocumentEdit {
                range: delete_start..start,
                inserted_text: String::new(),
                selections_before: vec![*selection],
                selections_after: vec![Selection::caret(delete_start)],
            });
        }
    }

    if !edits.is_empty() {
        document.apply_multi_edit(edits);
        true
    } else {
        false
    }
}

/// Handles word-backspace across all selections.
pub fn backspace_word_all(document: &mut Document) -> bool {
    if document.selections.is_empty() {
        return false;
    }
    document.validate();

    let mut sorted_selections = document.selections.clone();
    sorted_selections.sort_by_key(|s| s.anchor.min(s.head));

    let mut edits = Vec::new();

    for selection in &sorted_selections {
        let (start, end) = if selection.anchor <= selection.head {
            (selection.anchor, selection.head)
        } else {
            (selection.head, selection.anchor)
        };

        if start != end {
            edits.push(DocumentEdit {
                range: start..end,
                inserted_text: String::new(),
                selections_before: vec![*selection],
                selections_after: vec![Selection::caret(start)],
            });
        } else if start > 0 {
            let delete_start = previous_word_boundary(&document.rope, start);
            edits.push(DocumentEdit {
                range: delete_start..start,
                inserted_text: String::new(),
                selections_before: vec![*selection],
                selections_after: vec![Selection::caret(delete_start)],
            });
        }
    }

    if !edits.is_empty() {
        document.apply_multi_edit(edits);
        true
    } else {
        false
    }
}

/// Handles delete key across all selections.
pub fn delete_all(document: &mut Document) -> bool {
    if document.selections.is_empty() {
        return false;
    }
    document.validate();

    // Process selections in order
    let mut sorted_selections = document.selections.clone();
    sorted_selections.sort_by_key(|s| s.anchor.min(s.head));

    let mut edits = Vec::new();

    for selection in &sorted_selections {
        let (start, end) = if selection.anchor <= selection.head {
            (selection.anchor, selection.head)
        } else {
            (selection.head, selection.anchor)
        };

        if start != end {
            // Delete selection
            edits.push(DocumentEdit {
                range: start..end,
                inserted_text: String::new(),
                selections_before: vec![*selection],
                selections_after: vec![Selection::caret(start)],
            });
        } else if start < document.rope.byte_len() {
            // Delete next grapheme
            let delete_end = next_grapheme_boundary(&document.rope, start);
            edits.push(DocumentEdit {
                range: start..delete_end,
                inserted_text: String::new(),
                selections_before: vec![*selection],
                selections_after: vec![Selection::caret(start)],
            });
        }
    }

    if !edits.is_empty() {
        document.apply_multi_edit(edits);
        true
    } else {
        false
    }
}

/// Handles word-delete across all selections.
pub fn delete_word_all(document: &mut Document) -> bool {
    if document.selections.is_empty() {
        return false;
    }
    document.validate();

    let mut sorted_selections = document.selections.clone();
    sorted_selections.sort_by_key(|s| s.anchor.min(s.head));

    let mut edits = Vec::new();

    for selection in &sorted_selections {
        let (start, end) = if selection.anchor <= selection.head {
            (selection.anchor, selection.head)
        } else {
            (selection.head, selection.anchor)
        };

        if start != end {
            edits.push(DocumentEdit {
                range: start..end,
                inserted_text: String::new(),
                selections_before: vec![*selection],
                selections_after: vec![Selection::caret(start)],
            });
        } else if start < document.rope.byte_len() {
            let delete_end = next_word_boundary(&document.rope, start);
            edits.push(DocumentEdit {
                range: start..delete_end,
                inserted_text: String::new(),
                selections_before: vec![*selection],
                selections_after: vec![Selection::caret(start)],
            });
        }
    }

    if !edits.is_empty() {
        document.apply_multi_edit(edits);
        true
    } else {
        false
    }
}

fn word_at_selection(rope: &Rope, selection: Selection) -> Option<(usize, usize)> {
    let (start, end) = if selection.anchor <= selection.head {
        (selection.anchor, selection.head)
    } else {
        (selection.head, selection.anchor)
    };

    if start != end {
        return Some((start, end));
    }

    let offset = start;
    if offset >= rope.byte_len() {
        return None;
    }

    let char_at_caret = rope.byte_slice(offset..).chars().next();
    let char_at_caret = char_at_caret?;

    if !is_word_char(char_at_caret) {
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
            Some(c) if is_word_char(c) => {
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
        if is_word_char(c) {
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

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
