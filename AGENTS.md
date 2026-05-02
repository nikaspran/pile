# pile Agent Guide

## Vision

`pile` is an infinite scratchpad: a fast, native editor for dumping and
recovering large numbers of unsaved notes. The application should feel closer to
a reliable text tray than an IDE. It must not ask users to save, choose projects,
configure workspaces, or manage files before writing.

The product is optimized for:

- reliability during crashes, restarts, and frequent app exits;
- low-latency typing and navigation;
- hundreds of open scratch buffers;
- mixed prose/code notes with content-aware highlighting;
- small, comprehensible systems over feature breadth.

## Non-Goals

Do not add LSP integration, project trees, integrated terminals, build systems,
debuggers, plugin frameworks, or manual save prompts. File import/export can
exist later, but the primary workflow is unsaved scratch buffers restored by the
session system.

## Architecture Boundaries

The live app state belongs to the UI thread. Do not put the main editor model
behind shared mutable synchronization just to communicate with workers. Workers
should receive immutable snapshots, cheap rope clones, ids, versions, and
bounded requests.

Main document text must be stored in a rope. Temporary strings are acceptable
for UI layout, serialization boundaries, search snippets, and interoperability
with APIs that require contiguous text, but they must not become the canonical
editor buffer.

Persistence is a background service. Every meaningful state mutation should
enqueue a session snapshot. The worker owns debounce timing, binary
serialization, and atomic file replacement. The UI must never block on routine
saves, and it must never show a save prompt.

Syntax highlighting should be content-aware. Prefer bounded heuristics and
tree-sitter parse quality over file extensions. Mixed-language notes are a first
class use case: Markdown fences, embedded snippets, and injected ranges should
resolve to separate highlight spans without changing the underlying document.

Navigation should be modeled independently from the rendered tab strip. The app
starts with tabs, but state should remain compatible with future virtualized tab
lists, tab search, and command-palette navigation.

## Engineering Expectations

Keep changes small and architecture-preserving. Favor explicit data flow over
hidden global state. Prefer simple structs and typed messages before broader
abstractions.

When adding editor behavior, include tests around the model or persistence layer
where practical. UI rendering can be smoke-tested later, but session
restoration, debounce behavior, language detection, and text transformations
should have direct coverage.

Performance-sensitive paths should avoid full-document string materialization.
If a temporary `String` is unavoidable, keep it bounded to a visible range,
search window, or persistence operation and make that scope obvious.

## Dependency Rationale

- `eframe`/`egui`: native immediate-mode GUI with low setup overhead.
- `crop`: rope storage with cheap clones and serde support.
- `crossbeam-channel`: small, predictable worker communication.
- `bincode`: compact session snapshots.
- `atomic-write-file`: crash-resistant session replacement.
- `directories`: platform-correct local data directories.
- `tree-sitter` and grammar crates: incremental syntax parsing and injected
  language support.

## Documentation Guidance

Keep public docs focused on the product vision and user-visible behavior. Keep
agent docs focused on boundaries, non-goals, and architectural invariants. Avoid
pinning high-level docs to exact module paths until the implementation shape is
more stable.
