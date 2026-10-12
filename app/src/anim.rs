//! Spring animations (SwiftUI-like), integrated per frame and keyed by egui `Id`.

use egui::{Context, Id};

/// A damped spring moving `pos` toward a target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub pos: f32,
    pub vel: f32,
}

/// Response and damping of the default UI spring (slightly under-damped, like
/// SwiftUI's `.spring(response: 0.35, dampingFraction: 0.82)`).
pub const RESPONSE: f32 = 0.35;
pub const DAMPING: f32 = 0.82;

impl Spring {
    pub fn at(pos: f32) -> Self {
        Self { pos, vel: 0.0 }
    }

    /// Advance by `dt` seconds toward `target`. `response` is the undamped period in
    /// seconds and `damping` the damping fraction (1 = critically damped).
    pub fn step(&mut self, target: f32, dt: f32, response: f32, damping: f32) {
        let omega = std::f32::consts::TAU / response.max(1e-3);
        let k = omega * omega;
        let c = 2.0 * damping * omega;
        // Sub-step for stability with long frames.
        let n = (dt / (1.0 / 240.0)).ceil().clamp(1.0, 32.0) as usize;
        let h = dt / n as f32;
        for _ in 0..n {
            let acc = -k * (self.pos - target) - c * self.vel;
            self.vel += acc * h;
            self.pos += self.vel * h;
        }
    }

    pub fn settled(&self, target: f32, scale: f32) -> bool {
        let eps = 1e-3 * scale.abs().max(1.0);
        (self.pos - target).abs() < eps && self.vel.abs() < eps * 10.0
    }
}

/// Animate a value toward `target` with the default spring; returns the current value.
/// The first call snaps to the target. Requests repaints until the spring settles.
pub fn spring(ctx: &Context, id: Id, target: f32) -> f32 {
    spring_with(ctx, id, target, RESPONSE, DAMPING)
}

pub fn spring_with(ctx: &Context, id: Id, target: f32, response: f32, damping: f32) -> f32 {
    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
    let mut s = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || Spring::at(target)));
    if s.settled(target, target) {
        s = Spring::at(target);
    } else {
        s.step(target, dt, response, damping);
        if s.settled(target, target) {
            s = Spring::at(target);
        } else {
            ctx.request_repaint();
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, s));
    s.pos
}

/// Spring between 0 and 1 for a boolean (hover/focus fades, panel open/closed).
pub fn spring_bool(ctx: &Context, id: Id, on: bool) -> f32 {
    spring_with(ctx, id, if on { 1.0 } else { 0.0 }, 0.25, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_with_small_overshoot() {
        let mut s = Spring::at(0.0);
        let mut max = 0.0f32;
        for _ in 0..240 {
            s.step(100.0, 1.0 / 120.0, RESPONSE, DAMPING);
            max = max.max(s.pos);
        }
        assert!((s.pos - 100.0).abs() < 0.5, "{}", s.pos);
        assert!(max > 100.0 && max < 106.0, "overshoot {max}");
        let mut c = Spring::at(0.0);
        let mut cmax = 0.0f32;
        for _ in 0..240 {
            c.step(1.0, 1.0 / 60.0, 0.25, 1.0);
            cmax = cmax.max(c.pos);
        }
        assert!(cmax <= 1.0 + 1e-3, "critically damped must not overshoot: {cmax}");
    }

    #[test]
    fn long_frames_stay_stable() {
        let mut s = Spring::at(0.0);
        s.step(1.0, 0.1, RESPONSE, DAMPING);
        assert!(s.pos.is_finite() && s.pos.abs() < 2.0);
    }

    #[test]
    fn context_helper_snaps_then_animates() {
        let ctx = Context::default();
        let id = Id::new("x");
        assert_eq!(spring(&ctx, id, 5.0), 5.0);
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        let v = spring(&ctx, id, 10.0);
        assert!((5.0..=10.5).contains(&v));
    }
}
