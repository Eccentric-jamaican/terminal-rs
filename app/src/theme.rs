//! Bloomberg-terminal look: black background, amber text, dense monospace everything.

use egui::{Color32, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub const BG: Color32 = Color32::from_rgb(0, 0, 0);
pub const PANEL: Color32 = Color32::from_rgb(10, 10, 10);
pub const HEADER: Color32 = Color32::from_rgb(28, 28, 28);
pub const GRID: Color32 = Color32::from_rgb(30, 30, 30);
pub const BORDER: Color32 = Color32::from_rgb(55, 55, 55);
pub const AMBER: Color32 = Color32::from_rgb(255, 176, 0);
pub const AMBER_DIM: Color32 = Color32::from_rgb(170, 118, 0);
pub const ORANGE_BAR: Color32 = Color32::from_rgb(251, 139, 30);
pub const TEXT: Color32 = Color32::from_rgb(230, 230, 230);
pub const MUTED: Color32 = Color32::from_rgb(130, 130, 130);
pub const UP: Color32 = Color32::from_rgb(38, 209, 125);
pub const DOWN: Color32 = Color32::from_rgb(255, 69, 69);
pub const CYAN: Color32 = Color32::from_rgb(0, 200, 255);
pub const FOCUS: Color32 = Color32::from_rgb(255, 176, 0);

pub const DEM: Color32 = Color32::from_rgb(56, 132, 255);
pub const REP: Color32 = Color32::from_rgb(235, 52, 52);
pub const IND: Color32 = Color32::from_rgb(170, 90, 255);
pub const UNKNOWN_PARTY: Color32 = Color32::from_rgb(150, 150, 150);

/// Overlay palette for indicators and compare series.
pub const SERIES: [Color32; 6] = [
    Color32::from_rgb(0, 160, 255),
    Color32::from_rgb(255, 0, 200),
    Color32::from_rgb(255, 230, 0),
    Color32::from_rgb(0, 230, 200),
    Color32::from_rgb(200, 200, 200),
    Color32::from_rgb(255, 120, 60),
];

pub fn party_color(p: tr_congress::Party) -> Color32 {
    match p {
        tr_congress::Party::Democrat => DEM,
        tr_congress::Party::Republican => REP,
        tr_congress::Party::Independent => IND,
        tr_congress::Party::Unknown => UNKNOWN_PARTY,
    }
}

pub fn rgba(c: Color32) -> [f32; 4] {
    [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, c.a() as f32 / 255.0]
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = Color32::from_rgb(14, 14, 14);
    v.override_text_color = Some(TEXT);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.selection.bg_fill = Color32::from_rgb(90, 60, 0);
    v.selection.stroke = Stroke::new(1.0, AMBER);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = HEADER;
    v.widgets.inactive.weak_bg_fill = HEADER;
    v.widgets.hovered.bg_fill = Color32::from_rgb(50, 40, 10);
    v.widgets.active.bg_fill = Color32::from_rgb(80, 60, 0);
    v.window_corner_radius = 0.into();
    v.menu_corner_radius = 0.into();
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = 0.into();
    }
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, mono(10.0)),
            (TextStyle::Body, mono(12.5)),
            (TextStyle::Button, mono(12.5)),
            (TextStyle::Heading, mono(15.0)),
            (TextStyle::Monospace, mono(12.5)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(6.0, 2.0);
        s.spacing.button_padding = egui::vec2(5.0, 1.0);
        s.spacing.interact_size.y = 16.0;
    });
}
