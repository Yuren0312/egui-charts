//! Frame order and panels inside the host's Ui.

use super::*;
use crate::config::CrosshairStyle;
use crate::ui::app_state::ChartAppState;
use crate::ui::drawing_toolbar::{DrawingToolbarDomainState, categories::cursors::CursorType};
use crate::ui::symbol_header::{OhlcData, SymbolHeader, SymbolInfo};
use crate::ui::widget_bar::RightBarIcon;
use crate::ui_kit::notifications::NotificationPanel;
use crate::widget::SeriesId;

struct ToolbarSnapshot<'a> {
    symbol: &'a str,
    timeframe: Timeframe,
    chart_type: crate::model::ChartType,
}
impl ChartAppState for ToolbarSnapshot<'_> {
    fn active_symbol(&self) -> &str {
        self.symbol
    }
    fn active_timeframe(&self) -> &Timeframe {
        &self.timeframe
    }
    fn chart_type(&self) -> crate::model::ChartType {
        self.chart_type
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn is_replay_active(&self) -> bool {
        false
    }
}

impl Terminal {
    /// Draws the terminal into `ui`, performs what its controls emit, and returns the
    /// intercepted commands in emission order.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Vec<Command> {
        let ctx = ui.ctx().clone();
        if self.ctx.is_none() {
            self.ctx = Some(ctx.clone());
            self.run(Command::SetTheme(self.theme.clone()));
            let repaint = ctx.clone();
            let viewport = ctx.viewport_id();
            self.inbox
                .set_waker(move || repaint.request_repaint_of(viewport));
            for command in std::mem::take(&mut self.pending) {
                self.run(command);
            }
        }
        ui.set_style(ctx.global_style());
        // A colour picker drag ends, as one edit, when the pointer is released.
        if self.colour_drag && !ctx.input(|i| i.pointer.any_down()) {
            self.colour_drag = false;
            self.chart.drawings_mut().end_drag_handle();
        }
        self.drain_inbox(&ctx);
        self.chart.pump(&ctx);
        let mut commands = Vec::new();
        self.show_dialogs(&ctx, &mut commands);
        self.handle_keys(&ctx, &mut commands);
        self.sync_controls();
        let bg = self.theme.ui.panel_bg;
        egui::Panel::top("egui_charts_terminal_top")
            .exact_size(crate::ui::top_toolbar::config::TOOLBAR_HEIGHT)
            .show_separator_line(false)
            .frame(panel_frame(bg))
            .show(ui, |ui| {
                let state = ToolbarSnapshot {
                    symbol: self.chart.symbol(),
                    timeframe: self.timeframe(),
                    chart_type: self.chart.chart_type(),
                };
                let action = self.top_toolbar.show_with_state(ui, &state);
                self.route_top(action, &mut commands);
            });
        egui::Panel::top("egui_charts_terminal_header")
            .exact_size(38.0)
            .frame(panel_frame(bg))
            .show(ui, |ui| {
                // There is no OHLC header until the feed has supplied a bar.
                if let Some(bar) = self.chart.bars(SeriesId::MAIN).last() {
                    let instrument = self.chart.instrument();
                    SymbolHeader::new(SymbolInfo {
                        symbol: self.chart.symbol().into(),
                        company_name: instrument
                            .and_then(|i| i.description.clone())
                            .unwrap_or_default(),
                        exchange: instrument
                            .and_then(|i| i.exchange.clone())
                            .unwrap_or_default(),
                        timeframe: self.timeframe().as_str().into(),
                    })
                    .with_ohlc(OhlcData::new(
                        bar.open, bar.high, bar.low, bar.close, bar.volume,
                    ))
                    .show(ui);
                }
            });
        egui::Panel::left("egui_charts_terminal_drawings")
            .exact_size(52.0)
            .resizable(false)
            .frame(panel_frame(bg))
            .show(ui, |ui| {
                let action = self.drawing_toolbar.show_with_action(ui);
                self.route_drawing(action, &mut commands);
            });
        egui::Panel::right("egui_charts_terminal_widgets")
            .exact_size(52.0)
            .resizable(false)
            .frame(panel_frame(bg))
            .show(ui, |ui| {
                let action = self.widget_bar.show(ui);
                self.route_widget(action, &mut commands);
            });
        if self.open_panel == Some(Panel::ObjectTree) {
            egui::Panel::right("egui_charts_terminal_objects")
                .default_size(280.0)
                .min_size(220.0)
                .resizable(true)
                .show(ui, |ui| {
                    let mut sources = self.build_source_items();
                    let data = self.build_data_window();
                    let action = self.object_tree.show(ui, Some(&data), &mut sources);
                    self.route_object(action, &mut commands);
                });
        }
        egui::Panel::bottom("egui_charts_terminal_bottom")
            .exact_size(32.0)
            .frame(panel_frame(bg))
            .show(ui, |ui| {
                let action = self.timeframe_toolbar.show(ui);
                self.route_bottom(action, &mut commands);
            });
        self.chart_rect = egui::CentralPanel::default()
            .frame(panel_frame(bg))
            .show(ui, |ui| {
                self.chart.show(ui);
            })
            .response
            .rect;
        for command in std::mem::take(&mut self.pending) {
            self.run(command);
        }
        if let Some(action) = self.menus.show(&ctx, &mut self.chart) {
            self.route_menu(action, &mut commands);
        }
        NotificationPanel::new().show_with_toasts(&ctx, &mut self.toasts);
        let mut intercepted = Vec::new();
        for command in commands {
            if self
                .intercept
                .as_mut()
                .is_some_and(|intercept| intercept(&command))
            {
                if matches!(command, Command::Export(_)) {
                    self.export_path = None;
                }
                intercepted.push(command);
            } else {
                self.run(command);
            }
        }
        ui.set_style(ctx.global_style());
        intercepted
    }

    pub(super) fn timeframe(&self) -> Timeframe {
        self.chart.timeframe().unwrap_or(Timeframe::Day1)
    }

    fn sync_controls(&mut self) {
        self.top_toolbar
            .timeframe_selector
            .set_model(self.timeframe());
        self.top_toolbar.chart_type_selector.current = self.chart.chart_type();
        let dm = self.chart.drawings();
        self.top_toolbar.state.can_undo = dm.can_undo();
        self.top_toolbar.state.can_redo = dm.can_redo();
        self.drawing_toolbar
            .sync_from_domain_state(&DrawingToolbarDomainState {
                sel_tool: dm.active_tool,
                magnet_mode: dm.options.magnet_mode,
                stay_in_drawing_mode: dm.stay_in_drawing_mode,
                drawing_color: dm.options.default_color,
            });
        self.drawing_toolbar.state.eraser_mode_enabled = dm.eraser_mode;
        self.drawing_toolbar.state.current_cursor_type = if dm.eraser_mode {
            CursorType::Eraser
        } else {
            match self.chart.chart_options().crosshair.style {
                CrosshairStyle::Full => CursorType::Cross,
                CrosshairStyle::Dot => CursorType::Dot,
                CrosshairStyle::Arrow => CursorType::Arrow,
            }
        };
        self.drawing_toolbar.state.has_zoom_history = self.chart.state().has_zoom_history();
        self.timeframe_toolbar.state_mut().date_picker_open = self.date_picker.is_open;
        self.widget_bar.active_icon = self
            .open_panel
            .map(|Panel::ObjectTree| RightBarIcon::ObjectTree);
    }
}

fn panel_frame(bg: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::NONE)
        .inner_margin(egui::Margin::ZERO)
}
