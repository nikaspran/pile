use crop::Rope;
use pile::model::{AppState, Document, SessionSnapshot, Selection};
use std::collections::BTreeSet;
use uuid::Uuid;

/// Generate random text of approximately the given byte length
pub fn generate_text(bytes: usize) -> String {
    let words = [
        "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog",
        "hello", "world", "rust", "code", "editor", "buffer", "text",
        "line", "cursor", "selection", "search", "replace", "syntax",
    ];
    let mut text = String::with_capacity(bytes);
    let mut current = 0;
    let mut word_idx = 0;

    while current < bytes {
        let word = words[word_idx % words.len()];
        word_idx += 1;

        if current + word.len() + 1 > bytes {
            break;
        }

        text.push_str(word);
        current += word.len();

        if current < bytes {
            text.push(' ');
            current += 1;
        }

        if word_idx % 10 == 0 && current + 1 < bytes {
            text.push('\n');
            current += 1;
        }
    }
    text
}

/// Generate random positions for editing
pub fn generate_random_positions(max_bytes: usize, count: usize) -> Vec<usize> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut positions = Vec::with_capacity(count);
    for i in 0..count {
        let mut hasher = DefaultHasher::new();
        i.hash(&mut hasher);
        let hash = hasher.finish();
        positions.push((hash as usize) % max_bytes.max(1));
    }
    positions
}

/// Generate Rust code of approximately the given line count
pub fn generate_rust_code(lines: usize) -> String {
    let mut code = String::new();
    code.push_str("// Generated Rust code\n");
    code.push_str("use std::collections::HashMap;\n\n");

    for i in 0..lines {
        if i % 20 == 0 {
            code.push_str(&format!("\nfn function_{}() -> i32 {{\n", i / 20));
            code.push_str("    let mut x = 0;\n");
            code.push_str("    for i in 0..10 {\n");
            code.push_str("        x += i * 2;\n");
            code.push_str("    }\n");
            code.push_str("    let map: HashMap<String, i32> = HashMap::new();\n");
            code.push_str("    if x > 50 {\n");
            code.push_str("        println!(\"x is {}\", x);\n");
            code.push_str("    }\n");
            code.push_str("    x\n");
            code.push_str("}\n");
        } else {
            code.push_str(&format!("    // line {}\n", i));
        }
    }
    code
}

/// Generate JavaScript code of approximately the given line count
pub fn generate_js_code(lines: usize) -> String {
    let mut code = String::new();
    code.push_str("// Generated JavaScript code\n\n");

    for i in 0..lines {
        if i % 15 == 0 {
            code.push_str(&format!("function func{}() {{\n", i / 15));
            code.push_str("  const arr = [1, 2, 3, 4, 5];\n");
            code.push_str("  let sum = 0;\n");
            code.push_str("  for (const item of arr) {\n");
            code.push_str("    sum += item;\n");
            code.push_str("  }\n");
            code.push_str("  return sum;\n");
            code.push_str("}\n\n");
        } else {
            code.push_str(&format!("// line {}\n", i));
        }
    }
    code
}

/// Generate Python code of approximately the given line count
pub fn generate_python_code(lines: usize) -> String {
    let mut code = String::new();
    code.push_str("# Generated Python code\n\n");

    for i in 0..lines {
        if i % 15 == 0 {
            code.push_str(&format!("def function_{}():\n", i / 15));
            code.push_str("    result = 0\n");
            code.push_str("    for i in range(10):\n");
            code.push_str("        result += i * 2\n");
            code.push_str("    print(f\"result is {result}\")\n");
            code.push_str("    return result\n\n");
        } else {
            code.push_str(&format!("# line {}\n", i));
        }
    }
    code
}

/// Create a test session snapshot with the given number of tabs
pub fn create_test_snapshot(tab_count: usize, tab_size: usize) -> SessionSnapshot {
    let mut documents = Vec::new();
    let mut tab_order = Vec::new();

    for i in 0..tab_count {
        let mut doc = Document::new_untitled(i as u64 + 1, 4, false);
        let text = generate_text(tab_size);
        doc.replace_text(&text);
        doc.title_hint = format!("Tab {}", i);

        let id = doc.id;
        documents.push(doc);
        tab_order.push(id);
    }

    let active = tab_order.first().copied().unwrap_or(Uuid::nil());
    let mut state = AppState::empty();
    state.documents = documents;
    state.tab_order = tab_order;
    state.active_document = active;
    state.next_untitled_index = tab_count as u64 + 1;

    SessionSnapshot {
        schema_version: 2,
        state,
        panes: Vec::new(),
        active_pane: 0,
    }
}

/// Parse syntax for given text (simplified - would use tree-sitter in real impl)
pub fn parse_syntax(text: &str) -> usize {
    // Simulate parsing work
    let mut count = 0;
    for line in text.lines() {
        count += line.len();
    }
    count
}

/// Parse and highlight text
pub fn parse_and_highlight(text: &str) -> Vec<(usize, usize, u8)> {
    // Simplified highlight span generation
    let mut spans = Vec::new();
    let mut pos = 0;
    for line in text.lines() {
        if line.starts_with("fn ") || line.starts_with("let ") || line.starts_with("use ") {
            spans.push((pos, pos + 2, 1)); // keyword
        }
        pos += line.len() + 1;
    }
    spans
}

/// Search text for a pattern (simple contains check)
pub fn search_text(text: &str, pattern: &str) -> usize {
    text.matches(pattern).count()
}

/// Search text with regex
pub fn search_regex(text: &str, pattern: &str) -> usize {
    regex::Regex::new(pattern)
        .map(|re| re.find_iter(text).count())
        .unwrap_or(0)
}

/// Windowed search implementation (simulating the 16KB window approach)
pub fn windowed_search(text: &str, pattern: &str, window_size: usize) -> usize {
    let bytes = text.as_bytes();
    let mut count = 0;
    let mut offset = 0;

    while offset < bytes.len() {
        let end = (offset + window_size).min(bytes.len());
        let window = &bytes[offset..end];
        if let Ok(window_str) = std::str::from_utf8(window) {
            count += window_str.matches(pattern).count();
        }
        offset = end;
    }
    count
}
