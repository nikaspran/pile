use eframe::Frame;
use objc2_app_kit::NSView;
use objc2_foundation::NSRect;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const MAX_ZOOM_INSET: f64 = 24.0;
const FRAME_EPSILON: f64 = 1.0;

pub fn fill_visible_frame_when_zoomed(frame: &Frame) {
    let Ok(window_handle) = frame.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = window_handle.as_raw() else {
        return;
    };

    // AppKit handles are only valid on the main thread. eframe::App::update runs
    // on the UI thread, and raw-window-handle exposes this pointer for this use.
    let ns_view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    let Some(ns_window) = ns_view.window() else {
        return;
    };

    if !ns_window.isZoomed() {
        return;
    }

    let Some(screen) = ns_window.screen() else {
        return;
    };

    let frame = ns_window.frame();
    let visible = screen.visibleFrame();

    if should_fill_visible_frame(frame, visible) {
        ns_window.setFrame_display(visible, true);
    }
}

fn should_fill_visible_frame(frame: NSRect, visible: NSRect) -> bool {
    let left = frame.origin.x - visible.origin.x;
    let bottom = frame.origin.y - visible.origin.y;
    let right = visible_max_x(visible) - visible_max_x(frame);
    let top = visible_max_y(visible) - visible_max_y(frame);

    let is_inset = [left, bottom, right, top]
        .into_iter()
        .all(|gap| gap >= -FRAME_EPSILON && gap <= MAX_ZOOM_INSET);

    let already_matches = [left, bottom, right, top]
        .into_iter()
        .all(|gap| gap.abs() <= FRAME_EPSILON);

    is_inset && !already_matches
}

fn visible_max_x(rect: NSRect) -> f64 {
    rect.origin.x + rect.size.width
}

fn visible_max_y(rect: NSRect) -> f64 {
    rect.origin.y + rect.size.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::{CGPoint, CGRect, CGSize};

    #[test]
    fn fills_small_zoom_inset() {
        let visible = rect(0.0, 0.0, 1000.0, 700.0);
        let frame = rect(10.0, 10.0, 980.0, 680.0);

        assert!(should_fill_visible_frame(frame, visible));
    }

    #[test]
    fn leaves_matching_frame_alone() {
        let visible = rect(0.0, 0.0, 1000.0, 700.0);

        assert!(!should_fill_visible_frame(visible, visible));
    }

    #[test]
    fn leaves_large_manual_size_alone() {
        let visible = rect(0.0, 0.0, 1000.0, 700.0);
        let frame = rect(120.0, 90.0, 700.0, 500.0);

        assert!(!should_fill_visible_frame(frame, visible));
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
    }
}
