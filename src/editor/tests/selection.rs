use super::*;

#[test]
fn word_at_selection_finds_word_under_caret() {
    let rope = Rope::from("hello world");
    let sel = Selection::caret(3); // inside "hello"
    let result = word_at_selection(&rope, sel);
    assert_eq!(result, Some((0, 5)));
}

#[test]
fn word_at_selection_uses_existing_selection() {
    let rope = Rope::from("hello world");
    let sel = Selection { anchor: 0, head: 5 };
    let result = word_at_selection(&rope, sel);
    assert_eq!(result, Some((0, 5)));
}

#[test]
fn word_at_selection_returns_none_for_whitespace() {
    let rope = Rope::from("hello world");
    let sel = Selection::caret(5); // space between words
    let result = word_at_selection(&rope, sel);
    assert_eq!(result, None);
}

#[test]
fn word_at_selection_handles_non_alphanumeric_word() {
    let rope = Rope::from("hello_world test");
    let sel = Selection::caret(8); // inside "hello_world"
    let result = word_at_selection(&rope, sel);
    assert_eq!(result, Some((0, 11)));
}

#[test]
fn add_all_matches_does_not_duplicate_primary_occurrence() {
    let mut document = document("foo foo");
    set_primary_selection(&mut document, Selection::caret(0));

    add_all_matches(&mut document);

    assert_eq!(
        document.selections,
        vec![Selection::caret(0), Selection { anchor: 4, head: 7 },]
    );
    assert_eq!(
        document.occurrence_selections,
        vec![
            Selection { anchor: 0, head: 3 },
            Selection { anchor: 4, head: 7 },
        ]
    );
}

#[test]
fn clear_secondary_cursors_first_collapses_selected_text() {
    let mut document = document("alpha beta gamma");
    document.selections = vec![
        Selection { anchor: 0, head: 5 },
        Selection {
            anchor: 11,
            head: 16,
        },
    ];
    document.occurrence_selections = document.selections.clone();
    document.multi_cursor_query = Some("alpha".to_string());

    assert!(clear_secondary_cursors(&mut document));

    assert_eq!(
        document.selections,
        vec![Selection::caret(5), Selection::caret(16)]
    );
    assert!(document.occurrence_selections.is_empty());
    assert_eq!(document.multi_cursor_query, None);
}

#[test]
fn clear_secondary_cursors_removes_secondary_carets_when_no_text_is_selected() {
    let mut document = document("alpha beta gamma");
    document.selections = vec![Selection::caret(5), Selection::caret(16)];

    assert!(clear_secondary_cursors(&mut document));

    assert_eq!(document.selections, vec![Selection::caret(5)]);
}

#[test]
fn clear_secondary_cursors_noops_for_single_caret() {
    let mut document = document("alpha beta gamma");
    document.selections = vec![Selection::caret(5)];

    assert!(!clear_secondary_cursors(&mut document));

    assert_eq!(document.selections, vec![Selection::caret(5)]);
}

#[test]
fn grapheme_movement_respects_clusters() {
    // "café" where 'é' is e + combining acute (1+2=3 bytes, 1 grapheme)
    let rope = Rope::from("café");
    // "c"=1, "a"=1, "f"=1, "e\u{0301}"=3 bytes
    let c_len = "c".len(); // 1
    let ca_len = "ca".len(); // 2
    let caf_len = "caf".len(); // 3
    let cafe_len = "café".len(); // 6

    // Moving right grapheme by grapheme
    let after_c = next_grapheme_boundary(&rope, 0);
    assert_eq!(after_c, c_len);

    let after_ca = next_grapheme_boundary(&rope, after_c);
    assert_eq!(after_ca, ca_len);

    let after_caf = next_grapheme_boundary(&rope, after_ca);
    assert_eq!(after_caf, caf_len);

    // The 'é' is one grapheme (e + combining acute = 3 bytes)
    let after_e_acute = next_grapheme_boundary(&rope, after_caf);
    assert_eq!(after_e_acute, cafe_len);

    // Moving left from end
    let before_e_acute = previous_grapheme_boundary(&rope, cafe_len);
    assert_eq!(before_e_acute, caf_len);

    let before_caf = previous_grapheme_boundary(&rope, before_e_acute);
    assert_eq!(before_caf, ca_len);
}

#[test]
fn grapheme_movement_emoji() {
    // Emoji with ZWJ are single graphemes
    let rope = Rope::from("👨‍👩‍👧‍👦 family");
    // Move right through emoji grapheme
    let emoji_len = "👨‍👩‍👧‍👦".len();
    let after_emoji = next_grapheme_boundary(&rope, 0);
    assert_eq!(after_emoji, emoji_len);

    // Moving left from after space
    let space_pos = emoji_len + 1; // emoji + space
    let before_space = previous_grapheme_boundary(&rope, space_pos);
    assert_eq!(before_space, emoji_len);
}

#[test]
fn test_expand_selection_by_word_from_caret() {
    let mut document = document("hello world foo");
    set_primary_selection(&mut document, Selection::caret(6)); // at start of "world"

    expand_selection_by_word(&mut document);

    let sel = primary_selection(&document);
    assert_eq!(sel.anchor, 6);
    assert_eq!(sel.head, 11); // end of "world"
}

#[test]
fn test_expand_selection_by_word_expands_existing_selection() {
    let mut document = document("hello world foo");
    set_primary_selection(&mut document, Selection { anchor: 3, head: 8 }); // overlaps "lo wo"

    expand_selection_by_word(&mut document);

    let sel = primary_selection(&document);
    assert_eq!(sel.anchor, 0); // start of "hello"
    assert_eq!(sel.head, 11); // end of "world"
}

#[test]
fn test_contract_selection_by_word() {
    let mut document = document("hello world foo");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: 15,
        },
    ); // selects "hello world foo"

    contract_selection_by_word(&mut document);

    let sel = primary_selection(&document);
    // Should contract to "world foo" (from "hello world foo")
    assert!(sel.anchor > 0); // should have moved past "hello"
    assert!(sel.head <= 15);
}

#[test]
fn test_expand_selection_by_line_single_line() {
    let mut document = document("first line\nsecond line\nthird line");
    let line_start = "first line\n".len();
    set_primary_selection(&mut document, Selection::caret(line_start + 3)); // in "second"

    expand_selection_by_line(&mut document);

    let sel = primary_selection(&document);
    assert_eq!(sel.anchor, line_start); // start of "second line"
    let (_, end) = visual_line_bounds(&document.rope, 1);
    assert_eq!(sel.head, end); // end of "second line"
}

#[test]
fn test_expand_selection_by_line_multiline_selection() {
    let mut document = document("first line\nsecond line\nthird line");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 3,
            head: "first line\nsecond".len(),
        },
    ); // spans first and part of second

    expand_selection_by_line(&mut document);

    let sel = primary_selection(&document);
    assert_eq!(sel.anchor, 0); // start of first line
    let (_, end) = visual_line_bounds(&document.rope, 1);
    assert_eq!(sel.head, end); // end of second line
}

#[test]
fn test_contract_selection_by_line() {
    let mut document = document("first line\nsecond line\nthird line");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: "first line\nsecond line\n".len(),
        },
    ); // first two lines

    contract_selection_by_line(&mut document);

    let sel = primary_selection(&document);
    // Should contract to just "second line"
    let line_start = "first line\n".len();
    assert!(sel.anchor >= line_start);
    assert!(sel.head <= "first line\nsecond line".len());
}

#[test]
fn test_expand_selection_by_bracket_pair_simple() {
    let mut document = document("(hello world)");
    set_primary_selection(&mut document, Selection::caret(3)); // inside parens

    expand_selection_by_bracket_pair(&mut document);

    let sel = primary_selection(&document);
    assert_eq!(sel.anchor, 0); // '('
    assert_eq!(sel.head, 13); // after ')' (string is 13 bytes, positions 0-12, so 13 is end)
}

#[test]
fn test_expand_selection_by_bracket_pair_nested() {
    let mut document = document("(outer (inner) text)");
    // String bytes: ( =0, o=1, u=2, t=3, e=4, r=5, space=6, (=7, i=8, n=9, n=10, e=11, r=12, )=13, space=14, t=15, e=16, x=17, t=18, )=19
    set_primary_selection(&mut document, Selection::caret(10)); // inside "inner"

    expand_selection_by_bracket_pair(&mut document);

    let sel = primary_selection(&document);
    // Should match the inner pair: '(' at 7, ')' at 13
    assert_eq!(sel.anchor, 7); // '(' before "inner"
    assert_eq!(sel.head, 14); // after ")" at position 13 (13 + 1)
}

#[test]
fn test_contract_selection_by_bracket_pair() {
    let mut document = document("(hello world)");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 0,
            head: 14,
        },
    ); // entire "(hello world)"

    contract_selection_by_bracket_pair(&mut document);

    let sel = primary_selection(&document);
    // Should contract to "hello world"
    assert_eq!(sel.anchor, 1); // after '('
    assert_eq!(sel.head, 13); // before ')'
}

#[test]
fn test_expand_selection_by_indent_block_basic() {
    let text = "no indent\n  indented line 1\n  indented line 2\n  indented line 3\nno indent";
    let mut document = document(text);
    let offset = "no indent\n  indented".len(); // in second line
    set_primary_selection(&mut document, Selection::caret(offset));

    expand_selection_by_indent_block(&mut document);

    let sel = primary_selection(&document);
    let expected_start = "no indent\n".len(); // 10
    // The indent block ends at position 63 (the '\n' after "indented line 3")
    let expected_end =
        "no indent\n  indented line 1\n  indented line 2\n  indented line 3\n".len() - 1; // 63
    assert_eq!(sel.anchor, expected_start);
    assert_eq!(sel.head, expected_end);
}

#[test]
fn test_expand_selection_by_indent_block_with_blank_line_boundary() {
    let text = "  indented 1\n\n  indented 2";
    let mut document = document(text);
    let offset = 2; // in first indented line
    set_primary_selection(&mut document, Selection::caret(offset));

    expand_selection_by_indent_block(&mut document);

    let sel = primary_selection(&document);
    // Should only select first indented line (blank line is boundary)
    let (_, end) = visual_line_bounds(&document.rope, 0);
    assert_eq!(sel.anchor, 0);
    assert_eq!(sel.head, end);
}

#[test]
fn test_contract_selection_by_indent_block() {
    let text = "no indent\n  indented line 1\n  indented line 2\n  indented line 3\nno indent";
    let mut document = document(text);
    let start = "no indent\n".len();
    let end = "no indent\n  indented line 1\n  indented line 2\n  indented line 3\n".len();
    set_primary_selection(
        &mut document,
        Selection {
            anchor: start,
            head: end,
        },
    ); // all three indented lines

    contract_selection_by_indent_block(&mut document);

    let sel = primary_selection(&document);
    // Should contract to just "indented line 2"
    assert!(sel.anchor > start);
    assert!(sel.head < end);
}

// ============================================================================
// High-DPI and Font Fallback Tests
// ============================================================================
