# pile

`pile` is a minimalist, native scratchpad editor for keeping hundreds of unsaved
notes alive without ceremony.

The core promise is an invisible hot exit: users should be able to paste text,
close the app, lose power, restart, and find their working set restored without
ever being asked to save. The editor is not an IDE. It avoids project trees,
terminals, LSP integration, and workspace management in favor of fast text
capture, reliable restoration, and low-friction navigation across many buffers.

## Design Principles

- Keep the UI thread responsive and deterministic.
- Store document text in a rope, not a contiguous `String`.
- Treat persistence as automatic infrastructure, not a user workflow.
- Prefer content-aware behavior over filename or extension assumptions.
- Add only editor essentials: tabs, search, replace, multiple cursors, line
  numbers, and a minimap.
- Keep the architecture ready for hundreds of active scratch buffers.

## Current Scaffold

This first slice establishes the native egui app, rope-backed documents, a
debounced background session saver, content-detection scaffolding, and shared
agent documentation. The editing surface is intentionally plain for now; richer
editor behaviors should build on the same state and persistence boundaries
rather than replacing them.
