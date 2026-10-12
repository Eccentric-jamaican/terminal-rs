//! GPU chart rendering for terminal-rs.
//!
//! - [`viewport`]: bar-index/price <-> pixel math, pan and zoom.
//! - [`lod`]: level-of-detail pyramids that bound per-frame work to the plot width.
//! - [`gpu`]: wgpu pipelines (candles, OHLC bars, volume, lines, areas, histograms)
//!   and the egui paint-callback adapter.
//! - [`scale`]: axis ticks and labels. [`drawings`]: user drawing model and hit testing.

pub mod drawings;
pub mod headless;
pub mod gpu;
pub mod lod;
pub mod scene;
pub mod scale;
pub mod viewport;

pub use egui_wgpu::wgpu;
