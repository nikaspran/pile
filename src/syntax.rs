use content_inspector::{ContentType, inspect};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug)]
pub struct LanguageDetection {
    pub language: LanguageId,
    pub confidence: f32,
}

#[derive(Default)]
pub struct LanguageRegistry;

impl LanguageRegistry {
    pub fn detect(&self, text: &str) -> LanguageDetection {
        if matches!(inspect(text.as_bytes()), ContentType::BINARY) {
            return LanguageDetection {
                language: LanguageId::PlainText,
                confidence: 1.0,
            };
        }

        let sample = bounded_sample(text);

        if looks_like_markdown(sample) {
            return LanguageDetection {
                language: LanguageId::Markdown,
                confidence: 0.85,
            };
        }

        if looks_like_json(sample) {
            return LanguageDetection {
                language: LanguageId::Json,
                confidence: 0.8,
            };
        }

        if looks_like_rust(sample) {
            return LanguageDetection {
                language: LanguageId::Rust,
                confidence: 0.75,
            };
        }

        if looks_like_python(sample) {
            return LanguageDetection {
                language: LanguageId::Python,
                confidence: 0.7,
            };
        }

        if looks_like_javascript(sample) {
            return LanguageDetection {
                language: LanguageId::JavaScript,
                confidence: 0.65,
            };
        }

        if looks_like_typescript(sample) {
            return LanguageDetection {
                language: LanguageId::TypeScript,
                confidence: 0.65,
            };
        }

        if looks_like_toml(sample) {
            return LanguageDetection {
                language: LanguageId::Toml,
                confidence: 0.6,
            };
        }

        if looks_like_yaml(sample) {
            return LanguageDetection {
                language: LanguageId::Yaml,
                confidence: 0.6,
            };
        }

        if looks_like_bash(sample) {
            return LanguageDetection {
                language: LanguageId::Bash,
                confidence: 0.6,
            };
        }

        LanguageDetection {
            language: LanguageId::PlainText,
            confidence: 0.4,
        }
    }
}

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

fn looks_like_markdown(text: &str) -> bool {
    text.lines().any(|line| {
        line.starts_with("# ")
            || line.starts_with("## ")
            || line.starts_with("- ")
            || line.starts_with("* ")
            || line.starts_with("```")
    })
}

fn looks_like_json(text: &str) -> bool {
    let trimmed = text.trim_start();
    (trimmed.starts_with('{') || trimmed.starts_with('[')) && text.contains(':')
}

fn looks_like_rust(text: &str) -> bool {
    text.contains("fn ") || text.contains("use ") || text.contains("impl ") || text.contains("let ")
}

fn looks_like_python(text: &str) -> bool {
    text.contains("def ") || text.contains("import ") || text.contains("if __name__")
}

fn looks_like_javascript(text: &str) -> bool {
    text.contains("function ") || text.contains("const ") || text.contains("let ")
}

fn looks_like_typescript(text: &str) -> bool {
    text.contains("interface ") || text.contains("type ") || text.contains(": string")
}

fn looks_like_toml(text: &str) -> bool {
    text.lines()
        .any(|line| line.starts_with('[') && line.ends_with(']'))
}

fn looks_like_yaml(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        !trimmed.starts_with("//") && trimmed.contains(": ")
    })
}

fn looks_like_bash(text: &str) -> bool {
    text.starts_with("#!/") || text.contains("set -e") || text.contains("export ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_markdown_before_embedded_code() {
        let registry = LanguageRegistry;
        let detection = registry.detect("# Notes\n\n```rust\nfn main() {}\n```\n");

        assert_eq!(detection.language, LanguageId::Markdown);
    }

    #[test]
    fn detects_common_structured_scratch_content() {
        let registry = LanguageRegistry;

        assert_eq!(
            registry.detect("fn main() { let value = 1; }").language,
            LanguageId::Rust
        );
        assert_eq!(
            registry.detect("{\"key\": true}").language,
            LanguageId::Json
        );
        assert_eq!(
            registry.detect("def main():\n    pass\n").language,
            LanguageId::Python
        );
    }
}
