use crop::Rope;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum LanguageId {
    PlainText,
    Markdown,
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Json,
    Toml,
    Yaml,
    Bash,
}

impl LanguageId {
    pub const ALL: [LanguageId; 10] = [
        LanguageId::PlainText,
        LanguageId::Markdown,
        LanguageId::Rust,
        LanguageId::JavaScript,
        LanguageId::TypeScript,
        LanguageId::Python,
        LanguageId::Json,
        LanguageId::Toml,
        LanguageId::Yaml,
        LanguageId::Bash,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            LanguageId::PlainText => "Plain Text",
            LanguageId::Markdown => "Markdown",
            LanguageId::Rust => "Rust",
            LanguageId::JavaScript => "JavaScript",
            LanguageId::TypeScript => "TypeScript",
            LanguageId::Python => "Python",
            LanguageId::Json => "JSON",
            LanguageId::Toml => "TOML",
            LanguageId::Yaml => "YAML",
            LanguageId::Bash => "Bash",
        }
    }

    /// Returns the line comment prefix for this language, if any.
    pub fn comment_prefix(&self) -> Option<&'static str> {
        match self {
            LanguageId::PlainText | LanguageId::Markdown | LanguageId::Json => None,
            LanguageId::Rust | LanguageId::JavaScript | LanguageId::TypeScript => Some("//"),
            LanguageId::Python | LanguageId::Yaml | LanguageId::Bash => Some("#"),
            LanguageId::Toml => Some("#"),
        }
    }

    /// Returns the block comment delimiters (open, close) for this language, if any.
    #[allow(dead_code)]
    pub fn block_comment_delimiters(&self) -> Option<(&'static str, &'static str)> {
        match self {
            LanguageId::Rust | LanguageId::JavaScript | LanguageId::TypeScript => {
                Some(("/*", "*/"))
            }
            LanguageId::Python => Some(("\"\"\"", "\"\"\"")),
            LanguageId::Bash => Some((":", ":")), // Bash uses heredoc, not typical block comments
            LanguageId::PlainText
            | LanguageId::Markdown
            | LanguageId::Json
            | LanguageId::Toml
            | LanguageId::Yaml => None,
        }
    }

    /// Returns true if this language uses tree-sitter for syntax awareness.
    pub fn has_tree_sitter(&self) -> bool {
        !matches!(self, LanguageId::PlainText)
    }
}

#[derive(Clone, Debug)]
pub struct LanguageDetection {
    pub language: LanguageId,
    pub confidence: f32,
}

/// A detection rule that contributes to a language score.
#[derive(Clone)]
pub struct DetectionRule {
    /// Weight of this rule in the total score (0.0 to 1.0).
    pub weight: f32,
    /// The check function that returns a score contribution (0.0 to 1.0).
    pub check: fn(&str) -> f32,
}

/// Scored content detector that evaluates all languages and returns the best match.
pub struct ScoredDetector {
    rules: Vec<(LanguageId, Vec<DetectionRule>)>,
}

impl ScoredDetector {
    fn new() -> Self {
        let mut detector = Self { rules: Vec::new() };

        // Markdown
        detector.add_rules(
            LanguageId::Markdown,
            &[
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
        );

        // Json
        detector.add_rules(
            LanguageId::Json,
            &[
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
                        if text.contains("true") || text.contains("false") || text.contains("null")
                        {
                            1.0
                        } else {
                            0.0
                        }
                    },
                },
            ],
        );

        // Rust
        detector.add_rules(
            LanguageId::Rust,
            &[
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
        );

        // Python
        detector.add_rules(
            LanguageId::Python,
            &[
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
                            && (text.contains("def ")
                                || text.contains("if ")
                                || text.contains("for "))
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
        );

        // JavaScript
        detector.add_rules(
            LanguageId::JavaScript,
            &[
                DetectionRule {
                    weight: 0.25,
                    check: |text| {
                        if text.contains("function ") || text.contains("=>") {
                            1.0
                        } else {
                            0.0
                        }
                    },
                },
                DetectionRule {
                    weight: 0.20,
                    check: |text| {
                        if text.contains("const ") || text.contains("let ") || text.contains("var ")
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
        );

        // TypeScript
        detector.add_rules(
            LanguageId::TypeScript,
            &[
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
                        if text.contains("<T>") || text.contains("<T,") || text.contains("extends ")
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
                        if text.contains("import ") && text.contains("from ") {
                            1.0
                        } else {
                            0.0
                        }
                    },
                },
            ],
        );

        // Toml
        detector.add_rules(
            LanguageId::Toml,
            &[
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
        );

        // Yaml
        detector.add_rules(
            LanguageId::Yaml,
            &[
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
        );

        // Bash
        detector.add_rules(
            LanguageId::Bash,
            &[
                DetectionRule {
                    weight: 0.30,
                    check: |text| {
                        if text.starts_with("#!/bin/bash")
                            || text.starts_with("#!/usr/bin/env bash")
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
                        if text.contains("if [") || text.contains("for ") || text.contains("while ")
                        {
                            1.0
                        } else {
                            0.0
                        }
                    },
                },
            ],
        );

        detector
    }

    /// Creates a new empty detector with no rules.
    pub fn new_empty() -> Self {
        Self { rules: Vec::new() }
    }

    /// Adds detection rules for a language.
    pub fn add_rules(&mut self, lang: LanguageId, rules: &[DetectionRule]) {
        let owned_rules: Vec<DetectionRule> = rules.to_vec();
        self.rules.push((lang, owned_rules));
    }

    /// Score all languages against the text and return the best match.
    pub fn detect(&self, text: &str) -> LanguageDetection {
        let mut best_lang = LanguageId::PlainText;
        let mut best_score = 0.2; // Threshold for plain text detection

        for (lang, rules) in &self.rules {
            let mut score = 0.0;
            for rule in rules {
                score += rule.weight * (rule.check)(text);
            }
            score = score.min(1.0);

            if score > best_score {
                best_score = score;
                best_lang = *lang;
            }
        }

        LanguageDetection {
            language: best_lang,
            confidence: best_score,
        }
    }
}

impl Default for ScoredDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
fn bounded_sample(text: &str) -> &str {
    let max = 16 * 1024;
    if text.len() <= max {
        text
    } else {
        let boundary = text
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= max)
            .last()
            .unwrap_or(0);
        &text[..boundary]
    }
}

#[allow(dead_code)]
fn bounded_rope_sample(rope: &Rope) -> String {
    let max = 16 * 1024;
    let end = floor_char_boundary(rope, rope.byte_len().min(max));
    rope.byte_slice(..end).to_string()
}

#[allow(dead_code)]
fn floor_char_boundary(rope: &Rope, mut offset: usize) -> usize {
    offset = offset.min(rope.byte_len());
    while offset > 0 && !rope.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar_registry::GrammarRegistry;

    #[test]
    fn detects_markdown_before_embedded_code() {
        let registry = GrammarRegistry::default();
        let detection = registry.detect("# Notes\n\n```rust\nfn main() {}\n```\n");

        assert_eq!(detection.language, LanguageId::Markdown);
        assert!(detection.confidence > 0.2);
    }

    #[test]
    fn detects_markdown_for_common_whole_note_shapes() {
        let registry = GrammarRegistry::default();

        let heading = registry.detect("# Notes\n\nSome prose under a heading.\n");
        assert_eq!(heading.language, LanguageId::Markdown);
        assert!(heading.confidence > 0.2);

        let list = registry.detect("- first\n- second\n- third\n");
        assert_eq!(list.language, LanguageId::Markdown);
        assert!(list.confidence > 0.2);

        let fence = registry.detect("```rust\nfn main() {}\n```\n");
        assert_eq!(fence.language, LanguageId::Markdown);
        assert!(fence.confidence > 0.2);
    }

    #[test]
    fn detects_common_structured_scratch_content() {
        let registry = GrammarRegistry::default();

        let rust = registry.detect("fn main() { let value = 1; }");
        assert_eq!(rust.language, LanguageId::Rust);
        assert!(rust.confidence > 0.2);

        let json = registry.detect("{\"key\": true}");
        assert_eq!(json.language, LanguageId::Json);
        assert!(json.confidence > 0.2);

        let python = registry.detect("def main():\n    pass\n");
        assert_eq!(python.language, LanguageId::Python);
        assert!(python.confidence > 0.2);
    }

    #[test]
    fn detects_javascript_with_function_and_const() {
        let registry = GrammarRegistry::default();
        let detection = registry.detect("const add = (a, b) => a + b;");
        assert_eq!(detection.language, LanguageId::JavaScript);
        assert!(detection.confidence > 0.2);
    }

    #[test]
    fn detects_javascript_console_log_snippet() {
        let registry = GrammarRegistry::default();
        let detection = registry.detect("console.log('hello')\n");
        assert_eq!(detection.language, LanguageId::JavaScript);
        assert!(detection.confidence > 0.2);
    }

    #[test]
    fn detects_typescript_with_interface() {
        let registry = GrammarRegistry::default();
        let detection = registry.detect("interface User {\n  name: string;\n  age: number;\n}");
        assert_eq!(detection.language, LanguageId::TypeScript);
        assert!(detection.confidence > 0.2);
    }

    #[test]
    fn detects_toml_with_sections() {
        let registry = GrammarRegistry::default();
        let detection = registry.detect("[server]\nport = 8080\nhost = \"localhost\"\n");
        assert_eq!(detection.language, LanguageId::Toml);
        assert!(detection.confidence > 0.2);
    }

    #[test]
    fn detects_yaml_with_mappings() {
        let registry = GrammarRegistry::default();
        let detection = registry.detect("name: John\nage: 30\nactive: true\n");
        assert_eq!(detection.language, LanguageId::Yaml);
        assert!(detection.confidence > 0.2);
    }

    #[test]
    fn detects_bash_with_shebang() {
        let registry = GrammarRegistry::default();
        let detection = registry.detect("#!/bin/bash\nset -e\nexport PATH=/usr/local/bin:$PATH\n");
        assert_eq!(detection.language, LanguageId::Bash);
        assert!(detection.confidence > 0.2);
    }

    #[test]
    fn detects_plain_text_for_unknown_content() {
        let registry = GrammarRegistry::default();
        let detection =
            registry.detect("Just some random text without any code.\nNothing special here.\n");
        assert_eq!(detection.language, LanguageId::PlainText);
    }

    #[test]
    fn scored_detector_returns_highest_scoring_language() {
        let detector = ScoredDetector::new();

        // JSON should score highest for JSON input
        let json_score = detector.detect("{\"key\": true, \"items\": [1, 2, 3]}");
        assert_eq!(json_score.language, LanguageId::Json);

        // Rust should score highest for Rust input
        let rust_score =
            detector.detect("pub fn main() {\n    let x = 42;\n    println!(\"{}\", x);\n}");
        assert_eq!(rust_score.language, LanguageId::Rust);
    }

    #[test]
    fn binary_content_detected_as_plaintext() {
        let registry = GrammarRegistry::default();
        // Use binary data with null bytes - clearly binary
        let binary: Vec<u8> = vec![0x00, 0x01, 0x02, 0x89, 0x50, 0x4E, 0x47];
        let text = String::from_utf8_lossy(&binary);
        let detection = registry.detect(&text);
        assert_eq!(detection.language, LanguageId::PlainText);
        // Binary detection may not always return 1.0 with from_utf8_lossy
        // as it replaces invalid UTF-8 with replacement characters
        assert!(detection.confidence >= 0.2);
    }
}
