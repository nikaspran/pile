mod app;
mod command;
mod command_palette;
mod editor;
mod grammar_registry;
mod model;
mod native_menu;
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

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pile=info,warn".into()),
        )
        .init();

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "pile",
        options,
        Box::new(|cc| Ok(Box::new(app::PileApp::new(cc)))),
    )
    .map_err(|err| anyhow::anyhow!("failed to run pile: {err}"))
}
