//! Turns chart state (data, viewport, chart type, overlays) into GPU draw commands.
//! Shared by the egui widget and the headless frame-time benchmark.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tr_core::Bar;

use crate::gpu::{DrawCmd, LineKind, LineRef, PaneTransform, SeriesRef, Style, Uniforms};
use crate::lod::{BarPyramid, LinePyramid, level_for};
use crate::viewport::{PriceScale, Viewport};

static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

/// Process-unique key for GPU buffer caching.
pub fn next_key() -> u64 {
    NEXT_KEY.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ChartType {
    #[default]
    Candles,
    Bars,
    Line,
    Area,
}

impl ChartType {
    pub const ALL: [ChartType; 4] = [ChartType::Candles, ChartType::Bars, ChartType::Line, ChartType::Area];

    pub fn label(self) -> &'static str {
        match self {
            ChartType::Candles => "Candles",
            ChartType::Bars => "Bars",
            ChartType::Line => "Line",
            ChartType::Area => "Area",
        }
    }
}

/// GPU-ready series: bars plus their LOD pyramid and a close-price line pyramid.
#[derive(Clone)]
pub struct SeriesGpu {
    pub bars: SeriesRef,
    pub closes: LineRef,
}

impl SeriesGpu {
    pub fn build(bars: &[Bar]) -> Self {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        Self {
            bars: SeriesRef { key: next_key(), version: 1, pyramid: Arc::new(BarPyramid::build(bars)) },
            closes: line_ref(&closes),
        }
    }

    pub fn len(&self) -> usize {
        self.bars.pyramid.len0
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn line_ref(values: &[f64]) -> LineRef {
    LineRef { key: next_key(), version: 1, pyramid: Arc::new(LinePyramid::build(values)) }
}

/// A line overlay with its colour.
#[derive(Clone)]
pub struct Overlay {
    pub line: LineRef,
    pub color: [f32; 4],
    pub kind: LineKind,
}

/// Everything needed to draw the main price pane.
pub struct PricePane<'a> {
    pub series: &'a SeriesGpu,
    pub chart_type: ChartType,
    pub show_volume: bool,
    pub overlays: &'a [Overlay],
    pub viewport: &'a Viewport,
    pub height_px: f64,
    pub log: bool,
    /// `None` = autoscale to the visible bars.
    pub manual_scale: Option<PriceScale>,
    pub style: Style,
}

pub struct PaneOutput {
    pub cmds: Vec<DrawCmd>,
    pub scale: PriceScale,
    pub level: usize,
    pub vol_max: f64,
}

fn transform(v: &Viewport, height_px: f64, scale: &PriceScale) -> PaneTransform {
    let (y_lo, y_hi) = scale.bounds_t();
    PaneTransform {
        width_px: v.width_px as f32,
        height_px: height_px as f32,
        left: v.left(),
        visible_bars: v.visible_bars(),
        y_lo,
        y_hi,
        log: scale.log,
    }
}

/// Autoscale range for the visible part of the series and overlays.
pub fn visible_extent(series: &SeriesGpu, overlays: &[Overlay], v: &Viewport, chart_type: ChartType) -> Option<(f64, f64, f64)> {
    let p = &series.bars.pyramid;
    let (lo, hi) = v.visible_range(p.len0);
    let k = level_for(v.bar_px, p.levels());
    let (mut l, mut h, vmax) = match chart_type {
        ChartType::Line | ChartType::Area => {
            let (l, h) = series.closes.pyramid.extent(k, lo, hi)?;
            (l, h, p.extent(k, lo, hi).map(|e| e.2).unwrap_or(0.0))
        }
        _ => p.extent(k, lo, hi)?,
    };
    for o in overlays.iter().filter(|o| o.kind == LineKind::Line) {
        let kk = level_for(v.bar_px, o.line.pyramid.levels());
        if let Some((a, b)) = o.line.pyramid.extent(kk, lo, hi) {
            l = l.min(a);
            h = h.max(b);
        }
    }
    Some((l, h, vmax))
}

pub fn price_pane(p: &PricePane<'_>) -> PaneOutput {
    let v = p.viewport;
    let pyr = &p.series.bars.pyramid;
    let (lo, hi) = v.visible_range(pyr.len0);
    let k = level_for(v.bar_px, pyr.levels());
    let ext = visible_extent(p.series, p.overlays, v, p.chart_type);
    let vol_max = ext.map(|e| e.2).unwrap_or(1.0);
    let scale = p.manual_scale.unwrap_or_else(|| {
        let (l, h) = ext.map(|e| (e.0, e.1)).unwrap_or((0.0, 1.0));
        // Leave room at the bottom for the volume overlay.
        let mut s = PriceScale::padded(l, h, 0.08, p.log, p.height_px);
        if p.show_volume {
            let (a, b) = s.bounds_t();
            let extra = (b - a) * 0.12;
            let lo_t = a - extra;
            s.lo = if s.log { lo_t.exp() } else { lo_t };
        }
        s
    });
    let t = transform(v, p.height_px, &scale);
    let mut style = p.style;
    style.ohlc_bars = p.chart_type == ChartType::Bars;
    style.vol_max = vol_max as f32;
    let mut cmds = Vec::new();
    if p.show_volume {
        let mut vs = style;
        vs.color[3] = 0.35;
        cmds.push(DrawCmd::Volume {
            data: p.series.bars.clone(),
            range: pyr.instance_range(k, lo, hi),
            uniforms: Uniforms::new(&t, &vs),
        });
    }
    match p.chart_type {
        ChartType::Candles | ChartType::Bars => cmds.push(DrawCmd::Candles {
            data: p.series.bars.clone(),
            range: pyr.instance_range(k, lo, hi),
            uniforms: Uniforms::new(&t, &style),
        }),
        ChartType::Line | ChartType::Area => {
            let lp = &p.series.closes.pyramid;
            let kk = level_for(v.bar_px, lp.levels());
            let range = lp.segment_range(kk, lo, hi);
            if p.chart_type == ChartType::Area {
                let mut a = style;
                a.color[3] = 0.25;
                cmds.push(DrawCmd::Line {
                    kind: LineKind::Area,
                    data: p.series.closes.clone(),
                    range,
                    uniforms: Uniforms::new(&t, &a),
                });
            }
            let mut ls = style;
            ls.thickness_px = 2.0;
            cmds.push(DrawCmd::Line { kind: LineKind::Line, data: p.series.closes.clone(), range, uniforms: Uniforms::new(&t, &ls) });
        }
    }
    cmds.extend(overlay_cmds(p.overlays, v, &t, &style));
    PaneOutput { cmds, scale, level: k, vol_max }
}

fn overlay_cmds(overlays: &[Overlay], v: &Viewport, t: &PaneTransform, base: &Style) -> Vec<DrawCmd> {
    let len = overlays.first().map(|o| o.line.pyramid.level_len(0)).unwrap_or(0);
    let (lo, hi) = v.visible_range(len.max(v.right.ceil().max(0.0) as usize));
    overlays
        .iter()
        .map(|o| {
            let lp = &o.line.pyramid;
            let k = level_for(v.bar_px, lp.levels());
            let mut s = *base;
            s.color = o.color;
            let range = match o.kind {
                LineKind::Hist => {
                    s.hist_width_bars = (base.body_frac as f64 * (1u64 << k) as f64) as f32;
                    let n = lp.level_len(k);
                    let off = lp.offsets[k];
                    let a = (lo >> k).min(n);
                    let b = hi.div_ceil(1 << k).min(n);
                    ((off + a) as u32, (off + b) as u32)
                }
                _ => lp.segment_range(k, lo, hi),
            };
            DrawCmd::Line { kind: o.kind, data: o.line.clone(), range, uniforms: Uniforms::new(t, &s) }
        })
        .collect()
}

/// An indicator sub-pane (RSI, MACD): lines and histograms with their own scale.
pub fn sub_pane(overlays: &[Overlay], v: &Viewport, height_px: f64, fixed: Option<(f64, f64)>, style: Style) -> PaneOutput {
    let len = overlays.first().map(|o| o.line.pyramid.level_len(0)).unwrap_or(0);
    let (lo, hi) = v.visible_range(len);
    let (l, h) = fixed.unwrap_or_else(|| {
        let mut ext: Option<(f64, f64)> = None;
        for o in overlays {
            let k = level_for(v.bar_px, o.line.pyramid.levels());
            if let Some((a, b)) = o.line.pyramid.extent(k, lo, hi) {
                ext = Some(ext.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
            }
        }
        let (a, b) = ext.unwrap_or((-1.0, 1.0));
        // Histograms grow from zero, so keep zero in range.
        if overlays.iter().any(|o| o.kind == LineKind::Hist) { (a.min(0.0), b.max(0.0)) } else { (a, b) }
    });
    let scale = if fixed.is_some() { PriceScale::new(l, h, false, height_px) } else { PriceScale::padded(l, h, 0.1, false, height_px) };
    let t = transform(v, height_px, &scale);
    let cmds = overlay_cmds(overlays, v, &t, &style);
    PaneOutput { cmds, scale, level: 0, vol_max: 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tr_core::{Timeframe, sample};

    #[test]
    fn price_pane_bounds_instances_for_huge_series() {
        let bars = sample::generate("X", Timeframe::M1, 1_000_000, 1_790_000_000);
        let s = SeriesGpu::build(&bars);
        let v = Viewport { right: 1_000_000.0, bar_px: 0.0016, width_px: 1600.0 };
        let out = price_pane(&PricePane {
            series: &s,
            chart_type: ChartType::Candles,
            show_volume: true,
            overlays: &[],
            viewport: &v,
            height_px: 800.0,
            log: false,
            manual_scale: None,
            style: Style::default(),
        });
        assert_eq!(out.cmds.len(), 2);
        for c in &out.cmds {
            if let DrawCmd::Candles { range, .. } | DrawCmd::Volume { range, .. } = c {
                assert!(range.1 - range.0 <= 810, "{range:?}");
                assert!(range.1 > range.0);
            }
        }
        // Autoscale contains every visible bar.
        let lo = bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
        let hi = bars.iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
        assert!(out.scale.lo <= lo && out.scale.hi >= hi);
    }

    #[test]
    fn sub_pane_fixed_and_auto() {
        let vals: Vec<f64> = (0..100).map(|i| (i as f64 * 0.2).sin()).collect();
        let o = Overlay { line: line_ref(&vals), color: [1.0; 4], kind: LineKind::Hist };
        let v = Viewport::fit_last(100, 100, 500.0);
        let out = sub_pane(std::slice::from_ref(&o), &v, 100.0, None, Style::default());
        assert!(out.scale.lo < -0.9 && out.scale.hi > 0.9);
        let fixed = sub_pane(&[o], &v, 100.0, Some((0.0, 100.0)), Style::default());
        assert_eq!((fixed.scale.lo, fixed.scale.hi), (0.0, 100.0));
    }
}
