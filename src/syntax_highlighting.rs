//! Syntax highlighting using tree-sitter.
//!
//! This module provides incremental parse state per document and converts
//! tree-sitter parse trees into highlight spans for rendering.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use tree_sitter::{InputEdit, Language, Parser, Point, Tree};
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

use crate::syntax::LanguageId;

/// Highlight configuration stored per language to avoid rebuilding queries.
struct PerLanguageConfig {
    config: HighlightConfiguration,
}

/// Converts a `LanguageId` to a tree-sitter `Language` and highlight config.
fn language_config(lang: LanguageId) -> Option<PerLanguageConfig> {
    let (language, name, highlights, injections, locals): (
        Language,
        &str,
        &str,
        &str,
        &str,
    ) = match lang {
        LanguageId::PlainText => return None,
        LanguageId::Rust => (
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            tree_sitter_rust::TAGS_QUERY,
        ),
        LanguageId::JavaScript => (
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ),
        LanguageId::TypeScript => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        ),
        LanguageId::Python => (
            tree_sitter_python::LANGUAGE.into(),
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_python::TAGS_QUERY,
        ),
        LanguageId::Json => (
            tree_sitter_json::LANGUAGE.into(),
            "json",
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        LanguageId::Toml => (
            tree_sitter_toml_ng::LANGUAGE.into(),
            "toml",
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        LanguageId::Yaml => (
            tree_sitter_yaml::LANGUAGE.into(),
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        LanguageId::Bash => (
            tree_sitter_bash::LANGUAGE.into(),
            "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        ),
        LanguageId::Markdown => (
            tree_sitter_md::LANGUAGE.into(),
            "markdown",
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            tree_sitter_md::INJECTION_QUERY_BLOCK,
            "",
        ),
    };

    let mut config = HighlightConfiguration::new(language, name, highlights, injections, locals).ok()?;

    // Standard highlight names recognized by tree-sitter-highlight
    let highlight_names: &[&str] = &[
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
    config.configure(highlight_names);

    Some(PerLanguageConfig { config })
}

/// Returns a static map of language name to `HighlightConfiguration` for injection resolution.
fn injection_language_registry() -> &'static HashMap<&'static str, Arc<HighlightConfiguration>> {
    static REGISTRY: OnceLock<HashMap<&'static str, Arc<HighlightConfiguration>>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut map: HashMap<&'static str, Arc<HighlightConfiguration>> = HashMap::new();

        let highlight_names: &[&str] = &[
            "attribute", "comment", "constant", "constant.builtin", "constructor",
            "embedded", "function", "function.builtin", "keyword", "module",
            "number", "operator", "property", "property.builtin", "punctuation",
            "punctuation.bracket", "punctuation.delimiter", "punctuation.special",
            "string", "string.special", "tag", "type", "type.builtin",
            "variable", "variable.builtin", "variable.parameter",
        ];

        // Rust
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            tree_sitter_rust::TAGS_QUERY,
        ) {
            config.configure(highlight_names);
            map.insert("rust", Arc::new(config));
        }

        // JavaScript
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ) {
            config.configure(highlight_names);
            let arc_config = Arc::new(config);
            map.insert("javascript", arc_config.clone());
            map.insert("js", arc_config);
        }

        // TypeScript
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        ) {
            config.configure(highlight_names);
            let arc_config = Arc::new(config);
            map.insert("typescript", arc_config.clone());
            map.insert("ts", arc_config);
        }

        // TSX
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        ) {
            config.configure(highlight_names);
            map.insert("tsx", Arc::new(config));
        }

        // Python
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_python::LANGUAGE.into(),
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_python::TAGS_QUERY,
        ) {
            config.configure(highlight_names);
            let arc_config = Arc::new(config);
            map.insert("python", arc_config.clone());
            map.insert("py", arc_config);
        }

        // JSON
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_json::LANGUAGE.into(),
            "json",
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            config.configure(highlight_names);
            map.insert("json", Arc::new(config));
        }

        // TOML
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_toml_ng::LANGUAGE.into(),
            "toml",
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            config.configure(highlight_names);
            map.insert("toml", Arc::new(config));
        }

        // YAML
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_yaml::LANGUAGE.into(),
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            config.configure(highlight_names);
            let arc_config = Arc::new(config);
            map.insert("yaml", arc_config.clone());
            map.insert("yml", arc_config);
        }

        // Bash
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_bash::LANGUAGE.into(),
            "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        ) {
            config.configure(highlight_names);
            let arc_config = Arc::new(config);
            map.insert("bash", arc_config.clone());
            map.insert("sh", arc_config);
        }

        // Markdown (for embedded code blocks)
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_md::LANGUAGE.into(),
            "markdown",
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            tree_sitter_md::INJECTION_QUERY_BLOCK,
            "",
        ) {
            config.configure(highlight_names);
            let arc_config = Arc::new(config);
            map.insert("markdown", arc_config.clone());
            map.insert("md", arc_config);
        }

        map
    })
}

/// Per-document syntax highlighting state.
///
/// Stores the parsed tree-sitter `Tree` for incremental reparsing and
/// caches highlight spans keyed by document revision.
#[derive(Clone, Debug)]
pub struct DocumentSyntaxState {
    /// The most recent parse tree. `None` for plain text.
    tree: Option<Tree>,
    /// The language used to produce `tree`.
    parsed_as: Option<LanguageId>,
    /// Cached highlight spans for the last processed revision.
    cached_spans: Option<(u64, Vec<HighlightSpan>)>,
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
            cached_spans: None,
        }
    }

    /// Returns highlight spans for the given document text and detected language.
    ///
    /// Uses incremental reparsing when the language hasn't changed and a prior
    /// tree exists. Results are cached by document `revision`.
    pub fn highlight(
        &mut self,
        text: &str,
        language: LanguageId,
        revision: u64,
    ) -> Vec<HighlightSpan> {
        // Return cached result if revision hasn't changed
        if let Some((cached_rev, spans)) = &self.cached_spans {
            if *cached_rev == revision && self.parsed_as == Some(language) {
                return spans.clone();
            }
        }

        // Plain text: no highlighting
        if language == LanguageId::PlainText {
            self.tree = None;
            self.parsed_as = Some(LanguageId::PlainText);
            self.cached_spans = Some((revision, Vec::new()));
            return Vec::new();
        }

        // Get or rebuild language config
        let Some(per_lang) = language_config(language) else {
            self.tree = None;
            self.parsed_as = Some(language);
            self.cached_spans = Some((revision, Vec::new()));
            return Vec::new();
        };

        // Perform incremental or full parse
        let mut parser = Parser::new();
        parser
            .set_language(&per_lang.config.language)
            .expect("tree-sitter language should be valid");

        let tree = if self.parsed_as == Some(language) && self.tree.is_some() {
            // Incremental reparse: tree-sitter reuses the old tree
            parser.parse(text, self.tree.as_ref()).unwrap_or_else(|| {
                // Fallback to full parse on error
                parser.parse(text, None).unwrap()
            })
        } else {
            // Language changed or no prior tree: full parse
            parser.parse(text, None).unwrap()
        };

        self.tree = Some(tree);
        self.parsed_as = Some(language);

        // Generate highlight spans using tree-sitter-highlight
        let spans = Self::generate_highlight_spans(&per_lang.config, text);

        self.cached_spans = Some((revision, spans.clone()));
        spans
    }

    /// Generate highlight spans from the parsed tree using `tree-sitter-highlight`.
    ///
    /// Supports injected languages by resolving language names through the
    /// injection registry, enabling range-based highlighting for embedded code
    /// (e.g., JavaScript inside Markdown fenced code blocks).
    fn generate_highlight_spans(config: &HighlightConfiguration, text: &str) -> Vec<HighlightSpan> {
        let mut highlighter = Highlighter::new();
        let registry = injection_language_registry();

        let Ok(events) = highlighter.highlight(config, text.as_bytes(), None, |lang_name| {
            registry.get(lang_name).map(|c| &**c as &HighlightConfiguration)
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
    pub fn edit(&mut self, start_byte: usize, old_end_byte: usize, new_end_byte: usize) {
        if let Some(tree) = &mut self.tree {
            let start_position = byte_offset_to_point(tree.root_node().utf8_text(&[]).unwrap_or(""), start_byte);
            let old_end_position = byte_offset_to_point(tree.root_node().utf8_text(&[]).unwrap_or(""), old_end_byte);
            let new_end_position = byte_offset_to_point(tree.root_node().utf8_text(&[]).unwrap_or(""), new_end_byte);

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

    /// Invalidate cached spans (e.g., when the document revision changes externally).
    pub fn invalidate_cache(&mut self) {
        self.cached_spans = None;
    }

    /// Returns the tree-sitter `Tree` if available.
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
    pub fn is_inside_comment(&self, text: &str, offset: usize) -> bool {
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
    pub fn is_inside_string(&self, text: &str, offset: usize) -> bool {
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
    pub fn node_type_at(&self, offset: usize) -> Option<String> {
        let tree = self.tree.as_ref()?;
        let node = tree.root_node();
        let leaf = find_leaf_at_offset(node, offset)?;
        Some(leaf.kind().to_string())
    }

    /// Calculate syntax-aware indentation for a new line at the given offset.
    /// Returns the indentation string (spaces or tabs) to use.
    pub fn indentation_at(&self, text: &str, offset: usize, tab_width: usize, use_soft_tabs: bool) -> Option<String> {
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
            if kind == "block" || kind == "suite" || kind.contains("_block") {
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
        let indent_count = if use_soft_tabs { tab_width * depth } else { depth };
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
        if child_count ==0 {
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
pub fn highlight_color(name: &str, theme: crate::settings::Theme) -> egui::Color32 {
    match name {
        "keyword" | "keyword.*" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(255, 150, 100),
            crate::settings::Theme::Light => egui::Color32::from_rgb(200, 80, 30),
        },
        "string" | "string.*" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(150, 220, 150),
            crate::settings::Theme::Light => egui::Color32::from_rgb(50, 160, 50),
        },
        "number" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(180, 200, 255),
            crate::settings::Theme::Light => egui::Color32::from_rgb(80, 120, 200),
        },
        "comment" | "comment.*" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgba_premultiplied(150, 150, 150, 180),
            crate::settings::Theme::Light => egui::Color32::from_rgba_premultiplied(120, 120, 120, 180),
        },
        "function" | "function.*" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(130, 200, 255),
            crate::settings::Theme::Light => egui::Color32::from_rgb(30, 120, 210),
        },
        "type" | "type.*" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(220, 180, 255),
            crate::settings::Theme::Light => egui::Color32::from_rgb(150, 80, 200),
        },
        "variable.parameter" | "variable.builtin" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(255, 200, 130),
            crate::settings::Theme::Light => egui::Color32::from_rgb(200, 120, 40),
        },
        "operator" | "punctuation" | "punctuation.*" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(200, 200, 200),
            crate::settings::Theme::Light => egui::Color32::from_rgb(80, 80, 80),
        },
        "constant" | "constant.*" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(200, 180, 255),
            crate::settings::Theme::Light => egui::Color32::from_rgb(120, 80, 200),
        },
        "tag" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(255, 130, 180),
            crate::settings::Theme::Light => egui::Color32::from_rgb(200, 60, 120),
        },
        "attribute" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(180, 220, 180),
            crate::settings::Theme::Light => egui::Color32::from_rgb(80, 160, 80),
        },
        "embedded" => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(200, 200, 160),
            crate::settings::Theme::Light => egui::Color32::from_rgb(140, 140, 80),
        },
        _ => match theme {
            crate::settings::Theme::Dark => egui::Color32::from_rgb(220, 220, 220),
            crate::settings::Theme::Light => egui::Color32::from_rgb(30, 30, 30),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::LanguageId;

    #[test]
    fn highlight_rust_code() {
        let mut state = DocumentSyntaxState::new();
        let code = "fn main() {\n    let x = 42;\n}\n";
        let spans = state.highlight(code, LanguageId::Rust, 1);
        // Should have some highlight spans for Rust code
        assert!(!spans.is_empty());
    }

    #[test]
    fn highlight_plain_text() {
        let mut state = DocumentSyntaxState::new();
        let text = "Just some plain text without code.";
        let spans = state.highlight(text, LanguageId::PlainText, 1);
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
        let spans1 = state.highlight(code, LanguageId::Rust, 1);
        let spans2 = state.highlight(code, LanguageId::Rust, 1);
        // Same revision should return cached result
        assert_eq!(spans1.len(), spans2.len());
    }
}
