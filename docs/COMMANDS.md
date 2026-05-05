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

### `AppCommand` (src/app.rs:34-43)

A smaller enum for app-level commands that require state changes outside the
editor (tab management, bookmarks, undo/redo routing). These are handled by
`execute_command()` and `handle_command()` in `app.rs`.

### `NativeMenuCommand` (src/native_menu.rs:2-8)

A subset of commands available in the native menu bar. Converted to `AppCommand`
via a `From` impl for dispatch.

## Command Metadata and Shortcuts

Each command carries metadata via `CommandMetadata` (src/command.rs:108-114):

```rust
pub struct CommandMetadata {
    pub command: Command,
    pub name: &'static str,
    pub description: &'static str,
    pub category: CommandCategory,
    pub shortcut: Option<egui::KeyboardShortcut>,
}
```

The `all_commands()` function (src/command.rs:148-769) returns the full list
with shortcuts defined using `egui::KeyboardShortcut`.

### Shortcut Modifiers

| Modifier | egui Constant |
|----------|---------------|
| Command (⌘/Ctrl) | `Modifiers::COMMAND` |
| Shift | `Modifiers::SHIFT` |
| Alt/Option | `Modifiers::ALT` |
| Ctrl (explicit) | `Modifiers::CTRL` |

### Shortcut Registration Locations

Shortcuts are currently defined in two places:

1. **`command.rs`** - `all_commands()` for palette display and metadata
2. **`app.rs`** - `handle_keyboard_shortcuts()` (lines 760-969) for actual handling

Additionally, **`editor/input.rs`** (lines 26-331) handles editor-local keyboard
input directly (typing, arrows, delete, etc.) without going through the command
system.

### Commands Without Default Shortcuts

Some commands are available in the palette but have no default keyboard shortcut:
`ToggleWrapMode`, `ToggleVisibleWhitespace`, `ToggleIndentGuides`,
`ToggleMinimap`, `ToggleTheme`, `SearchInTabs`, `DuplicateLines`,
`NormalizeWhitespace`.

## Command Palette (src/command_palette.rs)

The command palette provides fuzzy-search access to all commands:

- `CommandPalette::show()` renders the palette UI with filtered results
- `fuzzy_match()` in `command.rs` provides matching against command names and descriptions
- Selecting a command executes it via a callback to `handle_command()` in `app.rs`

Activation: `Cmd+Shift+P` (or via `CommandPalette` command).

## Dispatch Flow

```
User input
├── editor/input.rs (typing, arrows, editor-local keys)
├── app.rs:handle_keyboard_shortcuts() (app-level shortcuts)
└── Command palette selection
    └── app.rs:handle_command()
        ├── AppCommand → execute_command()
        └── Editor commands → forwarded to active editor
```

## Keybinding Conventions

- **Cmd (⌘)** is the primary modifier for actions (new, close, save-free actions)
- **Cmd+Shift** is used for reverse or extended versions (Redo, Add All Matches)
- **Cmd+Alt** is used for find/replace and split pane actions
- **Alt** alone is used for word movement and selection expansion
- **F-keys** are used for tab operations (F2 rename, F3 find under cursor, F4 bookmarks)
- **Cmd+P** opens quick tab switcher (distinct from command palette at Cmd+Shift+P)

## Future Work

- Unify shortcut registration into a single source of truth instead of duplicating
  between `command.rs` and `app.rs`
- Add user-configurable keybinding overrides (see ROADMAP.md: "Add keybinding configuration")
- Consider adding a shortcut conflict detection system
