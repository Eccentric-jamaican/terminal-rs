//! Axis tick generation: "nice" price steps and calendar-aware time labels.

use chrono::{DateTime, Datelike, Timelike, Utc};
use tr_core::Timeframe;

/// Round step for ~`target` ticks across `[lo, hi]` (1, 2, 2.5, 5 x 10^n).
pub fn nice_step(lo: f64, hi: f64, target: usize) -> f64 {
    let span = (hi - lo).abs().max(f64::MIN_POSITIVE);
    let raw = span / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

/// Price ticks within `[lo, hi]`.
pub fn price_ticks(lo: f64, hi: f64, target: usize) -> Vec<f64> {
    let step = nice_step(lo, hi, target);
    let mut v = (lo / step).ceil() * step;
    let mut out = Vec::new();
    while v <= hi + step * 1e-9 && out.len() < 200 {
        out.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
        v += step;
    }
    out
}

/// Decimal places appropriate for a tick step.
pub fn decimals_for(step: f64) -> usize {
    if step >= 10.0 {
        0
    } else if step >= 0.01 {
        2
    } else {
        ((-step.log10()).ceil() as usize).min(8)
    }
}

pub fn format_price(p: f64, decimals: usize) -> String {
    format!("{p:.decimals$}")
}

/// Compact volume formatting: 37.88M, 101.8K.
pub fn format_volume(v: f64) -> String {
    match v.abs() {
        x if x >= 1e9 => format!("{:.2}B", v / 1e9),
        x if x >= 1e6 => format!("{:.2}M", v / 1e6),
        x if x >= 1e3 => format!("{:.1}K", v / 1e3),
        _ => format!("{v:.0}"),
    }
}

/// How significant a time boundary is; larger boundaries get labels first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Boundary {
    None,
    Minute,
    Hour,
    Day,
    Month,
    Year,
}

fn dt(ts: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(ts, 0).unwrap_or_default()
}

pub fn boundary(prev: i64, cur: i64) -> Boundary {
    let (a, b) = (dt(prev), dt(cur));
    if a.year() != b.year() {
        Boundary::Year
    } else if a.month() != b.month() {
        Boundary::Month
    } else if a.ordinal() != b.ordinal() {
        Boundary::Day
    } else if a.hour() != b.hour() {
        Boundary::Hour
    } else if a.minute() != b.minute() {
        Boundary::Minute
    } else {
        Boundary::None
    }
}

pub fn label(ts: i64, b: Boundary, tf: Timeframe) -> String {
    let d = dt(ts);
    match b {
        Boundary::Year => d.format("%Y").to_string(),
        Boundary::Month => d.format("%b").to_string(),
        Boundary::Day if tf.is_intraday() => d.format("%d").to_string(),
        Boundary::Day => d.format("%d").to_string(),
        _ => d.format("%H:%M").to_string(),
    }
}

/// Full timestamp for crosshair labels.
pub fn crosshair_label(ts: i64, tf: Timeframe) -> String {
    let d = dt(ts);
    if tf.is_intraday() { d.format("%a %d %b '%y %H:%M").to_string() } else { d.format("%a %d %b '%y").to_string() }
}

/// Choose labelled bars among `times[lo..hi]`, preferring higher boundaries, with at least
/// `min_gap_bars` between labels. Returns `(index, label)` pairs in ascending order.
pub fn time_ticks(times: &[i64], lo: usize, hi: usize, min_gap_bars: f64, tf: Timeframe) -> Vec<(usize, String)> {
    let hi = hi.min(times.len());
    if lo >= hi {
        return Vec::new();
    }
    let gap = min_gap_bars.max(1.0).ceil() as usize;
    let mut cands: Vec<(Boundary, usize)> =
        (lo.max(1)..hi).map(|i| (boundary(times[i - 1], times[i]), i)).filter(|(b, _)| *b > Boundary::None).collect();
    // Higher boundaries first; within a level, keep chronological order.
    cands.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut chosen: Vec<(usize, Boundary)> = Vec::new();
    for (b, i) in cands {
        if chosen.iter().all(|(j, _)| i.abs_diff(*j) >= gap) {
            chosen.push((i, b));
        }
        if chosen.len() > 64 {
            break;
        }
    }
    chosen.sort_by_key(|(i, _)| *i);
    chosen.into_iter().map(|(i, b)| (i, label(times[i], b, tf))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_steps() {
        assert_eq!(nice_step(0.0, 100.0, 10), 10.0);
        assert_eq!(nice_step(0.0, 23.0, 10), 2.5);
        assert_eq!(nice_step(330.0, 342.0, 8), 2.0);
        let t = price_ticks(331.3, 342.7, 8);
        assert_eq!(t.first(), Some(&332.0));
        assert_eq!(t.last(), Some(&342.0));
    }

    #[test]
    fn formatting() {
        assert_eq!(format_volume(37_881_199.0), "37.88M");
        assert_eq!(format_volume(101_820.0), "101.8K");
        assert_eq!(decimals_for(0.25), 2);
        assert_eq!(decimals_for(0.001), 3);
        assert_eq!(decimals_for(10.0), 0);
        assert_eq!(decimals_for(2.0), 2);
    }

    #[test]
    fn time_ticks_prefer_month_boundaries() {
        // Daily bars across 3 months.
        let start = 1_767_225_600; // 2026-01-01
        let times: Vec<i64> = (0..90).map(|i| start + i * 86_400).collect();
        let ticks = time_ticks(&times, 0, 90, 20.0, Timeframe::D1);
        let labels: Vec<&str> = ticks.iter().map(|(_, l)| l.as_str()).collect();
        assert!(labels.contains(&"Feb") && labels.contains(&"Mar"), "{labels:?}");
        assert!(ticks.windows(2).all(|w| w[1].0 - w[0].0 >= 20));
    }
}
