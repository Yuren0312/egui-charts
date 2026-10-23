//! Data Window component
//!
//! Displays real-time OHLCV data at the cursor position.
//! Collapsible section at the top of the Object Tree panel.

use egui::{RichText, Ui};

use crate::theme::ThemeExt;

use super::types::DataWindowInfo;
use crate::ext::UiExt;
use crate::scales::PriceFormat;
use crate::tokens::DESIGN_TOKENS;

/// Render the Data Window section
///
/// Shows OHLCV data at cursor position in a compact grid layout.
pub fn show_data_window(ui: &mut Ui, data: &DataWindowInfo, expanded: bool) -> bool {
    let mut is_expanded = expanded;

    // Header with collapse button
    ui.horizontal(|ui| {
        // Collapse/expand button
        let icon = if is_expanded { "v" } else { ">" };
        if ui.small_button(icon).clicked() {
            is_expanded = !is_expanded;
        }

        // Title
        ui.strong_label("Data Window");

        // Symbol badge
        ui.right_aligned(|ui| {
            if !data.symbol.is_empty() {
                ui.label(
                    RichText::new(&data.symbol)
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            }
        });
    });

    // Content (if expanded)
    if is_expanded {
        ui.indent("data_window_content", |ui| {
            ui.space_xs();

            // Timestamp and timeframe row
            ui.horizontal(|ui| {
                if !data.timestamp.is_empty() {
                    ui.hint_label(&data.timestamp);
                }
                if !data.timeframe.is_empty() {
                    ui.hint_label(&data.timeframe);
                }
            });

            ui.space_xs();

            // OHLCV grid
            show_ohlcv_grid(ui, data);

            ui.space_sm();
        });
    }

    is_expanded
}

/// Render the OHLCV grid
fn show_ohlcv_grid(ui: &mut Ui, data: &DataWindowInfo) {
    let change_color = if data.is_bullish() {
        ui.active_theme().chart.candle_up
    } else {
        ui.active_theme().chart.candle_down
    };

    let format = data.price_format;
    // A zero price is a window without a bar.
    let price = |price: f64| {
        if price == 0.0 {
            "\u{2014}".to_string()
        } else {
            format.price(price)
        }
    };
    egui::Grid::new("ohlcv_grid")
        .num_columns(4)
        .spacing([DESIGN_TOKENS.spacing.md, DESIGN_TOKENS.spacing.xs])
        .min_col_width(50.0)
        .show(ui, |ui| {
            // Row 1: Open, High
            ui.hint_label("O");
            ui.label(price(data.open));
            ui.hint_label("H");
            ui.label(price(data.high));
            ui.end_row();

            // Row 2: Low, Close
            ui.hint_label("L");
            ui.label(price(data.low));
            ui.hint_label("C");
            ui.label(RichText::new(price(data.close)).color(change_color));
            ui.end_row();

            // Row 3: Volume, Change
            ui.hint_label("Vol");
            ui.label(format_volume(data.volume, format));
            ui.hint_label("Chg");
            let change = if data.close == 0.0 {
                "\u{2014}".to_string()
            } else {
                format!(
                    "{:+.decimals$} ({:+.2}%)",
                    data.change,
                    data.change_percent,
                    decimals = format.decimals_of(data.close),
                )
            };
            ui.label(RichText::new(change).color(change_color).small());
            ui.end_row();
        });
}

/// Format volume with K/M/B suffixes from a thousand up, and with the volume decimals of
/// `format` below
fn format_volume(volume: f64, format: PriceFormat) -> String {
    if volume == 0.0 {
        return "\u{2014}".to_string();
    }

    if volume >= 1_000_000_000.0 {
        format!("{:.2}B", volume / 1_000_000_000.0)
    } else if volume >= 1_000_000.0 {
        format!("{:.2}M", volume / 1_000_000.0)
    } else if volume >= 1_000.0 {
        format!("{:.2}K", volume / 1_000.0)
    } else {
        format.volume(volume)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_volume() {
        let format = PriceFormat::default();
        assert_eq!(format_volume(1_500_000_000.0, format), "1.50B");
        assert_eq!(format_volume(2_500_000.0, format), "2.50M");
        assert_eq!(format_volume(3_500.0, format), "3.50K");
        assert_eq!(format_volume(500.0, format), "500");
        // No bar under the crosshair.
        assert_eq!(format_volume(0.0, format), "\u{2014}");
    }
}
