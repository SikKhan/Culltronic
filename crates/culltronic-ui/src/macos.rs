//! Native window layout information for the full-size macOS content view.

use objc2_app_kit::NSView;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Height obscured by the title bar, in egui points. AppKit updates its
/// layout rectangle as the window changes, including when entering fullscreen.
pub fn top_content_inset(frame: &eframe::Frame, zoom_factor: f32) -> Option<f32> {
    let handle = frame.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: eframe owns this AppKit view for the lifetime of the borrowed
    // window handle, and calls App::ui on the main thread.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let window = view.window()?;
    let safe = view.convertRect_fromView(window.contentLayoutRect(), None);
    let bounds = view.bounds();
    let inset = if view.isFlipped() {
        safe.origin.y - bounds.origin.y
    } else {
        bounds.origin.y + bounds.size.height - safe.origin.y - safe.size.height
    };
    Some((inset as f32 / zoom_factor).max(0.0))
}
