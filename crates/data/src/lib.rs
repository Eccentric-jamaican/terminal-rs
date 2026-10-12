//! Market data behind a pluggable [`MarketData`] trait.
//!
//! Provider selection ([`from_env`]):
//! - `TR_PROVIDER=sample` forces deterministic sample data (fully offline).
//! - `APCA_API_KEY_ID` + `APCA_API_SECRET_KEY` select Alpaca (free IEX feed by default,
//!   override with `APCA_DATA_FEED`).
//! - Otherwise Yahoo Finance's public chart history is used, which needs no key.
//!
//! Every live provider is wrapped in [`Fallback`], which serves sample data if the network
//! fails, so the terminal always has something to draw.

pub mod alpaca;
pub mod cache;
pub mod sample;
pub mod yahoo;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tr_core::{Bar, Timeframe};

pub use cache::BarCache;

/// Latest quote for a symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub symbol: String,
    pub last: f64,
    pub prev_close: f64,
    pub ts: i64,
}

impl Quote {
    pub fn change(&self) -> f64 {
        self.last - self.prev_close
    }

    pub fn change_pct(&self) -> f64 {
        if self.prev_close == 0.0 { 0.0 } else { self.change() / self.prev_close * 100.0 }
    }
}

/// A source of historical bars and quotes.
#[async_trait]
pub trait MarketData: Send + Sync {
    /// Short provider name shown in the status bar.
    fn name(&self) -> &str;

    /// Up to `limit` bars at `tf` ending at or before `end` (Unix seconds; `None` = now),
    /// ascending by time. Paging older history means calling again with `end` set to the
    /// oldest bar already held.
    async fn history(&self, symbol: &str, tf: Timeframe, end: Option<i64>, limit: usize) -> Result<Vec<Bar>>;

    /// Latest quotes for `symbols`. The default derives them from daily history.
    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        let mut out = Vec::with_capacity(symbols.len());
        for s in symbols {
            let bars = self.history(s, Timeframe::D1, None, 2).await?;
            if let Some(last) = bars.last() {
                let prev = if bars.len() > 1 { bars[bars.len() - 2].close } else { last.open };
                out.push(Quote { symbol: s.clone(), last: last.close, prev_close: prev, ts: last.ts });
            }
        }
        Ok(out)
    }
}

pub type Provider = Arc<dyn MarketData>;

/// Serves from `primary`, falling back to sample data on error.
pub struct Fallback<P> {
    primary: P,
    sample: sample::SampleProvider,
    name: String,
}

impl<P: MarketData> Fallback<P> {
    pub fn new(primary: P) -> Self {
        let name = primary.name().to_string();
        Self { primary, sample: sample::SampleProvider::default(), name }
    }
}

#[async_trait]
impl<P: MarketData> MarketData for Fallback<P> {
    fn name(&self) -> &str {
        &self.name
    }

    async fn history(&self, symbol: &str, tf: Timeframe, end: Option<i64>, limit: usize) -> Result<Vec<Bar>> {
        match self.primary.history(symbol, tf, end, limit).await {
            Ok(b) if !b.is_empty() => Ok(b),
            Ok(_) => Ok(Vec::new()),
            Err(e) => {
                tracing::warn!("{} history {symbol} {tf} failed, using sample data: {e:#}", self.name);
                self.sample.history(symbol, tf, end, limit).await
            }
        }
    }

    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        match self.primary.quotes(symbols).await {
            Ok(q) => Ok(q),
            Err(e) => {
                tracing::warn!("{} quotes failed, using sample data: {e:#}", self.name);
                self.sample.quotes(symbols).await
            }
        }
    }
}

/// Build the provider described in the crate docs from environment variables.
pub fn from_env() -> Provider {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    if var("TR_PROVIDER").is_some_and(|p| p.eq_ignore_ascii_case("sample")) {
        return Arc::new(sample::SampleProvider::default());
    }
    if let (Some(key), Some(secret)) = (var("APCA_API_KEY_ID"), var("APCA_API_SECRET_KEY")) {
        let feed = var("APCA_DATA_FEED").unwrap_or_else(|| "iex".into());
        match alpaca::AlpacaProvider::new(key, secret, feed) {
            Ok(p) => return Arc::new(Fallback::new(p)),
            Err(e) => tracing::warn!("alpaca init failed: {e:#}"),
        }
    }
    match yahoo::YahooProvider::new() {
        Ok(p) => Arc::new(Fallback::new(p)),
        Err(_) => Arc::new(sample::SampleProvider::default()),
    }
}

/// Keep at most `limit` bars ending at or before `end`.
pub(crate) fn clip(mut bars: Vec<Bar>, end: Option<i64>, limit: usize) -> Vec<Bar> {
    if let Some(end) = end {
        bars.retain(|b| b.ts < end);
    }
    if bars.len() > limit {
        bars.drain(..bars.len() - limit);
    }
    bars
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Broken;

    #[async_trait]
    impl MarketData for Broken {
        fn name(&self) -> &str {
            "broken"
        }
        async fn history(&self, _: &str, _: Timeframe, _: Option<i64>, _: usize) -> Result<Vec<Bar>> {
            anyhow::bail!("offline")
        }
    }

    #[tokio::test]
    async fn fallback_serves_sample_data() {
        let p = Fallback::new(Broken);
        let bars = p.history("AAPL", Timeframe::D1, None, 100).await.unwrap();
        assert_eq!(bars.len(), 100);
        let q = p.quotes(&["AAPL".into()]).await.unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(p.name(), "broken");
    }

    #[test]
    fn clip_respects_end_and_limit() {
        let bars: Vec<Bar> = (0..10).map(|i| Bar::new(i, 1.0, 1.0, 1.0, 1.0, 1.0)).collect();
        let c = clip(bars, Some(8), 3);
        assert_eq!(c.iter().map(|b| b.ts).collect::<Vec<_>>(), vec![5, 6, 7]);
    }

    #[test]
    fn quote_change() {
        let q = Quote { symbol: "X".into(), last: 110.0, prev_close: 100.0, ts: 0 };
        assert_eq!(q.change(), 10.0);
        assert!((q.change_pct() - 10.0).abs() < 1e-9);
    }
}
