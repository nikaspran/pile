# Command Model and Keybinding Conventions

This document describes the command architecture in `pile`, how commands are
defined, dispatched, and bound to keyboard shortcuts.

## Command Enums

The application uses three command enums, each scoped to a different layer:

### `Command` (src/command.rs)

The primary, comprehensive command enum. Every user-facing action is represented
here. This enum powers the command palette and carries metadata for display and
shortcut registration.

Categories (defined in `CommandCategory`):
- **App** - session-level actions (NewScratch, CloseScratch, Undo, Redo)
- **Motion** - cursor movement (MoveLeft, MoveWordRight, MoveDocumentStart, etc.)
- **Selection** - movement with selection (SelectLeft, SelectWordRight, etc.)
- **SelectionExpansion** - scope-based expansion (ExpandWord, ExpandBracketPair, etc.)
- **LineOperations** - line-level edits (Indent, DuplicateLines, SortLines, etc.)
- **MultiCursor** - multiple cursor management (AddNextMatch, SplitSelectionIntoLines, etc.)
- **Editing** - content transforms (ToggleComments, UpperCase, LowerCase, TitleCase)
- **Search** - search UI and navigation (Find, FindReplace, FindUnderCursor, etc.)
- **View** - display toggles (CommandPalette, ToggleWrapMode, ToggleMinimap, etc.)

### `AppCommand` (src/app/commands.rs)

A smaller enum for app-level commands that require state changes outside the
editor (tab management, bookmarks, undo/redo routing). These are converted from
native menu commands and, where behavior is identical, from command
palette/shortcut commands before dispatch through `execute_command()` in
`app.rs`.

### `NativeMenuCommand` (src/native_menu.rs:2-8)

A subset of commands available in the native menu bar. Converted to `AppCommand`
via a `From` impl for dispatch.

## Command Metadata and Shortcuts

Each command carries metadata via `CommandMetadata`:

```rust
pub struct CommandMetadata {
    pub command: Command,
    pub name: &'static str,
    pub description: &'static str,
    pub category: CommandCategory,
    pub shortcut: Option<egui::KeyboardShortcut>,
}
```

The `all_commands()` function returns the full list with shortcuts defined
using `egui::KeyboardShortcut`.

### Shortcut Modifiers

| Modifier | egui Constant |
|----------|---------------|
| Command (⌘/Ctrl) | `Modifiers::COMMAND` |
| Shift | `Modifiers::SHIFT` |
| Alt/Option | `Modifiers::ALT` |
| Ctrl (explicit) | `Modifiers::CTRL` |

### Shortcut Registration

Default keybindings are registered in **`command.rs`** via
`default_shortcuts()`. Raw key events are resolved through
`command_for_key_event()` against a command list for the active layer:

- **`KEYBOARD_COMMANDS`** - app-level shortcuts consumed by
  `app.rs:handle_keyboard_shortcuts()`
- **`EDITOR_KEY_COMMANDS`** - editor-local shortcuts consumed by
  `editor/input.rs`

Text and paste events remain data-bearing input events in `editor/input.rs`;
key-only actions such as movement, selection, delete/backspace, newline, line
operations, and case conversion resolve through the command shortcut table.

### Commands Without Default Shortcuts

Some commands are available in the palette but have no default keyboard shortcut:
`ToggleWrapMode`, `ToggleVisibleWhitespace`, `ToggleIndentGuides`,
`ToggleMinimap`, `ToggleTheme`, `SearchInTabs`, `NormalizeWhitespace`.

## Command Palette (src/command_palette.rs)

The command palette provides fuzzy-search access to all commands:

- `CommandPalette::show()` renders the palette UI with filtered results
- `fuzzy_match()` in `command.rs` provides matching against command names and descriptions
- Selecting a command executes it via a callback to `handle_command()` in `app.rs`

Activation: `Cmd+Shift+P` (or via `CommandPalette` command).

## Dispatch Flow

```
User input
├── editor/input.rs
│   ├── text/paste events
│   └── command.rs:command_for_key_event(..., EDITOR_KEY_COMMANDS)
├── app.rs:handle_keyboard_shortcuts() (app-level shortcuts)
│   └── command.rs:default_shortcuts() filtered by KEYBOARD_COMMANDS
├── native menu command
└── command palette selection
    └── app.rs:handle_command()
        ├── AppCommand::from_command(...) → execute_command()
        └── command-specific app/search/view handling
```

## Keybinding Conventions

- **Cmd (⌘)** is the primary modifier for actions (new, close, save-free actions)
- **Cmd+Shift** is used for reverse or extended versions (Redo, Add All Matches)
- **Cmd+Alt** is used for find/replace and split pane actions
- **Alt/Option** is used for word movement, word deletion, and selection expansion
- **Ctrl** variants are available for common non-macOS word deletion defaults
- **F-keys** are used for tab operations (F2 rename, F3 find under cursor, F4 bookmarks)
- **Cmd+P** opens quick tab switcher (distinct from command palette at Cmd+Shift+P)

## Future Work

- Add user-configurable keybinding overrides by replacing or layering on top of
  `default_shortcuts()` before dispatch
- Consider adding a shortcut conflict detection system
