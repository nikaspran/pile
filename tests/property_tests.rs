//! Property tests for rope edits and selection transformations.
//!
//! These tests use property-based testing to verify invariants of
//! rope operations and selection transformations.

use crop::Rope;
use pile::DocumentEdit;
use pile::model::{Document, Selection};
use proptest::prelude::*;

/// Find the nearest character boundary at or before the given byte position.
fn floor_char_boundary(s: &str, mut pos: usize) -> usize {
    pos = pos.min(s.len());
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

proptest! {
    /// Property: After inserting text at any position, the text length increases by the insertion length.
    #[test]
    fn prop_rope_insert_length_increases(s in "\\PC*", insert in "\\PC*") {
        let mut rope = Rope::from(s.as_str());
        let pos = floor_char_boundary(&s, s.len() / 2);
        let old_len = rope.byte_len();

        rope.insert(pos, &insert);

        prop_assert_eq!(rope.byte_len(), old_len + insert.len());
    }

    /// Property: After deleting a range, the text length decreases by the range size.
    #[test]
    fn prop_rope_delete_length_decreases(s in "\\PC*") {
        let len = s.len();
        if len < 2 {
            return Ok(());
        }
        let mut rope = Rope::from(s.as_str());
        let start = 0;
        let end = floor_char_boundary(&s, len / 2);

        rope.delete(start..end);

        prop_assert!(rope.byte_len() <= len);
        prop_assert_eq!(rope.byte_len(), len - (end - start));
    }

    /// Property: DocumentEdit with replace_selection produces correct result.
    #[test]
    fn prop_document_edit_replaces_correctly(s in "\\PC*", insert in "\\PC*") {
        if s.is_empty() {
            return Ok(());
        }
        let mut doc = Document::new_untitled(1, 4, true);
        doc.rope = Rope::from(s.as_str());

        let start = 0;
        let end = floor_char_boundary(&s, s.len() / 2);
        let selection = Selection { anchor: start, head: end };

        let edit = DocumentEdit::replace_selection(selection, start..end, &insert);
        doc.apply_edit(edit);

        let result = doc.text();
        prop_assert!(result.starts_with(&insert));
    }

    /// Property: Selection anchor <= head implies valid selection.
    #[test]
    fn prop_selection_order_preserved(anchor in 0usize..1000, head in 0usize..1000) {
        let selection = if anchor <= head {
            Selection { anchor, head }
        } else {
            Selection { anchor: head, head: anchor }
        };

        let min = selection.anchor.min(selection.head);
        let max = selection.anchor.max(selection.head);
        prop_assert!(min <= max);
    }

    /// Property: Applying and undoing an edit restores the original text.
    #[test]
    fn prop_undo_restores_original(s in "\\PC*", insert in "\\PC*") {
        if s.is_empty() {
            return Ok(());
        }
        let mut doc = Document::new_untitled(1, 4, true);
        doc.rope = Rope::from(s.as_str());

        let original_text = doc.text();
        let selection = Selection { anchor: 0, head: s.len() };

        let edit = DocumentEdit::replace_selection(selection, 0..s.len(), &insert);
        doc.apply_edit(edit);

        // Undo should restore original
        let undone = doc.undo();
        prop_assert!(undone);
        prop_assert_eq!(doc.text(), original_text);
    }

    /// Property: Rope byte_len is consistent with to_string().len().
    #[test]
    fn prop_rope_byte_len_consistent(s in "\\PC*") {
        let rope = Rope::from(s.as_str());
        prop_assert_eq!(rope.byte_len(), s.len());
        prop_assert_eq!(rope.to_string().len(), s.len());
    }

    /// Property: After editing, the document revision increases.
    #[test]
    fn prop_revision_changes(s in "\\PC*", insert in "\\PC*") {
        if s.is_empty() || insert.is_empty() {
            return Ok(());
        }
        let mut doc = Document::new_untitled(1, 4, true);
        doc.rope = Rope::from(s.as_str());
        let initial_revision = doc.revision;

        let selection = Selection { anchor: 0, head: s.len() };
        let edit = DocumentEdit::replace_selection(selection, 0..s.len(), &insert);
        doc.apply_edit(edit);

        // Revision should increase after edit
        prop_assert!(doc.revision > initial_revision);
    }
}

/// Traditional tests for edge cases that proptest might not cover well.
mod traditional_tests {
    use super::*;

    #[test]
    fn test_empty_rope_insert() {
        let mut rope = Rope::from("");
        rope.insert(0, "hello");
        assert_eq!(rope.to_string(), "hello");
    }

    #[test]
    fn test_empty_rope_delete() {
        let mut rope = Rope::from("");
        // Deleting empty range should be a no-op
        rope.delete(0..0);
        assert_eq!(rope.to_string(), "");
    }

    #[test]
    fn test_selection_with_equal_anchor_head() {
        let sel = Selection { anchor: 5, head: 5 };
        assert_eq!(sel.anchor, sel.head);
        assert_eq!(sel.anchor.min(sel.head), 5);
    }

    #[test]
    fn test_document_edit_empty_replacement() {
        let mut doc = Document::new_untitled(1, 4, true);
        doc.rope = Rope::from("hello");
        let selection = Selection { anchor: 0, head: 5 };

        let edit = DocumentEdit::replace_selection(selection, 0..5, "");
        doc.apply_edit(edit);

        assert_eq!(doc.text(), "");
    }

    #[test]
    fn test_rope_insert_at_end() {
        let mut rope = Rope::from("hello");
        rope.insert(rope.byte_len(), " world");
        assert_eq!(rope.to_string(), "hello world");
    }

    #[test]
    fn test_rope_delete_entire_content() {
        let mut rope = Rope::from("hello");
        let len = rope.byte_len();
        rope.delete(0..len);
        assert_eq!(rope.to_string(), "");
    }
}
