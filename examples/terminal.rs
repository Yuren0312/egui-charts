//! A terminal over the synthetic feed, with native CSV export.
//!
//! Run `cargo run --example terminal`; `--static` disables the live stream.

#[path = "live_feed.rs"]
pub mod live_feed;

use egui_charts::model::Timeframe;
use egui_charts::ui::export::ExportFormat;
use egui_charts::ui::terminal::{ExportContents, Terminal};
use std::sync::Arc;

/// Builds the terminal the example runs.
pub fn terminal(live: bool) -> Terminal {
    Terminal::new(Arc::new(live_feed::SyntheticFeed::with_stream(
        20_000, live,
    )))
    .with_symbol("SYNTH")
    .with_timeframe(Timeframe::Min1)
    .with_file_export(&[ExportFormat::Csv], |file| {
        if let ExportContents::Text(text) = file.contents {
            let path = file.path.unwrap_or_else(|| file.name.into());
            if let Err(error) = std::fs::write(&path, text) {
                eprintln!("Could not save {}: {error}", path.display());
            }
        }
    })
}

pub fn main() -> eframe::Result {
    let mut terminal = terminal(!std::env::args().any(|arg| arg == "--static"));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_ui_native("egui-charts: terminal", options, move |ui, _frame| {
        terminal.show(ui);
    })
}
