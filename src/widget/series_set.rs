//! Added bar series, their price scales and their placement on the main series' time axis.

use super::{Chart, SeriesId};
use crate::chart::renderers::{ChartMapping, RenderContext, StyleColors};
use crate::chart::rendering;
use crate::data::FeedState;
use crate::data::merge::{Merged, Origin, merge};
use crate::model::{Bar, BarClock, ChartType, PriceSource, next_revision, revise};
use crate::scales::{PriceMap, PriceScale, PriceScaleId, PriceTransform, scale_bounds};
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, Utc};
use egui::{Color32, Pos2, Rect};
use std::collections::BTreeSet;
use std::ops::Range;

/// How a series added with [`Chart::add_series`] is drawn. Set fields that differ from the
/// default with struct update syntax: `SeriesOptions { price_scale: PriceScaleId::Left,
/// ..Default::default() }`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeriesOptions {
    /// How each bar is drawn. Volume candles, TPO and the price-based types (Renko, Kagi,
    /// Line Break, Point and Figure, range bars) are drawn as a line. Line by default.
    pub chart_type: ChartType,
    /// The scale the series is drawn against. The right scale, shared with the main, by default.
    pub price_scale: PriceScaleId,
    /// The colour of lines and areas, and of rising bars.
    pub color: Color32,
    /// The colour of falling bars and of a baseline area below its base line.
    pub down_color: Color32,
}

impl Default for SeriesOptions {
    fn default() -> Self {
        Self {
            chart_type: ChartType::Line,
            price_scale: PriceScaleId::Right,
            color: DESIGN_TOKENS.semantic.extended.info,
            down_color: DESIGN_TOKENS.semantic.extended.error,
        }
    }
}

pub(super) struct Series {
    pub id: SeriesId,
    pub symbol: String,
    pub options: SeriesOptions,
    pub visible: bool,
    pub bars: Vec<Bar>,
    pub revision: u64,
    pub from_ticks: BTreeSet<DateTime<Utc>>,
    pub feed: FeedState,
    pub instrument: Option<crate::model::Instrument>,
    pub scale: PriceScale,
    pub transform: PriceTransform,
    pub map: PriceMap,
    pub drawn: Range<usize>,
}

impl Series {
    pub fn new(id: SeriesId, symbol: String, options: SeriesOptions) -> Self {
        let scale = PriceScale::new(Default::default());
        let transform = PriceTransform::default();
        let map = scale.map(transform, 0.0, 0.0);
        Self {
            id,
            symbol,
            options,
            visible: true,
            bars: Vec::new(),
            revision: next_revision(),
            from_ticks: BTreeSet::new(),
            feed: FeedState::default(),
            instrument: None,
            scale,
            transform,
            map,
            drawn: 0..0,
        }
    }

    pub fn apply(&mut self, clock: &BarClock, incoming: Vec<(Bar, Origin)>) -> Merged {
        let old_len = self.bars.len();
        let merged = merge(&mut self.bars, &mut self.from_ticks, clock, incoming);
        revise(
            &mut self.revision,
            old_len,
            self.bars.len(),
            merged.first_changed,
        );
        self.feed.rejected_bars += merged.rejected;
        merged
    }
}

impl Chart {
    pub(crate) fn series_symbol(&self, id: SeriesId) -> Option<&str> {
        if id == SeriesId::MAIN || id == SeriesId::VOLUME {
            Some(&self.symbol)
        } else {
            self.series
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.symbol.as_str())
        }
    }

    pub(super) fn render_series_legend(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        time: DateTime<Utc>,
    ) {
        if self.chart_type.is_time_independent() {
            return;
        }
        for (k, s) in self.series.iter().filter(|s| s.visible).enumerate() {
            let value = s
                .bars
                .partition_point(|bar| bar.time <= time)
                .checked_sub(1)
                .map(|i| {
                    let close = s.bars[i].close;
                    let decimals = s
                        .instrument
                        .as_ref()
                        .and_then(|i| i.price_decimals)
                        .map_or_else(
                            || crate::chart::renderers::price_decimals(close),
                            usize::from,
                        );
                    format!("{close:.decimals$}")
                });
            crate::chart::renderers::render_legend_row(
                painter,
                Pos2::new(
                    rect.min.x + crate::styles::sizing::chart::PADDING,
                    rect.min.y + 12.0 + 18.0 * (k + 1) as f32,
                ),
                &s.symbol,
                value.as_deref(),
                s.options.color,
                self.config.text_color,
            );
        }
    }

    pub(super) fn hit_test_added(
        &self,
        pos: Pos2,
        coords: &ChartMapping,
    ) -> Option<(SeriesId, usize)> {
        use crate::chart::hit_test::{HIT_TOLERANCE, point_to_segment_distance};
        if !coords.rect.contains(pos) || self.chart_type.is_time_independent() {
            return None;
        }
        let window = self.series_window();
        let main = &self.state.data().bars[window.clone()];
        for s in self
            .series
            .iter()
            .rev()
            .filter(|s| s.visible && !s.drawn.is_empty())
        {
            let x = |bar: &Bar| x_of(bar.time, main, window.start, coords);
            let bars = &s.bars[s.drawn.clone()];
            let hit = if matches!(
                drawn_type(s.options.chart_type),
                ChartType::Bars
                    | ChartType::Candles
                    | ChartType::HollowCandles
                    | ChartType::Heikin
                    | ChartType::HighLow
            ) {
                bars.iter().any(|bar| {
                    let (high, low) = (s.map.price_to_y(bar.high), s.map.price_to_y(bar.low));
                    (pos.x - x(bar)).abs() <= coords.bar_spacing * self.config.candle_width / 2.0
                        && (high.min(low)..=high.max(low)).contains(&pos.y)
                })
            } else {
                bars.windows(2).any(|pair| {
                    point_to_segment_distance(
                        pos,
                        Pos2::new(x(&pair[0]), s.map.price_to_y(pair[0].close)),
                        Pos2::new(x(&pair[1]), s.map.price_to_y(pair[1].close)),
                    ) <= HIT_TOLERANCE
                })
            };
            if hit {
                return Some((s.id, coords.x_to_idx(pos.x)));
            }
        }
        None
    }

    pub(super) fn render_added_selection(
        &self,
        painter: &egui::Painter,
        coords: &ChartMapping,
        id: SeriesId,
    ) {
        use crate::chart::series::selection_render::draw_selection_dot;
        use crate::chart::series::{SelectionHandleConfig, calculate_dot_interval};
        if self.chart_type.is_time_independent() {
            return;
        }
        let Some(s) = self.series.iter().find(|s| s.id == id && s.visible) else {
            return;
        };
        let config = SelectionHandleConfig {
            dot_interval: calculate_dot_interval(coords.bar_spacing),
            ..Default::default()
        };
        let window = self.series_window();
        let main = &self.state.data().bars[window.clone()];
        for i in s.drawn.clone().step_by(config.dot_interval) {
            let bar = &s.bars[i];
            let pos = Pos2::new(
                x_of(bar.time, main, window.start, coords),
                s.map.price_to_y(bar.close),
            );
            if coords.rect.contains(pos) {
                draw_selection_dot(painter, pos, &config);
            }
        }
    }

    pub(super) fn fit_scales(&mut self) -> PriceTransform {
        let (start, end) = self.state.visible_range();
        let mut right = (f64::INFINITY, f64::NEG_INFINITY);
        let mut left = right;
        let mut reference = None;
        if start < end && !self.chart_type.is_time_independent() {
            let main = &self.state.data().bars;
            let (t0, t1) = (main[start].time, main[end - 1].time);
            for s in self.series.iter_mut().filter(|s| s.visible) {
                let mode = match s.options.price_scale {
                    PriceScaleId::Right => self.state.price_scale().options().mode,
                    PriceScaleId::Left => self.left_scale.options().mode,
                    PriceScaleId::Overlay => s.scale.options().mode,
                };
                let base = s
                    .bars
                    .get(s.bars.partition_point(|b| b.time <= t0).saturating_sub(1))
                    .map_or(f64::NAN, |b| b.close);
                s.transform = PriceTransform::with_base(mode, base);
                let a = s.bars.partition_point(|b| b.time < t0);
                let b = s.bars.partition_point(|b| b.time <= t1);
                let bounds = scale_bounds(
                    s.bars[a..b].iter().flat_map(|b| [b.low, b.high]),
                    s.transform,
                );
                match s.options.price_scale {
                    PriceScaleId::Right => {
                        right = (right.0.min(bounds.0), right.1.max(bounds.1));
                    }
                    PriceScaleId::Left => {
                        reference.get_or_insert(s.transform);
                        left = (left.0.min(bounds.0), left.1.max(bounds.1));
                    }
                    PriceScaleId::Overlay => s.scale.fit(s.transform, bounds),
                }
            }
        }
        if let Some(reference) = reference {
            self.left_scale.fit(reference, left);
        }
        self.state.fit_price_scale_with(
            self.studies
                .indicators()
                .iter()
                .zip(&self.study_autoscale)
                .filter(|(_, autoscale)| **autoscale)
                .map(|(study, _)| study.as_ref()),
            self.main_visible,
            right,
        )
    }

    pub(super) fn map_series(&mut self, rect: Rect) {
        let window = self.series_window();
        let main = &self.state.data().bars;
        for s in &mut self.series {
            s.drawn = 0..0;
            if !s.visible || window.is_empty() || self.chart_type.is_time_independent() {
                continue;
            }
            let scale = match s.options.price_scale {
                PriceScaleId::Right => self.state.price_scale(),
                PriceScaleId::Left => &self.left_scale,
                PriceScaleId::Overlay => &s.scale,
            };
            s.map = scale.map(s.transform, rect.top(), rect.bottom());
            let a = s.bars.partition_point(|b| b.time < main[window.start].time);
            let b = s
                .bars
                .partition_point(|b| b.time <= main[window.end - 1].time);
            s.drawn = a..b;
        }
    }

    pub(super) fn series_window(&self) -> Range<usize> {
        let (start, end) = self.state.visible_range();
        if start == end {
            return 0..0;
        }
        start.saturating_sub(1)..(end + 1).min(self.state.data().len())
    }

    pub(super) fn render_added_series(
        &self,
        price_ctx: &RenderContext,
        coords: &ChartMapping,
        colors: &StyleColors,
    ) {
        let window = self.series_window();
        let main = &self.state.data().bars[window.clone()];
        let defaults = crate::model::ChartTypeParams::default();
        for s in self.series.iter().filter(|s| !s.drawn.is_empty()) {
            let colors = StyleColors {
                bullish: s.options.color,
                bearish: s.options.down_color,
                bullish_border: None,
                bearish_border: None,
                bullish_wick: None,
                bearish_wick: None,
                ..*colors
            };
            let ctx = rendering::CandleDataContext {
                price_ctx,
                volume_ctx: price_ctx,
                price_scale: &s.map,
                colors: &colors,
                visible_data: &s.bars[s.drawn.clone()],
                full_data: &s.bars,
                revision: s.revision,
                start_idx: s.drawn.start,
            };
            let params = rendering::ChartTypeParams::new(
                rendering::BarDimensions::new(
                    coords.bar_spacing * self.config.candle_width,
                    self.config.wick_width,
                ),
                rendering::VolumeSettings::new(false, 0.0),
                &defaults,
                rendering::TradingColors::new(s.options.color, s.options.down_color),
                rendering::CoordMapping::new(coords.rect.min.x),
                PriceSource::Close,
            );
            rendering::render_chart_type(
                drawn_type(s.options.chart_type),
                &ctx,
                &params,
                |j, _| x_of(s.bars[j].time, main, window.start, coords),
            );
        }
    }
}

fn drawn_type(chart_type: ChartType) -> ChartType {
    if chart_type.is_time_independent()
        || matches!(
            chart_type,
            ChartType::VolumeCandles | ChartType::TimePriceOpportunity
        )
    {
        ChartType::Line
    } else {
        chart_type
    }
}

/// Search only the main window; add the fraction in pixels, so long histories lose no slots
/// to the precision of an f32 index.
fn x_of(time: DateTime<Utc>, main: &[Bar], start: usize, coords: &ChartMapping) -> f32 {
    let i = main.partition_point(|b| b.time <= time).saturating_sub(1);
    let fraction = main.get(i + 1).map_or(0.0, |next| {
        (time - main[i].time).as_seconds_f64() / (next.time - main[i].time).as_seconds_f64()
    });
    coords.idx_to_x(start + i) + fraction as f32 * coords.bar_spacing
}

#[cfg(test)]
mod tests {
    use crate::model::{Bar, BarData, ChartType};
    use crate::scales::{PriceScaleId, PriceScaleMode};
    use crate::widget::test_frames::{frame, shapes};
    use crate::widget::{Chart, SeriesId, SeriesOptions};
    use chrono::{DateTime, Utc};
    use egui::{Color32, Context, Pos2, Shape};

    fn values(minutes: &[i64], closes: &[f64]) -> Vec<Bar> {
        minutes
            .iter()
            .zip(closes)
            .map(|(&minute, &close)| {
                let time = DateTime::<Utc>::from_timestamp(1_700_000_000 + minute * 60, 0).unwrap();
                Bar::new(time, close, close, close, close, 1.0)
            })
            .collect()
    }

    fn comparison() -> (Chart, SeriesId) {
        let mut chart = Chart::new(BarData::from_bars(values(
            &[0, 1, 2, 3, 6, 7],
            &[100.0, 110.0, 120.0, 130.0, 140.0, 150.0],
        )));
        chart.set_chart_type(ChartType::Line);
        chart.config_mut().show_volume = false;
        let id = chart.add_series(
            "A",
            SeriesOptions {
                color: Color32::from_rgb(1, 2, 3),
                ..Default::default()
            },
        );
        chart.set_data(
            id,
            values(
                &[-1, 0, 2, 3, 4, 5, 7, 8],
                &[190.0, 200.0, 240.0, 260.0, 270.0, 275.0, 300.0, 310.0],
            ),
        );
        (chart, id)
    }

    fn path(shapes: &[Shape], color: Color32) -> Vec<Pos2> {
        shapes
            .iter()
            .find_map(|shape| match shape {
                Shape::Path(path) if path.stroke.color == egui::epaint::ColorMode::Solid(color) => {
                    Some(path.points.clone())
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Bars between main sessions keep their elapsed-time fraction, and relative modes use
    /// each symbol's own base. The two outside bars are never drawn, including during replay.
    #[test]
    fn added_series_are_placed_by_time() {
        for mode in [PriceScaleMode::Normal, PriceScaleMode::Percentage] {
            let (mut chart, id) = comparison();
            chart.price_scale_mut(id).unwrap().options_mut().mode = mode;
            let ctx = Context::default();
            frame(&ctx, &mut chart, 0.0, Vec::new());
            let painted = shapes(&ctx, &mut chart, 0.02, Vec::new());
            let main = path(&painted, chart.config().bullish_color);
            let added = path(&painted, Color32::from_rgb(1, 2, 3));
            assert_eq!(main.len(), 6);
            assert_eq!(added.len(), 6, "{mode:?}");
            let s = main[1].x - main[0].x;
            let expected = [
                main[0].x,
                main[2].x,
                main[3].x,
                main[3].x + s / 3.0,
                main[3].x + 2.0 * s / 3.0,
                main[5].x,
            ];
            for (point, x) in added.iter().zip(expected) {
                assert!((point.x - x).abs() < 1e-3, "{mode:?}: {point:?}, x={x}");
            }
            if mode == PriceScaleMode::Percentage {
                for (a, m) in [(0, 0), (1, 2), (2, 3), (5, 5)] {
                    assert!(
                        (added[a].y - main[m].y).abs() < 1e-3,
                        "{a}: {:?} {:?}",
                        added[a],
                        main[m]
                    );
                }
            }
        }
    }

    /// Each axis fits only its shown series, and reads a changed scale assignment on the
    /// next frame. Bounds include 10% margins, worked from the extrema in this table.
    #[test]
    fn each_price_scale_fits_its_shown_series() {
        for (case, right, left, overlay) in [
            ("normal", (75.0, 255.0), (7.6, 12.4), Some((988.0, 1012.0))),
            (
                "percent",
                (-13.0, 23.0),
                (-110.0 / 3.0, 10.0 / 3.0),
                Some((988.0, 1012.0)),
            ),
            (
                "main hidden",
                (185.0, 245.0),
                (7.6, 12.4),
                Some((988.0, 1012.0)),
            ),
            (
                "A hidden",
                (88.0, 112.0),
                (7.6, 12.4),
                Some((988.0, 1012.0)),
            ),
            ("C moved", (-2.0, 1102.0), (7.6, 12.4), None),
            (
                "price based",
                (88.0, 112.0),
                (0.0, 100.0),
                Some((0.0, 100.0)),
            ),
        ] {
            let mut chart = Chart::new(BarData::from_bars(values(
                &[0, 1440, 2880, 4320],
                &[100.0, 110.0, 90.0, 105.0],
            )));
            let a = chart.add_series("A", SeriesOptions::default());
            let b = chart.add_series(
                "B",
                SeriesOptions {
                    price_scale: PriceScaleId::Left,
                    ..Default::default()
                },
            );
            let c = chart.add_series(
                "C",
                SeriesOptions {
                    price_scale: PriceScaleId::Overlay,
                    ..Default::default()
                },
            );
            chart.set_data(
                a,
                values(&[0, 1440, 2880, 4320], &[200.0, 240.0, 190.0, 210.0]),
            );
            chart.set_data(b, values(&[1440, 2880, 4320], &[12.0, 8.0, 11.0]));
            chart.set_data(
                c,
                values(&[0, 1440, 2880, 4320], &[1000.0, 1010.0, 990.0, 1005.0]),
            );
            match case {
                "percent" => {
                    for id in [a, b] {
                        chart.price_scale_mut(id).unwrap().options_mut().mode =
                            PriceScaleMode::Percentage;
                    }
                }
                "main hidden" => {
                    chart.set_series_visible(SeriesId::MAIN, false);
                }
                "A hidden" => {
                    chart.set_series_visible(a, false);
                }
                "C moved" => chart.series_options_mut(c).unwrap().price_scale = PriceScaleId::Right,
                "price based" => chart.set_chart_type(ChartType::Renko),
                _ => {}
            }
            let ctx = Context::default();
            frame(&ctx, &mut chart, 0.0, Vec::new());
            frame(&ctx, &mut chart, 0.02, Vec::new());
            for (id, expected) in [(a, Some(right)), (b, Some(left)), (c, overlay)] {
                if let Some((min, max)) = expected {
                    let range = chart.price_scale_mut(id).unwrap().range();
                    assert!(
                        (range.min - min).abs() < 1e-8 && (range.max - max).abs() < 1e-8,
                        "{case} {id:?}: {range:?}, expected {min}..{max}"
                    );
                }
            }
        }
    }
    /// Clicks use the added series' own scale, and hidden series cannot be selected.
    #[test]
    fn clicks_select_an_added_series_on_its_scale() {
        use crate::widget::{ChartElementId, RightClickTarget};
        use egui::{Event, Modifiers, PointerButton};
        for (hidden, button) in [
            (false, PointerButton::Primary),
            (true, PointerButton::Primary),
            (false, PointerButton::Secondary),
        ] {
            let (mut chart, id) = comparison();
            chart.series_options_mut(id).unwrap().price_scale = PriceScaleId::Left;
            let ctx = Context::default();
            frame(&ctx, &mut chart, 0.0, Vec::new());
            let painted = shapes(&ctx, &mut chart, 0.02, Vec::new());
            let main = path(&painted, chart.config().bullish_color);
            let rect = chart.get_rendered_price_rect();
            // Visible added closes 200..300, with 10% margins, give 190..310.
            let pos = Pos2::new(
                main[2].x,
                rect.max.y - (240.0 - 190.0) / 120.0 * rect.height(),
            );
            // Both series have the same relative shape; isolate the added line from the main.
            chart.set_series_visible(SeriesId::MAIN, false);
            if hidden {
                chart.set_series_visible(id, false);
            }
            frame(&ctx, &mut chart, 0.04, vec![Event::PointerMoved(pos)]);
            for (time, pressed) in [(0.06, true), (0.08, false)] {
                frame(
                    &ctx,
                    &mut chart,
                    time,
                    vec![Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    }],
                );
            }
            assert_eq!(
                chart.selected_element(),
                (!hidden).then_some(ChartElementId::Series(id)),
                "hidden={hidden}, {button:?}"
            );
            if button == PointerButton::Secondary {
                let Some(RightClickTarget::Element { id: hit, price, .. }) =
                    chart.take_right_click()
                else {
                    panic!("no series menu");
                };
                assert_eq!(hit, ChartElementId::Series(id));
                assert!(
                    (price - 240.0).abs() < 120.0 / f64::from(rect.height()),
                    "{price}"
                );
            }
        }
    }
}
