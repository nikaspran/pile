use super::*;

#[test]
fn typing_and_paste_insert_at_caret() {
    let mut document = document("");

    assert!(replace_selection_with(&mut document, "hi"));
    assert_eq!(document.text(), "hi");
    assert_eq!(primary_selection(&document), Selection::caret(2));

    assert!(replace_selection_with(&mut document, "\nthere"));
    assert_eq!(document.text(), "hi\nthere");
    assert_eq!(primary_selection(&document), Selection::caret(8));
}

#[test]
fn typing_replaces_selected_range() {
    let mut document = document("hello world");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 6,
            head: 11,
        },
    );

    assert!(replace_selection_with(&mut document, "pile"));

    assert_eq!(document.text(), "hello pile");
    assert_eq!(primary_selection(&document), Selection::caret(10));
}

#[test]
fn newline_preserves_current_line_indent() {
    let mut document = document("fn main() {\n    let value = 1;");
    let end = document.rope.byte_len();
    set_primary_selection(&mut document, Selection::caret(end));

    assert!(insert_newline_with_auto_indent(&mut document));

    assert_eq!(document.text(), "fn main() {\n    let value = 1;\n    ");
    assert_eq!(
        primary_selection(&document),
        Selection::caret(document.rope.byte_len())
    );
}

#[test]
fn newline_replaces_selection_and_uses_selection_start_indent() {
    let mut document = document("    first selected\n    second selected");
    let start = "    fi".len();
    let end = "    first selected\n    second sele".len();
    set_primary_selection(
        &mut document,
        Selection {
            anchor: start,
            head: end,
        },
    );

    assert!(insert_newline_with_auto_indent(&mut document));

    assert_eq!(document.text(), "    fi\n    cted");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("    fi\n    ".len())
    );
}

#[test]
fn indent_at_caret_indents_current_line() {
    let mut document = document("alpha\nbeta");
    set_primary_selection(&mut document, Selection::caret("alpha\nbe".len()));

    assert!(indent_selection(&mut document));

    assert_eq!(document.text(), "alpha\n    beta");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("alpha\n    be".len())
    );
}

#[test]
fn indent_selection_indents_touched_lines() {
    let mut document = document("one\ntwo\nthree");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 1,
            head: "one\ntwo".len(),
        },
    );

    assert!(indent_selection(&mut document));

    assert_eq!(document.text(), "    one\n    two\nthree");
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 5,
            head: "    one\n    two".len()
        }
    );
}

#[test]
fn indent_selection_excludes_line_at_selection_end_boundary() {
    let mut document = document("one\ntwo\nthree");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: "one\ntwo\n".len(),
        },
    );

    assert!(indent_selection(&mut document));

    assert_eq!(document.text(), "    one\n    two\nthree");
}

#[test]
fn outdent_selection_removes_tabs_or_up_to_four_spaces() {
    let mut document = document("    one\n  two\n\tthree\nfour");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: "    one\n  two\n\tthree".len(),
        },
    );

    assert!(outdent_selection(&mut document));

    assert_eq!(document.text(), "one\ntwo\nthree\nfour");
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 0,
            head: "one\ntwo\nthree".len()
        }
    );
}

#[test]
fn outdent_without_leading_whitespace_is_noop() {
    let mut document = document("alpha\nbeta");
    let revision_before = document.revision;
    set_primary_selection(&mut document, Selection::caret(2));

    assert!(!outdent_selection(&mut document));

    assert_eq!(document.text(), "alpha\nbeta");
    assert_eq!(document.revision, revision_before);
}

#[test]
fn duplicate_line_at_caret_copies_current_line_below() {
    let mut document = document("one\ntwo");
    set_primary_selection(&mut document, Selection::caret("one\nt".len()));

    assert!(duplicate_selected_lines(&mut document));

    assert_eq!(document.text(), "one\ntwo\ntwo");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("one\ntwo\nt".len())
    );
}

#[test]
fn duplicate_selected_lines_preserves_selection_shape() {
    let mut document = document("one\ntwo\nthree");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 1,
            head: "one\ntw".len(),
        },
    );

    assert!(duplicate_selected_lines(&mut document));

    assert_eq!(document.text(), "one\ntwo\none\ntwo\nthree");
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: "one\ntwo\n".len() + 1,
            head: "one\ntwo\none\ntw".len()
        }
    );
}

#[test]
fn duplicate_empty_document_creates_blank_line() {
    let mut document = document("");

    assert!(duplicate_selected_lines(&mut document));

    assert_eq!(document.text(), "\n");
    assert_eq!(primary_selection(&document), Selection::caret(1));
}

#[test]
fn delete_line_at_caret_removes_current_line() {
    let mut document = document("one\ntwo\nthree");
    set_primary_selection(&mut document, Selection::caret("one\nt".len()));

    assert!(delete_selected_lines(&mut document));

    assert_eq!(document.text(), "one\nthree");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("one\n".len())
    );
}

#[test]
fn delete_selected_lines_removes_touched_lines() {
    let mut document = document("one\ntwo\nthree\nfour");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: "one\n".len(),
            head: "one\ntwo\nthr".len(),
        },
    );

    assert!(delete_selected_lines(&mut document));

    assert_eq!(document.text(), "one\nfour");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("one\n".len())
    );
}

#[test]
fn delete_last_line_removes_preceding_line_break() {
    let mut document = document("one\ntwo");
    set_primary_selection(&mut document, Selection::caret("one\nt".len()));

    assert!(delete_selected_lines(&mut document));

    assert_eq!(document.text(), "one");
    assert_eq!(primary_selection(&document), Selection::caret("one".len()));
}

#[test]
fn delete_line_excludes_line_at_selection_end_boundary() {
    let mut document = document("one\ntwo\nthree");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: "one\ntwo\n".len(),
        },
    );

    assert!(delete_selected_lines(&mut document));

    assert_eq!(document.text(), "three");
    assert_eq!(primary_selection(&document), Selection::caret(0));
}

#[test]
fn move_line_up_swaps_with_previous_line() {
    let mut document = document("one\ntwo\nthree");
    set_primary_selection(&mut document, Selection::caret("one\ntw".len()));

    assert!(move_selected_lines_up(&mut document));

    assert_eq!(document.text(), "two\none\nthree");
    assert_eq!(primary_selection(&document), Selection::caret("tw".len()));
}

#[test]
fn move_line_up_at_document_start_is_noop() {
    let mut document = document("one\ntwo");
    let revision_before = document.revision;
    set_primary_selection(&mut document, Selection::caret(1));

    assert!(!move_selected_lines_up(&mut document));

    assert_eq!(document.text(), "one\ntwo");
    assert_eq!(document.revision, revision_before);
    assert_eq!(primary_selection(&document), Selection::caret(1));
}

#[test]
fn move_last_line_up_preserves_line_break_between_lines() {
    let mut document = document("one\ntwo");
    set_primary_selection(&mut document, Selection::caret("one\ntw".len()));

    assert!(move_selected_lines_up(&mut document));

    assert_eq!(document.text(), "two\none");
    assert_eq!(primary_selection(&document), Selection::caret("tw".len()));
}

#[test]
fn move_selected_lines_up_preserves_selection_shape() {
    let mut document = document("zero\none\ntwo\nthree");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: "zero\no".len(),
            head: "zero\none\ntw".len(),
        },
    );

    assert!(move_selected_lines_up(&mut document));

    assert_eq!(document.text(), "one\ntwo\nzero\nthree");
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: "o".len(),
            head: "one\ntw".len()
        }
    );
}

#[test]
fn move_line_down_swaps_with_next_line() {
    let mut document = document("one\ntwo\nthree");
    set_primary_selection(&mut document, Selection::caret("on".len()));

    assert!(move_selected_lines_down(&mut document));

    assert_eq!(document.text(), "two\none\nthree");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("two\non".len())
    );
}

#[test]
fn move_line_down_at_document_end_is_noop() {
    let mut document = document("one\ntwo");
    let revision_before = document.revision;
    set_primary_selection(&mut document, Selection::caret("one\nt".len()));

    assert!(!move_selected_lines_down(&mut document));

    assert_eq!(document.text(), "one\ntwo");
    assert_eq!(document.revision, revision_before);
    assert_eq!(
        primary_selection(&document),
        Selection::caret("one\nt".len())
    );
}

#[test]
fn move_line_down_over_last_line_preserves_line_break_between_lines() {
    let mut document = document("one\ntwo");
    set_primary_selection(&mut document, Selection::caret("on".len()));

    assert!(move_selected_lines_down(&mut document));

    assert_eq!(document.text(), "two\none");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("two\non".len())
    );
}

#[test]
fn move_selected_lines_down_preserves_selection_shape() {
    let mut document = document("zero\none\ntwo\nthree");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: "zero\no".len(),
            head: "zero\none\ntw".len(),
        },
    );

    assert!(move_selected_lines_down(&mut document));

    assert_eq!(document.text(), "zero\nthree\none\ntwo");
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: "zero\nthree\no".len(),
            head: "zero\nthree\none\ntw".len()
        }
    );
}

#[test]
fn join_line_at_caret_merges_with_next_line() {
    let mut document = document("one\n  two\nthree");
    set_primary_selection(&mut document, Selection::caret("on".len()));

    assert!(join_selected_lines(&mut document));

    assert_eq!(document.text(), "one two\nthree");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("one two".len())
    );
}

#[test]
fn join_selected_lines_merges_all_touched_lines() {
    let mut document = document("one\n  two\n\tthree\nfour");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 1,
            head: "one\n  two\n\tthr".len(),
        },
    );

    assert!(join_selected_lines(&mut document));

    assert_eq!(document.text(), "one two three\nfour");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("one two three".len())
    );
}

#[test]
fn join_line_avoids_extra_space_for_empty_sides() {
    let mut document = document("one\n\n  two");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: "one\n\n  tw".len(),
        },
    );

    assert!(join_selected_lines(&mut document));

    assert_eq!(document.text(), "one two");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("one two".len())
    );
}

#[test]
fn join_line_trims_trailing_horizontal_whitespace() {
    let mut document = document("one   \n\t two");
    set_primary_selection(&mut document, Selection::caret(1));

    assert!(join_selected_lines(&mut document));

    assert_eq!(document.text(), "one two");
}

#[test]
fn join_last_line_is_noop() {
    let mut document = document("one\ntwo");
    let revision_before = document.revision;
    set_primary_selection(&mut document, Selection::caret("one\nt".len()));

    assert!(!join_selected_lines(&mut document));

    assert_eq!(document.text(), "one\ntwo");
    assert_eq!(document.revision, revision_before);
    assert_eq!(
        primary_selection(&document),
        Selection::caret("one\nt".len())
    );
}

#[test]
fn sort_selected_lines_orders_touched_lines() {
    let mut document = document("gamma\nalpha\nbeta\nomega");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 1,
            head: "gamma\nalpha\nbe".len(),
        },
    );

    assert!(sort_selected_lines(&mut document));

    assert_eq!(document.text(), "alpha\nbeta\ngamma\nomega");
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 0,
            head: "alpha\nbeta\ngamma\n".len()
        }
    );
}

#[test]
fn sort_selected_lines_excludes_line_at_selection_end_boundary() {
    let mut document = document("b\na\nc");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: "b\na\n".len(),
        },
    );

    assert!(sort_selected_lines(&mut document));

    assert_eq!(document.text(), "a\nb\nc");
}

#[test]
fn sort_selected_lines_without_trailing_newline_keeps_none() {
    let mut document = document("b\na");
    let end = document.rope.byte_len();
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: end,
        },
    );

    assert!(sort_selected_lines(&mut document));

    assert_eq!(document.text(), "a\nb");
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 0,
            head: "a\nb".len()
        }
    );
}

#[test]
fn sort_selected_lines_noops_for_single_or_already_sorted_lines() {
    let mut document = document("a\nb\nc");
    let revision_before = document.revision;
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: "a\nb\n".len(),
        },
    );

    assert!(!sort_selected_lines(&mut document));

    assert_eq!(document.text(), "a\nb\nc");
    assert_eq!(document.revision, revision_before);

    set_primary_selection(&mut document, Selection::caret(1));
    assert!(!sort_selected_lines(&mut document));
}

#[test]
fn backspace_and_delete_handle_boundaries_and_lines() {
    let mut document = document("ab\ncd");
    set_primary_selection(&mut document, Selection::caret(3));

    assert!(backspace(&mut document));
    assert_eq!(document.text(), "abcd");
    assert_eq!(primary_selection(&document), Selection::caret(2));

    assert!(delete(&mut document));
    assert_eq!(document.text(), "abd");
    assert_eq!(primary_selection(&document), Selection::caret(2));

    set_primary_selection(&mut document, Selection::caret(0));
    assert!(!backspace(&mut document));
    let end = document.rope.byte_len();
    set_primary_selection(&mut document, Selection::caret(end));
    assert!(!delete(&mut document));
}

#[test]
fn word_backspace_deletes_to_previous_word_boundary() {
    let mut document = document("alpha beta  gamma");
    set_primary_selection(&mut document, Selection::caret("alpha beta  gam".len()));

    assert!(backspace_word(&mut document));

    assert_eq!(document.text(), "alpha beta  ma");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("alpha beta  ".len())
    );
}

#[test]
fn word_delete_deletes_to_next_word_boundary() {
    let mut document = document("alpha beta  gamma");
    set_primary_selection(&mut document, Selection::caret("alpha ".len()));

    assert!(delete_word(&mut document));

    assert_eq!(document.text(), "alpha   gamma");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("alpha ".len())
    );
}

#[test]
fn word_deletion_replaces_selection_before_using_word_boundaries() {
    let mut document = document("alpha beta gamma");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: "alpha ".len(),
            head: "alpha beta".len(),
        },
    );

    assert!(backspace_word(&mut document));

    assert_eq!(document.text(), "alpha  gamma");
    assert_eq!(
        primary_selection(&document),
        Selection::caret("alpha ".len())
    );
}

#[test]
fn word_deletion_handles_multiple_cursors() {
    let mut document = document("alpha beta gamma delta");
    document.selections = vec![
        Selection::caret("alpha be".len()),
        Selection::caret("alpha beta gamma de".len()),
    ];

    assert!(backspace_word_all(&mut document));

    assert_eq!(document.text(), "alpha ta gamma lta");
    assert_eq!(
        document.selections,
        vec![
            Selection::caret("alpha ".len()),
            Selection::caret("alpha ta gamma ".len()),
        ]
    );

    document.selections = vec![
        Selection::caret("alpha ".len()),
        Selection::caret("alpha ta ".len()),
    ];

    assert!(delete_word_all(&mut document));

    assert_eq!(document.text(), "alpha   lta");
    assert_eq!(
        document.selections,
        vec![
            Selection::caret("alpha ".len()),
            Selection::caret("alpha  ".len())
        ]
    );
}
