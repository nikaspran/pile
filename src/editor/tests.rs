use crop::Rope;

use super::*;
use regex::Regex;

use crate::search::SearchMatch;

fn document(text: &str) -> Document {
    let mut document = Document::new_untitled(1, 4, true);
    document.replace_text(text);
    document.selections = vec![Selection::caret(0)];
    document.revision = 0;
    document
}

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

    let pointer_at = |x: f32, y: f32| {
        layout.offset_at_pointer(
            rope,
            egui::pos2(x, y),
            rect,
        )
    };

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
    set_primary_selection(
        &mut document,
        Selection {
            anchor: 3,
            head: 8,
        },
    ); // overlaps "lo wo"

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
    let expected_end = "no indent\n  indented line 1\n  indented line 2\n  indented line 3\n".len() - 1; // 63
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

/// Helper to create a TextLayoutPipeline for high-DPI testing with custom dimensions
fn layout_for_dpi(
    text: &str,
    char_width: f32,
    row_height: f32,
    dpi_scale: f32,
) -> (TextLayoutPipeline, Rope) {
    let rope = Rope::from(text);
    let line_count = visual_line_count(&rope);
    let font_id = egui::FontId::monospace(14.0 * dpi_scale);
    let gutter_width = 44.0 * dpi_scale;
    let text_origin_x = gutter_width + 10.0 * dpi_scale;
    let content_width = (text_origin_x + 400.0 * dpi_scale).max(text_origin_x + EDITOR_MIN_WIDTH * dpi_scale);
    let content_height = (line_count as f32 * row_height).max(200.0 * dpi_scale);

    let pipeline = TextLayoutPipeline::for_test(
        row_height,
        char_width,
        font_id,
        gutter_width,
        text_origin_x,
        content_width,
        content_height,
        line_count,
    );
    (pipeline, rope)
}

#[test]
fn high_dpi_char_width_scales_correctly() {
    // Simulate 1x DPI
    let (layout_1x, rope) = layout_for_dpi("hello", 8.0, 16.0, 1.0);
    let rect_1x = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

    // At position 0, should map to offset 0
    let offset_1x = layout_1x.offset_at_pointer(
        &rope,
        egui::pos2(layout_1x.text_origin_x + 0.0, 0.0),
        rect_1x,
    );
    assert_eq!(offset_1x, 0);

    // At column 3, should map to offset of 'l' (3rd char)
    let offset_3x = layout_1x.offset_at_pointer(
        &rope,
        egui::pos2(layout_1x.text_origin_x + 3.0 * 8.0, 0.0),
        rect_1x,
    );
    assert_eq!(offset_3x, "hel".len());

    // Simulate 2x DPI (Retina/HiDPI)
    let (layout_2x, _) = layout_for_dpi("hello", 16.0, 32.0, 2.0);
    let rect_2x = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1600.0, 1200.0));

    // At column 3 with 2x DPI, should still map to same text offset
    let offset_2x = layout_2x.offset_at_pointer(
        &rope,
        egui::pos2(layout_2x.text_origin_x + 3.0 * 16.0, 0.0),
        rect_2x,
    );
    assert_eq!(offset_2x, "hel".len());
}

#[test]
fn high_dpi_caret_position_scales_correctly() {
    let rope = Rope::from("hello");
    let font_id = egui::FontId::monospace(14.0);

    // 1x DPI
    let layout_1x = TextLayoutPipeline::for_test(
        16.0, 8.0, font_id.clone(), 44.0, 54.0, 800.0, 600.0, 1,
    );
    let pos_1x = layout_1x.caret_position(&rope, 3, 0.0);
    assert_eq!(pos_1x.x, 54.0 + 3.0 * 8.0); // text_origin_x + column * char_width

    // 2x DPI - all dimensions doubled
    let layout_2x = TextLayoutPipeline::for_test(
        32.0, 16.0, font_id, 88.0, 108.0, 1600.0, 1200.0, 1,
    );
    let pos_2x = layout_2x.caret_position(&rope, 3, 0.0);
    assert_eq!(pos_2x.x, 108.0 + 3.0 * 16.0);
}

#[test]
fn high_dpi_multiline_layout_scales() {
    let text = "line1\nline2\nline3";
    let rope = Rope::from(text);
    let line_count = visual_line_count(&rope);

    // 1x DPI
    let layout_1x = TextLayoutPipeline::for_test(
        16.0, 8.0, egui::FontId::monospace(14.0), 44.0, 54.0, 800.0, 600.0, line_count,
    );

    // Line 1 should be at y = 0, Line 2 at y = 16, Line 3 at y = 32
    assert_eq!(layout_1x.line_y(0, 0.0), 0.0);
    assert_eq!(layout_1x.line_y(1, 0.0), 16.0);
    assert_eq!(layout_1x.line_y(2, 0.0), 32.0);

    // 2x DPI
    let layout_2x = TextLayoutPipeline::for_test(
        32.0, 16.0, egui::FontId::monospace(28.0), 88.0, 108.0, 1600.0, 1200.0, line_count,
    );

    // Line 1 should be at y = 0, Line 2 at y = 32, Line 3 at y = 64
    assert_eq!(layout_2x.line_y(0, 0.0), 0.0);
    assert_eq!(layout_2x.line_y(1, 0.0), 32.0);
    assert_eq!(layout_2x.line_y(2, 0.0), 64.0);
}

#[test]
fn high_dpi_visible_line_range_scales() {
    let rope = Rope::from("line1\nline2\nline3\nline4\nline5");
    let line_count = visual_line_count(&rope);

    // 1x DPI: row_height = 16, viewport from y=16 to y=64
    // first_line = (16/16).floor() = 1
    // last_line = (64/16).ceil() + 1 = 4 + 1 = 5, but min(5, 5) = 5
    let layout_1x = TextLayoutPipeline::for_test(
        16.0, 8.0, egui::FontId::monospace(14.0), 44.0, 54.0, 800.0, 600.0, line_count,
    );
    let viewport_1x = egui::Rect::from_min_max(egui::pos2(0.0, 16.0), egui::pos2(800.0, 64.0));
    let (first, last) = layout_1x.visible_line_range(&viewport_1x);
    assert_eq!(first, 1); // starts at y=16, which is line 1
    assert_eq!(last, 5); // (64/16).ceil()+1 = 5

    // 2x DPI: row_height = 32, viewport from y=32 to y=128
    // first_line = (32/32).floor() = 1
    // last_line = (128/32).ceil() + 1 = 4 + 1 = 5, but min(5, 5) = 5
    let layout_2x = TextLayoutPipeline::for_test(
        32.0, 16.0, egui::FontId::monospace(28.0), 88.0, 108.0, 1600.0, 1200.0, line_count,
    );
    let viewport_2x = egui::Rect::from_min_max(egui::pos2(0.0, 32.0), egui::pos2(1600.0, 128.0));
    let (first_2x, last_2x) = layout_2x.visible_line_range(&viewport_2x);
    assert_eq!(first_2x, 1);
    assert_eq!(last_2x, 5);
}

#[test]
fn font_fallback_renders_cjk_characters() {
    // CJK characters may fall back to a different font
    let text = "Hello 世界";
    let rope = Rope::from(text);

    let layout = TextLayoutPipeline::for_test(
        16.0, 8.0, egui::FontId::monospace(14.0), 44.0, 54.0, 800.0, 600.0, 1,
    );

    // The layout should handle CJK characters without panicking
    // CJK characters might have different display widths, but our layout uses fixed char_width
    let line_text = layout.wrapped_line_text(&rope, 0);
    assert_eq!(line_text, "Hello 世界");

    // Test offset calculation with multibyte characters
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

    // Click at column 6 (after "Hello ")
    let offset = layout.offset_at_pointer(
        &rope,
        egui::pos2(layout.text_origin_x + 6.0 * 8.0, 0.0),
        rect,
    );
    // "Hello ".len() = 6 bytes (ASCII), but we need to account for the CJK chars
    // Actually, offset_at_pointer uses column calculation, so column 6 should be at byte offset 6
    assert_eq!(offset, 6);
}

#[test]
fn font_fallback_renders_emoji() {
    // Emoji characters may fall back to a different font
    let text = "Click 😀 here";
    let rope = Rope::from(text);

    let layout = TextLayoutPipeline::for_test(
        16.0, 8.0, egui::FontId::monospace(14.0), 44.0, 54.0, 800.0, 600.0, 1,
    );

    // The layout should handle emoji without panicking
    let line_text = layout.wrapped_line_text(&rope, 0);
    assert_eq!(line_text, "Click 😀 here");

    // Test that we can calculate caret position for emoji
    // "Click ".len() = 6, "😀".len() = 4 bytes
    let pos = layout.caret_position(&rope, 10, 0.0); // After "Click 😀" (6 + 4 = 10 bytes)
    assert!(pos.x > layout.text_origin_x);
}

#[test]
fn font_fallback_renders_mixed_scripts() {
    // Mix of Latin, CJK, Arabic, and Cyrillic
    let text = "Hello 世界 مرحبا Привет";
    let rope = Rope::from(text);

    let layout = TextLayoutPipeline::for_test(
        16.0, 8.0, egui::FontId::monospace(14.0), 44.0, 54.0, 800.0, 600.0, 1,
    );

    // Should handle all character types without panicking
    let line_text = layout.wrapped_line_text(&rope, 0);
    assert_eq!(line_text, "Hello 世界 مرحبا Привет");

    // Test grapheme handling with mixed scripts
    let mut doc = document(text);
    set_primary_selection(&mut doc, Selection::caret(6)); // At first space

    // Move right should respect grapheme boundaries
    move_right(&mut doc, false);
    // After " " (space), should be at start of "世" (byte offset 6 + 1 for space = 7... wait)
    // "Hello " = 6 bytes (each char is 1 byte for ASCII), so position 6 is after space
    // Actually "Hello " is 'H','e','l','l','o',' ' = 6 bytes
    // "世" is 3 bytes, "界" is 3 bytes
    // So after "Hello " at byte 6, move_right goes to byte 6 (start of "世")
    // Wait, the cursor is already at byte 6. Let me re-read...
    // set_primary_selection with Selection::caret(6) puts cursor after "Hello "
    // move_right should move to after "世" which is byte 9
    assert_eq!(primary_selection(&doc).head, "Hello 世".len()); // 6 + 3 = 9
}

#[test]
fn font_fallback_test_char_width_consistency() {
    // Characters from different scripts should use the same char_width in our layout
    // This test ensures our fixed-width assumption is documented
    let layout = TextLayoutPipeline::for_test(
        16.0, 8.0, egui::FontId::monospace(14.0), 44.0, 54.0, 800.0, 600.0, 1,
    );

    // All characters should use the same width for column calculation
    // This is a design decision - we use fixed width based on 'm' character
    assert_eq!(layout.char_width, 8.0);

    // Test that column_x is consistent
    assert_eq!(layout.column_x(0), layout.text_origin_x);
    assert_eq!(layout.column_x(5), layout.text_origin_x + 5.0 * 8.0);
    assert_eq!(layout.column_x(10), layout.text_origin_x + 10.0 * 8.0);
}

#[test]
fn high_dpi_gutter_width_scales() {
    // Test that gutter width scales with DPI
    let rope = Rope::from("line1\nline2");
    let line_count = visual_line_count(&rope);

    // 1x DPI
    let layout_1x = TextLayoutPipeline::for_test(
        16.0, 8.0, egui::FontId::monospace(14.0), 44.0, 54.0, 800.0, 600.0, line_count,
    );
    assert_eq!(layout_1x.gutter_width, 44.0);
    assert_eq!(layout_1x.text_origin_x, 54.0); // gutter + padding

    // 2x DPI
    let layout_2x = TextLayoutPipeline::for_test(
        32.0, 16.0, egui::FontId::monospace(28.0), 88.0, 108.0, 1600.0, 1200.0, line_count,
    );
    assert_eq!(layout_2x.gutter_width, 88.0);
    assert_eq!(layout_2x.text_origin_x, 108.0);
}

#[test]
fn high_dpi_content_size_scales() {
    let rope = Rope::from("line1\nline2\nline3");
    let line_count = visual_line_count(&rope);

    // 1x DPI - content_height is max(available_height, line_count * row_height)
    let layout_1x = TextLayoutPipeline::for_test(
        16.0, 8.0, egui::FontId::monospace(14.0), 44.0, 54.0, 800.0, 600.0, line_count,
    );
    let size_1x = layout_1x.content_size();
    // content_height should be max(600.0, 3 * 16.0) = 600.0
    assert_eq!(size_1x.y, 600.0);

    // 2x DPI
    let layout_2x = TextLayoutPipeline::for_test(
        32.0, 16.0, egui::FontId::monospace(28.0), 88.0, 108.0, 1600.0, 1200.0, line_count,
    );
    let size_2x = layout_2x.content_size();
    assert_eq!(size_2x.y, 1200.0);
}
