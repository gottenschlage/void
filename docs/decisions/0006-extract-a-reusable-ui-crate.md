# ADR 0006: Extract `crates/ui` for reusable, domain-agnostic UI primitives

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

`crates/void/src` had accumulated both reusable UI code (`text_input.rs`,
`icons.rs`) and screen-specific components (`sidebar.rs`, `branch_header.rs`,
`branch_dialog.rs`) as flat modules in the binary crate. The latter three also
duplicated the same visual shapes repeatedly: a ~5-line interactive-row style
(fixed height, horizontal padding, rounded corners, text size) appeared 5
times across `sidebar.rs` and `branch_dialog.rs`, and an elevated
bordered-surface style (background, border, shadow) appeared 3 times across a
dropdown menu, a base-branch picker, and the branch-creation dialog panel.

Zed's own `crates/zed/src` holds almost no UI code — `main.rs`, `zed.rs`
(app wiring), `reliability`, `visual_test_runner`. Every screen surface
(`title_bar`, `project_panel`, `activity_indicator`, ...) is its own
top-level crate, and Zed's `ui` crate (`crates/ui/Cargo.toml`) depends only
on `gpui`, `theme`, `icons`, and generic utilities — never on `workspace`,
`project`, or any domain crate. ~80 feature crates depend on `ui`, one
direction only.

## Considered options

### Mirror Zed exactly: a crate per screen

`crates/sidebar`, `crates/branch_header`, `crates/branch_dialog`, each its own
crate depending on `ui` and `workspace`. Matches Zed's convention precisely,
but for three modest files this is more Cargo.toml boilerplate and compile
units than the current product size justifies — Zed's crate-per-feature
split earns its cost at ~150 crates and many contributors; Void is not there.

### One `ui` crate with `components/` (reusable) and `blocks/` (screens) folders inside it

Keeps everything in one crate, but would make `ui` depend on `workspace` (for
`Branch`/`Repository` types used by the screen components), which erases the
one property worth keeping from Zed's split: a UI-primitive layer with zero
product knowledge.

### One `ui` crate for primitives only; screens stay in `crates/void/src`

Matches the property that actually matters — reusable code has no domain
knowledge, product code does — without paying for per-screen crates Void
doesn't need yet.

## Decision

Take the third option. New crate `crates/ui` (`Cargo.toml` depends only on
`gpui` and `theme`, mirroring Zed's own `ui` crate exactly), with flat files
at its root — no `components/`/`blocks/` subfolders, since a handful of files
doesn't need them (Zed itself keeps small primitives like `avatar.rs` or
`divider.rs` flat in `ui/src/components/`, only reaching for a subfolder like
`button/` once a component splits into several files).

Moved as-is: `text_input.rs`, `icons.rs` → `icon.rs` (visibility changed from
`pub(crate)` to `pub`; the `actions!` namespace renamed from
`workspace_name_input` to `text_input` since it's no longer binary-specific).

New, extracted from real duplication:
- `row.rs` — `ListRow` extension trait, `.list_row(height)`, providing the
  shared five-property row shape. Deliberately does *not* include
  `cursor_pointer()`: the `workspace-repository` row has no pointer cursor of
  its own (only its inner pin/archive buttons do), so baking it in would have
  silently changed that row's behavior. Callers add `cursor_pointer()`
  themselves when a row is directly clickable.
- `popover.rs` — `popover(cx) -> Div`, the elevated bordered surface for
  anchored dropdowns (`rounded_md`/`shadow_md`).
- `dialog.rs` — `dialog(cx) -> Div`, the same surface at `rounded_lg`/
  `shadow_lg` for standalone modal panels.

`crates/void/src/{sidebar,branch_header,branch_dialog}.rs` stay exactly where
they are — each depends on `workspace::{Branch, Repository, ...}` — and were
updated to build on `ui::{ListRow, popover, dialog, icon, TextInput}` instead
of repeating the chrome inline.

## Consequences

- `crates/void`'s five duplicated row blocks and three duplicated
  elevated-surface blocks are now one call each to `.list_row(_)`,
  `popover(cx)`, or `dialog(cx)`.
- `ui` has zero dependency on `workspace` or any Void domain type, so it can
  be reused by any future screen crate without pulling in product code —
  same property Zed's `ui` crate has, at a much smaller scale.
- Void does not yet have a crate per screen. If `sidebar.rs`,
  `branch_header.rs`, or `branch_dialog.rs` grow substantially, splitting
  each into its own crate (depending on `ui` + `workspace`) remains the clear
  next step and was deliberately deferred, not ruled out.
- `drag_preview()` (`sidebar.rs`) and `DraggedBranch::render()`
  (`branch_header.rs`) were left untouched even though they resemble
  `popover`'s shape — neither has rounded corners in the original code, and
  changing that would be a visual change disguised as a refactor.

## References

- Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`
- Zed `crates/ui/Cargo.toml`, `crates/ui/src/ui.rs`
- Zed `crates/ui/src/components/{avatar.rs,divider.rs,button/}` (flat file vs.
  subfolder threshold)
- Zed `crates/zed/src/` (thin binary crate; feature crates hold all UI)
- Zed `crates/theme/Cargo.toml::[lib] path = "src/theme.rs"` (crate-root file
  named after the crate, not `lib.rs` — same pattern used for `ui/src/ui.rs`)

## Implementation update

ADR 0011 later moved the product screens from the binary into `workspace`, preserving this decision's dependency direction. ADR 0014 split the low-level text-input element from its entity/input state and added `unicode-segmentation` for correct extended-grapheme movement. ADR 0017 restored semantic Zed theme tokens for the bundled Vercel variants and added proportional sizing; `ui` therefore remains domain-independent and depends only on `gpui`, `theme`, and `unicode-segmentation`.
