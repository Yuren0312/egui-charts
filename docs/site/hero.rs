//! The live chart on the project site: the chart widget alone, on the demo's synthetic sample data.

use std::sync::Arc;

use eframe::egui;
use egui_charts::model::{BarData, Timeframe};
use egui_charts::theme::{Theme, apply_to_egui};
use egui_charts::widget::Chart;

#[path = "../../examples/demo/sampledata.rs"]
#[allow(dead_code)]
mod sampledata;

pub struct HeroApp {
    chart: Chart,
    /// The colour scheme the theme was last applied for.
    dark: Option<bool>,
}

impl HeroApp {
    pub fn new(ctx: &egui::Context) -> Self {
        ctx.set_fonts(fonts());
        let bars = sampledata::generate_sample_bars("SAMPLE", 600, Timeframe::Hour1);
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_visible_bars(120);
        chart.set_symbol("SAMPLE");
        chart.set_timeframe(Timeframe::Hour1);
        Self { chart, dark: None }
    }
}

/// egui's proportional font for every family. The chart draws no monospace text, and egui's
/// monospace font would add about 150 kB to the compressed download.
fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert(
        "Ubuntu-Light".to_owned(),
        Arc::new(egui::FontData::from_static(
            epaint_default_fonts::UBUNTU_LIGHT,
        )),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .insert(family, vec!["Ubuntu-Light".to_owned()]);
    }
    fonts
}

impl eframe::App for HeroApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // egui follows the system colour scheme; apply the matching preset to egui and to the
        // chart when it changes.
        let dark = ui.ctx().theme() == egui::Theme::Dark;
        if self.dark != Some(dark) {
            let theme = if dark { Theme::dark() } else { Theme::light() };
            apply_to_egui(ui.ctx(), &theme);
            ui.set_style(ui.ctx().global_style());
            let config = self.chart.config_mut();
            *config = theme.apply_to_config(config.clone());
            self.dark = Some(dark);
        }
        self.chart.show(ui);
    }
}
