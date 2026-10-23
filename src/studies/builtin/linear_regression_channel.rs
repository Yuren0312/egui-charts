use super::clamp_param;
use crate::model::Bar;
use crate::studies::math::Moments;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Linear regression channel fitted to the closes of the last `period` bars.
///
/// Outputs upper, middle and lower lines across that window, with no value on
/// earlier bars or until a full window is available. The bands are `multiplier`
/// residual standard deviations from the line, using an n - 1 denominator.
/// Every calculation or update refits the whole channel. A window containing
/// a non-finite close has no values.
#[derive(Clone)]
pub struct LinearRegressionChannel {
    period: usize,
    multiplier: f64,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl LinearRegressionChannel {
    /// Create a channel with a period of at least 2. `multiplier` is clamped
    /// into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period: period.max(2),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.indicators.bb_upper,
                DESIGN_TOKENS.semantic.indicators.bb_middle,
                DESIGN_TOKENS.semantic.indicators.bb_lower,
            ],
            visible: true,
        }
    }
}

impl Default for LinearRegressionChannel {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for LinearRegressionChannel {
    fn name(&self) -> &str {
        "LinReg Channel"
    }

    fn desc(&self) -> &str {
        "Linear Regression Channel - Best fit line and residual deviation bands over the last bars"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();
        self.values.resize(data.len(), IndicatorValue::None);
        if data.len() < self.period {
            return;
        }
        let start = data.len() - self.period;
        let window = &data[start..];
        if window.iter().any(|bar| !bar.close.is_finite()) {
            return;
        }
        let Some(fit) = Moments::of(window).fit() else {
            return;
        };
        let offset = self.multiplier * fit.sigma;
        for (i, value) in self.values[start..].iter_mut().enumerate() {
            let middle = fit.intercept + fit.slope * i as f64;
            *value = IndicatorValue::Multiple(vec![middle + offset, middle, middle - offset]);
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        } else if !colors.is_empty() {
            self.colors = vec![colors[0]; 3];
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
        3
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("LinReg Upper({}, {})", self.period, self.multiplier),
            format!("LinReg Middle({})", self.period),
            format!("LinReg Lower({}, {})", self.period, self.multiplier),
        ]
    }
}
