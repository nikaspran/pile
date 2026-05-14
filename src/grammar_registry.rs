//! Grammar registry for managing language definitions.
//!
//! This module provides a centralized registry for language grammars, allowing
//! languages to be registered without modifying core editor code.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use tree_sitter::Language;
use tree_sitter_highlight::HighlightConfiguration;

use crate::syntax::{
    DetectionRule, LanguageDetection, LanguageId, ScoredDetector, has_javascript_function_signal,
};

/// Configuration for a language grammar.
pub struct GrammarConfig {
    /// The language ID.
    pub id: LanguageId,
    /// The tree-sitter language name (used in queries and injection registry).
    pub name: &'static str,
    /// Alternative names for this language (e.g., "js" for JavaScript).
    pub aliases: &'static [&'static str],
    /// The tree-sitter language, if available.
    pub ts_language: Option<Language>,
    /// The highlight query source.
    pub highlight_query: &'static str,
    /// The injection query source.
    pub injection_query: &'static str,
    /// The locals query source.
    pub locals_query: &'static str,
    /// Line comment prefix, if any.
    #[allow(dead_code)]
    pub comment_prefix: Option<&'static str>,
    /// Block comment delimiters (open, close), if any.
    #[allow(dead_code)]
    pub block_comment: Option<(&'static str, &'static str)>,
    /// Detection rules for content-based language detection.
    pub detection_rules: &'static [DetectionRule],
}

impl GrammarConfig {
    /// Returns true if this language uses tree-sitter for syntax awareness.
    #[allow(dead_code)]
    pub fn has_tree_sitter(&self) -> bool {
        self.ts_language.is_some()
    }

    /// Builds a `HighlightConfiguration` for this grammar.
    pub fn build_highlight_config(&self) -> Option<HighlightConfiguration> {
        let language = self.ts_language.clone()?;
        let mut config = HighlightConfiguration::new(
            language,
            self.name,
            self.highlight_query,
            self.injection_query,
            self.locals_query,
        )
        .ok()?;

        // Configure with standard highlight names
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
        config.configure(highlight_names);

        Some(config)
    }
}

/// Central registry for language grammars.
pub struct GrammarRegistry {
    grammars: HashMap<LanguageId, GrammarConfig>,
    name_to_id: HashMap<&'static str, LanguageId>,
    detector: ScoredDetector,
    injection_registry: OnceLock<HashMap<&'static str, Arc<HighlightConfiguration>>>,
}

impl GrammarRegistry {
    /// Returns the process-wide grammar registry.
    pub fn shared() -> &'static Self {
        static REGISTRY: OnceLock<GrammarRegistry> = OnceLock::new();
        REGISTRY.get_or_init(GrammarRegistry::default)
    }

    /// Creates a new empty grammar registry.
    pub fn new() -> Self {
        Self {
            grammars: HashMap::new(),
            name_to_id: HashMap::new(),
            detector: ScoredDetector::new_empty(),
            injection_registry: OnceLock::new(),
        }
    }

    /// Registers a grammar with the registry.
    pub fn register(&mut self, config: GrammarConfig) {
        let id = config.id;
        let name = config.name;

        // Register detection rules
        if !config.detection_rules.is_empty() {
            self.detector.add_rules(id, config.detection_rules);
        }

        // Register name mapping
        self.name_to_id.insert(name, id);
        for &alias in config.aliases {
            self.name_to_id.insert(alias, id);
        }

        // Store the grammar
        self.grammars.insert(id, config);
    }

    /// Returns the grammar config for the given language ID.
    #[allow(dead_code)]
    pub fn get(&self, id: LanguageId) -> Option<&GrammarConfig> {
        self.grammars.get(&id)
    }

    /// Returns the language ID for a given name or alias.
    #[allow(dead_code)]
    pub fn get_id_by_name(&self, name: &str) -> Option<LanguageId> {
        self.name_to_id.get(name).copied()
    }

    #[allow(dead_code)]
    /// Returns the tree-sitter language for the given language ID.
    pub fn get_language(&self, id: LanguageId) -> Option<&Language> {
        let grammar = self.grammars.get(&id)?;
        grammar.ts_language.as_ref()
    }

    /// Detects the language of the given text.
    pub fn detect(&self, text: &str) -> LanguageDetection {
        self.detector.detect(text)
    }

    /// Detects the language from a rope sample.
    pub fn detect_rope(&self, rope: &crop::Rope) -> LanguageDetection {
        let text = bounded_rope_sample(rope);
        self.detect(&text)
    }

    /// Returns the highlight configuration for the given language ID.
    pub fn highlight_config(&self, id: LanguageId) -> Option<HighlightConfiguration> {
        self.grammars.get(&id)?.build_highlight_config()
    }

    /// Returns the injection language registry (lazy-initialized).
    pub fn injection_registry(&self) -> &HashMap<&'static str, Arc<HighlightConfiguration>> {
        self.injection_registry.get_or_init(|| {
            let mut map: HashMap<&'static str, Arc<HighlightConfiguration>> = HashMap::new();

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

            for grammar in self.grammars.values() {
                if let Some(mut config) = grammar.build_highlight_config() {
                    config.configure(highlight_names);
                    let arc_config = Arc::new(config);
                    map.insert(grammar.name, arc_config.clone());
                    for &alias in grammar.aliases {
                        map.insert(alias, arc_config.clone());
                    }
                }
            }

            if let Ok(mut config) = HighlightConfiguration::new(
                tree_sitter_md::INLINE_LANGUAGE.into(),
                "markdown_inline",
                tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
                tree_sitter_md::INJECTION_QUERY_INLINE,
                "",
            ) {
                config.configure(highlight_names);
                map.insert("markdown_inline", Arc::new(config));
            }

            map
        })
    }

    /// Returns the comment prefix for the given language, if any.
    #[allow(dead_code)]
    pub fn comment_prefix(&self, id: LanguageId) -> Option<&'static str> {
        self.grammars.get(&id)?.comment_prefix
    }

    /// Returns the block comment delimiters for the given language, if any.
    #[allow(dead_code)]
    pub fn block_comment_delimiters(&self, id: LanguageId) -> Option<(&'static str, &'static str)> {
        self.grammars.get(&id)?.block_comment
    }

    /// Returns true if the language uses tree-sitter.
    #[allow(dead_code)]
    pub fn has_tree_sitter(&self, id: LanguageId) -> bool {
        self.grammars
            .get(&id)
            .map_or(false, |g| g.has_tree_sitter())
    }
}
impl Default for GrammarRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        register_builtin_grammars(&mut registry);
        registry
    }
}

/// Registers all built-in language grammars.
fn register_builtin_grammars(registry: &mut GrammarRegistry) {
    // Markdown
    registry.register(GrammarConfig {
        id: LanguageId::Markdown,
        name: "markdown",
        aliases: &["md"],
        ts_language: Some(tree_sitter_md::LANGUAGE.into()),
        highlight_query: tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
        injection_query: tree_sitter_md::INJECTION_QUERY_BLOCK,
        locals_query: "",
        comment_prefix: None,
        block_comment: None,
        detection_rules: &[
            DetectionRule {
                weight: 0.5,
                check: |text| {
                    let heading = text
                        .lines()
                        .filter(|l| {
                            let trimmed = l.trim_start();
                            trimmed.starts_with("# ")
                                || trimmed.starts_with("## ")
                                || trimmed.starts_with("### ")
                        })
                        .count();
                    (heading as f32 / 2.0).min(1.0)
                },
            },
            DetectionRule {
                weight: 0.35,
                check: |text| {
                    if text.lines().any(|l| l.trim_start().starts_with("```")) {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    let list = text
                        .lines()
                        .filter(|l| {
                            let trimmed = l.trim_start();
                            trimmed.starts_with("- ")
                                || trimmed.starts_with("* ")
                                || trimmed.starts_with("1. ")
                        })
                        .count();
                    (list as f32 / 3.0).min(1.0)
                },
            },
            DetectionRule {
                weight: 0.15,
                check: |text| {
                    if text.contains("**") || text.contains("__") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.15,
                check: |text| {
                    if text.contains("[") && text.contains("](") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
        ],
    });

    // JSON
    registry.register(GrammarConfig {
        id: LanguageId::Json,
        name: "json",
        aliases: &[],
        ts_language: Some(tree_sitter_json::LANGUAGE.into()),
        highlight_query: tree_sitter_json::HIGHLIGHTS_QUERY,
        injection_query: "",
        locals_query: "",
        comment_prefix: None,
        block_comment: None,
        detection_rules: &[
            DetectionRule {
                weight: 0.5,
                check: |text| {
                    let trimmed = text.trim_start();
                    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
                        return 0.0;
                    }
                    if trimmed.starts_with('[') {
                        let rest = &trimmed[1..];
                        if rest.contains(" = ") {
                            return 0.0;
                        }
                        if rest.contains(',') || rest.contains(']') {
                            return 1.0;
                        }
                        return 0.0;
                    }
                    if trimmed.starts_with('{') {
                        let rest = &trimmed[1..];
                        if rest.contains(" = ") {
                            return 0.0;
                        }
                        return 1.0;
                    }
                    0.0
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    let kv_pairs = text.matches('"').count() / 2;
                    if kv_pairs >= 1 && text.contains(": ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    if text.contains("true") || text.contains("false") || text.contains("null") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
        ],
    });

    // Rust
    registry.register(GrammarConfig {
        id: LanguageId::Rust,
        name: "rust",
        aliases: &[],
        ts_language: Some(tree_sitter_rust::LANGUAGE.into()),
        highlight_query: tree_sitter_rust::HIGHLIGHTS_QUERY,
        injection_query: tree_sitter_rust::INJECTIONS_QUERY,
        locals_query: tree_sitter_rust::TAGS_QUERY,
        comment_prefix: Some("//"),
        block_comment: Some(("/*", "*/")),
        detection_rules: &[
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    let count = text.matches("fn ").count() + text.matches("pub fn ").count();
                    (count as f32 / 3.0).min(1.0)
                },
            },
            DetectionRule {
                weight: 0.20,
                check: |text| {
                    if text.contains("use ") || text.contains("mod ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.20,
                check: |text| {
                    if text.contains("impl ") || text.contains("trait ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.15,
                check: |text| {
                    if text.contains("let ") && text.contains(" = ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.10,
                check: |text| {
                    if text.contains("-> ") || text.contains("::") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.10,
                check: |text| {
                    if text.contains("#[") && text.contains(']') {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
        ],
    });

    // Python
    registry.register(GrammarConfig {
        id: LanguageId::Python,
        name: "python",
        aliases: &["py"],
        ts_language: Some(tree_sitter_python::LANGUAGE.into()),
        highlight_query: tree_sitter_python::HIGHLIGHTS_QUERY,
        injection_query: "",
        locals_query: tree_sitter_python::TAGS_QUERY,
        comment_prefix: Some("#"),
        block_comment: Some(("\"\"\"", "\"\"\"")),
        detection_rules: &[
            DetectionRule {
                weight: 0.40,
                check: |text| {
                    if text.contains("def ") || text.contains("class ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    if text.contains("import ") || text.contains("from ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.15,
                check: |text| {
                    if text.contains("if __name__") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.10,
                check: |text| {
                    if text.contains(":")
                        && (text.contains("def ") || text.contains("if ") || text.contains("for "))
                    {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.10,
                check: |text| {
                    if text.contains("print(") || text.contains("return ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
        ],
    });

    // JavaScript
    registry.register(GrammarConfig {
        id: LanguageId::JavaScript,
        name: "javascript",
        aliases: &["js"],
        ts_language: Some(tree_sitter_javascript::LANGUAGE.into()),
        highlight_query: tree_sitter_javascript::HIGHLIGHT_QUERY,
        injection_query: tree_sitter_javascript::INJECTIONS_QUERY,
        locals_query: tree_sitter_javascript::LOCALS_QUERY,
        comment_prefix: Some("//"),
        block_comment: Some(("/*", "*/")),
        detection_rules: &[
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    if has_javascript_function_signal(text) || text.contains("=>") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.20,
                check: |text| {
                    if text.contains("const ") || text.contains("let ") || text.contains("var ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    if text.contains("console.log") || text.contains("export ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.15,
                check: |text| {
                    if text.contains("require(") || text.contains("import ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.20,
                check: |text| {
                    if text.contains("document.") || text.contains("window.") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
        ],
    });

    // TypeScript
    registry.register(GrammarConfig {
        id: LanguageId::TypeScript,
        name: "typescript",
        aliases: &["ts"],
        ts_language: Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        highlight_query: tree_sitter_typescript::HIGHLIGHTS_QUERY,
        injection_query: "",
        locals_query: tree_sitter_typescript::LOCALS_QUERY,
        comment_prefix: Some("//"),
        block_comment: Some(("/*", "*/")),
        detection_rules: &[
            DetectionRule {
                weight: 0.30,
                check: |text| {
                    if text.contains(": string")
                        || text.contains(": number")
                        || text.contains(": boolean")
                    {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    if text.contains("interface ") || text.contains("type ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.20,
                check: |text| {
                    if text.contains("<T>") || text.contains("<T,") || text.contains("extends ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    if text.contains("import ") && text.contains("from ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
        ],
    });

    // TOML
    registry.register(GrammarConfig {
        id: LanguageId::Toml,
        name: "toml",
        aliases: &[],
        ts_language: Some(tree_sitter_toml_ng::LANGUAGE.into()),
        highlight_query: tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
        injection_query: "",
        locals_query: "",
        comment_prefix: Some("#"),
        block_comment: None,
        detection_rules: &[
            DetectionRule {
                weight: 0.45,
                check: |text| {
                    let count = text
                        .lines()
                        .filter(|l| {
                            let trimmed = l.trim_start();
                            trimmed.starts_with('[')
                                && trimmed.ends_with(']')
                                && !trimmed.starts_with("[[")
                                && !trimmed.contains('"')
                        })
                        .count();
                    (count as f32 / 2.0).min(1.0)
                },
            },
            DetectionRule {
                weight: 0.30,
                check: |text| {
                    let kv = text
                        .lines()
                        .filter(|l| {
                            let trimmed = l.trim_start();
                            !trimmed.starts_with('[')
                                && trimmed.contains(" = ")
                                && !trimmed.starts_with('"')
                        })
                        .count();
                    (kv as f32 / 3.0).min(1.0)
                },
            },
            DetectionRule {
                weight: 0.15,
                check: |text| {
                    if text.contains("[[") && text.contains("]]") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.10,
                check: |text| {
                    if text.lines().any(|l| l.trim_start().starts_with('#')) {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
        ],
    });

    // YAML
    registry.register(GrammarConfig {
        id: LanguageId::Yaml,
        name: "yaml",
        aliases: &["yml"],
        ts_language: Some(tree_sitter_yaml::LANGUAGE.into()),
        highlight_query: tree_sitter_yaml::HIGHLIGHTS_QUERY,
        injection_query: "",
        locals_query: "",
        comment_prefix: Some("#"),
        block_comment: None,
        detection_rules: &[
            DetectionRule {
                weight: 0.30,
                check: |text| {
                    let kv = text
                        .lines()
                        .filter(|l| {
                            let trimmed = l.trim_start();
                            !trimmed.starts_with("//") && trimmed.contains(": ")
                        })
                        .count();
                    (kv as f32 / 10.0).min(1.0)
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    if text.starts_with("---") || text.contains("\n---") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.20,
                check: |text| {
                    if text.contains("- ")
                        && text
                            .lines()
                            .filter(|l| l.trim_start().starts_with("- "))
                            .count()
                            > 1
                    {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    if text.contains(": true")
                        || text.contains(": false")
                        || text.contains(": null")
                    {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
        ],
    });

    // Bash
    registry.register(GrammarConfig {
        id: LanguageId::Bash,
        name: "bash",
        aliases: &["sh"],
        ts_language: Some(tree_sitter_bash::LANGUAGE.into()),
        highlight_query: tree_sitter_bash::HIGHLIGHT_QUERY,
        injection_query: "",
        locals_query: "",
        comment_prefix: Some("#"),
        block_comment: Some((":", ":")), // Bash uses heredoc, not typical block comments
        detection_rules: &[
            DetectionRule {
                weight: 0.30,
                check: |text| {
                    if text.starts_with("#!/bin/bash") || text.starts_with("#!/usr/bin/env bash") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.25,
                check: |text| {
                    if text.contains("set -e") || text.contains("set -u") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.20,
                check: |text| {
                    if text.contains("export ") && text.contains("=") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.15,
                check: |text| {
                    if text.contains("echo ") || text.contains("printf ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
            DetectionRule {
                weight: 0.10,
                check: |text| {
                    if text.contains("if [") || text.contains("for ") || text.contains("while ") {
                        1.0
                    } else {
                        0.0
                    }
                },
            },
        ],
    });
}

/// Helper function to sample a rope for detection.
fn bounded_rope_sample(rope: &crop::Rope) -> String {
    let max = 16 * 1024;
    let end = floor_char_boundary(rope, rope.byte_len().min(max));
    rope.byte_slice(..end).to_string()
}

fn floor_char_boundary(rope: &crop::Rope, mut offset: usize) -> usize {
    offset = offset.min(rope.byte_len());
    while offset > 0 && !rope.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
