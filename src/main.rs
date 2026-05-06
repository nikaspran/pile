mod app;
mod command;
mod command_palette;
mod editor;
mod grammar_registry;
mod model;
mod native_menu;
mod parse_worker;
mod persistence;
mod preferences;
mod search;
mod settings;
mod stress_tests;
mod syntax;
mod syntax_highlighting;
mod tab_switcher;
mod theme;

use anyhow::Result;

use eframe::egui;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pile=info,warn".into()),
        )
        .init();

    let settings_path = persistence::default_settings_path();
    let settings = persistence::load_settings(&settings_path);
    let window_state = &settings.window_state;

    let mut options = eframe::NativeOptions::default();

    if let Some(size) = window_state.size {
        options.viewport.inner_size = Some(egui::Vec2::new(size[0], size[1]));
    }
    if let Some(position) = window_state.position {
        options.viewport.position = Some(egui::Pos2::new(position[0], position[1]));
    }
    if let Some(fullscreen) = window_state.fullscreen {
        options.viewport.fullscreen = Some(fullscreen);
    }
    if let Some(maximized) = window_state.maximized {
        options.viewport.maximized = Some(maximized);
    }

    eframe::run_native(
        "pile",
        options,
        Box::new(|cc| Ok(Box::new(app::PileApp::new(cc)))),
    )
    .map_err(|err| anyhow::anyhow!("failed to run pile: {err}"))
}
