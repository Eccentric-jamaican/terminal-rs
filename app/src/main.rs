//! terminal-rs: a keyboard-driven, Bloomberg-style market terminal with GPU charts and
//! congressional (STOCK Act) trade tracking.

mod app;
mod chart_view;
mod command;
mod congress_view;
mod layout;
mod theme;
mod worker;

use std::time::Instant;

fn main() -> eframe::Result {
    let t0 = Instant::now();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,wgpu_core=warn,wgpu_hal=warn,naga=warn".into()))
        .init();

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("terminal-rs")
            .with_inner_size([1600.0, 960.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "terminal-rs",
        options,
        Box::new(move |cc| {
            let rs = cc.wgpu_render_state.as_ref().ok_or("terminal-rs requires the wgpu renderer")?;
            rs.renderer
                .write()
                .callback_resources
                .insert(tr_chart::gpu::ChartRenderer::new(&rs.device, rs.target_format, 1));
            let provider = tr_data::from_env();
            let name = provider.name().to_string();
            let worker = worker::Worker::spawn(cc.egui_ctx.clone(), provider)?;
            tracing::info!("window + GPU ready in {:?} (adapter: {})", t0.elapsed(), rs.adapter.get_info().name);
            Ok(Box::new(app::TerminalApp::new(&cc.egui_ctx, worker, name)))
        }),
    )
}
