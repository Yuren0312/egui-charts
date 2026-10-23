//! Translate component actions into semantic commands in emission order.

use super::*;
use crate::chart::renderers::price_decimals;
use crate::ui::context_menu::{ChartContextAction, ContextMenuAction};
use crate::ui::dialogs::{
    DrawingContextMenuAction, DrawingPropertiesAction, SeriesContextMenuAction,
    SeriesSettingsAction,
};
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::export::ExportResponse;
use crate::ui::timeframe_toolbar::{DatePickerAction, TimeframeToolbarAction};
use crate::ui::top_toolbar::TopToolbarAction;
use crate::ui::top_toolbar::components::indicator_dialog::IndicatorDialogAction;
use crate::ui::top_toolbar::components::settings_menu::SettingsAction;
use crate::ui::widget_bar::{RightBarIcon, WidgetBarAction};
use crate::widget::SeriesId;
use Command as C;
use DrawingCommand as D;

impl Terminal {
    pub(super) fn route_top(&self, action: TopToolbarAction, commands: &mut Vec<Command>) {
        commands.push(match action {
            TopToolbarAction::IntervalSelected(tf) => C::SetTimeframe(tf),
            TopToolbarAction::ChartStyleSelected(name) => {
                let Some(kind) = chart_type_from_name(&name) else {
                    return;
                };
                C::SetChartType(kind)
            }
            TopToolbarAction::OpenSymbolSearch => C::Open(Dialog::SymbolSearch),
            TopToolbarAction::OpenIndicators => C::Open(Dialog::Indicators),
            TopToolbarAction::OpenSettings => C::Open(Dialog::ChartSettings),
            TopToolbarAction::TakeScreenshot => C::Screenshot,
            TopToolbarAction::Undo => C::Undo,
            TopToolbarAction::Redo => C::Redo,
            TopToolbarAction::ThemeToggle => C::SetTheme(if self.theme.dark {
                Theme::light()
            } else {
                Theme::dark()
            }),
            TopToolbarAction::ToggleFullscreen => C::SetFullscreen(
                !self
                    .ctx
                    .as_ref()
                    .is_some_and(|ctx| ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))),
            ),
            // These controls are hidden until their services are available.
            TopToolbarAction::OpenCompare
            | TopToolbarAction::CreateAlert
            | TopToolbarAction::ToggleReplay
            | TopToolbarAction::OpenIndicatorTemplates
            | TopToolbarAction::Save
            | TopToolbarAction::Layout(_)
            | TopToolbarAction::None => return,
        });
    }

    pub(super) fn route_drawing(&self, action: DrawingToolbarAction, commands: &mut Vec<Command>) {
        use DrawingToolbarAction as A;
        let dm = self.chart.drawings();
        let command = match action {
            A::SelectTool(tool) => C::Drawing(D::SelectTool(Some(tool))),
            A::SetCursorType(cursor) => C::Drawing(D::SetCursor(cursor)),
            A::ToggleMagnet => C::Drawing(D::SetMagnet(
                (!dm.options.magnet_mode).then_some(self.drawing_toolbar.state.magnet_type),
            )),
            A::SetMagnetType(kind) => C::Drawing(D::SetMagnet(Some(kind))),
            A::ToggleEraserMode => C::Drawing(D::SetEraser(!dm.eraser_mode)),
            A::ToggleStayInDrawingMode => C::Drawing(D::SetStayInDrawing(!dm.stay_in_drawing_mode)),
            A::ColorChanged(color) => C::Drawing(D::SetDefaultColor(
                egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]),
            )),
            A::InsertEmoji { icon_name } => C::Drawing(D::InsertIcon(icon_name.into())),
            A::ClearAllDrawings | A::RemoveAllDrawings => C::Drawing(D::RemoveAll),
            A::HideAllDrawings => {
                C::Drawing(D::SetAllVisible(!dm.drawings.iter().all(|d| d.visible)))
            }
            A::LockAllDrawings => {
                C::Drawing(D::SetAllLocked(!dm.drawings.iter().all(|d| d.locked)))
            }
            A::HideAllIndicators => {
                C::SetAllStudiesVisible(!self.chart.indicators().all(|(_, i)| i.is_visible()))
            }
            A::RemoveAllIndicators | A::RemoveAllStudies => C::RemoveAllStudies,
            A::ZoomIn => C::View(ViewCommand::ZoomIn),
            A::ZoomOut => C::View(ViewCommand::ZoomOut),
            A::None
            | A::ToggleFavoritesToolbar
            | A::ToggleValuesTooltip
            | A::OpenTemplateMenu
            | A::SaveTemplate { .. }
            | A::LoadTemplate { .. }
            | A::HideAllPoss
            | A::HideAllOrders => return,
        };
        commands.push(command);
    }

    pub(super) fn route_bottom(&self, action: TimeframeToolbarAction, commands: &mut Vec<Command>) {
        commands.push(match action {
            TimeframeToolbarAction::DateRangeChanged(range) => {
                C::View(ViewCommand::ShowRange(range))
            }
            TimeframeToolbarAction::OpenDatePicker => C::Open(Dialog::GoToDate),
            _ => return, // Time zone, session and adjustment controls are hidden.
        });
    }

    pub(super) fn route_widget(&self, action: WidgetBarAction, commands: &mut Vec<Command>) {
        commands.push(match action {
            WidgetBarAction::Clicked(RightBarIcon::Help) => C::Open(Dialog::Shortcuts),
            WidgetBarAction::Clicked(RightBarIcon::ObjectTree) => {
                C::ShowPanel(if self.open_panel == Some(Panel::ObjectTree) {
                    None
                } else {
                    Some(Panel::ObjectTree)
                })
            }
            _ => return,
        });
    }

    pub(super) fn route_menu(&self, action: ChartContextAction, commands: &mut Vec<Command>) {
        use ContextMenuAction as A;
        use DrawingContextMenuAction as M;
        use SeriesContextMenuAction as S;
        commands.push(match action {
            ChartContextAction::Chart(action) => match action {
                A::CopyPrice(price) => self.copy_price(price),
                A::OpenSettings => C::Open(Dialog::ChartSettings),
                A::OpenSeriesSettings => C::Open(Dialog::SeriesSettings(SeriesId::MAIN)),
                A::ObjectTree => C::ShowPanel(Some(Panel::ObjectTree)),
                A::RemoveAllIndicators => C::RemoveAllStudies,
                A::ResetChart => C::View(ViewCommand::Reset),
                A::ExportCsv => C::Export(ExportFormat::Csv),
                _ => return,
            },
            ChartContextAction::Series(action) => {
                let (id, price) = self.menus.series_context();
                match action {
                    S::OpenSettings => C::Open(Dialog::SeriesSettings(id)),
                    S::AddIndicator => C::Open(Dialog::Indicators),
                    S::CopyPrice => self.copy_price(price),
                    _ => return,
                }
            }
            ChartContextAction::Drawing(action) => {
                let Some(id) = self.chart.drawings().sel_drawing() else {
                    return;
                };
                let Some(drawing) = self.chart.drawings().drawings.iter().find(|d| d.id == id)
                else {
                    return;
                };
                match action {
                    M::Edit => C::Open(Dialog::DrawingProperties(id)),
                    M::Delete => C::Drawing(D::Delete(id)),
                    M::Clone => C::Drawing(D::Clone(id)),
                    M::Lock => C::Drawing(D::SetLocked(id, !drawing.locked)),
                    M::Hide => C::Drawing(D::SetVisible(id, !drawing.visible)),
                    M::BringToFront => C::Drawing(D::BringToFront(id)),
                    M::SendToBack => C::Drawing(D::SendToBack(id)),
                    M::None => return,
                }
            }
        });
    }

    fn copy_price(&self, price: f64) -> Command {
        let decimals = self
            .chart
            .instrument()
            .and_then(|i| i.price_decimals)
            .map_or_else(|| price_decimals(price), usize::from);
        C::CopyText(format!("{price:.decimals$}"))
    }

    pub(super) fn show_dialogs(&mut self, ctx: &egui::Context, commands: &mut Vec<Command>) {
        let dialog_open = self.symbol_search.is_open()
            || self.indicator_dialog.is_open()
            || self.settings_dialog.is_open
            || self.series_settings.is_open
            || self.drawing_props.is_open
            || self.export_dialog.is_open()
            || self.date_picker.is_open
            || self.keyboard_shortcuts.is_open;
        if let IndicatorDialogAction::AddIndicator(study) =
            self.indicator_dialog.show(ctx, &self.theme)
        {
            commands.push(C::AddStudy(study));
        }
        if let SettingsAction::Apply(state) = self.settings_dialog.show(ctx, &self.theme) {
            commands.push(C::ApplyChartSettings(Box::new(state)));
        }
        if let Some(symbol) = self.symbol_search.show(ctx, &[]) {
            commands.push(C::SetSymbol(symbol.name));
        }
        if let SeriesSettingsAction::Apply(id, settings) = self.series_settings.show(ctx) {
            commands.push(C::ApplySeriesSettings(id, settings));
        }
        match self.drawing_props.show(ctx) {
            DrawingPropertiesAction::Apply(id, props) => {
                commands.push(C::Drawing(D::ApplyProps(id.0, props)))
            }
            DrawingPropertiesAction::Delete(id) => commands.push(C::Drawing(D::Delete(id.0))),
            DrawingPropertiesAction::Clone(id) => commands.push(C::Drawing(D::Clone(id.0))),
            _ => {}
        }
        self.keyboard_shortcuts.show_with_history(ctx, true);
        if let ExportResponse::Export { config, save_path } = self.export_dialog.show(ctx) {
            self.export_path = save_path;
            commands.push(C::Export(config.format));
        }
        match self.date_picker.show(ctx) {
            DatePickerAction::GoToDate(date) => commands.push(C::View(ViewCommand::GoTo(
                date.and_time(chrono::NaiveTime::MIN).and_utc(),
            ))),
            DatePickerAction::SetRange(a, b) => commands.push(C::View(ViewCommand::ShowTimes(
                a.and_time(chrono::NaiveTime::MIN).and_utc(),
                b.succ_opt()
                    .unwrap_or(b)
                    .and_time(chrono::NaiveTime::MIN)
                    .and_utc(),
            ))),
            _ => {}
        }
        if dialog_open {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
    }
}

fn chart_type_from_name(name: &str) -> Option<crate::model::ChartType> {
    use crate::model::ChartType;
    match name.to_lowercase().as_str() {
        "bars" => Some(ChartType::Bars),
        "candles" | "candlestick" => Some(ChartType::Candles),
        "hollow candles" | "hollow" => Some(ChartType::HollowCandles),
        "line" => Some(ChartType::Line),
        "line with markers" => Some(ChartType::LineWithMarkers),
        "step line" => Some(ChartType::StepLine),
        "area" => Some(ChartType::Area),
        "hlc area" => Some(ChartType::HlcArea),
        "baseline" => Some(ChartType::Baseline),
        "columns" => Some(ChartType::Columns),
        "heikin ashi" | "heikin" => Some(ChartType::Heikin),
        "renko" => Some(ChartType::Renko),
        "kagi" => Some(ChartType::Kagi),
        "line break" => Some(ChartType::LineBreak),
        "point and figure" | "point & figure" => Some(ChartType::PointAndFigure),
        "range" => Some(ChartType::Range),
        "high-low" | "high low" => Some(ChartType::HighLow),
        "volume candles" => Some(ChartType::VolumeCandles),
        "time price opportunity" | "tpo" | "market profile" => {
            Some(ChartType::TimePriceOpportunity)
        }
        _ => None,
    }
}
