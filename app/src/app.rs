//! Top-level terminal: command line, docked panes, saved layouts, hotkeys and status bar.
//!
//! Submodules: `chrome` (toolbar, sidebar, status bar), `workspace` (dock rendering,
//! drag and drop, pop-out windows, link groups, saved layouts), `panels` (per-kind pane
//! content) and `settings_view` (preferences UI).

mod chrome;
mod panels;
mod settings_view;
mod workspace;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{Color32, Event, Key, Modifiers, Rect, Ui, UiBuilder, pos2, vec2};
use serde::{Deserialize, Serialize};
use tr_chart::drawings::Tool;
use tr_congress::Trade;
use tr_congress::db::{Alert, PoliticianSummary};
use tr_data::{NewsItem, OrderBook, Quote, SearchHit, YieldCurves};

use crate::anim;
use crate::chart_view::ChartState;
use crate::command::{self, Command, Function};
use crate::congress_view::{Action, FeedFilter};
use crate::dock::{Node, PaneId};
use crate::layout::Layout;
use crate::layouts::{LayoutDoc, Link, PaneSettings, Store, WindowGeometry};
use crate::markets_view;
use crate::prefs::{self, Prefs};
use crate::theme;
use crate::worker::{Profile, Request, Response, Worker, data_dir};

use chrome::{SIDEBAR_W, STATUS_H, SidebarCmd, TOOLBAR_H, TRAFFIC_INSET};
use settings_view::SettingsTab;

pub enum PaneKind {
    Chart(Box<ChartState>),
    Feed { filter: FeedFilter },
    Ticker { symbol: String, trades: Vec<Trade>, loading: bool },
    Profile { query: String, data: Option<Box<Profile>>, loading: bool },
    Politicians,
    Alerts,
    Watchlist,
    Help,
    Wei,
    Curve,
    Search { query: String },
    News(Box<NewsPane>),
    OrderBook(Box<BookPane>),
    Settings,
}

pub struct NewsPane {
    /// `None` = general market headlines.
    pub symbol: Option<String>,
    pub items: Vec<NewsItem>,
    pub error: Option<String>,
    pub loading: bool,
    pub fetched: Option<Instant>,
}

impl NewsPane {
    pub fn new(symbol: Option<String>) -> Self {
        Self { symbol, items: Vec::new(), error: None, loading: true, fetched: None }
    }
}

pub struct BookPane {
    pub symbol: String,
    pub depth: usize,
    /// Price bucket; 0 = raw levels.
    pub group: f64,
    pub book: Option<OrderBook>,
    pub error: Option<String>,
    pub fetched: Option<Instant>,
}

impl BookPane {
    pub fn new(symbol: &str, depth: usize, group: f64) -> Self {
        Self { symbol: symbol.into(), depth, group, book: None, error: None, fetched: None }
    }
}

impl PaneKind {
    /// Short tab title.
    fn tab_title(&self) -> String {
        let short = |s: &str| tr_data::security(s).ticker;
        match self {
            PaneKind::Chart(c) => short(&c.symbol),
            PaneKind::Feed { .. } => "Congress".into(),
            PaneKind::Ticker { symbol, .. } => format!("{} Congress", short(symbol)),
            PaneKind::Profile { query, data, .. } => {
                data.as_ref().map_or_else(|| query.clone(), |p| p.politician.name.clone())
            }
            PaneKind::Politicians => "Politicians".into(),
            PaneKind::Alerts => "Alerts".into(),
            PaneKind::Watchlist => "Watchlist".into(),
            PaneKind::Help => "Help".into(),
            PaneKind::Wei => "World Markets".into(),
            PaneKind::Curve => "Yield Curve".into(),
            PaneKind::Search { query } if query.is_empty() => "Search".into(),
            PaneKind::Search { query } => format!("Search: {query}"),
            PaneKind::News(n) => match &n.symbol {
                Some(s) => format!("{} News", short(s)),
                None => "Top News".into(),
            },
            PaneKind::OrderBook(b) => format!("{} Depth", short(&b.symbol)),
            PaneKind::Settings => "Settings".into(),
        }
    }

    /// The Bloomberg-style function code shown on the tab.
    fn code(&self) -> &'static str {
        match self {
            PaneKind::Chart(_) => "GP",
            PaneKind::Feed { .. } | PaneKind::Ticker { .. } => "CONG",
            PaneKind::Profile { .. } | PaneKind::Politicians => "POL",
            PaneKind::Alerts => "ALRT",
            PaneKind::Watchlist => "WL",
            PaneKind::Help => "HELP",
            PaneKind::Wei => "WEI",
            PaneKind::Curve => "GC",
            PaneKind::Search { .. } => "SECF",
            PaneKind::News(_) => "CN",
            PaneKind::OrderBook(_) => "OB",
            PaneKind::Settings => "PDF",
        }
    }

    /// The security this pane follows, for symbol-driven panels.
    fn symbol(&self) -> Option<&str> {
        match self {
            PaneKind::Chart(c) => Some(&c.symbol),
            PaneKind::Ticker { symbol, .. } => Some(symbol),
            PaneKind::News(n) => n.symbol.as_deref(),
            PaneKind::OrderBook(b) => Some(&b.symbol),
            _ => None,
        }
    }

    fn follows_symbol(&self) -> bool {
        matches!(self, PaneKind::Chart(_) | PaneKind::Ticker { .. } | PaneKind::News(_) | PaneKind::OrderBook(_))
    }

    fn settings(&self) -> PaneSettings {
        match self {
            PaneKind::Chart(c) => PaneSettings::Chart {
                symbol: c.symbol.clone(),
                tf: c.tf,
                chart_type: c.chart_type,
                log: c.log,
                indicators: c.indicators.clone(),
                drawings: c.drawings.clone(),
                compare: c.compare.clone(),
                show_volume: c.show_volume,
                show_congress: c.show_congress,
                magnet: c.magnet,
            },
            PaneKind::Feed { filter } => {
                PaneSettings::Feed((*filter != FeedFilter::default()).then(|| (*filter).into()))
            }
            PaneKind::Ticker { symbol, .. } => PaneSettings::Ticker(symbol.clone()),
            PaneKind::Profile { query, .. } => PaneSettings::Profile(query.clone()),
            PaneKind::Politicians => PaneSettings::Politicians,
            PaneKind::Alerts => PaneSettings::Alerts,
            PaneKind::Watchlist => PaneSettings::Watchlist,
            PaneKind::Help => PaneSettings::Help,
            PaneKind::Wei => PaneSettings::Wei,
            PaneKind::Curve => PaneSettings::Curve,
            PaneKind::Search { query } => PaneSettings::Search(query.clone()),
            PaneKind::News(n) => PaneSettings::News(n.symbol.clone()),
            PaneKind::OrderBook(b) => {
                PaneSettings::OrderBook { symbol: b.symbol.clone(), depth: b.depth, group: b.group }
            }
            PaneKind::Settings => PaneSettings::Settings,
        }
    }

    fn from_settings(p: PaneSettings) -> PaneKind {
        match p {
            PaneSettings::Chart {
                symbol,
                tf,
                chart_type,
                log,
                indicators,
                drawings,
                compare,
                show_volume,
                show_congress,
                magnet,
            } => {
                let mut c = ChartState::new(0, &symbol, tf);
                c.chart_type = chart_type;
                c.log = log;
                c.indicators = indicators;
                c.drawings = drawings;
                c.compare = compare;
                c.show_volume = show_volume;
                c.show_congress = show_congress;
                c.magnet = magnet;
                PaneKind::Chart(Box::new(c))
            }
            PaneSettings::Feed(f) => PaneKind::Feed { filter: f.map(Into::into).unwrap_or_default() },
            PaneSettings::Ticker(s) => PaneKind::Ticker { symbol: s, trades: Vec::new(), loading: true },
            PaneSettings::Profile(q) => PaneKind::Profile { query: q, data: None, loading: true },
            PaneSettings::Politicians => PaneKind::Politicians,
            PaneSettings::Alerts => PaneKind::Alerts,
            PaneSettings::Watchlist => PaneKind::Watchlist,
            PaneSettings::Help => PaneKind::Help,
            PaneSettings::Wei => PaneKind::Wei,
            PaneSettings::Curve => PaneKind::Curve,
            PaneSettings::Search(q) => PaneKind::Search { query: q },
            PaneSettings::News(s) => PaneKind::News(Box::new(NewsPane::new(s))),
            PaneSettings::OrderBook { symbol, depth, group } => {
                PaneKind::OrderBook(Box::new(BookPane::new(&symbol, depth, group)))
            }
            PaneSettings::Settings => PaneKind::Settings,
        }
    }
}

pub struct Pane {
    pub id: PaneId,
    pub kind: PaneKind,
    pub link: Option<Link>,
    /// User-chosen tab title.
    pub title: Option<String>,
    /// Set while popped out into its own window.
    pub window: Option<WindowGeometry>,
}

impl Pane {
    fn display_title(&self) -> String {
        self.title.clone().unwrap_or_else(|| self.kind.tab_title())
    }
}

/// App-wide state persisted in `settings.json` (layouts live in their own files).
#[derive(Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    watchlist: Vec<String>,
    #[serde(default = "default_true")]
    sidebar: bool,
    /// Name of the layout to restore on launch.
    #[serde(default)]
    active_layout: Option<String>,
    // Pre-docking settings files stored a preset and a pane list here.
    #[serde(default, skip_serializing)]
    layout: Option<Layout>,
    #[serde(default, skip_serializing)]
    panes: Option<Vec<serde_json::Value>>,
}

fn default_true() -> bool {
    true
}

const DEFAULT_WATCHLIST: [&str; 11] = [
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
];

/// Convert a pre-docking settings file into a layout document.
fn migrate_legacy(s: &Settings, defaults: &prefs::PanelDefaults) -> Option<LayoutDoc> {
    let panes: Vec<PaneSettings> = s
        .panes
        .as_ref()?
        .iter()
        .filter_map(|v| {
            // Unit variants were stored as bare strings ("Feed"); wrap them as {"Feed": null}.
            let v = match v {
                serde_json::Value::String(name) => serde_json::json!({ name.as_str(): null }),
                other => other.clone(),
            };
            serde_json::from_value(v).ok()
        })
        .collect();
    if panes.is_empty() {
        return None;
    }
    let _ = defaults;
    Some(LayoutDoc::preset("Default", s.layout.unwrap_or(Layout::Grid2x2), panes))
}

/// An in-progress tab drag.
#[derive(Debug, Clone, Copy)]
pub struct Drag {
    pub pane: PaneId,
}

pub struct TerminalApp {
    worker: Worker,
    provider: String,
    panes: Vec<Pane>,
    dock: Node,
    focus: PaneId,
    next_id: PaneId,
    last_opened: Option<PaneId>,
    /// Preset last applied, for the toolbar picker (cleared by manual rearranging).
    preset: Option<Layout>,
    drag: Option<Drag>,
    /// Tab being renamed inline: pane and edit buffer.
    renaming_tab: Option<(PaneId, String)>,

    store: Store,
    layout_name: String,
    layout_names: Vec<String>,
    last_saved: Vec<u8>,
    /// Layout being renamed in the sidebar: old name and edit buffer.
    renaming_layout: Option<(String, String)>,

    prefs: Prefs,
    applied_appearance: Option<prefs::Appearance>,
    rebinding: Option<prefs::Action>,
    settings_tab: SettingsTab,
    io_path: String,

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
    system_accent: Color32,
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

fn prefs_path() -> std::path::PathBuf {
    data_dir().join("prefs.json")
}

impl TerminalApp {
    pub fn new(ctx: &egui::Context, worker: Worker, provider: String) -> Self {
        let system_accent =
            tr_macos::accent_color().map_or(theme::DEFAULT_ACCENT, |[r, g, b]| Color32::from_rgb(r, g, b));
        let mut prefs = Prefs::load(&prefs_path());
        match std::env::var("TR_THEME").map(|v| v.to_ascii_lowercase()).as_deref() {
            Ok("light") => prefs.appearance.mode = prefs::ThemeMode::Light,
            Ok("dark") => prefs.appearance.mode = prefs::ThemeMode::Dark,
            _ => {}
        }
        theme::configure(ctx, &prefs.appearance, system_accent);
        let settings: Option<Settings> =
            std::fs::read(settings_path()).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let mut app = Self {
            worker,
            provider,
            panes: Vec::new(),
            dock: Node::default(),
            focus: 0,
            next_id: 1,
            last_opened: None,
            preset: None,
            drag: None,
            renaming_tab: None,
            store: Store::new(data_dir().join("layouts")),
            layout_name: "Default".into(),
            layout_names: Vec::new(),
            last_saved: Vec::new(),
            renaming_layout: None,
            applied_appearance: Some(prefs.appearance.clone()),
            prefs,
            rebinding: None,
            settings_tab: SettingsTab::Appearance,
            io_path: String::new(),
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
            politicians: Vec::new(),
            pol_search: String::new(),
            alerts: Vec::new(),
            congress_status: String::new(),
            watchlist: DEFAULT_WATCHLIST.map(String::from).to_vec(),
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
            system_accent,
            last_accent_poll: None,
            window_styled: false,
            vibrancy: false,
        };
        let mut doc = None;
        if let Some(s) = &settings {
            if !s.watchlist.is_empty() {
                // Older settings stored bare tickers; normalise to market keys.
                app.watchlist = s.watchlist.iter().map(|w| tr_data::security(w).key()).collect();
                app.watchlist.dedup();
            }
            app.sidebar_open = s.sidebar;
            if let Some(name) = &s.active_layout {
                match app.store.load(name) {
                    Ok(d) => doc = Some(d),
                    Err(e) => tracing::warn!("restoring layout {name:?}: {e:#}"),
                }
            }
            if doc.is_none() {
                doc = migrate_legacy(s, &app.prefs.panels);
            }
        }
        let doc = doc
            .or_else(|| app.store.list().first().and_then(|n| app.store.load(n).ok()))
            .unwrap_or_else(|| LayoutDoc::default_workspace("Default", &app.prefs.panels));
        app.apply_doc(doc);
        if !app.store.exists(&app.layout_name) {
            app.save_layout();
        }
        app.refresh_layout_names();
        app.worker.send(Request::Latest { limit: 2_000 });
        app.worker.send(Request::SearchPoliticians(String::new()));
        app.worker.send(Request::Alerts);
        app
    }

    fn save_settings(&self) {
        let s = Settings {
            watchlist: self.watchlist.clone(),
            sidebar: self.sidebar_open,
            active_layout: Some(self.layout_name.clone()),
            layout: None,
            panes: None,
        };
        if let Ok(json) = serde_json::to_vec_pretty(&s)
            && let Err(e) = crate::layouts::write_atomic(&settings_path(), &json)
        {
            tracing::warn!("saving settings: {e}");
        }
    }

    fn save_prefs(&mut self) {
        if let Err(e) = self.prefs.save(&prefs_path()) {
            self.status = format!("Could not save preferences: {e}");
        }
    }

    fn run_actions(&mut self, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::OpenChart(s) => self.open_security(&s),
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
                Action::LinkedSymbol(link, s) => self.broadcast(link, &s, None),
                Action::Track(q, on) => {
                    self.worker.send(Request::Track { query: q.clone(), on });
                    // Refresh any open profile so the star updates.
                    for p in &self.panes {
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
            Command::Symbol(s) => self.open_security(&s),
            Command::SymbolFunction(s, Function::Congress) => {
                self.open_panel(PaneKind::Ticker { symbol: s.clone(), trades: Vec::new(), loading: true });
                self.status = format!("{s}: congressional trades");
            }
            Command::SymbolFunction(s, Function::News) => {
                self.open_panel(PaneKind::News(Box::new(NewsPane::new(Some(s.clone())))));
                self.status = format!("{s}: news");
            }
            Command::SymbolFunction(s, Function::OrderBook(depth)) => {
                let depth = depth.unwrap_or(self.prefs.panels.book_depth).clamp(5, 200);
                self.open_panel(PaneKind::OrderBook(Box::new(BookPane::new(&s, depth, 0.0))));
                self.status = format!("{s}: order book");
            }
            Command::SymbolFunction(s, f @ (Function::NewPanel(_) | Function::Split { .. })) => {
                self.apply_with_symbol(f, Some(s));
            }
            Command::SymbolFunction(s, f) => {
                self.open_security(&s);
                self.apply(f);
            }
            Command::Function(f) => self.apply(f),
        }
    }

    fn apply(&mut self, f: Function) {
        self.apply_with_symbol(f, None);
    }

    fn apply_with_symbol(&mut self, f: Function, symbol: Option<String>) {
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
            Function::Congress => self.open_panel(PaneKind::Feed { filter: FeedFilter::default() }),
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
            Function::Layout(arg) => self.layout_command(&arg),
            Function::Layouts => {
                self.settings_tab = SettingsTab::Layouts;
                self.open_panel(PaneKind::Settings);
                self.status = format!("Layouts: {}", self.layout_names.join(" · "));
            }
            Function::NewPanel(spec) => {
                let kind = self.panel_from_spec(&spec, symbol.as_deref());
                match kind {
                    Some(kind) => {
                        let target = self.focused_visible();
                        let id = self.new_pane(kind);
                        self.dock_pane(id, target, crate::dock::Zone::Right);
                    }
                    None => self.status = format!("Unknown panel {spec:?}; try CHART, NEWS, BOOK, WEI, GC, CONG, WL"),
                }
            }
            Function::Split { down, panel } => match self.panel_from_spec(&panel, symbol.as_deref()) {
                Some(kind) => {
                    let target = self.focused_visible();
                    let id = self.new_pane(kind);
                    let zone = if down { crate::dock::Zone::Bottom } else { crate::dock::Zone::Right };
                    self.dock_pane(id, target, zone);
                }
                None => self.status = format!("Unknown panel {panel:?}"),
            },
            Function::ClosePane => self.close_pane(self.focus),
            Function::PopOut => self.pop_out(self.focus),
            Function::Link(link) => {
                let focus = self.focus;
                self.set_link(focus, link);
            }
            Function::News => self.open_panel(PaneKind::News(Box::new(NewsPane::new(self.focused_symbol())))),
            Function::OrderBook(depth) => {
                let sym = self.focused_symbol().unwrap_or_else(|| "BTCUSD Curncy".into());
                let depth = depth.unwrap_or(self.prefs.panels.book_depth).clamp(5, 200);
                self.open_panel(PaneKind::OrderBook(Box::new(BookPane::new(&sym, depth, 0.0))));
            }
            Function::Settings => self.open_panel(PaneKind::Settings),
            Function::Theme(t) => {
                let a = &mut self.prefs.appearance;
                for w in t.split_whitespace() {
                    match w {
                        "LIGHT" => a.mode = prefs::ThemeMode::Light,
                        "DARK" => a.mode = prefs::ThemeMode::Dark,
                        "SYSTEM" | "AUTO" => a.mode = prefs::ThemeMode::System,
                        "COMPACT" => a.density = prefs::Density::Compact,
                        "REGULAR" => a.density = prefs::Density::Regular,
                        "COMFORTABLE" => a.density = prefs::Density::Comfortable,
                        "MONO" => a.ui_font = prefs::UiFont::Monospace,
                        "SF" | "SANS" => a.ui_font = prefs::UiFont::System,
                        other => self.status = format!("THEME: unknown option {other}"),
                    }
                }
                self.save_prefs();
            }
            Function::Title(t) => {
                let focus = self.focus;
                if let Some(p) = self.pane_mut(focus) {
                    p.title = (!t.trim().is_empty()).then(|| t.trim().to_string());
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
        match self.panes.iter_mut().find(|p| p.id == pane).map(|p| &mut p.kind) {
            Some(PaneKind::Chart(c)) => Some(c),
            _ => None,
        }
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
                Response::News { pane, symbol, items } => {
                    if let Some(p) = self.panes.iter_mut().find(|p| p.id == pane)
                        && let PaneKind::News(n) = &mut p.kind
                        && n.symbol == symbol
                    {
                        n.loading = false;
                        match items {
                            Ok(items) => {
                                n.items = items;
                                n.error = None;
                            }
                            Err(e) => n.error = Some(e),
                        }
                    }
                }
                Response::OrderBook { pane, symbol, book } => {
                    if let Some(p) = self.panes.iter_mut().find(|p| p.id == pane)
                        && let PaneKind::OrderBook(b) = &mut p.kind
                        && b.symbol == symbol
                    {
                        match book {
                            Ok(book) => {
                                b.book = Some(book);
                                b.error = None;
                            }
                            Err(e) => b.error = Some(e),
                        }
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
        let presses: Vec<(Key, Modifiers)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    Event::Key { key, pressed: true, repeat: false, modifiers, .. } if !prefs::is_modifier(*key) => {
                        Some((*key, *modifiers))
                    }
                    _ => None,
                })
                .collect()
        });
        // Settings is capturing a new binding: the next key press is it.
        if let Some(action) = self.rebinding {
            if let Some(&(key, m)) = presses.first() {
                ctx.input_mut(|i| i.consume_key(m, key));
                self.rebinding = None;
                if key != Key::Escape {
                    let chord = prefs::Chord::from_event(key, m);
                    let displaced = self.prefs.hotkeys.bind(action, chord);
                    self.status = match displaced {
                        Some(d) => {
                            format!("{} bound to {} (removed from {})", action.label(), chord.display(), d.label())
                        }
                        None => format!("{} bound to {}", action.label(), chord.display()),
                    };
                    self.save_prefs();
                }
            }
            return;
        }
        let cmd_id = egui::Id::new("command-line");
        let focused = ctx.memory(|m| m.focused());
        let cmd_focused = focused == Some(cmd_id);
        let other_text_focused = focused.is_some() && !cmd_focused;
        if other_text_focused {
            return;
        }
        // Chorded bindings (⌘, Ctrl, Alt) work even while the command line has text.
        for &(key, m) in &presses {
            let Some(action) = self.prefs.hotkeys.action_for(key, m) else { continue };
            let chorded = m.command || m.ctrl || m.alt;
            if !chorded && !self.cmd.is_empty() {
                continue;
            }
            if !self.hotkey_applies(action) {
                continue;
            }
            ctx.input_mut(|i| i.consume_key(m, key));
            self.run_hotkey(action);
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
        if !self.cmd.is_empty() || !self.focused_is_chart() {
            return;
        }
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
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

    fn hotkey_applies(&self, a: prefs::Action) -> bool {
        use prefs::Action as A;
        match a {
            A::DrawTrendline
            | A::DrawHorizontal
            | A::DrawVertical
            | A::DrawFib
            | A::ResetView
            | A::LogScale
            | A::Magnet => self.focused_is_chart(),
            a => a.layout_slot().is_none_or(|i| i < self.layout_names.len()),
        }
    }

    fn run_hotkey(&mut self, a: prefs::Action) {
        use prefs::Action as A;
        match a {
            A::Help => self.apply(Function::Help),
            A::Congress => self.apply(Function::Congress),
            A::Politicians => self.apply(Function::Politician(String::new())),
            A::Alerts => self.apply(Function::Alerts),
            A::Watchlist => self.apply(Function::Watchlist),
            A::Refresh => self.apply(Function::Refresh),
            A::FrameTiming => self.show_perf = !self.show_perf,
            A::NextPane => self.cycle_focus(1),
            A::PrevPane => self.cycle_focus(-1),
            A::ToggleSidebar => self.sidebar_open = !self.sidebar_open,
            A::SplitRight => self.apply(Function::Split { down: false, panel: String::new() }),
            A::SplitDown => self.apply(Function::Split { down: true, panel: String::new() }),
            A::NewTab => {
                let target = self.focused_visible();
                let kind = self.panel_from_spec("", None).expect("chart");
                let id = self.new_pane(kind);
                self.dock_pane(id, target, crate::dock::Zone::Center);
            }
            A::ClosePane => self.close_pane(self.focus),
            A::PopOut => self.pop_out(self.focus),
            A::SaveLayout => {
                self.save_layout();
                self.status = format!("Saved layout {:?}", self.layout_name);
            }
            A::Settings => self.apply(Function::Settings),
            A::DrawTrendline => self.with_chart(|c, _| c.start_tool(Tool::Trendline)),
            A::DrawHorizontal => self.with_chart(|c, _| c.start_tool(Tool::Horizontal)),
            A::DrawVertical => self.with_chart(|c, _| c.start_tool(Tool::Vertical)),
            A::DrawFib => self.with_chart(|c, _| c.start_tool(Tool::Fib)),
            A::ResetView => self.with_chart(|c, _| c.reset_view()),
            A::LogScale => self.with_chart(|c, _| c.log = !c.log),
            A::Magnet => self.with_chart(|c, _| c.magnet = !c.magnet),
            a => {
                if let Some(name) = a.layout_slot().and_then(|i| self.layout_names.get(i)).cloned() {
                    self.switch_layout(&name);
                }
            }
        }
    }

    fn periodic(&mut self) {
        let visible = self.visible_panes();
        let shown = |f: &dyn Fn(&PaneKind) -> bool| visible.iter().any(|&i| f(&self.panes[i].kind));
        let visible_wl = self.sidebar_open || shown(&|k| matches!(k, PaneKind::Watchlist));
        if visible_wl && self.last_quotes.is_none_or(|t| t.elapsed() > Duration::from_secs(20)) {
            self.worker.send(Request::Quotes(self.watchlist.clone()));
            self.last_quotes = Some(Instant::now());
        }
        if shown(&|k| matches!(k, PaneKind::Wei)) && self.last_wei.is_none_or(|t| t.elapsed() > Duration::from_secs(20))
        {
            self.worker.send(Request::Quotes(markets_view::overview_keys()));
            self.last_wei = Some(Instant::now());
        }
        if shown(&|k| matches!(k, PaneKind::Curve))
            && self.last_curve.is_none_or(|t| t.elapsed() > Duration::from_secs(600))
        {
            self.worker.send(Request::YieldCurves);
            self.last_curve = Some(Instant::now());
        }
        // News every 5 minutes and order books every second, while visible.
        for &i in &visible {
            let p = &mut self.panes[i];
            match &mut p.kind {
                PaneKind::News(n) if n.fetched.is_none_or(|t| t.elapsed() > Duration::from_secs(300)) => {
                    n.fetched = Some(Instant::now());
                    self.worker.send(Request::News { pane: p.id, symbol: n.symbol.clone() });
                }
                PaneKind::OrderBook(b) if b.fetched.is_none_or(|t| t.elapsed() > Duration::from_millis(1000)) => {
                    b.fetched = Some(Instant::now());
                    self.worker.send(Request::OrderBook { pane: p.id, symbol: b.symbol.clone(), depth: b.depth });
                }
                _ => {}
            }
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
        if self.last_save.elapsed() > Duration::from_secs(10) {
            self.autosave();
            self.last_save = Instant::now();
        }
        self.toasts.retain(|(_, t)| t.elapsed() < Duration::from_secs(10));
    }

    /// Save the active layout (only when it changed) and app settings.
    fn autosave(&mut self) {
        let json = self.current_doc().to_json();
        if json != self.last_saved {
            self.save_layout();
        }
        self.save_settings();
    }
}

/// Apply the native macOS window chrome (unified toolbar, inset traffic lights, sidebar
/// vibrancy). Returns whether the window is now translucent.
#[cfg(target_os = "macos")]
fn style_native_window(frame: &eframe::Frame, vibrancy: bool) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let vibrancy = vibrancy && std::env::var("TR_VIBRANCY").map_or(true, |v| v != "0");
    let Ok(handle) = frame.window_handle() else { return false };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else { return false };
    // SAFETY: called on the main thread (eframe runs `ui` there) with a live NSView.
    let ok = unsafe { tr_macos::style_window(h.ns_view.as_ptr(), vibrancy) };
    tracing::info!("macOS window chrome applied: {ok} (vibrancy {vibrancy})");
    ok && vibrancy
}

#[cfg(not(target_os = "macos"))]
fn style_native_window(_frame: &eframe::Frame, _vibrancy: bool) -> bool {
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
        if !self.window_styled {
            self.window_styled = true;
            self.vibrancy = style_native_window(frame, self.prefs.appearance.vibrancy);
        }
        if self.last_accent_poll.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
            if let Some([r, g, b]) = tr_macos::accent_color() {
                self.system_accent = Color32::from_rgb(r, g, b);
            }
            self.last_accent_poll = Some(Instant::now());
        }
        if self.applied_appearance.as_ref() != Some(&self.prefs.appearance) {
            theme::configure(&ctx, &self.prefs.appearance, self.system_accent);
            self.applied_appearance = Some(self.prefs.appearance.clone());
        }
        theme::refresh(&ctx, &self.prefs.appearance, self.system_accent);
        self.drain();
        self.periodic();

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
        let gap = theme::dp(6.0);
        let area = Rect::from_min_max(
            pos2(main.left() + gap, toolbar.bottom() + gap),
            pos2(main.right() - gap, status.top() - gap * 0.5),
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

        let mut actions = Vec::new();
        for c in sidebar_cmds {
            match c {
                SidebarCmd::Action(a) => actions.push(a),
                SidebarCmd::Function(f) => self.apply(f),
                SidebarCmd::Layout(op) => self.layout_op(op),
            }
        }
        actions.extend(self.render_dock(ui, area, gap));
        actions.extend(self.render_windows(&ctx));
        self.run_actions(actions);
        self.toasts(&ctx);

        // TR_SMOKE_SECS=N: render for N seconds, log frame stats and quit (CI smoke test).
        if let Some(secs) = std::env::var("TR_SMOKE_SECS").ok().and_then(|v| v.parse::<f64>().ok()) {
            ctx.request_repaint();
            if !self.smoke_done && self.started.elapsed().as_secs_f64() > secs {
                self.smoke_done = true;
                tracing::info!(
                    "smoke: {} frames in {:.1}s, ui cpu {:.2} ms/frame, panes {}, layout {:?}",
                    ctx.cumulative_frame_nr(),
                    self.started.elapsed().as_secs_f64(),
                    self.perf_cpu_ms,
                    self.panes.len(),
                    self.layout_name
                );
                self.autosave();
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
        self.autosave();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_migrate_to_a_layout() {
        let old = r#"{"layout":"MainRight2","watchlist":["AAPL","BTCUSD Curncy"],"sidebar":false,
            "panes":[{"Chart":{"symbol":"NVDA","tf":"D1","chart_type":"Candles","log":false,"indicators":[],
            "drawings":[],"compare":null}},"Feed","Wei"]}"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert!(!s.sidebar);
        let doc = migrate_legacy(&s, &prefs::PanelDefaults::default()).unwrap();
        assert_eq!(doc.panes.len(), 3);
        assert_eq!(doc.dock.visible(), vec![1, 2, 3]);
        assert!(matches!(&doc.panes[0].settings, PaneSettings::Chart { symbol, .. } if symbol == "NVDA"));
        assert_eq!(doc.panes[1].settings, PaneSettings::Feed(None));
        // New-format files carry no legacy fields and are not migrated.
        let new: Settings = serde_json::from_str(r#"{"watchlist":[],"active_layout":"Desk"}"#).unwrap();
        assert!(migrate_legacy(&new, &prefs::PanelDefaults::default()).is_none());
        assert_eq!(new.active_layout.as_deref(), Some("Desk"));
    }

    #[test]
    fn pane_settings_round_trip_through_kinds() {
        let d = prefs::PanelDefaults::default();
        for s in [
            crate::layouts::chart_settings("BTCUSD Curncy", &d),
            PaneSettings::Feed(Some(crate::layouts::FeedSettings { buys: Some(true), ..Default::default() })),
            PaneSettings::News(Some("AAPL US Equity".into())),
            PaneSettings::News(None),
            PaneSettings::OrderBook { symbol: "ETHUSD Curncy".into(), depth: 50, group: 1.0 },
            PaneSettings::Search("gold".into()),
            PaneSettings::Settings,
            PaneSettings::Wei,
        ] {
            assert_eq!(PaneKind::from_settings(s.clone()).settings(), s);
        }
    }
}
