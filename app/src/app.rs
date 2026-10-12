//! Top-level terminal: command line, tiling panes, hotkeys, status/function-key bars.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Event, Key, Modifiers, Rect, RichText, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};
use serde::{Deserialize, Serialize};
use tr_chart::drawings::{Drawing, Tool};
use tr_chart::scene::ChartType;
use tr_congress::Trade;
use tr_congress::db::{Alert, PoliticianSummary};
use tr_core::Timeframe;
use tr_data::Quote;

use crate::chart_view::ChartState;
use crate::command::{self, Command, Function, IndicatorSpec};
use crate::congress_view::{self, Action, FeedFilter};
use crate::layout::Layout;
use crate::theme::{self, AMBER, AMBER_DIM, MUTED, TEXT, mono};
use crate::worker::{PaneId, Profile, Request, Response, Worker, data_dir};

const TOP_H: f32 = 40.0;
const BOTTOM_H: f32 = 20.0;
const PANE_TITLE_H: f32 = 18.0;

pub enum PaneKind {
    Chart(Box<ChartState>),
    Feed,
    Ticker { symbol: String, trades: Vec<Trade>, loading: bool },
    Profile { query: String, data: Option<Box<Profile>>, loading: bool },
    Politicians,
    Alerts,
    Watchlist,
    Help,
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
}

pub struct TerminalApp {
    worker: Worker,
    provider: String,
    layout: Layout,
    panes: Vec<Pane>,
    focus: usize,
    next_id: PaneId,
    last_opened: Option<usize>,

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
    /// Frame-time readout (F12): smoothed CPU time of `ui()` and interval between frames.
    show_perf: bool,
    perf_cpu_ms: f64,
    perf_dt_ms: f64,
    last_frame: Option<Instant>,
}

fn settings_path() -> std::path::PathBuf {
    data_dir().join("settings.json")
}

impl TerminalApp {
    pub fn new(ctx: &egui::Context, worker: Worker, provider: String) -> Self {
        theme::apply(ctx);
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
            watchlist: ["SPY", "QQQ", "AAPL", "MSFT", "NVDA", "AMZN", "GOOGL", "META", "TSLA", "JPM"]
                .map(String::from)
                .to_vec(),
            quotes: HashMap::new(),
            last_quotes: None,
            last_feed: None,
            last_save: Instant::now(),
            started: Instant::now(),
            show_perf: std::env::var_os("TR_FPS").is_some(),
            perf_cpu_ms: 0.0,
            perf_dt_ms: 0.0,
            last_frame: None,
        };
        match settings {
            Some(s) => {
                app.layout = s.layout;
                app.watchlist = s.watchlist;
                for p in s.panes {
                    let kind = app.kind_from_settings(p);
                    app.push_pane(kind);
                }
            }
            None => {
                for kind in [
                    PaneKind::Chart(Box::new(ChartState::new(0, "AAPL", Timeframe::D1))),
                    PaneKind::Feed,
                    PaneKind::Ticker { symbol: "AAPL".into(), trades: Vec::new(), loading: true },
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
            })
            .collect();
        let s = Settings { layout: self.layout, watchlist: self.watchlist.clone(), panes };
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
        self.panes[idx] = Pane { id, kind };
    }

    fn fit_panes_to_layout(&mut self) {
        let n = self.layout.slots();
        while self.panes.len() < n {
            let has = |f: fn(&PaneKind) -> bool| self.panes.iter().any(|p| f(&p.kind));
            let kind = if !has(|k| matches!(k, PaneKind::Feed)) {
                PaneKind::Feed
            } else if !has(|k| matches!(k, PaneKind::Watchlist)) {
                PaneKind::Watchlist
            } else if !has(|k| matches!(k, PaneKind::Politicians)) {
                PaneKind::Politicians
            } else if !has(|k| matches!(k, PaneKind::Alerts)) {
                PaneKind::Alerts
            } else {
                let sym = ["SPY", "QQQ", "NVDA", "MSFT"][self.panes.len() % 4];
                PaneKind::Chart(Box::new(ChartState::new(0, sym, Timeframe::D1)))
            };
            self.push_pane(kind);
        }
        self.panes.truncate(n);
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
        self.status = format!("{symbol} US Equity");
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
            Function::WatchAdd(syms) => {
                for s in syms {
                    if !self.watchlist.contains(&s) {
                        self.watchlist.push(s);
                    }
                }
                self.last_quotes = None;
            }
            Function::WatchRemove(syms) => self.watchlist.retain(|s| !syms.contains(s)),
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
                    self.panes.remove(self.focus);
                    let prev =
                        Layout::ALL.into_iter().rev().find(|l| l.slots() <= self.panes.len()).unwrap_or(Layout::One);
                    self.layout = prev;
                    self.fit_panes_to_layout();
                }
            }
            Function::Help => self.open_panel(PaneKind::Help),
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
                Response::History { pane, generation, symbol, tf, bars, gpu, prepended, from_cache, exhausted } => {
                    let worker = &self.worker;
                    if let Some(PaneKind::Chart(c)) = self.panes.iter_mut().find(|p| p.id == pane).map(|p| &mut p.kind)
                        && c.symbol == symbol
                        && c.tf == tf
                    {
                        c.on_history(generation, bars, gpu, prepended, from_cache, exhausted, worker);
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

    fn top_bar(&mut self, ui: &mut Ui, rect: Rect) -> bool {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, theme::PANEL);
        let logo = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(118.0, 20.0));
        p.rect_filled(logo, 0.0, AMBER);
        p.text(logo.center(), Align2::CENTER_CENTER, "TERMINAL-RS", mono(13.0), Color32::BLACK);
        let now = chrono::Local::now();
        let clock = format!("{}  {} UTC", now.format("%a %d %b %H:%M:%S"), chrono::Utc::now().format("%H:%M"));
        p.text(pos2(rect.right() - 8.0, rect.top() + 14.0), Align2::RIGHT_CENTER, clock, mono(12.0), AMBER);
        let unread = self.alerts.iter().filter(|a| !a.seen).count();
        let right_info = format!(
            "LAYOUT {}  DATA {}{}",
            self.layout.label(),
            self.provider.to_uppercase(),
            if unread > 0 { format!("   ALERTS {unread}") } else { String::new() }
        );
        p.text(
            pos2(rect.right() - 8.0, rect.top() + 31.0),
            Align2::RIGHT_CENTER,
            right_info,
            mono(11.0),
            if unread > 0 { AMBER } else { MUTED },
        );

        let cmd_rect = Rect::from_min_max(
            pos2(logo.right() + 8.0, rect.top() + 4.0),
            pos2(rect.right() - 300.0, rect.top() + 24.0),
        );
        let mut child = ui.new_child(UiBuilder::new().max_rect(cmd_rect));
        let id = egui::Id::new("command-line");
        let te = egui::TextEdit::singleline(&mut self.cmd)
            .id(id)
            .font(mono(14.0))
            .text_color(AMBER)
            .hint_text(
                RichText::new("<TICKER> <GO>   e.g. AAPL · NVDA CONG · POL PELOSI · RSI · 5 · LAYOUT 4")
                    .color(AMBER_DIM),
            )
            .desired_width(cmd_rect.width())
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(20, 16, 0))
                    .stroke(Stroke::new(1.0, AMBER_DIM))
                    .inner_margin(vec2(6.0, 2.0)),
            );
        let resp = child.add(te);
        if self.focus_cmd {
            resp.request_focus();
            // Place the cursor at the end after type-to-command.
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                let ccursor = egui::text::CCursor::new(self.cmd.chars().count());
                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                state.store(ui.ctx(), id);
            }
            self.focus_cmd = false;
        }
        let focused = resp.has_focus();
        let (enter, esc, up, down) = ui.input(|i| {
            (
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
            )
        });
        if resp.lost_focus() && enter {
            let input = std::mem::take(&mut self.cmd);
            if !input.trim().is_empty() {
                self.execute(&input);
            }
            self.cmd_hist_pos = None;
            // The command line stays live, Bloomberg-style.
            self.focus_cmd = true;
        } else if focused && esc {
            self.cmd.clear();
            resp.surrender_focus();
        } else if focused && (up || down) && !self.cmd_history.is_empty() {
            let n = self.cmd_history.len();
            let pos = match (self.cmd_hist_pos, up) {
                (None, true) => n - 1,
                (Some(p), true) => p.saturating_sub(1),
                (Some(p), false) if p + 1 < n => p + 1,
                _ => {
                    self.cmd_hist_pos = None;
                    self.cmd.clear();
                    return focused;
                }
            };
            self.cmd_hist_pos = Some(pos);
            self.cmd = self.cmd_history[pos].clone();
        }
        let hint = command::hint(&self.cmd);
        p.text(
            pos2(cmd_rect.left() + 2.0, rect.top() + 32.0),
            Align2::LEFT_CENTER,
            hint,
            mono(11.0),
            if self.cmd.is_empty() { MUTED } else { TEXT },
        );
        focused
    }

    fn bottom_bar(&self, ui: &Ui, rect: Rect) {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, theme::PANEL);
        let keys = [
            ("F1", "HELP"),
            ("F2", "CONG"),
            ("F3", "POL"),
            ("F4", "ALRT"),
            ("F5", "WL"),
            ("F6", "SYNC"),
            ("TAB", "PANE"),
            ("ALT+T/H/V/F", "DRAW"),
            ("ALT+R", "RESET"),
        ];
        let mut x = rect.left() + 4.0;
        for (k, label) in keys {
            let g = p.layout_no_wrap(k.to_string(), mono(11.0), Color32::BLACK);
            let box_r = Rect::from_min_size(pos2(x, rect.top() + 3.0), vec2(g.size().x + 6.0, rect.height() - 6.0));
            p.rect_filled(box_r, 0.0, theme::ORANGE_BAR);
            p.galley(pos2(x + 3.0, rect.center().y - g.size().y / 2.0), g, Color32::BLACK);
            x = box_r.right() + 3.0;
            let g = p.layout_no_wrap(label.to_string(), mono(11.0), TEXT);
            p.galley(pos2(x, rect.center().y - g.size().y / 2.0), g.clone(), TEXT);
            x += g.size().x + 10.0;
        }
        let status = if self.show_perf {
            format!("UI {:.2} ms  frame {:.1} ms ({:.0} fps)   {}", self.perf_cpu_ms, self.perf_dt_ms, 1000.0 / self.perf_dt_ms.max(0.1), self.status)
        } else {
            self.status.clone()
        };
        p.text(pos2(rect.right() - 6.0, rect.center().y), Align2::RIGHT_CENTER, status, mono(11.0), AMBER);
    }

    fn pane_frame(ui: &Ui, rect: Rect, title: &str, focused: bool) -> Rect {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, theme::BG);
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), PANE_TITLE_H));
        p.rect_filled(bar, 0.0, if focused { Color32::from_rgb(70, 50, 0) } else { theme::HEADER });
        p.text(bar.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, title, mono(12.0), AMBER);
        if focused {
            p.rect_stroke(rect, 0.0, Stroke::new(1.0, theme::FOCUS), StrokeKind::Inside);
        }
        Rect::from_min_max(pos2(rect.left() + 4.0, bar.bottom() + 2.0), pos2(rect.right() - 4.0, rect.bottom() - 2.0))
    }

    fn watchlist(&mut self, ui: &mut Ui) -> Vec<Action> {
        let mut out = Vec::new();
        ui.add(
            egui::Label::new(
                RichText::new("SYMBOL       LAST       CHG      CHG%")
                    .font(mono(11.5))
                    .color(AMBER)
                    .background_color(theme::HEADER),
            )
            .extend(),
        );
        let mut remove = None;
        for s in &self.watchlist {
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::Label::new(RichText::new(format!("{s:<8}")).font(mono(12.5)).color(AMBER))
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
                        let c = if q.change() >= 0.0 { theme::UP } else { theme::DOWN };
                        ui.label(
                            RichText::new(format!("{:>10.2} {:>+9.2} {:>+8.2}%", q.last, q.change(), q.change_pct()))
                                .font(mono(12.5))
                                .color(c),
                        );
                    }
                    None => {
                        ui.label(RichText::new("       …").font(mono(12.5)).color(MUTED));
                    }
                }
            });
        }
        if let Some(s) = remove {
            self.watchlist.retain(|x| *x != s);
        }
        ui.label(
            RichText::new("WL ADD <TICKER> · right-click to remove · click to chart").font(mono(10.5)).color(MUTED),
        );
        out
    }

    fn help(ui: &mut Ui) {
        const HELP: &str = "\
COMMAND LINE  <SECURITY> [<FUNCTION>] <GO>   (Enter = <GO>; type anywhere to start)
  AAPL                 load AAPL in the focused chart (also: AAPL US EQUITY)
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

DATA  APCA_API_KEY_ID/APCA_API_SECRET_KEY -> Alpaca; otherwise Yahoo; TR_PROVIDER=sample offline.";
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(HELP).font(mono(12.0)).color(TEXT));
        });
    }

    fn periodic(&mut self) {
        let visible_wl = self.panes.iter().any(|p| matches!(p.kind, PaneKind::Watchlist));
        if visible_wl && self.last_quotes.is_none_or(|t| t.elapsed() > Duration::from_secs(20)) {
            self.worker.send(Request::Quotes(self.watchlist.clone()));
            self.last_quotes = Some(Instant::now());
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

    fn toasts(&self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("toasts")));
        for (k, (msg, _)) in self.toasts.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(screen.right() - 470.0, screen.top() + 48.0 + k as f32 * 30.0),
                vec2(460.0, 26.0),
            );
            p.rect_filled(r, 0.0, Color32::from_rgb(60, 40, 0));
            p.rect_stroke(r, 0.0, Stroke::new(1.0, AMBER), StrokeKind::Inside);
            p.text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, format!("ALERT  {msg}"), mono(12.0), AMBER);
        }
    }
}

impl eframe::App for TerminalApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let frame_start = Instant::now();
        if let Some(prev) = self.last_frame.replace(frame_start) {
            let dt = prev.elapsed().as_secs_f64() * 1e3;
            self.perf_dt_ms = if self.perf_dt_ms == 0.0 { dt } else { self.perf_dt_ms * 0.9 + dt * 0.1 };
        }
        let ctx = ui.ctx().clone();
        self.drain();
        self.periodic();

        let screen = ui.max_rect();
        ui.painter().rect_filled(screen, 0.0, theme::BG);
        let top = Rect::from_min_size(screen.min, vec2(screen.width(), TOP_H));
        let bottom = Rect::from_min_max(pos2(screen.left(), screen.bottom() - BOTTOM_H), screen.max);
        let area = Rect::from_min_max(
            pos2(screen.left() + 2.0, top.bottom() + 2.0),
            pos2(screen.right() - 2.0, bottom.top() - 2.0),
        );

        self.hotkeys(&ctx);
        self.top_bar(ui, top);
        self.bottom_bar(ui, bottom);

        let rects = self.layout.rects(area, 3.0);
        let mut actions = Vec::new();
        let mut new_focus = None;
        for (i, rect) in rects.into_iter().enumerate() {
            let focused = i == self.focus;
            let Some(pane) = self.panes.get_mut(i) else { break };
            match &mut pane.kind {
                PaneKind::Chart(c) => {
                    if c.show(ui, rect, focused, &self.worker) {
                        new_focus = Some(i);
                    }
                }
                kind => {
                    let title = kind.title();
                    let inner = Self::pane_frame(ui, rect, &title, focused);
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
                                child.label(RichText::new("Loading…").color(MUTED));
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
