//! Tiling layout presets. Each preset splits the work area into N slots; panes fill slots
//! in order.

use egui::{Rect, pos2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Layout {
    One,
    TwoCols,
    TwoRows,
    /// Large pane on the left, two stacked on the right.
    MainRight2,
    Grid2x2,
    Grid3x2,
}

impl Layout {
    pub const ALL: [Layout; 6] =
        [Layout::One, Layout::TwoCols, Layout::TwoRows, Layout::MainRight2, Layout::Grid2x2, Layout::Grid3x2];

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_uppercase().as_str() {
            "1" => Layout::One,
            "2" | "2V" | "2C" => Layout::TwoCols,
            "2H" | "2R" => Layout::TwoRows,
            "3" => Layout::MainRight2,
            "4" => Layout::Grid2x2,
            "6" => Layout::Grid3x2,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Layout::One => "1",
            Layout::TwoCols => "2",
            Layout::TwoRows => "2H",
            Layout::MainRight2 => "3",
            Layout::Grid2x2 => "4",
            Layout::Grid3x2 => "6",
        }
    }

    pub fn slots(self) -> usize {
        match self {
            Layout::One => 1,
            Layout::TwoCols | Layout::TwoRows => 2,
            Layout::MainRight2 => 3,
            Layout::Grid2x2 => 4,
            Layout::Grid3x2 => 6,
        }
    }

    /// Slot rectangles inside `area`, separated by `gap` pixels.
    pub fn rects(self, area: Rect, gap: f32) -> Vec<Rect> {
        let grid = |cols: usize, rows: usize| -> Vec<Rect> {
            let w = (area.width() - gap * (cols - 1) as f32) / cols as f32;
            let h = (area.height() - gap * (rows - 1) as f32) / rows as f32;
            (0..rows)
                .flat_map(|r| {
                    (0..cols).map(move |c| {
                        let x = area.left() + c as f32 * (w + gap);
                        let y = area.top() + r as f32 * (h + gap);
                        Rect::from_min_size(pos2(x, y), egui::vec2(w, h))
                    })
                })
                .collect()
        };
        match self {
            Layout::One => vec![area],
            Layout::TwoCols => grid(2, 1),
            Layout::TwoRows => grid(1, 2),
            Layout::Grid2x2 => grid(2, 2),
            Layout::Grid3x2 => grid(3, 2),
            Layout::MainRight2 => {
                let main_w = (area.width() - gap) * 0.62;
                let main = Rect::from_min_size(area.min, egui::vec2(main_w, area.height()));
                let right = Rect::from_min_max(pos2(main.right() + gap, area.top()), area.max);
                let half = (right.height() - gap) / 2.0;
                vec![
                    main,
                    Rect::from_min_size(right.min, egui::vec2(right.width(), half)),
                    Rect::from_min_max(pos2(right.left(), right.top() + half + gap), right.max),
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_tile_the_area() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        for l in Layout::ALL {
            let rs = l.rects(area, 2.0);
            assert_eq!(rs.len(), l.slots(), "{l:?}");
            for r in &rs {
                assert!(area.expand(0.01).contains_rect(*r), "{l:?} {r:?}");
            }
            // Slots never overlap.
            for (i, a) in rs.iter().enumerate() {
                for b in &rs[i + 1..] {
                    assert!(a.intersect(*b).area() <= 0.0 || !a.intersects(*b), "{l:?}");
                }
            }
            assert_eq!(Layout::parse(l.label()), Some(l));
        }
        let g = Layout::Grid2x2.rects(area, 2.0);
        assert_eq!(g[3].max, pos2(1000.0, 600.0));
    }
}
