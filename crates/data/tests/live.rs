//! Network tests against Yahoo; run with `cargo test -p tr-data -- --ignored`.
use tr_core::Timeframe;
use tr_data::MarketData;
use tr_data::yahoo::YahooProvider;

#[tokio::test]
#[ignore = "network"]
async fn yahoo_history_and_paging() {
    let y = YahooProvider::new().unwrap();
    for tf in [Timeframe::M5, Timeframe::H4, Timeframe::D1, Timeframe::W1, Timeframe::MN1] {
        let recent = y.history("AAPL", tf, None, 200).await.unwrap();
        assert!(recent.len() >= 100, "{tf}: {}", recent.len());
        assert!(recent.windows(2).all(|w| w[0].ts < w[1].ts), "{tf}");
        let older = y.history("AAPL", tf, Some(recent[0].ts), 200).await.unwrap();
        assert!(older.last().unwrap().ts < recent[0].ts, "{tf}");
    }
    let q = y.quotes(&["AAPL".into(), "MSFT".into()]).await.unwrap();
    assert_eq!(q.len(), 2);
}
