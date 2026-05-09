#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NativeMenuCommand {
    // App
    NewScratch,
    CloseScratch,
    RenameTab,
    ImportFile,
    ExportFile,
    Preferences,
    Quit,

    // Edit
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    ToggleComments,
    UpperCase,
    LowerCase,
    TitleCase,

    // Selection
    ExpandWord,
    ContractWord,
    ExpandLine,
    ContractLine,
    ExpandBracketPair,
    ContractBracketPair,
    ExpandIndentBlock,
    ContractIndentBlock,

    // Line operations
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

    // Multi-cursor
    AddNextMatch,
    AddAllMatches,
    SplitSelectionIntoLines,
    ClearSecondaryCursors,

    // Search
    Find,
    FindReplace,
    FindUnderCursor,
    SearchInTabs,

    // View
    CommandPalette,
    ToggleWrapMode,
    ToggleVisibleWhitespace,
    ToggleIndentGuides,
    ToggleMinimap,
    ToggleStatusBar,
    ToggleTheme,
    GoToLine,

    // Bookmarks
    ToggleBookmark,
    JumpToNextBookmark,
    ClearBookmarks,

    // Window
    SplitPaneHorizontal,
    SplitPaneVertical,
    ClosePane,
    PinTab,
    MoveTabLeft,
    MoveTabRight,
}

pub struct NativeMenu {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    _menu: muda::Menu,
}

impl NativeMenu {
    pub fn install() -> Option<Self> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            match build_menu() {
                Ok(menu) => {
                    install_menu(&menu);
                    Some(Self { _menu: menu })
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to install native menu");
                    None
                }
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            None
        }
    }

    pub fn next_command(&self) -> Option<NativeMenuCommand> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
                if let Some(cmd) = command_from_id(event.id.as_ref()) {
                    return Some(cmd);
                }
            }
        }
        None
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn install_menu(menu: &muda::Menu) {
    #[cfg(target_os = "macos")]
    {
        menu.init_for_nsapp();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = menu.init_for_hwnd(std::ptr::null_mut());
    }
    #[cfg(target_os = "linux")]
    {
        let _ = menu.init_for_gtk();
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn command_from_id(id: &str) -> Option<NativeMenuCommand> {
    use NativeMenuCommand::*;
    match id {
        // App
        "pile.new_scratch" => Some(NewScratch),
        "pile.close_scratch" => Some(CloseScratch),
        "pile.rename_tab" => Some(RenameTab),
        "pile.import_file" => Some(ImportFile),
        "pile.export_file" => Some(ExportFile),
        "pile.preferences" => Some(Preferences),
        "pile.quit" => Some(Quit),

        // Edit
        "pile.undo" => Some(Undo),
        "pile.redo" => Some(Redo),
        "pile.cut" => Some(Cut),
        "pile.copy" => Some(Copy),
        "pile.paste" => Some(Paste),
        "pile.select_all" => Some(SelectAll),
        "pile.toggle_comments" => Some(ToggleComments),
        "pile.upper_case" => Some(UpperCase),
        "pile.lower_case" => Some(LowerCase),
        "pile.title_case" => Some(TitleCase),

        // Selection
        "pile.expand_word" => Some(ExpandWord),
        "pile.contract_word" => Some(ContractWord),
        "pile.expand_line" => Some(ExpandLine),
        "pile.contract_line" => Some(ContractLine),
        "pile.expand_bracket" => Some(ExpandBracketPair),
        "pile.contract_bracket" => Some(ContractBracketPair),
        "pile.expand_indent" => Some(ExpandIndentBlock),
        "pile.contract_indent" => Some(ContractIndentBlock),

        // Line operations
        "pile.indent" => Some(Indent),
        "pile.outdent" => Some(Outdent),
        "pile.duplicate_lines" => Some(DuplicateLines),
        "pile.delete_lines" => Some(DeleteLines),
        "pile.move_lines_up" => Some(MoveLinesUp),
        "pile.move_lines_down" => Some(MoveLinesDown),
        "pile.join_lines" => Some(JoinLines),
        "pile.sort_lines" => Some(SortLines),
        "pile.reverse_lines" => Some(ReverseLines),
        "pile.trim_whitespace" => Some(TrimTrailingWhitespace),

        // Multi-cursor
        "pile.add_next_match" => Some(AddNextMatch),
        "pile.add_all_matches" => Some(AddAllMatches),
        "pile.split_selection" => Some(SplitSelectionIntoLines),
        "pile.clear_cursors" => Some(ClearSecondaryCursors),

        // Search
        "pile.find" => Some(Find),
        "pile.find_replace" => Some(FindReplace),
        "pile.find_under_cursor" => Some(FindUnderCursor),
        "pile.search_in_tabs" => Some(SearchInTabs),

        // View
        "pile.command_palette" => Some(CommandPalette),
        "pile.toggle_wrap" => Some(ToggleWrapMode),
        "pile.toggle_whitespace" => Some(ToggleVisibleWhitespace),
        "pile.toggle_indent" => Some(ToggleIndentGuides),
        "pile.toggle_minimap" => Some(ToggleMinimap),
        "pile.toggle_status_bar" => Some(ToggleStatusBar),
        "pile.toggle_theme" => Some(ToggleTheme),
        "pile.go_to_line" => Some(GoToLine),

        // Bookmarks
        "pile.toggle_bookmark" => Some(ToggleBookmark),
        "pile.next_bookmark" => Some(JumpToNextBookmark),
        "pile.clear_bookmarks" => Some(ClearBookmarks),

        // Window
        "pile.split_h" => Some(SplitPaneHorizontal),
        "pile.split_v" => Some(SplitPaneVertical),
        "pile.close_pane" => Some(ClosePane),
        "pile.pin_tab" => Some(PinTab),
        "pile.move_tab_left" => Some(MoveTabLeft),
        "pile.move_tab_right" => Some(MoveTabRight),

        _ => None,
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn parse_accel(s: &str) -> Result<muda::accelerator::Accelerator, muda::AcceleratorParseError> {
    s.parse()
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn build_menu() -> muda::Result<muda::Menu> {
    use muda::*;

    let menu = Menu::new();

    // App menu (macOS) or File menu (other platforms)
    #[cfg(target_os = "macos")]
    {
        let about = PredefinedMenuItem::about(
            Some("About pile"),
            Some(AboutMetadata {
                name: Some("pile".to_owned()),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
                comments: Some(env!("CARGO_PKG_DESCRIPTION").to_owned()),
                copyright: Some("Copyright (c) 2026 Nikas Praninskas".to_owned()),
                license: Some(env!("CARGO_PKG_LICENSE").to_owned()),
                ..Default::default()
            }),
        );
        let preferences = MenuItem::with_id(
            "pile.preferences",
            "Preferences...",
            true,
            Some(parse_accel("cmdorctrl+,")?),
        );
        let app_menu = Submenu::with_items(
            "pile",
            true,
            &[
                &about,
                &PredefinedMenuItem::separator(),
                &preferences,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ],
        )?;
        menu.append(&app_menu)?;
    }

    // File menu
    let new_scratch = MenuItem::with_id(
        "pile.new_scratch",
        "New Scratch",
        true,
        Some(parse_accel("cmdorctrl+n")?),
    );
    let close_scratch = MenuItem::with_id(
        "pile.close_scratch",
        "Close Scratch",
        true,
        Some(parse_accel("cmdorctrl+w")?),
    );
    let import_file = MenuItem::with_id(
        "pile.import_file",
        "Import File...",
        true,
        Some(parse_accel("cmdorctrl+shift+i")?),
    );
    let export_file = MenuItem::with_id(
        "pile.export_file",
        "Export File...",
        true,
        Some(parse_accel("cmdorctrl+shift+e")?),
    );
    let rename_tab = MenuItem::with_id("pile.rename_tab", "Rename Tab", true, None);
    let pin_tab = MenuItem::with_id("pile.pin_tab", "Pin/Unpin Tab", true, None);

    #[cfg(target_os = "macos")]
    let file_items: &[&dyn muda::IsMenuItem] = &[
        &new_scratch,
        &close_scratch,
        &PredefinedMenuItem::separator(),
        &import_file,
        &export_file,
        &PredefinedMenuItem::separator(),
        &rename_tab,
        &pin_tab,
    ];

    #[cfg(not(target_os = "macos"))]
    let file_items: &[&dyn muda::IsMenuItem] = &[
        &new_scratch,
        &close_scratch,
        &PredefinedMenuItem::separator(),
        &import_file,
        &export_file,
        &PredefinedMenuItem::separator(),
        &rename_tab,
        &pin_tab,
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(
            "pile.preferences",
            "Preferences...",
            true,
            Some(parse_accel("cmdorctrl+,")?),
        ),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ];

    let file_menu = Submenu::with_items("File", true, file_items)?;
    menu.append(&file_menu)?;

    // Edit menu
    let undo_item = MenuItem::with_id("pile.undo", "Undo", true, Some(parse_accel("cmdorctrl+z")?));
    let redo_item = MenuItem::with_id(
        "pile.redo",
        "Redo",
        true,
        Some(parse_accel("cmdorctrl+shift+z")?),
    );
    let cut_item = MenuItem::with_id("pile.cut", "Cut", true, Some(parse_accel("cmdorctrl+x")?));
    let copy_item = MenuItem::with_id("pile.copy", "Copy", true, Some(parse_accel("cmdorctrl+c")?));
    let paste_item = MenuItem::with_id(
        "pile.paste",
        "Paste",
        true,
        Some(parse_accel("cmdorctrl+v")?),
    );
    let select_all_item = MenuItem::with_id(
        "pile.select_all",
        "Select All",
        true,
        Some(parse_accel("cmdorctrl+a")?),
    );

    let toggle_comments = MenuItem::with_id(
        "pile.toggle_comments",
        "Toggle Comments",
        true,
        Some(parse_accel("cmdorctrl+/")?),
    );
    let upper_case = MenuItem::with_id(
        "pile.upper_case",
        "Upper Case",
        true,
        Some(parse_accel("cmdorctrl+ctrl+u")?),
    );
    let lower_case = MenuItem::with_id(
        "pile.lower_case",
        "Lower Case",
        true,
        Some(parse_accel("cmdorctrl+ctrl+l")?),
    );
    let title_case = MenuItem::with_id(
        "pile.title_case",
        "Title Case",
        true,
        Some(parse_accel("cmdorctrl+ctrl+t")?),
    );

    let edit_menu = Submenu::with_items(
        "Edit",
        true,
        &[
            &undo_item,
            &redo_item,
            &PredefinedMenuItem::separator(),
            &cut_item,
            &copy_item,
            &paste_item,
            &select_all_item,
            &PredefinedMenuItem::separator(),
            &toggle_comments,
            &upper_case,
            &lower_case,
            &title_case,
        ],
    )?;
    menu.append(&edit_menu)?;

    // Selection menu
    let expand_word = MenuItem::with_id(
        "pile.expand_word",
        "Expand Selection by Word",
        true,
        Some(parse_accel("alt+w")?),
    );
    let contract_word = MenuItem::with_id(
        "pile.contract_word",
        "Contract Selection by Word",
        true,
        Some(parse_accel("alt+shift+w")?),
    );
    let expand_line = MenuItem::with_id(
        "pile.expand_line",
        "Expand Selection by Line",
        true,
        Some(parse_accel("alt+l")?),
    );
    let contract_line = MenuItem::with_id(
        "pile.contract_line",
        "Contract Selection by Line",
        true,
        Some(parse_accel("alt+shift+l")?),
    );
    let expand_bracket = MenuItem::with_id(
        "pile.expand_bracket",
        "Expand Selection by Bracket Pair",
        true,
        Some(parse_accel("alt+b")?),
    );
    let contract_bracket = MenuItem::with_id(
        "pile.contract_bracket",
        "Contract Selection by Bracket Pair",
        true,
        Some(parse_accel("alt+shift+b")?),
    );
    let expand_indent = MenuItem::with_id(
        "pile.expand_indent",
        "Expand Selection by Indent Block",
        true,
        Some(parse_accel("alt+i")?),
    );
    let contract_indent = MenuItem::with_id(
        "pile.contract_indent",
        "Contract Selection by Indent Block",
        true,
        Some(parse_accel("alt+shift+i")?),
    );

    let selection_menu = Submenu::with_items(
        "Selection",
        true,
        &[
            &expand_word,
            &contract_word,
            &PredefinedMenuItem::separator(),
            &expand_line,
            &contract_line,
            &PredefinedMenuItem::separator(),
            &expand_bracket,
            &contract_bracket,
            &PredefinedMenuItem::separator(),
            &expand_indent,
            &contract_indent,
        ],
    )?;
    menu.append(&selection_menu)?;

    // Line Operations menu
    let indent = MenuItem::with_id(
        "pile.indent",
        "Indent Selection",
        true,
        Some(parse_accel("tab")?),
    );
    let outdent = MenuItem::with_id(
        "pile.outdent",
        "Outdent Selection",
        true,
        Some(parse_accel("shift+tab")?),
    );
    let duplicate_lines = MenuItem::with_id(
        "pile.duplicate_lines",
        "Duplicate Lines",
        true,
        Some(parse_accel("cmdorctrl+shift+d")?),
    );
    let delete_lines = MenuItem::with_id(
        "pile.delete_lines",
        "Delete Lines",
        true,
        Some(parse_accel("cmdorctrl+shift+k")?),
    );
    let move_lines_up = MenuItem::with_id(
        "pile.move_lines_up",
        "Move Lines Up",
        true,
        Some(parse_accel("alt+up")?),
    );
    let move_lines_down = MenuItem::with_id(
        "pile.move_lines_down",
        "Move Lines Down",
        true,
        Some(parse_accel("alt+down")?),
    );
    let join_lines = MenuItem::with_id(
        "pile.join_lines",
        "Join Lines",
        true,
        Some(parse_accel("cmdorctrl+j")?),
    );
    let sort_lines = MenuItem::with_id(
        "pile.sort_lines",
        "Sort Lines",
        true,
        Some(parse_accel("cmdorctrl+shift+s")?),
    );
    let reverse_lines = MenuItem::with_id(
        "pile.reverse_lines",
        "Reverse Lines",
        true,
        Some(parse_accel("cmdorctrl+shift+r")?),
    );
    let trim_whitespace = MenuItem::with_id(
        "pile.trim_whitespace",
        "Trim Trailing Whitespace",
        true,
        Some(parse_accel("cmdorctrl+alt+t")?),
    );

    let line_ops_menu = Submenu::with_items(
        "Line Operations",
        true,
        &[
            &indent,
            &outdent,
            &PredefinedMenuItem::separator(),
            &duplicate_lines,
            &delete_lines,
            &PredefinedMenuItem::separator(),
            &move_lines_up,
            &move_lines_down,
            &PredefinedMenuItem::separator(),
            &join_lines,
            &sort_lines,
            &reverse_lines,
            &trim_whitespace,
        ],
    )?;
    menu.append(&line_ops_menu)?;

    // Multi-Cursor menu
    let add_next = MenuItem::with_id(
        "pile.add_next_match",
        "Add Next Match",
        true,
        Some(parse_accel("cmdorctrl+d")?),
    );
    let add_all = MenuItem::with_id(
        "pile.add_all_matches",
        "Add All Matches",
        true,
        Some(parse_accel("cmdorctrl+shift+l")?),
    );
    let split_selection = MenuItem::with_id(
        "pile.split_selection",
        "Split Selection into Lines",
        true,
        Some(parse_accel("cmdorctrl+shift+l")?),
    );
    let clear_cursors = MenuItem::with_id(
        "pile.clear_cursors",
        "Clear Secondary Cursors",
        true,
        Some(parse_accel("escape")?),
    );

    let multicursor_menu = Submenu::with_items(
        "Multi-Cursor",
        true,
        &[
            &add_next,
            &add_all,
            &PredefinedMenuItem::separator(),
            &split_selection,
            &clear_cursors,
        ],
    )?;
    menu.append(&multicursor_menu)?;

    // Search menu
    let find = MenuItem::with_id("pile.find", "Find", true, Some(parse_accel("cmdorctrl+f")?));
    let find_replace = MenuItem::with_id(
        "pile.find_replace",
        "Find and Replace",
        true,
        Some(parse_accel("cmdorctrl+alt+f")?),
    );
    let find_under_cursor = MenuItem::with_id(
        "pile.find_under_cursor",
        "Find Under Cursor",
        true,
        Some(parse_accel("f3")?),
    );
    let search_in_tabs = MenuItem::with_id("pile.search_in_tabs", "Search in Tabs", true, None);

    let search_menu = Submenu::with_items(
        "Search",
        true,
        &[
            &find,
            &find_replace,
            &PredefinedMenuItem::separator(),
            &find_under_cursor,
            &search_in_tabs,
        ],
    )?;
    menu.append(&search_menu)?;

    // View menu
    let command_palette = MenuItem::with_id(
        "pile.command_palette",
        "Command Palette",
        true,
        Some(parse_accel("cmdorctrl+shift+p")?),
    );
    let go_to_line = MenuItem::with_id(
        "pile.go_to_line",
        "Go to Line",
        true,
        Some(parse_accel("cmdorctrl+g")?),
    );
    let toggle_wrap = MenuItem::with_id("pile.toggle_wrap", "Toggle Wrap Mode", true, None);
    let toggle_whitespace = MenuItem::with_id(
        "pile.toggle_whitespace",
        "Toggle Visible Whitespace",
        true,
        None,
    );
    let toggle_indent = MenuItem::with_id(
        "pile.toggle_indent",
        "Toggle Indentation Guides",
        true,
        None,
    );
    let toggle_minimap = MenuItem::with_id("pile.toggle_minimap", "Toggle Minimap", true, None);
    let toggle_status_bar =
        MenuItem::with_id("pile.toggle_status_bar", "Toggle Status Bar", true, None);
    let toggle_theme = MenuItem::with_id("pile.toggle_theme", "Toggle Theme", true, None);

    let view_menu = Submenu::with_items(
        "View",
        true,
        &[
            &command_palette,
            &go_to_line,
            &PredefinedMenuItem::separator(),
            &toggle_wrap,
            &toggle_whitespace,
            &toggle_indent,
            &toggle_minimap,
            &toggle_status_bar,
            &PredefinedMenuItem::separator(),
            &toggle_theme,
        ],
    )?;
    menu.append(&view_menu)?;

    // Bookmarks menu
    let toggle_bookmark = MenuItem::with_id(
        "pile.toggle_bookmark",
        "Toggle Bookmark",
        true,
        Some(parse_accel("cmdorctrl+f2")?),
    );
    let next_bookmark = MenuItem::with_id(
        "pile.next_bookmark",
        "Jump to Next Bookmark",
        true,
        Some(parse_accel("f4")?),
    );
    let clear_bookmarks = MenuItem::with_id(
        "pile.clear_bookmarks",
        "Clear All Bookmarks",
        true,
        Some(parse_accel("cmdorctrl+shift+f2")?),
    );

    let bookmarks_menu = Submenu::with_items(
        "Bookmarks",
        true,
        &[&toggle_bookmark, &next_bookmark, &clear_bookmarks],
    )?;
    menu.append(&bookmarks_menu)?;

    // Window menu
    let split_h = MenuItem::with_id(
        "pile.split_h",
        "Split Pane Horizontal",
        true,
        Some(parse_accel("cmdorctrl+shift+h")?),
    );
    let split_v = MenuItem::with_id(
        "pile.split_v",
        "Split Pane Vertical",
        true,
        Some(parse_accel("cmdorctrl+shift+v")?),
    );
    let close_pane = MenuItem::with_id(
        "pile.close_pane",
        "Close Pane",
        true,
        Some(parse_accel("cmdorctrl+shift+w")?),
    );
    let move_left = MenuItem::with_id(
        "pile.move_tab_left",
        "Move Tab Left",
        true,
        Some(parse_accel("cmdorctrl+alt+left")?),
    );
    let move_right = MenuItem::with_id(
        "pile.move_tab_right",
        "Move Tab Right",
        true,
        Some(parse_accel("cmdorctrl+alt+right")?),
    );

    #[cfg(target_os = "macos")]
    let window_items: &[&dyn muda::IsMenuItem] = &[
        &split_h,
        &split_v,
        &close_pane,
        &PredefinedMenuItem::separator(),
        &move_left,
        &move_right,
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::maximize(None),
        &PredefinedMenuItem::fullscreen(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::bring_all_to_front(None),
    ];

    #[cfg(not(target_os = "macos"))]
    let window_items: &[&dyn muda::IsMenuItem] = &[
        &split_h,
        &split_v,
        &close_pane,
        &PredefinedMenuItem::separator(),
        &move_left,
        &move_right,
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::maximize(None),
        &PredefinedMenuItem::fullscreen(None),
    ];

    let window_menu = Submenu::with_items("Window", true, window_items)?;
    menu.append(&window_menu)?;

    Ok(menu)
}
