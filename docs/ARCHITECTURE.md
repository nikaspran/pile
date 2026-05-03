# Architecture Notes

This document records the current internal direction for `pile`. It is meant for
future contributors and agents working in the codebase, not for end-user product
documentation.

## Current Shape

The live application state still belongs to the UI thread. `PileApp` owns the
session state, editor view state, search state, syntax detection state, native
menu bridge, and save-worker channel. Background persistence receives cloned
session snapshots and owns debounce timing plus atomic replacement.

Document text is stored in `crop::Rope`. Temporary `String` values are used at
serialization boundaries, bounded syntax detection samples, focused search
windows, display titles, and edit snippets. They should not become the canonical
editor buffer.

## Editor Modules

`src/editor.rs` is the editor module root and the rendering entry point. It keeps
the public editor API stable for the rest of the app and delegates implementation
details to focused submodules:

- `editor/geometry.rs`: byte, line, column, grapheme, word, selection, and paint
  geometry helpers.
- `editor/input.rs`: egui event dispatch into editor commands.
- `editor/ops.rs`: primitive text edits, newline indentation, backspace/delete,
  and shared undo snapshot helpers.
- `editor/line_ops.rs`: indentation, duplicate/delete/move/join/sort/reverse
  line operations and line-range helpers.
- `editor/motion.rs`: cursor and selection movement.
- `editor/replace.rs`: replace and replace-all operations.
- `editor/tests.rs`: editor behavior tests.

Prefer keeping new editor behavior in the smallest matching submodule. Avoid
adding new behavior directly to `editor.rs` unless it is part of rendering or the
public module surface.

## Command Flow

App-level commands should go through the `AppCommand` dispatcher in `app.rs`.
Keyboard shortcuts, native menu actions, and toolbar controls should share this
path so metadata refresh, focus updates, and session snapshots are not forgotten.

Editor-local input is still handled in `editor/input.rs` because it depends on
egui text events and editor view state such as preferred column. Future command
palette work should introduce typed command metadata before adding another
parallel shortcut path.

## Search Flow

Search state and navigation behavior live in `search.rs`. `app.rs` renders the
search bar and calls into `SearchState` for match state, current result labels,
global/local match navigation, and occurrence selection.

Literal search uses bounded rope windows with overlap. Regex search is currently
correct but materializes the full rope. That is acceptable as a containment point
for now, but large-buffer work should replace it with a bounded or streaming
strategy before treating regex search as performance-safe.

## Edit Transactions

The model now has a narrow transaction API around `DocumentEdit`:

- `Document::apply_edit` applies one range replacement and records one undo
  transaction.
- `Document::apply_continuing_edit` applies one range replacement into the
  current typing group.
- `Document::apply_grouped_edit` applies one range replacement as its own undo
  group.
- `Document::record_full_document_replacement` records undo for operations that
  still mutate the rope manually and snapshot the whole document.

Use these helpers for new single-range edits. Avoid adding new editor code that
manually updates rope, selection, revision, and undo state when a `DocumentEdit`
can express the change.

The current transaction API is intentionally not a full multi-cursor system.
Several line operations still perform manual multi-step rope edits and then
record full-document undo snapshots. Before implementing multiple cursors,
replace-all transactions, or rectangular selection edits, extend the model with a
multi-edit transaction type that can apply non-overlapping ranges in reverse
order and record one grouped undo step.

## Near-Term Direction

The next cleanup should move remaining direct editor mutations toward explicit
transactions:

- Add a multi-edit transaction type for ordered, non-overlapping range edits.
- Migrate replace-all, indentation, outdent, line move, duplicate-line, and
  delete-line operations away from full-document snapshots where practical.
- Keep whole-document snapshots only for operations whose natural implementation
  is truly whole-buffer.
- Move editor tests closer to their target modules once the transaction
  boundaries settle.

These changes should preserve the product invariant that routine editing never
blocks on persistence and never asks the user to save.
