use super::*;

#[test]
fn movement_respects_multibyte_char_boundaries() {
    let mut document = document("aé日");
    let end = document.rope.byte_len();
    set_primary_selection(&mut document, Selection::caret(end));

    move_left(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(3));
    move_left(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(1));
    move_right(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(3));
}

#[test]
fn vertical_and_line_boundary_movement_tracks_columns() {
    let mut document = document("abc\nde\nfghi");
    let mut view_state = EditorViewState::default();
    set_primary_selection(&mut document, Selection::caret(3));

    move_vertical(&mut document, &mut view_state, 1, false);
    assert_eq!(primary_selection(&document), Selection::caret(6));

    move_vertical(&mut document, &mut view_state, 1, false);
    assert_eq!(primary_selection(&document), Selection::caret(10));

    move_home(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(7));

    move_end(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(11));
}

#[test]
fn shift_arrow_extends_selection() {
    let mut document = document("hello world");
    set_primary_selection(&mut document, Selection::caret(3));

    move_right(&mut document, true);
    move_right(&mut document, true);
    assert_eq!(
        primary_selection(&document),
        Selection { anchor: 3, head: 5 }
    );

    move_left(&mut document, true);
    assert_eq!(
        primary_selection(&document),
        Selection { anchor: 3, head: 4 }
    );
}

#[test]
fn shift_home_end_extend_to_line_bounds() {
    let mut document = document("hello world");
    set_primary_selection(&mut document, Selection::caret(6));

    move_home(&mut document, true);
    assert_eq!(
        primary_selection(&document),
        Selection { anchor: 6, head: 0 }
    );

    set_primary_selection(&mut document, Selection::caret(6));
    move_end(&mut document, true);
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 6,
            head: 11
        }
    );
}

#[test]
fn shift_vertical_preserves_anchor_and_preferred_column() {
    let mut document = document("abcd\nef\nghij");
    let mut view_state = EditorViewState::default();
    set_primary_selection(&mut document, Selection::caret(3));

    move_vertical(&mut document, &mut view_state, 1, true);
    assert_eq!(
        primary_selection(&document),
        Selection { anchor: 3, head: 7 }
    );
    assert_eq!(view_state.preferred_column, Some(3));

    move_vertical(&mut document, &mut view_state, 1, true);
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 3,
            head: 11
        }
    );
}

#[test]
fn movement_updates_all_cursors() {
    let mut document = document("abcd\nefgh\nijkl");
    document.selections = vec![
        Selection::caret(1),
        Selection::caret("abcd\ne".len()),
        Selection::caret("abcd\nefgh\ni".len()),
    ];

    move_right(&mut document, false);

    assert_eq!(
        document.selections,
        vec![
            Selection::caret(2),
            Selection::caret("abcd\nef".len()),
            Selection::caret("abcd\nefgh\nij".len()),
        ]
    );
}

#[test]
fn vertical_movement_updates_all_cursors() {
    let mut document = document("abcd\nef\nghij");
    let mut view_state = EditorViewState::default();
    document.selections = vec![Selection::caret(3), Selection::caret("abcd\ne".len())];

    move_vertical(&mut document, &mut view_state, 1, false);

    assert_eq!(
        document.selections,
        vec![
            Selection::caret("abcd\nef".len()),
            Selection::caret("abcd\nef\ng".len()),
        ]
    );
    assert_eq!(view_state.preferred_column, None);
}

#[test]
fn shift_movement_extends_all_cursors() {
    let mut document = document("abcd\nefgh");
    document.selections = vec![Selection::caret(1), Selection::caret("abcd\ne".len())];

    move_right(&mut document, true);

    assert_eq!(
        document.selections,
        vec![
            Selection { anchor: 1, head: 2 },
            Selection {
                anchor: "abcd\ne".len(),
                head: "abcd\nef".len(),
            },
        ]
    );
}

#[test]
fn move_word_right_skips_whitespace_then_word() {
    let mut document = document("  foo bar");
    set_primary_selection(&mut document, Selection::caret(0));

    move_word_right(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(5));

    move_word_right(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(9));
}

#[test]
fn move_word_right_stops_at_punctuation() {
    let mut document = document("foo, bar");
    set_primary_selection(&mut document, Selection::caret(0));

    move_word_right(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(3));

    move_word_right(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(4));

    move_word_right(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(8));
}

#[test]
fn move_word_left_symmetric() {
    let mut document = document("  foo bar");
    let end = document.rope.byte_len();
    set_primary_selection(&mut document, Selection::caret(end));

    move_word_left(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(6));

    move_word_left(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(2));

    move_word_left(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(0));
}

#[test]
fn move_word_right_unicode_lands_on_char_boundary() {
    let text = "héllo wörld";
    let mut document = document(text);
    set_primary_selection(&mut document, Selection::caret(0));

    move_word_right(&mut document, false);
    let after_first = "héllo".len();
    assert_eq!(primary_selection(&document), Selection::caret(after_first));
    assert!(document.rope.is_char_boundary(after_first));

    move_word_right(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(text.len()));
}

#[test]
fn move_word_extends_selection() {
    let mut document = document("foo bar baz");
    set_primary_selection(&mut document, Selection::caret(0));

    move_word_right(&mut document, true);
    assert_eq!(
        primary_selection(&document),
        Selection { anchor: 0, head: 3 }
    );

    move_word_right(&mut document, true);
    assert_eq!(
        primary_selection(&document),
        Selection { anchor: 0, head: 7 }
    );
}

#[test]
fn word_motion_at_document_edges_is_noop() {
    let mut document = document("foo");
    set_primary_selection(&mut document, Selection::caret(0));
    move_word_left(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(0));

    let end = document.rope.byte_len();
    set_primary_selection(&mut document, Selection::caret(end));
    move_word_right(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(end));
}

#[test]
fn visual_lines_include_empty_document_and_trailing_newline() {
    assert_eq!(visual_line_count(&Rope::from("")), 1);
    assert_eq!(visual_line_count(&Rope::from("a\n")), 2);
    assert_eq!(byte_for_line_column(&Rope::from("a\n"), 1, 0), 2);
}

#[test]
fn search_highlight_columns_clip_to_visual_line() {
    let rope = Rope::from("abc\ndef");

    assert_eq!(
        highlight_columns_for_line(
            &rope,
            SearchHighlight {
                start: 1,
                end: 3,
                is_current: false,
            },
            0,
        ),
        Some(HighlightColumns {
            start_column: 1,
            end_column: 3,
        })
    );
    assert_eq!(
        highlight_columns_for_line(
            &rope,
            SearchHighlight {
                start: 1,
                end: 3,
                is_current: false,
            },
            1,
        ),
        None
    );
}

#[test]
fn search_highlight_columns_split_multiline_matches() {
    let rope = Rope::from("abc\ndef");
    let highlight = SearchHighlight {
        start: 2,
        end: 6,
        is_current: true,
    };

    assert_eq!(
        highlight_columns_for_line(&rope, highlight, 0),
        Some(HighlightColumns {
            start_column: 2,
            end_column: 3,
        })
    );
    assert_eq!(
        highlight_columns_for_line(&rope, highlight, 1),
        Some(HighlightColumns {
            start_column: 0,
            end_column: 2,
        })
    );
}

#[test]
fn search_highlight_columns_count_multibyte_characters() {
    let rope = Rope::from("aé日z");
    let highlight = SearchHighlight {
        start: 1,
        end: "aé日".len(),
        is_current: false,
    };

    assert_eq!(
        highlight_columns_for_line(&rope, highlight, 0),
        Some(HighlightColumns {
            start_column: 1,
            end_column: 3,
        })
    );
}

#[test]
fn search_selection_is_clamped_to_valid_byte_offsets() {
    let mut document = document("aé日");
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 2,
            head: 999,
        },
    );

    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 1,
            head: document.rope.byte_len(),
        }
    );
}

#[test]
fn offset_at_pointer_maps_clicks_to_byte_offsets() {
    let document = document("abc\nhello\nworld");
    let rope = &document.rope;
    let line_count = visual_line_count(rope);
    let row_height = 10.0;
    let char_width = 8.0;
    let text_origin_x = 20.0;
    let gutter_width = 44.0;
    let content_width = 400.0_f32.max(text_origin_x + super::EDITOR_MIN_WIDTH);
    let content_height = (line_count as f32 * row_height).max(200.0);
    let font_id = egui::FontId::monospace(14.0);

    // Use the new test constructor to avoid needing private fields
    let layout = super::layout::TextLayoutPipeline::for_test(
        row_height,
        char_width,
        font_id,
        gutter_width,
        text_origin_x,
        content_width,
        content_height,
        line_count,
    );

    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 200.0));

    let pointer_at = |x: f32, y: f32| layout.offset_at_pointer(rope, egui::pos2(x, y), rect);

    assert_eq!(pointer_at(text_origin_x - 50.0, 0.0), 0);
    assert_eq!(pointer_at(text_origin_x + char_width * 2.0, 0.0), 2);
    assert_eq!(
        pointer_at(text_origin_x + char_width * 100.0, row_height * 1.5),
        "abc\nhello".len()
    );
    assert_eq!(
        pointer_at(text_origin_x, row_height * 50.0),
        "abc\nhello\n".len()
    );
}

#[test]
fn document_boundary_motion_jumps_to_doc_ends() {
    let text = "abc\ndef\nghi";
    let mut document = document(text);
    set_primary_selection(&mut document, Selection::caret(5));

    move_document_start(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(0));

    move_document_end(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(text.len()));
}

#[test]
fn document_boundary_motion_extends_selection() {
    let text = "abc\ndef\nghi";
    let mut document = document(text);
    set_primary_selection(&mut document, Selection::caret(5));

    move_document_end(&mut document, true);
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 5,
            head: text.len()
        }
    );

    move_document_start(&mut document, true);
    assert_eq!(
        primary_selection(&document),
        Selection { anchor: 5, head: 0 }
    );
}

#[test]
fn paragraph_motion_jumps_blank_line_boundaries() {
    // line indices: 0:"first" 1:"more" 2:"" 3:"second" 4:"two" 5:"" 6:"third"
    let text = "first\nmore\n\nsecond\ntwo\n\nthird";
    let mut document = document(text);

    // From caret on line 0, paragraph_down lands on the blank between "more" and "second".
    set_primary_selection(&mut document, Selection::caret(2));
    move_paragraph_down(&mut document, false);
    let blank_one = "first\nmore\n".len();
    assert_eq!(primary_selection(&document), Selection::caret(blank_one));

    // Next paragraph_down lands on the blank before "third".
    move_paragraph_down(&mut document, false);
    let blank_two = "first\nmore\n\nsecond\ntwo\n".len();
    assert_eq!(primary_selection(&document), Selection::caret(blank_two));

    // Past the last blank, paragraph_down clamps to EOF.
    move_paragraph_down(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(text.len()));

    // From EOF, paragraph_up walks back through blanks.
    move_paragraph_up(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(blank_two));

    move_paragraph_up(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(blank_one));

    // No earlier blank — clamps to doc start.
    move_paragraph_up(&mut document, false);
    assert_eq!(primary_selection(&document), Selection::caret(0));
}

#[test]
fn paragraph_motion_extends_selection() {
    let text = "first\n\nsecond";
    let mut document = document(text);
    set_primary_selection(&mut document, Selection::caret(0));

    move_paragraph_down(&mut document, true);
    let blank = "first\n".len();
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 0,
            head: blank
        }
    );
}

#[test]
fn page_motion_steps_by_visible_rows_minus_one() {
    let text = "l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7";
    let mut document = document(text);
    set_primary_selection(&mut document, Selection::caret(0));
    let mut view_state = EditorViewState {
        visible_rows: Some(5),
        ..Default::default()
    };

    move_page(&mut document, &mut view_state, 1, false);
    // Step is 4 → land on line 4 column 0.
    assert_eq!(
        primary_selection(&document),
        Selection::caret("l0\nl1\nl2\nl3\n".len())
    );

    move_page(&mut document, &mut view_state, 1, false);
    assert_eq!(
        primary_selection(&document),
        Selection::caret("l0\nl1\nl2\nl3\nl4\nl5\nl6\n".len())
    );

    // Past EOF clamps to last line.
    move_page(&mut document, &mut view_state, 1, false);
    assert_eq!(
        primary_selection(&document),
        Selection::caret("l0\nl1\nl2\nl3\nl4\nl5\nl6\n".len())
    );

    move_page(&mut document, &mut view_state, -1, false);
    assert_eq!(
        primary_selection(&document),
        Selection::caret("l0\nl1\nl2\n".len())
    );
}

#[test]
fn page_motion_preserves_preferred_column() {
    let text = "abcdefgh\nx\nlong line\n12345678";
    let mut document = document(text);
    // Start at column 6 of line 0.
    set_primary_selection(&mut document, Selection::caret(6));
    let mut view_state = EditorViewState {
        visible_rows: Some(3),
        ..Default::default()
    };

    // Step = 2; lands on line 2 ("long line"), column 6.
    move_page(&mut document, &mut view_state, 1, false);
    assert_eq!(
        primary_selection(&document),
        Selection::caret("abcdefgh\nx\nlong l".len())
    );
    assert_eq!(view_state.preferred_column, Some(6));

    // Step back 2; lands on line 0 column 6 with preferred column intact.
    move_page(&mut document, &mut view_state, -1, false);
    assert_eq!(primary_selection(&document), Selection::caret(6));
    assert_eq!(view_state.preferred_column, Some(6));
}

#[test]
fn page_motion_extends_selection() {
    let text = "a\nb\nc\nd\ne";
    let mut document = document(text);
    set_primary_selection(&mut document, Selection::caret(0));
    let mut view_state = EditorViewState {
        visible_rows: Some(3),
        ..Default::default()
    };

    move_page(&mut document, &mut view_state, 1, true);
    assert_eq!(
        primary_selection(&document),
        Selection {
            anchor: 0,
            head: "a\nb\n".len()
        }
    );
}
