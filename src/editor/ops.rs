use crop::Rope;

use crate::model::{Document, DocumentEdit, Selection};
use crate::syntax_highlighting::DocumentSyntaxState;

use super::{
    byte_of_visual_line, clamp_primary_selection, line_index_of_byte, next_grapheme_boundary,
    previous_grapheme_boundary, primary_selection, selection_range,
    set_primary_selection,
};

pub fn replace_selection_with(document: &mut Document, text: &str) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    if start == end && text.is_empty() {
        return false;
    }

    let edit = DocumentEdit::replace_selection(selection, start..end, text);
    document.apply_continuing_edit(edit);
    true
}

pub(super) fn record_full_document_undo(
    document: &mut Document,
    original_text: String,
    selection_before: Selection,
) {
    document.record_full_document_replacement(original_text, selection_before);
}

fn leading_whitespace(rope: &Rope, line_start: usize, limit: usize) -> String {
    let mut indent = String::new();
    for char in rope.byte_slice(line_start..limit).chars() {
        if char == ' ' || char == '\t' {
            indent.push(char);
        } else {
            break;
        }
    }
    indent
}

pub fn insert_newline_with_auto_indent(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, _) = selection_range(selection);
    let line = line_index_of_byte(&document.rope, start);
    let line_start = byte_of_visual_line(&document.rope, line);
    
    // Try syntax-aware indentation first
    let indent = if document.syntax_state.parsed_as().map_or(false, |l| l.has_tree_sitter()) {
        let text = document.rope.byte_slice(..).to_string();
        document.syntax_state
            .indentation_at(&text, start, document.tab_width, document.use_soft_tabs)
            .unwrap_or_else(|| leading_whitespace(&document.rope, line_start, start))
    } else {
        leading_whitespace(&document.rope, line_start, start)
    };
    
    let text = format!("\n{indent}");

    replace_selection_with(document, &text)
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

    let delete_start = previous_grapheme_boundary(&document.rope, start);
    let edit = DocumentEdit::replace_selection(selection, delete_start..start, "");
    document.apply_continuing_edit(edit);
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

    let delete_end = next_grapheme_boundary(&document.rope, start);
    let edit = DocumentEdit::replace_selection(selection, start..delete_end, "");
    document.apply_continuing_edit(edit);
    true
}

pub fn convert_case_selection(document: &mut Document, case_type: CaseType) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    if start == end {
        return false;
    }

    let text = document.rope.byte_slice(start..end).to_string();
    let converted = match case_type {
        CaseType::Upper => text.to_uppercase(),
        CaseType::Lower => text.to_lowercase(),
        CaseType::Title => title_case(&text),
    };

    if converted == text {
        return false;
    }

    let edit = DocumentEdit::replace_selection(selection, start..end, &converted);
    document.apply_grouped_edit(edit);
    true
}

pub fn convert_case_all_selections(document: &mut Document, case_type: CaseType) -> bool {
    if document.selections.is_empty() {
        return false;
    }

    let selections: Vec<Selection> = document.selections.clone();
    let mut edits = Vec::new();
    let mut offset: isize = 0;

    for selection in selections.iter() {
        let (start, end) = selection_range(*selection);
        let adjusted_start = (start as isize + offset) as usize;
        let adjusted_end = (end as isize + offset) as usize;

        if adjusted_start == adjusted_end {
            continue;
        }

        let text = document.rope.byte_slice(adjusted_start..adjusted_end).to_string();
        let converted = match case_type {
            CaseType::Upper => text.to_uppercase(),
            CaseType::Lower => text.to_lowercase(),
            CaseType::Title => title_case(&text),
        };

        if converted == text {
            continue;
        }

        edits.push(DocumentEdit {
            range: adjusted_start..adjusted_end,
            inserted_text: converted.clone(),
            selections_before: vec![*selection],
            selections_after: vec![Selection::caret(adjusted_start + converted.len())],
        });

        offset += converted.len() as isize - (adjusted_end as isize - adjusted_start as isize);
    }

    if edits.is_empty() {
        return false;
    }

    document.apply_multi_edit(edits);
    true
}

pub enum CaseType {
    Upper,
    Lower,
    Title,
}

fn title_case(text: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for ch in text.chars() {
        if ch.is_whitespace() {
            result.push(ch);
            capitalize_next = true;
        } else if capitalize_next {
            result.push_str(&ch.to_uppercase().to_string());
            capitalize_next = false;
        } else {
            result.push_str(&ch.to_lowercase().to_string());
        }
    }

    result
}

pub fn insert_char_with_pairing(document: &mut Document, ch: char) -> bool {
    let pairing = match ch {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '<' => Some('>'),
        _ => None,
    };

    if let Some(close) = pairing {
        // Check if we're inside a string or comment - if so, don't auto-pair
        let syntax_state = &document.syntax_state;
        let selection = primary_selection(document);
        let (start, _) = selection_range(selection);
        let text = document.rope.byte_slice(..).to_string();
        
        if syntax_state.parsed_as().map_or(false, |l| l.has_tree_sitter()) {
            if syntax_state.is_inside_string(&text, start) || syntax_state.is_inside_comment(&text, start) {
                // Inside string or comment - just insert the character normally
                return replace_selection_with(document, &ch.to_string());
            }
        }

        // Auto-close: insert opening and closing pair
        let selection = primary_selection(document);
        let (start, end) = selection_range(selection);
        let text = format!("{ch}{close}");
        let edit = DocumentEdit {
            range: start..end,
            inserted_text: text,
            selections_before: vec![selection],
            selections_after: vec![Selection::caret(start + ch.len_utf8())],
        };
        document.apply_continuing_edit(edit);
        true
    } else if is_closing_bracket(ch) {
        // Skip-over: if next char is the same closing bracket, just move past it
        let selection = primary_selection(document);
        let (start, _) = selection_range(selection);
        if start < document.rope.byte_len() {
            let next_char = document.rope.byte_slice(start..).chars().next();
            if next_char == Some(ch) {
                let new_pos = start + ch.len_utf8();
                set_primary_selection(document, Selection::caret(new_pos));
                return true;
            }
        }
        // Otherwise insert normally
        replace_selection_with(document, &ch.to_string())
    } else {
        replace_selection_with(document, &ch.to_string())
    }
}

pub fn backspace_with_pair_deletion(document: &mut Document) -> bool {
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    // If it's a selection, just delete normally
    if start != end {
        return replace_selection_with(document, "");
    }

    if start == 0 {
        return false;
    }

    // Check if we're inside a string or comment - if so, don't do pair deletion
    let syntax_state = &document.syntax_state;
    if syntax_state.parsed_as().map_or(false, |l| l.has_tree_sitter()) {
        let text = document.rope.byte_slice(..).to_string();
        if syntax_state.is_inside_string(&text, start) || syntax_state.is_inside_comment(&text, start) {
            return backspace(document);
        }
    }

    // Check if we're backspacing over an opening bracket/quote
    let char_before = document.rope.byte_slice(..start).chars().next_back();
    if let Some(ch) = char_before {
        let pairing = match ch {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '<' => Some('>'),
            _ => None,
        };

        if let Some(close) = pairing {
            // Check if the next char after the opening is the matching closing
            let close_len = close.len_utf8();
            let ch_len = ch.len_utf8();
            if start + ch_len < document.rope.byte_len() {
                let next_char = document.rope.byte_slice((start + ch_len)..).chars().next();
                if next_char == Some(close) {
                    // Delete both the opening and closing brackets
                    let delete_end = start + ch_len + close_len;
                    let edit = DocumentEdit::replace_selection(
                        selection,
                        start..delete_end,
                        "",
                    );
                    document.apply_continuing_edit(edit);
                    return true;
                }
            }
        }
    }

    // Otherwise backspace normally
    backspace(document)
}

fn is_closing_bracket(ch: char) -> bool {
    ch == ')' || ch == ']' || ch == '}' || ch == '"' || ch == '\'' || ch == '>'
}
