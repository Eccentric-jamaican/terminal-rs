# terminal-rs

A fast, keyboard-driven market terminal in pure Rust: TradingView-class GPU charting with a
Bloomberg Terminal look (black background, amber monospace, dense tiled panes, a command line
like `AAPL <GO>`), plus **congressional trade tracking ingested directly from the official STOCK
Act disclosure systems**.

![terminal-rs: chart with fib/trendline drawings and congress markers, alerts, per-ticker congress list, watchlist, live feed and a politician profile](docs/img/terminal.png)

> **Built from reverse-engineering research.** Chart UX was studied behaviourally on
> tradingview.com with [REA](https://github.com/morluto/rea) in website mode (passive captures of
> the public, logged-out chart page). Findings, evidence IDs and their design implications are in
> [`docs/analysis/`](docs/analysis/README.md). Only behaviour was studied: no TradingView code,
> scripts, icons, fonts or other assets are used or included here. The disclosure parsers were
> likewise built by examining the public Senate eFD and House Clerk sites and their filings.

## Features

- **Command line** — `<SECURITY> [<FUNCTION>] <GO>` (Enter is `<GO>`); typing anywhere starts a
  command, with a live interpretation underneath (`5` → "Interval 5 minutes"). History with ↑/↓.
- **Tiling layouts** — `LAYOUT 1|2|2H|3|4|6`, Tab to cycle focus; any pane can be a chart, the
  congress feed, a per-ticker list, a politician profile, the politician directory, alerts, a
  watchlist or help. Layout, watchlist, charts, indicators and drawings persist between runs.
- **Charts** — candles, OHLC bars, line and area; 1m, 5m, 15m, 30m, 1H, 4H, 1D, 1W, 1M plus quick
  ranges (1D … ALL); volume overlay; SMA, EMA, VWAP, Bollinger overlays; RSI and MACD sub-panes;
  `CMP <SYM>` compare rebased to the left edge of the view; log scale; crosshair with OHLC legend.
- **Drawings** — trendline (Alt+T), horizontal (Alt+H), vertical (Alt+V) and fib retracement
  (Alt+F), stored in (time, price) so they survive zoom and timeframe changes; magnet (Alt+M) snaps
  to OHLC; click to select, Del to remove.
- **Market data behind a trait** (`tr_data::MarketData`) — Alpaca when `APCA_API_KEY_ID` and
  `APCA_API_SECRET_KEY` are set (free IEX feed; `APCA_DATA_FEED` to override), otherwise Yahoo
  Finance history (no key); `TR_PROVIDER=sample` runs fully offline on deterministic sample data,
  and live providers fall back to sample data when the network is unavailable. Bars are cached in
  SQLite so charts paint from disk on start. Older history pages in as you scroll left.
- **Congress trades** (key feature)
  - Ingests Periodic Transaction Reports from the **Senate eFD** (efdsearch.senate.gov: agreement
    + CSRF session, DataTables search API, HTML transaction tables) and the **House Clerk**
    (disclosures-clerk.house.gov: yearly `{year}FD.zip` XML index, PTR PDFs parsed with pure-Rust
    text extraction). Scanned paper filings are detected and recorded as such.
  - Normalises politician, party, chamber, state, ticker, buy/sell, amount range, transaction date
    and filed date into SQLite. Party comes from the official rosters (clerk.house.gov
    `MemberData.xml`, senate.gov `senators_cfm.xml`); politicians are keyed by bioguide ID, which
    also repairs typos in the source index.
  - Refreshed on a schedule (hourly by default, `TR_CONGRESS_REFRESH_MIN`, `0` disables;
    `REFRESH`/F6 forces one), incrementally — only unseen filings are fetched.
  - Panels: latest trades feed with party/chamber/side filters (`CONG`); per-ticker "who in
    Congress traded it" (`NVDA CONG`, or right-click any ticker); politician profile with trade
    history and **estimated performance** (amount-weighted directional return, hit rate, average
    disclosure lag) (`POL PELOSI`); directory (`POL`).
  - **Chart overlay**: B/S markers at transaction dates coloured by party (blue D, red R, purple
    I); hover for a card with politician, amount, owner, traded/filed dates and the disclosure lag.
  - **Alerts**: `TRACK <name>` (or ☆ in the directory/profile); when a tracked politician files a
    new PTR you get a toast, an `ALERTS n` badge and an entry in `ALERTS`.

![Congress marker hover card with disclosure lag](docs/img/hover-card.png)

## Running

```sh
cargo run --release                  # Yahoo data, congress ingest in the background
TR_PROVIDER=sample cargo run --release   # offline
APCA_API_KEY_ID=… APCA_API_SECRET_KEY=… cargo run --release   # Alpaca
```

Data lives in `~/Library/Application Support/terminal-rs` (macOS) or
`$XDG_DATA_HOME/terminal-rs` (Linux), or `TR_DATA_DIR`. The first congress ingest covers the
current and previous year (a few minutes; it runs in the background while you work).
`cargo run -p tr-congress --example ingest -- congress.sqlite3` runs one ingest from the CLI.

Press **F1** (or type `HELP`) for the full command and hotkey list. Highlights:

| Input | Action |
|---|---|
| `AAPL` · `AAPL US EQUITY <GO>` | load a security in the focused chart |
| `5` `1H` `D` `1W` `1M` · `YTD` `5Y` `ALL` | interval · quick range |
| `RSI` `MACD` `BB` `SMA 50` `EMA 20` `VWAP` · `CLEAR` | toggle indicators |
| `CMP MSFT` · `LOG` · `CANDLE` `BAR` `LINE` `AREA` | compare · log scale · chart type |
| `CONG` · `NVDA CONG` · `POL PELOSI` · `TRACK PELOSI` · `ALERTS` | congress functions |
| wheel / drag / drag axes / double-click | zoom / pan / stretch / reset |
| Alt+T/H/V/F · Alt+R · Alt+L · Esc · Del | draw · reset view · log · cancel · delete drawing |
| F1–F6 · Tab · F12 | help, CONG, POL, ALRT, WL, SYNC · next pane · frame-time readout |

## Architecture

```
app/            eframe shell: command line, tiling panes, chart widget, congress panels
  worker.rs     tokio runtime: all network, SQLite, pyramid and indicator work (UI never blocks)
crates/core     Bar/Series, timeframes and resampling, indicators, deterministic sample data
crates/data     MarketData trait; Alpaca, Yahoo, sample providers; SQLite bar cache
crates/chart    wgpu renderer, LOD pyramids, viewport math, axes, drawings, egui paint callback
crates/congress Senate eFD + House Clerk parsers, rosters, SQLite store, refresh, alerts, performance
docs/analysis   REA research notes on TradingView chart UX
```

**Rendering.** Bars are uploaded once per data version into a storage buffer as a level-of-detail
pyramid (level *k* merges 2^k bars), and geometry is generated in the vertex shader (one instance
per bar or line segment). Each frame picks the level that keeps instances at ≥ 2 px, so the work per
frame is bounded by the plot width, not the series length: a 1M-bar chart fully zoomed out draws
about 800 instances. Pan and zoom only rewrite a few hundred bytes of uniforms; bar indices are split
into integer and fraction for f32 precision at millions of bars. Axes, legend, crosshair, drawings and
markers are cheap egui shapes.

**Threading.** The UI thread sends requests to the worker and drains responses each frame; history
fetches, pyramid building (≈ 200 ms for 1M bars), indicators, SQLite queries and the congress ingest
all run on tokio tasks.

## Performance

Measured on Linux with Mesa **llvmpipe (a CPU software rasterizer)** — a real GPU is much faster:

- **Cold start**: window and GPU ready in **143 ms**; charts paint from the SQLite cache as soon as
  the worker answers.
- **1M-bar pan/zoom** (`cargo run --release -p tr-chart --example framebench`, 1600×900, candles +
  volume + 2 overlays, 300 frames per phase):

  | phase | CPU per frame p50 / p99 | until GPU done p50 / p99 |
  |---|---|---|
  | zoom out to all 1M bars | 0.07 / 0.20 ms | 3.09 / 5.27 ms |
  | pan across history | 0.06 / 0.18 ms | 1.40 / 3.93 ms |
  | zoom in to 8 px/bar | 0.05 / 0.19 ms | 1.34 / 2.63 ms |

  All within the 8.33 ms budget for 120 fps even on the software rasterizer. Criterion versions:
  `cargo bench -p tr-chart --bench frame`.
- **In the app** (F12 readout, interactive wheel-zoom + drag): UI CPU ≈ 0.2 ms per frame, 118 fps.
  To try a million bars interactively:
  `TR_PROVIDER=sample TR_SAMPLE_BARS=1000000 TR_HISTORY_LIMIT=1000000 TR_FPS=1 cargo run --release`, then `1 <GO>`.
  With all 1M one-minute bars visible on llvmpipe under Xvfb the app measured UI CPU 0.36 ms and
  76 fps for the whole window (software rasterization and presentation bound; the chart pass itself
  is the benchmark above).
- **Indicators over 1M bars** (`cargo bench -p tr-core`): SMA 1.8 ms, EMA 2.2 ms, VWAP 7.0 ms,
  Bollinger 29 ms; RSI and MACD are single-pass (≈ 12 ms and 26 ms including first-touch of their
  output memory).

## Tests

```sh
cargo test --workspace                       # unit + fixture tests (parsers, indicators, chart, app)
cargo test -p tr-data -- --ignored           # live Yahoo history/paging test
cargo bench -p tr-core && cargo bench -p tr-chart
```

- Parsers are tested against real filings captured from the official sources
  (`crates/congress/tests/fixtures`): multi-page House PDFs with partial sales and page-break rows,
  spouse/joint owners, wrapped and open-ended amounts, CUSIP vs ticker, exchanges, private holdings,
  a scanned paper filing, Senate search JSON (electronic, paper and amended reports) and PTR tables.
  An end-to-end refresh test covers discovery, roster joins, idempotency and alerts.
- Indicators are checked against reference values (Wilder's RSI example, hand-computed EMA, MACD,
  population-SD Bollinger, session VWAP).
- The chart renders real frames offscreen on whatever adapter is present and checks pixels.

## Platform notes and limitations

- macOS is the primary target (wgpu Metal backend); Linux works with Vulkan or GL. This repository
  was developed and run on Linux (X11, software Vulkan). For macOS, the GPU and UI stack (`tr-core`,
  `tr-chart`, eframe/winit) was cross-checked for `aarch64-apple-darwin`; the full app could not be
  built here because the bundled SQLite C library needs the Apple SDK, so it has not yet been run on
  a Mac.
- Scanned paper filings (some House PTRs and Senate `/paper/` reports) are recorded and alerted but
  not OCR'd, so their trades are not itemised.
- Estimated performance uses disclosed value bands (midpoints), so it is an estimate of trade-level
  skill, not a reconstruction of anyone's portfolio.
- Yahoo's chart endpoint is unofficial and rate-limited; use Alpaca for a supported free source.
  Senate eFD terms prohibit using the data for commercial purposes or credit decisions; the terminal
  accepts the eFD agreement on your behalf for personal research use.
