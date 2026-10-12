//! Look and feel: a native macOS-style chrome (SF Pro, rounded cards, system accent,
//! light/dark following the system) around Bloomberg-dense data (SF Mono numbers, amber
//! tickers, green/red moves).
//!
//! Colours come from the current [`Palette`], selected each frame from the system
//! appearance and accent colour; accessors like [`amber`] read it.

use std::cell::Cell;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme, Visuals};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Window background behind cards (shows vibrancy on macOS when translucent).
    pub window: Color32,
    /// Sidebar fill; translucent so the window's vibrancy shows through on macOS.
    pub sidebar: Color32,
    /// Card (pane) background — data lives here.
    pub card: Color32,
    pub card_header: Color32,
    pub border: Color32,
    pub grid: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub amber: Color32,
    pub amber_dim: Color32,
    pub up: Color32,
    pub down: Color32,
    pub cyan: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub accent: Color32,
    pub series: [Color32; 6],
}

impl Palette {
    pub fn dark(accent: Color32) -> Self {
        Self {
            dark: true,
            window: Color32::from_rgb(30, 30, 32),
            sidebar: Color32::from_rgba_unmultiplied(28, 28, 30, 150),
            card: Color32::from_rgb(12, 12, 13),
            card_header: Color32::from_rgb(28, 28, 30),
            border: Color32::from_rgba_unmultiplied(255, 255, 255, 22),
            grid: Color32::from_rgb(34, 34, 36),
            text: Color32::from_rgb(236, 236, 238),
            muted: Color32::from_rgb(142, 142, 147),
            amber: Color32::from_rgb(255, 176, 0),
            amber_dim: Color32::from_rgb(170, 118, 0),
            up: Color32::from_rgb(48, 209, 88),
            down: Color32::from_rgb(255, 69, 58),
            cyan: Color32::from_rgb(100, 210, 255),
            control: Color32::from_rgba_unmultiplied(255, 255, 255, 18),
            control_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 30),
            accent,
            series: [
                Color32::from_rgb(10, 132, 255),
                Color32::from_rgb(255, 55, 95),
                Color32::from_rgb(255, 214, 10),
                Color32::from_rgb(100, 210, 255),
                Color32::from_rgb(200, 200, 205),
                Color32::from_rgb(255, 159, 10),
            ],
        }
    }

    pub fn light(accent: Color32) -> Self {
        Self {
            dark: false,
            window: Color32::from_rgb(236, 236, 238),
            sidebar: Color32::from_rgba_unmultiplied(246, 246, 248, 150),
            card: Color32::from_rgb(255, 255, 255),
            card_header: Color32::from_rgb(244, 244, 246),
            border: Color32::from_rgba_unmultiplied(0, 0, 0, 26),
            grid: Color32::from_rgb(234, 234, 237),
            text: Color32::from_rgb(29, 29, 31),
            muted: Color32::from_rgb(110, 110, 115),
            amber: Color32::from_rgb(178, 104, 0),
            amber_dim: Color32::from_rgb(196, 150, 70),
            up: Color32::from_rgb(28, 150, 64),
            down: Color32::from_rgb(215, 40, 40),
            cyan: Color32::from_rgb(0, 122, 204),
            control: Color32::from_rgba_unmultiplied(0, 0, 0, 14),
            control_hover: Color32::from_rgba_unmultiplied(0, 0, 0, 24),
            accent,
            series: [
                Color32::from_rgb(0, 122, 255),
                Color32::from_rgb(214, 0, 120),
                Color32::from_rgb(196, 140, 0),
                Color32::from_rgb(0, 150, 170),
                Color32::from_rgb(90, 90, 96),
                Color32::from_rgb(220, 110, 0),
            ],
        }
    }

    pub fn for_theme(theme: Theme, accent: Color32) -> Self {
        match theme {
            Theme::Dark => Self::dark(accent),
            Theme::Light => Self::light(accent),
        }
    }
}

/// macOS default (blue) accent, used when the system does not report one.
pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(10, 132, 255);

thread_local! {
    static CURRENT: Cell<Palette> = Cell::new(Palette::dark(DEFAULT_ACCENT));
}

pub fn palette() -> Palette {
    CURRENT.with(Cell::get)
}

pub fn set_palette(p: Palette) {
    CURRENT.with(|c| c.set(p));
}

pub fn window() -> Color32 {
    palette().window
}
pub fn sidebar() -> Color32 {
    palette().sidebar
}
pub fn card() -> Color32 {
    palette().card
}
pub fn header() -> Color32 {
    palette().card_header
}
pub fn border() -> Color32 {
    palette().border
}
pub fn grid() -> Color32 {
    palette().grid
}
pub fn text() -> Color32 {
    palette().text
}
pub fn muted() -> Color32 {
    palette().muted
}
pub fn amber() -> Color32 {
    palette().amber
}
pub fn amber_dim() -> Color32 {
    palette().amber_dim
}
pub fn up() -> Color32 {
    palette().up
}
pub fn down() -> Color32 {
    palette().down
}
pub fn cyan() -> Color32 {
    palette().cyan
}
pub fn accent() -> Color32 {
    palette().accent
}
pub fn control() -> Color32 {
    palette().control
}
pub fn control_hover() -> Color32 {
    palette().control_hover
}
pub fn series(i: usize) -> Color32 {
    palette().series[i % 6]
}
/// Selection/highlight wash derived from the accent colour.
pub fn accent_wash() -> Color32 {
    let a = accent();
    Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), if palette().dark { 70 } else { 45 })
}

pub fn dem() -> Color32 {
    if palette().dark { Color32::from_rgb(64, 156, 255) } else { Color32::from_rgb(0, 102, 214) }
}
pub fn rep() -> Color32 {
    if palette().dark { Color32::from_rgb(255, 82, 82) } else { Color32::from_rgb(200, 30, 30) }
}
pub fn ind() -> Color32 {
    if palette().dark { Color32::from_rgb(191, 90, 242) } else { Color32::from_rgb(140, 50, 200) }
}

pub fn party_color(p: tr_congress::Party) -> Color32 {
    match p {
        tr_congress::Party::Democrat => dem(),
        tr_congress::Party::Republican => rep(),
        tr_congress::Party::Independent => ind(),
        tr_congress::Party::Unknown => muted(),
    }
}

pub fn rgba(c: Color32) -> [f32; 4] {
    [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, c.a() as f32 / 255.0]
}

/// Numbers, tickers and tables (SF Mono on macOS).
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Chrome: titles, buttons, labels (SF Pro on macOS).
pub fn ui_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// Corner radii for cards and controls.
pub const CARD_RADIUS: u8 = 10;
pub const CONTROL_RADIUS: u8 = 6;

/// Use the system UI fonts when present (read from the OS at runtime, not bundled).
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let candidates: &[(&str, FontFamily, &[&str])] = &[
        (
            "system-ui",
            FontFamily::Proportional,
            &[
                "/System/Library/Fonts/SFNS.ttf",
                "/System/Library/Fonts/SFNSText.ttf",
                "/System/Library/Fonts/Helvetica.ttc",
            ],
        ),
        (
            "system-mono",
            FontFamily::Monospace,
            &["/System/Library/Fonts/SFNSMono.ttf", "/System/Library/Fonts/Menlo.ttc"],
        ),
    ];
    for (name, family, paths) in candidates {
        if let Some(bytes) = paths.iter().find_map(|p| std::fs::read(p).ok()) {
            fonts.font_data.insert((*name).into(), std::sync::Arc::new(FontData::from_owned(bytes)));
            fonts.families.entry(family.clone()).or_default().insert(0, (*name).into());
        }
    }
    ctx.set_fonts(fonts);
}

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = p.card;
    v.extreme_bg_color = p.card;
    v.faint_bg_color = p.card_header;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_shadow =
        Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(if p.dark { 110 } else { 40 }) };
    v.popup_shadow =
        Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(if p.dark { 100 } else { 35 }) };
    v.selection.bg_fill = accent_wash_for(p);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.bg_fill = p.control;
    v.widgets.inactive.weak_bg_fill = p.control;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_fill = p.control_hover;
    v.widgets.hovered.weak_bg_fill = p.control_hover;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.bg_fill = accent_wash_for(p);
    v.widgets.active.weak_bg_fill = accent_wash_for(p);
    v.window_corner_radius = CARD_RADIUS.into();
    v.menu_corner_radius = 8.into();
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CONTROL_RADIUS.into();
    }
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v
}

fn accent_wash_for(p: &Palette) -> Color32 {
    Color32::from_rgba_unmultiplied(p.accent.r(), p.accent.g(), p.accent.b(), if p.dark { 70 } else { 45 })
}

/// One-time setup: fonts and text styles. Visuals are refreshed by [`refresh`].
pub fn apply(ctx: &egui::Context, accent: Color32) {
    install_fonts(ctx);
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, ui_font(10.5)),
            (TextStyle::Body, ui_font(13.0)),
            (TextStyle::Button, ui_font(13.0)),
            (TextStyle::Heading, ui_font(16.0)),
            (TextStyle::Monospace, mono(12.0)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(6.0, 3.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 20.0;
        s.spacing.menu_margin = egui::Margin::same(6);
        s.animation_time = 0.18;
    });
    refresh(ctx, accent);
}

/// Pick the palette for the current system appearance and accent; call every frame.
pub fn refresh(ctx: &egui::Context, accent: Color32) {
    let p = Palette::for_theme(ctx.theme(), accent);
    if p != palette() {
        ctx.set_visuals_of(Theme::Dark, visuals(&Palette::dark(accent)));
        ctx.set_visuals_of(Theme::Light, visuals(&Palette::light(accent)));
    }
    set_palette(p);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_are_readable() {
        for p in [Palette::dark(DEFAULT_ACCENT), Palette::light(DEFAULT_ACCENT)] {
            let lum = |c: Color32| 0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32;
            // Body text contrasts strongly with the card it sits on.
            assert!((lum(p.text) - lum(p.card)).abs() > 150.0, "{}", p.dark);
            assert!((lum(p.muted) - lum(p.card)).abs() > 60.0);
        }
        set_palette(Palette::light(Color32::RED));
        assert_eq!(accent(), Color32::RED);
        assert!(!palette().dark);
    }
}
