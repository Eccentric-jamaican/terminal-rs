//! Bloomberg-style command line: `<SECURITY> [<FUNCTION> args] <GO>`.
//!
//! Examples: `AAPL`, `AAPL US EQUITY <GO>`, `5`, `1W`, `RSI`, `SMA 50`, `CMP MSFT`,
//! `AAPL CONG`, `CONG`, `POL PELOSI`, `TRACK PELOSI`, `LAYOUT 4`, `FIB`, `HELP`.

use tr_chart::drawings::Tool;
use tr_chart::scene::ChartType;
use tr_core::Timeframe;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IndicatorSpec {
    Sma(usize),
    Ema(usize),
    Vwap,
    Bollinger(usize, f64),
    Rsi(usize),
    Macd(usize, usize, usize),
}

impl IndicatorSpec {
    pub fn label(&self) -> String {
        match self {
            IndicatorSpec::Sma(n) => format!("SMA {n}"),
            IndicatorSpec::Ema(n) => format!("EMA {n}"),
            IndicatorSpec::Vwap => "VWAP".into(),
            IndicatorSpec::Bollinger(n, k) => format!("BB {n} {k}"),
            IndicatorSpec::Rsi(n) => format!("RSI {n}"),
            IndicatorSpec::Macd(a, b, c) => format!("MACD {a} {b} {c}"),
        }
    }

    /// Oscillators get their own sub-pane.
    pub fn is_pane(&self) -> bool {
        matches!(self, IndicatorSpec::Rsi(_) | IndicatorSpec::Macd(..))
    }

    /// Same indicator family (used to toggle and replace).
    pub fn same_kind(&self, other: &IndicatorSpec) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
            && match (self, other) {
                (IndicatorSpec::Sma(a), IndicatorSpec::Sma(b)) | (IndicatorSpec::Ema(a), IndicatorSpec::Ema(b)) => a == b,
                _ => true,
            }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Symbol(String),
    SymbolFunction(String, Function),
    Function(Function),
}

/// Quick range: span of history plus the interval used to show it (mirrors common charting presets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    D1,
    D5,
    M1,
    M3,
    M6,
    Ytd,
    Y1,
    Y5,
    All,
}

impl Range {
    pub const ALL: [Range; 9] = [Range::D1, Range::D5, Range::M1, Range::M3, Range::M6, Range::Ytd, Range::Y1, Range::Y5, Range::All];

    pub fn label(self) -> &'static str {
        match self {
            Range::D1 => "1D",
            Range::D5 => "5D",
            Range::M1 => "1M",
            Range::M3 => "3M",
            Range::M6 => "6M",
            Range::Ytd => "YTD",
            Range::Y1 => "1Y",
            Range::Y5 => "5Y",
            Range::All => "ALL",
        }
    }

    pub fn timeframe(self) -> Timeframe {
        match self {
            Range::D1 => Timeframe::M1,
            Range::D5 => Timeframe::M5,
            Range::M1 => Timeframe::M30,
            Range::M3 => Timeframe::H1,
            Range::M6 => Timeframe::H4,
            Range::Ytd | Range::Y1 => Timeframe::D1,
            Range::Y5 => Timeframe::W1,
            Range::All => Timeframe::MN1,
        }
    }

    /// Approximate seconds of history to show (`None` = everything).
    pub fn span_secs(self, now: i64) -> Option<i64> {
        const D: i64 = 86_400;
        Some(match self {
            Range::D1 => D,
            Range::D5 => 5 * D,
            Range::M1 => 30 * D,
            Range::M3 => 91 * D,
            Range::M6 => 182 * D,
            Range::Ytd => {
                use chrono::Datelike;
                let y = chrono::DateTime::from_timestamp(now, 0).unwrap_or_default().year();
                now - chrono::NaiveDate::from_ymd_opt(y, 1, 1)?.and_hms_opt(0, 0, 0)?.and_utc().timestamp()
            }
            Range::Y1 => 365 * D,
            Range::Y5 => 5 * 365 * D,
            Range::All => return None,
        })
    }

    fn parse(s: &str) -> Option<Range> {
        Range::ALL.into_iter().find(|r| r.label().eq_ignore_ascii_case(s))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Function {
    Timeframe(Timeframe),
    ChartType(ChartType),
    Indicator(IndicatorSpec),
    ClearIndicators,
    Compare(Option<String>),
    LogScale,
    Range(Range),
    Draw(Tool),
    ClearDrawings,
    Magnet,
    /// Congress trades: feed when used alone, per-ticker list after a symbol.
    Congress,
    Politician(String),
    Track(String),
    Untrack(String),
    Alerts,
    Refresh,
    Watchlist,
    WatchAdd(Vec<String>),
    WatchRemove(Vec<String>),
    Layout(String),
    NewPane,
    ClosePane,
    Help,
}

/// Bloomberg yellow-key suffixes that may follow a ticker and are ignored.
const MARKET_SECTORS: [&str; 8] = ["US", "EQUITY", "EQ", "INDEX", "CURNCY", "COMDTY", "<GO>", "GO"];

fn num<T: std::str::FromStr>(t: Option<&&str>) -> Option<T> {
    t.and_then(|s| s.parse().ok())
}

fn parse_function(head: &str, rest: &[&str]) -> Option<Function> {
    let up = head.to_ascii_uppercase();
    let words = || rest.iter().map(|s| s.to_ascii_uppercase()).collect::<Vec<_>>();
    let joined = || rest.join(" ");
    Some(match up.as_str() {
        "CANDLE" | "CANDLES" => Function::ChartType(ChartType::Candles),
        "BAR" | "BARS" | "OHLC" => Function::ChartType(ChartType::Bars),
        "LINE" => Function::ChartType(ChartType::Line),
        "AREA" | "MOUNTAIN" => Function::ChartType(ChartType::Area),
        "SMA" => Function::Indicator(IndicatorSpec::Sma(num(rest.first()).unwrap_or(20))),
        "EMA" => Function::Indicator(IndicatorSpec::Ema(num(rest.first()).unwrap_or(20))),
        "VWAP" => Function::Indicator(IndicatorSpec::Vwap),
        "BB" | "BOLL" | "BOLLINGER" => {
            Function::Indicator(IndicatorSpec::Bollinger(num(rest.first()).unwrap_or(20), num(rest.get(1)).unwrap_or(2.0)))
        }
        "RSI" => Function::Indicator(IndicatorSpec::Rsi(num(rest.first()).unwrap_or(14))),
        "MACD" => Function::Indicator(IndicatorSpec::Macd(
            num(rest.first()).unwrap_or(12),
            num(rest.get(1)).unwrap_or(26),
            num(rest.get(2)).unwrap_or(9),
        )),
        "CLEAR" | "CLR" => match words().first().map(String::as_str) {
            Some("DRAW") | Some("DRAWINGS") => Function::ClearDrawings,
            _ => Function::ClearIndicators,
        },
        "CMP" | "COMPARE" | "COMP" => Function::Compare(rest.first().map(|s| s.to_ascii_uppercase())),
        "LOG" => Function::LogScale,
        "TL" | "TREND" | "TRENDLINE" => Function::Draw(Tool::Trendline),
        "HL" | "HLINE" => Function::Draw(Tool::Horizontal),
        "VL" | "VLINE" => Function::Draw(Tool::Vertical),
        "FIB" => Function::Draw(Tool::Fib),
        "MAGNET" | "MAG" => Function::Magnet,
        "CONG" | "CONGRESS" => Function::Congress,
        "POL" | "PROFILE" => Function::Politician(joined()),
        "TRACK" => Function::Track(joined()),
        "UNTRACK" => Function::Untrack(joined()),
        "ALRT" | "ALERTS" | "ALERT" => Function::Alerts,
        "REFRESH" | "SYNC" => Function::Refresh,
        "W" | "WL" | "WATCH" | "WATCHLIST" | "MON" => match words().first().map(String::as_str) {
            Some("ADD") => Function::WatchAdd(words()[1..].to_vec()),
            Some("DEL") | Some("RM") | Some("REMOVE") => Function::WatchRemove(words()[1..].to_vec()),
            _ => Function::Watchlist,
        },
        "LAYOUT" | "GRID" => Function::Layout(rest.first().map(|s| s.to_ascii_uppercase()).unwrap_or_else(|| "1".into())),
        "NEW" => Function::NewPane,
        "CLOSE" => Function::ClosePane,
        "HELP" | "?" | "H" => Function::Help,
        "RANGE" => Function::Range(Range::parse(rest.first()?)?),
        // Timeframes win where spellings overlap (`1D`, `1M`); otherwise try a range (`5D`, `YTD`).
        _ => return head.parse::<Timeframe>().ok().map(Function::Timeframe).or_else(|| Range::parse(&up).map(Function::Range)),
    })
}

fn looks_like_ticker(s: &str) -> bool {
    let core = s.trim_start_matches('^');
    !core.is_empty()
        && core.len() <= 12
        && core.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '=' | '/'))
        && core.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
}

/// Parse one command line. Returns `None` for empty or unrecognised input.
pub fn parse(input: &str) -> Option<Command> {
    let tokens: Vec<&str> = input
        .split_whitespace()
        .filter(|t| !t.eq_ignore_ascii_case("<GO>"))
        .collect();
    let (&head, rest) = tokens.split_first()?;
    // A function keyword (or timeframe) wins over a ticker of the same spelling,
    // except for single letters that are common tickers when followed by nothing.
    if let Some(f) = parse_function(head, rest) {
        return Some(Command::Function(f));
    }
    if !looks_like_ticker(head) {
        return None;
    }
    let symbol = head.to_ascii_uppercase().replace('/', ".");
    let rest: Vec<&str> = rest.iter().copied().filter(|t| !MARKET_SECTORS.contains(&t.to_ascii_uppercase().as_str())).collect();
    match rest.split_first() {
        None => Some(Command::Symbol(symbol)),
        Some((&f, args)) => parse_function(f, args).map(|func| Command::SymbolFunction(symbol, func)),
    }
}

/// One-line live interpretation of partially typed input, shown under the command line.
pub fn hint(input: &str) -> String {
    match parse(input) {
        None if input.trim().is_empty() => "Type a ticker and press <GO> (Enter).  HELP for functions.".into(),
        None => "Unrecognised command".into(),
        Some(Command::Symbol(s)) => format!("Load {s}"),
        Some(Command::SymbolFunction(s, f)) => format!("{s}: {}", describe(&f)),
        Some(Command::Function(f)) => describe(&f),
    }
}

fn describe(f: &Function) -> String {
    match f {
        Function::Timeframe(tf) => format!("Interval {}", match tf {
            Timeframe::M1 => "1 minute",
            Timeframe::M5 => "5 minutes",
            Timeframe::M15 => "15 minutes",
            Timeframe::M30 => "30 minutes",
            Timeframe::H1 => "1 hour",
            Timeframe::H4 => "4 hours",
            Timeframe::D1 => "1 day",
            Timeframe::W1 => "1 week",
            Timeframe::MN1 => "1 month",
        }),
        Function::ChartType(c) => format!("Chart type {}", c.label()),
        Function::Indicator(i) => format!("Toggle {}", i.label()),
        Function::ClearIndicators => "Remove all indicators".into(),
        Function::Compare(Some(s)) => format!("Compare with {s}"),
        Function::Compare(None) => "Remove comparison".into(),
        Function::LogScale => "Toggle log scale".into(),
        Function::Range(r) => format!("Show {} at {}", r.label(), r.timeframe()),
        Function::Draw(t) => format!("Draw {}", t.label()),
        Function::ClearDrawings => "Remove all drawings".into(),
        Function::Magnet => "Toggle magnet (snap to OHLC)".into(),
        Function::Congress => "Congressional trades".into(),
        Function::Politician(n) => format!("Politician profile: {n}"),
        Function::Track(n) => format!("Track {n} (alert on new filings)"),
        Function::Untrack(n) => format!("Stop tracking {n}"),
        Function::Alerts => "Alerts".into(),
        Function::Refresh => "Refresh congressional disclosures now".into(),
        Function::Watchlist => "Watchlist".into(),
        Function::WatchAdd(s) => format!("Add {} to watchlist", s.join(", ")),
        Function::WatchRemove(s) => format!("Remove {} from watchlist", s.join(", ")),
        Function::Layout(l) => format!("Layout {l}"),
        Function::NewPane => "Split: add a pane".into(),
        Function::ClosePane => "Close focused pane".into(),
        Function::Help => "Help".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols() {
        assert_eq!(parse("aapl"), Some(Command::Symbol("AAPL".into())));
        assert_eq!(parse("AAPL US EQUITY <GO>"), Some(Command::Symbol("AAPL".into())));
        assert_eq!(parse("brk/b"), Some(Command::Symbol("BRK.B".into())));
        assert_eq!(parse("^GSPC"), Some(Command::Symbol("^GSPC".into())));
        assert_eq!(parse("   "), None);
        assert_eq!(parse("123abc"), None);
    }

    #[test]
    fn timeframes_and_ranges() {
        assert_eq!(parse("5"), Some(Command::Function(Function::Timeframe(Timeframe::M5))));
        assert_eq!(parse("1W"), Some(Command::Function(Function::Timeframe(Timeframe::W1))));
        assert_eq!(parse("1M"), Some(Command::Function(Function::Timeframe(Timeframe::MN1))));
        assert_eq!(parse("1m"), Some(Command::Function(Function::Timeframe(Timeframe::M1))));
        assert_eq!(parse("YTD"), Some(Command::Function(Function::Range(Range::Ytd))));
        assert_eq!(parse("RANGE 5D"), Some(Command::Function(Function::Range(Range::D5))));
        assert_eq!(Range::Y5.timeframe(), Timeframe::W1);
    }

    #[test]
    fn functions() {
        assert_eq!(parse("rsi"), Some(Command::Function(Function::Indicator(IndicatorSpec::Rsi(14)))));
        assert_eq!(parse("SMA 50"), Some(Command::Function(Function::Indicator(IndicatorSpec::Sma(50)))));
        assert_eq!(parse("bb 20 2.5"), Some(Command::Function(Function::Indicator(IndicatorSpec::Bollinger(20, 2.5)))));
        assert_eq!(parse("MACD"), Some(Command::Function(Function::Indicator(IndicatorSpec::Macd(12, 26, 9)))));
        assert_eq!(parse("CMP msft"), Some(Command::Function(Function::Compare(Some("MSFT".into())))));
        assert_eq!(parse("CMP"), Some(Command::Function(Function::Compare(None))));
        assert_eq!(parse("FIB"), Some(Command::Function(Function::Draw(Tool::Fib))));
        assert_eq!(parse("CLEAR DRAW"), Some(Command::Function(Function::ClearDrawings)));
        assert_eq!(parse("POL Nancy Pelosi"), Some(Command::Function(Function::Politician("Nancy Pelosi".into()))));
        assert_eq!(parse("WL ADD msft nvda"), Some(Command::Function(Function::WatchAdd(vec!["MSFT".into(), "NVDA".into()]))));
        assert_eq!(parse("LAYOUT 4"), Some(Command::Function(Function::Layout("4".into()))));
    }

    #[test]
    fn symbol_with_function() {
        assert_eq!(parse("NVDA CONG <GO>"), Some(Command::SymbolFunction("NVDA".into(), Function::Congress)));
        assert_eq!(parse("AAPL US EQUITY 1W"), Some(Command::SymbolFunction("AAPL".into(), Function::Timeframe(Timeframe::W1))));
        assert_eq!(parse("AAPL WHATEVER"), None);
    }

    #[test]
    fn hints() {
        assert_eq!(hint("5"), "Interval 5 minutes");
        assert_eq!(hint("msft"), "Load MSFT");
        assert!(hint("").contains("ticker"));
    }

    #[test]
    fn indicator_identity() {
        assert!(IndicatorSpec::Rsi(14).same_kind(&IndicatorSpec::Rsi(21)));
        assert!(!IndicatorSpec::Sma(20).same_kind(&IndicatorSpec::Sma(50)));
        assert!(IndicatorSpec::Macd(12, 26, 9).is_pane());
    }
}
