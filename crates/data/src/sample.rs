use anyhow::Result;
use async_trait::async_trait;
use tr_core::{Bar, Timeframe, sample};

use crate::MarketData;

/// Deterministic synthetic data anchored at a fixed "now" per process, so paging is
/// consistent: page N+1 is exactly the bars preceding page N.
#[derive(Debug, Clone)]
pub struct SampleProvider {
    now: i64,
    /// Total history available per symbol/timeframe.
    pub depth: usize,
}

impl Default for SampleProvider {
    /// History depth defaults to 20,000 bars; `TR_SAMPLE_BARS` raises it (e.g. 1000000).
    fn default() -> Self {
        let depth = std::env::var("TR_SAMPLE_BARS").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000);
        Self { now: chrono::Utc::now().timestamp(), depth }
    }
}

impl SampleProvider {
    pub fn at(now: i64, depth: usize) -> Self {
        Self { now, depth }
    }
}

#[async_trait]
impl MarketData for SampleProvider {
    fn name(&self) -> &str {
        "sample"
    }

    async fn history(&self, symbol: &str, tf: Timeframe, end: Option<i64>, limit: usize) -> Result<Vec<Bar>> {
        let all = sample::generate(symbol, tf, self.depth, self.now);
        Ok(crate::clip(all, end, limit))
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
}
