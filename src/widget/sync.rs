//! Multi-chart crosshair and time-scale synchronization.

use super::Chart;
use crate::model::ChartPoint;
use chrono::{DateTime, Utc};

impl Chart {
    /// The point under the pointer over the main pane in the last frame: the open time of the bar
    /// slot under it, past the last bar at the chart's timeframe per bar, and the price at its height.
    /// `None` when the pointer is anywhere else, the axes included.
    pub fn crosshair(&self) -> Option<ChartPoint> {
        self.crosshair
    }

    /// Draws a crosshair on the bar whose slot holds `time` while the pointer is not over this chart,
    /// to follow another chart's [`crosshair`](Self::crosshair). `None` removes it.
    pub fn set_synced_crosshair(&mut self, time: Option<DateTime<Utc>>) {
        self.synced_crosshair = time;
    }

    /// The times at the left and right edges of the main pane in the last frame; `None` without bars.
    pub fn visible_time_range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let index = self.time_index()?;
        let range = self.state.time_scale().visible_logical_range();
        Some((
            index.time_at(f64::from(range.left)),
            index.time_at(f64::from(range.right)),
        ))
    }

    /// Scrolls and zooms the main pane to span `from` to `to` on this chart's own bars, at the width
    /// of the last frame. Ignored without bars, before the first frame, or unless `to` is after `from`;
    /// the bar spacing limits and fixed edges still apply.
    pub fn set_visible_time_range(&mut self, from: DateTime<Utc>, to: DateTime<Utc>) {
        if !self.last_rendered_price_rect.is_positive() {
            return;
        }
        let Some(index) = self.time_index() else {
            return;
        };
        let (left, right) = (index.position(from) as f32, index.position(to) as f32);
        let time_scale = self.state.time_scale_mut();
        let width = time_scale.width();
        if right <= left || width <= 0.0 {
            return;
        }
        let base = time_scale.base_idx() as f32;
        time_scale.set_bar_spacing(width / (right - left + 1.0));
        time_scale.set_right_offset(right - base);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData};
    use crate::widget::test_frames::frame;

    /// Days 20 and 59 of a daily series are bars 40 and 118 of this 12-hour series.
    /// The edges must retain those times after the next frame lays out the chart.
    #[test]
    fn a_time_range_spans_the_same_times_on_another_interval() {
        let time = |s| DateTime::<Utc>::from_timestamp(s, 0).unwrap();
        let bars = (0..200)
            .map(|i| {
                Bar::new(
                    time(1_700_000_000 + i * 43_200),
                    100.0,
                    200.0,
                    100.0,
                    200.0,
                    1.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let ctx = egui::Context::default();
        frame(&ctx, &mut chart, 0.0, vec![]);
        let (from, to) = (time(1_701_728_000), time(1_705_097_600));
        chart.set_visible_time_range(from, to);
        frame(&ctx, &mut chart, 0.1, vec![]);
        let (left, right) = chart.visible_time_range().unwrap();
        for (got, want) in [(left, from), (right, to)] {
            assert!(
                (got - want).num_seconds().abs() <= 60,
                "got {got}, want {want}"
            );
        }
    }
}
