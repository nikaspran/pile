mod app;
mod command;
mod command_palette;
mod editor;
mod model;
mod native_menu;
mod persistence;
mod search;
mod syntax;
mod tab_switcher;

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
