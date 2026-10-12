//! User preferences persisted in `prefs.json`: appearance (theme mode, colours, fonts,
//! density), the hotkey map and defaults for new panels.

use std::collections::BTreeMap;

use egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};
use tr_chart::scene::ChartType;
use tr_core::Timeframe;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Density {
    Compact,
    #[default]
    Regular,
    Comfortable,
}

impl Density {
    pub const ALL: [Density; 3] = [Density::Compact, Density::Regular, Density::Comfortable];

    /// Multiplier for row heights, paddings and gaps.
    pub fn factor(self) -> f32 {
        match self {
            Density::Compact => 0.82,
            Density::Regular => 1.0,
            Density::Comfortable => 1.2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Density::Compact => "Compact",
            Density::Regular => "Regular",
            Density::Comfortable => "Comfortable",
        }
    }
}

/// Font for window chrome (labels, titles, menus).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UiFont {
    /// SF Pro on macOS (the platform UI font).
    #[default]
    System,
    /// Monospace everywhere, classic terminal look.
    Monospace,
}

/// Font for numbers, tickers and the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MonoFont {
    /// SF Mono on macOS, else the bundled monospace font.
    #[default]
    System,
    Menlo,
    Monaco,
    Courier,
    /// egui's bundled Hack.
    Bundled,
}

impl MonoFont {
    pub const ALL: [MonoFont; 5] =
        [MonoFont::System, MonoFont::Menlo, MonoFont::Monaco, MonoFont::Courier, MonoFont::Bundled];

    pub fn label(self) -> &'static str {
        match self {
            MonoFont::System => "System (SF Mono)",
            MonoFont::Menlo => "Menlo",
            MonoFont::Monaco => "Monaco",
            MonoFont::Courier => "Courier New",
            MonoFont::Bundled => "Hack (bundled)",
        }
    }

    /// Candidate font files, first existing wins.
    pub fn paths(self) -> &'static [&'static str] {
        match self {
            MonoFont::System => &[
                "/System/Library/Fonts/SFNSMono.ttf",
                "/System/Library/Fonts/Menlo.ttc",
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            ],
            MonoFont::Menlo => &["/System/Library/Fonts/Menlo.ttc"],
            MonoFont::Monaco => &["/System/Library/Fonts/Monaco.ttf", "/System/Library/Fonts/Monaco.dfont"],
            MonoFont::Courier => &[
                "/System/Library/Fonts/Supplemental/Courier New.ttf",
                "/usr/share/fonts/truetype/msttcorefonts/Courier_New.ttf",
            ],
            MonoFont::Bundled => &[],
        }
    }
}

pub type Rgb = [u8; 3];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub mode: ThemeMode,
    /// `None` follows the system accent colour.
    pub accent: Option<Rgb>,
    pub up: Option<Rgb>,
    pub down: Option<Rgb>,
    /// Tickers, the command line and other "Bloomberg amber" highlights.
    pub highlight: Option<Rgb>,
    pub ui_font: UiFont,
    pub mono_font: MonoFont,
    /// Global text size multiplier.
    pub font_scale: f32,
    pub density: Density,
    pub vibrancy: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: ThemeMode::System,
            accent: None,
            up: None,
            down: None,
            highlight: None,
            ui_font: UiFont::System,
            mono_font: MonoFont::System,
            font_scale: 1.0,
            density: Density::Regular,
            vibrancy: true,
        }
    }
}

/// Defaults applied to newly created panels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelDefaults {
    pub chart_type: ChartType,
    pub timeframe: Timeframe,
    pub show_volume: bool,
    pub congress_markers: bool,
    pub book_depth: usize,
}

impl Default for PanelDefaults {
    fn default() -> Self {
        Self {
            chart_type: ChartType::Candles,
            timeframe: Timeframe::D1,
            show_volume: true,
            congress_markers: true,
            book_depth: 20,
        }
    }
}

/// Every rebindable action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Action {
    Help,
    Congress,
    Politicians,
    Alerts,
    Watchlist,
    Refresh,
    FrameTiming,
    NextPane,
    PrevPane,
    ToggleSidebar,
    SplitRight,
    SplitDown,
    NewTab,
    ClosePane,
    PopOut,
    SaveLayout,
    Settings,
    DrawTrendline,
    DrawHorizontal,
    DrawVertical,
    DrawFib,
    ResetView,
    LogScale,
    Magnet,
    Layout1,
    Layout2,
    Layout3,
    Layout4,
    Layout5,
    Layout6,
    Layout7,
    Layout8,
    Layout9,
}

impl Action {
    pub const ALL: [Action; 33] = [
        Action::Help,
        Action::Congress,
        Action::Politicians,
        Action::Alerts,
        Action::Watchlist,
        Action::Refresh,
        Action::FrameTiming,
        Action::NextPane,
        Action::PrevPane,
        Action::ToggleSidebar,
        Action::SplitRight,
        Action::SplitDown,
        Action::NewTab,
        Action::ClosePane,
        Action::PopOut,
        Action::SaveLayout,
        Action::Settings,
        Action::DrawTrendline,
        Action::DrawHorizontal,
        Action::DrawVertical,
        Action::DrawFib,
        Action::ResetView,
        Action::LogScale,
        Action::Magnet,
        Action::Layout1,
        Action::Layout2,
        Action::Layout3,
        Action::Layout4,
        Action::Layout5,
        Action::Layout6,
        Action::Layout7,
        Action::Layout8,
        Action::Layout9,
    ];

    pub fn label(self) -> String {
        match self {
            Action::Help => "Help".into(),
            Action::Congress => "Congress feed".into(),
            Action::Politicians => "Politicians".into(),
            Action::Alerts => "Alerts".into(),
            Action::Watchlist => "Watchlist".into(),
            Action::Refresh => "Refresh Congress filings".into(),
            Action::FrameTiming => "Frame timing readout".into(),
            Action::NextPane => "Next pane".into(),
            Action::PrevPane => "Previous pane".into(),
            Action::ToggleSidebar => "Toggle sidebar".into(),
            Action::SplitRight => "Split right".into(),
            Action::SplitDown => "Split down".into(),
            Action::NewTab => "New tab".into(),
            Action::ClosePane => "Close pane".into(),
            Action::PopOut => "Pop out pane".into(),
            Action::SaveLayout => "Save layout".into(),
            Action::Settings => "Settings".into(),
            Action::DrawTrendline => "Draw trendline".into(),
            Action::DrawHorizontal => "Draw horizontal line".into(),
            Action::DrawVertical => "Draw vertical line".into(),
            Action::DrawFib => "Draw Fibonacci retracement".into(),
            Action::ResetView => "Reset chart view".into(),
            Action::LogScale => "Toggle log scale".into(),
            Action::Magnet => "Toggle magnet".into(),
            a => format!("Switch to layout {}", a.layout_slot().unwrap_or(0) + 1),
        }
    }

    /// Index into the saved-layout list for `Layout1..Layout9`.
    pub fn layout_slot(self) -> Option<usize> {
        let i = Action::ALL.iter().position(|&a| a == self)?;
        let first = Action::ALL.iter().position(|&a| a == Action::Layout1)?;
        (i >= first).then(|| i - first)
    }
}

/// Modifier keys arrive as key presses of their own; they never form a binding alone.
pub fn is_modifier(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
    )
}

/// A key with exact modifiers. `command` is ⌘ on macOS and Ctrl elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub alt: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub command: bool,
}

impl Chord {
    pub fn new(key: Key) -> Self {
        Self { key, alt: false, shift: false, ctrl: false, command: false }
    }

    pub fn from_event(key: Key, m: Modifiers) -> Self {
        // On non-mac platforms Ctrl is the command key; report it once.
        let mac = cfg!(target_os = "macos");
        Self { key, alt: m.alt, shift: m.shift, ctrl: if mac { m.ctrl } else { false }, command: m.command }
    }

    /// Parse `"Alt+T"`, `"Cmd+Shift+S"`, `"Ctrl+Cmd+S"`, `"F1"`, `"Shift+Tab"`.
    pub fn parse(s: &str) -> Option<Self> {
        let mut c = Chord::new(Key::Escape);
        let mut key = None;
        for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "alt" | "option" | "opt" | "⌥" => c.alt = true,
                "shift" | "⇧" => c.shift = true,
                "ctrl" | "control" | "⌃" => {
                    if cfg!(target_os = "macos") {
                        c.ctrl = true
                    } else {
                        c.command = true
                    }
                }
                "cmd" | "command" | "⌘" | "mod" => c.command = true,
                _ => key = Some(Key::from_name(part).or_else(|| Key::from_name(&part.to_ascii_uppercase()))?),
            }
        }
        c.key = key.filter(|k| !is_modifier(*k))?;
        Some(c)
    }

    /// Canonical text form (round-trips through [`Chord::parse`]).
    pub fn format(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.command {
            s.push_str(if cfg!(target_os = "macos") { "Cmd+" } else { "Ctrl+" });
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        s.push_str(self.key.name());
        s
    }

    /// Compact display form, using macOS symbols on macOS.
    pub fn display(&self) -> String {
        if !cfg!(target_os = "macos") {
            let f = self.format();
            return match f.rsplit_once('+') {
                Some((mods, _)) => format!("{mods}+{}", self.key.symbol_or_name()),
                None => self.key.symbol_or_name().to_string(),
            };
        }
        let mut s = String::new();
        for (on, sym) in [(self.ctrl, "⌃"), (self.alt, "⌥"), (self.shift, "⇧"), (self.command, "⌘")] {
            if on {
                s.push_str(sym);
            }
        }
        s.push_str(self.key.symbol_or_name());
        s
    }

    pub fn matches(&self, key: Key, m: Modifiers) -> bool {
        *self == Chord::from_event(key, m)
    }
}

/// Hotkey bindings; stored as text so the JSON is human-editable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Keymap(pub BTreeMap<Action, String>);

impl Default for Keymap {
    fn default() -> Self {
        let mac = cfg!(target_os = "macos");
        let mut m = BTreeMap::new();
        let mut bind = |a: Action, k: &str| {
            m.insert(a, k.to_string());
        };
        bind(Action::Help, "F1");
        bind(Action::Congress, "F2");
        bind(Action::Politicians, "F3");
        bind(Action::Alerts, "F4");
        bind(Action::Watchlist, "F5");
        bind(Action::Refresh, "F6");
        bind(Action::FrameTiming, "F12");
        bind(Action::NextPane, "Tab");
        bind(Action::PrevPane, "Shift+Tab");
        bind(Action::ToggleSidebar, if mac { "Ctrl+Cmd+S" } else { "Ctrl+Shift+S" });
        bind(Action::SplitRight, "Cmd+D");
        bind(Action::SplitDown, "Cmd+Shift+D");
        bind(Action::NewTab, "Cmd+T");
        bind(Action::ClosePane, "Cmd+W");
        bind(Action::PopOut, "Cmd+Shift+P");
        bind(Action::SaveLayout, "Cmd+S");
        bind(Action::Settings, "Cmd+Comma");
        bind(Action::DrawTrendline, "Alt+T");
        bind(Action::DrawHorizontal, "Alt+H");
        bind(Action::DrawVertical, "Alt+V");
        bind(Action::DrawFib, "Alt+F");
        bind(Action::ResetView, "Alt+R");
        bind(Action::LogScale, "Alt+L");
        bind(Action::Magnet, "Alt+M");
        for (i, a) in Action::ALL.iter().filter(|a| a.layout_slot().is_some()).enumerate() {
            bind(*a, &format!("Cmd+{}", i + 1));
        }
        Keymap(m)
    }
}

impl Keymap {
    pub fn chord(&self, a: Action) -> Option<Chord> {
        self.0.get(&a).and_then(|s| Chord::parse(s))
    }

    /// Which action a key press triggers.
    pub fn action_for(&self, key: Key, m: Modifiers) -> Option<Action> {
        self.0.iter().find(|(_, s)| Chord::parse(s).is_some_and(|c| c.matches(key, m))).map(|(a, _)| *a)
    }

    /// Bind `a` to `chord`, unbinding any other action that used it. Returns the displaced action.
    pub fn bind(&mut self, a: Action, chord: Chord) -> Option<Action> {
        let text = chord.format();
        let displaced = self.0.iter().find(|(b, s)| **b != a && Chord::parse(s) == Some(chord)).map(|(b, _)| *b);
        if let Some(d) = displaced {
            self.0.remove(&d);
        }
        self.0.insert(a, text);
        displaced
    }

    /// Fill in defaults for actions missing from an older or hand-edited file.
    pub fn with_defaults(mut self) -> Self {
        for (a, k) in Keymap::default().0 {
            let taken = Chord::parse(&k).is_some_and(|c| self.0.values().any(|v| Chord::parse(v) == Some(c)));
            if !self.0.contains_key(&a) && !taken {
                self.0.insert(a, k);
            }
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub appearance: Appearance,
    pub hotkeys: Keymap,
    pub panels: PanelDefaults,
}

impl Prefs {
    pub fn load(path: &std::path::Path) -> Prefs {
        let mut p: Prefs = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        p.hotkeys = p.hotkeys.with_defaults();
        p.appearance.font_scale = p.appearance.font_scale.clamp(0.7, 1.6);
        p
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        crate::layouts::write_atomic(path, &serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_parse_and_round_trip() {
        let c = Chord::parse("Alt+T").unwrap();
        assert_eq!(c, Chord { key: Key::T, alt: true, shift: false, ctrl: false, command: false });
        for s in ["F1", "Shift+Tab", "Cmd+Shift+D", "Alt+R", "Cmd+1", "Cmd+Comma"] {
            let c = Chord::parse(s).unwrap();
            assert_eq!(Chord::parse(&c.format()), Some(c), "{s}");
        }
        assert!(Chord::parse("Cmd+").is_none());
        assert!(Chord::parse("Cmd+NotAKey").is_none());
        assert!(Chord::parse("AltLeft").is_none(), "a bare modifier is not a binding");
        assert!(is_modifier(Key::SuperRight) && !is_modifier(Key::F9));
        assert!(c.matches(Key::T, Modifiers::ALT));
        assert!(!c.matches(Key::T, Modifiers::ALT | Modifiers::SHIFT), "modifiers match exactly");
    }

    #[test]
    fn keymap_defaults_lookup_and_rebinding() {
        let mut k = Keymap::default();
        assert_eq!(k.action_for(Key::F1, Modifiers::NONE), Some(Action::Help));
        assert_eq!(k.action_for(Key::Tab, Modifiers::SHIFT), Some(Action::PrevPane));
        assert_eq!(k.action_for(Key::Tab, Modifiers::NONE), Some(Action::NextPane));
        assert_eq!(k.action_for(Key::Num3, Modifiers::COMMAND), Some(Action::Layout3));
        // Rebinding a used chord steals it from the previous action.
        let displaced = k.bind(Action::Settings, Chord::parse("F1").unwrap());
        assert_eq!(displaced, Some(Action::Help));
        assert_eq!(k.action_for(Key::F1, Modifiers::NONE), Some(Action::Settings));
        assert!(k.chord(Action::Help).is_none());
        // Missing actions get defaults back unless their chord is taken.
        let restored = k.clone().with_defaults();
        assert!(restored.chord(Action::Help).is_none());
        assert!(restored.chord(Action::Magnet).is_some());
        assert_eq!(Action::Layout9.layout_slot(), Some(8));
        assert_eq!(Action::Help.layout_slot(), None);
    }

    #[test]
    fn prefs_json_round_trip_and_partial_files() {
        let mut p = Prefs::default();
        p.appearance.mode = ThemeMode::Light;
        p.appearance.accent = Some([255, 45, 85]);
        p.appearance.density = Density::Compact;
        p.panels.chart_type = ChartType::Line;
        p.hotkeys.bind(Action::Help, Chord::parse("Cmd+Shift+H").unwrap());
        let json = serde_json::to_string_pretty(&p).unwrap();
        assert!(json.contains("\"Help\": \"Cmd+Shift+H\"") || json.contains("\"Help\": \"Ctrl+Shift+H\""));
        let back: Prefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        // Older or hand-written files with only some fields still load.
        let partial: Prefs = serde_json::from_str(r#"{"appearance":{"density":"Comfortable"}}"#).unwrap();
        assert_eq!(partial.appearance.density, Density::Comfortable);
        assert_eq!(partial.appearance.font_scale, 1.0);
        assert_eq!(partial.panels, PanelDefaults::default());
        let dir = std::env::temp_dir().join(format!("tr-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefs.json");
        p.save(&path).unwrap();
        assert_eq!(Prefs::load(&path), p);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
