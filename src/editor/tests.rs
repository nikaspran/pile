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
fn editor_carets_are_hidden_when_editor_is_unfocused() {
    assert!(!should_paint_editor_carets(false));
    assert!(should_paint_editor_carets(true));
}

#[test]
fn indentation_guides_align_with_text_columns() {
    let layout = TextLayoutPipeline::for_test(
        20.0,
        8.0,
        egui::FontId::monospace(14.0),
        44.0,
        54.0,
        320.0,
        200.0,
        1,
    );

    assert_eq!(indentation_guide_x(&layout, 4, 10.0), 10.0 + 54.0 + 4.0 * 8.0);
}

#[test]
fn indentation_guides_follow_observed_indent_columns() {
    let rope = Rope::from("  one\n  two\n      child\nplain\n");
    assert_eq!(visible_indent_guide_columns(&rope, 0, 4), vec![2, 6]);
    assert_eq!(indentation_guide_columns_for_line(0, &[2, 6]), Vec::<usize>::new());
    assert_eq!(indentation_guide_columns_for_line(2, &[2, 6]), vec![2]);
    assert_eq!(indentation_guide_columns_for_line(6, &[2, 6]), vec![2, 6]);
}

mod editing;
mod layout_tests;
mod motion;
mod replace_undo;
mod selection;
