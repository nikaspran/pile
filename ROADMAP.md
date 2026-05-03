# pile Roadmap

This document tracks the major missing capabilities needed for `pile` to become
a mature, fast, general-purpose scratchpad editor while preserving its core
constraint: no project system, no LSP, no terminal, and no manual save prompts.

## Editing Core

- Completed: replace the temporary `egui::TextEdit` surface with a v1 custom
  rope-native editor view that renders visible lines without routine full-buffer
  materialization.
- Completed: implement cursor movement by grapheme cluster.
- Completed: implement cursor movement by word, line, paragraph, document boundary, and page.
- Completed: add mouse drag selection and richer selection rendering for the custom editor.
- Completed: add robust selection expansion and contraction for character, word, line,
  bracket pair, and indentation block scopes.
- Completed: add multiple cursors as a first-class model with add-next-match, add-all-match,
  split-selection-into-lines, and rectangular/column selection.
- Completed: add undo/redo backed by an edit history that understands grouped
  typing, paste, replace, and multi-cursor transactions. A single-range
  `DocumentEdit` transaction API exists; multi-range transactions are now
  implemented for multiple cursors, replace-all, and line-operation cleanup.
- Completed: add indentation commands for Tab, Shift-Tab, and auto-indent on
  newline.
- Completed: Add tab width settings, soft tabs, and whitespace normalization.
- Completed: Add bracket and quote pairing with overwrite, skip-over, and delete-pair
  behavior.
- Completed: add duplicate-line and delete-line commands.
- Completed: add move-line-up and move-line-down commands.
- Completed: add join-lines command.
- Completed: add sort-lines command.
- Completed: add remaining line operations: reverse lines and trim trailing whitespace.
- Completed: Add case conversion commands for selections and cursors.
- Completed: Add comment toggling for detected code regions.

## Search and Replace

- Completed: add incremental in-buffer search with match counts and current-match
  navigation.
- Completed: add match highlighting for all visible search results in the custom
  editor renderer, with a distinct current match.
- Completed: add replace and replace-all (undo grouping deferred until an
  undo/redo stack lands).
- Completed: add regular expression search and replace.
- Completed: add case-sensitive, whole-word, and wrap-around modes for in-buffer
  search.
- Completed: move in-buffer search from active-document string materialization to a
  rope-native bounded-window search engine.
- Completed: add search-in-tabs across all open scratch buffers.
- Add search result previews with quick navigation.
- Add find-under-cursor and select-next-occurrence commands.

## Navigation and Tabs

- Add command palette infrastructure for all commands.
- Add quick tab switcher with fuzzy search across hundreds of buffers.
- Add recently used tab ordering.
- Add virtualized tab list rendering for large sessions.
- Add tab close buttons, tab reordering, and pinned tabs.
- Add split editor panes with shared document backing.
- Add go-to-line and go-to-symbol-like navigation for detected document
  structure.
- Add bookmarks or lightweight marks within scratch buffers.
- Add session-level tab restore ordering, active pane restore, and focus restore.

## Rendering and Layout

- Build a custom text layout pipeline with stable line heights, fast viewport
  measurement, and no nested editor frame.
- Persist and restore custom editor scroll offsets per document.
- Add line wrapping modes: no wrap, viewport wrap, and ruler wrap.
- Add configurable rulers.
- Add current-line highlight.
- Add visible whitespace rendering.
- Add indentation guides.
- Add bracket matching highlights.
- Add minimap with viewport indicator and click/drag navigation.
- Add smooth scrolling and large-file-safe viewport virtualization.
- Add high-DPI and font fallback testing.
- Add theme support with bundled dark and light themes.

## Syntax and Language Awareness

- Replace placeholder language heuristics with a scored content detector.
- Wire tree-sitter parsers into incremental parse state per document.
- Add range-based highlighting for injected languages.
- Add Markdown fenced-code injection support.
- Add syntax-aware comments, brackets, and indentation rules.
- Add diagnostics-free parse status display for low-confidence detection.
- Add grammar registry configuration for adding languages without changing
  editor core code.
- Add highlight cache invalidation keyed by document revision and visible byte
  range.

## Persistence and Reliability

- Store sessions in a versioned envelope with explicit migration hooks.
- Add crash-resilient backup rotation for corrupt session files.
- Add restore validation for tab order, active document, selections, scroll, and
  syntax metadata.
- Add save-worker telemetry and surfaced recovery logs.
- Add deterministic flush on app close and system shutdown events.
- Add stress tests for rapid edits, many tabs, large buffers, and repeated
  crash/restart cycles.
- Add bounded snapshot memory accounting so huge sessions cannot stall the UI.

## Native App Integration

- Complete native menu support across supported desktop platforms.
- Add macOS application bundle metadata, icon, and signing-ready packaging.
- Add native file import/export commands without making files the primary
  workflow.
- Add clipboard integration for rich/plain text where available.
- Add drag-and-drop text/file import.
- Add native preferences window for editor settings.
- Add per-platform keyboard shortcut conventions.
- Add window state restore: size, position, fullscreen, and display.

## Settings and Customization

- Add persisted settings separate from hot-exit session state.
- Add font family, font size, line height, tab width, and wrap settings.
- Add theme selection.
- Add keybinding configuration.
- Add ignored grammar/language preferences for content detection.
- Add status bar visibility and minimap visibility settings.
- Add command palette entries for toggles.

## Performance Work

- Establish benchmarks for editing latency, startup restore time, syntax parse
  time, search time, and memory use.
- Add large-buffer tests for megabyte and multi-megabyte scratch documents.
- Avoid full-buffer string conversion in render and edit paths.
- Use rope slices for viewport layout, search windows, and parser input.
- Add background parse scheduling with cancellation by document revision.
- Add profiling hooks for UI frame time and save-worker latency.
- Audit allocations in typing, scrolling, search, and tab switching.

## Testing

- Add model tests for editing transactions and multi-cursor behavior.
- Add persistence tests for schema migration and corrupt restore handling.
- Add syntax tests for injected ranges and mixed-language documents.
- Add golden tests for search/replace edge cases.
- Add UI smoke tests for tab switching, renaming, shortcuts, and session restore.
- Add property tests for rope edits and selection transformations.
- Add platform checks for native menu command delivery.

## Documentation

- In progress: document the command model and keybinding conventions.
- Completed: document the current editor split and transaction direction.
- Document persistence guarantees and recovery behavior.
- Document language detection and injection architecture.
- Document performance invariants for future contributors and agents.
- Document non-goals so feature additions do not pull the app toward IDE scope.
