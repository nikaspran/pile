use crop::Rope;

use crate::model::{Document, DocumentEdit, Selection};

use super::{
    byte_of_visual_line, clamp_primary_selection, line_index_of_byte, next_grapheme_boundary,
    previous_grapheme_boundary, primary_selection, selection_range,
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
    let indent = leading_whitespace(&document.rope, line_start, start);
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
