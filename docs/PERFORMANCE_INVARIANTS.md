# Performance Invariants

This document records performance invariants that all future code changes must preserve. These are not suggestions—they are requirements for the `pile` product.

## Core Invariants

### 1. No Full-Buffer String Materialization in Hot Paths

**Rule**: The editor, search, layout, and render paths must not convert the entire document `Rope` to a `String` during routine operations.

**Background**: `crop::Rope` stores text in chunks. Converting to a contiguous `String` requires walking the entire structure and copying bytes. For megabyte-sized documents, this causes visible stalls.

**Allowed**:
- Bounded window materialization (e.g., 16KB search windows in `search.rs`)
- RopeSlice::chars() iteration for small outputs (e.g., building context strings for search previews)
- Temporary strings at serialization boundaries (persistence, parse worker input)

**Forbidden**:
- `rope.to_string()` in editor typing, scrolling, or rendering paths
- Materializing the full document for syntax highlighting (use `DocumentSyntaxState` cache + background worker)

**Test**: `stress_tests::tests::large_buffer_*` verify megabyte documents don't stall.

### 2. UI Thread Must Never Block on Persistence

**Rule**: All session saves happen on a background thread via `SaveWorker`. The UI thread sends `SaveMsg` and continues immediately.

**Background**: Users perceive any UI freeze as a crash. The save worker owns debounce timing, serialization, and atomic file replacement.

**Implementation**:
- `SaveWorker` receives `SaveMsg::Changed(snapshot)` via `crossbeam-channel`
- Debounces rapid edits (default: 2 seconds)
- Serializes `SessionSnapshot` using `bincode`
- Writes atomically via `atomic-write-file`

**Forbidden**:
- `fs::write()` or `bincode::serialize()` on the UI thread
- Any `.expect()` or `?` in save path that could block

### 3. Routine Editing Transactions Must Use DocumentEdit

**Rule**: New single-range edits should go through `Document::apply_edit(DocumentEdit)` or its grouped variants.

**Background**: The `DocumentEdit` API records undo state, updates revision, and applies the edit in one place. Manual rope mutations scatter this logic and cause undo bugs.

**Use**:
- `Document::apply_edit()` - single edit with undo
- `Document::apply_grouped_edit()` - single edit as its own undo group
- `Document::apply_continuing_edit()` - part of current typing group
- `Document::apply_multi_edit()` - multiple non-overlapping range edits

**Avoid**: Directly calling `document.rope.delete()` + `document.rope.insert()` without recording undo.

### 4. Syntax Highlighting Must Be Incremental and Background

**Rule**: Parse state is stored per document in `DocumentSyntaxState`. Reparsing happens on a background thread via `ParseWorker`.

**Background**: Tree-sitter parsers are fast, but still too slow to run on the UI thread for large documents during typing.

**Implementation**:
- `DocumentSyntaxState::needs_parse()` checks if reparse is needed
- `ParseWorker` receives rope snapshots and sends back `(Tree, Vec<HighlightSpan>)`
- `DocumentSyntaxState::update_from_parse_result()` updates cached spans

**Invariant**: The UI thread only reads from `DocumentSyntaxState::highlight()` which returns cached spans when available.

### 5. Search Windows Must Be Bounded

**Rule**: The `find_matches()` function uses sliding windows of `SEARCH_WINDOW_BYTES` (16KB) to avoid materializing huge documents.

**Background**: Regex search requires `&str`, which means materialization. Bounded windows keep this bounded.

**Implementation**:
- `SEARCH_WINDOW_BYTES = 16 * 1024`
- Windows overlap by `needle_len - 1` bytes to catch matches spanning window boundaries
- Regex search is currently full-materialization (acceptable for now, but should be replaced for large-buffer safety)

### 6. Allocation Budget for Session Snapshots

**Rule**: Sessions have a size budget (`check_snapshot_budget()`). Huge sessions are rejected to prevent UI stalls.

**Background**: A session with hundreds of tabs, each with megabyte documents, could produce tens of megabytes of bincode output. Serializing this would stall the UI.

**Implementation**:
- `check_snapshot_budget()` returns `BudgetCheck::Over(max)` if too large
- The save worker checks this before serializing
- `MAX_SESSION_BYTES` is the threshold (currently 50MB)

## Performance-Safe Patterns

### Pattern: Bounded Rope Sampling
```rust
fn bounded_sample(rope: &Rope) -> String {
    let max = 16 * 1024;
    let end = floor_char_boundary(rope, rope.byte_len().min(max));
    rope.byte_slice(..end).to_string()
}
```

### Pattern: Windowed Search
```rust
while window_start < rope_len {
    let window_end = floor_char_boundary(rope, (window_start + SEARCH_WINDOW_BYTES).min(rope_len));
    let window = rope.byte_slice(window_start..window_end).to_string();
    // search within window
}
```

### Pattern: Cached Highlight Spans
```rust
pub fn highlight(&mut self, ..) -> Vec<HighlightSpan> {
    if let Some((cached_rev, ..)) = &self.cached_spans {
        if *cached_rev == revision {
            return cached_spans.clone(); // O(1) hit
        }
    }
    // Recompute and cache
}
```

## Stress Test Coverage

Run `cargo test --stress_tests` to verify these invariants:

| Test | What it verifies |
|------|------------------|
| `large_buffer_5mb` | Basic operations at 5MB |
| `large_buffer_10mb` | Basic operations at 10MB |
| `large_buffer_20mb_*` | Various operations at 20MB |
| `large_buffer_50mb_*` | Operations at 50MB |
| `large_buffer_100mb_*` | Upper bound operations at 100MB |
| `rapid_edits_*` | Rapid typing doesn't stall |
| `crash_restart_*` | Session restore works after crash |

## Profiling Hooks

The save worker tracks timing:
- `SaveTelemetry::record_save_duration()`
- `SaveTelemetry::median_save_duration_ms()`
- `SaveTelemetry::p95_save_duration_ms()`

Use these to detect regressions in serialization performance.

## What NOT to Do

- **Don't** add `"use rope.to_string()"` in render/scroll/edit paths
- **Don't** add synchronous file I/O on the UI thread
- **Don't** replace tree-sitter background parsing with synchronous parsing
- **Don't** remove the `SEARCH_WINDOW_BYTES` limit for "simplicity"
- **Don't** bypass `DocumentEdit` for "just this one quick edit"
