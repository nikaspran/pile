use crop::Rope;
use regex::Regex;

use crate::{
    editor::word_at_selection,
    model::{Document, DocumentId, Selection},
};

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
    pub document_id: DocumentId,
    pub document_title: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Clone, Debug)]
pub struct SearchResultPreview {
    pub document_id: Option<DocumentId>,
    pub document_title: Option<String>,
    pub line_number: usize,
    pub context_before: String,
    pub matched_text: String,
    pub context_after: String,
}

#[derive(Clone, Debug, Default)]
pub struct SearchState {
    pub visible: bool,
    pub replace_visible: bool,
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
    pub search_all_tabs: bool,
    pub matches: Vec<SearchMatch>,
    pub current_match: Option<usize>,
    pub focus_pending: bool,
    pub selection_pending: bool,
    pub occurrence_selections: Vec<Selection>,
    pub global_results: Vec<GlobalSearchResult>,
    pub global_index: Option<usize>,
    pub preview_items: Vec<SearchResultPreview>,
    pub preview_index: Option<usize>,
    pub preview_visible: bool,
}

impl SearchState {
    pub fn recompute(&mut self, rope: &Rope, documents: &[Document]) {
        let old_range = self
            .current_match
            .and_then(|index| self.matches.get(index).copied());
        let options = self.options();

        if self.search_all_tabs {
            self.global_results = find_matches_in_documents(documents, &self.query, options);
            self.preview_items = build_global_preview_items(documents, &self.global_results, 40);
            self.matches.clear();
            self.current_match = None;
            self.global_index = if self.global_results.is_empty() {
                None
            } else if let Some(old) = old_range {
                self.global_results
                    .iter()
                    .position(|r| r.match_start == old.start && r.match_end == old.end)
                    .or(Some(0))
            } else {
                Some(0)
            };
        } else {
            self.matches = find_matches(rope, &self.query, options);
            self.preview_items = build_preview_items(rope, &self.matches, 40);
            self.global_results.clear();
            self.global_index = None;
            self.current_match = if self.matches.is_empty() {
                None
            } else if let Some(old_range) = old_range {
                self.matches
                    .iter()
                    .position(|range| *range == old_range)
                    .or(Some(0))
            } else {
                Some(0)
            };
        }
        if !self.preview_items.is_empty() {
            self.preview_index = Some(0);
        } else {
            self.preview_index = None;
        }
    }

    pub fn options(&self) -> SearchOptions {
        SearchOptions {
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            use_regex: self.use_regex,
        }
    }

    pub fn replacement_regex(&self) -> Option<Regex> {
        self.use_regex
            .then(|| Regex::new(&self.query).ok())
            .flatten()
    }

    pub fn has_matches(&self) -> bool {
        if self.search_all_tabs {
            !self.global_results.is_empty()
        } else {
            !self.matches.is_empty()
        }
    }

    pub fn current_global_result(&self) -> Option<&GlobalSearchResult> {
        self.global_index
            .and_then(|index| self.global_results.get(index))
    }

    pub fn current_match(&self) -> Option<SearchMatch> {
        self.current_match
            .and_then(|index| self.matches.get(index).copied())
    }

    pub fn current_result_title(&self) -> Option<&str> {
        self.current_global_result()
            .map(|result| result.document_title.as_str())
    }

    pub fn matches_in_document(&self, document_id: DocumentId) -> Vec<SearchMatch> {
        self.global_results
            .iter()
            .filter(|result| result.document_id == document_id)
            .map(|result| SearchMatch {
                start: result.match_start,
                end: result.match_end,
            })
            .collect()
    }

    pub fn next_match(&mut self) {
        if self.search_all_tabs {
            self.global_index = advance_match(self.global_index, self.global_results.len(), 1);
        } else {
            self.current_match = advance_match(self.current_match, self.matches.len(), 1);
        }
        self.selection_pending = true;
    }

    pub fn previous_match(&mut self) {
        if self.search_all_tabs {
            self.global_index = advance_match(self.global_index, self.global_results.len(), -1);
        } else {
            self.current_match = advance_match(self.current_match, self.matches.len(), -1);
        }
        self.selection_pending = true;
    }

    pub fn current_label(&self) -> String {
        if self.search_all_tabs {
            match (self.global_index, self.global_results.len()) {
                (_, 0) => "0 / 0".to_owned(),
                (Some(index), total) => format!("{} / {total}", index + 1),
                (None, total) => format!("0 / {total}"),
            }
        } else {
            match (self.current_match, self.matches.len()) {
                (_, 0) => "0 / 0".to_owned(),
                (Some(index), total) => format!("{} / {total}", index + 1),
                (None, total) => format!("0 / {total}"),
            }
        }
    }

    pub fn select_next_occurrence(&mut self, rope: &Rope, primary: Selection) {
        let query = if let Some((start, end)) = word_at_selection(rope, primary) {
            let text = rope.byte_slice(start..end).to_string();
            if text.is_empty() {
                return;
            }
            text
        } else {
            return;
        };

        if self.occurrence_selections.is_empty() {
            self.occurrence_selections.push(primary);
            self.query = query.clone();
        }

        let matches = find_matches(rope, &query, self.options());

        let all_selected: std::collections::HashSet<_> = self
            .occurrence_selections
            .iter()
            .map(|s| (s.anchor.min(s.head), s.anchor.max(s.head)))
            .collect();

        let next = matches
            .iter()
            .find(|m| !all_selected.contains(&(m.start, m.end)))
            .copied();

        if let Some(m) = next {
            self.occurrence_selections.push(Selection {
                anchor: m.start,
                head: m.end,
            });
        }
    }

    pub fn find_under_cursor(&mut self, rope: &Rope, primary: Selection) {
        self.occurrence_selections.clear();
        let (start, end) = if let Some((s, e)) = word_at_selection(rope, primary) {
            (s, e)
        } else {
            return;
        };
        if start == end {
            return;
        }
        let text = rope.byte_slice(start..end).to_string();
        if text.is_empty() {
            return;
        }
        self.query = text;
        self.recompute(rope, &[]);
        self.occurrence_selections.push(Selection {
            anchor: start,
            head: end,
        });
    }
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
    let window_size = SEARCH_WINDOW_BYTES;
    let overlap_size = window_size / 2;
    let mut matches = Vec::new();
    let mut window_start = 0;
    let mut last_emitted_start = 0;

    while window_start < rope_len {
        let window_end = floor_char_boundary(rope, (window_start + window_size).min(rope_len));
        let window_end = if window_end <= window_start {
            next_char_boundary(rope, window_start)
        } else {
            window_end
        };

        let window_text = rope.byte_slice(window_start..window_end).to_string();

        let mut search_start = 0;
        while let Some(capture) = regex.find_at(&window_text, search_start) {
            let local_start = capture.start();
            let local_end = capture.end();
            let abs_start = window_start + local_start;
            let abs_end = window_start + local_end;

            if abs_start >= last_emitted_start
                && (!options.whole_word || is_whole_word_match(rope, abs_start, abs_end))
            {
                matches.push(SearchMatch {
                    start: abs_start,
                    end: abs_end,
                });
                last_emitted_start = abs_start + 1;
            }

            if local_end <= search_start {
                break;
            }
            search_start = local_end;
        }

        if window_end >= rope_len {
            break;
        }
        window_start = floor_char_boundary(rope, (window_end - overlap_size).max(window_start));
        if window_start >= window_end {
            window_start = next_char_boundary(rope, window_end);
        }
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
    documents: &[Document],
    query: &str,
    options: SearchOptions,
) -> Vec<GlobalSearchResult> {
    let mut results = Vec::new();

    for document in documents {
        let matches = find_matches(&document.rope, query, options);
        for m in matches {
            results.push(GlobalSearchResult {
                document_id: document.id,
                document_title: document.display_title(),
                match_start: m.start,
                match_end: m.end,
            });
        }
    }

    results
}

pub fn build_preview_items(rope: &Rope, matches: &[SearchMatch], context_chars: usize) -> Vec<SearchResultPreview> {
    let mut items = Vec::new();
    let rope_len = rope.byte_len();

    for m in matches {
        let line_number = rope.byte_slice(..m.start).lines().count();

        let context_start = m.start.saturating_sub(context_chars);
        let context_start = floor_char_boundary(rope, context_start);
        let context_end = (m.end + context_chars).min(rope_len);
        let context_end = floor_char_boundary(rope, context_end);

        let before = rope.byte_slice(context_start..m.start).to_string();
        let matched = rope.byte_slice(m.start..m.end).to_string();
        let after = rope.byte_slice(m.end..context_end).to_string();

        items.push(SearchResultPreview {
            document_id: None,
            document_title: None,
            line_number,
            context_before: before,
            matched_text: matched,
            context_after: after,
        });
    }

    items
}

pub fn build_global_preview_items(
    documents: &[Document],
    results: &[GlobalSearchResult],
    context_chars: usize,
) -> Vec<SearchResultPreview> {
    use std::collections::HashMap;
    let doc_map: HashMap<DocumentId, &Document> = documents
        .iter()
        .map(|d| (d.id, d))
        .collect();
    let mut items = Vec::new();

    for r in results {
        let Some(document) = doc_map.get(&r.document_id) else {
            continue;
        };
        let rope = &document.rope;
        let rope_len = rope.byte_len();
        let line_number = rope.byte_slice(..r.match_start).lines().count();

        let context_start = r.match_start.saturating_sub(context_chars);
        let context_start = floor_char_boundary(rope, context_start);
        let context_end = (r.match_end + context_chars).min(rope_len);
        let context_end = floor_char_boundary(rope, context_end);

        let before = rope.byte_slice(context_start..r.match_start).to_string();
        let matched = rope.byte_slice(r.match_start..r.match_end).to_string();
        let after = rope.byte_slice(r.match_end..context_end).to_string();

        items.push(SearchResultPreview {
            document_id: Some(r.document_id),
            document_title: Some(r.document_title.clone()),
            line_number,
            context_before: before,
            matched_text: matched,
            context_after: after,
        });
    }

    items
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

    fn search_state() -> SearchState {
        SearchState::default()
    }

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
    fn select_next_occurrence_adds_first_word() {
        let rope = Rope::from("hello world hello");
        let primary = Selection { anchor: 0, head: 5 };
        let mut state = search_state();

        state.select_next_occurrence(&rope, primary);

        assert_eq!(state.occurrence_selections.len(), 2);
        assert_eq!(state.occurrence_selections[0], primary);
        assert_eq!(state.occurrence_selections[1].anchor, 12);
        assert_eq!(state.occurrence_selections[1].head, 17);
    }

    #[test]
    fn find_under_cursor_selects_word() {
        let rope = Rope::from("hello world hello");
        let primary = Selection::caret(0);
        let mut state = search_state();

        state.find_under_cursor(&rope, primary);

        assert_eq!(state.occurrence_selections.len(), 1);
        let sel = state.occurrence_selections[0];
        assert_eq!((sel.anchor, sel.head), (0, 5));
        assert_eq!(state.query, "hello");
    }

    #[test]
    fn find_under_cursor_clears_previous() {
        let rope = Rope::from("hello world");
        let primary = Selection::caret(6);
        let mut state = search_state();
        state.query = "previous".to_owned();
        state
            .occurrence_selections
            .push(Selection { anchor: 0, head: 5 });

        state.find_under_cursor(&rope, primary);

        assert_eq!(state.occurrence_selections.len(), 1);
        let sel = state.occurrence_selections[0];
        assert_eq!((sel.anchor, sel.head), (6, 11));
        assert_eq!(state.query, "world");
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
    fn regex_search_finds_matches_across_window_boundaries() {
        let prefix = "a".repeat(SEARCH_WINDOW_BYTES - 2);
        let text = format!("{prefix}needle");
        let matches = find_matches(
            &Rope::from(text),
            "ne+dle",
            SearchOptions {
                case_sensitive: true,
                whole_word: false,
                use_regex: true,
            },
        );

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
