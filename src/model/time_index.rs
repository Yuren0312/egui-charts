//! Drawing anchors in time, and the mapping between times and a series' bar positions.

use super::{Bar, Timeframe};
use chrono::{DateTime, TimeDelta, Utc};

/// A drawing anchor: a time on the chart's time axis and a price. [`TimeIndex`] places the time
/// on the bars each frame, so the anchor stays on its bar when bars are loaded, inserted or
/// replaced by another interval's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartPoint {
    /// A bar's open time, or a time between or beyond the bars.
    pub time: DateTime<Utc>,
    /// Price level.
    pub price: f64,
}
impl ChartPoint {
    /// Creates an anchor at `time` and `price`.
    pub fn new(time: DateTime<Utc>, price: f64) -> Self {
        Self { time, price }
    }
}

/// Places times on a series' bars and back. Bar `i` is at position `i`; a time between two bars
/// is at its share of the time between their opens; a time before the first or after the last bar
/// is at one `timeframe` per bar. A bar's open time maps to its index and back exactly, other times
/// come back within a millisecond. `bars` are in time order, as a chart holds them; equal times
/// map to the last of them. Months extrapolate at the timeframe's 30 days.
#[derive(Debug, Clone, Copy)]
pub struct TimeIndex<'a> {
    bars: &'a [Bar],
    step: TimeDelta,
}

impl<'a> TimeIndex<'a> {
    /// `None` without bars.
    pub fn new(bars: &'a [Bar], timeframe: Timeframe) -> Option<Self> {
        (!bars.is_empty()).then(|| Self {
            bars,
            step: TimeDelta::milliseconds(timeframe.duration_ms()),
        })
    }

    /// The fractional bar position of `time`.
    pub fn position(&self, time: DateTime<Utc>) -> f64 {
        let next = self.bars.partition_point(|bar| bar.time <= time);
        let (at, span) = match next {
            0 => (0, self.step),
            n if n == self.bars.len() => (n - 1, self.step),
            n => (n - 1, self.bars[n].time - self.bars[n - 1].time),
        };
        at as f64 + (time - self.bars[at].time).as_seconds_f64() / span.as_seconds_f64()
    }

    /// The time at a fractional bar position, including positions outside the series.
    pub fn time_at(&self, position: f64) -> DateTime<Utc> {
        let last = self.bars.len() - 1;
        let (at, span) = if position <= 0.0 || position.is_nan() {
            (0, self.step)
        } else if position >= last as f64 {
            (last, self.step)
        } else {
            let at = position as usize;
            (at, self.bars[at + 1].time - self.bars[at].time)
        };
        let nanos = (position - at as f64) * span.as_seconds_f64() * 1e9;
        // Never panics: `as` saturates and the add is checked.
        self.bars[at]
            .time
            .checked_add_signed(TimeDelta::nanoseconds(nanos.round() as i64))
            .unwrap_or(if nanos < 0.0 {
                DateTime::<Utc>::MIN_UTC
            } else {
                DateTime::<Utc>::MAX_UTC
            })
    }

    /// The bar whose slot holds `time`: the last bar opening at or before it, and for the last
    /// bar only within one timeframe of its open.
    pub fn bar_containing(&self, time: DateTime<Utc>) -> Option<usize> {
        let at = self
            .bars
            .partition_point(|bar| bar.time <= time)
            .checked_sub(1)?;
        (at + 1 < self.bars.len() || time - self.bars[at].time < self.step).then_some(at)
    }

    /// The slot a time from another chart is drawn on: the bar that holds it, and in a gap of a
    /// missing bar or more (a night, a weekend) the first bar after the gap once the time is a
    /// timeframe past the bar before it. Before the first and after the last bar, one slot per
    /// timeframe.
    pub fn slot(&self, time: DateTime<Utc>) -> f64 {
        let next = self.bars.partition_point(|bar| bar.time <= time);
        if let Some(at) = next.checked_sub(1)
            && let Some(after) = self.bars.get(next)
            && time - self.bars[at].time >= self.step
            && after.time - self.bars[at].time >= self.step * 2
        {
            return next as f64;
        }
        self.position(time).floor()
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Bar, TimeIndex, Timeframe};
    use chrono::{TimeDelta, TimeZone, Utc};

    #[test]
    fn time_index_places_times_on_the_bars_and_back() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let bars = [0, 60, 120, 300]
            .map(|seconds| Bar::new(start + TimeDelta::seconds(seconds), 1.0, 1.0, 1.0, 1.0, 0.0));
        let index = TimeIndex::new(&bars, Timeframe::Min1).unwrap();
        for (seconds, position) in [
            (-90, -1.5),
            (0, 0.0),
            (30, 0.5),
            (120, 2.0),
            (210, 2.5),
            (300, 3.0),
            (330, 3.5),
            (420, 5.0),
        ] {
            let time = start + TimeDelta::seconds(seconds);
            assert_eq!(index.position(time), position, "{seconds} s");
            assert_eq!(index.time_at(position), time, "position {position}");
        }
        let third = start + TimeDelta::seconds(20);
        assert!((index.position(third) - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(index.time_at(index.position(third)), third);
        // The gap after bar 2 runs from 180 s to bar 3 at 300 s.
        for (millis, holds, slot) in [
            (-1, None, -1.0),
            (0, Some(0), 0.0),
            (59_999, Some(0), 0.0),
            (60_000, Some(1), 1.0),
            (179_999, Some(2), 2.0),
            (180_000, Some(2), 3.0),
            (200_000, Some(2), 3.0),
            (359_999, Some(3), 3.0),
            (360_000, None, 4.0),
        ] {
            let time = start + TimeDelta::milliseconds(millis);
            assert_eq!(index.bar_containing(time), holds, "{millis} ms");
            assert_eq!(index.slot(time), slot, "{millis} ms");
        }
        assert!(TimeIndex::new(&[], Timeframe::Min1).is_none());
    }
}
