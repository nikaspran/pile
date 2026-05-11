use super::*;

#[test]
fn replace_match_replaces_range_and_moves_caret() {
    let mut document = document("hello world hello");
    let revision_before = document.revision;

    let caret = replace_match(
        &mut document,
        SearchMatch { start: 6, end: 11 },
        "earth",
        None,
    );

    assert_eq!(document.text(), "hello earth hello");
    assert_eq!(caret, 11);
    assert_eq!(primary_selection(&document), Selection::caret(11));
    assert_eq!(document.revision, revision_before + 1);
}

#[test]
fn replace_match_handles_empty_replacement() {
    let mut document = document("delete me here");

    let caret = replace_match(&mut document, SearchMatch { start: 7, end: 9 }, "", None);

    assert_eq!(document.text(), "delete  here");
    assert_eq!(caret, 7);
    assert_eq!(primary_selection(&document), Selection::caret(7));
}

#[test]
fn replace_all_matches_applies_in_reverse_order() {
    let mut document = document("foo bar foo bar foo");
    let revision_before = document.revision;

    let count = replace_all_matches(
        &mut document,
        &[
            SearchMatch { start: 0, end: 3 },
            SearchMatch { start: 8, end: 11 },
            SearchMatch { start: 16, end: 19 },
        ],
        "qux",
        None,
    );

    assert_eq!(count, 3);
    assert_eq!(document.text(), "qux bar qux bar qux");
    assert_eq!(primary_selection(&document), Selection::caret(3));
    assert_eq!(document.revision, revision_before + 1);
}

#[test]
fn replace_all_matches_handles_replacement_containing_query() {
    let mut document = document("a a a");

    let count = replace_all_matches(
        &mut document,
        &[
            SearchMatch { start: 0, end: 1 },
            SearchMatch { start: 2, end: 3 },
            SearchMatch { start: 4, end: 5 },
        ],
        "aa",
        None,
    );

    assert_eq!(count, 3);
    assert_eq!(document.text(), "aa aa aa");
}

#[test]
fn replace_all_matches_handles_multibyte_text() {
    let mut document = document("aé日 aé日");

    let count = replace_all_matches(
        &mut document,
        &[
            SearchMatch { start: 1, end: 6 },
            SearchMatch { start: 8, end: 13 },
        ],
        "x",
        None,
    );

    assert_eq!(count, 2);
    assert_eq!(document.text(), "ax ax");
}

#[test]
fn replace_all_regex_handles_changing_match_lengths() {
    let mut document = document("a1 b22 c333");
    let regex = Regex::new(r"\d+").unwrap();

    let count = replace_all_matches(
        &mut document,
        &[
            SearchMatch { start: 1, end: 2 },
            SearchMatch { start: 4, end: 6 },
            SearchMatch { start: 8, end: 11 },
        ],
        "[$0]",
        Some(&regex),
    );

    assert_eq!(count, 3);
    assert_eq!(document.text(), "a[1] b[22] c[333]");
}

#[test]
fn replace_all_matches_no_op_for_empty_input() {
    let mut document = document("untouched");
    let revision_before = document.revision;

    let count = replace_all_matches(&mut document, &[], "x", None);

    assert_eq!(count, 0);
    assert_eq!(document.text(), "untouched");
    assert_eq!(document.revision, revision_before);
}

#[test]
fn undo_restores_text_after_typing() {
    let mut document = document("");
    replace_selection_with(&mut document, "hello");
    document.commit_undo_group();

    assert!(document.undo());
    assert_eq!(document.text(), "");
    assert_eq!(primary_selection(&document), Selection::caret(0));
}

#[test]
fn undo_restores_text_after_backspace() {
    let mut document = document("hello");
    set_primary_selection(&mut document, Selection::caret(5));
    backspace(&mut document);
    document.commit_undo_group();

    assert!(document.undo());
    assert_eq!(document.text(), "hello");
    assert_eq!(primary_selection(&document), Selection::caret(5));
}

#[test]
fn undo_restores_text_after_delete() {
    let mut document = document("hello");
    set_primary_selection(&mut document, Selection::caret(0));
    delete(&mut document);
    document.commit_undo_group();

    assert!(document.undo());
    assert_eq!(document.text(), "hello");
    assert_eq!(primary_selection(&document), Selection::caret(0));
}

#[test]
fn undo_restores_text_after_delete_line() {
    let mut document = document("one\ntwo\nthree");
    set_primary_selection(&mut document, Selection::caret("one\nt".len()));
    delete_selected_lines(&mut document);
    document.commit_undo_group();

    assert!(document.undo());
    assert_eq!(document.text(), "one\ntwo\nthree");
}

#[test]
fn undo_restores_text_after_duplicate_line() {
    let mut document = document("one\ntwo");
    set_primary_selection(&mut document, Selection::caret("one\nt".len()));
    duplicate_selected_lines(&mut document);
    document.commit_undo_group();

    assert!(document.undo());
    assert_eq!(document.text(), "one\ntwo");
}

#[test]
fn undo_restores_text_after_indent() {
    let mut document = document("alpha\nbeta");
    set_primary_selection(&mut document, Selection::caret("alpha\nbe".len()));
    indent_selection(&mut document);
    document.commit_undo_group();

    assert!(document.undo());
    assert_eq!(document.text(), "alpha\nbeta");
}

#[test]
fn undo_restores_text_after_sort_lines() {
    let mut document = document("gamma\nalpha\nbeta\nomega");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 1,
            head: "gamma\nalpha\nbe".len(),
        },
    );
    sort_selected_lines(&mut document);
    document.commit_undo_group();

    assert!(document.undo());
    assert_eq!(document.text(), "gamma\nalpha\nbeta\nomega");
}

#[test]
fn redo_reapplies_undone_edit() {
    let mut document = document("");
    replace_selection_with(&mut document, "hello");
    document.commit_undo_group();

    assert!(document.undo());
    assert_eq!(document.text(), "");

    assert!(document.redo());
    assert_eq!(document.text(), "hello");
}

#[test]
fn redo_is_cleared_after_new_edit() {
    let mut document = document("");
    replace_selection_with(&mut document, "hello");
    document.commit_undo_group();

    assert!(document.undo());
    assert!(document.can_redo());

    replace_selection_with(&mut document, "world");
    document.commit_undo_group();
    assert!(!document.can_redo());
}

#[test]
fn undo_noop_when_empty() {
    let mut document = document("hello");
    assert!(!document.undo());
    assert_eq!(document.text(), "hello");
}

#[test]
fn redo_noop_when_empty() {
    let mut document = document("hello");
    assert!(!document.redo());
    assert_eq!(document.text(), "hello");
}

#[test]
fn undo_replaces_selection_text_and_restores_selection() {
    let mut document = document("hello world");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 6,
            head: 11,
        },
    );
    replace_selection_with(&mut document, "pile");
    document.commit_undo_group();

    assert_eq!(document.text(), "hello pile");

    assert!(document.undo());
    assert_eq!(document.text(), "hello world");
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 6,
            head: 11,
        }
    );
}

#[test]
fn multiple_undo_steps_chain_correctly() {
    let mut document = document("");
    replace_selection_with(&mut document, "a");
    document.commit_undo_group();
    replace_selection_with(&mut document, "b");
    document.commit_undo_group();
    replace_selection_with(&mut document, "c");
    document.commit_undo_group();

    assert_eq!(document.text(), "abc");

    assert!(document.undo());
    assert_eq!(document.text(), "ab");

    assert!(document.undo());
    assert_eq!(document.text(), "a");

    assert!(document.undo());
    assert_eq!(document.text(), "");

    assert!(!document.undo());
}

#[test]
fn replace_all_can_be_undone() {
    let mut document = document("foo bar foo");
    replace_all_matches(
        &mut document,
        &[
            SearchMatch { start: 0, end: 3 },
            SearchMatch { start: 8, end: 11 },
        ],
        "baz",
        None,
    );
    document.commit_undo_group();

    assert_eq!(document.text(), "baz bar baz");

    assert!(document.undo());
    assert_eq!(document.text(), "foo bar foo");
}

#[test]
fn replace_match_can_be_undone() {
    let mut document = document("hello world");
    replace_match(
        &mut document,
        SearchMatch { start: 6, end: 11 },
        "earth",
        None,
    );
    document.commit_undo_group();

    assert_eq!(document.text(), "hello earth");

    assert!(document.undo());
    assert_eq!(document.text(), "hello world");
}

#[test]
fn move_line_up_can_be_undone() {
    let mut document = document("one\ntwo\nthree");
    set_primary_selection(&mut document, Selection::caret("one\ntw".len()));
    move_selected_lines_up(&mut document);
    document.commit_undo_group();

    assert_eq!(document.text(), "two\none\nthree");

    assert!(document.undo());
    assert_eq!(document.text(), "one\ntwo\nthree");
}

#[test]
fn join_lines_can_be_undone() {
    let mut document = document("one\n  two\nthree");
    set_primary_selection(&mut document, Selection::caret("on".len()));
    join_selected_lines(&mut document);
    document.commit_undo_group();

    assert_eq!(document.text(), "one two\nthree");

    assert!(document.undo());
    assert_eq!(document.text(), "one\n  two\nthree");
}

#[test]
fn outdent_can_be_undone() {
    let mut document = document("    one\n  two\n\tthree\nfour");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: "    one\n  two\n\tthree".len(),
        },
    );
    outdent_selection(&mut document);
    document.commit_undo_group();

    assert_eq!(document.text(), "one\ntwo\nthree\nfour");

    assert!(document.undo());
    assert_eq!(document.text(), "    one\n  two\n\tthree\nfour");
}
