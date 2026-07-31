//! Narrow AppKit adapter for native title-bar button visibility.
//!
//! GPUI owns creation and lifetime of the `NSView` exposed by its public
//! `raw-window-handle` implementation. All calls into this module are made by
//! GPUI callbacks on AppKit's main thread.

use gpui::Window;
use objc2_app_kit::{NSView, NSWindowButton};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Shows or hides all three native macOS traffic-light buttons.
#[expect(
    unsafe_code,
    reason = "GPUI exposes AppKit only through a validated raw-window-handle pointer"
)]
pub(crate) fn set_traffic_lights_visible(window: &Window, visible: bool) -> Result<(), String> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("could not obtain the native window handle: {error}"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("GPUI returned a non-AppKit window handle on macOS".into());
    };

    // SAFETY: raw-window-handle specifies `ns_view` as a valid `NSView` pointer.
    // GPUI keeps that view alive for the lifetime of `Window`, and this function
    // runs synchronously on GPUI's AppKit main thread.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let native_window = view
        .window()
        .ok_or_else(|| "GPUI's NSView is not attached to an NSWindow".to_owned())?;

    for button_kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = native_window.standardWindowButton(button_kind) {
            button.setHidden(!visible);
        }
    }

    Ok(())
}
