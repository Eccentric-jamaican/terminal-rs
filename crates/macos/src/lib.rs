//! Native macOS window chrome. Everything here is a no-op on other platforms.
//!
//! - [`style_window`]: transparent full-size title bar with the traffic lights inset by an
//!   empty unified `NSToolbar`, plus an `NSVisualEffectView` (sidebar material) behind the
//!   app's content view so translucent regions show system vibrancy.
//! - [`accent_color`]: the user's system accent colour (`NSColor.controlAccentColor`).

use std::ffi::c_void;

/// Height of the unified title bar/toolbar region the app should leave for the system,
/// and the horizontal space taken by the traffic lights, in points.
pub const TITLEBAR_HEIGHT: f32 = 52.0;
pub const TRAFFIC_LIGHTS_WIDTH: f32 = 78.0;

#[cfg(target_os = "macos")]
mod imp {
    use super::c_void;
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSColor, NSColorSpace, NSToolbar, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode, NSWindowTitleVisibility,
        NSWindowToolbarStyle,
    };
    use objc2_foundation::NSString;

    /// # Safety
    /// `ns_view` must be a valid `NSView*` attached to a window, and this must run on the
    /// main thread (eframe calls the app's `ui` on the main thread on macOS).
    pub unsafe fn style_window(ns_view: *mut c_void, vibrancy: bool) -> bool {
        let Some(mtm) = MainThreadMarker::new() else { return false };
        if ns_view.is_null() {
            return false;
        }
        // SAFETY: caller guarantees a live NSView on the main thread.
        let view: &NSView = unsafe { &*(ns_view as *const NSView) };
        let Some(window) = view.window() else { return false };
        window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        window.setTitlebarAppearsTransparent(true);
        let toolbar = NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), &NSString::from_str("terminal-rs.toolbar"));
        window.setToolbar(Some(&toolbar));
        window.setToolbarStyle(NSWindowToolbarStyle::Unified);
        if vibrancy {
            let Some(content) = window.contentView() else { return true };
            let frame = content.frame();
            let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), frame);
            effect.setMaterial(NSVisualEffectMaterial::Sidebar);
            effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            effect.setState(NSVisualEffectState::FollowsWindowActiveState);
            effect.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            // Behind the content view (which hosts the Metal layer), so translucent
            // pixels the app draws reveal the blurred desktop/wallpaper.
            match unsafe { content.superview() } {
                Some(parent) => {
                    parent.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, Some(&content))
                }
                None => content.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, None),
            }
            window.setOpaque(false);
            window.setBackgroundColor(Some(&NSColor::clearColor()));
        }
        true
    }

    pub fn accent_color() -> Option<[u8; 3]> {
        MainThreadMarker::new()?;
        let c = NSColor::controlAccentColor();
        let srgb = c.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
        let to = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Some([to(srgb.redComponent()), to(srgb.greenComponent()), to(srgb.blueComponent())])
    }
}

/// Apply native chrome to the window owning `ns_view`. Returns whether it was applied.
///
/// # Safety
/// On macOS, `ns_view` must be a valid `NSView*` and the call must be on the main thread.
pub unsafe fn style_window(ns_view: *mut c_void, vibrancy: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: forwarded caller contract.
        unsafe { imp::style_window(ns_view, vibrancy) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ns_view, vibrancy);
        false
    }
}

/// The system accent colour as sRGB, if the platform has one.
pub fn accent_color() -> Option<[u8; 3]> {
    #[cfg(target_os = "macos")]
    {
        imp::accent_color()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

#[cfg(test)]
mod tests {
    #[test]
    fn stubs_are_safe_off_macos() {
        if !super::is_macos() {
            assert!(super::accent_color().is_none());
            assert!(!unsafe { super::style_window(std::ptr::null_mut(), true) });
        }
    }
}
