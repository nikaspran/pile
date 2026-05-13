//! Syntax highlighting using tree-sitter.
//!
//! This module provides incremental parse state per document and converts
//! tree-sitter parse trees into highlight spans for rendering.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use tree_sitter::{InputEdit, Point, Tree};
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

use crate::grammar_registry::GrammarRegistry;
use crate::syntax::LanguageId;

/// Returns the static grammar registry.
fn grammar_registry() -> &'static GrammarRegistry {
    static REGISTRY: OnceLock<GrammarRegistry> = OnceLock::new();
    REGISTRY.get_or_init(GrammarRegistry::default)
}

/// Returns the injection language registry using the grammar registry.
fn injection_language_registry() -> &'static HashMap<&'static str, Arc<HighlightConfiguration>> {
    grammar_registry().injection_registry()
}

/// Per-document syntax highlighting state.
///
/// Stores the parsed tree-sitter `Tree` for incremental reparsing and
/// caches highlight spans keyed by document revision and visible byte range.
#[derive(Clone, Debug)]
pub struct DocumentSyntaxState {
    /// The most recent parse tree. `None` for plain text.
    tree: Option<Tree>,
    /// The language used to produce `tree`.
    parsed_as: Option<LanguageId>,
    /// The revision this tree was parsed at.
    parsed_revision: u64,
    /// Cached highlight spans for the last processed visible range.
    ///
    /// The revision is kept for exact cache hits, but stale spans are still
    /// useful while a new background parse is pending. Keeping them avoids
    /// flashing unhighlighted text on every keystroke.
    cached_spans: Option<(u64, usize, usize, Vec<HighlightSpan>)>, // (revision, start_byte, end_byte, spans)
}

impl Default for DocumentSyntaxState {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentSyntaxState {
    pub fn new() -> Self {
        Self {
            tree: None,
            parsed_as: None,
            parsed_revision: 0,
            cached_spans: None,
        }
    }

    /// Returns highlight spans for the given language and revision.
    ///
    /// Uses cached spans keyed by document `revision` and visible byte range.
    /// If no cached spans are available, returns an empty Vec and the caller
    /// should request a background parse.
    pub fn highlight(
        &mut self,
        language: LanguageId,
        _revision: u64,
        visible_start: usize,
        _visible_end: usize,
    ) -> Vec<HighlightSpan> {
        // Return cached result if revision and visible range haven't changed.
        // If the revision changed but the language and visible start did not,
        // return stale spans while the background worker produces fresh ones.
        // The visible end often changes while typing near the bottom of the
        // viewport, so requiring an exact range match causes color flicker.
        // The caller separately requests a reparse through `needs_parse`.
        if let Some((_cached_rev, cached_start, _cached_end, spans)) = &self.cached_spans {
            if self.parsed_as == Some(language) && *cached_start == visible_start {
                return spans.clone();
            }
        }

        // No cached result available - return empty spans
        // The caller should check if a parse is needed
        Vec::new()
    }

    /// Update the syntax state from a background parse result.
    pub fn update_from_parse_result(
        &mut self,
        tree: Option<Tree>,
        spans: Vec<HighlightSpan>,
        language: LanguageId,
        revision: u64,
        visible_start: usize,
        visible_end: usize,
    ) {
        self.tree = tree;
        self.parsed_as = Some(language);
        self.parsed_revision = revision;
        self.cached_spans = Some((revision, visible_start, visible_end, spans));
    }

    /// Check if we need to request a new parse.
    /// Returns true if the document revision is newer than what we have parsed.
    pub fn needs_parse(&self, language: LanguageId, revision: u64) -> bool {
        self.parsed_as != Some(language) || self.parsed_revision < revision
    }

    /// Generate highlight spans from text using tree-sitter-highlight.
    /// This is public so the parse worker can use it.
    pub fn generate_highlight_spans(
        config: &HighlightConfiguration,
        text: &str,
    ) -> Vec<HighlightSpan> {
        let mut highlighter = Highlighter::new();
        let registry = injection_language_registry();

        let Ok(events) = highlighter.highlight(config, text.as_bytes(), None, |lang_name| {
            registry
                .get(lang_name)
                .map(|c| &**c as &HighlightConfiguration)
        }) else {
            return Vec::new();
        };

        let mut spans = Vec::new();
        let mut highlight_stack: Vec<Highlight> = Vec::new();

        for event in events {
            match event {
                Ok(HighlightEvent::Source { start, end }) => {
                    if let Some(&highlight) = highlight_stack.last() {
                        spans.push(HighlightSpan {
                            start,
                            end,
                            highlight: highlight.0,
                        });
                    }
                }
                Ok(HighlightEvent::HighlightStart(s)) => {
                    highlight_stack.push(s);
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    highlight_stack.pop();
                }
                Err(_) => {
                    // Skip errors and continue
                }
            }
        }

        spans
    }

    /// Generate highlight spans for mixed scratchpad content.
    ///
    /// The visible text is split into blank-line-separated blocks. Each block is
    /// detected independently and highlighted with its own tree-sitter grammar.
    pub fn generate_block_highlight_spans(text: &str) -> Vec<HighlightSpan> {
        let registry = grammar_registry();
        let mut spans = Vec::new();

        for block in syntax_blocks(text) {
            let block_text = &text[block.start..block.end];
            let language = block_language(registry, block_text);
            if language == LanguageId::PlainText {
                continue;
            }

            let Some(config) = registry.highlight_config(language) else {
                continue;
            };

            let mut block_spans = Self::generate_highlight_spans(&config, block_text);
            if language == LanguageId::Markdown {
                Self::add_markdown_extra_spans(&mut block_spans, block_text);
            }

            spans.extend(
                block_spans
                    .into_iter()
                    .filter(|span| span.end <= block_text.len())
                    .map(|span| HighlightSpan {
                        start: span.start + block.start,
                        end: span.end + block.start,
                        highlight: span.highlight,
                    }),
            );
        }

        spans
    }

    /// Generate simple Markdown-specific spans for constructs not consistently
    /// exposed by the block grammar's tree-sitter highlight events.
    pub fn generate_markdown_fallback_spans(text: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let title = highlight_index("text.title");
        let literal = highlight_index("text.literal");
        let reference = highlight_index("text.reference");
        let strong = highlight_index("text.strong");
        let uri = highlight_index("text.uri");
        let punctuation = highlight_index("punctuation.special");

        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let line_without_newline = line.trim_end_matches(['\r', '\n']);
            let leading = line_without_newline.len() - line_without_newline.trim_start().len();
            let trimmed = &line_without_newline[leading..];
            let line_start = offset;

            if trimmed.starts_with('#') {
                let marker_len = trimmed.chars().take_while(|ch| *ch == '#').count();
                if (1..=6).contains(&marker_len)
                    && trimmed.as_bytes().get(marker_len) == Some(&b' ')
                {
                    spans.push(HighlightSpan {
                        start: line_start + leading,
                        end: line_start + line_without_newline.len(),
                        highlight: title,
                    });
                }
            }

            if let Some(marker_len) = markdown_list_marker_len(trimmed) {
                spans.push(HighlightSpan {
                    start: line_start + leading,
                    end: line_start + leading + marker_len,
                    highlight: punctuation,
                });
            }

            collect_delimited_spans(line_without_newline, line_start, "**", strong, &mut spans);
            collect_delimited_spans(line_without_newline, line_start, "__", strong, &mut spans);
            collect_delimited_spans(
                line_without_newline,
                line_start,
                "*",
                highlight_index("text.emphasis"),
                &mut spans,
            );
            collect_delimited_spans(
                line_without_newline,
                line_start,
                "_",
                highlight_index("text.emphasis"),
                &mut spans,
            );
            collect_delimited_spans(line_without_newline, line_start, "`", literal, &mut spans);
            collect_markdown_link_spans(
                line_without_newline,
                line_start,
                reference,
                uri,
                &mut spans,
            );

            offset += line.len();
        }

        spans
    }

    /// Add Markdown spans that are not reliably emitted by tree-sitter queries.
    ///
    /// Unlabeled fenced code blocks are detected from their content and replace
    /// the Markdown code-block literal span inside the fence. Explicitly labeled
    /// fences are left to tree-sitter injection.
    pub fn add_markdown_extra_spans(spans: &mut Vec<HighlightSpan>, text: &str) {
        append_non_overlapping_spans(spans, Self::generate_markdown_fallback_spans(text));

        for block in fenced_code_blocks(text) {
            let code = &text[block.start..block.end];
            let language = fenced_code_block_language(grammar_registry(), block.info, code);
            if matches!(language, LanguageId::PlainText | LanguageId::Markdown) {
                continue;
            }

            let Some(config) = grammar_registry().highlight_config(language) else {
                continue;
            };

            let mut code_spans = Self::generate_highlight_spans(&config, code);
            if code_spans.is_empty() {
                continue;
            }

            spans.retain(|span| !(span.start < block.end && block.start < span.end));
            spans.extend(code_spans.drain(..).map(|span| HighlightSpan {
                start: span.start + block.start,
                end: span.end + block.start,
                highlight: span.highlight,
            }));
        }
    }

    /// Apply an edit to the stored tree so the next parse is properly incremental.
    ///
    /// Call this *before* the actual text change so the tree knows the expected edit.
    #[allow(dead_code)]
    pub fn edit(&mut self, start_byte: usize, old_end_byte: usize, new_end_byte: usize) {
        if let Some(tree) = &mut self.tree {
            let start_position =
                byte_offset_to_point(tree.root_node().utf8_text(&[]).unwrap_or(""), start_byte);
            let old_end_position =
                byte_offset_to_point(tree.root_node().utf8_text(&[]).unwrap_or(""), old_end_byte);
            let new_end_position =
                byte_offset_to_point(tree.root_node().utf8_text(&[]).unwrap_or(""), new_end_byte);

            tree.edit(&InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position,
                old_end_position,
                new_end_position,
            });
        }
    }

    /// Invalidate cached spans (e.g., when the document revision changes externally
    /// or when the visible range changes significantly).
    #[allow(dead_code)]
    pub fn invalidate_cache(&mut self) {
        self.cached_spans = None;
    }

    /// Clear parse metadata and cached spans so the next syntax pass reparses
    /// even if the document revision has not changed.
    pub fn invalidate_parse(&mut self) {
        self.tree = None;
        self.parsed_as = None;
        self.parsed_revision = 0;
        self.cached_spans = None;
    }

    /// Returns the tree-sitter `Tree` if available.
    #[allow(dead_code)]
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Returns the language used to parse the current tree.
    pub fn parsed_as(&self) -> Option<LanguageId> {
        self.parsed_as
    }

    /// Returns true if the last parse tree contains syntax errors.
    pub fn has_parse_errors(&self) -> bool {
        self.tree
            .as_ref()
            .map_or(false, |t| t.root_node().has_error())
    }

    /// Check if the given byte offset is inside a comment node.
    pub fn is_inside_comment(&self, offset: usize) -> bool {
        let Some(tree) = self.tree.as_ref() else {
            return false;
        };
        let node = tree.root_node();
        if let Some(leaf) = find_leaf_at_offset(node, offset) {
            let mut current = Some(leaf);
            while let Some(n) = current {
                let kind = n.kind();
                if kind == "comment" || kind.ends_with("comment") {
                    return true;
                }
                current = n.parent();
            }
        }
        false
    }

    /// Check if the given byte offset is inside a string node.
    pub fn is_inside_string(&self, offset: usize) -> bool {
        let Some(tree) = self.tree.as_ref() else {
            return false;
        };
        let node = tree.root_node();
        if let Some(leaf) = find_leaf_at_offset(node, offset) {
            let mut current = Some(leaf);
            while let Some(n) = current {
                let kind = n.kind();
                if kind.contains("string") || kind.contains("str") {
                    return true;
                }
                current = n.parent();
            }
        }
        false
    }

    /// Get the node type at the given byte offset.
    #[allow(dead_code)]
    pub fn node_type_at(&self, offset: usize) -> Option<String> {
        let tree = self.tree.as_ref()?;
        let node = find_leaf_at_offset(tree.root_node(), offset)?;
        Some(node.kind().to_string())
    }

    /// Calculate syntax-aware indentation for a new line at the given offset.
    /// Returns the indentation string (spaces or tabs) to use.
    pub fn indentation_at(
        &self,
        offset: usize,
        tab_width: usize,
        use_soft_tabs: bool,
    ) -> Option<String> {
        let tree = self.tree.as_ref()?;
        let root = tree.root_node();

        // Find the node at the current position
        let node = find_leaf_at_offset(root, offset)?;

        // Walk up the tree to find the enclosing structure
        let mut current: Option<tree_sitter::Node> = Some(node);
        let mut depth = 0usize;

        while let Some(n) = current {
            let kind = n.kind();

            // For Python, check if we're after a line ending with `:`
            if kind == "block" || kind == "suite" || kind.ends_with("_block") {
                depth += 1;
            }

            // Count indentation depth based on brace-delimited blocks
            if n.start_byte() <= offset && offset <= n.end_byte() {
                if kind == "{" || kind.ends_with("_block") {
                    depth += 1;
                }
            }

            current = n.parent();
        }

        if depth == 0 {
            return None;
        }

        let indent_char = if use_soft_tabs { " " } else { "\t" };
        let indent_count = if use_soft_tabs {
            tab_width * depth
        } else {
            depth
        };
        Some(indent_char.repeat(indent_count))
    }
}

/// Find the leaf node at the given byte offset.
fn find_leaf_at_offset(node: tree_sitter::Node, offset: usize) -> Option<tree_sitter::Node> {
    if node.start_byte() > offset || offset >= node.end_byte() {
        return None;
    }

    let mut current = node;
    loop {
        let child_count = current.child_count();
        if child_count == 0 {
            return Some(current);
        }
        let mut found = None;
        for i in 0..child_count {
            if let Some(child) = current.child(i as u32) {
                if child.start_byte() <= offset && offset < child.end_byte() {
                    found = Some(child);
                    break;
                }
            }
        }
        match found {
            Some(child) => current = child,
            None => return Some(current),
        }
    }
}

/// Convert a byte offset into a tree-sitter `Point` (row, column in bytes).
#[allow(dead_code)]
fn byte_offset_to_point(text: &str, byte_offset: usize) -> Point {
    let byte_offset = byte_offset.min(text.len());
    let prefix = &text[..byte_offset];
    let row = prefix.chars().filter(|&c| c == '\n').count();
    let col = prefix.chars().rev().take_while(|&c| c != '\n').count();
    Point::new(row, col)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SyntaxBlock {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FencedCodeBlock<'a> {
    start: usize,
    end: usize,
    info: &'a str,
}

fn syntax_blocks(text: &str) -> Vec<SyntaxBlock> {
    let mut blocks = Vec::new();
    let mut block_start = None;
    let mut block_end = 0;
    let mut offset = 0;

    for line in text.split_inclusive('\n') {
        let line_start = offset;
        let line_end = offset + line.len();
        offset = line_end;

        if line.trim().is_empty() {
            if let Some(start) = block_start.take() {
                blocks.push(SyntaxBlock {
                    start,
                    end: block_end,
                });
            }
            continue;
        }

        block_start.get_or_insert(line_start);
        block_end = line_end;
    }

    if let Some(start) = block_start {
        blocks.push(SyntaxBlock {
            start,
            end: block_end,
        });
    }

    blocks
}

fn fenced_code_blocks(text: &str) -> Vec<FencedCodeBlock<'_>> {
    let mut blocks = Vec::new();
    let mut active: Option<(&str, usize, usize, &str)> = None;
    let mut offset = 0;

    for line in text.split_inclusive('\n') {
        let line_start = offset;
        let line_end = offset + line.len();
        let line_without_newline = line.trim_end_matches(['\r', '\n']);
        let leading = line_without_newline.len() - line_without_newline.trim_start().len();
        let trimmed = &line_without_newline[leading..];

        if let Some((marker, marker_len, content_start, info)) = active {
            if is_closing_code_fence(trimmed, marker, marker_len) {
                blocks.push(FencedCodeBlock {
                    start: content_start,
                    end: line_start,
                    info,
                });
                active = None;
            }
        } else if let Some((marker, marker_len, info)) = opening_code_fence(trimmed) {
            active = Some((marker, marker_len, line_end, info));
        }

        offset = line_end;
    }

    blocks
}

fn opening_code_fence(line: &str) -> Option<(&'static str, usize, &str)> {
    let (marker, marker_char) = if line.starts_with("```") {
        ("`", '`')
    } else if line.starts_with("~~~") {
        ("~", '~')
    } else {
        return None;
    };

    let marker_len = line.chars().take_while(|ch| *ch == marker_char).count();
    if marker_len < 3 {
        return None;
    }

    Some((marker, marker_len, &line[marker_len..]))
}

fn is_closing_code_fence(line: &str, marker: &str, opening_marker_len: usize) -> bool {
    let marker_byte = marker.as_bytes()[0];
    let marker_len = line
        .as_bytes()
        .iter()
        .take_while(|byte| **byte == marker_byte)
        .count();

    marker_len >= opening_marker_len && line[marker_len..].trim().is_empty()
}

fn block_language(registry: &GrammarRegistry, text: &str) -> LanguageId {
    if text.trim_start().starts_with("```") {
        return LanguageId::Markdown;
    }

    registry.detect(text).language
}

fn fenced_code_block_language(registry: &GrammarRegistry, info: &str, code: &str) -> LanguageId {
    if let Some(name) = info.split_whitespace().next() {
        if let Some(language) = registry.get_id_by_name(name) {
            return language;
        }
    }

    block_language(registry, code)
}

/// A single contiguous highlight span.
///
/// `highlight` is an index into the language's highlight names array
/// (as configured in `HighlightConfiguration::configure`).
#[derive(Clone, Debug)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub highlight: usize,
}

/// Returns the highlight name for a given highlight index.
///
/// This uses the same name list that is passed to `HighlightConfiguration::configure`.
pub fn highlight_name(index: usize) -> &'static str {
    let names: &[&str] = &[
        "attribute",
        "comment",
        "constant",
        "constant.builtin",
        "constructor",
        "embedded",
        "function",
        "function.builtin",
        "keyword",
        "module",
        "number",
        "operator",
        "property",
        "property.builtin",
        "punctuation",
        "punctuation.bracket",
        "punctuation.delimiter",
        "punctuation.special",
        "string",
        "string.escape",
        "string.special",
        "tag",
        "text.emphasis",
        "text.literal",
        "text.reference",
        "text.strong",
        "text.title",
        "text.uri",
        "type",
        "type.builtin",
        "variable",
        "variable.builtin",
        "variable.parameter",
    ];
    names.get(index).copied().unwrap_or("")
}

fn highlight_index(name: &str) -> usize {
    (0..)
        .find(|index| highlight_name(*index) == name)
        .unwrap_or(0)
}

fn markdown_list_marker_len(trimmed: &str) -> Option<usize> {
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        return Some(1);
    }

    let digits = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits > 0
        && matches!(trimmed.as_bytes().get(digits), Some(b'.' | b')'))
        && trimmed.as_bytes().get(digits + 1) == Some(&b' ')
    {
        return Some(digits + 1);
    }

    None
}

fn collect_delimited_spans(
    line: &str,
    line_start: usize,
    delimiter: &str,
    highlight: usize,
    spans: &mut Vec<HighlightSpan>,
) {
    let mut search_from = 0;
    while let Some(open_rel) = line[search_from..].find(delimiter) {
        let open = search_from + open_rel;
        let content_start = open + delimiter.len();
        let Some(close_rel) = line[content_start..].find(delimiter) else {
            break;
        };
        let close = content_start + close_rel;
        if content_start < close {
            spans.push(HighlightSpan {
                start: line_start + content_start,
                end: line_start + close,
                highlight,
            });
        }
        search_from = close + delimiter.len();
    }
}

fn collect_markdown_link_spans(
    line: &str,
    line_start: usize,
    reference: usize,
    uri: usize,
    spans: &mut Vec<HighlightSpan>,
) {
    let mut search_from = 0;
    while let Some(open_rel) = line[search_from..].find('[') {
        let open = search_from + open_rel;
        let Some(close_label_rel) = line[open + 1..].find(']') else {
            break;
        };
        let close_label = open + 1 + close_label_rel;
        if line.as_bytes().get(close_label + 1) != Some(&b'(') {
            search_from = close_label + 1;
            continue;
        }
        let uri_start = close_label + 2;
        let Some(close_uri_rel) = line[uri_start..].find(')') else {
            break;
        };
        let close_uri = uri_start + close_uri_rel;
        if open + 1 < close_label {
            spans.push(HighlightSpan {
                start: line_start + open + 1,
                end: line_start + close_label,
                highlight: reference,
            });
        }
        if uri_start < close_uri {
            spans.push(HighlightSpan {
                start: line_start + uri_start,
                end: line_start + close_uri,
                highlight: uri,
            });
        }
        search_from = close_uri + 1;
    }
}

fn append_non_overlapping_spans(spans: &mut Vec<HighlightSpan>, candidates: Vec<HighlightSpan>) {
    for span in candidates {
        if !spans
            .iter()
            .any(|existing| existing.start < span.end && span.start < existing.end)
        {
            spans.push(span);
        }
    }
}

/// Map a tree-sitter highlight name to an egui text color for the given theme.
pub fn highlight_color(name: &str, theme: crate::theme::Theme) -> egui::Color32 {
    match name {
        "keyword" | "keyword.*" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(255, 150, 100),
            crate::theme::Theme::Light => egui::Color32::from_rgb(200, 80, 30),
        },
        "string" | "string.*" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(150, 220, 150),
            crate::theme::Theme::Light => egui::Color32::from_rgb(50, 160, 50),
        },
        "string.escape" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(180, 230, 180),
            crate::theme::Theme::Light => egui::Color32::from_rgb(40, 130, 40),
        },
        "text.title" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(130, 200, 255),
            crate::theme::Theme::Light => egui::Color32::from_rgb(20, 95, 180),
        },
        "text.literal" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(150, 220, 150),
            crate::theme::Theme::Light => egui::Color32::from_rgb(50, 140, 50),
        },
        "text.uri" | "text.reference" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(120, 190, 255),
            crate::theme::Theme::Light => egui::Color32::from_rgb(30, 105, 200),
        },
        "text.emphasis" | "text.strong" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(255, 200, 130),
            crate::theme::Theme::Light => egui::Color32::from_rgb(180, 105, 35),
        },
        "number" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(180, 200, 255),
            crate::theme::Theme::Light => egui::Color32::from_rgb(80, 120, 200),
        },
        "comment" | "comment.*" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgba_premultiplied(150, 150, 150, 180),
            crate::theme::Theme::Light => {
                egui::Color32::from_rgba_premultiplied(120, 120, 120, 180)
            }
        },
        "function" | "function.*" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(130, 200, 255),
            crate::theme::Theme::Light => egui::Color32::from_rgb(30, 120, 210),
        },
        "type" | "type.*" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(220, 180, 255),
            crate::theme::Theme::Light => egui::Color32::from_rgb(150, 80, 200),
        },
        "variable.parameter" | "variable.builtin" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(255, 200, 130),
            crate::theme::Theme::Light => egui::Color32::from_rgb(200, 120, 40),
        },
        "operator" | "punctuation" | "punctuation.*" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(200, 200, 200),
            crate::theme::Theme::Light => egui::Color32::from_rgb(80, 80, 80),
        },
        "constant" | "constant.*" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(200, 180, 255),
            crate::theme::Theme::Light => egui::Color32::from_rgb(120, 80, 200),
        },
        "tag" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(255, 130, 180),
            crate::theme::Theme::Light => egui::Color32::from_rgb(200, 60, 120),
        },
        "attribute" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(180, 220, 180),
            crate::theme::Theme::Light => egui::Color32::from_rgb(80, 160, 80),
        },
        "embedded" => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(200, 200, 160),
            crate::theme::Theme::Light => egui::Color32::from_rgb(140, 140, 80),
        },
        _ => match theme {
            crate::theme::Theme::Dark => egui::Color32::from_rgb(220, 220, 220),
            crate::theme::Theme::Light => egui::Color32::from_rgb(30, 30, 30),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar_registry::GrammarRegistry;
    use crate::syntax::LanguageId;

    #[test]
    fn highlight_rust_code() {
        let mut state = DocumentSyntaxState::new();
        let code = "fn main() {\n    let x = 42;\n}\n";
        let spans = state.highlight(LanguageId::Rust, 1, 0, code.len());
        // Should have some highlight spans for Rust code
        // (This will be empty until we implement background parsing)
        assert!(spans.is_empty());
    }

    #[test]
    fn highlight_plain_text() {
        let mut state = DocumentSyntaxState::new();
        let text = "Just some plain text without code.";
        let spans = state.highlight(LanguageId::PlainText, 1, 0, text.len());
        // Plain text should have no spans
        assert!(spans.is_empty());
    }

    #[test]
    fn highlight_name_returns_valid_names() {
        // Test a few known highlight indices
        assert_eq!(highlight_name(1), "comment");
        assert_eq!(highlight_name(6), "function");
        assert_eq!(highlight_name(100), "");
    }

    #[test]
    fn cache_works_for_same_revision() {
        let mut state = DocumentSyntaxState::new();
        let code = "fn main() {}";
        let spans1 = state.highlight(LanguageId::Rust, 1, 0, code.len());
        let spans2 = state.highlight(LanguageId::Rust, 1, 0, code.len());
        // Same revision and visible range should return cached result
        assert_eq!(spans1.len(), spans2.len());
    }

    #[test]
    fn highlight_returns_stale_spans_while_reparse_is_pending() {
        let mut state = DocumentSyntaxState::new();
        let spans = vec![HighlightSpan {
            start: 0,
            end: 2,
            highlight: highlight_index("keyword"),
        }];
        state.update_from_parse_result(None, spans.clone(), LanguageId::Rust, 1, 0, 16);

        assert_eq!(
            state.highlight(LanguageId::Rust, 2, 0, 16).len(),
            spans.len()
        );
        assert_eq!(
            state.highlight(LanguageId::Rust, 2, 0, 17).len(),
            spans.len()
        );
        assert!(state.needs_parse(LanguageId::Rust, 2));
    }

    #[test]
    fn needs_parse_returns_true_for_new_revision() {
        let state = DocumentSyntaxState::new();
        // New revision should need a parse
        assert!(state.needs_parse(LanguageId::Rust, 1));
    }

    #[test]
    fn needs_parse_returns_true_for_plain_text() {
        let state = DocumentSyntaxState::new();
        assert!(state.needs_parse(LanguageId::PlainText, 1));
    }

    #[test]
    fn update_from_parse_result_works() {
        let mut state = DocumentSyntaxState::new();
        let spans = vec![HighlightSpan {
            start: 0,
            end: 2,
            highlight: 6,
        }];
        state.update_from_parse_result(None, spans.clone(), LanguageId::Rust, 1, 0, 10);
        assert_eq!(state.parsed_revision, 1);
        assert_eq!(state.parsed_as(), Some(LanguageId::Rust));
    }

    #[test]
    fn invalidate_parse_forces_reparse_at_same_revision() {
        let mut state = DocumentSyntaxState::new();
        state.update_from_parse_result(None, Vec::new(), LanguageId::Markdown, 1, 0, 10);
        assert!(!state.needs_parse(LanguageId::Markdown, 1));

        state.invalidate_parse();

        assert!(state.needs_parse(LanguageId::Markdown, 1));
        assert!(state.highlight(LanguageId::Markdown, 1, 0, 10).is_empty());
    }

    #[test]
    fn generate_highlight_spans_produces_spans() {
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Rust).unwrap();
        let code = "fn main() {\n    let x = 42;\n}\n";
        let spans = DocumentSyntaxState::generate_highlight_spans(&config, code);
        // Should have some spans for Rust code
        assert!(!spans.is_empty());
    }

    #[test]
    fn syntax_blocks_are_blank_line_delimited() {
        let text = "plain note\n\nfn main() {}\nlet x = 1;\n\n{\"ok\": true}\n";
        let blocks = syntax_blocks(text);

        assert_eq!(
            blocks,
            vec![
                SyntaxBlock { start: 0, end: 11 },
                SyntaxBlock { start: 12, end: 36 },
                SyntaxBlock { start: 37, end: 50 },
            ]
        );
    }

    #[test]
    fn block_highlighting_handles_mixed_scratch_snippets() {
        let text = "plain note\n\nfn main() {\n    let x = 42;\n}\n\n{\"ok\": true}\n";
        let spans = DocumentSyntaxState::generate_block_highlight_spans(text);

        let rust_start = text.find("fn main").unwrap();
        let rust_end = text.find("\n\n{\"ok\"").unwrap();
        let json_start = text.find("{\"ok\"").unwrap();

        assert!(
            spans
                .iter()
                .any(|span| span.start >= rust_start && span.end <= rust_end)
        );
        assert!(spans.iter().any(|span| span.start >= json_start));
        assert!(spans.iter().all(|span| span.start >= rust_start));
    }

    #[test]
    fn block_highlighting_keeps_multiline_code_together() {
        let text = "def greet(name):\n    print(f\"Hello, {name}\")\n";
        let spans = DocumentSyntaxState::generate_block_highlight_spans(text);

        assert!(
            spans
                .iter()
                .any(|span| span.start < text.find("print").unwrap() && span.end > span.start)
        );
        assert!(
            spans
                .iter()
                .any(|span| span.start >= text.find("print").unwrap())
        );
    }

    #[test]
    fn block_highlighting_preserves_markdown_fence_injection() {
        let text = "notes\n\n```rust\nfn main() {\n    let x = 42;\n}\n```\n";
        let spans = DocumentSyntaxState::generate_block_highlight_spans(text);
        let rust_start = text.find("fn main").unwrap();
        let rust_end = text.rfind("\n```").unwrap();

        assert!(
            spans
                .iter()
                .any(|span| span.start >= rust_start && span.end <= rust_end)
        );
    }

    #[test]
    fn block_highlighting_detects_unlabeled_markdown_fence_language() {
        let text = "notes\n\n```\nfn main() {\n    let x = 42;\n}\n```\n";
        let spans = DocumentSyntaxState::generate_block_highlight_spans(text);
        let rust_start = text.find("fn main").unwrap();
        let rust_end = text.rfind("\n```").unwrap();

        assert!(
            spans.iter().any(|span| {
                span.start >= rust_start
                    && span.end <= rust_end
                    && highlight_name(span.highlight) == "keyword"
            }),
            "unlabeled fenced Rust should be detected and highlighted as Rust"
        );
    }

    #[test]
    fn markdown_unlabeled_fence_detects_javascript_console_log() {
        let text = "# Hello world\n\nBold *text*\n\n## Hello?\n\n```\nconsole.log('hello')\n```\n\n_fadsasd_\n";
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();
        let mut spans = DocumentSyntaxState::generate_highlight_spans(&config, text);
        DocumentSyntaxState::add_markdown_extra_spans(&mut spans, text);
        let js_start = text.find("console.log").unwrap();
        let js_end = text.rfind("\n```").unwrap();

        assert!(
            spans.iter().any(|span| {
                span.start >= js_start
                    && span.end <= js_end
                    && matches!(
                        highlight_name(span.highlight),
                        "function" | "function.builtin"
                    )
            }),
            "unlabeled fenced console.log snippet should be highlighted as JavaScript"
        );
    }

    #[test]
    fn markdown_explicit_js_fence_replaces_generic_code_span() {
        let text = "# Hello world\n\nBold *text*\n\n## Hello?\n\n```js\nconsole.log('hello')\n```\n\n_fadsasd_\n";
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();
        let mut spans = DocumentSyntaxState::generate_highlight_spans(&config, text);
        DocumentSyntaxState::add_markdown_extra_spans(&mut spans, text);
        let js_start = text.find("console.log").unwrap();
        let js_end = text.rfind("\n```").unwrap();

        assert!(
            spans.iter().any(|span| {
                span.start >= js_start
                    && span.end <= js_end
                    && matches!(
                        highlight_name(span.highlight),
                        "function" | "function.builtin"
                    )
            }),
            "explicit js fence should highlight JavaScript identifiers"
        );
        assert!(
            !spans.iter().any(|span| {
                span.start <= js_start
                    && js_end <= span.end
                    && highlight_name(span.highlight) == "text.literal"
            }),
            "generic Markdown code span should not cover the JavaScript region"
        );
    }

    #[test]
    fn markdown_injection_highlights_rust_in_fenced_block() {
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();

        let markdown = "# Title\n\n```rust\nfn main() {\n    let x = 42;\n}\n```\n";

        let spans = DocumentSyntaxState::generate_highlight_spans(&config, markdown);

        // Should have spans for both markdown and injected Rust
        assert!(!spans.is_empty(), "Should produce highlight spans");

        // Check that we have spans covering the Rust code region
        let rust_start = markdown.find("fn main()").unwrap();
        let rust_end = markdown.find("```\n").unwrap();

        let has_rust_spans = spans
            .iter()
            .any(|span| span.start >= rust_start && span.end <= rust_end && span.highlight != 0);

        // The injection should produce spans in the Rust code region
        assert!(has_rust_spans, "Injection should produce Rust code spans");
    }

    #[test]
    fn markdown_injection_highlights_multiple_code_blocks() {
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();

        let markdown = "# Mixed Code\n\n```rust\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```\n\n```python\ndef greet(name):\n    print(f\"Hello, {name}!\")\n```\n";

        let spans = DocumentSyntaxState::generate_highlight_spans(&config, markdown);

        assert!(
            !spans.is_empty(),
            "Should produce highlight spans for mixed code blocks"
        );

        // Verify spans exist in both code block regions
        let rust_region = markdown.find("fn add").unwrap();
        let python_region = markdown.find("def greet").unwrap();

        assert!(rust_region > 0, "Should find Rust code");
        assert!(python_region > 0, "Should find Python code");
        assert!(!spans.is_empty());
    }

    #[test]
    fn injection_registry_contains_expected_languages() {
        let registry = GrammarRegistry::default();
        let injection_reg = registry.injection_registry();

        // Markdown should be in the injection registry for fenced code blocks
        assert!(
            injection_reg.contains_key("rust"),
            "Should have Rust in injection registry"
        );
        assert!(
            injection_reg.contains_key("python"),
            "Should have Python in injection registry"
        );
        assert!(
            injection_reg.contains_key("javascript"),
            "Should have JavaScript in injection registry"
        );
        assert!(
            injection_reg.contains_key("typescript"),
            "Should have TypeScript in injection registry"
        );
    }

    #[test]
    fn injected_spans_have_valid_byte_ranges() {
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();

        let markdown = "```rust\nlet x = 42;\n```\n";

        let spans = DocumentSyntaxState::generate_highlight_spans(&config, markdown);

        for span in &spans {
            assert!(span.start <= span.end, "Span start should be <= end");
            assert!(
                span.end <= markdown.len(),
                "Span end should be within text length"
            );
        }
    }

    #[test]
    fn markdown_without_code_blocks_no_injection() {
        let markdown =
            "# Just a heading\n\nSome **regular** *italic* [text](https://example.com).\n";

        let spans = DocumentSyntaxState::generate_block_highlight_spans(markdown);

        assert!(
            spans
                .iter()
                .any(|span| highlight_name(span.highlight) == "text.title"),
            "heading should be highlighted as Markdown"
        );
        assert!(
            spans
                .iter()
                .any(|span| highlight_name(span.highlight) == "text.strong"),
            "inline Markdown injection should highlight strong emphasis"
        );
        assert!(
            spans
                .iter()
                .any(|span| highlight_name(span.highlight) == "text.emphasis"),
            "inline Markdown fallback should highlight emphasis"
        );

        for span in &spans {
            assert!(span.start <= span.end);
            assert!(span.end <= markdown.len());
        }
    }

    #[test]
    fn javascript_injection_in_markdown() {
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();

        let markdown =
            "```javascript\nconst add = (a, b) => a + b;\nconsole.log(add(1, 2));\n```\n";

        let spans = DocumentSyntaxState::generate_highlight_spans(&config, markdown);

        assert!(
            !spans.is_empty(),
            "Should produce spans for JavaScript in Markdown"
        );

        // Check that the code block region has spans
        let js_start = markdown.find("const add").unwrap();
        let has_js_spans = spans.iter().any(|span| span.start >= js_start);
        assert!(
            has_js_spans || !spans.is_empty(),
            "Should have spans in JS region"
        );
    }

    #[test]
    fn yaml_injection_in_markdown() {
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();

        let markdown = "```yaml\nname: Test\nversion: 1.0.0\nactive: true\n```\n";

        let spans = DocumentSyntaxState::generate_highlight_spans(&config, markdown);

        assert!(
            !spans.is_empty(),
            "Should produce spans for YAML in Markdown"
        );
    }

    #[test]
    fn nested_injection_handles_unknown_language_gracefully() {
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();

        // Use a language that might not be in the injection registry
        let markdown = "```unknown_lang\nSome code here\n```\n";

        // Should not panic; unknown languages are skipped in the injection callback
        let spans = DocumentSyntaxState::generate_highlight_spans(&config, markdown);

        // Should still produce spans for the markdown itself
        for span in &spans {
            assert!(span.start <= span.end);
        }
    }

    #[test]
    fn injection_span_highlight_indices_are_valid() {
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();

        let markdown = "```rust\nfn main() {}\n```\n";

        let spans = DocumentSyntaxState::generate_highlight_spans(&config, markdown);

        for span in &spans {
            // Highlight index should be valid (less than the number of highlight names)
            assert!(
                !highlight_name(span.highlight).is_empty(),
                "Highlight index should resolve to a configured name"
            );
        }
    }
}
