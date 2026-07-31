# ADR 0007: Fix the drag-and-drop accent color and add auto-scroll near edges

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

Two problems surfaced in the sidebar's repository/branch reordering and the
branch tab bar's reordering:

1. The drag-over indicator border used `cx.theme().colors().text_accent`
   (`crates/void/src/theme.rs`), but `text_accent` had never actually been
   differentiated from `text` — both were `rgb(0xededed)`, near-white. Against
   the near-black backgrounds, the 2px drag-over border rendered as a stark
   white line, which read as harsh/unpolished next to the terminal tab bar's
   own drag indicator, which hardcodes a blue (`rgb(0x3794ff)`,
   `crates/void_terminal/src/panel.rs`).
2. Reordering an item to the very top/start of a list only works if the drop
   target is currently visible — none of the three scrollable drag surfaces
   (sidebar's repository list, the branch tab bar, the terminal tab bar)
   scrolled themselves while a drag was in progress, so an item scrolled out
   of view could never be reached as a drop target.

Zed's own drag implementations (`crates/workspace/src/pane.rs`) don't solve
auto-scroll-during-drag either — there is no Zed reference to port for this
specific behavior, only the general `on_drag_move` mechanism its tab bar uses
for a different purpose (pane-split direction).

## Decision

**Accent color**: give `text_accent` its own real value, `rgb(0x3794ff)`
(the same blue the terminal tab bar already used), instead of reusing
`text`'s value. This is a one-line change in `theme.rs`; every call site that
reads `text_accent` (drag-over borders in `sidebar.rs`/`branch_header.rs`,
primary-button backgrounds in `branch_dialog.rs`/`application.rs`) picks it up
automatically, so the primary "Create" buttons now render as a filled blue
button instead of white, consistent with the drag indicator.

**Auto-scroll near edges**: added `ui::auto_scroll_toward_edge(handle, cursor,
bounds)` (`crates/ui/src/auto_scroll.rs`) — a small pure function, not a new
trait or generic drag abstraction. It nudges a `ScrollHandle`'s offset toward
whichever edge of the container the cursor is within 48px of, by a fixed
16px step per call; the axis a container doesn't scroll on is a no-op since
its `max_offset` is zero, so the same function works for the sidebar's
vertical list and both tab bars' horizontal lists without an axis parameter.

Each of the three scrollable containers (`workspace/src/view/sidebar`'s `repository-list`,
`workspace/src/view/branches/tabs.rs`'s `branch-headers`, `void_terminal`'s `terminal-tabs`) now
tracks a `ScrollHandle` field and calls this function from an
`on_drag_move::<T>` handler registered per draggable payload type, calling
`cx.notify()` afterward so the scrolled position repaints.

`void_terminal` gained a dependency on `crates/ui` for this one function —
still one-directional (`ui` has no dependency on `void_terminal` or any
domain crate), consistent with ADR 0006.

## Consequences

- The drag-over indicator and primary-button accent are now one real color,
  sourced from the theme instead of duplicated as a hardcoded terminal
  constant and an accidental near-white theme value.
- Dragging a row/tab toward the top or start of a scrolled-off list now
  reaches it, since the container scrolls itself while the drag is held near
  its edge.
- `auto_scroll_toward_edge` fires once per `on_drag_move` event (which GPUI
  dispatches on mouse movement during a drag), not on a fixed timer — holding
  the cursor perfectly still at the edge without any further movement won't
  keep scrolling. This matches the natural drag gesture (the cursor is rarely
  perfectly still) and avoids adding timer/task lifecycle management for a
  case with no Zed precedent to model it on.
- `void_terminal`'s tab bar still doesn't use the `theme` crate anywhere else
  (its other colors remain hardcoded `rgb(...)`); only the auto-scroll helper
  was adopted from `ui`. Threading `theme` through `void_terminal` broadly is
  a larger, separate change, deliberately out of scope here.

## References

- `crates/ui/src/auto_scroll.rs`, `crates/void/src/theme.rs`
- `crates/workspace/src/view/sidebar/mod.rs` (`repository_list_scroll`, `scroll_toward_drag`)
- `crates/workspace/src/view/branches/tabs.rs` (`tabs_scroll`, `scroll_toward_drag`)
- `crates/void_terminal/src/panel.rs` (`tabs_scroll`, `scroll_toward_drag`)
- GPUI `ScrollHandle`/`DragMoveEvent`: `gpui/src/elements/div.rs`
