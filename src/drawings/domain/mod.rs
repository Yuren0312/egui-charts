//! Pure domain types for the drawing system.
//!
//! This layer contains the core data structures with minimal dependencies:
//!
//! - [`Drawing`] -- the central struct representing a drawing on the chart,
//!   with dual coordinates (screen + chart), styling, and tool-specific config.
//! - [`ChartPoint`] -- persistent `(time, price)` coordinate that survives
//!   pan/zoom operations.
//! - [`DrawingToolType`] -- enum of all drawing tool types with interaction
//!   mode, display name, and serialization support.
//! - [`DrawingInteractionMode`] -- how mouse input maps to point capture.
//! - Options types: [`LineStyle`], [`ArrowStyle`], [`FontWeight`],
//!   [`HandlePos`], [`TimeframeVisibility`], [`DrawingOptions`],
//!   [`FibonacciConfig`], [`FibonacciLevel`], [`VolumeProfileConfig`],
//!   [`RegressionTrendConfig`], [`PositionSize`].
//!
//! `Drawing` uses `egui` for screen coordinates and colours; `ChartPoint` uses chrono for time.

mod drawing;
mod options;
// tool_type is now a subdirectory with multiple modules
mod tool_type;

pub use crate::model::ChartPoint;
pub use drawing::Drawing;
pub use options::{
    ArrowStyle, DrawingOptions, FibonacciConfig, FibonacciLevel, FontWeight, HandlePos, LineStyle,
    PositionSize, RegressionTrendConfig, TimeframeVisibility, VolumeProfileConfig,
};
pub use tool_type::{DrawingInteractionMode, DrawingToolType, ParseToolTypeError};
