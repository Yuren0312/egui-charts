//! Commands emitted by terminal controls and performed by the terminal.

use crate::chart::series::SeriesSettings;
use crate::drawings::DrawingToolType;
use crate::model::{ChartType, DateRange, Timeframe};
use crate::studies::Indicator;
use crate::theme::Theme;
use crate::ui::dialogs::DrawingProps;
use crate::ui::drawing_toolbar::{categories::cursors::CursorType, state::MagnetType};
use crate::ui::export::ExportFormat;
use crate::ui::top_toolbar::components::settings_menu::data::ChartSettingsState;
use crate::widget::SeriesId;
use chrono::{DateTime, Utc};
use egui::Color32;

/// What a terminal control asks for. The host may intercept it before it is performed.
#[derive(Clone)]
#[non_exhaustive]
pub enum Command {
    /// Load a symbol from the feed.
    SetSymbol(String),
    /// Load another interval.
    SetTimeframe(Timeframe),
    /// Change the price renderer.
    SetChartType(ChartType),
    /// Apply the chart settings dialog.
    ApplyChartSettings(Box<ChartSettingsState>),
    /// Apply the price series settings.
    ApplySeriesSettings(SeriesId, SeriesSettings),
    /// Apply and retain a theme for this session.
    SetTheme(Theme),
    /// Change the visible range.
    View(ViewCommand),
    /// Add the study created with its configured parameters.
    AddStudy(Box<dyn Indicator>),
    /// Remove one study.
    RemoveStudy(SeriesId),
    /// Remove every study.
    RemoveAllStudies,
    /// Set one study's visibility.
    SetStudyVisible(SeriesId, bool),
    /// Set every study's visibility.
    SetAllStudiesVisible(bool),
    /// Set the first line colour of a study.
    SetStudyColor(SeriesId, Color32),
    /// Change drawings or drawing input.
    Drawing(DrawingCommand),
    /// Undo a drawing edit.
    Undo,
    /// Redo a drawing edit.
    Redo,
    /// Open a dialog.
    Open(Dialog),
    /// Open a side panel, or close it.
    ShowPanel(Option<Panel>),
    /// Copy text to the clipboard.
    CopyText(String),
    /// Copy the chart image to the clipboard.
    Screenshot,
    /// Export the chart through the host or clipboard.
    Export(ExportFormat),
    /// Set the viewport fullscreen state.
    SetFullscreen(bool),
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SetSymbol(a) => f.debug_tuple("SetSymbol").field(a).finish(),
            Self::SetTimeframe(a) => f.debug_tuple("SetTimeframe").field(a).finish(),
            Self::SetChartType(a) => f.debug_tuple("SetChartType").field(a).finish(),
            Self::ApplyChartSettings(a) => f.debug_tuple("ApplyChartSettings").field(a).finish(),
            Self::ApplySeriesSettings(a, b) => f
                .debug_tuple("ApplySeriesSettings")
                .field(a)
                .field(b)
                .finish(),
            Self::SetTheme(a) => f.debug_tuple("SetTheme").field(a).finish(),
            Self::View(a) => f.debug_tuple("View").field(a).finish(),
            Self::AddStudy(a) => f.debug_tuple("AddStudy").field(&a.name()).finish(),
            Self::RemoveStudy(a) => f.debug_tuple("RemoveStudy").field(a).finish(),
            Self::RemoveAllStudies => f.write_str("RemoveAllStudies"),
            Self::SetStudyVisible(a, b) => {
                f.debug_tuple("SetStudyVisible").field(a).field(b).finish()
            }
            Self::SetAllStudiesVisible(a) => {
                f.debug_tuple("SetAllStudiesVisible").field(a).finish()
            }
            Self::SetStudyColor(a, b) => f.debug_tuple("SetStudyColor").field(a).field(b).finish(),
            Self::Drawing(a) => f.debug_tuple("Drawing").field(a).finish(),
            Self::Undo => f.write_str("Undo"),
            Self::Redo => f.write_str("Redo"),
            Self::Open(a) => f.debug_tuple("Open").field(a).finish(),
            Self::ShowPanel(a) => f.debug_tuple("ShowPanel").field(a).finish(),
            Self::CopyText(a) => f.debug_tuple("CopyText").field(a).finish(),
            Self::Screenshot => f.write_str("Screenshot"),
            Self::Export(a) => f.debug_tuple("Export").field(a).finish(),
            Self::SetFullscreen(a) => f.debug_tuple("SetFullscreen").field(a).finish(),
        }
    }
}

/// A change to the visible chart range.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ViewCommand {
    /// Reset the view.
    Reset,
    /// Enable box zoom.
    ZoomIn,
    /// Restore the previous zoom.
    ZoomOut,
    /// Centre on the bar nearest a time, loading older history to reach it.
    GoTo(DateTime<Utc>),
    /// Show an estimated number of bars for a range.
    ShowRange(DateRange),
    /// Show the loaded bars from the first time up to the second, exclusive.
    ShowTimes(DateTime<Utc>, DateTime<Utc>),
}

/// A drawing edit or input mode.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum DrawingCommand {
    /// Select a drawing tool.
    SelectTool(Option<DrawingToolType>),
    /// Set the cursor mode.
    SetCursor(CursorType),
    /// Set magnet mode and strength, or disable it.
    SetMagnet(Option<MagnetType>),
    /// Set eraser mode.
    SetEraser(bool),
    /// Keep drawing after completing a shape.
    SetStayInDrawing(bool),
    /// Set the colour for new drawings.
    SetDefaultColor(Color32),
    /// Place this icon or emoji with the next click.
    InsertIcon(String),
    /// Select a drawing or clear selection.
    Select(Option<usize>),
    /// Delete a drawing.
    Delete(usize),
    /// Duplicate a drawing with an offset.
    Clone(usize),
    /// Set drawing visibility.
    SetVisible(usize, bool),
    /// Set the drawing lock.
    SetLocked(usize, bool),
    /// Bring a drawing to the front.
    BringToFront(usize),
    /// Send a drawing to the back.
    SendToBack(usize),
    /// Move one step forward.
    MoveUp(usize),
    /// Move one step backward.
    MoveDown(usize),
    /// Centre on the first anchor.
    ZoomTo(usize),
    /// Apply the drawing properties dialog.
    ApplyProps(usize, DrawingProps),
    /// Set a drawing colour.
    SetColor(usize, Color32),
    /// Remove all unlocked drawings.
    RemoveAll,
    /// Set every drawing's visibility.
    SetAllVisible(bool),
    /// Set every drawing's lock.
    SetAllLocked(bool),
}

/// A terminal dialog.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Dialog {
    /// Open symbol search.
    SymbolSearch,
    /// Open the study catalogue.
    Indicators,
    /// Open chart settings.
    ChartSettings,
    /// Open series settings.
    SeriesSettings(SeriesId),
    /// Open drawing properties.
    DrawingProperties(usize),
    /// Open export.
    Export,
    /// Open the date picker.
    GoToDate,
    /// Open keyboard help.
    Shortcuts,
}

/// A terminal side panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Panel {
    /// The chart's studies and drawings.
    ObjectTree,
}
