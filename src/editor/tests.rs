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

mod editing;
mod layout_tests;
mod motion;
mod replace_undo;
mod selection;
