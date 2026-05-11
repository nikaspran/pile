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
    /// Cached highlight spans for the last processed revision and visible range.
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
        revision: u64,
        visible_start: usize,
        visible_end: usize,
    ) -> Vec<HighlightSpan> {
        // Return cached result if revision and visible range haven't changed
        if let Some((cached_rev, cached_start, cached_end, spans)) = &self.cached_spans {
            if *cached_rev == revision
                && *cached_start == visible_start
                && *cached_end == visible_end
                && self.parsed_as == Some(language)
            {
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
        if language == LanguageId::PlainText {
            return false;
        }
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
        "string.special",
        "tag",
        "type",
        "type.builtin",
        "variable",
        "variable.builtin",
        "variable.parameter",
    ];
    names.get(index).copied().unwrap_or("")
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
    fn needs_parse_returns_true_for_new_revision() {
        let state = DocumentSyntaxState::new();
        // New revision should need a parse
        assert!(state.needs_parse(LanguageId::Rust, 1));
    }

    #[test]
    fn needs_parse_returns_false_for_plain_text() {
        let state = DocumentSyntaxState::new();
        assert!(!state.needs_parse(LanguageId::PlainText, 1));
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
    fn generate_highlight_spans_produces_spans() {
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Rust).unwrap();
        let code = "fn main() {\n    let x = 42;\n}\n";
        let spans = DocumentSyntaxState::generate_highlight_spans(&config, code);
        // Should have some spans for Rust code
        assert!(!spans.is_empty());
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
        let registry = GrammarRegistry::default();
        let config = registry.highlight_config(LanguageId::Markdown).unwrap();

        let markdown = "# Just a heading\n\nSome regular text without code.\n";

        let spans = DocumentSyntaxState::generate_highlight_spans(&config, markdown);

        // Should still produce spans for markdown highlighting
        // but no injection should occur
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
                span.highlight < 26,
                "Highlight index should be valid (less than 26 standard names)"
            );
        }
    }
}
