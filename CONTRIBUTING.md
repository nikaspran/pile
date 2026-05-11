# Contributing to pile

Thanks for helping improve `pile`. The project is intentionally narrow: it is a
native scratchpad editor with automatic session restoration, not an IDE.

## Product Boundaries

Good contributions usually improve:

- reliability of hot-exit session restoration;
- editor latency and large-buffer behavior;
- scratch-buffer navigation across many tabs;
- search, replace, highlighting, and text transformations;
- platform packaging and native integration.

Avoid adding LSP support, project trees, integrated terminals, debuggers,
workspace management, plugin systems, or save prompts.

## Local Setup

Install Rust 1.88 or newer. On Linux, install the GUI development packages
required by `eframe`; `.github/workflows/build.yml` lists known-good packages
for several distributions.

```sh
cargo build --locked
cargo run --locked
```

## Required Checks

Run these before opening a pull request:

```sh
cargo fmt --check
cargo clippy --locked --all-targets
cargo test --locked
```

For performance-sensitive changes, also run:

```sh
cargo bench
```

Clippy currently runs as a non-deny check. New code should avoid adding new
warnings; tightening Clippy to `-D warnings` is tracked as cleanup work.

## Architecture Notes

Read these before changing core behavior:

- `docs/ARCHITECTURE.md`
- `docs/PERFORMANCE_INVARIANTS.md`
- `docs/NON_GOALS.md`
- `AGENTS.md`

Important invariants:

- The UI thread owns live app state.
- Document text is canonically stored as `crop::Rope`.
- Routine persistence runs in the background from immutable snapshots.
- Rendering and editing paths should avoid full-document string materialization.
- Tests should cover model, persistence, language detection, and text
  transformations where practical.

## Pull Requests

Keep pull requests focused. Include:

- a short summary of user-visible behavior or internal cleanup;
- tests run;
- screenshots or recordings for visible UI changes;
- notes about persistence/session compatibility when relevant.

If a change can affect data recovery, session migration, or large-buffer
performance, call that out explicitly.
