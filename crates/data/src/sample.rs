use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tr_core::{Bar, Timeframe, sample};

use crate::{LiveFeed, MarketData, security};

/// Deterministic synthetic data anchored at a fixed "now" per process, so paging is
/// consistent: page N+1 is exactly the bars preceding page N. Levels and volatility
/// follow the asset class (BTCUSD near 65,000, EURUSD near 1.09, US10Y near 4.2%).
#[derive(Debug, Clone)]
pub struct SampleProvider {
    now: i64,
    /// Total history available per symbol/timeframe.
    pub depth: usize,
    live_ticks: bool,
}

impl Default for SampleProvider {
    /// History depth defaults to 20,000 bars; `TR_SAMPLE_BARS` raises it (e.g. 1000000).
    fn default() -> Self {
        let depth = std::env::var("TR_SAMPLE_BARS").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000);
        Self { now: chrono::Utc::now().timestamp(), depth, live_ticks: false }
    }
}

impl SampleProvider {
    pub fn at(now: i64, depth: usize) -> Self {
        Self { now, depth, live_ticks: false }
    }

    /// Also simulate a live tick stream (offline mode).
    pub fn with_live_ticks(mut self) -> Self {
        self.live_ticks = true;
        self
    }

    fn bars(&self, symbol: &str, tf: Timeframe) -> Vec<Bar> {
        sample::for_security(&security(symbol), tf, self.depth, self.now)
    }
}

#[async_trait]
impl MarketData for SampleProvider {
    fn name(&self) -> &str {
        "sample"
    }

    async fn history(&self, symbol: &str, tf: Timeframe, end: Option<i64>, limit: usize) -> Result<Vec<Bar>> {
        Ok(crate::clip(self.bars(symbol, tf), end, limit))
    }

    /// Random-walk ticks on the last bar every 400 ms, rolling into new bars on schedule.
    fn live(&self, symbol: &str, tf: Timeframe) -> Option<LiveFeed> {
        if !self.live_ticks {
            return None;
        }
        let mut bar = *self.bars(symbol, tf).last()?;
        let sec = security(symbol);
        let (tx, rx) = mpsc::channel(64);
        let tick_vol = sec.class.sample_vol() * (0.4 / (365.0 * 86_400.0f64)).sqrt() * 8.0;
        let mut rng = sample::Rng::new(sample::seed_for(symbol) ^ 0x5eed);
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(Duration::from_millis(400));
            loop {
                iv.tick().await;
                let now = chrono::Utc::now().timestamp();
                let bucket = tf.bucket_start(now);
                if bucket > bar.ts {
                    bar = Bar::new(bucket, bar.close, bar.close, bar.close, bar.close, 0.0);
                }
                let px = bar.close * (tick_vol * rng.normal()).exp();
                bar.close = px;
                bar.high = bar.high.max(px);
                bar.low = bar.low.min(px);
                bar.volume += (rng.f64() * 100.0).round();
                if tx.send(bar).await.is_err() {
                    break;
                }
            }
        });
        Some(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn paging_is_contiguous() {
        let p = SampleProvider::at(1_790_000_000, 1_000);
        let recent = p.history("AAPL", Timeframe::H1, None, 300).await.unwrap();
        let older = p.history("AAPL", Timeframe::H1, Some(recent[0].ts), 300).await.unwrap();
        assert_eq!(older.len(), 300);
        assert_eq!(older.last().unwrap().ts + 3600, recent[0].ts);
        assert_eq!(older.last().unwrap().close, recent[0].open);
    }

    #[tokio::test]
    async fn class_aware_levels_and_quotes() {
        let p = SampleProvider::at(1_790_000_000, 500);
        let q = p.quotes(&["BTCUSD Curncy".into(), "US10Y Govt".into(), "EURUSD Curncy".into()]).await.unwrap();
        assert!(q[0].last > 10_000.0);
        assert!(q[1].last > 0.0 && q[1].last < 20.0);
        assert!(q[2].last > 0.5 && q[2].last < 2.0);
    }

    #[tokio::test]
    async fn simulated_ticks() {
        let p = SampleProvider::at(chrono::Utc::now().timestamp(), 100).with_live_ticks();
        let mut rx = p.live("ETHUSD Curncy", Timeframe::M1).unwrap();
        let a = rx.recv().await.unwrap();
        let b = rx.recv().await.unwrap();
        assert!(a.high >= a.low && b.ts >= a.ts && b.close > 0.0);
        assert!(SampleProvider::at(0, 10).live("X", Timeframe::M1).is_none());
    }
}
