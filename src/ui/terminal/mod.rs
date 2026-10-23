//! An assembled chart with toolbars, dialogs and commands. One terminal per viewport.

mod command;
mod keys;
mod layout;
mod objects;
mod route;
mod run;

pub use command::{Command, Dialog, DrawingCommand, Panel, ViewCommand};

use crate::data::{Datafeed, inbox::Inbox};
use crate::model::Timeframe;
use crate::theme::Theme;
use crate::ui::Control;
use crate::ui::context_menu::ChartMenus;
use crate::ui::dialogs::{DrawingPropertiesDialog, KeyboardShortcutsDialog, SeriesSettingsDialog};
use crate::ui::drawing_toolbar::DrawingToolbar;
use crate::ui::export::{ExportDialog, ExportFormat};
use crate::ui::timeframe_toolbar::{DatePicker, TimeframeToolbar};
use crate::ui::top_toolbar::TopToolbar;
use crate::ui::top_toolbar::components::settings_menu::{SettingsDialog, data::ChartSettingsState};
use crate::ui::top_toolbar::components::{SymbolSearchDialog, indicator_dialog::IndicatorDialog};
use crate::ui::widget_bar::{
    WidgetBar,
    panels::object_tree::{ObjectTreeConfig, ObjectTreePanel},
};
use crate::ui_kit::notifications::Toasts;
use crate::{Chart, ChartBuilder};
use std::path::PathBuf;
use std::sync::Arc;

/// A finished export. The host chooses how to save it; the library writes no files.
pub struct ExportFile {
    /// Suggested file name.
    pub name: String,
    /// Destination entered in the export dialog, if any.
    pub path: Option<PathBuf>,
    /// Image pixels or CSV text.
    pub contents: ExportContents,
}

/// Contents passed to the host's export callback.
pub enum ExportContents {
    /// The chart's rectangle, in physical pixels.
    Image(Arc<egui::ColorImage>),
    /// CSV text.
    Text(String),
}

type Intercept = Box<dyn FnMut(&Command) -> bool>;
type Save = Box<dyn FnMut(ExportFile)>;

/// A chart whose toolbars, dialogs and menus perform their actions by default.
///
/// The Icons/Emojis menu draws emoji: enable egui's `monochrome_emoji_fonts` feature or install an emoji font.
pub struct Terminal {
    chart: Chart,
    theme: Theme,
    ctx: Option<egui::Context>,
    pending: Vec<Command>,
    intercept: Option<Intercept>,
    inbox: Arc<Inbox>,
    toasts: Toasts,
    formats: &'static [ExportFormat],
    save: Option<Save>,
    export_path: Option<PathBuf>,
    colour_drag: bool,
    chart_rect: egui::Rect,
    menus: ChartMenus,
    top_toolbar: TopToolbar,
    drawing_toolbar: DrawingToolbar,
    timeframe_toolbar: TimeframeToolbar,
    widget_bar: WidgetBar,
    indicator_dialog: IndicatorDialog,
    settings_dialog: SettingsDialog,
    settings_state: ChartSettingsState,
    symbol_search: SymbolSearchDialog,
    series_settings: SeriesSettingsDialog,
    drawing_props: DrawingPropertiesDialog,
    keyboard_shortcuts: KeyboardShortcutsDialog,
    export_dialog: ExportDialog,
    date_picker: DatePicker,
    object_tree: ObjectTreePanel,
    open_panel: Option<Panel>,
}

impl Terminal {
    /// A terminal over `feed` showing the symbol set with `with_symbol`, or an empty chart
    /// with the symbol search open.
    pub fn new(feed: Arc<dyn Datafeed>) -> Self {
        let mut terminal = Self {
            chart: ChartBuilder::new()
                .with_symbol("")
                .with_timeframe(Timeframe::Day1)
                .with_feed(feed)
                .build(),
            theme: Theme::dark(),
            ctx: None,
            pending: Vec::new(),
            intercept: None,
            inbox: Arc::new(Inbox::default()),
            toasts: Toasts::new(),
            formats: &[],
            save: None,
            export_path: None,
            colour_drag: false,
            chart_rect: egui::Rect::NOTHING,
            menus: ChartMenus::default(),
            top_toolbar: TopToolbar::new(),
            drawing_toolbar: DrawingToolbar::default(),
            timeframe_toolbar: TimeframeToolbar::default(),
            widget_bar: WidgetBar::default(),
            indicator_dialog: IndicatorDialog::new(),
            settings_dialog: SettingsDialog::new(),
            settings_state: ChartSettingsState::default(),
            symbol_search: SymbolSearchDialog::new(),
            series_settings: SeriesSettingsDialog::new(),
            drawing_props: DrawingPropertiesDialog::new(),
            keyboard_shortcuts: KeyboardShortcutsDialog::new(),
            export_dialog: ExportDialog::new().with_formats(&[ExportFormat::Clipboard]),
            date_picker: DatePicker::new(),
            object_tree: ObjectTreePanel::with_config(ObjectTreeConfig {
                enable_multi_select: false,
                enable_inline_edit: false,
                enable_drag_drop: false,
                enable_keyboard_shortcuts: false,
                enable_indicator_settings: false,
                enable_data_source_actions: false,
                ..Default::default()
            }),
            open_panel: None,
        };
        terminal.symbol_search.open();
        for control in [
            Control::Alerts,
            Control::Trading,
            Control::Replay,
            Control::Compare,
            Control::Layouts,
            Control::Templates,
            Control::TimeZone,
            Control::Session,
            Control::DividendAdjustment,
        ] {
            terminal = terminal.hide(control);
        }
        terminal
    }

    /// Sets the initial symbol.
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.chart.set_symbol(symbol);
        if !self.chart.symbol().is_empty() {
            self.symbol_search.close();
        }
        self
    }

    /// Sets the initial interval, which defaults to one day.
    pub fn with_timeframe(mut self, timeframe: Timeframe) -> Self {
        self.chart.set_timeframe(timeframe);
        self
    }

    /// Sets the theme applied on the first frame. Defaults to the dark theme.
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Returns commands matching `intercept` from `show` instead of performing them.
    pub fn with_intercept(mut self, intercept: impl FnMut(&Command) -> bool + 'static) -> Self {
        self.intercept = Some(Box::new(intercept));
        self
    }

    /// Offers these formats alongside the clipboard and hands completed exports to `save`.
    pub fn with_file_export(
        mut self,
        formats: &'static [ExportFormat],
        save: impl FnMut(ExportFile) + 'static,
    ) -> Self {
        self.formats = formats;
        self.save = Some(Box::new(save));
        let mut offered = formats.to_vec();
        if !offered.contains(&ExportFormat::Clipboard) {
            offered.push(ExportFormat::Clipboard);
        }
        self.export_dialog = ExportDialog::new().with_formats(&offered);
        self
    }

    /// Hides a control and all of its entries in the terminal's components.
    pub fn hide(mut self, control: Control) -> Self {
        self.top_toolbar.set_control_visible(control, false);
        self.drawing_toolbar.set_control_visible(control, false);
        self.timeframe_toolbar.set_control_visible(control, false);
        self.widget_bar.set_control_visible(control, false);
        self.menus.set_control_visible(control, false);
        self.settings_dialog.set_control_visible(control, false);
        self
    }

    /// The chart on screen.
    pub fn chart(&self) -> &Chart {
        &self.chart
    }

    /// Customises the chart, its studies and drawings.
    pub fn chart_mut(&mut self) -> &mut Chart {
        &mut self.chart
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HistoryPage, HistoryRequest, Reply};
    use crate::drawings::DrawingToolType;
    use crate::drawings::manager::DrawingSyncAction;
    use crate::theme::ThemeExt;
    use crate::ui::dialogs::DrawingProps;
    use crate::widget::SeriesId;
    use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect, Vec2};

    struct Feed(bool);
    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            if self.0 {
                reply.send(Ok(HistoryPage {
                    bars: crate::widget::test_frames::chart(&[10.0], &[12.0])
                        .bars(SeriesId::MAIN)
                        .to_vec(),
                    end_of_history: true,
                }));
            }
        }
    }

    fn frame(
        ctx: &egui::Context,
        terminal: &mut Terminal,
        time: f64,
        events: Vec<Event>,
    ) -> Vec<Command> {
        let mut returned = Vec::new();
        ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0))),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| returned.extend(terminal.show(ui)),
        )
        .drop_without_applying_deltas();
        returned
    }

    fn add_drawing(terminal: &mut Terminal) {
        terminal
            .chart_mut()
            .drawings_mut()
            .start_drawing_with_coords(
                DrawingToolType::HorizontalLine,
                Pos2::new(10.0, 11.0),
                |x| x,
                f64::from,
            );
    }

    #[test]
    fn intercepted_commands_are_returned_not_performed() {
        for dialog in [
            Dialog::SymbolSearch,
            Dialog::Indicators,
            Dialog::ChartSettings,
            Dialog::SeriesSettings(SeriesId::MAIN),
            Dialog::DrawingProperties(0),
            Dialog::Export,
            Dialog::GoToDate,
            Dialog::Shortcuts,
        ] {
            let ctx = egui::Context::default();
            let mut terminal = Terminal::new(Arc::new(Feed(false))).with_symbol("TEST");
            add_drawing(&mut terminal);
            terminal.run(Command::Open(dialog));
            // Each dialog draws over a chart with no bars.
            for tick in 0..3 {
                frame(&ctx, &mut terminal, tick as f64, vec![]);
            }
        }
        let z = |modifiers| Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        for intercept in [true, false] {
            let ctx = egui::Context::default();
            let mut terminal = Terminal::new(Arc::new(Feed(true)))
                .with_symbol("TEST")
                .with_intercept(move |command| intercept && matches!(command, Command::Undo));
            terminal.run(Command::SetTheme(Theme::light()));
            frame(&ctx, &mut terminal, 0.0, vec![]);
            assert!(!ctx.active_theme().dark);
            frame(&ctx, &mut terminal, 1.0, vec![]);
            assert_eq!(terminal.chart().bars(SeriesId::MAIN).len(), 100);
            add_drawing(&mut terminal);
            let commands = frame(&ctx, &mut terminal, 2.0, vec![z(Modifiers::COMMAND)]);
            if intercept {
                assert!(matches!(commands.as_slice(), [Command::Undo]));
                assert_eq!(terminal.chart().drawings().drawings.len(), 1);
                terminal.run(Command::Undo);
            } else {
                assert!(commands.is_empty());
            }
            assert!(terminal.chart().drawings().drawings.is_empty());
        }
        // egui matches Command+Z with Shift held too, so redo is matched first.
        let ctx = egui::Context::default();
        let mut terminal = Terminal::new(Arc::new(Feed(false)))
            .with_symbol("TEST")
            .with_intercept(|_| true);
        frame(&ctx, &mut terminal, 0.0, vec![]);
        let shifted = frame(
            &ctx,
            &mut terminal,
            1.0,
            vec![z(Modifiers::COMMAND | Modifiers::SHIFT)],
        );
        assert!(matches!(shifted.as_slice(), [Command::Redo]));
    }

    /// A drawing edit run as a command is saved and undone like an edit on the chart: it queues
    /// one update for the host to store, and Undo restores the drawing.
    #[test]
    fn drawing_commands_are_saved_and_undone() {
        let props = DrawingProps {
            line_width: 3.0,
            ..Default::default()
        };
        for edit in [
            DrawingCommand::SetVisible(0, false),
            DrawingCommand::SetLocked(0, true),
            DrawingCommand::SetColor(0, egui::Color32::RED),
            DrawingCommand::ApplyProps(0, props),
        ] {
            let ctx = egui::Context::default();
            let mut terminal = Terminal::new(Arc::new(Feed(false))).with_symbol("TEST");
            add_drawing(&mut terminal);
            frame(&ctx, &mut terminal, 0.0, vec![]);
            let fields = |t: &Terminal| {
                let d = &t.chart().drawings().drawings[0];
                (d.visible, d.locked, d.color, d.stroke_width)
            };
            let before = fields(&terminal);
            let sync = &terminal.chart().drawings().pending_sync;
            sync.lock().unwrap().clear();
            terminal.run(Command::Drawing(edit.clone()));
            frame(&ctx, &mut terminal, 1.0, vec![]);
            let queued =
                std::mem::take(&mut *terminal.chart().drawings().pending_sync.lock().unwrap());
            assert!(
                matches!(queued.as_slice(), [DrawingSyncAction::UpdateDrawing(0)]),
                "{edit:?}: {queued:?}"
            );
            terminal.run(Command::Undo);
            assert_eq!(fields(&terminal), before, "{edit:?}");
        }
    }
}
