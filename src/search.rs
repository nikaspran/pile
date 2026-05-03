use crop::Rope;
use regex::Regex;
use uuid::Uuid;

const SEARCH_WINDOW_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchMatch {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct GlobalSearchResult {
    pub document_id: Uuid,
    pub document_title: String,
    pub match_start: usize,
    pub match_end: usize,
}

pub fn find_matches(rope: &Rope, query: &str, options: SearchOptions) -> Vec<SearchMatch> {
    if query.is_empty() || rope.byte_len() == 0 {
        return Vec::new();
    }

    if options.use_regex {
        return find_regex_matches(rope, query, options);
    }

    let needle = if options.case_sensitive {
        query.to_owned()
    } else {
        query.to_ascii_lowercase()
    };
    let body_len = SEARCH_WINDOW_BYTES.max(query.len());
    let overlap_len = query.len().saturating_sub(1);
    let rope_len = rope.byte_len();
    let mut matches = Vec::new();
    let mut window_start = 0;
    let mut emit_from = 0;

    while window_start < rope_len {
        let body_end = floor_char_boundary(rope, (window_start + body_len).min(rope_len));
        let body_end = if body_end <= window_start {
            next_char_boundary(rope, window_start)
        } else {
            body_end
        };
        let window_end = floor_char_boundary(rope, (body_end + overlap_len).min(rope_len));
        let window_end = if window_end < body_end {
            body_end
        } else {
            window_end
        };

        let window = rope.byte_slice(window_start..window_end).to_string();
        let haystack = if options.case_sensitive {
            window.clone()
        } else {
            window.to_ascii_lowercase()
        };

        let mut search_from = 0;
        while let Some(relative_start) = haystack[search_from..].find(&needle) {
            let local_start = search_from + relative_start;
            let start = window_start + local_start;
            let end = start + query.len();

            if start >= emit_from
                && start < body_end
                && end <= rope_len
                && (!options.whole_word || is_whole_word_match(rope, start, end))
            {
                matches.push(SearchMatch { start, end });
            }

            search_from = local_start + query.len();
        }

        if body_end >= rope_len {
            break;
        }
        emit_from = body_end;
        window_start = body_end;
    }

    matches
}

fn find_regex_matches(rope: &Rope, query: &str, options: SearchOptions) -> Vec<SearchMatch> {
    let regex = Regex::new(query).ok();
    let Some(regex) = regex.as_ref() else {
        return Vec::new();
    };

    let rope_len = rope.byte_len();
    let mut matches = Vec::new();
    let window_size = SEARCH_WINDOW_BYTES;
    let mut window_start: usize = 0;

    while window_start < rope_len {
        let window_end = floor_char_boundary(rope, (window_start + window_size).min(rope_len));
        let window_end = if window_end <= window_start {
            next_char_boundary(rope, window_start)
        } else {
            window_end
        };

        let window = rope.byte_slice(window_start..window_end).to_string();
        let mut last_end: usize = 0;

        for capture in regex.find_iter(&window) {
            let local_start = capture.start();
            let local_end = capture.end();

            if local_start < last_end {
                continue;
            }
            last_end = local_end;

            let start = window_start + local_start;
            let end = window_start + local_end;

            if (!options.whole_word || is_whole_word_match(rope, start, end))
                && end <= rope_len
            {
                matches.push(SearchMatch { start, end });
            }
        }

        if window_end >= rope_len {
            break;
        }
        window_start = window_end;
    }

    matches
}

pub fn advance_match(current: Option<usize>, total: usize, delta: isize) -> Option<usize> {
    if total == 0 {
        return None;
    }

    let Some(current) = current else {
        return Some(if delta < 0 { total - 1 } else { 0 });
    };

    let current = current as isize;
    let total = total as isize;
    Some((current + delta).rem_euclid(total) as usize)
}

pub fn find_matches_in_documents(
    documents: &[crate::model::Document],
    query: &str,
    options: SearchOptions,
) -> Vec<GlobalSearchResult> {
    let mut results = Vec::new();

    for document in documents {
        let matches = find_matches(&document.rope, query, options);
        for m in matches {
            results.push(GlobalSearchResult {
                document_id: document.id,
                document_title: document.title_hint.clone(),
                match_start: m.start,
                match_end: m.end,
            });
        }
    }

    results
}

fn is_whole_word_match(rope: &Rope, start: usize, end: usize) -> bool {
    let before = rope.byte_slice(..start).chars().next_back();
    let after = rope.byte_slice(end..).chars().next();

    !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
}

fn is_word_char(char: char) -> bool {
    char.is_alphanumeric() || char == '_'
}

fn floor_char_boundary(rope: &Rope, mut offset: usize) -> usize {
    offset = offset.min(rope.byte_len());
    while offset > 0 && !rope.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn next_char_boundary(rope: &Rope, offset: usize) -> usize {
    let offset = floor_char_boundary(rope, offset);
    if offset >= rope.byte_len() {
        return rope.byte_len();
    }

    rope.byte_slice(offset..)
        .chars()
        .next()
        .map_or(rope.byte_len(), |char| offset + char.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(case_sensitive: bool, whole_word: bool) -> SearchOptions {
        SearchOptions {
            case_sensitive,
            whole_word,
            use_regex: false,
        }
    }

    #[test]
    fn search_handles_regex() {
        let matches = find_matches(
            &Rope::from("foo123 bar456 baz789"),
            r"\d+",
            SearchOptions {
                case_sensitive: false,
                whole_word: false,
                use_regex: true,
            },
        );

        assert_eq!(
            matches,
            vec![
                SearchMatch { start: 3, end: 6 },
                SearchMatch { start: 10, end: 13 },
                SearchMatch { start: 17, end: 20 },
            ]
        );
    }

    #[test]
    fn search_handles_regex_empty_query() {
        assert!(
            find_matches(
                &Rope::from("text"),
                "",
                SearchOptions {
                    case_sensitive: false,
                    whole_word: false,
                    use_regex: true,
                },
            )
            .is_empty()
        );
    }

    #[test]
    fn search_handles_invalid_regex() {
        let matches = find_matches(
            &Rope::from("text"),
            r"[invalid",
            SearchOptions {
                case_sensitive: false,
                whole_word: false,
                use_regex: true,
            },
        );
        assert!(matches.is_empty());
    }

    #[test]
    fn search_regex_with_whole_word() {
        let matches = find_matches(
            &Rope::from("cat concatenate cat_ cat"),
            r"\bcat\b",
            SearchOptions {
                case_sensitive: true,
                whole_word: false,
                use_regex: true,
            },
        );

        assert_eq!(
            matches,
            vec![
                SearchMatch { start: 0, end: 3 },
                SearchMatch { start: 21, end: 24 },
            ]
        );
    }

    #[test]
    fn search_returns_non_overlapping_matches() {
        let matches = find_matches(&Rope::from("aaaa"), "aa", options(true, false));

        assert_eq!(
            matches,
            vec![
                SearchMatch { start: 0, end: 2 },
                SearchMatch { start: 2, end: 4 }
            ]
        );
    }

    #[test]
    fn search_handles_case_sensitivity() {
        let rope = Rope::from("Hello hello");

        assert_eq!(find_matches(&rope, "hello", options(true, false)).len(), 1);
        assert_eq!(find_matches(&rope, "hello", options(false, false)).len(), 2);
    }

    #[test]
    fn search_can_restrict_to_whole_words() {
        let matches = find_matches(
            &Rope::from("cat concatenate cat_ cat"),
            "cat",
            options(true, true),
        );

        assert_eq!(
            matches,
            vec![
                SearchMatch { start: 0, end: 3 },
                SearchMatch { start: 21, end: 24 }
            ]
        );
    }

    #[test]
    fn search_handles_empty_query() {
        assert!(find_matches(&Rope::from("text"), "", options(true, false)).is_empty());
    }

    #[test]
    fn search_reports_byte_offsets_for_multibyte_text() {
        let rope = Rope::from("aé日 aé日");
        let matches = find_matches(&rope, "é日", options(true, false));

        assert_eq!(
            matches,
            vec![
                SearchMatch { start: 1, end: 6 },
                SearchMatch { start: 8, end: 13 }
            ]
        );
    }

    #[test]
    fn search_finds_matches_across_window_boundaries() {
        let prefix = "a".repeat(SEARCH_WINDOW_BYTES - 2);
        let text = format!("{prefix}needle");
        let matches = find_matches(&Rope::from(text), "needle", options(true, false));

        assert_eq!(
            matches,
            vec![SearchMatch {
                start: SEARCH_WINDOW_BYTES - 2,
                end: SEARCH_WINDOW_BYTES + 4
            }]
        );
    }

    #[test]
    fn search_navigation_wraps() {
        assert_eq!(advance_match(None, 3, 1), Some(0));
        assert_eq!(advance_match(None, 3, -1), Some(2));
        assert_eq!(advance_match(Some(2), 3, 1), Some(0));
        assert_eq!(advance_match(Some(0), 3, -1), Some(2));
        assert_eq!(advance_match(Some(0), 0, 1), None);
    }

    #[test]
    fn find_matches_in_documents_finds_across_tabs() {
        let mut doc1 = crate::model::Document::new_untitled(1);
        doc1.rope = Rope::from("hello world");
        doc1.title_hint = "Doc 1".to_owned();

        let mut doc2 = crate::model::Document::new_untitled(2);
        doc2.rope = Rope::from("foo hello bar");
        doc2.title_hint = "Doc 2".to_owned();

        let documents = vec![doc1.clone(), doc2.clone()];
        let results = find_matches_in_documents(
            &documents,
            "hello",
            SearchOptions {
                case_sensitive: true,
                whole_word: false,
                use_regex: false,
            },
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].document_id, doc1.id);
        assert_eq!(results[0].match_start, 0);
        assert_eq!(results[0].match_end, 5);
        assert_eq!(results[1].document_id, doc2.id);
        assert_eq!(results[1].match_start, 4);
        assert_eq!(results[1].match_end, 9);
    }
}
