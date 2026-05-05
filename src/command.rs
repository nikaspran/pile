use eframe::egui;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Command {
    // App commands
    NewScratch,
    CloseScratch,
    RenameTab,
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

    // View
    CommandPalette,
    ToggleWrapMode,
    ToggleVisibleWhitespace,
    ToggleIndentGuides,
    ToggleMinimap,
    ToggleTheme,

    // File
    ImportFile,
    ExportFile,
    Preferences,
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
            || format!("{:?}", self.command).to_lowercase().contains(&query)
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
    use egui::{KeyboardShortcut, Key, Modifiers};

    vec![
        // App commands
        CommandMetadata {
            command: NewScratch,
            name: "New Scratch",
            description: "Create a new scratch buffer",
            category: App,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND,
                logical_key: Key::N,
            }),
        },
        CommandMetadata {
            command: CloseScratch,
            name: "Close Scratch",
            description: "Close the current scratch buffer",
            category: App,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND,
                logical_key: Key::W,
            }),
        },
        CommandMetadata {
            command: RenameTab,
            name: "Rename Tab",
            description: "Rename the current tab",
            category: App,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::F2,
            }),
        },
        CommandMetadata {
            command: Undo,
            name: "Undo",
            description: "Undo the last edit",
            category: App,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND,
                logical_key: Key::Z,
            }),
        },
        CommandMetadata {
            command: Redo,
            name: "Redo",
            description: "Redo the last undone edit",
            category: App,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::Z,
            }),
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
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::Home,
            }),
        },
        CommandMetadata {
            command: MoveDocumentEnd,
            name: "Move to Document End",
            description: "Move cursor to the end of the document",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::End,
            }),
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
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::ArrowUp,
            }),
        },
        CommandMetadata {
            command: MoveParagraphDown,
            name: "Move Paragraph Down",
            description: "Move cursor down by one paragraph",
            category: Motion,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::ALT,
                logical_key: Key::ArrowDown,
            }),
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
            shortcut: None,
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
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::ALT,
                logical_key: Key::ArrowUp,
            }),
        },
        CommandMetadata {
            command: MoveLinesDown,
            name: "Move Lines Down",
            description: "Move the selected lines down",
            category: LineOperations,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::ALT,
                logical_key: Key::ArrowDown,
            }),
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
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::T,
            }),
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
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::D,
            }),
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
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND,
                logical_key: Key::D,
            }),
        },
        CommandMetadata {
            command: SearchInTabs,
            name: "Search in Tabs",
            description: "Search across all open tabs",
            category: Search,
            shortcut: None,
        },
        // View
        CommandMetadata {
            command: CommandPalette,
            name: "Command Palette",
            description: "Open the command palette",
            category: View,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::P,
            }),
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
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND,
                logical_key: Key::Comma,
            }),
        },
        // File commands
        CommandMetadata {
            command: ImportFile,
            name: "Import File",
            description: "Import text from a file into the current scratch buffer",
            category: App,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::I,
            }),
        },
        CommandMetadata {
            command: ExportFile,
            name: "Export File",
            description: "Export the current scratch buffer content to a file",
            category: App,
            shortcut: Some(KeyboardShortcut {
                modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
                logical_key: Key::E,
            }),
        },
    ]
}
