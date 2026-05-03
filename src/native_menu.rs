#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeMenuCommand {
    NewScratch,
    CloseScratch,
    RenameTab,
    Undo,
    Redo,
}

#[cfg(target_os = "macos")]
mod platform {
    use muda::{
        AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
        accelerator::Accelerator,
    };
    use tracing::warn;

    use super::NativeMenuCommand;

    const NEW_SCRATCH_ID: &str = "pile.new_scratch";
    const CLOSE_SCRATCH_ID: &str = "pile.close_scratch";
    const RENAME_TAB_ID: &str = "pile.rename_tab";
    const UNDO_ID: &str = "pile.undo";
    const REDO_ID: &str = "pile.redo";

    pub struct NativeMenu {
        _menu: Menu,
    }

    impl NativeMenu {
        pub fn install() -> Option<Self> {
            match build_menu() {
                Ok(menu) => {
                    menu.init_for_nsapp();
                    Some(Self { _menu: menu })
                }
                Err(err) => {
                    warn!(error = %err, "failed to install native macOS menu");
                    None
                }
            }
        }

        pub fn next_command(&self) -> Option<NativeMenuCommand> {
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                match event.id.as_ref() {
                    NEW_SCRATCH_ID => return Some(NativeMenuCommand::NewScratch),
                    CLOSE_SCRATCH_ID => return Some(NativeMenuCommand::CloseScratch),
                    RENAME_TAB_ID => return Some(NativeMenuCommand::RenameTab),
                    UNDO_ID => return Some(NativeMenuCommand::Undo),
                    REDO_ID => return Some(NativeMenuCommand::Redo),
                    _ => {}
                }
            }

            None
        }
    }

    fn build_menu() -> muda::Result<Menu> {
        let menu = Menu::new();

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
        let app_menu = Submenu::with_items(
            "pile",
            true,
            &[
                &about,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ],
        )?;

        let new_scratch = MenuItem::with_id(
            NEW_SCRATCH_ID,
            "New Scratch",
            true,
            Some("cmdorctrl+n".parse::<Accelerator>()?),
        );
        let close_scratch = MenuItem::with_id(
            CLOSE_SCRATCH_ID,
            "Close Scratch",
            true,
            Some("cmdorctrl+w".parse::<Accelerator>()?),
        );
        let rename_tab = MenuItem::with_id(RENAME_TAB_ID, "Rename Tab", true, None);
        let file_menu = Submenu::with_items(
            "File",
            true,
            &[
                &new_scratch,
                &close_scratch,
                &PredefinedMenuItem::separator(),
                &rename_tab,
            ],
        )?;

        let undo_item = MenuItem::with_id(
            UNDO_ID,
            "Undo",
            true,
            Some("cmdorctrl+z".parse::<Accelerator>()?),
        );
        let redo_item = MenuItem::with_id(
            REDO_ID,
            "Redo",
            true,
            Some("cmdorctrl+shift+z".parse::<Accelerator>()?),
        );
        let edit_menu = Submenu::with_items(
            "Edit",
            true,
            &[
                &undo_item,
                &redo_item,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::cut(None),
                &PredefinedMenuItem::copy(None),
                &PredefinedMenuItem::paste(None),
                &PredefinedMenuItem::select_all(None),
            ],
        )?;

        let window_menu = Submenu::with_items(
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(None),
                &PredefinedMenuItem::fullscreen(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::bring_all_to_front(None),
            ],
        )?;

        menu.append_items(&[&app_menu, &file_menu, &edit_menu, &window_menu])?;
        Ok(menu)
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::NativeMenuCommand;

    pub struct NativeMenu;

    impl NativeMenu {
        pub fn install() -> Option<Self> {
            None
        }

        pub fn next_command(&self) -> Option<NativeMenuCommand> {
            None
        }
    }
}

pub use platform::NativeMenu;
