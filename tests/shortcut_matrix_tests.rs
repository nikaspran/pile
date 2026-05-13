use std::collections::{HashMap, HashSet};

use egui::{Key, KeyboardShortcut, Modifiers};
use pile::command::{
    Command, EDITOR_KEY_COMMANDS, all_commands, command_for_key_event, default_shortcuts,
};

fn shortcut(modifiers: Modifiers, logical_key: Key) -> KeyboardShortcut {
    KeyboardShortcut {
        modifiers,
        logical_key,
    }
}

fn command_shortcuts() -> HashMap<Command, HashSet<KeyboardShortcut>> {
    let mut shortcuts = HashMap::new();
    for binding in default_shortcuts() {
        shortcuts
            .entry(binding.command)
            .or_insert_with(HashSet::new)
            .insert(binding.shortcut);
    }
    shortcuts
}

#[test]
fn editor_core_standard_shortcuts_are_bound() {
    use Command::*;

    let shortcuts = command_shortcuts();
    let expected = [
        (NewScratch, shortcut(Modifiers::COMMAND, Key::N)),
        (CloseScratch, shortcut(Modifiers::COMMAND, Key::W)),
        (Undo, shortcut(Modifiers::COMMAND, Key::Z)),
        (
            Redo,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z),
        ),
        (Cut, shortcut(Modifiers::COMMAND, Key::X)),
        (Copy, shortcut(Modifiers::COMMAND, Key::C)),
        (Paste, shortcut(Modifiers::COMMAND, Key::V)),
        (SelectAll, shortcut(Modifiers::COMMAND, Key::A)),
        (Find, shortcut(Modifiers::COMMAND, Key::F)),
        (
            FindReplace,
            shortcut(Modifiers::COMMAND | Modifiers::ALT, Key::F),
        ),
        (FindUnderCursor, shortcut(Modifiers::NONE, Key::F3)),
        (GoToLine, shortcut(Modifiers::COMMAND, Key::G)),
        (
            CommandPalette,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::P),
        ),
        (QuickSwitchTabs, shortcut(Modifiers::COMMAND, Key::P)),
        (AddNextMatch, shortcut(Modifiers::COMMAND, Key::D)),
        (
            AddAllMatches,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::L),
        ),
        (
            SplitSelectionIntoLines,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::L),
        ),
        (
            DuplicateLines,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::D),
        ),
        (
            DuplicateLines,
            shortcut(Modifiers::SHIFT | Modifiers::ALT, Key::ArrowDown),
        ),
        (MoveLinesUp, shortcut(Modifiers::ALT, Key::ArrowUp)),
        (MoveLinesDown, shortcut(Modifiers::ALT, Key::ArrowDown)),
        (
            DeleteLines,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::K),
        ),
        (JoinLines, shortcut(Modifiers::COMMAND, Key::J)),
        (ToggleComments, shortcut(Modifiers::COMMAND, Key::Slash)),
        (ToggleBookmark, shortcut(Modifiers::COMMAND, Key::F2)),
        (JumpToNextBookmark, shortcut(Modifiers::NONE, Key::F4)),
        (
            ClearBookmarks,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::F2),
        ),
        (
            SplitPaneHorizontal,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::H),
        ),
        (
            SplitPaneVertical,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::V),
        ),
        (
            ClosePane,
            shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::W),
        ),
        (PinTab, shortcut(Modifiers::ALT | Modifiers::SHIFT, Key::P)),
        (
            MoveTabLeft,
            shortcut(Modifiers::COMMAND | Modifiers::ALT, Key::ArrowLeft),
        ),
        (
            MoveTabRight,
            shortcut(Modifiers::COMMAND | Modifiers::ALT, Key::ArrowRight),
        ),
    ];

    for (command, shortcut) in expected {
        assert!(
            shortcuts
                .get(&command)
                .is_some_and(|actual| actual.contains(&shortcut)),
            "{command:?} should be bound to {shortcut:?}"
        );
    }
}

#[test]
fn default_shortcuts_do_not_have_unintentional_conflicts() {
    use Command::*;

    let allowed_conflicts = HashMap::from([(
        shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::L),
        HashSet::from([AddAllMatches, SplitSelectionIntoLines]),
    )]);

    let mut by_shortcut: HashMap<KeyboardShortcut, HashSet<Command>> = HashMap::new();
    for binding in default_shortcuts() {
        by_shortcut
            .entry(binding.shortcut)
            .or_default()
            .insert(binding.command);
    }

    for (shortcut, commands) in by_shortcut {
        if commands.len() <= 1 {
            continue;
        }
        assert_eq!(
            allowed_conflicts.get(&shortcut),
            Some(&commands),
            "unexpected shortcut conflict for {shortcut:?}: {commands:?}"
        );
    }
}

#[test]
fn command_metadata_primary_shortcuts_are_registered_defaults() {
    let defaults: HashSet<_> = default_shortcuts()
        .into_iter()
        .map(|binding| (binding.command, binding.shortcut))
        .collect();

    for metadata in all_commands() {
        if let Some(shortcut) = metadata.shortcut {
            assert!(
                defaults.contains(&(metadata.command, shortcut)),
                "{:?} metadata shortcut {:?} is not in default_shortcuts()",
                metadata.command,
                shortcut
            );
        }
    }
}

#[test]
fn editor_key_commands_resolve_through_default_shortcuts() {
    use Command::*;

    let cases = [
        (Modifiers::NONE, Key::Backspace, Backspace),
        (Modifiers::NONE, Key::Delete, DeleteForward),
        (Modifiers::ALT, Key::Backspace, BackspaceWord),
        (Modifiers::ALT, Key::Delete, DeleteWordForward),
        (Modifiers::CTRL, Key::Backspace, BackspaceWord),
        (Modifiers::CTRL, Key::Delete, DeleteWordForward),
        (Modifiers::NONE, Key::ArrowLeft, MoveLeft),
        (Modifiers::SHIFT, Key::ArrowRight, SelectRight),
        (Modifiers::ALT, Key::W, ExpandWord),
        (Modifiers::SHIFT | Modifiers::ALT, Key::W, ContractWord),
        (Modifiers::NONE, Key::Enter, InsertNewline),
    ];

    for (modifiers, key, command) in cases {
        assert_eq!(
            command_for_key_event(key, modifiers, EDITOR_KEY_COMMANDS),
            Some(command),
            "{modifiers:?}+{key:?} should resolve to {command:?}"
        );
    }
}
