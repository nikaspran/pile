# pile

`pile` is a native scratchpad editor for keeping many unsaved notes alive
without ceremony. It is built for fast capture, low-latency editing, and
automatic restoration after normal exits, crashes, restarts, and power loss.

The app is intentionally not an IDE. It has no project tree, LSP integration,
terminal, debugger, plugin system, workspace setup, or manual save prompt. The
main workflow is: write text, close the app whenever, and come back to the same
scratch buffers later.

## Features

- Rope-backed editor buffers for large scratch documents.
- Automatic background session persistence with atomic replacement and backup
  recovery.
- Tabs, quick tab switching, recent tab ordering, pinned tabs, and split panes.
- Multiple cursors, column selection, selection expansion, and common line
  operations.
- Search and replace with case-sensitive, whole-word, regex, and search-in-tabs
  modes.
- Content-aware syntax detection and tree-sitter highlighting with Markdown code
  fence injection.
- Configurable wrapping, rulers, visible whitespace, indentation guides,
  minimap, font settings, themes, and status bar.
- Native menus, clipboard integration, drag-and-drop import, and explicit
  file import/export for interop.

## Install

Prebuilt release artifacts are published from GitHub Actions when version tags
are pushed:

- `pile-VERSION-x86_64-apple-darwin-macos.zip`
- `pile-VERSION-aarch64-apple-darwin-macos.zip`
- `pile-VERSION-x86_64-pc-windows-msvc-windows.zip`
- `pile-VERSION-x86_64-unknown-linux-gnu-linux.tar.gz`
- `pile_VERSION_amd64.deb`

Download the artifact for your platform from the GitHub Releases page. macOS
downloads contain `pile.app`; Windows downloads contain `pile.exe`; Linux
downloads contain an installable `/usr`-style tree or a Debian package.

```sh
tar -xzf pile-0.1.0-x86_64-unknown-linux-gnu-linux.tar.gz
./pile-0.1.0-x86_64-unknown-linux-gnu-linux/bin/pile
```

Release assets include `SHA256SUMS` and `pile-update-manifest.json` for
download verification and future update checks.

## Build From Source

Requirements:

- Rust 1.88 or newer.
- Platform GUI dependencies required by `eframe`/`egui`.
- On Linux, install X11/Wayland/GTK development packages. The GitHub Actions
  workflow shows the exact packages used for Ubuntu, Debian, Fedora, and Arch.

```sh
git clone https://github.com/nikaspran/pile.git
cd pile
cargo build --locked --release
./target/release/pile
```

For development:

```sh
cargo fmt --check
cargo clippy --locked --all-targets
cargo test --locked
```

## CLI Retrieval

Running `pile` without arguments starts the native app. Read-only CLI commands
are available for tools and agents that need to inspect the last persisted
session snapshot:

```sh
pile list
pile search "query"
pile get <document-id>
```

CLI output defaults to JSON for machine use. Pass `--format human` for compact
terminal output. Each command reads the platform default session file unless
`--session <path>` is provided.

Useful options:

- `pile list --closed` includes recently closed scratch buffers.
- `pile search "query" --closed --case-sensitive --whole-word --regex`
  searches open buffers and, when requested, closed buffers.
- `pile search "query" --limit 20 --context 120` bounds returned matches and
  surrounding context.
- `pile get <document-id> --closed --lines 10:25` retrieves a 1-based inclusive
  line range.

## Data and Recovery

`pile` stores scratch buffers in an automatic session file under the
platform-specific local data directory selected by the `directories` crate. The
session is serialized with `bincode`, wrapped in a versioned envelope, and
written with atomic file replacement. Recent session backups are rotated next to
the main session file and are used for recovery if the main session is corrupt.

Settings are stored separately as JSON in the same application data area.

Not saved:

- Undo/redo history.
- Clipboard contents.
- Transient UI state such as command palette visibility.

`pile` does not intentionally collect telemetry or send usage data.

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Commands and shortcuts](docs/COMMANDS.md)
- [Language detection](docs/LANGUAGE_DETECTION.md)
- [Performance invariants](docs/PERFORMANCE_INVARIANTS.md)
- [Non-goals](docs/NON_GOALS.md)
- [Releasing](docs/RELEASING.md)
- [Updates](docs/UPDATES.md)
- [Roadmap](ROADMAP.md)

## Contributing

Contributions are welcome when they preserve the core product boundary: a fast,
reliable scratchpad, not an IDE. Start with [CONTRIBUTING.md](CONTRIBUTING.md)
for setup, tests, and pull request expectations.

## License

`pile` is licensed under the MIT License. See [LICENSE](LICENSE).
