//! Preferences panel (`PREFS`): appearance, hotkeys, panel defaults and saved layouts.

use egui::{Color32, RichText, Ui};
use tr_chart::scene::ChartType;
use tr_core::Timeframe;

use super::TerminalApp;
use super::workspace::LayoutOp;
use crate::prefs::{self, Action, Density, MonoFont, ThemeMode, UiFont};
use crate::theme::{self, muted, text, ui_font};
use crate::widgets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Hotkeys,
    Panels,
    Layouts,
}

impl SettingsTab {
    const ALL: [SettingsTab; 4] =
        [SettingsTab::Appearance, SettingsTab::Hotkeys, SettingsTab::Panels, SettingsTab::Layouts];

    fn label(self) -> &'static str {
        match self {
            SettingsTab::Appearance => "Appearance",
            SettingsTab::Hotkeys => "Hotkeys",
            SettingsTab::Panels => "Panels",
            SettingsTab::Layouts => "Layouts",
        }
    }
}

pub enum SettingsCmd {
    Layout(LayoutOp),
    SavePrefs,
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(theme::dp(10.0));
    ui.label(RichText::new(title).font(ui_font(13.5)).color(text()));
    ui.separator();
}

/// An optional colour override with a picker and a reset button. Returns whether it changed.
fn color_override(ui: &mut Ui, label: &str, value: &mut Option<prefs::Rgb>, default: Color32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([140.0, 20.0], egui::Label::new(RichText::new(label).font(ui_font(12.5))));
        let mut rgb = value.unwrap_or([default.r(), default.g(), default.b()]);
        if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
            *value = Some(rgb);
            changed = true;
        }
        if value.is_some() {
            if ui.small_button("Default").clicked() {
                *value = None;
                changed = true;
            }
        } else {
            ui.label(RichText::new("default").font(ui_font(11.0)).color(muted()));
        }
    });
    changed
}

fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([140.0, 20.0], egui::Label::new(RichText::new(label).font(ui_font(12.5))));
        add(ui);
    });
}

impl TerminalApp {
    pub(super) fn settings_panel(&mut self, ui: &mut Ui) -> Vec<SettingsCmd> {
        let mut cmds = Vec::new();
        let labels: Vec<&str> = SettingsTab::ALL.iter().map(|t| t.label()).collect();
        let sel = SettingsTab::ALL.iter().position(|t| *t == self.settings_tab);
        if let Some(i) = widgets::segmented(ui, "settings-tabs", &labels, sel, 24.0) {
            self.settings_tab = SettingsTab::ALL[i];
            self.rebinding = None;
        }
        let width = ui.available_width() - 14.0;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(width);
            ui.spacing_mut().item_spacing.y = theme::dp(6.0);
            match self.settings_tab {
                SettingsTab::Appearance => self.appearance_tab(ui, &mut cmds),
                SettingsTab::Hotkeys => self.hotkeys_tab(ui, &mut cmds),
                SettingsTab::Panels => self.panels_tab(ui, &mut cmds),
                SettingsTab::Layouts => self.layouts_tab(ui, &mut cmds),
            }
        });
        cmds
    }

    pub(super) fn run_settings_cmds(&mut self, cmds: Vec<SettingsCmd>) {
        for c in cmds {
            match c {
                SettingsCmd::Layout(op) => self.layout_op(op),
                SettingsCmd::SavePrefs => self.save_prefs(),
            }
        }
    }

    fn appearance_tab(&mut self, ui: &mut Ui, cmds: &mut Vec<SettingsCmd>) {
        let before = self.prefs.appearance.clone();
        let a = &mut self.prefs.appearance;
        section(ui, "Theme");
        row(ui, "Appearance", |ui| {
            let modes = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];
            let sel = modes.iter().position(|m| *m == a.mode);
            if let Some(i) = widgets::segmented(ui, "theme-mode", &["System", "Light", "Dark"], sel, 22.0) {
                a.mode = modes[i];
            }
        });
        row(ui, "Accent colour", |ui| {
            let mut follow = a.accent.is_none();
            if ui.checkbox(&mut follow, "Follow system").changed() {
                a.accent = if follow { None } else { Some([10, 132, 255]) };
            }
            if let Some(rgb) = &mut a.accent {
                egui::color_picker::color_edit_button_srgb(ui, rgb);
            }
        });
        let base = theme::Palette::for_theme(ui.ctx().theme(), theme::DEFAULT_ACCENT);
        color_override(ui, "Up / gains", &mut a.up, base.up);
        color_override(ui, "Down / losses", &mut a.down, base.down);
        color_override(ui, "Highlight (tickers)", &mut a.highlight, base.amber);
        row(ui, "Sidebar position", |ui| {
            use crate::prefs::SidebarSide;
            let sides = [SidebarSide::Left, SidebarSide::Right];
            let sel = sides.iter().position(|s| *s == a.sidebar_side);
            if let Some(i) = widgets::segmented(ui, "sidebar-side", &["Left", "Right"], sel, 22.0) {
                a.sidebar_side = sides[i];
            }
        });
        if cfg!(target_os = "macos") {
            row(ui, "Sidebar vibrancy", |ui| {
                ui.checkbox(&mut a.vibrancy, "Translucent sidebar (applies on next launch)");
            });
        }

        section(ui, "Text");
        row(ui, "Interface font", |ui| {
            let fonts = [UiFont::System, UiFont::Monospace];
            let sel = fonts.iter().position(|f| *f == a.ui_font);
            if let Some(i) = widgets::segmented(ui, "ui-font", &["System (SF Pro)", "Monospace"], sel, 22.0) {
                a.ui_font = fonts[i];
            }
        });
        row(ui, "Numbers font", |ui| {
            egui::ComboBox::from_id_salt("mono-font").selected_text(a.mono_font.label()).show_ui(ui, |ui| {
                for f in MonoFont::ALL {
                    let available =
                        f == MonoFont::Bundled || f.paths().iter().any(|p| std::path::Path::new(p).exists());
                    let label =
                        if available { f.label().to_string() } else { format!("{} (not installed)", f.label()) };
                    ui.add_enabled_ui(available, |ui| ui.selectable_value(&mut a.mono_font, f, label));
                }
            });
        });
        row(ui, "Text size", |ui| {
            ui.add(
                egui::Slider::new(&mut a.font_scale, 0.8..=1.4)
                    .step_by(0.05)
                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
            );
        });
        row(ui, "Density", |ui| {
            let labels: Vec<&str> = Density::ALL.iter().map(|d| d.label()).collect();
            let sel = Density::ALL.iter().position(|d| *d == a.density);
            if let Some(i) = widgets::segmented(ui, "density", &labels, sel, 22.0) {
                a.density = Density::ALL[i];
            }
        });
        ui.add_space(8.0);
        if ui.button("Restore Default Appearance").clicked() {
            *a = prefs::Appearance::default();
        }
        if self.prefs.appearance != before {
            cmds.push(SettingsCmd::SavePrefs);
        }
    }

    fn hotkeys_tab(&mut self, ui: &mut Ui, cmds: &mut Vec<SettingsCmd>) {
        section(ui, "Keyboard shortcuts");
        if self.hotkey_table(ui) {
            cmds.push(SettingsCmd::SavePrefs);
        }
    }

    fn panels_tab(&mut self, ui: &mut Ui, cmds: &mut Vec<SettingsCmd>) {
        let before = self.prefs.panels.clone();
        let d = &mut self.prefs.panels;
        section(ui, "New charts");
        row(ui, "Chart type", |ui| {
            let labels: Vec<&str> = ChartType::ALL.iter().map(|c| c.label()).collect();
            let sel = ChartType::ALL.iter().position(|c| *c == d.chart_type);
            if let Some(i) = widgets::segmented(ui, "def-ct", &labels, sel, 22.0) {
                d.chart_type = ChartType::ALL[i];
            }
        });
        row(ui, "Interval", |ui| {
            let labels: Vec<&str> = Timeframe::ALL.iter().map(|t| t.label()).collect();
            let sel = Timeframe::ALL.iter().position(|t| *t == d.timeframe);
            if let Some(i) = widgets::segmented(ui, "def-tf", &labels, sel, 22.0) {
                d.timeframe = Timeframe::ALL[i];
            }
        });
        row(ui, "Show", |ui| {
            ui.checkbox(&mut d.show_volume, "Volume");
            ui.checkbox(&mut d.congress_markers, "Congress trade markers");
        });
        section(ui, "New order books");
        row(ui, "Depth (levels)", |ui| {
            let depths = [10usize, 20, 50];
            if let Some(i) = widgets::segmented(
                ui,
                "def-depth",
                &["10", "20", "50"],
                depths.iter().position(|&x| x == d.book_depth),
                22.0,
            ) {
                d.book_depth = depths[i];
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new("Each pane's own settings (chart type, indicators, filters, depth, link group, tab title) are saved with the layout.")
                .font(ui_font(11.5))
                .color(muted()),
        );
        if self.prefs.panels != before {
            cmds.push(SettingsCmd::SavePrefs);
        }
    }

    fn layouts_tab(&mut self, ui: &mut Ui, cmds: &mut Vec<SettingsCmd>) {
        section(ui, "Saved layouts");
        ui.label(
            RichText::new(format!("Stored as JSON in {}", self.store.dir.display())).font(ui_font(11.5)).color(muted()),
        );
        let names = self.layout_names.clone();
        egui::Grid::new("layouts").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            for (i, name) in names.iter().enumerate() {
                let current = name.eq_ignore_ascii_case(&self.layout_name);
                let hint = self
                    .prefs
                    .hotkeys
                    .chord(
                        Action::ALL
                            .iter()
                            .copied()
                            .filter(|a| a.layout_slot().is_some())
                            .nth(i)
                            .unwrap_or(Action::Help),
                    )
                    .filter(|_| i < 9)
                    .map(|c| format!("   {}", c.display()))
                    .unwrap_or_default();
                let label = RichText::new(format!("{}{name}{hint}", if current { "● " } else { "   " }))
                    .font(ui_font(12.5))
                    .color(if current { theme::accent() } else { text() });
                ui.label(label);
                ui.horizontal(|ui| {
                    if !current && ui.small_button("Switch").clicked() {
                        cmds.push(SettingsCmd::Layout(LayoutOp::Switch(name.clone())));
                    }
                    if ui.small_button("Rename").clicked() {
                        self.renaming_layout = Some((name.clone(), name.clone()));
                        self.sidebar_open = true;
                    }
                    if ui.small_button("Duplicate").clicked() {
                        cmds.push(SettingsCmd::Layout(LayoutOp::Duplicate(name.clone(), None)));
                    }
                    if ui.small_button("Export").clicked() {
                        cmds.push(SettingsCmd::Layout(LayoutOp::Export(name.clone(), None)));
                    }
                    if ui.small_button("Delete").clicked() {
                        cmds.push(SettingsCmd::Layout(LayoutOp::Delete(name.clone())));
                    }
                });
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("New Layout").clicked() {
                cmds.push(SettingsCmd::Layout(LayoutOp::New(None)));
            }
            if ui.button("Open Layouts Folder").clicked() {
                cmds.push(SettingsCmd::Layout(LayoutOp::OpenFolder));
            }
        });
        section(ui, "Import / export");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.io_path).hint_text("~/Downloads/layout.json").desired_width(320.0),
            );
            if ui.button("Import").clicked() && !self.io_path.trim().is_empty() {
                let p = self.io_path.trim();
                let p = match p.strip_prefix("~/") {
                    Some(rest) => std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default().join(rest),
                    None => p.into(),
                };
                cmds.push(SettingsCmd::Layout(LayoutOp::Import(p)));
            }
            if ui.button("Export Current").clicked() {
                let p = self.io_path.trim();
                let path = (!p.is_empty()).then(|| match p.strip_prefix("~/") {
                    Some(rest) => std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default().join(rest),
                    None => p.into(),
                });
                cmds.push(SettingsCmd::Layout(LayoutOp::Export(self.layout_name.clone(), path)));
            }
        });
        ui.label(
            RichText::new(
                "Export writes to the path given, or ~/Downloads. Imported layouts are copied into the layouts folder.",
            )
            .font(ui_font(11.5))
            .color(muted()),
        );
    }
}
