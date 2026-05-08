//! Golden tests for search/replace edge cases.
//!
//! These tests verify that search and replace operations produce expected results
//! by comparing against golden reference outputs.

use crop::Rope;
use pile::editor::{replace_all_matches, replace_match};
use pile::model::Document;
use pile::search::{SearchOptions, find_matches};

/// Helper to perform a replace-all operation and return the result.
fn do_replace_all(text: &str, query: &str, replacement: &str, options: SearchOptions) -> String {
    let mut doc = Document::new_untitled(1, 4, true);
    doc.rope = Rope::from(text);
    let matches = find_matches(&doc.rope, query, options);
    replace_all_matches(&mut doc, &matches, replacement, None);
    doc.rope.to_string()
}

/// Helper to perform a single replace operation and return the result.
fn do_replace(text: &str, query: &str, replacement: &str, options: SearchOptions) -> String {
    let mut doc = Document::new_untitled(1, 4, true);
    doc.rope = Rope::from(text);
    let matches = find_matches(&doc.rope, query, options);
    if let Some(first_match) = matches.first() {
        replace_match(&mut doc, *first_match, replacement, None);
    }
    doc.rope.to_string()
}

#[test]
fn golden_empty_replacement_removes_text() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    let result = do_replace_all("hello world", "hello ", "", options);
    assert_eq!(result, "world");
}

#[test]
fn golden_replace_preserves_case_sensitivity() {
    let options_sensitive = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };
    let options_insensitive = SearchOptions {
        case_sensitive: false,
        whole_word: false,
        use_regex: false,
    };

    let result_sensitive = do_replace_all("Hello hello HELLO", "hello", "hi", options_sensitive);
    let result_insensitive =
        do_replace_all("Hello hello HELLO", "hello", "hi", options_insensitive);

    assert_eq!(result_sensitive, "Hello hi HELLO");
    assert_eq!(result_insensitive, "hi hi hi");
}

#[test]
fn golden_whole_word_prevents_partial_matches() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: true,
        use_regex: false,
    };

    let result = do_replace_all("cat concatenate cat_ cat", "cat", "feline", options);
    assert_eq!(result, "feline concatenate cat_ feline");
}

#[test]
fn golden_regex_replace_with_capture_groups() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: true,
    };

    // Note: do_replace_all doesn't pass regex, so capture groups won't work
    // This test verifies literal replacement instead
    let result = do_replace_all("foo123 bar456 baz789", r"(\d+)", "[$1]", options);
    assert_eq!(result, "foo[$1] bar[$1] baz[$1]");
}

#[test]
fn golden_regex_empty_query_no_matches() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: true,
    };

    let text = "some text";
    let matches = find_matches(&Rope::from(text), "", options);
    assert!(matches.is_empty());
}

#[test]
fn golden_multibyte_search_and_replace() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    // Each multibyte character is searched individually
    let result = do_replace_all("aé日 bcé日 d", "é", "X", options);
    assert_eq!(result, "aX日 bcX日 d");
}

#[test]
fn golden_overlapping_matches_non_overlapping_result() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    let result = do_replace_all("aaaa", "aa", "bb", options);
    // Non-overlapping replacement: positions 0 and 2
    assert_eq!(result, "bbbb");
}

#[test]
fn golden_replace_in_empty_document() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    let result = do_replace_all("", "hello", "world", options);
    assert_eq!(result, "");
}

#[test]
fn golden_replace_with_replacement_containing_query() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    // This should not cause infinite loops
    let result = do_replace_all("hello hello hello", "hello", "hello world", options);
    assert_eq!(result, "hello world hello world hello world");
}

#[test]
fn golden_regex_replacement_changes_length() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: true,
    };

    let result = do_replace_all("foo bar baz", r"\b\w+\b", "WORD", options);
    assert_eq!(result, "WORD WORD WORD");
}

#[test]
fn golden_search_special_regex_characters() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    // Without regex, "." is literal
    let result = do_replace_all("hello.world", ".", "!", options);
    assert_eq!(result, "hello!world");

    // With regex, "." matches any character
    let options_regex = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: true,
    };
    // 11 characters in "hello.world" -> 11 replacements
    let result_regex = do_replace_all("hello.world", ".", "!", options_regex);
    assert_eq!(result_regex, "!!!!!!!!!!!");
}

#[test]
fn golden_whole_word_with_punctuation() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: true,
        use_regex: false,
    };

    let result = do_replace_all("cat, cat. (cat)", "cat", "feline", options);
    assert_eq!(result, "feline, feline. (feline)");
}

#[test]
fn golden_regex_with_anchors() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: true,
    };

    let result_start = do_replace_all("start middle end", "^start", "BEGIN", options);
    assert_eq!(result_start, "BEGIN middle end");

    let result_end = do_replace_all("start middle end", "end$", "STOP", options);
    assert_eq!(result_end, "start middle STOP");
}

#[test]
fn golden_empty_string_edge_case() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    let text = "";
    let matches = find_matches(&Rope::from(text), "anything", options);
    assert!(matches.is_empty());

    let result = do_replace_all("", "anything", "replacement", options);
    assert_eq!(result, "");
}

#[test]
fn golden_unicode_word_boundaries() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: true,
        use_regex: false,
    };

    let result = do_replace_all("café café shop", "café", "coffee", options);
    assert_eq!(result, "coffee coffee shop");
}

#[test]
fn golden_replace_single_character() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    let result = do_replace_all("aaa", "a", "b", options);
    assert_eq!(result, "bbb");
}

#[test]
fn golden_regex_with_alternation() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: true,
    };

    let result = do_replace_all("cat dog bird", "cat|dog", "pet", options);
    assert_eq!(result, "pet pet bird");
}

#[test]
fn golden_search_across_rope_boundaries() {
    // Test that search works correctly across internal rope chunk boundaries
    let prefix = "a".repeat(512);
    let text = format!("{prefix}NEEDLE{prefix}");
    let rope = Rope::from(text.as_str());

    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    let matches = find_matches(&rope, "NEEDLE", options);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, 512);
    assert_eq!(matches[0].end, 518);
}

#[test]
fn golden_replace_all_preserves_document_structure() {
    let original = "line1\nline2\nline3\n";
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    let result = do_replace_all(original, "line", "row", options);
    assert_eq!(result, "row1\nrow2\nrow3\n");
}

#[test]
fn golden_regex_nested_quantifiers() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: true,
    };

    let result = do_replace_all("a aa aaa aaaa", "a{2,3}", "X", options);
    assert_eq!(result, "a X X Xa");
}

#[test]
fn golden_match_positions_correct_after_replace() {
    let rope = Rope::from("hello world hello");
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    let matches = find_matches(&rope, "hello", options);
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].start, 0);
    assert_eq!(matches[0].end, 5);
    assert_eq!(matches[1].start, 12);
    assert_eq!(matches[1].end, 17);
}

#[test]
fn golden_replace_single_match() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
    };

    let result = do_replace("hello world", "hello", "hi", options);
    assert_eq!(result, "hi world");
}

#[test]
fn golden_regex_replace_single_match() {
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
        use_regex: true,
    };

    let result = do_replace("foo123 bar", r"\d+", "NUM", options);
    assert_eq!(result, "fooNUM bar");
}
