//! Renders real frames offscreen (any adapter, including software Vulkan) and checks pixels.

use tr_chart::gpu::{Frame, LineKind, Style};
use tr_chart::headless::Headless;
use tr_chart::scene::{ChartType, Overlay, PricePane, SeriesGpu, line_ref, price_pane, sub_pane};
use tr_chart::viewport::Viewport;
use tr_core::{Timeframe, indicators, sample};

fn dump_ppm(path: &str, w: u32, h: u32, rgba: &[u8]) {
    let mut out = format!("P6 {w} {h} 255\n").into_bytes();
    out.extend(rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]));
    std::fs::write(path, out).unwrap();
}

fn count(px: &[u8], pred: impl Fn(&[u8]) -> bool) -> usize {
    px.chunks_exact(4).filter(|p| pred(p)).count()
}

#[test]
fn renders_candles_volume_lines_and_hist() {
    let Some(mut h) = Headless::new(800, 400) else {
        eprintln!("no wgpu adapter; skipping");
        return;
    };
    eprintln!("adapter: {:?}", h.adapter_info.name);
    let bars = sample::generate("AAPL", Timeframe::D1, 2_000, 1_790_000_000);
    let series = SeriesGpu::build(&bars);
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let overlays = [Overlay { line: line_ref(&indicators::sma(&closes, 20)), color: [0.0, 0.6, 1.0, 1.0], kind: LineKind::Line, scale: 1.0 }];
    let v = Viewport::fit_last(bars.len(), 150, 800.0);
    for ct in ChartType::ALL {
        let out = price_pane(&PricePane {
            series: &series,
            chart_type: ct,
            show_volume: true,
            overlays: &overlays,
            viewport: &v,
            height_px: 400.0,
            log: false,
            manual_scale: None,
            style: Style::default(),
        });
        h.render(&mut [Frame { pane: 1, frame_no: 0, cmds: out.cmds }]);
        let px = h.read_pixels();
        if let Ok(dir) = std::env::var("TR_DUMP_DIR") {
            dump_ppm(&format!("{dir}/{ct:?}.ppm"), 800, 400, &px);
        }
        let green = count(&px, |p| p[1] > 150 && p[0] < 100);
        let red = count(&px, |p| p[0] > 200 && p[1] < 100);
        let blue = count(&px, |p| p[2] > 200 && p[0] < 50);
        eprintln!("{ct:?}: green {green} red {red} blue {blue}");
        assert!(blue > 300, "{ct:?}: SMA overlay not drawn");
        match ct {
            ChartType::Candles | ChartType::Bars => assert!(green > 500 && red > 500, "{ct:?}"),
            ChartType::Line | ChartType::Area => {
                let amber = count(&px, |p| p[0] > 200 && p[1] > 120 && p[1] < 200 && p[2] < 60);
                assert!(amber > 300, "{ct:?}: price line missing ({amber})");
            }
        }
    }

    // MACD histogram in a sub-pane.
    let m = indicators::macd(&closes, 12, 26, 9);
    let hist = [Overlay { line: line_ref(&m.histogram), color: [1.0; 4], kind: LineKind::Hist, scale: 1.0 }];
    let out = sub_pane(&hist, &v, 400.0, None, Style::default());
    h.render(&mut [Frame { pane: 2, frame_no: 0, cmds: out.cmds }]);
    let px = h.read_pixels();
    assert!(count(&px, |p| p[1] > 150 && p[0] < 100) > 200);
    assert!(count(&px, |p| p[0] > 200 && p[1] < 100) > 200);
}
