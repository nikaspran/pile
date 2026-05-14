//! Tests for platform-facing command metadata and shortcut formatting.

use std::collections::HashSet;

use pile::command::{
    Command, EDITOR_KEY_COMMANDS, KEYBOARD_COMMANDS, all_commands, default_shortcuts,
};

#[test]
fn app_and_editor_keyboard_commands_have_metadata() {
    let metadata_commands: HashSet<Command> = all_commands()
        .into_iter()
        .map(|metadata| metadata.command)
        .collect();

    for command in KEYBOARD_COMMANDS.iter().chain(EDITOR_KEY_COMMANDS.iter()) {
        assert!(
            metadata_commands.contains(command),
            "{command:?} is dispatchable but missing command metadata"
        );
    }
}

#[test]
fn command_metadata_is_nonempty_and_unique() {
    let all_cmds = all_commands();
    assert!(!all_cmds.is_empty());

    let mut commands = HashSet::new();
    let mut names = HashSet::new();
    for metadata in &all_cmds {
        assert!(!metadata.name.trim().is_empty());
        assert!(!metadata.description.trim().is_empty());
        assert!(
            commands.insert(metadata.command),
            "duplicate command metadata for {:?}",
            metadata.command
        );
        assert!(
            names.insert(metadata.name),
            "duplicate command palette name: {}",
            metadata.name
        );
    }
}

#[test]
fn metadata_primary_shortcuts_are_registered_defaults() {
    let defaults: HashSet<_> = default_shortcuts()
        .into_iter()
        .map(|binding| (binding.command, binding.shortcut))
        .collect();

    for metadata in all_commands() {
        if let Some(shortcut) = metadata.shortcut {
            assert!(
                defaults.contains(&(metadata.command, shortcut)),
                "{:?} metadata shortcut is not registered as a default shortcut",
                metadata.command
            );
        }
    }
}

#[test]
fn shortcut_formatting_per_platform() {
    use egui::KeyboardShortcut;
    use pile::command::format_shortcut;

    let shortcut = KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::N);
    let ctx = egui::Context::default();
    let formatted = format_shortcut(&shortcut, &ctx);

    assert!(!formatted.is_empty());
    if cfg!(target_os = "macos") {
        assert!(formatted.contains('⌃'));
    } else {
        assert!(formatted.contains("Ctrl"));
    }
}

#[test]
fn native_menu_install_symbol_is_available() {
    let install = pile::native_menu::NativeMenu::install;
    let _ = install;
}
