# Indicators

## Observed (`ev_c8bc591b`)

- The **Indicators** toolbar button opens a modal "Indicators, metrics, and strategies" with a search box
  focused immediately. Left column: Personal (My scripts, Purchased), Built-in (Technicals, Fundamentals),
  Community (Editors' picks, Top, Trending, Marketplace).
- **Incremental prefix search**: typing `R` filters the list instantly and highlights the matched prefix
  in each result ("**R**elative strength index", "**R**SI divergence indicator", …).
- On load the free chart attaches studies server-side: the WebSocket shows six `create_study` calls and
  `du` updates carrying study points (`ev_5888cfc1`), e.g. the volume study and dividends/earnings markers
  (the `E`/`D` badges on the time axis).

## Documented defaults

| Indicator | Default params | Placement |
|---|---|---|
| SMA / EMA | length 9 (we use 20/50) on close | overlay on price |
| VWAP | session anchor, hlc3 source | overlay |
| Bollinger Bands | length 20, mult 2.0, SMA basis | overlay |
| RSI | length 14, Wilder smoothing, 70/30 bands | separate pane below |
| MACD | 12, 26, 9 (EMA), histogram + 2 lines | separate pane below |

- Oscillators stack as panes beneath the price pane; panes share the time axis and crosshair; each pane has
  its own price scale and legend with a settings/hide/remove control.

## terminal-rs implications

- Command-driven indicator entry: the indicator palette (`/` or the `IND` command) with the same
  prefix-filter-and-highlight behaviour; Enter adds.
- Overlays (SMA, EMA, VWAP, BB) draw on the price pane; RSI and MACD each add a sub-pane with its own scale;
  sub-pane heights are a fraction of the chart rect and share the x-transform with the price pane.
- Indicators computed locally in `tr-core::indicators` (all O(n), ~ms for 1M bars) and cached per
  (series version, params); recomputed off the UI thread when data changes.
