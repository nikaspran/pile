use super::*;
use crate::syntax::LanguageId;
use uuid::Uuid;

#[test]
fn opens_and_closes_scratch_documents_without_losing_last_buffer() {
    let mut state = AppState::empty();
    let first = state.active_document;
    let second = state.open_untitled(4, true);

    assert_ne!(first, second);
    assert_eq!(state.documents.len(), 2);
    assert_eq!(state.active_document, second);

    state.close_active(4, true);

    assert_eq!(state.documents.len(), 1);
    assert_eq!(state.active_document, first);
    assert_eq!(state.closed_documents().len(), 1);

    // Closing the last document creates a new scratch instead of clearing text
    state.close_active(4, true);

    assert_eq!(state.documents.len(), 1);
    assert_ne!(state.active_document, first);
    assert_eq!(state.closed_documents().len(), 2);
}

#[test]
fn set_active_ignores_unknown_documents() {
    let mut state = AppState::empty();
    let active = state.active_document;

    assert!(!state.set_active(Uuid::new_v4()));
    assert_eq!(state.active_document, active);

    let second = state.open_untitled(4, true);
    assert!(state.set_active(active));
    assert_eq!(state.active_document, active);
    assert!(state.set_active(second));
    assert_eq!(state.active_document, second);
}

#[test]
fn close_active_selects_other_most_recent_document() {
    let mut state = AppState::empty();
    let first = state.active_document;
    let second = state.open_untitled(4, true);
    let third = state.open_untitled(4, true);

    assert!(state.set_active(second));
    assert!(state.set_active(third));
    assert!(state.set_active(first));

    state.close_active(4, true);

    assert_eq!(state.active_document, third);
    assert_eq!(state.recent_order(), &[third, second]);
}

#[test]
fn document_title_tracks_first_non_empty_line_until_renamed() {
    let mut document = Document::new_untitled(1, 4, true);
    assert_eq!(document.display_title(), "Untitled");

    document.replace_text("\n  First real line  \nSecond line");
    assert_eq!(document.display_title(), "First real line");

    document.rename("Manual title");
    assert_eq!(document.display_title(), "Manual title");

    document.replace_text("Different first line");
    assert_eq!(document.display_title(), "Manual title");

    document.rename("");
    assert_eq!(document.display_title(), "Different first line");
}

#[test]
fn document_syntax_override_wins_over_auto_detection() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("fn main() { let value = 1; }");

    assert_eq!(document.detect_syntax().unwrap().language, LanguageId::Rust);

    document.syntax_override = Some(LanguageId::Markdown);
    let detection = document.detect_syntax().unwrap();

    assert_eq!(detection.language, LanguageId::Markdown);
    assert_eq!(detection.confidence, 1.0);
}

#[test]
fn document_edit_replaces_range_and_records_undo() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello world");
    document.revision = 0;
    let selection = Selection {
        anchor: 6,
        head: 11,
    };

    document.apply_grouped_edit(DocumentEdit::replace_selection(selection, 6..11, "pile"));

    assert_eq!(document.text(), "hello pile");
    assert_eq!(document.selections, vec![Selection::caret(10)]);
    assert_eq!(document.revision, 1);

    assert!(document.undo());
    assert_eq!(document.text(), "hello world");
    assert_eq!(document.selections, vec![selection]);
}

#[test]
fn continuing_edits_share_undo_group_until_committed() {
    let mut document = Document::new_untitled(1, 4, true);

    document.apply_continuing_edit(DocumentEdit::replace_selection(
        Selection::caret(0),
        0..0,
        "a",
    ));
    document.apply_continuing_edit(DocumentEdit::replace_selection(
        Selection::caret(1),
        1..1,
        "b",
    ));
    document.commit_undo_group();

    assert_eq!(document.text(), "ab");
    assert!(document.undo());
    assert_eq!(document.text(), "");
}

#[test]
fn full_document_replacement_records_single_undo_step() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("one\ntwo");
    document.revision = 0;
    let original = document.text();
    let selection = Selection::caret(0);

    document.rope.delete(0..document.rope.byte_len());
    document.rope.insert(0, "two\none");
    document.record_full_document_replacement(original, selection);
    document.revision += 1;

    assert_eq!(document.text(), "two\none");
    assert!(document.undo());
    assert_eq!(document.text(), "one\ntwo");
    assert_eq!(document.selections, vec![selection]);
}

#[test]
fn undo_state_groups_typing_into_single_step() {
    let mut undo = UndoState::default();

    undo.begin_group();
    undo.record(EditTransaction {
        start: 0,
        end: 0,
        deleted_text: String::new(),
        inserted_text: "a".to_owned(),
        selections_before: vec![Selection::caret(0)],
    });
    undo.begin_group();
    undo.record(EditTransaction {
        start: 1,
        end: 1,
        deleted_text: String::new(),
        inserted_text: "b".to_owned(),
        selections_before: vec![Selection::caret(1)],
    });
    undo.commit_group();

    let group = undo.undo().unwrap();
    assert_eq!(group.len(), 2);
}

#[test]
fn undo_state_begins_new_group_commits_previous_typing() {
    let mut undo = UndoState::default();

    undo.begin_group();
    undo.record(EditTransaction {
        start: 0,
        end: 0,
        deleted_text: String::new(),
        inserted_text: "a".to_owned(),
        selections_before: vec![Selection::caret(0)],
    });

    // commit_and_start_new_group for a discrete operation should commit the typing group
    undo.commit_and_start_new_group();
    undo.record(EditTransaction {
        start: 0,
        end: 0,
        deleted_text: String::new(),
        inserted_text: "b".to_owned(),
        selections_before: vec![Selection::caret(0)],
    });
    undo.commit_group();

    // Two separate undo steps: "b" and "a"
    assert!(undo.undo().is_some());
    assert!(undo.undo().is_some());
    assert!(undo.undo().is_none());
}

#[test]
fn undo_state_clears_redo_on_new_edit() {
    let mut undo = UndoState::default();

    undo.begin_group();
    undo.record(EditTransaction {
        start: 0,
        end: 0,
        deleted_text: String::new(),
        inserted_text: "hello".to_owned(),
        selections_before: vec![Selection::caret(0)],
    });
    undo.commit_group();

    undo.undo();
    assert!(undo.can_redo());

    undo.begin_group();
    undo.record(EditTransaction {
        start: 0,
        end: 0,
        deleted_text: String::new(),
        inserted_text: "world".to_owned(),
        selections_before: vec![Selection::caret(0)],
    });
    undo.commit_group();

    assert!(!undo.can_redo());
}

#[test]
fn undo_state_discard_group_clears_pending_typing() {
    let mut undo = UndoState::default();

    undo.begin_group();
    undo.record(EditTransaction {
        start: 0,
        end: 0,
        deleted_text: String::new(),
        inserted_text: "a".to_owned(),
        selections_before: vec![Selection::caret(0)],
    });

    undo.discard_group();
    assert!(!undo.can_undo());
}

#[test]
fn undo_state_clear_resets_all_stacks() {
    let mut undo = UndoState::default();

    undo.begin_group();
    undo.record(EditTransaction {
        start: 0,
        end: 0,
        deleted_text: String::new(),
        inserted_text: "hello".to_owned(),
        selections_before: vec![Selection::caret(0)],
    });
    undo.commit_group();
    undo.undo();

    undo.clear();
    assert!(!undo.can_undo());
    assert!(!undo.can_redo());
}

#[test]
fn multi_edit_creates_single_undo_group() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello world");
    document.revision = 0;

    // Simulate multi-cursor: replace "hello" and "world" with "hi" and "there"
    let edits = vec![
        DocumentEdit {
            range: 0..5,
            inserted_text: "hi".to_owned(),
            selections_before: vec![Selection::caret(0)],
            selections_after: vec![Selection::caret(2)],
        },
        DocumentEdit {
            range: 6..11,
            inserted_text: "there".to_owned(),
            selections_before: vec![Selection::caret(6)],
            selections_after: vec![Selection::caret(8)],
        },
    ];

    document.apply_multi_edit(edits);
    assert_eq!(document.text(), "hi there");
    assert_eq!(document.revision, 1);

    // Single undo should undo both changes
    assert!(document.undo());
    assert_eq!(document.text(), "hello world");
    assert_eq!(document.revision, 2);
}

#[test]
fn multi_edit_undo_single_edit() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");
    document.revision = 0;

    // Single edit via multi_edit
    let edits = vec![DocumentEdit {
        range: 0..5,
        inserted_text: "hi".to_owned(),
        selections_before: vec![Selection::caret(0)],
        selections_after: vec![Selection::caret(2)],
    }];

    document.apply_multi_edit(edits);
    assert_eq!(document.text(), "hi");

    // Undo should work
    assert!(document.undo());
    assert_eq!(document.text(), "hello");
}

#[test]
fn multi_edit_undo_restores_all_selections() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("a b c");
    document.revision = 0;

    let sel1 = Selection { anchor: 0, head: 1 };
    let sel2 = Selection { anchor: 2, head: 3 };
    let sel3 = Selection { anchor: 4, head: 5 };

    let edits = vec![
        DocumentEdit {
            range: 0..1,
            inserted_text: "x".to_owned(),
            selections_before: vec![sel1],
            selections_after: vec![Selection::caret(1)],
        },
        DocumentEdit {
            range: 2..3,
            inserted_text: "y".to_owned(),
            selections_before: vec![sel2],
            selections_after: vec![Selection::caret(3)],
        },
        DocumentEdit {
            range: 4..5,
            inserted_text: "z".to_owned(),
            selections_before: vec![sel3],
            selections_after: vec![Selection::caret(5)],
        },
    ];

    document.apply_multi_edit(edits);
    assert_eq!(document.text(), "x y z");

    // Undo should restore original selections
    assert!(document.undo());
    assert_eq!(document.text(), "a b c");
    assert_eq!(document.selections.len(), 1);
    assert_eq!(document.selections[0], sel1);
}

#[test]
fn validate_repairs_stale_tab_order() {
    let mut state = AppState::empty();
    let valid_id = state.active_document;
    let stale_id = Uuid::new_v4();

    state.tab_order.push(stale_id);
    state.tab_order.push(valid_id); // duplicate

    state.validate();

    assert_eq!(state.tab_order.len(), 1);
    assert_eq!(state.tab_order[0], valid_id);
}

#[test]
fn move_tab_to_index_reorders_tabs() {
    let mut state = AppState::empty();
    let first = state.active_document;
    let second = state.open_untitled(4, true);
    let third = state.open_untitled(4, true);

    assert!(state.move_tab_to_index(third, 0));
    assert_eq!(state.tab_order, vec![third, first, second]);

    assert!(state.move_tab_to_index(third, 2));
    assert_eq!(state.tab_order, vec![first, second, third]);
}

#[test]
fn move_tab_to_index_ignores_missing_or_unchanged_tabs() {
    let mut state = AppState::empty();
    let first = state.active_document;
    let second = state.open_untitled(4, true);

    assert!(!state.move_tab_to_index(Uuid::new_v4(), 0));
    assert!(!state.move_tab_to_index(first, 0));
    assert_eq!(state.tab_order, vec![first, second]);
}

#[test]
fn validate_fixes_missing_active_document() {
    let mut state = AppState::empty();
    state.active_document = Uuid::new_v4(); // missing

    state.validate();

    assert!(state.document(state.active_document).is_some());
    assert!(state.tab_order.contains(&state.active_document));
}

#[test]
fn validate_creates_document_when_empty() {
    let mut state = AppState {
        documents: vec![],
        tab_order: vec![],
        active_document: Uuid::new_v4(),
        next_untitled_index: 2,
        recent_order: vec![],
        closed_documents: vec![],
        next_closed_order: 0,
    };

    state.validate();

    assert!(!state.documents.is_empty());
    assert!(!state.tab_order.is_empty());
    assert!(state.document(state.active_document).is_some());
}

#[test]
fn document_validate_clamps_selections() {
    let mut doc = Document::new_untitled(1, 4, true);
    doc.replace_text("hello");
    let len = doc.rope.byte_len();

    doc.selections = vec![
        Selection {
            anchor: 0,
            head: len + 100,
        }, // out of bounds
        Selection {
            anchor: len + 50,
            head: len + 50,
        }, // out of bounds
    ];

    doc.validate();

    for sel in &doc.selections {
        assert!(sel.anchor <= len);
        assert!(sel.head <= len);
    }
}

#[test]
fn document_validate_fixes_scroll_and_tab_width() {
    let mut doc = Document::new_untitled(1, 4, true);

    doc.scroll = ScrollState { x: -5.0, y: -10.0 };
    doc.tab_width = 0;

    doc.validate();

    assert!(doc.scroll.x >= 0.0);
    assert!(doc.scroll.y >= 0.0);
    assert_eq!(doc.tab_width, 4);
}

// ============================================================================
// Multi-Cursor Editing Transaction Tests
// ============================================================================

#[test]
fn multi_edit_with_overlapping_ranges_fails_gracefully() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello world");
    document.revision = 0;

    // Overlapping ranges should still apply (in reverse order)
    let edits = vec![
        DocumentEdit {
            range: 0..5,
            inserted_text: "hi".to_owned(),
            selections_before: vec![Selection::caret(0)],
            selections_after: vec![Selection::caret(2)],
        },
        DocumentEdit {
            range: 3..8,
            inserted_text: "there".to_owned(),
            selections_before: vec![Selection::caret(3)],
            selections_after: vec![Selection::caret(8)],
        },
    ];

    // This should not panic - edits are applied in reverse order
    document.apply_multi_edit(edits);
    // The exact result depends on order, but should not crash
    assert!(document.revision >= 1);
}

#[test]
fn multi_edit_with_overlapping_full_deletion_does_not_panic() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("abc");
    document.revision = 0;

    let edits = vec![
        DocumentEdit {
            range: 0..3,
            inserted_text: String::new(),
            selections_before: vec![Selection { anchor: 0, head: 3 }],
            selections_after: vec![Selection::caret(0)],
        },
        DocumentEdit {
            range: 0..1,
            inserted_text: String::new(),
            selections_before: vec![Selection::caret(1)],
            selections_after: vec![Selection::caret(0)],
        },
    ];

    document.apply_multi_edit(edits);

    assert_eq!(document.text(), "");
    assert_eq!(document.selections, vec![Selection::caret(0)]);
}

#[test]
fn multi_edit_preserves_document_state_on_empty_edits() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");
    document.revision = 0;

    let edits = vec![];
    document.apply_multi_edit(edits);

    assert_eq!(document.text(), "hello");
    assert_eq!(document.revision, 0);
}

#[test]
fn multi_edit_with_adjacent_non_overlapping_ranges() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("abcdef");
    document.revision = 0;

    // Two adjacent edits: replace "ab" and "cd"
    let edits = vec![
        DocumentEdit {
            range: 0..2,
            inserted_text: "AB".to_owned(),
            selections_before: vec![Selection::caret(0)],
            selections_after: vec![Selection::caret(2)],
        },
        DocumentEdit {
            range: 2..4,
            inserted_text: "CD".to_owned(),
            selections_before: vec![Selection::caret(2)],
            selections_after: vec![Selection::caret(4)],
        },
    ];

    document.apply_multi_edit(edits);
    assert_eq!(document.text(), "ABCDef");
}

#[test]
fn multi_edit_uses_original_offsets_for_different_length_replacements() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("abc def ghi");
    document.revision = 0;

    let edits = vec![
        DocumentEdit {
            range: 0..3,
            inserted_text: "alpha".to_owned(),
            selections_before: vec![Selection::caret(0)],
            selections_after: vec![Selection::caret(5)],
        },
        DocumentEdit {
            range: 8..11,
            inserted_text: "x".to_owned(),
            selections_before: vec![Selection::caret(8)],
            selections_after: vec![Selection::caret(9)],
        },
    ];

    document.apply_multi_edit(edits);

    assert_eq!(document.text(), "alpha def x");
    assert_eq!(
        document.selections,
        vec![Selection::caret(5), Selection::caret(11)]
    );
    assert!(document.undo());
    assert_eq!(document.text(), "abc def ghi");
}

#[test]
fn multi_edit_undo_restores_all_selections_correctly() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("a b c d");
    document.revision = 0;

    let sel1 = Selection { anchor: 0, head: 1 };
    let sel2 = Selection { anchor: 2, head: 3 };
    let sel3 = Selection { anchor: 4, head: 5 };

    let edits = vec![
        DocumentEdit {
            range: 0..1,
            inserted_text: "X".to_owned(),
            selections_before: vec![sel1],
            selections_after: vec![Selection::caret(1)],
        },
        DocumentEdit {
            range: 2..3,
            inserted_text: "Y".to_owned(),
            selections_before: vec![sel2],
            selections_after: vec![Selection::caret(3)],
        },
        DocumentEdit {
            range: 4..5,
            inserted_text: "Z".to_owned(),
            selections_before: vec![sel3],
            selections_after: vec![Selection::caret(5)],
        },
    ];

    document.apply_multi_edit(edits);
    assert_eq!(document.text(), "X Y Z d");

    // Undo should restore original text and first selection
    assert!(document.undo());
    assert_eq!(document.text(), "a b c d");
    // After undo, selections should be restored to the first edit's selections_before
    assert_eq!(document.selections.len(), 1);
    assert_eq!(document.selections[0], sel1);
}

#[test]
fn multi_edit_with_insertion_only_no_deletion() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");
    document.revision = 0;

    // Insert at multiple positions (all at same point for simplicity)
    let edits = vec![DocumentEdit {
        range: 5..5,
        inserted_text: " world".to_owned(),
        selections_before: vec![Selection::caret(5)],
        selections_after: vec![Selection::caret(11)],
    }];

    document.apply_multi_edit(edits);
    assert_eq!(document.text(), "hello world");
    assert_eq!(document.revision, 1);

    assert!(document.undo());
    assert_eq!(document.text(), "hello");
}

#[test]
fn multi_edit_with_deletion_only_no_insertion() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello world");
    document.revision = 0;

    let edits = vec![DocumentEdit {
        range: 5..11,
        inserted_text: String::new(),
        selections_before: vec![Selection::caret(5)],
        selections_after: vec![Selection::caret(5)],
    }];

    document.apply_multi_edit(edits);
    assert_eq!(document.text(), "hello");
    assert_eq!(document.revision, 1);

    assert!(document.undo());
    assert_eq!(document.text(), "hello world");
}

#[test]
fn multi_edit_undo_then_redo_restores_state() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("foo bar baz");
    document.revision = 0;

    let edits = vec![
        DocumentEdit {
            range: 0..3,
            inserted_text: "FOO".to_owned(),
            selections_before: vec![Selection::caret(0)],
            selections_after: vec![Selection::caret(3)],
        },
        DocumentEdit {
            range: 4..7,
            inserted_text: "BAR".to_owned(),
            selections_before: vec![Selection::caret(4)],
            selections_after: vec![Selection::caret(7)],
        },
    ];

    document.apply_multi_edit(edits);
    assert_eq!(document.text(), "FOO BAR baz");

    // Undo
    assert!(document.undo());
    assert_eq!(document.text(), "foo bar baz");

    // Redo
    assert!(document.redo());
    assert_eq!(document.text(), "FOO BAR baz");
}

#[test]
fn multi_edit_with_multibyte_characters() {
    let mut document = Document::new_untitled(1, 4, true);
    // "a"=1, "é"=2, "日"=3, "b"=1 -> total 7 bytes
    // Byte positions: a=0..1, é=1..3, 日=3..6, b=6..7
    document.replace_text("aé日b");
    document.revision = 0;

    // Apply two single edits that each handle multibyte characters correctly
    // First: replace "日" (bytes 3..6) with "ri"
    let edit1 = DocumentEdit {
        range: 3..6,
        inserted_text: "ri".to_owned(),
        selections_before: vec![Selection::caret(3)],
        selections_after: vec![Selection::caret(5)],
    };
    document.apply_edit(edit1);
    // "aé" (3 bytes) + "ri" (2 bytes) + "b" (1 byte) = "aérib" (6 bytes)
    assert_eq!(document.text(), "aérib");

    // Second: replace "é" (bytes 1..3) with "e"
    let edit2 = DocumentEdit {
        range: 1..3,
        inserted_text: "e".to_owned(),
        selections_before: vec![Selection::caret(1)],
        selections_after: vec![Selection::caret(2)],
    };
    document.apply_edit(edit2);
    // "a" (1 byte) + "e" (1 byte) + "rib" (3 bytes) = "aerib" (5 bytes)
    assert_eq!(document.text(), "aerib");
}

// ============================================================================
// Multi-Cursor Selection Behavior Tests
// ============================================================================

#[test]
fn multiple_cursors_are_independent_after_edit() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("one\ntwo\nthree");
    document.revision = 0;

    // Set up multiple selections
    document.selections = vec![
        Selection { anchor: 0, head: 3 }, // "one"
        Selection { anchor: 4, head: 7 }, // "two"
    ];

    // Apply edit to first selection
    document.apply_edit(DocumentEdit {
        range: 0..3,
        inserted_text: "ONE".to_owned(),
        selections_before: vec![Selection { anchor: 0, head: 3 }],
        selections_after: vec![Selection::caret(3)],
    });

    // The selections should be updated by the edit
    assert_eq!(document.text(), "ONE\ntwo\nthree");
}

#[test]
fn primary_selection_is_first_in_vec() {
    let mut document = Document::new_untitled(1, 4, true);
    document.selections = vec![
        Selection { anchor: 0, head: 0 }, // primary
        Selection { anchor: 5, head: 5 }, // secondary
        Selection {
            anchor: 10,
            head: 10,
        }, // secondary
    ];

    // Primary selection is conventionally the first one
    assert_eq!(document.selections[0].anchor, 0);
    assert_eq!(document.selections.len(), 3);
}

#[test]
fn selections_are_clamped_on_validate() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");
    let len = document.rope.byte_len();

    // Add selections with out-of-bounds positions
    document.selections = vec![
        Selection {
            anchor: 0,
            head: len + 100,
        },
        Selection {
            anchor: len + 50,
            head: len + 50,
        },
        Selection { anchor: 2, head: 3 }, // valid
    ];

    document.validate();

    // All selections should be clamped to valid range
    for sel in &document.selections {
        assert!(sel.anchor <= len);
        assert!(sel.head <= len);
    }
}

#[test]
fn empty_selections_vec_gets_default_on_validate() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");
    let len = document.rope.byte_len();

    document.selections = vec![];
    document.validate();

    assert!(!document.selections.is_empty());
    assert_eq!(document.selections[0], Selection::caret(len));
}

#[test]
fn backward_selections_are_valid() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");

    // Backward selection (anchor > head)
    let backward = Selection { anchor: 4, head: 1 };
    document.selections = vec![backward];

    // Should be valid - backward selections are allowed
    document.validate();
    assert_eq!(document.selections.len(), 1);
    // The selection should still be backward after validation
    assert_eq!(document.selections[0].anchor, 4);
    assert_eq!(document.selections[0].head, 1);
}

// ============================================================================
// Undo/Redo Stack Behavior Tests
// ============================================================================

#[test]
fn undo_stack_depth_matches_edit_count() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("");
    document.revision = 0;

    // Apply 3 grouped edits
    document.apply_continuing_edit(DocumentEdit::replace_selection(
        Selection::caret(0),
        0..0,
        "a",
    ));
    document.apply_continuing_edit(DocumentEdit::replace_selection(
        Selection::caret(1),
        1..1,
        "b",
    ));
    document.apply_continuing_edit(DocumentEdit::replace_selection(
        Selection::caret(2),
        2..2,
        "c",
    ));
    document.commit_undo_group();

    assert_eq!(document.text(), "abc");

    // Single undo should undo all three
    assert!(document.undo());
    assert_eq!(document.text(), "");
}

#[test]
fn redo_stack_cleared_on_new_edit() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");
    document.revision = 0;

    // Make an edit and undo it
    document.apply_grouped_edit(DocumentEdit::replace_selection(
        Selection::caret(5),
        5..5,
        " world",
    ));
    assert!(document.undo());
    assert!(document.can_redo());

    // New edit should clear redo stack
    document.apply_grouped_edit(DocumentEdit::replace_selection(
        Selection::caret(5),
        5..5,
        "!",
    ));
    assert!(!document.can_redo());
}

#[test]
fn undo_state_tracks_multiple_undo_groups() {
    let mut undo = UndoState::default();

    // First group
    undo.begin_group();
    undo.record(EditTransaction {
        start: 0,
        end: 0,
        deleted_text: String::new(),
        inserted_text: "a".to_owned(),
        selections_before: vec![Selection::caret(0)],
    });
    undo.commit_group();

    // Second group
    undo.begin_group();
    undo.record(EditTransaction {
        start: 1,
        end: 1,
        deleted_text: String::new(),
        inserted_text: "b".to_owned(),
        selections_before: vec![Selection::caret(1)],
    });
    undo.commit_group();

    assert!(undo.can_undo());
    undo.undo(); // undoes "b"
    undo.undo(); // undoes "a"
    assert!(!undo.can_undo());
}

#[test]
fn interleaved_undo_redo_operations() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("");
    document.revision = 0;

    // Edit 1
    document.apply_grouped_edit(DocumentEdit::replace_selection(
        Selection::caret(0),
        0..0,
        "first",
    ));

    // Edit 2
    document.apply_grouped_edit(DocumentEdit::replace_selection(
        Selection::caret(5),
        5..5,
        " second",
    ));

    assert_eq!(document.text(), "first second");

    // Undo edit 2
    assert!(document.undo());
    assert_eq!(document.text(), "first");

    // Redo edit 2
    assert!(document.redo());
    assert_eq!(document.text(), "first second");

    // Undo both
    assert!(document.undo());
    assert_eq!(document.text(), "first");
    assert!(document.undo());
    assert_eq!(document.text(), "");
}

// ============================================================================
// Single Edit Transaction Tests
// ============================================================================

#[test]
fn single_edit_transaction_records_correct_bounds() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello world");
    document.revision = 0;

    let sel_before = Selection {
        anchor: 6,
        head: 11,
    };
    document.apply_grouped_edit(DocumentEdit {
        range: 6..11,
        inserted_text: "earth".to_owned(),
        selections_before: vec![sel_before],
        selections_after: vec![Selection::caret(11)],
    });

    // Undo and verify the transaction recorded correct deleted text
    assert!(document.undo());
    assert_eq!(document.text(), "hello world");
    assert_eq!(document.selections[0], sel_before);
}

#[test]
fn edit_at_document_start() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");
    document.revision = 0;

    document.apply_grouped_edit(DocumentEdit {
        range: 0..0,
        inserted_text: "++".to_owned(),
        selections_before: vec![Selection::caret(0)],
        selections_after: vec![Selection::caret(2)],
    });

    assert_eq!(document.text(), "++hello");
    assert_eq!(document.revision, 1);
}

#[test]
fn edit_at_document_end() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");
    let len = document.rope.byte_len();
    document.revision = 0;

    document.apply_grouped_edit(DocumentEdit {
        range: len..len,
        inserted_text: "++".to_owned(),
        selections_before: vec![Selection::caret(len)],
        selections_after: vec![Selection::caret(len + 2)],
    });

    assert_eq!(document.text(), "hello++");
}

#[test]
fn edit_replacing_entire_document() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("original text");
    document.revision = 0;

    let original_len = document.rope.byte_len();
    document.apply_grouped_edit(DocumentEdit {
        range: 0..original_len,
        inserted_text: "new text".to_owned(),
        selections_before: vec![Selection::caret(0)],
        selections_after: vec![Selection::caret(8)],
    });

    assert_eq!(document.text(), "new text");

    assert!(document.undo());
    assert_eq!(document.text(), "original text");
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
fn undo_when_nothing_to_undo() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");

    assert!(!document.undo());
    assert_eq!(document.text(), "hello");
}

#[test]
fn redo_when_nothing_to_redo() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");

    assert!(!document.redo());
    assert_eq!(document.text(), "hello");
}

#[test]
fn multi_edit_with_empty_selections_after() {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text("hello");
    document.revision = 0;

    let edits = vec![DocumentEdit {
        range: 0..5,
        inserted_text: "hi".to_owned(),
        selections_before: vec![Selection::caret(0)],
        selections_after: vec![], // empty selections after
    }];

    document.apply_multi_edit(edits);
    // Empty selection output is repaired to a valid caret.
    assert_eq!(document.selections, vec![Selection::caret(2)]);
}

#[test]
fn document_edit_replace_selection_helper() {
    let sel = Selection { anchor: 2, head: 5 };
    let edit = DocumentEdit::replace_selection(sel, 2..5, "new");

    assert_eq!(edit.range, 2..5);
    assert_eq!(edit.inserted_text, "new");
    assert_eq!(edit.selections_before, vec![sel]);
    assert_eq!(edit.selections_after, vec![Selection::caret(5)]);
}

#[test]
fn undo_state_is_typing_flag_management() {
    let mut undo = UndoState::default();

    assert!(!undo.is_typing);

    undo.begin_group();
    assert!(undo.is_typing);

    undo.commit_group();
    assert!(!undo.is_typing);

    undo.begin_group();
    assert!(undo.is_typing);

    undo.discard_group();
    assert!(!undo.is_typing);
}

#[test]
fn can_undo_respects_typing_group() {
    let mut undo = UndoState::default();

    assert!(!undo.can_undo());

    undo.begin_group();
    undo.record(EditTransaction {
        start: 0,
        end: 0,
        deleted_text: String::new(),
        inserted_text: "a".to_owned(),
        selections_before: vec![Selection::caret(0)],
    });

    assert!(undo.can_undo()); // typing group has edits

    undo.discard_group();
    assert!(!undo.can_undo()); // typing group discarded
}

#[test]
fn close_active_moves_document_to_closed_history() {
    let mut state = AppState::empty();
    // Open a second document
    let id2 = state.open_untitled(4, true);
    assert_eq!(state.documents.len(), 2);
    assert!(state.closed_documents().is_empty());

    // Close active (the second doc)
    state.close_active(4, true);
    assert_eq!(state.documents.len(), 1);
    assert_eq!(state.closed_documents().len(), 1);
    assert_eq!(state.closed_documents()[0].document.id, id2);

    // Close active again (now the first doc)
    state.close_active(4, true);
    assert_eq!(state.documents.len(), 1); // always at least 1
    assert_eq!(state.closed_documents().len(), 2); // first doc also moved to history
}

#[test]
fn close_document_by_id_moves_to_history() {
    let mut state = AppState::empty();
    let id2 = state.open_untitled(4, true);
    assert_eq!(state.documents.len(), 2);

    // Close the non-active document
    assert!(state.close_document_by_id(id2));
    assert_eq!(state.documents.len(), 1);
    assert_eq!(state.closed_documents().len(), 1);

    // Reopen it
    assert!(state.reopen_document(id2));
    assert_eq!(state.documents.len(), 2);
    assert_eq!(state.closed_documents().len(), 0);
    assert_eq!(state.active_document, id2);
}

#[test]
fn reopen_document_restores_content() {
    let mut state = AppState::empty();
    let id2 = state.open_untitled(4, true);

    // Add some content to doc2
    if let Some(doc) = state.document_mut(id2) {
        doc.replace_text("hello world");
    }

    // Close it
    state.close_active(4, true);
    assert!(state.document(id2).is_none());

    // Reopen and verify content
    assert!(state.reopen_document(id2));
    let reopened = state.document(id2).expect("document should exist");
    assert_eq!(reopened.rope.to_string(), "hello world");
}

#[test]
fn permanently_delete_removes_from_history() {
    let mut state = AppState::empty();
    let id2 = state.open_untitled(4, true);
    state.close_active(4, true);
    assert_eq!(state.closed_documents().len(), 1);

    assert!(state.permanently_delete_document(id2));
    assert_eq!(state.closed_documents().len(), 0);

    // Cannot delete again
    assert!(!state.permanently_delete_document(id2));
}

#[test]
fn last_closed_document_returns_most_recent() {
    let mut state = AppState::empty();
    let id2 = state.open_untitled(4, true);
    state.close_active(4, true);

    let last = state.last_closed_document();
    assert!(last.is_some());
    assert_eq!(last.unwrap().document.id, id2);
}
