//! Top-level terminal: command line, tiling panes, hotkeys, status/function-key bars.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{Color32, Event, Key, Modifiers, Rect, RichText, Ui, UiBuilder, pos2, vec2};
use serde::{Deserialize, Serialize};
use tr_chart::drawings::{Drawing, Tool};
use tr_chart::scene::ChartType;
use tr_congress::Trade;
use tr_congress::db::{Alert, PoliticianSummary};
use tr_core::Timeframe;
use tr_data::{Quote, SearchHit, YieldCurves};

use crate::anim;
use crate::chart_view::ChartState;
use crate::command::{self, Command, Function, IndicatorSpec};
use crate::congress_view::{self, Action, FeedFilter};
use crate::layout::Layout;
use crate::markets_view;
use crate::theme::{self, amber, mono, muted, text};
use crate::worker::{PaneId, Profile, Request, Response, Worker, data_dir};

mod chrome;

use chrome::{GAP, SIDEBAR_W, STATUS_H, SidebarCmd, TOOLBAR_H, TRAFFIC_INSET};

pub enum PaneKind {
    Chart(Box<ChartState>),
    Feed,
    Ticker { symbol: String, trades: Vec<Trade>, loading: bool },
    Profile { query: String, data: Option<Box<Profile>>, loading: bool },
    Politicians,
    Alerts,
    Watchlist,
    Help,
    Wei,
    Curve,
    Search { query: String },
}

impl PaneKind {
    fn title(&self) -> String {
        match self {
            PaneKind::Chart(c) => format!("GP {}", c.symbol),
            PaneKind::Feed => "CONG  Latest congressional trades (STOCK Act PTRs)".into(),
            PaneKind::Ticker { symbol, .. } => format!("{symbol} CONG  Who in Congress traded {symbol}"),
            PaneKind::Profile { query, data, .. } => match data {
                Some(p) => format!("POL  {}", p.politician.name),
                None => format!("POL  {query}"),
            },
            PaneKind::Politicians => "POL  Politicians (☆ to track)".into(),
            PaneKind::Alerts => "ALRT  Alerts for tracked politicians".into(),
            PaneKind::Watchlist => "WL  Watchlist".into(),
            PaneKind::Help => "HELP  Commands and hotkeys".into(),
            PaneKind::Wei => "WEI  World markets: indices, futures, rates, FX, crypto, commodities".into(),
            PaneKind::Curve => "GC  US Treasury yield curve (Treasury.gov par yields)".into(),
            PaneKind::Search { query } => format!("SECF  Security search: {query}"),
        }
    }
}

pub struct Pane {
    pub id: PaneId,
    pub kind: PaneKind,
}

/// Persisted between runs.
#[derive(Serialize, Deserialize)]
struct Settings {
    layout: Layout,
    watchlist: Vec<String>,
    panes: Vec<PaneSettings>,
    #[serde(default = "default_true")]
    sidebar: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
enum PaneSettings {
    Chart {
        symbol: String,
        tf: Timeframe,
        chart_type: ChartType,
        log: bool,
        indicators: Vec<IndicatorSpec>,
        drawings: Vec<Drawing>,
        compare: Option<String>,
    },
    Feed,
    Ticker(String),
    Profile(String),
    Politicians,
    Alerts,
    Watchlist,
    Help,
    Wei,
    Curve,
    Search(String),
}

pub struct TerminalApp {
    worker: Worker,
    provider: String,
    layout: Layout,
    panes: Vec<Pane>,
    focus: usize,
    next_id: PaneId,
    last_opened: Option<usize>,
    curves: Option<Box<YieldCurves>>,
    last_wei: Option<Instant>,
    last_curve: Option<Instant>,
    wei_updated: Option<String>,
    /// Latest network search results and the query they answer.
    net_hits: (String, Vec<SearchHit>),
    /// Query typed in the command line awaiting a debounced network search.
    search_pending: Option<(String, Instant)>,
    suggestion: Option<usize>,

    cmd: String,
    cmd_history: Vec<String>,
    cmd_hist_pos: Option<usize>,
    focus_cmd: bool,

    status: String,
    toasts: Vec<(String, Instant)>,

    latest: Vec<Trade>,
    feed_filter: FeedFilter,
    politicians: Vec<PoliticianSummary>,
    pol_search: String,
    alerts: Vec<Alert>,
    congress_status: String,

    watchlist: Vec<String>,
    quotes: HashMap<String, Quote>,
    last_quotes: Option<Instant>,
    last_feed: Option<Instant>,
    last_save: Instant,
    started: Instant,
    smoke_done: bool,
    /// Frame-time readout (F12): smoothed CPU time of `ui()` and interval between frames.
    show_perf: bool,
    perf_cpu_ms: f64,
    perf_dt_ms: f64,
    last_frame: Option<Instant>,

    sidebar_open: bool,
    /// System accent colour (macOS) or the default blue.
    accent: Color32,
    last_accent_poll: Option<Instant>,
    window_styled: bool,
    /// The window is see-through behind the sidebar (macOS vibrancy).
    vibrancy: bool,
}

/// Resolve a run of words (`BTCUSD CRNCY AAPL US EQUITY MSFT`) into market keys.
fn keys_from_words(words: &[String]) -> Vec<String> {
    let tokens: Vec<&str> = words.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match tr_core::Security::resolve_tokens(&tokens[i..]) {
            Some((sec, used)) => {
                out.push(sec.key());
                i += used;
            }
            None => i += 1,
        }
    }
    out
}

fn settings_path() -> std::path::PathBuf {
    data_dir().join("settings.json")
}

impl TerminalApp {
    pub fn new(ctx: &egui::Context, worker: Worker, provider: String) -> Self {
        let accent = tr_macos::accent_color().map_or(theme::DEFAULT_ACCENT, |[r, g, b]| Color32::from_rgb(r, g, b));
        theme::apply(ctx, accent);
        match std::env::var("TR_THEME").map(|v| v.to_ascii_lowercase()).as_deref() {
            Ok("light") => ctx.set_theme(egui::ThemePreference::Light),
            Ok("dark") => ctx.set_theme(egui::ThemePreference::Dark),
            _ => ctx.set_theme(egui::ThemePreference::System),
        }
        let settings: Option<Settings> =
            std::fs::read(settings_path()).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let mut app = Self {
            worker,
            provider,
            layout: Layout::Grid2x2,
            panes: Vec::new(),
            focus: 0,
            next_id: 1,
            last_opened: None,
            curves: None,
            last_wei: None,
            last_curve: None,
            wei_updated: None,
            net_hits: (String::new(), Vec::new()),
            search_pending: None,
            suggestion: None,
            cmd: String::new(),
            cmd_history: Vec::new(),
            cmd_hist_pos: None,
            focus_cmd: true,
            status: "Ready. Type a ticker and press <GO> (Enter). F1 for help.".into(),
            toasts: Vec::new(),
            latest: Vec::new(),
            feed_filter: FeedFilter::default(),
            politicians: Vec::new(),
            pol_search: String::new(),
            alerts: Vec::new(),
            congress_status: String::new(),
            watchlist: [
                "SPX Index",
                "AAPL US Equity",
                "NVDA US Equity",
                "SPY US Equity",
                "BTCUSD Curncy",
                "ETHUSD Curncy",
                "EURUSD Curncy",
                "USDJPY Curncy",
                "US10Y Govt",
                "CL1 Comdty",
                "GC1 Comdty",
            ]
            .map(String::from)
            .to_vec(),
            quotes: HashMap::new(),
            last_quotes: None,
            last_feed: None,
            last_save: Instant::now(),
            started: Instant::now(),
            smoke_done: false,
            show_perf: std::env::var_os("TR_FPS").is_some(),
            perf_cpu_ms: 0.0,
            perf_dt_ms: 0.0,
            last_frame: None,
            sidebar_open: true,
            accent,
            last_accent_poll: None,
            window_styled: false,
            vibrancy: false,
        };
        match settings {
            Some(s) => {
                app.layout = s.layout;
                // Older settings stored bare tickers; normalise to market keys.
                app.watchlist = s.watchlist.iter().map(|w| tr_data::security(w).key()).collect();
                app.watchlist.dedup();
                app.sidebar_open = s.sidebar;
                for p in s.panes {
                    let kind = app.kind_from_settings(p);
                    app.push_pane(kind);
                }
            }
            None => {
                for kind in [
                    PaneKind::Chart(Box::new(ChartState::new(0, "AAPL", Timeframe::D1))),
                    PaneKind::Feed,
                    PaneKind::Wei,
                    PaneKind::Watchlist,
                ] {
                    app.push_pane(kind);
                }
            }
        }
        app.fit_panes_to_layout();
        app.worker.send(Request::Latest { limit: 2_000 });
        app.worker.send(Request::SearchPoliticians(String::new()));
        app.worker.send(Request::Alerts);
        app
    }

    fn kind_from_settings(&mut self, p: PaneSettings) -> PaneKind {
        match p {
            PaneSettings::Chart { symbol, tf, chart_type, log, indicators, drawings, compare } => {
                let mut c = ChartState::new(0, &symbol, tf);
                c.chart_type = chart_type;
                c.log = log;
                c.indicators = indicators;
                c.drawings = drawings;
                c.compare = compare;
                PaneKind::Chart(Box::new(c))
            }
            PaneSettings::Feed => PaneKind::Feed,
            PaneSettings::Ticker(s) => PaneKind::Ticker { symbol: s, trades: Vec::new(), loading: true },
            PaneSettings::Profile(q) => PaneKind::Profile { query: q, data: None, loading: true },
            PaneSettings::Politicians => PaneKind::Politicians,
            PaneSettings::Alerts => PaneKind::Alerts,
            PaneSettings::Watchlist => PaneKind::Watchlist,
            PaneSettings::Help => PaneKind::Help,
            PaneSettings::Wei => PaneKind::Wei,
            PaneSettings::Curve => PaneKind::Curve,
            PaneSettings::Search(q) => PaneKind::Search { query: q },
        }
    }

    fn save_settings(&self) {
        let panes = self
            .panes
            .iter()
            .map(|p| match &p.kind {
                PaneKind::Chart(c) => PaneSettings::Chart {
                    symbol: c.symbol.clone(),
                    tf: c.tf,
                    chart_type: c.chart_type,
                    log: c.log,
                    indicators: c.indicators.clone(),
                    drawings: c.drawings.clone(),
                    compare: c.compare.clone(),
                },
                PaneKind::Feed => PaneSettings::Feed,
                PaneKind::Ticker { symbol, .. } => PaneSettings::Ticker(symbol.clone()),
                PaneKind::Profile { query, .. } => PaneSettings::Profile(query.clone()),
                PaneKind::Politicians => PaneSettings::Politicians,
                PaneKind::Alerts => PaneSettings::Alerts,
                PaneKind::Watchlist => PaneSettings::Watchlist,
                PaneKind::Help => PaneSettings::Help,
                PaneKind::Wei => PaneSettings::Wei,
                PaneKind::Curve => PaneSettings::Curve,
                PaneKind::Search { query } => PaneSettings::Search(query.clone()),
            })
            .collect();
        let s = Settings { layout: self.layout, watchlist: self.watchlist.clone(), panes, sidebar: self.sidebar_open };
        if let Ok(json) = serde_json::to_vec_pretty(&s) {
            let path = settings_path();
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(tmp, path);
            }
        }
    }

    /// Add a pane, assigning an id and kicking off any loads it needs.
    fn push_pane(&mut self, kind: PaneKind) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let kind = self.init_kind(id, kind);
        self.panes.push(Pane { id, kind });
        self.panes.len() - 1
    }

    fn init_kind(&mut self, id: PaneId, kind: PaneKind) -> PaneKind {
        match kind {
            PaneKind::Chart(mut c) => {
                c.id = id;
                c.load(&self.worker);
                PaneKind::Chart(c)
            }
            PaneKind::Ticker { symbol, .. } => {
                self.worker.send(Request::TickerTrades { pane: id, symbol: symbol.clone() });
                PaneKind::Ticker { symbol, trades: Vec::new(), loading: true }
            }
            PaneKind::Profile { query, .. } => {
                self.worker.send(Request::Profile { pane: id, query: query.clone() });
                PaneKind::Profile { query, data: None, loading: true }
            }
            k => k,
        }
    }

    fn replace_pane(&mut self, idx: usize, kind: PaneKind) {
        let id = self.next_id;
        self.next_id += 1;
        let kind = self.init_kind(id, kind);
        let old = std::mem::replace(&mut self.panes[idx], Pane { id, kind });
        self.worker.send(Request::Forget(old.id));
    }

    fn fit_panes_to_layout(&mut self) {
        let n = self.layout.slots();
        while self.panes.len() < n {
            let has = |f: fn(&PaneKind) -> bool| self.panes.iter().any(|p| f(&p.kind));
            let kind = if !has(|k| matches!(k, PaneKind::Feed)) {
                PaneKind::Feed
            } else if !has(|k| matches!(k, PaneKind::Watchlist)) {
                PaneKind::Watchlist
            } else if !has(|k| matches!(k, PaneKind::Wei)) {
                PaneKind::Wei
            } else if !has(|k| matches!(k, PaneKind::Curve)) {
                PaneKind::Curve
            } else if !has(|k| matches!(k, PaneKind::Politicians)) {
                PaneKind::Politicians
            } else if !has(|k| matches!(k, PaneKind::Alerts)) {
                PaneKind::Alerts
            } else {
                let sym = ["BTCUSD Curncy", "SPX Index", "EURUSD Curncy", "CL1 Comdty"][self.panes.len() % 4];
                PaneKind::Chart(Box::new(ChartState::new(0, sym, Timeframe::D1)))
            };
            self.push_pane(kind);
        }
        for p in self.panes.drain(n.min(self.panes.len())..) {
            self.worker.send(Request::Forget(p.id));
        }
        self.focus = self.focus.min(n - 1);
        self.last_opened = self.last_opened.filter(|&i| i < n);
    }

    /// Where to show a non-chart panel: an existing pane of the same type, else a pane
    /// whose kind is shown twice, else a non-chart pane that wasn't just opened, else the
    /// pane after the focused chart.
    fn target_pane(&self, same: impl Fn(&PaneKind) -> bool) -> usize {
        if let Some(i) = self.panes.iter().position(|p| same(&p.kind)) {
            return i;
        }
        let disc = |i: usize| std::mem::discriminant(&self.panes[i].kind);
        let n = self.panes.len();
        if let Some(i) = (0..n).rev().find(|&i| {
            !matches!(self.panes[i].kind, PaneKind::Chart(_)) && (0..n).any(|j| j != i && disc(j) == disc(i))
        }) {
            return i;
        }
        let candidates: Vec<usize> = (0..n).filter(|&i| !matches!(self.panes[i].kind, PaneKind::Chart(_))).collect();
        if let Some(&i) = candidates.iter().find(|&&i| Some(i) != self.last_opened && i != self.focus) {
            return i;
        }
        if let Some(&i) = candidates.first() {
            return i;
        }
        if n > 1 { (self.focus + 1) % n } else { self.focus }
    }

    fn chart_index(&mut self) -> usize {
        if matches!(self.panes[self.focus].kind, PaneKind::Chart(_)) {
            return self.focus;
        }
        if let Some(i) = self.panes.iter().position(|p| matches!(p.kind, PaneKind::Chart(_))) {
            return i;
        }
        let f = self.focus;
        self.replace_pane(f, PaneKind::Chart(Box::new(ChartState::new(0, "SPY", Timeframe::D1))));
        f
    }

    fn with_chart(&mut self, f: impl FnOnce(&mut ChartState, &Worker)) {
        let i = self.chart_index();
        self.focus = i;
        if let PaneKind::Chart(c) = &mut self.panes[i].kind {
            f(c, &self.worker);
        }
    }

    fn open_chart(&mut self, symbol: &str) {
        self.with_chart(|c, w| c.set_symbol(symbol, w));
        self.status = tr_data::security(symbol).key();
    }

    fn open_panel(&mut self, kind: PaneKind) {
        let disc = std::mem::discriminant(&kind);
        let idx = self.target_pane(|k| std::mem::discriminant(k) == disc);
        self.replace_pane(idx, kind);
        self.focus = idx;
        self.last_opened = Some(idx);
    }

    fn run_actions(&mut self, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::OpenChart(s) => self.open_chart(&s),
                Action::OpenTicker(s) => {
                    self.open_panel(PaneKind::Ticker { symbol: s, trades: Vec::new(), loading: true })
                }
                Action::OpenProfile(q) => self.open_panel(PaneKind::Profile { query: q, data: None, loading: true }),
                Action::AddToWatchlist(s) => {
                    let key = tr_data::security(&s).key();
                    if !self.watchlist.contains(&key) {
                        self.status = format!("Added {key} to the watchlist");
                        self.watchlist.push(key);
                        self.last_quotes = None;
                    }
                }
                Action::Track(q, on) => {
                    self.worker.send(Request::Track { query: q.clone(), on });
                    // Refresh any open profile so the star updates.
                    for p in &mut self.panes {
                        if let PaneKind::Profile { query, .. } = &p.kind {
                            self.worker.send(Request::Profile { pane: p.id, query: query.clone() });
                        }
                    }
                }
            }
        }
    }

    fn execute(&mut self, input: &str) {
        let Some(cmd) = command::parse(input) else {
            self.status = format!("Unknown command: {input}  (HELP lists functions)");
            return;
        };
        self.cmd_history.push(input.trim().to_string());
        match cmd {
            Command::Symbol(s) => self.open_chart(&s),
            Command::SymbolFunction(s, Function::Congress) => {
                self.open_panel(PaneKind::Ticker { symbol: s.clone(), trades: Vec::new(), loading: true });
                self.status = format!("{s}: congressional trades");
            }
            Command::SymbolFunction(s, f) => {
                self.open_chart(&s);
                self.apply(f);
            }
            Command::Function(f) => self.apply(f),
        }
    }

    fn apply(&mut self, f: Function) {
        match f {
            Function::Timeframe(tf) => self.with_chart(|c, w| c.set_timeframe(tf, w)),
            Function::ChartType(t) => self.with_chart(|c, _| c.chart_type = t),
            Function::Indicator(spec) => self.with_chart(|c, w| c.toggle_indicator(spec, w)),
            Function::ClearIndicators => self.with_chart(|c, _| c.clear_indicators()),
            Function::Compare(s) => self.with_chart(|c, w| c.set_compare(s, w)),
            Function::LogScale => self.with_chart(|c, _| c.log = !c.log),
            Function::Range(r) => self.with_chart(|c, w| c.set_range(r, w)),
            Function::Draw(t) => self.with_chart(|c, _| c.start_tool(t)),
            Function::ClearDrawings => self.with_chart(|c, _| c.drawings.clear()),
            Function::Magnet => self.with_chart(|c, _| c.magnet = !c.magnet),
            Function::Congress => self.open_panel(PaneKind::Feed),
            Function::Politician(q) if q.trim().is_empty() => self.open_panel(PaneKind::Politicians),
            Function::Politician(q) => self.open_panel(PaneKind::Profile { query: q, data: None, loading: true }),
            Function::Track(q) => self.worker.send(Request::Track { query: q, on: true }),
            Function::Untrack(q) => self.worker.send(Request::Track { query: q, on: false }),
            Function::Alerts => {
                self.worker.send(Request::Alerts);
                self.open_panel(PaneKind::Alerts);
            }
            Function::Refresh => self.worker.send(Request::RefreshCongress),
            Function::Watchlist => self.open_panel(PaneKind::Watchlist),
            Function::WatchAdd(words) => {
                for key in keys_from_words(&words) {
                    if !self.watchlist.contains(&key) {
                        self.watchlist.push(key);
                    }
                }
                self.last_quotes = None;
            }
            Function::WatchRemove(words) => {
                let keys = keys_from_words(&words);
                self.watchlist.retain(|s| !keys.contains(s));
            }
            Function::Layout(l) => match Layout::parse(&l) {
                Some(l) => {
                    self.layout = l;
                    self.fit_panes_to_layout();
                }
                None => self.status = format!("Unknown layout {l}; use 1, 2, 2H, 3, 4 or 6"),
            },
            Function::NewPane => {
                let next = Layout::ALL.into_iter().find(|l| l.slots() > self.layout.slots());
                if let Some(l) = next {
                    self.layout = l;
                    self.fit_panes_to_layout();
                }
            }
            Function::ClosePane => {
                if self.panes.len() > 1 {
                    let old = self.panes.remove(self.focus);
                    self.worker.send(Request::Forget(old.id));
                    let prev =
                        Layout::ALL.into_iter().rev().find(|l| l.slots() <= self.panes.len()).unwrap_or(Layout::One);
                    self.layout = prev;
                    self.fit_panes_to_layout();
                }
            }
            Function::Help => self.open_panel(PaneKind::Help),
            Function::Wei => {
                self.last_wei = None;
                self.open_panel(PaneKind::Wei);
            }
            Function::Curve => {
                self.worker.send(Request::YieldCurves);
                self.last_curve = Some(Instant::now());
                self.open_panel(PaneKind::Curve);
            }
            Function::Search(q) => {
                if !q.trim().is_empty() {
                    self.worker.send(Request::Search(q.clone()));
                }
                self.open_panel(PaneKind::Search { query: q });
            }
        }
    }

    // ---- worker responses ----------------------------------------------------------------

    fn chart_mut(&mut self, pane: PaneId) -> Option<&mut ChartState> {
        self.panes.iter_mut().find(|p| p.id == pane).and_then(|p| match &mut p.kind {
            PaneKind::Chart(c) => Some(c.as_mut()),
            _ => None,
        })
    }

    fn drain(&mut self) {
        while let Ok(r) = self.worker.rx.try_recv() {
            match r {
                Response::History {
                    pane,
                    generation,
                    symbol,
                    tf,
                    bars,
                    gpu,
                    prepended,
                    from_cache,
                    exhausted,
                    live,
                } => {
                    let worker = &self.worker;
                    if let Some(PaneKind::Chart(c)) = self.panes.iter_mut().find(|p| p.id == pane).map(|p| &mut p.kind)
                        && c.symbol == symbol
                        && c.tf == tf
                    {
                        c.on_history(generation, bars, gpu, prepended, from_cache, exhausted, live, worker);
                    }
                }
                Response::HistoryError { pane, generation, error } => {
                    // Errors while paging just mean "no older history"; only report failed loads.
                    let empty = self.chart_mut(pane).map(|c| {
                        c.on_history_error(generation, error.clone());
                        c.is_empty()
                    });
                    if empty == Some(true) {
                        self.status = format!("data error: {error}");
                    }
                }
                Response::Indicators { pane, generation, built } => {
                    if let Some(c) = self.chart_mut(pane) {
                        c.on_indicators(generation, built);
                    }
                }
                Response::Compare { pane, generation, symbol, line } => {
                    if let Some(c) = self.chart_mut(pane) {
                        c.on_compare(generation, &symbol, line);
                    }
                }
                Response::Markers { pane, symbol, trades } => {
                    if let Some(c) = self.chart_mut(pane) {
                        c.on_markers(&symbol, trades);
                    }
                }
                Response::Quotes(q) => {
                    if q.len() > 20 {
                        self.wei_updated = Some(format!("updated {}", chrono::Local::now().format("%H:%M:%S")));
                    }
                    for x in q {
                        self.quotes.insert(x.symbol.clone(), x);
                    }
                }
                Response::Latest(t) => self.latest = t,
                Response::TickerTrades { pane, symbol, trades } => {
                    if let Some(p) = self.panes.iter_mut().find(|p| p.id == pane)
                        && let PaneKind::Ticker { symbol: s, trades: t, loading } = &mut p.kind
                        && *s == symbol
                    {
                        *t = trades;
                        *loading = false;
                    }
                }
                Response::Profile { pane, profile, query } => {
                    if let Some(p) = self.panes.iter_mut().find(|p| p.id == pane)
                        && let PaneKind::Profile { query: q, data, loading } = &mut p.kind
                        && *q == query
                    {
                        *data = profile;
                        *loading = false;
                    }
                }
                Response::Politicians(list) => self.politicians = list,
                Response::Alerts(a) => self.alerts = a,
                Response::NewAlerts(new) => {
                    for a in new {
                        tracing::info!("ALERT: {}", a.message);
                        self.toasts.push((a.message, Instant::now()));
                    }
                    self.worker.send(Request::Alerts);
                }
                Response::Status(s) => self.status = s,
                Response::Curves(c) => self.curves = Some(c),
                Response::SearchHits { query, hits } => self.net_hits = (query, hits),
                Response::CongressRefreshed(summary) => {
                    self.congress_status = format!("last ingest: {summary}");
                    self.status = format!("Congress disclosures updated: {summary}");
                    self.refresh_congress_views();
                }
            }
        }
    }

    fn refresh_congress_views(&mut self) {
        self.worker.send(Request::Latest { limit: 2_000 });
        self.worker.send(Request::SearchPoliticians(String::new()));
        for p in &self.panes {
            match &p.kind {
                PaneKind::Ticker { symbol, .. } => {
                    self.worker.send(Request::TickerTrades { pane: p.id, symbol: symbol.clone() })
                }
                PaneKind::Chart(c) => self.worker.send(Request::Markers { pane: p.id, symbol: c.symbol.clone() }),
                _ => {}
            }
        }
        self.last_feed = Some(Instant::now());
    }

    // ---- input ---------------------------------------------------------------------------

    /// Global and chart hotkeys. Runs before the command line so it can claim keys; when
    /// the command line holds text, keys belong to it.
    fn hotkeys(&mut self, ctx: &egui::Context) {
        // ⌃⌘S on macOS (Ctrl+S elsewhere) toggles the sidebar, as in native apps.
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::COMMAND, Key::S)) {
            self.sidebar_open = !self.sidebar_open;
        }
        let cmd_id = egui::Id::new("command-line");
        let focused = ctx.memory(|m| m.focused());
        let cmd_focused = focused == Some(cmd_id);
        let other_text_focused = focused.is_some() && !cmd_focused;
        if other_text_focused {
            return;
        }
        if !cmd_focused {
            // Type-anywhere: capture keystrokes until the command line has focus.
            // Judge modifiers per key event: the Alt release can arrive in the same frame
            // as Alt+T, so the frame-level modifier state is unreliable.
            let typed: String = ctx.input(|i| {
                let mut chord = false;
                let mut out = String::new();
                for e in &i.events {
                    match e {
                        Event::Key { pressed: true, modifiers, .. } => {
                            chord = modifiers.alt || modifiers.ctrl || modifiers.command
                        }
                        Event::Text(t) if !chord => out.push_str(t),
                        _ => {}
                    }
                }
                out
            });
            if !typed.is_empty() {
                let starts = !self.cmd.is_empty()
                    || typed.chars().next().is_some_and(|c| c.is_ascii_alphanumeric() || c == '^' || c == '?');
                if starts {
                    self.cmd.push_str(&typed.to_ascii_uppercase());
                    self.focus_cmd = true;
                }
            }
        }
        if !self.cmd.is_empty() {
            return;
        }
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if pressed(Modifiers::NONE, Key::Tab) {
            self.focus = (self.focus + 1) % self.panes.len();
        }
        if pressed(Modifiers::SHIFT, Key::Tab) {
            self.focus = (self.focus + self.panes.len() - 1) % self.panes.len();
        }
        for (k, f) in [
            (Key::F1, Function::Help),
            (Key::F2, Function::Congress),
            (Key::F3, Function::Politician(String::new())),
            (Key::F4, Function::Alerts),
            (Key::F5, Function::Watchlist),
            (Key::F6, Function::Refresh),
        ] {
            if pressed(Modifiers::NONE, k) {
                self.apply(f);
            }
        }
        if pressed(Modifiers::NONE, Key::F12) {
            self.show_perf = !self.show_perf;
        }
        let is_chart = matches!(self.panes[self.focus].kind, PaneKind::Chart(_));
        if !is_chart {
            return;
        }
        for (k, t) in
            [(Key::T, Tool::Trendline), (Key::H, Tool::Horizontal), (Key::V, Tool::Vertical), (Key::F, Tool::Fib)]
        {
            if pressed(Modifiers::ALT, k) {
                self.with_chart(|c, _| c.start_tool(t));
            }
        }
        if pressed(Modifiers::ALT, Key::R) {
            self.with_chart(|c, _| c.reset_view());
        }
        if pressed(Modifiers::ALT, Key::L) {
            self.with_chart(|c, _| c.log = !c.log);
        }
        if pressed(Modifiers::ALT, Key::M) {
            self.with_chart(|c, _| c.magnet = !c.magnet);
        }
        if pressed(Modifiers::NONE, Key::Escape) {
            self.with_chart(|c, _| {
                c.cancel();
            });
        }
        if pressed(Modifiers::NONE, Key::Delete) || pressed(Modifiers::NONE, Key::Backspace) {
            self.with_chart(|c, _| {
                c.delete_selected();
            });
        }
        if pressed(Modifiers::NONE, Key::ArrowLeft) {
            self.with_chart(|c, _| c.pan_bars(-5.0));
        }
        if pressed(Modifiers::NONE, Key::ArrowRight) {
            self.with_chart(|c, _| c.pan_bars(5.0));
        }
        if pressed(Modifiers::COMMAND, Key::ArrowUp)
            || pressed(Modifiers::NONE, Key::Plus)
            || pressed(Modifiers::NONE, Key::Equals)
        {
            self.with_chart(|c, _| c.zoom(1.25));
        }
        if pressed(Modifiers::COMMAND, Key::ArrowDown) || pressed(Modifiers::NONE, Key::Minus) {
            self.with_chart(|c, _| c.zoom(0.8));
        }
    }

    // ---- drawing -------------------------------------------------------------------------

    fn watchlist(&mut self, ui: &mut Ui) -> Vec<Action> {
        let mut out = Vec::new();
        ui.add(
            egui::Label::new(
                RichText::new("SECURITY             LAST       CHG    CHG%")
                    .font(mono(11.5))
                    .color(amber())
                    .background_color(theme::header()),
            )
            .extend(),
        );
        let mut remove = None;
        for s in &self.watchlist {
            ui.horizontal(|ui| {
                let sec = tr_data::security(s);
                let label = format!("{:<8} {:<6}", sec.ticker, sec.class.yellow_key().label());
                let r = ui.add(
                    egui::Label::new(RichText::new(label).font(mono(12.5)).color(amber()))
                        .extend()
                        .sense(egui::Sense::click()),
                );
                if r.clicked() {
                    out.push(Action::OpenChart(s.clone()));
                }
                if r.secondary_clicked() {
                    remove = Some(s.clone());
                }
                match self.quotes.get(s) {
                    Some(q) => {
                        let c = if q.change() >= 0.0 { theme::up() } else { theme::down() };
                        let d = sec.decimals(q.last);
                        let text = format!(
                            "{:>11.d$} {:>9} {:>+7.2}%",
                            q.last,
                            markets_view::change_text(&sec, q),
                            q.change_pct()
                        );
                        ui.add(egui::Label::new(RichText::new(text).font(mono(12.5)).color(c)).extend());
                    }
                    None => {
                        ui.label(RichText::new("       …").font(mono(12.5)).color(muted()));
                    }
                }
            });
        }
        if let Some(s) = remove {
            self.watchlist.retain(|x| *x != s);
        }
        ui.label(
            RichText::new("WL ADD <KEY> (e.g. WL ADD SOLUSD CRNCY) · right-click to remove · click to chart")
                .font(mono(10.5))
                .color(muted()),
        );
        out
    }

    fn help(ui: &mut Ui) {
        const HELP: &str = "\
COMMAND LINE  <SECURITY> [<FUNCTION>] <GO>   (Enter = <GO>; type anywhere to start)
  Market keys: AAPL <EQUITY> · SPY <EQUITY> · SPX <INDEX> · BTCUSD <CRNCY> · EURUSD <CRNCY>
               US10Y <GOVT> · CL1 <CMDTY> · GC1 <CMDTY>   (the key is optional for known tickers)
  Typing shows matches across all asset classes: ↑/↓ select, Tab complete, Enter load.
  WEI                  world markets overview     GC  US Treasury yield curve
  SECF <query>         security search (e.g. SECF gold cmdty, SECF yen)
  Crypto charts update live (Binance websocket; Coinbase fallback).
  1 5 15 30 1H 4H D 1W 1M   interval (1m..1 month)      1D 5D 3M 6M YTD 1Y 5Y ALL  ranges
  CANDLE BAR LINE AREA chart type                       LOG  log scale
  SMA 20  EMA 50  VWAP  BB 20 2  RSI 14  MACD 12 26 9   toggle indicators (CLEAR)
  CMP MSFT             compare (rebased to the main symbol; CMP alone removes)
  TL  HL  VL  FIB      trendline / horizontal / vertical / fib retracement (CLEAR DRAW)
  MAGNET               snap drawings to OHLC

CONGRESS (STOCK Act PTRs from Senate eFD + House Clerk, refreshed hourly)
  CONG                 latest trades feed           NVDA CONG   who in Congress traded NVDA
  POL                  politician directory          POL PELOSI  profile + est. performance
  TRACK PELOSI         alert on new filings          UNTRACK PELOSI
  ALERTS               alerts list                   REFRESH     ingest now
  Chart markers: B/S circles at transaction dates, blue = D, red = R, purple = I.
  Hover a marker for the politician, amount, filed date and disclosure lag.

LAYOUT / PANES
  LAYOUT 1|2|2H|3|4|6   NEW   CLOSE   TAB / SHIFT+TAB cycle focus   W  watchlist (WL ADD X)

CHART HOTKEYS (focused chart)
  wheel zoom · drag pan · drag price axis stretch · drag time axis zoom · double-click reset
  ←/→ pan · Ctrl/Cmd+↑/↓ or +/- zoom · Alt+R reset · Alt+L log · Alt+M magnet
  Alt+T trendline · Alt+H horizontal · Alt+V vertical · Alt+F fib · Esc cancel · Del remove
  F12 frame-time readout (UI CPU ms, frame interval, fps)

DATA  crypto: Binance/Coinbase · yields: Treasury.gov (FRED) · FX/indices/futures/ETFs/stocks: Yahoo
      (Alpaca for stocks with APCA_API_KEY_ID/APCA_API_SECRET_KEY) · TR_PROVIDER=sample offline.";
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(HELP).font(mono(12.0)).color(text()));
        });
    }

    fn periodic(&mut self) {
        let visible_wl = self.sidebar_open || self.panes.iter().any(|p| matches!(p.kind, PaneKind::Watchlist));
        if visible_wl && self.last_quotes.is_none_or(|t| t.elapsed() > Duration::from_secs(20)) {
            self.worker.send(Request::Quotes(self.watchlist.clone()));
            self.last_quotes = Some(Instant::now());
        }
        let visible = |f: fn(&PaneKind) -> bool| self.panes.iter().any(|p| f(&p.kind));
        if visible(|k| matches!(k, PaneKind::Wei))
            && self.last_wei.is_none_or(|t| t.elapsed() > Duration::from_secs(20))
        {
            self.worker.send(Request::Quotes(markets_view::overview_keys()));
            self.last_wei = Some(Instant::now());
        }
        if visible(|k| matches!(k, PaneKind::Curve))
            && self.last_curve.is_none_or(|t| t.elapsed() > Duration::from_secs(600))
        {
            self.worker.send(Request::YieldCurves);
            self.last_curve = Some(Instant::now());
        }
        if let Some((q, at)) = &self.search_pending
            && at.elapsed() > Duration::from_millis(300)
        {
            self.worker.send(Request::Search(q.clone()));
            self.search_pending = None;
        }
        if self.last_feed.is_none_or(|t| t.elapsed() > Duration::from_secs(60)) {
            self.worker.send(Request::Latest { limit: 2_000 });
            self.last_feed = Some(Instant::now());
        }
        if self.last_save.elapsed() > Duration::from_secs(20) {
            self.save_settings();
            self.last_save = Instant::now();
        }
        self.toasts.retain(|(_, t)| t.elapsed() < Duration::from_secs(10));
    }
}

/// Apply the native macOS window chrome (unified toolbar, inset traffic lights, sidebar
/// vibrancy). Returns whether the window is now translucent.
#[cfg(target_os = "macos")]
fn style_native_window(frame: &eframe::Frame) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let vibrancy = std::env::var("TR_VIBRANCY").map_or(true, |v| v != "0");
    let Ok(handle) = frame.window_handle() else { return false };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else { return false };
    // SAFETY: called on the main thread (eframe runs `ui` there) with a live NSView.
    let ok = unsafe { tr_macos::style_window(h.ns_view.as_ptr(), vibrancy) };
    tracing::info!("macOS window chrome applied: {ok} (vibrancy {vibrancy})");
    ok && vibrancy
}

#[cfg(not(target_os = "macos"))]
fn style_native_window(_frame: &eframe::Frame) -> bool {
    false
}

impl eframe::App for TerminalApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.vibrancy { [0.0; 4] } else { visuals.window_fill.to_normalized_gamma_f32() }
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let frame_start = Instant::now();
        if let Some(prev) = self.last_frame.replace(frame_start) {
            let dt = prev.elapsed().as_secs_f64() * 1e3;
            self.perf_dt_ms = if self.perf_dt_ms == 0.0 { dt } else { self.perf_dt_ms * 0.9 + dt * 0.1 };
        }
        let ctx = ui.ctx().clone();
        self.drain();
        self.periodic();

        if !self.window_styled {
            self.window_styled = true;
            self.vibrancy = style_native_window(frame);
        }
        if self.last_accent_poll.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
            if let Some([r, g, b]) = tr_macos::accent_color() {
                self.accent = Color32::from_rgb(r, g, b);
            }
            self.last_accent_poll = Some(Instant::now());
        }
        theme::refresh(&ctx, self.accent);

        let screen = ui.max_rect();
        let open = anim::spring_with(
            &ctx,
            egui::Id::new("sidebar-open"),
            if self.sidebar_open { 1.0 } else { 0.0 },
            0.32,
            0.9,
        );
        let sb_w = (SIDEBAR_W * open).round();
        let main = Rect::from_min_max(pos2(screen.left() + sb_w, screen.top()), screen.max);
        ui.painter().rect_filled(main, 0.0, theme::window());
        let toolbar = Rect::from_min_size(main.min, vec2(main.width(), TOOLBAR_H));
        let status = Rect::from_min_max(pos2(main.left(), main.bottom() - STATUS_H), main.max);
        let area = Rect::from_min_max(
            pos2(main.left() + GAP, toolbar.bottom() + GAP),
            pos2(main.right() - GAP, status.top() - GAP * 0.5),
        );

        self.hotkeys(&ctx);
        let mut sidebar_cmds = Vec::new();
        if sb_w >= 1.0 {
            let full = Rect::from_min_size(
                pos2(screen.left() + sb_w - SIDEBAR_W, screen.top()),
                vec2(SIDEBAR_W, screen.height()),
            );
            let mut child = ui.new_child(UiBuilder::new().max_rect(full).id_salt("sidebar-root"));
            child.set_clip_rect(Rect::from_min_max(screen.min, pos2(screen.left() + sb_w, screen.bottom())));
            sidebar_cmds = self.sidebar(&mut child, full);
        }
        self.toolbar(ui, toolbar, (TRAFFIC_INSET - sb_w).max(0.0));
        self.status_bar(ui, status);

        let rects = self.layout.rects(area, GAP);
        let mut actions = Vec::new();
        for c in sidebar_cmds {
            match c {
                SidebarCmd::Action(a) => actions.push(a),
                SidebarCmd::Function(f) => self.apply(f),
            }
        }
        let mut new_focus = None;
        for (i, rect) in rects.into_iter().enumerate() {
            let focused = i == self.focus;
            let Some(pane) = self.panes.get_mut(i) else { break };
            match &mut pane.kind {
                PaneKind::Chart(c) => {
                    if c.show(ui, rect, focused, &self.worker) {
                        new_focus = Some(i);
                    }
                    actions.append(&mut c.actions);
                }
                kind => {
                    let title = kind.title();
                    let inner = Self::pane_card(ui, rect, &title, focused, pane.id);
                    let mut child = ui.new_child(UiBuilder::new().max_rect(inner).id_salt(("pane", pane.id)));
                    child.set_clip_rect(inner);
                    match kind {
                        PaneKind::Feed => actions.extend(congress_view::feed(
                            &mut child,
                            &self.latest,
                            &mut self.feed_filter,
                            &self.congress_status,
                        )),
                        PaneKind::Ticker { symbol, trades, loading } => {
                            if *loading {
                                child.label(RichText::new("Loading…").color(muted()));
                            } else {
                                actions.extend(congress_view::ticker_trades(&mut child, symbol, trades));
                            }
                        }
                        PaneKind::Profile { query, data, loading } => {
                            actions.extend(congress_view::profile(&mut child, query, data.as_deref(), *loading))
                        }
                        PaneKind::Politicians => actions.extend(congress_view::politicians(
                            &mut child,
                            &self.politicians,
                            &mut self.pol_search,
                        )),
                        PaneKind::Alerts => {
                            let (a, seen) = congress_view::alerts(&mut child, &self.alerts);
                            actions.extend(a);
                            if seen {
                                self.worker.send(Request::MarkAlertsSeen);
                            }
                        }
                        PaneKind::Watchlist => {}
                        PaneKind::Help => Self::help(&mut child),
                        PaneKind::Wei => {
                            actions.extend(markets_view::wei(&mut child, &self.quotes, self.wei_updated.as_deref()))
                        }
                        PaneKind::Curve => actions.extend(markets_view::curve(&mut child, self.curves.as_deref())),
                        PaneKind::Search { query } => {
                            let hits = if self.net_hits.0 == *query { self.net_hits.1.as_slice() } else { &[] };
                            let (a, changed) = markets_view::search(&mut child, query, hits);
                            actions.extend(a);
                            if changed && query.trim().len() >= 2 {
                                self.search_pending = Some((query.clone(), Instant::now()));
                            }
                        }
                        PaneKind::Chart(_) => unreachable!(),
                    }
                    if matches!(pane.kind, PaneKind::Watchlist) {
                        let mut child = ui.new_child(UiBuilder::new().max_rect(inner).id_salt(("wl", pane.id)));
                        let wl = self.watchlist(&mut child);
                        actions.extend(wl);
                    }
                    let clicked = ui.input(|inp| {
                        inp.pointer.primary_pressed() && inp.pointer.interact_pos().is_some_and(|p| rect.contains(p))
                    });
                    if clicked {
                        new_focus = Some(i);
                    }
                }
            }
        }
        if let Some(f) = new_focus {
            self.focus = f;
        }
        self.run_actions(actions);
        self.toasts(&ctx);

        // TR_SMOKE_SECS=N: render for N seconds, log frame stats and quit (CI smoke test).
        if let Some(secs) = std::env::var("TR_SMOKE_SECS").ok().and_then(|v| v.parse::<f64>().ok()) {
            ctx.request_repaint();
            if !self.smoke_done && self.started.elapsed().as_secs_f64() > secs {
                self.smoke_done = true;
                tracing::info!(
                    "smoke: {} frames in {:.1}s, ui cpu {:.2} ms/frame, panes {}",
                    ctx.cumulative_frame_nr(),
                    self.started.elapsed().as_secs_f64(),
                    self.perf_cpu_ms,
                    self.panes.len()
                );
                self.save_settings();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        let cpu = frame_start.elapsed().as_secs_f64() * 1e3;
        self.perf_cpu_ms = self.perf_cpu_ms * 0.9 + cpu * 0.1;
        if self.show_perf {
            // Measure continuously while the readout is visible.
            ctx.request_repaint();
        }
        // Keep the clock ticking and pick up worker results without spinning.
        ctx.request_repaint_after(Duration::from_millis(500));
        if self.started.elapsed() < Duration::from_secs(2) {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self) {
        self.save_settings();
    }
}
