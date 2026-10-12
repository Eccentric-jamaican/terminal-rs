//! Deterministic synthetic OHLCV data, used when no market-data key is
//! configured and for benchmarks. The same symbol always yields the same path.

use crate::{Bar, Timeframe};

/// Small, fast xorshift64* PRNG so sample data needs no external crates.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Standard normal via Box-Muller.
    pub fn normal(&mut self) -> f64 {
        let u1 = self.f64().max(1e-12);
        let u2 = self.f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

/// FNV-1a hash used to seed per-symbol paths.
pub fn seed_for(symbol: &str) -> u64 {
    symbol.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

/// Generate `count` bars ending at (and including the bucket of) `end_ts`.
pub fn generate(symbol: &str, tf: Timeframe, count: usize, end_ts: i64) -> Vec<Bar> {
    let mut rng = Rng::new(seed_for(symbol) ^ tf.seconds() as u64);
    let step = tf.seconds();
    let end = tf.bucket_start(end_ts);
    // Annualised ~30% vol scaled to the bar interval.
    let sigma = 0.30 * (step as f64 / (365.0 * 86_400.0)).sqrt();
    let drift = 0.08 * step as f64 / (365.0 * 86_400.0);
    let mut price = 20.0 + (seed_for(symbol) % 480) as f64;
    let base_vol = 1.0e6 * (step as f64 / 86_400.0).sqrt();
    let mut bars = Vec::with_capacity(count);
    for i in 0..count {
        let ts = match tf {
            // Calendar buckets: step back roughly then snap so weeks/months align.
            Timeframe::W1 | Timeframe::MN1 => tf.bucket_start(end - (count - 1 - i) as i64 * step),
            _ => end - (count - 1 - i) as i64 * step,
        };
        let open = price;
        let ret = drift + sigma * rng.normal();
        let close = (open * ret.exp()).max(0.01);
        let wick = sigma * open * 0.6;
        let high = open.max(close) + wick * rng.f64();
        let low = (open.min(close) - wick * rng.f64()).max(0.005);
        let volume = base_vol * (0.5 + rng.f64()) * (1.0 + 20.0 * ret.abs() / sigma.max(1e-9) * 0.05);
        bars.push(Bar::new(ts, open, high, low, close, volume.round()));
        price = close;
    }
    bars.dedup_by_key(|b| b.ts);
    bars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_valid() {
        let a = generate("AAPL", Timeframe::D1, 500, 1_790_000_000);
        let b = generate("AAPL", Timeframe::D1, 500, 1_790_000_000);
        assert_eq!(a, b);
        assert_eq!(a.len(), 500);
        assert!(a.windows(2).all(|w| w[0].ts < w[1].ts));
        assert!(a.iter().all(|b| b.low <= b.open.min(b.close) && b.high >= b.open.max(b.close) && b.low > 0.0));
        assert_ne!(a, generate("MSFT", Timeframe::D1, 500, 1_790_000_000));
    }

    #[test]
    fn monthly_buckets_are_unique() {
        let m = generate("SPY", Timeframe::MN1, 120, 1_790_000_000);
        assert!(m.windows(2).all(|w| w[0].ts < w[1].ts));
        assert!(m.len() >= 110);
    }
}
