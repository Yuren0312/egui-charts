//! Object tree rows and routing. Study ids stay private to this combined id space.

use super::*;
use crate::ui::widget_bar::panels::object_tree::{DataWindowInfo, ObjectTreeAction, SourceItem};
use crate::widget::SeriesId;

const INDICATOR_ID_BASE: usize = 1_000_000;

impl Terminal {
    pub(super) fn build_source_items(&self) -> Vec<SourceItem> {
        let mut items = vec![SourceItem::data_source(
            0,
            format!("{} · {}", self.chart.symbol(), self.timeframe().as_str()),
        )];
        for (id, study) in self.chart.indicators() {
            let mut item = SourceItem::indicator(
                INDICATOR_ID_BASE + id.0,
                study.name(),
                study
                    .colors()
                    .first()
                    .copied()
                    .unwrap_or(egui::Color32::GRAY),
            );
            item.visible = study.is_visible();
            items.push(item);
        }
        for d in &self.chart.drawings().drawings {
            let mut item = SourceItem::drawing(d.id, d.tool_type, d.color32());
            item.visible = d.visible;
            item.locked = d.locked;
            items.push(item);
        }
        items
    }

    pub(super) fn build_data_window(&self) -> DataWindowInfo {
        let bars = self.chart.bars(SeriesId::MAIN);
        let index = self
            .chart
            .get_hover_bar_idx()
            .map(|i| i.round().max(0.0) as usize)
            .unwrap_or(bars.len().saturating_sub(1));
        let mut info =
            DataWindowInfo::new(self.chart.symbol()).with_timeframe(self.timeframe().as_str());
        if let Some(bar) = bars.get(index) {
            let formatter = crate::scales::TimeFormatterBuilder::new()
                .with_24_hour(true)
                .with_seconds(true)
                .with_timezone(self.chart.chart_options().time_scale.timezone.clone())
                .build();
            info = info
                .with_ohlcv(bar.open, bar.high, bar.low, bar.close, bar.volume)
                .with_timestamp(crate::chart::renderers::time_label(
                    bar.time,
                    self.timeframe().duration_ms() / 1000,
                    formatter.as_ref(),
                ));
            if let Some(previous) = index.checked_sub(1).and_then(|i| bars.get(i)) {
                info = info.with_prev_close(previous.close);
            }
        }
        info
    }

    pub(super) fn route_object(&self, action: ObjectTreeAction, commands: &mut Vec<Command>) {
        use Command as C;
        use DrawingCommand as D;
        use ObjectTreeAction as A;
        commands.push(match action {
            A::ToggleVisibility(id)
            | A::Delete(id)
            | A::RemoveIndicator(id)
            | A::ChangeColor(id, _)
            | A::Select(id) => {
                if let Some(study) = id.checked_sub(INDICATOR_ID_BASE).map(SeriesId) {
                    match action {
                        A::ToggleVisibility(_) => C::SetStudyVisible(
                            study,
                            !self
                                .chart
                                .indicators()
                                .find(|(id, _)| *id == study)
                                .is_some_and(|(_, i)| i.is_visible()),
                        ),
                        A::ChangeColor(_, color) => C::SetStudyColor(study, color),
                        A::Select(_) => C::Drawing(D::Select(None)),
                        _ => C::RemoveStudy(study),
                    }
                } else {
                    let Some(d) = self.chart.drawings().drawings.iter().find(|d| d.id == id) else {
                        return;
                    };
                    C::Drawing(match action {
                        A::ToggleVisibility(_) => D::SetVisible(id, !d.visible),
                        A::ChangeColor(_, color) => D::SetColor(id, color),
                        A::Select(_) => D::Select(Some(id)),
                        _ => D::Delete(id),
                    })
                }
            }
            A::ToggleLock(id) => {
                let Some(d) = self.chart.drawings().drawings.iter().find(|d| d.id == id) else {
                    return;
                };
                C::Drawing(D::SetLocked(id, !d.locked))
            }
            A::ClearSelection => C::Drawing(D::Select(None)),
            A::OpenProperties(id) => C::Open(Dialog::DrawingProperties(id)),
            A::Duplicate(id) => C::Drawing(D::Clone(id)),
            A::BringToFront(id) => C::Drawing(D::BringToFront(id)),
            A::SendToBack(id) => C::Drawing(D::SendToBack(id)),
            A::MoveUp(id) => C::Drawing(D::MoveUp(id)),
            A::MoveDown(id) => C::Drawing(D::MoveDown(id)),
            A::ZoomTo(id) => C::Drawing(D::ZoomTo(id)),
            A::ShowAll => C::Drawing(D::SetAllVisible(true)),
            A::HideAll => C::Drawing(D::SetAllVisible(false)),
            A::LockAll => C::Drawing(D::SetAllLocked(true)),
            A::UnlockAll => C::Drawing(D::SetAllLocked(false)),
            A::RemoveAllDrawings => C::Drawing(D::RemoveAll),
            A::RemoveAllIndicators => C::RemoveAllStudies,
            _ => return, // Multi-selection, rename and indicator settings are hidden.
        });
    }
}
