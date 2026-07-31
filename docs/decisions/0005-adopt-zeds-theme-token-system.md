# ADR 0005: Adopt Zed's `theme` token system instead of hardcoded colors

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

Void's UI read colors from `crate::theme`, a hand-rolled module of `u32` hex
constants (`BORDER`, `SURFACE`, `TEXT`, ...) fed into `gpui::rgb(...)` at every
call site. The pinned GPUI revision's `theme` and `settings` crates were
already declared workspace dependencies but never used — `::theme::init` ran
at startup (`application.rs`), installing Zed's real `GlobalTheme`, yet no
render code ever read it. The maintainer asked to follow Zed's real theme
architecture (as configured in their own `~/.config/zed/settings.json`, which
scopes `ui_font_size`, `buffer_font_size`, and `terminal.font_size`
independently) rather than one hardcoded constant module.

Zed's real system, inspected at the pinned commit in
`/Users/usama/Documents/archive/zed`, has three layers:

1. `theme::Theme` / `theme::ThemeColors` (`crates/theme/src/theme.rs`,
   `crates/theme/src/styles/colors.rs`) — a ~150-field struct plus
   `theme::ActiveTheme` (`cx.theme()`).
2. `theme::ThemeRegistry` — loads/holds named themes; `theme::init` registers
   Zed's bundled "One Dark" as the default `GlobalTheme`.
3. `theme_settings` (`crates/theme_settings/`, 2,692 lines) — the concrete,
   settings-file-backed implementation covering UI/buffer/agent/markdown/git-
   commit font sizes and JSON theme-file import (`ThemeColorsRefinement`
   deserialization). This layer is Zed-product-specific; Void has none of
   agent-panel, markdown-preview, or git-commit-box surfaces it configures.

## Considered options

### Pull in the full `theme_settings` crate

Gives Void JSON theme files and `settings.json`-driven font sizing identical
to Zed's. Rejected: it drags in ~2,700 lines of settings surface for features
Void does not have (agent/markdown/git-commit font sizes, theme-extension
JSON schema), which is unrelated scope for this change.

### Keep hardcoded hex constants, just retune the values

Cheapest, but leaves `theme::init`'s already-running `GlobalTheme` unused and
keeps every color as an untyped `u32` disconnected from Zed's real
`ThemeColors`/`ActiveTheme` plumbing this codebase already depends on.

### Use `theme::Theme`/`ThemeRegistry`/`ActiveTheme` directly, refining Zed's bundled default

Uses the real, typed theme system Void already vendors, without importing
settings-file parsing or JSON theme loading Void doesn't need yet.

## Decision

Take the narrow option. `crates/void/src/theme.rs::init` fetches Zed's bundled
`DEFAULT_DARK_THEME` ("One Dark") from `theme::ThemeRegistry::default_global`
and calls `ThemeColors::refined` (via the `refineable` crate, already a
transitive dependency of `theme`, now added directly) with a
`ThemeColorsRefinement` overriding only the tokens Void's UI renders:
surfaces, borders, elements, and text. Everything else (syntax, status, vim,
diff-hunk, and terminal-ANSI colors) stays Zed's verified default until Void
has a concrete use for it. `application.rs::run` calls this `theme::init(cx)`
right after `::theme::init(...)`, replacing the default `GlobalTheme` with
Void's refined one via `GlobalTheme::update_theme`.

Every render call site (`workspace/src/view/sidebar`, `workspace/src/view/branches`,
`ui/src/text_input/`, `void_update/src/status_view.rs`, `ui/src/icon.rs`, `void/src/application.rs`) now reads
`cx.theme().colors().<field>` / `cx.theme().status().<field>` through
`theme::ActiveTheme`, instead of `rgb(theme::CONSTANT)`. `icon()` and
`drag_preview()` gained a `cx: &App` parameter to reach the active theme; call
sites were updated accordingly. `UI_FONT` / `UI_FONT_SIZE` remain plain
constants in `crate::theme` — they are not part of `theme::Theme` (Zed itself
sources fonts from `theme_settings`, which this decision explicitly excludes)
— `UI_FONT_SIZE` was corrected from 15.0 to 13.0 to match the maintainer's own
Zed `ui_font_size`.

## Consequences

- Void's palette is a typed `theme::Theme` value, reachable anywhere via
  `cx.theme()`, instead of a parallel hand-rolled constant module.
- Adding `refineable` (pinned to the same Zed commit) as a direct dependency;
  no other new dependencies.
- Buffer/terminal font size remains unscoped from UI font size for now —
  `void_terminal::TerminalSettings` still hardcodes its own font family/size
  and was not touched by this change (out of scope; tracked as follow-up).
- Void has no theme picker, no JSON theme import, and no settings-file-driven
  font sizing. Changing Void's palette still means editing
  `crates/void/src/theme.rs::init`'s refinement literal, not a user-facing
  settings file. Adopting `theme_settings` for that remains a future decision
  if Void ever wants user-configurable themes/fonts.
- `border_focused` now reuses `theme::ThemeColors::border_transparent` rather
  than a visible ring color, matching the reference Vercel Dark theme's own
  `border.focused: transparent` (focus is communicated by background change,
  not a border).

## References

- Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`
- Zed `crates/theme/src/theme.rs::{init, ActiveTheme, GlobalTheme, Theme}`
- Zed `crates/theme/src/styles/colors.rs::ThemeColors`
- Zed `crates/theme/src/fallback_themes.rs::zed_default_dark`
- Zed `crates/theme/src/registry.rs::ThemeRegistry`
- Zed `crates/refineable/src/refineable.rs::Refineable`
- Zed `crates/theme_settings/src/{theme_settings.rs,settings.rs,schema.rs}`
  (inspected, not adopted — see "Considered options")
- <https://gpui.rs/>
