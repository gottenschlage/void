# ADR 0004: Own the integrated macOS title-bar content

- **Status:** Accepted
- **Date:** 2026-07-30
- **Decision owners:** Void maintainers

## Context

Void needs branch tabs and the sidebar toggle in the topmost window row without
losing the native macOS frame, rounded corners, shadow, resizing, or traffic
lights. The pinned GPUI revision supports a transparent title bar, full-size
content, traffic-light positioning, app-owned dragging, and a public AppKit
`NSView` raw handle. It does not expose standard-button visibility.

Sunware uses the same ownership split: the application draws the integrated
row, hides traffic lights with the collapsed sidebar, restores their position
when shown, and forces them visible in fullscreen. Zed provides the production
GPUI patterns for custom-title-bar movement and input boundaries.

## Considered options

### Keep a native title bar above Void's header

This preserves all AppKit behavior but duplicates vertical chrome and cannot
place branch tabs in the requested title-bar region.

### Fork GPUI to expose traffic-light visibility

This would provide a typed API but adds a long-lived platform fork for three
AppKit property writes.

### Use GPUI's native window plus a target-specific AppKit adapter

This preserves the native window and confines the missing visibility operation
to one documented macOS module. Other platforms remain unchanged.

## Decision

Use `TitlebarOptions { appears_transparent: true, traffic_light_position:
(16, 11) }`, `is_movable: true`, and `app_owns_titlebar_drag: true` on macOS.
Void draws a 37.5 px title row and owns movement using Zed's delayed
mouse-down/move pattern. Interactive descendants stop mouse-down propagation.

`VoidRoot` owns non-persisted sidebar visibility. The title segment transitions
between 240 px and 48 px, while the body sidebar transitions between 240 px and
zero without releasing its entity. Both animations are linear, 200 ms, and use
GPUI animation elements so reduced-motion mode resolves immediately to the end
state.

Add target-only `raw-window-handle 0.6` and `objc2-app-kit 0.3.2`. The adapter
casts the GPUI-provided AppKit handle to `NSView`, gets its retained `NSWindow`,
and sets `hidden` on the standard close, minimize, and zoom buttons. The cast is
performed synchronously on AppKit's main thread while GPUI's `Window` keeps the
view alive. Failure emits a diagnostic and otherwise changes no state.

Traffic lights are visible when the sidebar is open or the window is
fullscreen. A window-bounds observer detects fullscreen transitions. Showing
the controls also relies on GPUI's configured position, which its macOS window
implementation retains and reapplies.

## Consequences

- Void retains native macOS window behavior with product-owned title content.
- Toggle, selection, close, and reorder input cannot accidentally move the
  window.
- Sidebar collapse preserves branch and terminal entity identity.
- Two Apache-2.0/MIT-compatible target-only dependencies enter the binary;
  neither changes non-macOS builds or introduces a GPUI fork.
- The small unsafe boundary is isolated, documented, and covered by a pure
  visibility-policy test; AppKit integration still requires manual macOS QA.

## Implementation note

ADR 0011 later moved this state and title-bar implementation into the `workspace` crate and renamed the coordinator from `VoidRoot` to `WorkspaceView`; the ownership and behavior decided here are unchanged.

## References

- Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`
- Zed `crates/zed/src/zed.rs::build_window_options`
- Zed `crates/platform_title_bar/src/platform_title_bar.rs::PlatformTitleBar::render`
- Zed `crates/gpui_macos/src/window.rs::{MacWindow::new, HasWindowHandle for MacWindow}`
- Sunware `apps/desktop/src/main/index.ts`
- Sunware `apps/desktop/src/renderer/src/screens/workspace/index.tsx`
- Sunware `apps/desktop/src/renderer/src/screens/workspace/components/tab-bar.tsx`
- <https://gpui.rs/>
- <https://docs.rs/gpui/latest/gpui/struct.TitlebarOptions.html>
- <https://docs.rs/gpui/latest/gpui/struct.WindowOptions.html>
- <https://developer.apple.com/documentation/appkit/nsview/window>
- <https://developer.apple.com/documentation/appkit/nswindow/standardwindowbutton(_:)>
- <https://developer.apple.com/documentation/appkit/nsview/hidden>
