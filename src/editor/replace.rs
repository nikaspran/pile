use regex::Regex;

use crate::{
    model::{Document, DocumentEdit, Selection},
    search::SearchMatch,
};

use super::{primary_selection, record_full_document_undo, set_primary_selection};

pub fn replace_match(
    document: &mut Document,
    search_match: SearchMatch,
    replacement: &str,
    regex: Option<&Regex>,
) -> usize {
    let SearchMatch { start, end } = search_match;
    if end > document.rope.byte_len() || start > end {
        return start.min(document.rope.byte_len());
    }

    let inserted_text = if let Some(regex) = regex {
        let matched_text = document.rope.byte_slice(start..end).to_string();
        regex.replace(&matched_text, replacement).to_string()
    } else {
        replacement.to_owned()
    };

    let selection_before = primary_selection(document);
    let caret = start + inserted_text.len();
    document.apply_grouped_edit(DocumentEdit {
        range: start..end,
        inserted_text,
        selections_before: vec![selection_before],
        selections_after: vec![Selection::caret(caret)],
    });
    caret
}

pub fn replace_all_matches(
    document: &mut Document,
    matches: &[SearchMatch],
    replacement: &str,
    regex: Option<&Regex>,
) -> usize {
    if matches.is_empty() {
        return 0;
    }

    let original_text = document.text();
    let selection_before = primary_selection(document);
    let mut count = 0;

    if let Some(regex) = regex {
        for search_match in matches.iter().rev() {
            let SearchMatch { start, end } = *search_match;

            if end > document.rope.byte_len() || start > end {
                continue;
            }

            let matched_text = document.rope.byte_slice(start..end).to_string();
            let inserted_text = regex.replace(&matched_text, replacement).to_string();

            if start != end {
                document.rope.delete(start..end);
            }
            if !inserted_text.is_empty() {
                document.rope.insert(start, &inserted_text);
            }
            count += 1;
        }
    } else {
        // Non-regex: process in reverse order using original positions
        for search_match in matches.iter().rev() {
            let SearchMatch { start, end } = *search_match;

            if end > document.rope.byte_len() || start > end {
                continue;
            }

            if start != end {
                document.rope.delete(start..end);
            }
            if !replacement.is_empty() {
                document.rope.insert(start, replacement);
            }
            count += 1;
        }
    }

    if count > 0 {
        let first_start = matches.first().map(|m| m.start).unwrap_or(0);
        let caret = first_start + replacement.len();
        let caret = caret.min(document.rope.byte_len());

        record_full_document_undo(document, original_text, selection_before);

        set_primary_selection(document, Selection::caret(caret));
        document.revision += 1;
    }

    count
}
