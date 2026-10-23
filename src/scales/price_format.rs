//! The decimals a chart writes the prices and volumes of its instrument with.

#[cfg(feature = "widget")]
use crate::model::Bar;
use crate::model::Instrument;

/// How a chart writes the prices and volumes of its instrument: with the instrument's price
/// decimals, else those of its tick size, and its volume decimals. Prices without known decimals
/// are written with decimals chosen by magnitude, and volumes without them with none for a whole
/// number and 2 otherwise.
///
/// `Chart::price_format` gives the format of a chart, for labels the application draws next to
/// it; without the instrument's decimals it takes those its bars are quoted with. It is the
/// chart's precision only: [`PriceFormatter`](super::PriceFormatter) and its kin write values in
/// other notations.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PriceFormat {
    /// Decimals of prices.
    pub decimals: Option<usize>,
    /// Smallest price increment: positive and finite.
    pub tick_size: Option<f64>,
    /// Decimals of volumes.
    pub volume_decimals: Option<usize>,
}

impl PriceFormat {
    /// The format of `instrument`. A tick size that is not positive and finite is ignored.
    pub fn of(instrument: &Instrument) -> Self {
        let tick_size = instrument
            .tick_size
            .filter(|tick| tick.is_finite() && *tick > 0.0);
        Self {
            decimals: (instrument.price_decimals.map(usize::from))
                .or_else(|| tick_size.map(tick_decimals)),
            tick_size,
            volume_decimals: instrument.volume_decimals.map(usize::from),
        }
    }

    /// Decimals of `price`: the instrument's, else 2 from 100 up, 4 from 1 up and 6 below.
    pub fn decimals_of(&self, price: f64) -> usize {
        self.decimals.unwrap_or(if price.abs() >= 100.0 {
            2
        } else if price.abs() >= 1.0 {
            4
        } else {
            6
        })
    }

    /// `price` written with [`decimals_of`](Self::decimals_of) it.
    pub fn price(&self, price: f64) -> String {
        let decimals = self.decimals_of(price);
        format!("{price:.decimals$}")
    }

    /// `price` rounded to the nearest multiple of the tick size, else to the decimals, else
    /// unchanged.
    pub fn round_to_tick(&self, price: f64) -> f64 {
        // Adding 0.0 turns a negative zero into zero, which is written without a sign.
        match (self.tick_size, self.decimals) {
            (Some(tick), _) => (price / tick).round() * tick + 0.0,
            (None, Some(decimals)) => {
                let scale = 10f64.powi(decimals as i32);
                (price * scale).round() / scale + 0.0
            }
            (None, None) => price,
        }
    }

    /// `volume` written with the volume decimals, else with none for a whole number and 2
    /// otherwise.
    pub fn volume(&self, volume: f64) -> String {
        let fallback = if volume.fract() == 0.0 { 0 } else { 2 };
        let decimals = self.volume_decimals.unwrap_or(fallback);
        format!("{volume:.decimals$}")
    }
}

/// The decimals the prices of `bars` are quoted with: the most that any open, high, low or close
/// of the last 100 bars needs. `None` without bars, or when a price needs more than 8 decimals,
/// as computed or converted prices do: they are not quoted to any.
#[cfg(feature = "widget")]
pub(crate) fn decimals_in(bars: &[Bar]) -> Option<usize> {
    let recent = &bars[bars.len().saturating_sub(100)..];
    let prices = recent
        .iter()
        .flat_map(|bar| [bar.open, bar.high, bar.low, bar.close]);
    let mut most = None;
    for price in prices.filter(|price| price.is_finite()) {
        // Exact to a millionth of the last decimal: float error, not a finer quote.
        let fits = |decimals: &i32| {
            let scaled = price.abs() * 10f64.powi(*decimals);
            (scaled - scaled.round()).abs() <= 1e-6
        };
        most = most.max(Some((0..=8).find(fits)? as usize));
    }
    most
}

/// The fewest decimals, at most 12, that write `tick` exactly.
fn tick_decimals(tick: f64) -> usize {
    (0..12)
        .find(|&decimals| {
            let scaled = tick * 10f64.powi(decimals as i32);
            (scaled - scaled.round()).abs() <= scaled * 1e-9
        })
        .unwrap_or(12)
}
