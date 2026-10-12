//! Core market types shared across terminal-rs: bars, timeframes, technical
//! indicators and a deterministic sample-data generator.

pub mod bar;
pub mod indicators;
pub mod sample;
pub mod timeframe;

pub use bar::{Bar, Series};
pub use timeframe::Timeframe;
