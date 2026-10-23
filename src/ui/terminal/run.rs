//! Default command handling.

use super::*;
use crate::chart::series::SeriesSettings;
use crate::config::CrosshairStyle;
use crate::data::FeedError;
use crate::drawings::LineStyle;
use crate::ui::dialogs::{DrawingId, DrawingLineStyle, DrawingProps};
use crate::ui::drawing_toolbar::categories::cursors::CursorType;
use crate::widget::SeriesId;

impl Terminal {
    /// Performs `command`, whatever the intercept predicate says. Before the first `show`,
    /// commands wait for the context; images also wait for the chart to be laid out.
    pub fn run(&mut self, command: Command) {
        let Some(ctx) = self.ctx.clone() else {
            self.pending.push(command);
            return;
        };
        if !self.chart_rect.is_positive()
            && matches!(
                command,
                Command::Screenshot | Command::Export(ExportFormat::Png | ExportFormat::Clipboard)
            )
        {
            self.pending.push(command);
            return;
        }
        match command {
            Command::SetSymbol(symbol) => self.chart.set_symbol(symbol),
            Command::SetTimeframe(tf) => self.chart.set_timeframe(tf),
            Command::SetChartType(kind) => self.chart.set_chart_type(kind),
            Command::ApplyChartSettings(state) => {
                state.apply_to(self.chart.config_mut());
                self.settings_state = *state;
            }
            Command::ApplySeriesSettings(_, settings) => {
                self.chart.apply_series_settings(&settings)
            }
            Command::SetTheme(theme) => {
                crate::theme::apply_to_egui(&ctx, &theme);
                let config = self.chart.config_mut();
                config.background_color = theme.chart.bg;
                config.text_color = theme.chart.axis_text;
                config.grid_color = theme.chart.grid_line;
                self.theme = theme;
            }
            Command::View(view) => self.run_view(view),
            Command::AddStudy(study) => {
                self.chart.add_indicator(study);
            }
            Command::RemoveStudy(id) => {
                self.chart.remove_series(id);
            }
            Command::RemoveAllStudies => {
                let ids: Vec<_> = self.chart.indicators().map(|(id, _)| id).collect();
                for id in ids {
                    self.chart.remove_series(id);
                }
            }
            Command::SetStudyVisible(id, visible) => {
                self.chart.set_indicator_visible(id, visible);
            }
            Command::SetAllStudiesVisible(visible) => {
                let ids: Vec<_> = self.chart.indicators().map(|(id, _)| id).collect();
                for id in ids {
                    self.chart.set_indicator_visible(id, visible);
                }
            }
            Command::SetStudyColor(id, color) => {
                if let Some(study) = self.chart.indicator_mut(id) {
                    study.set_color(0, color);
                }
            }
            Command::Drawing(drawing) => self.run_drawing(drawing),
            Command::Undo => {
                self.chart.drawings_mut().undo();
            }
            Command::Redo => {
                self.chart.drawings_mut().redo();
            }
            Command::Open(dialog) => self.open_dialog(dialog),
            Command::ShowPanel(panel) => self.open_panel = panel,
            Command::CopyText(text) => ctx.copy_text(text),
            Command::Screenshot => self.export(ExportFormat::Clipboard),
            Command::Export(format) => self.export(format),
            Command::SetFullscreen(fullscreen) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen))
            }
        }
        ctx.request_repaint();
    }

    fn run_view(&mut self, command: ViewCommand) {
        match command {
            ViewCommand::Reset => {
                self.chart.set_visible_bars(100);
                self.chart
                    .apply_synced_time_scale(self.chart.get_time_scale_state().0, 0.0);
                self.chart.price_scale_mut().reset();
            }
            ViewCommand::ZoomIn => self.chart.set_zoom_mode(true),
            ViewCommand::ZoomOut => {
                self.chart.state.pop_zoom_state();
            }
            ViewCommand::ShowRange(range) => self
                .chart
                .set_visible_bars(range.estimated_bars(self.timeframe()).max(10)),
            ViewCommand::GoTo(time) => self.chart.scroll_to_time(time),
            ViewCommand::ShowTimes(a, b) => {
                let bars = self.chart.bars(SeriesId::MAIN);
                let start = bars.partition_point(|bar| bar.time < a.min(b));
                let end = bars.partition_point(|bar| bar.time < a.max(b));
                self.chart
                    .set_visible_bars(end.saturating_sub(start).max(10));
                self.chart.set_start_idx(start);
            }
        }
    }

    fn run_drawing(&mut self, command: DrawingCommand) {
        use DrawingCommand as D;
        if let D::ZoomTo(id) = command {
            if let Some(point) = self
                .chart
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id == id)
                .and_then(|d| d.chart_points.first())
            {
                self.chart.set_start_idx(
                    (point.bar_idx.max(0.0) as usize)
                        .saturating_sub(self.chart.get_visible_bars() / 2),
                );
            }
            return;
        }
        if let D::SetCursor(cursor) = command {
            self.chart.set_crosshair_style(match cursor {
                CursorType::Cross => CrosshairStyle::Full,
                CursorType::Dot => CrosshairStyle::Dot,
                _ => CrosshairStyle::Arrow,
            });
        }
        let dm = self.chart.drawings_mut();
        match command {
            D::SelectTool(tool) => dm.set_active_tool(tool),
            D::SetCursor(cursor) => {
                dm.set_active_tool(None);
                dm.set_eraser_mode(cursor == CursorType::Eraser);
            }
            D::SetMagnet(magnet) => {
                dm.options.magnet_mode = magnet.is_some();
                if let Some(magnet) = magnet {
                    self.drawing_toolbar.state.magnet_type = magnet;
                }
                dm.update_snap_options();
            }
            D::SetEraser(on) => dm.set_eraser_mode(on),
            D::SetStayInDrawing(on) => dm.stay_in_drawing_mode = on,
            D::SetDefaultColor(color) => dm.options.default_color = color.to_array(),
            D::InsertIcon(icon) => dm.start_icon_insertion(icon),
            D::Select(Some(id)) => dm.select(id),
            D::Select(None) => dm.deselect(),
            D::Delete(id) => dm.delete_drawing(id),
            D::Clone(id) => {
                if let Some(mut copy) = dm.drawings.iter().find(|d| d.id == id).cloned() {
                    for point in &mut copy.chart_points {
                        point.bar_idx += 3.0;
                    }
                    dm.add_synced_drawing(copy);
                }
            }
            D::BringToFront(id) => dm.bring_to_front(id),
            D::SendToBack(id) => dm.send_to_back(id),
            D::MoveUp(id) => dm.bring_forward(id),
            D::MoveDown(id) => dm.send_backward(id),
            D::RemoveAll => dm.clear_all(),
            D::SetAllVisible(visible) => dm.set_all_visible(visible),
            D::SetAllLocked(locked) => dm.set_all_locked(locked),
            D::SetVisible(id, visible) => dm.edit_drawing(id, |d| d.visible = visible),
            D::SetLocked(id, locked) => dm.edit_drawing(id, |d| d.locked = locked),
            D::SetColor(id, color) => {
                dm.drag_edit_drawing(id, |d| d.color = color.to_array());
                self.colour_drag = true;
            }
            D::ApplyProps(id, props) => dm.edit_drawing(id, |drawing| {
                drawing.color = props.color.to_array();
                drawing.stroke_width = props.line_width;
                drawing.line_style = match props.line_style {
                    DrawingLineStyle::Solid => LineStyle::Solid,
                    DrawingLineStyle::Dashed => LineStyle::Dashed,
                    DrawingLineStyle::Dotted => LineStyle::Dotted,
                };
                drawing.extend_left = props.extend_left;
                drawing.extend_right = props.extend_right;
                if !props.label.is_empty() {
                    drawing.text = Some(props.label);
                }
            }),
            D::ZoomTo(_) => unreachable!(),
        }
    }

    fn open_dialog(&mut self, dialog: Dialog) {
        match dialog {
            Dialog::SymbolSearch => self.symbol_search.open(),
            Dialog::Indicators => self.indicator_dialog.open(),
            Dialog::ChartSettings => self.settings_dialog.open(self.settings_state.clone()),
            Dialog::SeriesSettings(id) => self.series_settings.open(id, SeriesSettings::default()),
            Dialog::DrawingProperties(id) => {
                if let Some(d) = self.chart.drawings().drawings.iter().find(|d| d.id == id) {
                    let props = DrawingProps {
                        color: d.color32(),
                        line_width: d.stroke_width,
                        label: d.text.clone().unwrap_or_default(),
                        ..Default::default()
                    };
                    self.drawing_props
                        .open(DrawingId(id), d.tool_type.as_str().into(), props);
                }
            }
            Dialog::Export => self.export_dialog.open(),
            Dialog::GoToDate => self.date_picker.open(),
            Dialog::Shortcuts => self.keyboard_shortcuts.open(),
        }
    }

    fn export(&mut self, format: ExportFormat) {
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        let path = self.export_path.take();
        if format == ExportFormat::Csv {
            let text = match self.chart.state().data().to_csv_string() {
                Ok(text) => text,
                Err(error) => {
                    self.toasts.error(format!("CSV export failed: {error}"));
                    self.export_dialog.mark_error(error.to_string());
                    return;
                }
            };
            if self.formats.contains(&format) {
                self.deliver_export(path, "csv", ExportContents::Text(text));
            } else {
                ctx.copy_text(text);
                self.toasts.success("CSV copied to clipboard");
            }
            self.export_dialog.mark_success();
        } else {
            let reply = self.inbox.reply::<ExportFile>(u64::from(
                format == ExportFormat::Png && self.formats.contains(&format),
            ));
            let rect = self.chart_rect;
            let pixels_per_point = ctx.pixels_per_point();
            let name = format!("{}.png", self.chart.symbol());
            ctx.request_screenshot(move |image| {
                let image = Arc::new(image.region(&rect, Some(pixels_per_point)));
                reply.send(Ok(ExportFile {
                    name,
                    path,
                    contents: ExportContents::Image(image),
                }));
            });
        }
    }

    fn deliver_export(&mut self, path: Option<PathBuf>, extension: &str, contents: ExportContents) {
        if let Some(save) = &mut self.save {
            save(ExportFile {
                name: format!("{}.{}", self.chart.symbol(), extension),
                path,
                contents,
            });
        }
    }

    pub(super) fn drain_inbox(&mut self, ctx: &egui::Context) {
        for (file, answer) in self.inbox.drain().answers {
            let Ok(answer) = answer.downcast::<Result<ExportFile, FeedError>>() else {
                continue;
            };
            match *answer {
                Ok(export) => {
                    if file == 1 {
                        if let Some(save) = &mut self.save {
                            save(export);
                        }
                    } else if let ExportContents::Image(image) = export.contents {
                        ctx.copy_image((*image).clone());
                        self.toasts.success("Chart copied to clipboard");
                    }
                    self.export_dialog.mark_success();
                }
                Err(error) => {
                    self.toasts.error(format!("Screenshot failed: {error}"));
                    self.export_dialog.mark_error(error.to_string());
                }
            }
        }
    }
}
