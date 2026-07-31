# ADR 0016: Use GPUI's default colors

- **Status:** Superseded by [ADR 0017](0017-bundle-vercel-appearance-and-shared-scale.md)
- **Date:** 2026-08-01
- **Decision owners:** Void maintainers

## Context

Void previously refined Zed's bundled One Dark theme into a Vercel-inspired
palette. The product now requires GPUI's built-in default theme instead, with
minimal application-owned theming code.

At pinned Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`,
`gpui::Colors::default()` returns `Colors::light()`. `App::init_colors` stores
that palette globally, and `gpui::colors::DefaultColors` exposes it through
`cx.default_colors()`. The palette has eight base tokens and no syntax,
terminal, status, Git, or other product-specific semantic colors.

The pinned `terminal` crate independently calls `cx.theme()` when resolving
indexed colors requested by a terminal program. Removing Zed's base theme
initialization would therefore leave a reachable runtime panic even though
Void's terminal painter otherwise owns its colors.

## Decision

Initialize GPUI colors with `App::init_colors` and use the eight built-in
tokens directly in all Void render code. Remove Void's custom theme module,
its refinements, and UI-crate dependencies on Zed's `theme` crate. Do not add a
replacement wrapper or semantic palette.

Keep `theme::init(LoadThemes::JustBase, cx)` at application startup solely for
the pinned terminal backend. No Void render path reads that theme.

## Consequences

- The visible application uses GPUI's default light palette.
- GPUI's `container` token covers interactive surfaces and hover states. Its
  `selected` token covers primary controls and semantic emphasis for which the
  base palette has no dedicated color; `selected_text` is used on selected
  control backgrounds.
- Error and Git added/deleted states are no longer color-distinct because GPUI
  does not provide those semantic tokens.
- Switching automatically with system appearance is not included:
  `App::init_colors` installs `Colors::default()`, which is light at this
  revision. `Colors::for_appearance` exists but is not the default initializer.
- Zed's base theme remains an internal terminal compatibility dependency.

## References

- GPUI `crates/gpui/src/app.rs::App::init_colors`
- GPUI `crates/gpui/src/colors.rs::{Colors, GlobalColors, DefaultColors}`
- Zed `crates/terminal/src/terminal.rs::{Terminal::process_internal_event,get_color_at_index}`
- Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`
- <https://docs.rs/gpui/latest/gpui/colors/index.html>
