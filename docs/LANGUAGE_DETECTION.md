# Language Detection and Injection Architecture

## Overview

`pile` uses a two-phase approach for syntax awareness: content-based language detection for the primary document language, and tree-sitter injection for mixed-language documents (primarily Markdown fenced code blocks).

## Language Detection

### ScoredDetector

The `ScoredDetector` (in `src/syntax.rs`) evaluates multiple heuristics per language and returns the best match:

```rust
pub struct ScoredDetector {
    rules: Vec<(LanguageId, Vec<DetectionRule>)>,
}
```

Each `DetectionRule` has:
- `weight`: Importance of this rule (0.0 to 1.0, summed per language)
- `check`: Closure that returns a score contribution (0.0 to 1.0)

### Detection Process

1. `ScoredDetector::detect(text)` iterates over all registered languages
2. For each language, it sums: `weight * (rule.check)(text)`
3. The result is clamped to `[0.0, 1.0]`
4. The language with the highest score (above 0.2 threshold) wins

### Current Languages

| Language | Detection Cues |
|----------|----------------|
| Markdown | Headings, fenced code blocks, list markers, bold/italic, links |
| JSON | Starts with `{` or `[`, key-value pairs, boolean/null values |
| Rust | `fn `, `pub fn `, `use `, `mod `, `impl `, `let `, `-> `, `::`, `#[` |
| Python | `def `, `class `, `import `, `from `, `if __name__`, `:` with keywords |
| JavaScript | `function `, `=>`, `const `, `let `, `var `, `console.log`, `export `, `require(`, `import `, `document.`, `window.` |
| TypeScript | `: string`, `: number`, `: boolean`, `interface `, `type `, `<T>`, `extends `, `import..from` |
| TOML | `[section]` headers, `key = value` pairs, `[[array]]`, `#` comments |
| YAML | `key: value` mappings, `---` frontmatter, `- ` list items, `: true/false/null` |
| Bash | `#!/bin/bash`, `set -e`, `export..=`, `echo `, `printf `, `if [`, `for `, `while ` |

### Bounded Sampling

For large documents, detection uses `bounded_sample()` which limits input to 16KB to avoid materializing huge strings.

## Injection Architecture

### Tree-sitter Injection

Tree-sitter languages can declare "injection queries" that identify regions of the document that should be parsed as a different language.

### How it Works

1. **Grammar Registration**: Each language registers with `GrammarRegistry` (in `src/grammar_registry.rs`)
2. **Injection Query**: Languages like Markdown provide an `injection_query` (e.g., `tree_sitter_md::INJECTION_QUERY_BLOCK`)
3. **Highlight Configuration**: When building `HighlightConfiguration`, the injection query is passed to tree-sitter
4. **Injection Registry**: `GrammarRegistry::injection_registry()` builds a map of language names to their highlight configs
5. **During Highlighting**: `tree_sitter_highlight::Highlighter::highlight()` calls a callback to resolve injected language names

### Current Injection Support

| Host Language | Injected Languages | Injection Query Source |
|---------------|-------------------|----------------------|
| Markdown | Rust, Python, JavaScript, TypeScript, YAML, Bash, etc. | `tree_sitter_md::INJECTION_QUERY_BLOCK` |
| Rust | Rust (doc comments) | `tree_sitter_rust::INJECTIONS_QUERY` |
| JavaScript | JavaScript (template literals) | `tree_sitter_javascript::INJECTIONS_QUERY` |
| Others | None | (empty string) |

### Injection Flow

```
Document text
    ↓
tree-sitter-highlight::Highlighter::highlight(config, text, callback)
    ↓
Tree-sitter parses host language (e.g., Markdown)
    ↓
Injection query finds fenced code blocks
    ↓
Callback: injection_registry().get("rust") → HighlightConfiguration
    ↓
Nested parse: highlight fenced block as Rust
    ↓
HighlightEvent stream: Source { start, end } with active highlight stack
    ↓
DocumentSyntaxState::generate_highlight_spans() → Vec<HighlightSpan>
```

### HighlightSpan

```rust
pub struct HighlightSpan {
    pub start: usize,  // byte offset
    pub end: usize,    // byte offset
    pub highlight: usize, // index into highlight names array
}
```

## Key Files

- `src/syntax.rs`: `ScoredDetector`, `LanguageId`, `DetectionRule`
- `src/grammar_registry.rs`: `GrammarRegistry`, `GrammarConfig`, injection registry
- `src/syntax_highlighting.rs`: `DocumentSyntaxState`, `HighlightSpan`, highlight generation
- `src/parse_worker.rs`: Background parse scheduling with cancellation

## Design Decisions

1. **Scored detection over single-rule**: Allows combining multiple weak signals for robust detection
2. **Bounded sampling**: Prevents UI stalls on huge documents
3. **Injection via tree-sitter**: Leverages tree-sitter's native injection support rather than custom parsing
4. **Injection registry as cache**: Lazy-initialized `OnceLock` map for O(1) language lookup during highlighting
