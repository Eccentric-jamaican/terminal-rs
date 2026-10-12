//! Technical indicators. Every function returns a vector aligned with its
//! input; positions without enough history hold `f64::NAN`.
//!
//! Conventions follow the common charting definitions: EMA uses
//! alpha = 2/(n+1) seeded with an SMA, RSI uses Wilder smoothing, Bollinger
//! bands use the population standard deviation, and VWAP resets each session.

use crate::Bar;

/// Simple moving average over `n` values, computed with a running sum.
pub fn sma(values: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if n == 0 || values.len() < n {
        return out;
    }
    let mut sum: f64 = values[..n].iter().sum();
    out[n - 1] = sum / n as f64;
    for i in n..values.len() {
        sum += values[i] - values[i - n];
        out[i] = sum / n as f64;
    }
    out
}

/// Exponential smoothing seeded with the SMA of the first `n` values.
fn smooth(values: &[f64], n: usize, alpha: f64) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if n == 0 || values.len() < n {
        return out;
    }
    let mut prev = values[..n].iter().sum::<f64>() / n as f64;
    out[n - 1] = prev;
    for i in n..values.len() {
        prev = alpha * values[i] + (1.0 - alpha) * prev;
        out[i] = prev;
    }
    out
}

/// Exponential moving average, alpha = 2/(n+1).
pub fn ema(values: &[f64], n: usize) -> Vec<f64> {
    smooth(values, n, 2.0 / (n as f64 + 1.0))
}

/// Wilder's moving average (RMA), alpha = 1/n.
pub fn rma(values: &[f64], n: usize) -> Vec<f64> {
    smooth(values, n, 1.0 / n as f64)
}

/// Relative Strength Index with Wilder smoothing (first value at index `n`).
/// Single pass with no intermediate buffers: on large series the cost is dominated by
/// first-touch of output memory, so only the output is allocated.
pub fn rsi(closes: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; closes.len()];
    if n == 0 || closes.len() <= n {
        return out;
    }
    let value = |g: f64, l: f64| {
        if l == 0.0 { if g == 0.0 { 50.0 } else { 100.0 } } else { 100.0 - 100.0 / (1.0 + g / l) }
    };
    let (mut ag, mut al) = (0.0, 0.0);
    for i in 1..=n {
        let d = closes[i] - closes[i - 1];
        ag += d.max(0.0);
        al += (-d).max(0.0);
    }
    ag /= n as f64;
    al /= n as f64;
    out[n] = value(ag, al);
    let alpha = 1.0 / n as f64;
    for i in (n + 1)..closes.len() {
        let d = closes[i] - closes[i - 1];
        ag = alpha * d.max(0.0) + (1.0 - alpha) * ag;
        al = alpha * (-d).max(0.0) + (1.0 - alpha) * al;
        out[i] = value(ag, al);
    }
    out
}

#[derive(Debug, Clone)]
pub struct Macd {
    pub macd: Vec<f64>,
    pub signal: Vec<f64>,
    pub histogram: Vec<f64>,
}

/// Streaming EMA seeded with the SMA of its first `n` inputs.
struct Ema {
    n: usize,
    alpha: f64,
    seen: usize,
    acc: f64,
}

impl Ema {
    fn new(n: usize) -> Self {
        Self { n, alpha: 2.0 / (n as f64 + 1.0), seen: 0, acc: 0.0 }
    }

    /// Feed one value; returns the EMA once `n` values have been seen.
    fn push(&mut self, v: f64) -> Option<f64> {
        self.seen += 1;
        if self.seen < self.n {
            self.acc += v;
            None
        } else if self.seen == self.n {
            self.acc = (self.acc + v) / self.n as f64;
            Some(self.acc)
        } else {
            self.acc = self.alpha * v + (1.0 - self.alpha) * self.acc;
            Some(self.acc)
        }
    }
}

/// MACD line (fast EMA - slow EMA), its signal EMA and histogram, in one pass.
pub fn macd(closes: &[f64], fast: usize, slow: usize, signal: usize) -> Macd {
    let len = closes.len();
    let mut line = vec![f64::NAN; len];
    let mut sig = vec![f64::NAN; len];
    let mut histogram = vec![f64::NAN; len];
    if fast == 0 || slow == 0 || signal == 0 {
        return Macd { macd: line, signal: sig, histogram };
    }
    let (mut ef, mut es, mut eg) = (Ema::new(fast), Ema::new(slow), Ema::new(signal));
    for (i, &c) in closes.iter().enumerate() {
        let (f, s) = (ef.push(c), es.push(c));
        let (Some(f), Some(s)) = (f, s) else { continue };
        let m = f - s;
        line[i] = m;
        // The signal EMA starts once the MACD line itself is defined.
        if let Some(g) = eg.push(m) {
            sig[i] = g;
            histogram[i] = m - g;
        }
    }
    Macd { macd: line, signal: sig, histogram }
}

#[derive(Debug, Clone)]
pub struct Bands {
    pub middle: Vec<f64>,
    pub upper: Vec<f64>,
    pub lower: Vec<f64>,
}

/// Bollinger bands: SMA(n) +/- k * population standard deviation.
pub fn bollinger(closes: &[f64], n: usize, k: f64) -> Bands {
    let middle = sma(closes, n);
    let mut upper = vec![f64::NAN; closes.len()];
    let mut lower = vec![f64::NAN; closes.len()];
    if n > 0 && closes.len() >= n {
        let mut sum_sq: f64 = closes[..n].iter().map(|v| v * v).sum();
        for i in (n - 1)..closes.len() {
            if i >= n {
                sum_sq += closes[i] * closes[i] - closes[i - n] * closes[i - n];
            }
            let mean = middle[i];
            let var = (sum_sq / n as f64 - mean * mean).max(0.0);
            let sd = var.sqrt();
            upper[i] = mean + k * sd;
            lower[i] = mean - k * sd;
        }
    }
    Bands { middle, upper, lower }
}

/// Volume-weighted average price over hlc3, reset whenever `session_of(ts)` changes.
pub fn vwap_by(bars: &[Bar], session_of: impl Fn(i64) -> i64) -> Vec<f64> {
    let mut out = Vec::with_capacity(bars.len());
    let (mut pv, mut vol) = (0.0, 0.0);
    let mut session = None;
    for b in bars {
        let s = session_of(b.ts);
        if session != Some(s) {
            session = Some(s);
            pv = 0.0;
            vol = 0.0;
        }
        pv += b.hlc3() * b.volume;
        vol += b.volume;
        out.push(if vol > 0.0 { pv / vol } else { b.hlc3() });
    }
    out
}

/// Session VWAP resetting each UTC day. For daily and longer bars this
/// degenerates to hlc3, so callers typically anchor it by month instead.
pub fn vwap(bars: &[Bar]) -> Vec<f64> {
    vwap_by(bars, |ts| ts.div_euclid(86_400))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close_to(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn assert_series(got: &[f64], want: &[f64]) {
        assert_eq!(got.len(), want.len());
        for (i, (g, w)) in got.iter().zip(want).enumerate() {
            assert!((g.is_nan() && w.is_nan()) || close_to(*g, *w), "index {i}: got {g}, want {w}");
        }
    }

    const N: f64 = f64::NAN;

    #[test]
    fn sma_basic() {
        assert_series(&sma(&[1.0, 2.0, 3.0, 4.0, 5.0], 3), &[N, N, 2.0, 3.0, 4.0]);
        assert!(sma(&[1.0], 3)[0].is_nan());
        assert!(sma(&[1.0, 2.0], 0).iter().all(|v| v.is_nan()));
    }

    #[test]
    fn ema_seeded_with_sma() {
        // alpha = 0.5; seed = mean(1,2,3) = 2; then 0.5*4+0.5*2 = 3; 0.5*5+0.5*3 = 4.
        assert_series(&ema(&[1.0, 2.0, 3.0, 4.0, 5.0], 3), &[N, N, 2.0, 3.0, 4.0]);
        // Constant input stays constant.
        assert!(close_to(ema(&[7.0; 50], 10)[49], 7.0));
    }

    #[test]
    fn rsi_reference_values() {
        // Wilder's original 14-period example (New Concepts in Technical Trading Systems).
        let closes = [
            44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61, 46.28, 46.28,
            46.00, 46.03, 46.41, 46.22, 45.64,
        ];
        let r = rsi(&closes, 14);
        assert!(r[13].is_nan());
        assert!((r[14] - 70.46).abs() < 0.01, "rsi[14] = {}", r[14]);
        assert!((r[15] - 66.25).abs() < 0.01, "rsi[15] = {}", r[15]);
        assert!((r[19] - 57.92).abs() < 0.05, "rsi[19] = {}", r[19]);
    }

    #[test]
    fn rsi_extremes() {
        let up: Vec<f64> = (0..30).map(|i| i as f64).collect();
        assert_eq!(rsi(&up, 14)[29], 100.0);
        assert_eq!(rsi(&[5.0; 30], 14)[29], 50.0);
    }

    #[test]
    fn macd_matches_manual_emas() {
        let closes: Vec<f64> = (0..60).map(|i| 100.0 + (i as f64 * 0.3).sin() * 5.0 + i as f64 * 0.1).collect();
        let m = macd(&closes, 12, 26, 9);
        let f = ema(&closes, 12);
        let s = ema(&closes, 26);
        assert!(m.macd[24].is_nan());
        assert!(close_to(m.macd[25], f[25] - s[25]));
        // Signal needs 9 MACD values: first at 25 + 8.
        assert!(m.signal[32].is_nan());
        let seed: f64 = m.macd[25..34].iter().sum::<f64>() / 9.0;
        assert!(close_to(m.signal[33], seed));
        assert!(close_to(m.histogram[40], m.macd[40] - m.signal[40]));
    }

    #[test]
    fn bollinger_population_sd() {
        let b = bollinger(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0], 8, 2.0);
        // mean 5, population sd 2.
        assert!(close_to(b.middle[7], 5.0));
        assert!(close_to(b.upper[7], 9.0));
        assert!(close_to(b.lower[7], 1.0));
        assert!(b.upper[6].is_nan());
        // Rolling window stays numerically consistent with a fresh computation.
        let xs: Vec<f64> = (0..200).map(|i| ((i * 37) % 101) as f64).collect();
        let roll = bollinger(&xs, 20, 2.0);
        let fresh = bollinger(&xs[180..], 20, 2.0);
        assert!(close_to(roll.upper[199], fresh.upper[19]));
    }

    #[test]
    fn vwap_resets_per_session() {
        let day = 86_400;
        let bars = [
            Bar::new(0, 0.0, 12.0, 8.0, 10.0, 100.0),
            Bar::new(60, 0.0, 22.0, 18.0, 20.0, 300.0),
            Bar::new(day, 0.0, 6.0, 4.0, 5.0, 10.0),
        ];
        let v = vwap(&bars);
        assert!(close_to(v[0], 10.0));
        assert!(close_to(v[1], (10.0 * 100.0 + 20.0 * 300.0) / 400.0));
        assert!(close_to(v[2], 5.0));
    }
}
