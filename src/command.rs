use eframe::egui;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Command {
    // App commands
    NewScratch,
    CloseScratch,
    RenameTab,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Undo,
    Redo,

    // Editor - motion
    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    MoveUp,
    MoveDown,
    MoveDocumentStart,
    MoveDocumentEnd,
    MoveLineStart,
    MoveLineEnd,
    MoveParagraphUp,
    MoveParagraphDown,
    PageUp,
    PageDown,

    // Editor - selection
    SelectLeft,
    SelectRight,
    SelectWordLeft,
    SelectWordRight,
    SelectUp,
    SelectDown,
    #[allow(dead_code)]
    SelectDocumentStart,
    #[allow(dead_code)]
    SelectDocumentEnd,
    #[allow(dead_code)]
    SelectLineStart,
    #[allow(dead_code)]
    SelectLineEnd,
    #[allow(dead_code)]
    SelectParagraphUp,
    #[allow(dead_code)]
    SelectParagraphDown,
    #[allow(dead_code)]
    SelectPageUp,
    #[allow(dead_code)]
    SelectPageDown,

    // Editor - selection expansion
    ExpandWord,
    ContractWord,
    ExpandLine,
    ContractLine,
    ExpandBracketPair,
    ContractBracketPair,
    ExpandIndentBlock,
    ContractIndentBlock,

    // Editor - line operations
    Indent,
    Outdent,
    DuplicateLines,
    DeleteLines,
    MoveLinesUp,
    MoveLinesDown,
    JoinLines,
    SortLines,
    ReverseLines,
    TrimTrailingWhitespace,
    NormalizeWhitespace,

    // Editor - multi-cursor
    AddNextMatch,
    AddAllMatches,
    SplitSelectionIntoLines,
    ClearSecondaryCursors,

    // Editor - editing
    Backspace,
    DeleteForward,
    BackspaceWord,
    DeleteWordForward,
    InsertNewline,
    ToggleComments,
    UpperCase,
    LowerCase,
    TitleCase,

    // Search
    Find,
    FindReplace,
    FindUnderCursor,
    SelectNextOccurrence,
    SearchInTabs,
    GoToLine,

    // View
    CommandPalette,
    QuickSwitchTabs,
    ToggleWrapMode,
    ToggleVisibleWhitespace,
    ToggleIndentGuides,
    ToggleMinimap,
    ToggleStatusBar,
    ToggleTheme,

    // File
    ImportFile,
    ExportFile,
    Preferences,

    // Bookmarks
    ToggleBookmark,
    JumpToNextBookmark,
    ClearBookmarks,

    // Window / tabs
    SplitPaneHorizontal,
    SplitPaneVertical,
    ClosePane,
    PinTab,
    MoveTabLeft,
    MoveTabRight,
    ReopenLastClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandCategory {
    App,
    Motion,
    Selection,
    SelectionExpansion,
    LineOperations,
    MultiCursor,
    Editing,
    Search,
    View,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ShortcutBinding {
    pub command: Command,
    pub shortcut: egui::KeyboardShortcut,
}

pub fn default_shortcuts() -> Vec<ShortcutBinding> {
    use Command::*;
    use egui::{Key, KeyboardShortcut, Modifiers};

    let binding = |command, modifiers, logical_key| ShortcutBinding {
        command,
        shortcut: KeyboardShortcut {
            modifiers,
            logical_key,
        },
    };

    vec![
        binding(NewScratch, Modifiers::COMMAND, Key::N),
        binding(CloseScratch, Modifiers::COMMAND, Key::W),
        binding(RenameTab, Modifiers::NONE, Key::F2),
        binding(Cut, Modifiers::COMMAND, Key::X),
        binding(Copy, Modifiers::COMMAND, Key::C),
        binding(Paste, Modifiers::COMMAND, Key::V),
        binding(SelectAll, Modifiers::COMMAND, Key::A),
        binding(Undo, Modifiers::COMMAND, Key::Z),
        binding(Redo, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z),
        binding(MoveLeft, Modifiers::NONE, Key::ArrowLeft),
        binding(MoveRight, Modifiers::NONE, Key::ArrowRight),
        binding(MoveWordLeft, Modifiers::ALT, Key::ArrowLeft),
        binding(MoveWordRight, Modifiers::ALT, Key::ArrowRight),
        binding(MoveWordLeft, Modifiers::CTRL, Key::ArrowLeft),
        binding(MoveWordRight, Modifiers::CTRL, Key::ArrowRight),
        binding(MoveLineStart, Modifiers::COMMAND, Key::ArrowLeft),
        binding(MoveLineEnd, Modifiers::COMMAND, Key::ArrowRight),
        binding(MoveUp, Modifiers::NONE, Key::ArrowUp),
        binding(MoveDown, Modifiers::NONE, Key::ArrowDown),
        binding(MoveDocumentStart, Modifiers::CTRL, Key::Home),
        binding(MoveDocumentEnd, Modifiers::CTRL, Key::End),
        binding(MoveDocumentStart, Modifiers::COMMAND, Key::ArrowUp),
        binding(MoveDocumentEnd, Modifiers::COMMAND, Key::ArrowDown),
        binding(MoveLineStart, Modifiers::NONE, Key::Home),
        binding(MoveLineEnd, Modifiers::NONE, Key::End),
        binding(PageUp, Modifiers::NONE, Key::PageUp),
        binding(PageDown, Modifiers::NONE, Key::PageDown),
        binding(SelectLeft, Modifiers::SHIFT, Key::ArrowLeft),
        binding(SelectRight, Modifiers::SHIFT, Key::ArrowRight),
        binding(
            SelectWordLeft,
            Modifiers::SHIFT | Modifiers::ALT,
            Key::ArrowLeft,
        ),
        binding(
            SelectWordRight,
            Modifiers::SHIFT | Modifiers::ALT,
            Key::ArrowRight,
        ),
        binding(
            SelectWordLeft,
            Modifiers::SHIFT | Modifiers::CTRL,
            Key::ArrowLeft,
        ),
        binding(
            SelectWordRight,
            Modifiers::SHIFT | Modifiers::CTRL,
            Key::ArrowRight,
        ),
        binding(
            SelectLineStart,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::ArrowLeft,
        ),
        binding(
            SelectLineEnd,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::ArrowRight,
        ),
        binding(SelectUp, Modifiers::SHIFT, Key::ArrowUp),
        binding(SelectDown, Modifiers::SHIFT, Key::ArrowDown),
        binding(
            SelectDocumentStart,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::ArrowUp,
        ),
        binding(
            SelectDocumentEnd,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::ArrowDown,
        ),
        binding(SelectLineStart, Modifiers::SHIFT, Key::Home),
        binding(SelectLineEnd, Modifiers::SHIFT, Key::End),
        binding(SelectPageUp, Modifiers::SHIFT, Key::PageUp),
        binding(SelectPageDown, Modifiers::SHIFT, Key::PageDown),
        binding(ExpandWord, Modifiers::ALT, Key::W),
        binding(ContractWord, Modifiers::SHIFT | Modifiers::ALT, Key::W),
        binding(ExpandLine, Modifiers::ALT, Key::L),
        binding(ContractLine, Modifiers::SHIFT | Modifiers::ALT, Key::L),
        binding(ExpandBracketPair, Modifiers::ALT, Key::B),
        binding(
            ContractBracketPair,
            Modifiers::SHIFT | Modifiers::ALT,
            Key::B,
        ),
        binding(ExpandIndentBlock, Modifiers::ALT, Key::I),
        binding(
            ContractIndentBlock,
            Modifiers::SHIFT | Modifiers::ALT,
            Key::I,
        ),
        binding(Indent, Modifiers::NONE, Key::Tab),
        binding(Outdent, Modifiers::SHIFT, Key::Tab),
        binding(
            DuplicateLines,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::D,
        ),
        binding(
            DuplicateLines,
            Modifiers::SHIFT | Modifiers::ALT,
            Key::ArrowDown,
        ),
        binding(DeleteLines, Modifiers::COMMAND | Modifiers::SHIFT, Key::K),
        binding(MoveLinesUp, Modifiers::ALT, Key::ArrowUp),
        binding(MoveLinesDown, Modifiers::ALT, Key::ArrowDown),
        binding(JoinLines, Modifiers::COMMAND, Key::J),
        binding(SortLines, Modifiers::COMMAND | Modifiers::SHIFT, Key::S),
        binding(ReverseLines, Modifiers::COMMAND | Modifiers::SHIFT, Key::R),
        binding(
            TrimTrailingWhitespace,
            Modifiers::COMMAND | Modifiers::ALT,
            Key::T,
        ),
        binding(AddNextMatch, Modifiers::COMMAND, Key::D),
        binding(AddAllMatches, Modifiers::COMMAND | Modifiers::SHIFT, Key::L),
        binding(
            SplitSelectionIntoLines,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::L,
        ),
        binding(ClearSecondaryCursors, Modifiers::NONE, Key::Escape),
        binding(Backspace, Modifiers::NONE, Key::Backspace),
        binding(DeleteForward, Modifiers::NONE, Key::Delete),
        binding(BackspaceWord, Modifiers::ALT, Key::Backspace),
        binding(DeleteWordForward, Modifiers::ALT, Key::Delete),
        binding(BackspaceWord, Modifiers::CTRL, Key::Backspace),
        binding(DeleteWordForward, Modifiers::CTRL, Key::Delete),
        binding(InsertNewline, Modifiers::NONE, Key::Enter),
        binding(ToggleComments, Modifiers::COMMAND, Key::Slash),
        binding(UpperCase, Modifiers::COMMAND | Modifiers::CTRL, Key::U),
        binding(LowerCase, Modifiers::COMMAND | Modifiers::CTRL, Key::L),
        binding(TitleCase, Modifiers::COMMAND | Modifiers::CTRL, Key::T),
        binding(Find, Modifiers::COMMAND, Key::F),
        binding(FindReplace, Modifiers::COMMAND | Modifiers::ALT, Key::F),
        binding(FindUnderCursor, Modifiers::NONE, Key::F3),
        binding(GoToLine, Modifiers::COMMAND, Key::G),
        binding(
            CommandPalette,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::P,
        ),
        binding(QuickSwitchTabs, Modifiers::COMMAND, Key::P),
        binding(Preferences, Modifiers::COMMAND, Key::Comma),
        binding(ImportFile, Modifiers::COMMAND | Modifiers::SHIFT, Key::I),
        binding(ExportFile, Modifiers::COMMAND | Modifiers::SHIFT, Key::E),
        binding(ToggleBookmark, Modifiers::COMMAND, Key::F2),
        binding(JumpToNextBookmark, Modifiers::NONE, Key::F4),
        binding(
            ClearBookmarks,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::F2,
        ),
        binding(
            SplitPaneHorizontal,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::H,
        ),
        binding(
            SplitPaneVertical,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::V,
        ),
        binding(ClosePane, Modifiers::COMMAND | Modifiers::SHIFT, Key::W),
        binding(PinTab, Modifiers::ALT | Modifiers::SHIFT, Key::P),
        binding(
            ReopenLastClosed,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::T,
        ),
        binding(
            MoveTabLeft,
            Modifiers::COMMAND | Modifiers::ALT,
            Key::ArrowLeft,
        ),
        binding(
            MoveTabRight,
            Modifiers::COMMAND | Modifiers::ALT,
            Key::ArrowRight,
        ),
    ]
}

/// Commands checked by the app-level keyboard shortcut handler.
/// This is the single source of truth for which `default_shortcuts` bindings
/// are dispatched via `handle_command` (as opposed to editor-only commands
/// handled by `editor/input.rs`).
pub const KEYBOARD_COMMANDS: &[Command] = &[
    Command::NewScratch,
    Command::CloseScratch,
    Command::RenameTab,
    Command::Undo,
    Command::Redo,
    Command::SelectAll,
    Command::Find,
    Command::FindReplace,
    Command::FindUnderCursor,
    Command::DuplicateLines,
    Command::MoveLinesUp,
    Command::MoveLinesDown,
    Command::MoveTabLeft,
    Command::MoveTabRight,
    Command::PinTab,
    Command::SplitPaneHorizontal,
    Command::SplitPaneVertical,
    Command::ClosePane,
    Command::ImportFile,
    Command::ExportFile,
    Command::GoToLine,
    Command::CommandPalette,
    Command::QuickSwitchTabs,
    Command::ToggleBookmark,
    Command::JumpToNextBookmark,
    Command::ClearBookmarks,
    Command::ToggleWrapMode,
    Command::ToggleVisibleWhitespace,
    Command::ToggleIndentGuides,
    Command::ToggleMinimap,
    Command::ToggleStatusBar,
    Command::ToggleTheme,
    Command::SearchInTabs,
    Command::Preferences,
    Command::ReopenLastClosed,
];

/// Commands that are dispatched inside the custom editor widget from key events.
///
/// Text and paste events remain outside this list because they carry input data
/// rather than just a shortcut shape.
pub const EDITOR_KEY_COMMANDS: &[Command] = &[
    Command::Undo,
    Command::Redo,
    Command::MoveLeft,
    Command::MoveRight,
    Command::MoveWordLeft,
    Command::MoveWordRight,
    Command::MoveUp,
    Command::MoveDown,
    Command::MoveDocumentStart,
    Command::MoveDocumentEnd,
    Command::MoveLineStart,
    Command::MoveLineEnd,
    Command::MoveParagraphUp,
    Command::MoveParagraphDown,
    Command::PageUp,
    Command::PageDown,
    Command::SelectLeft,
    Command::SelectRight,
    Command::SelectWordLeft,
    Command::SelectWordRight,
    Command::SelectUp,
    Command::SelectDown,
    Command::SelectDocumentStart,
    Command::SelectDocumentEnd,
    Command::SelectLineStart,
    Command::SelectLineEnd,
    Command::SelectParagraphUp,
    Command::SelectParagraphDown,
    Command::SelectPageUp,
    Command::SelectPageDown,
    Command::ExpandWord,
    Command::ContractWord,
    Command::ExpandLine,
    Command::ContractLine,
    Command::ExpandBracketPair,
    Command::ContractBracketPair,
    Command::ExpandIndentBlock,
    Command::ContractIndentBlock,
    Command::Indent,
    Command::Outdent,
    Command::DuplicateLines,
    Command::DeleteLines,
    Command::MoveLinesUp,
    Command::MoveLinesDown,
    Command::JoinLines,
    Command::SortLines,
    Command::ReverseLines,
    Command::TrimTrailingWhitespace,
    Command::AddNextMatch,
    Command::AddAllMatches,
    Command::SplitSelectionIntoLines,
    Command::ClearSecondaryCursors,
    Command::Backspace,
    Command::DeleteForward,
    Command::BackspaceWord,
    Command::DeleteWordForward,
    Command::InsertNewline,
    Command::ToggleComments,
    Command::UpperCase,
    Command::LowerCase,
    Command::TitleCase,
];

pub fn command_for_key_event(
    key: egui::Key,
    modifiers: egui::Modifiers,
    candidates: &[Command],
) -> Option<Command> {
    default_shortcuts()
        .into_iter()
        .find(|binding| {
            candidates.contains(&binding.command)
                && binding.shortcut.logical_key == key
                && modifiers.matches_exact(binding.shortcut.modifiers)
        })
        .map(|binding| binding.command)
}

fn primary_shortcut(command: Command) -> Option<egui::KeyboardShortcut> {
    default_shortcuts()
        .into_iter()
        .find(|binding| binding.command == command)
        .map(|binding| binding.shortcut)
}

pub struct CommandMetadata {
    pub command: Command,
    pub name: &'static str,
    pub description: &'static str,
    #[allow(dead_code)]
    pub category: CommandCategory,
    #[allow(dead_code)]
    pub shortcut: Option<egui::KeyboardShortcut>,
}

impl CommandMetadata {
    pub fn matches_query(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query = query.to_lowercase();
        self.name.to_lowercase().contains(&query)
            || self.description.to_lowercase().contains(&query)
            || format!("{:?}", self.command)
                .to_lowercase()
                .contains(&query)
    }
}

/// Format a keyboard shortcut using per-platform conventions.
///
/// On macOS: uses symbols (⌘, ⇧, ⌥, ⌃) and displays keys like "N", "F1".
/// On other platforms: uses words (Ctrl, Shift, Alt) and displays keys like "N", "F1".
pub fn format_shortcut(shortcut: &egui::KeyboardShortcut, _ctx: &egui::Context) -> String {
    let modifiers = shortcut.modifiers;
    let mut parts = Vec::new();

    if cfg!(target_os = "macos") {
        // macOS: use symbols
        if modifiers.command {
            parts.push("⌘");
        }
        if modifiers.shift {
            parts.push("⇧");
        }
        if modifiers.alt {
            parts.push("⌥");
        }
        if modifiers.ctrl {
            parts.push("⌃");
        }
    } else {
        // Windows/Linux: use words
        if modifiers.command {
            parts.push("Ctrl");
        }
        if modifiers.shift {
            parts.push("Shift");
        }
        if modifiers.alt {
            parts.push("Alt");
        }
        if modifiers.ctrl {
            parts.push("Ctrl");
        }
    }

    // Format the key
    let key_text = format_key(&shortcut.logical_key);
    parts.push(&key_text);

    parts.join(if cfg!(target_os = "macos") { "" } else { "+" })
}

/// Format an egui Key into a human-readable string.
fn format_key(key: &egui::Key) -> String {
    match key {
        egui::Key::ArrowDown => "↓".to_string(),
        egui::Key::ArrowUp => "↑".to_string(),
        egui::Key::ArrowLeft => "←".to_string(),
        egui::Key::ArrowRight => "→".to_string(),
        egui::Key::Escape => "Esc".to_string(),
        egui::Key::Tab => "Tab".to_string(),
        egui::Key::Space => "Space".to_string(),
        egui::Key::Enter => "Enter".to_string(),
        egui::Key::Backspace => "Backspace".to_string(),
        egui::Key::Delete => "Delete".to_string(),
        egui::Key::Home => "Home".to_string(),
        egui::Key::End => "End".to_string(),
        egui::Key::PageUp => "PgUp".to_string(),
        egui::Key::PageDown => "PgDown".to_string(),
        egui::Key::F1 => "F1".to_string(),
        egui::Key::F2 => "F2".to_string(),
        egui::Key::F3 => "F3".to_string(),
        egui::Key::F4 => "F4".to_string(),
        egui::Key::F5 => "F5".to_string(),
        egui::Key::F6 => "F6".to_string(),
        egui::Key::F7 => "F7".to_string(),
        egui::Key::F8 => "F8".to_string(),
        egui::Key::F9 => "F9".to_string(),
        egui::Key::F10 => "F10".to_string(),
        egui::Key::F11 => "F11".to_string(),
        egui::Key::F12 => "F12".to_string(),
        egui::Key::Insert => "Insert".to_string(),
        egui::Key::Num0 => "0".to_string(),
        egui::Key::Num1 => "1".to_string(),
        egui::Key::Num2 => "2".to_string(),
        egui::Key::Num3 => "3".to_string(),
        egui::Key::Num4 => "4".to_string(),
        egui::Key::Num5 => "5".to_string(),
        egui::Key::Num6 => "6".to_string(),
        egui::Key::Num7 => "7".to_string(),
        egui::Key::Num8 => "8".to_string(),
        egui::Key::Num9 => "9".to_string(),
        _ => {
            // For letter keys, just use the debug output and clean it up
            let s = format!("{:?}", key);
            // Remove "Character(" prefix and ")" suffix if present
            if s.starts_with("Character(") && s.ends_with(')') {
                s[9..s.len() - 1].to_string()
            } else {
                s
            }
        }
    }
}

// Simple fuzzy match: characters of query must appear in order in target
pub fn fuzzy_match(query: &str, target: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    let target = target.to_lowercase();
    let mut target_chars = target.chars();
    for qc in query.chars() {
        loop {
            match target_chars.next() {
                Some(tc) if tc == qc => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

pub fn all_commands() -> Vec<CommandMetadata> {
    use Command::*;
    use CommandCategory::*;
    use egui::{Key, KeyboardShortcut, Modifiers};

    vec![
        // App commands
        CommandMetadata {
            command: NewScratch,
            name: "New Scratch",
            description: "Create a new scratch buffer",
            category: App,
            shortcut: primary_shortcut(NewScratch),
        },
        CommandMetadata {
            command: CloseScratch,
            name: "Close Scratch",
            description: "Close the current scratch buffer",
            category: App,
            shortcut: primary_shortcut(CloseScratch),
        },
        CommandMetadata {
            command: RenameTab,
            name: "Rename Tab",
            description: "Rename the current tab",
            category: App,
            shortcut: primary_shortcut(RenameTab),
        },
        CommandMetadata {
            command: Cut,
            name: "Cut",
            description: "Cut selected text to the clipboard",
            category: App,
            shortcut: primary_shortcut(Cut),
        },
        CommandMetadata {
            command: Copy,
            name: "Copy",
            description: "Copy selected text to the clipboard",
            category: App,
            shortcut: primary_shortcut(Copy),
        },
        CommandMetadata {
            command: Paste,
            name: "Paste",
            description: "Paste clipboard text into the current scratch",
            category: App,
            shortcut: primary_shortcut(Paste),
        },
        CommandMetadata {
            command: SelectAll,
            name: "Select All",
            description: "Select all text in the current scratch",
            category: App,
            shortcut: primary_shortcut(SelectAll),
        },
        CommandMetadata {
            command: Undo,
            name: "Undo",
            description: "Undo the last edit",
            category: App,
            shortcut: primary_shortcut(Undo),
        },
        CommandMetadata {
            command: Redo,
            name: "Redo",
            description: "Redo the last undone edit",
            category: App,
            shortcut: primary_shortcut(Redo),
        },
        // Motion commands
        CommandMetadata {
            command: MoveLeft,
            name: "Move Left",
            description: "Move cursor left by one character",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::ArrowLeft,
            }),
        },
        CommandMetadata {
            command: MoveRight,
            name: "Move Right",
            description: "Move cursor right by one character",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::ArrowRight,
            }),
        },
        CommandMetadata {
            command: MoveWordLeft,
            name: "Move Word Left",
            description: "Move cursor left by one word",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::ArrowLeft,
            }),
        },
        CommandMetadata {
            command: MoveWordRight,
            name: "Move Word Right",
            description: "Move cursor right by one word",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::ArrowRight,
            }),
        },
        CommandMetadata {
            command: MoveUp,
            name: "Move Up",
            description: "Move cursor up by one line",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::ArrowUp,
            }),
        },
        CommandMetadata {
            command: MoveDown,
            name: "Move Down",
            description: "Move cursor down by one line",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::ArrowDown,
            }),
        },
        CommandMetadata {
            command: MoveDocumentStart,
            name: "Move to Document Start",
            description: "Move cursor to the start of the document",
            category: Motion,
            shortcut: primary_shortcut(MoveDocumentStart),
        },
        CommandMetadata {
            command: MoveDocumentEnd,
            name: "Move to Document End",
            description: "Move cursor to the end of the document",
            category: Motion,
            shortcut: primary_shortcut(MoveDocumentEnd),
        },
        CommandMetadata {
            command: MoveLineStart,
            name: "Move to Line Start",
            description: "Move cursor to the start of the current line",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::Home,
            }),
        },
        CommandMetadata {
            command: MoveLineEnd,
            name: "Move to Line End",
            description: "Move cursor to the end of the current line",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::End,
            }),
        },
        CommandMetadata {
            command: MoveParagraphUp,
            name: "Move Paragraph Up",
            description: "Move cursor up by one paragraph",
            category: Motion,
            shortcut: None,
        },
        CommandMetadata {
            command: MoveParagraphDown,
            name: "Move Paragraph Down",
            description: "Move cursor down by one paragraph",
            category: Motion,
            shortcut: None,
        },
        CommandMetadata {
            command: PageUp,
            name: "Page Up",
            description: "Move cursor up by one page",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::PageUp,
            }),
        },
        CommandMetadata {
            command: PageDown,
            name: "Page Down",
            description: "Move cursor down by one page",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::PageDown,
            }),
        },
        // Selection commands
        CommandMetadata {
            command: SelectLeft,
            name: "Select Left",
            description: "Extend selection left by one character",
            category: Selection,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT,
                logical_key: Key::ArrowLeft,
            }),
        },
        CommandMetadata {
            command: SelectRight,
            name: "Select Right",
            description: "Extend selection right by one character",
            category: Selection,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT,
                logical_key: Key::ArrowRight,
            }),
        },
        CommandMetadata {
            command: SelectWordLeft,
            name: "Select Word Left",
            description: "Extend selection left by one word",
            category: Selection,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT | Modifiers::ALT,
                logical_key: Key::ArrowLeft,
            }),
        },
        CommandMetadata {
            command: SelectWordRight,
            name: "Select Word Right",
            description: "Extend selection right by one word",
            category: Selection,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT | Modifiers::ALT,
                logical_key: Key::ArrowRight,
            }),
        },
        CommandMetadata {
            command: SelectUp,
            name: "Select Up",
            description: "Extend selection up by one line",
            category: Selection,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT,
                logical_key: Key::ArrowUp,
            }),
        },
        CommandMetadata {
            command: SelectDown,
            name: "Select Down",
            description: "Extend selection down by one line",
            category: Selection,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT,
                logical_key: Key::ArrowDown,
            }),
        },
        // Selection expansion
        CommandMetadata {
            command: ExpandWord,
            name: "Expand Selection by Word",
            description: "Expand selection to include the current word",
            category: SelectionExpansion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::W,
            }),
        },
        CommandMetadata {
            command: ContractWord,
            name: "Contract Selection by Word",
            description: "Contract selection by removing the current word",
            category: SelectionExpansion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT | Modifiers::ALT,
                logical_key: Key::W,
            }),
        },
        CommandMetadata {
            command: ExpandLine,
            name: "Expand Selection by Line",
            description: "Expand selection to include the current line",
            category: SelectionExpansion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::L,
            }),
        },
        CommandMetadata {
            command: ContractLine,
            name: "Contract Selection by Line",
            description: "Contract selection by removing the current line",
            category: SelectionExpansion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT | Modifiers::ALT,
                logical_key: Key::L,
            }),
        },
        CommandMetadata {
            command: ExpandBracketPair,
            name: "Expand Selection by Bracket Pair",
            description: "Expand selection to include matching brackets",
            category: SelectionExpansion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::B,
            }),
        },
        CommandMetadata {
            command: ContractBracketPair,
            name: "Contract Selection by Bracket Pair",
            description: "Contract selection by removing matching brackets",
            category: SelectionExpansion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT | Modifiers::ALT,
                logical_key: Key::B,
            }),
        },
        CommandMetadata {
            command: ExpandIndentBlock,
            name: "Expand Selection by Indent Block",
            description: "Expand selection to include the current indent block",
            category: SelectionExpansion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::I,
            }),
        },
        CommandMetadata {
            command: ContractIndentBlock,
            name: "Contract Selection by Indent Block",
            description: "Contract selection by removing the current indent block",
            category: SelectionExpansion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT | Modifiers::ALT,
                logical_key: Key::I,
            }),
        },
        // Line operations
        CommandMetadata {
            command: Indent,
            name: "Indent Selection",
            description: "Indent the selected lines",
            category: LineOperations,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::Tab,
            }),
        },
        CommandMetadata {
            command: Outdent,
            name: "Outdent Selection",
            description: "Outdent the selected lines",
            category: LineOperations,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::SHIFT,
                logical_key: Key::Tab,
            }),
        },
        CommandMetadata {
            command: DuplicateLines,
            name: "Duplicate Lines",
            description: "Duplicate the selected lines",
            category: LineOperations,
            shortcut: primary_shortcut(DuplicateLines),
        },
        CommandMetadata {
            command: DeleteLines,
            name: "Delete Lines",
            description: "Delete the selected lines",
            category: LineOperations,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::K,
            }),
        },
        CommandMetadata {
            command: MoveLinesUp,
            name: "Move Lines Up",
            description: "Move the selected lines up",
            category: LineOperations,
            shortcut: primary_shortcut(MoveLinesUp),
        },
        CommandMetadata {
            command: MoveLinesDown,
            name: "Move Lines Down",
            description: "Move the selected lines down",
            category: LineOperations,
            shortcut: primary_shortcut(MoveLinesDown),
        },
        CommandMetadata {
            command: JoinLines,
            name: "Join Lines",
            description: "Join the selected lines into one",
            category: LineOperations,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND,
                logical_key: Key::J,
            }),
        },
        CommandMetadata {
            command: SortLines,
            name: "Sort Lines",
            description: "Sort the selected lines alphabetically",
            category: LineOperations,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::S,
            }),
        },
        CommandMetadata {
            command: ReverseLines,
            name: "Reverse Lines",
            description: "Reverse the order of selected lines",
            category: LineOperations,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::R,
            }),
        },
        CommandMetadata {
            command: TrimTrailingWhitespace,
            name: "Trim Trailing Whitespace",
            description: "Remove trailing whitespace from selected lines",
            category: LineOperations,
            shortcut: primary_shortcut(TrimTrailingWhitespace),
        },
        CommandMetadata {
            command: NormalizeWhitespace,
            name: "Normalize Whitespace",
            description: "Normalize whitespace in selected text",
            category: LineOperations,
            shortcut: None,
        },
        // Multi-cursor
        CommandMetadata {
            command: AddNextMatch,
            name: "Add Next Match",
            description: "Add the next match of the current selection as a cursor",
            category: MultiCursor,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND,
                logical_key: Key::D,
            }),
        },
        CommandMetadata {
            command: AddAllMatches,
            name: "Add All Matches",
            description: "Add all matches of the current selection as cursors",
            category: MultiCursor,
            shortcut: primary_shortcut(AddAllMatches),
        },
        CommandMetadata {
            command: SplitSelectionIntoLines,
            name: "Split Selection into Lines",
            description: "Split the selection into multiple cursors on each line",
            category: MultiCursor,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::L,
            }),
        },
        CommandMetadata {
            command: ClearSecondaryCursors,
            name: "Clear Secondary Cursors",
            description: "Remove all secondary cursors",
            category: MultiCursor,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::Escape,
            }),
        },
        // Editing
        CommandMetadata {
            command: Backspace,
            name: "Backspace",
            description: "Delete the previous character",
            category: Editing,
            shortcut: primary_shortcut(Backspace),
        },
        CommandMetadata {
            command: DeleteForward,
            name: "Delete Forward",
            description: "Delete the next character",
            category: Editing,
            shortcut: primary_shortcut(DeleteForward),
        },
        CommandMetadata {
            command: BackspaceWord,
            name: "Backspace Word",
            description: "Delete to the previous word boundary",
            category: Editing,
            shortcut: primary_shortcut(BackspaceWord),
        },
        CommandMetadata {
            command: DeleteWordForward,
            name: "Delete Word Forward",
            description: "Delete to the next word boundary",
            category: Editing,
            shortcut: primary_shortcut(DeleteWordForward),
        },
        CommandMetadata {
            command: InsertNewline,
            name: "Insert Newline",
            description: "Insert a newline with indentation",
            category: Editing,
            shortcut: primary_shortcut(InsertNewline),
        },
        CommandMetadata {
            command: ToggleComments,
            name: "Toggle Comments",
            description: "Toggle comments on the selected lines",
            category: Editing,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND,
                logical_key: Key::Slash,
            }),
        },
        CommandMetadata {
            command: UpperCase,
            name: "Convert to Upper Case",
            description: "Convert selected text to upper case",
            category: Editing,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::CTRL,
                logical_key: Key::U,
            }),
        },
        CommandMetadata {
            command: LowerCase,
            name: "Convert to Lower Case",
            description: "Convert selected text to lower case",
            category: Editing,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::CTRL,
                logical_key: Key::L,
            }),
        },
        CommandMetadata {
            command: TitleCase,
            name: "Convert to Title Case",
            description: "Convert selected text to title case",
            category: Editing,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::CTRL,
                logical_key: Key::T,
            }),
        },
        // Search
        CommandMetadata {
            command: Find,
            name: "Find",
            description: "Open the find search bar",
            category: Search,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND,
                logical_key: Key::F,
            }),
        },
        CommandMetadata {
            command: FindReplace,
            name: "Find and Replace",
            description: "Open the find and replace bar",
            category: Search,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::ALT,
                logical_key: Key::F,
            }),
        },
        CommandMetadata {
            command: FindUnderCursor,
            name: "Find Under Cursor",
            description: "Find all occurrences of the word under the cursor",
            category: Search,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::F3,
            }),
        },
        CommandMetadata {
            command: SelectNextOccurrence,
            name: "Select Next Occurrence",
            description: "Select the next occurrence of the current word",
            category: Search,
            shortcut: None,
        },
        CommandMetadata {
            command: SearchInTabs,
            name: "Search in Tabs",
            description: "Search across all open tabs",
            category: Search,
            shortcut: None,
        },
        CommandMetadata {
            command: GoToLine,
            name: "Go to Line",
            description: "Jump to a line number in the current scratch",
            category: Search,
            shortcut: primary_shortcut(GoToLine),
        },
        // View
        CommandMetadata {
            command: CommandPalette,
            name: "Command Palette",
            description: "Open the command palette",
            category: View,
            shortcut: primary_shortcut(CommandPalette),
        },
        CommandMetadata {
            command: QuickSwitchTabs,
            name: "Quick Switch Tabs",
            description: "Open the quick tab switcher",
            category: View,
            shortcut: primary_shortcut(QuickSwitchTabs),
        },
        CommandMetadata {
            command: ToggleWrapMode,
            name: "Toggle Wrap Mode",
            description: "Cycle through line wrap modes (No Wrap, Viewport Wrap, Ruler Wrap)",
            category: View,
            shortcut: None,
        },
        CommandMetadata {
            command: ToggleVisibleWhitespace,
            name: "Toggle Visible Whitespace",
            description: "Toggle visible rendering of spaces and tabs",
            category: View,
            shortcut: None,
        },
        CommandMetadata {
            command: ToggleIndentGuides,
            name: "Toggle Indentation Guides",
            description: "Toggle vertical indentation guide lines",
            category: View,
            shortcut: None,
        },
        CommandMetadata {
            command: ToggleMinimap,
            name: "Toggle Minimap",
            description: "Toggle minimap with viewport indicator",
            category: View,
            shortcut: None,
        },
        CommandMetadata {
            command: ToggleStatusBar,
            name: "Toggle Status Bar",
            description: "Toggle status bar at the bottom of the window",
            category: View,
            shortcut: None,
        },
        CommandMetadata {
            command: ToggleTheme,
            name: "Toggle Theme",
            description: "Switch between dark and light themes",
            category: View,
            shortcut: None,
        },
        CommandMetadata {
            command: Preferences,
            name: "Preferences",
            description: "Open the preferences window",
            category: View,
            shortcut: primary_shortcut(Preferences),
        },
        // File commands
        CommandMetadata {
            command: ImportFile,
            name: "Import File",
            description: "Import text from a file into the current scratch buffer",
            category: App,
            shortcut: primary_shortcut(ImportFile),
        },
        CommandMetadata {
            command: ExportFile,
            name: "Export File",
            description: "Export the current scratch buffer content to a file",
            category: App,
            shortcut: primary_shortcut(ExportFile),
        },
        CommandMetadata {
            command: ToggleBookmark,
            name: "Toggle Bookmark",
            description: "Toggle a bookmark at the current line",
            category: View,
            shortcut: primary_shortcut(ToggleBookmark),
        },
        CommandMetadata {
            command: JumpToNextBookmark,
            name: "Jump to Next Bookmark",
            description: "Move the cursor to the next bookmark",
            category: View,
            shortcut: primary_shortcut(JumpToNextBookmark),
        },
        CommandMetadata {
            command: ClearBookmarks,
            name: "Clear Bookmarks",
            description: "Clear all bookmarks in the current scratch",
            category: View,
            shortcut: primary_shortcut(ClearBookmarks),
        },
        CommandMetadata {
            command: SplitPaneHorizontal,
            name: "Split Pane Horizontal",
            description: "Split the editor pane horizontally",
            category: View,
            shortcut: primary_shortcut(SplitPaneHorizontal),
        },
        CommandMetadata {
            command: SplitPaneVertical,
            name: "Split Pane Vertical",
            description: "Split the editor pane vertically",
            category: View,
            shortcut: primary_shortcut(SplitPaneVertical),
        },
        CommandMetadata {
            command: ClosePane,
            name: "Close Pane",
            description: "Close the active editor pane",
            category: View,
            shortcut: primary_shortcut(ClosePane),
        },
        CommandMetadata {
            command: PinTab,
            name: "Pin Tab",
            description: "Pin or unpin the current tab",
            category: View,
            shortcut: primary_shortcut(PinTab),
        },
        CommandMetadata {
            command: MoveTabLeft,
            name: "Move Tab Left",
            description: "Move the current tab left",
            category: View,
            shortcut: primary_shortcut(MoveTabLeft),
        },
        CommandMetadata {
            command: MoveTabRight,
            name: "Move Tab Right",
            description: "Move the current tab right",
            category: View,
            shortcut: primary_shortcut(MoveTabRight),
        },
        CommandMetadata {
            command: ReopenLastClosed,
            name: "Reopen Last Closed",
            description: "Reopen the most recently closed document",
            category: App,
            shortcut: primary_shortcut(ReopenLastClosed),
        },
    ]
}
