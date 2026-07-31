# ADR 0017: Bundle Vercel appearance and a shared UI scale

- **Status:** Accepted
- **Date:** 2026-08-01
- **Decision owners:** Void maintainers

## Context

Void needs one visual identity with light and dark variants, automatic native
appearance following, proportional application scaling, and terminal text
scaling that remains independent from application chrome. GPUI's eight default
colors cannot represent the status, Git, terminal, and interaction semantics
already rendered by Void. Zed's complete user-theme settings surface is much
broader than Void needs.

Nathan Brodin's `zed-vercel-theme` provides a schema-v0.2.0 family containing
exactly `Vercel Light` and `Vercel Dark`. It is MIT licensed. Several fields
are explicitly `null`; Zed's refinement pipeline completes those fields from
its appearance-specific defaults.

At pinned Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `theme_settings::{deserialize_user_theme,refine_theme_family}` converts JSON
  content into complete runtime themes;
- `theme::{ThemeRegistry,GlobalTheme,ActiveTheme,SystemAppearance}` owns theme
  registration, activation, and access;
- `theme_settings::{adjust_ui_font_size,reset_ui_font_size}` models zoom as a
  transient global followed by `App::refresh_windows`;
- `Workspace::new` owns a `Context::observe_window_appearance` subscription and
  reloads the active theme after native changes.

## Decision

Bundle the upstream theme JSON and its MIT license under
`crates/void/assets/themes/`. The binary composition root parses it through
Zed's existing refinement path, validates that only the two expected variants
exist, registers them, and activates the one matching the system appearance.
`AppView` owns the appearance subscription for its window. Void does not expose
a theme picker, external theme loading, icon-theme selection, per-token
overrides, or settings-file observation.

Keep application scale and terminal font size separate:

- `ui` owns a 14 px default root scale, a 12–24 px transient adjustment range,
  zoom actions, `ComponentSize::{Xs,Sm,Md,Lg,Xl}`, and `scaled`, which converts
  baseline design pixels into rems;
- `AppView::render` applies the current scale with `Window::set_rem_size`;
- `void_terminal` owns an 8–32 px transient adjustment over each terminal's
  configured font size;
- key-context-specific terminal shortcuts override application zoom while a
  terminal is focused;
- reset actions restore configured defaults and keyboard zoom is not persisted.

One-pixel separators, native title-bar coordinates, terminal cell painting,
and pointer hit geometry remain in pixels. Ordinary control dimensions,
spacing, icons, dialogs, tabs, and sidebar measurements use rems derived from
the 14 px design baseline.

Terminal colors are resolved from the active theme during layout, not copied
once when a terminal session starts. Existing sessions therefore follow native
appearance changes and recompute cell and PTY geometry after font zoom.

## Consequences

- System appearance is the sole theme mode until a settings surface is
  explicitly requested. Light and dark are the only registered Void themes.
- The upstream JSON is authoritative for explicit fields; null fields inherit
  pinned Zed defaults and may change only when the pinned Zed revision changes.
- UI zoom scales rem-based application chrome but intentionally does not alter
  native or pixel-snapped geometry.
- Terminal zoom is global and transient, but preserves each terminal's
  configured base size.
- A future persistent appearance setting can add `System | Light | Dark`, UI
  scale, and terminal font size without changing runtime ownership.

## References

- Theme source: <https://github.com/NathanBrodin/zed-vercel-theme/blob/main/themes/vercel-theme.json>
- Theme license: <https://github.com/NathanBrodin/zed-vercel-theme/blob/main/LICENSE>
- Zed `crates/theme/src/theme.rs::{GlobalTheme,ActiveTheme,SystemAppearance}`
- Zed `crates/theme/src/registry.rs::ThemeRegistry`
- Zed `crates/theme_settings/src/theme_settings.rs::{deserialize_user_theme,refine_theme_family,reload_theme}`
- Zed `crates/theme_settings/src/settings.rs::{setup_ui_font,adjust_ui_font_size,reset_ui_font_size}`
- Zed `crates/workspace/src/workspace.rs::Workspace::new`
- GPUI `Window::{set_rem_size,observe_window_appearance}`
