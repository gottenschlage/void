use gpui::{Bounds, Pixels, Point, ScrollHandle, px};

const EDGE: Pixels = px(48.);
const STEP: Pixels = px(16.);

/// Nudges `handle`'s scroll offset toward whichever edge of `bounds` the
/// cursor is near. Call this from `on_drag_move` on a scrollable container so
/// a drop target that has scrolled out of view can still be reached mid-drag.
///
/// Works for either scroll axis: the axis a container doesn't scroll on is a
/// no-op, since its `max_offset` is zero.
pub fn auto_scroll_toward_edge(
    handle: &ScrollHandle,
    cursor: Point<Pixels>,
    bounds: Bounds<Pixels>,
) {
    let max = handle.max_offset();
    let mut offset = handle.offset();

    offset.y = scrolled(
        offset.y,
        max.y,
        cursor.y - bounds.top(),
        bounds.bottom() - cursor.y,
    );
    offset.x = scrolled(
        offset.x,
        max.x,
        cursor.x - bounds.left(),
        bounds.right() - cursor.x,
    );

    handle.set_offset(offset);
}

/// `offset` ranges from `0` (start) to `-max` (end); nudges it by `STEP`
/// toward whichever side the cursor is within `EDGE` of, clamped to range.
fn scrolled(
    offset: Pixels,
    max: Pixels,
    distance_from_start: Pixels,
    distance_from_end: Pixels,
) -> Pixels {
    if distance_from_start < EDGE && offset < px(0.) {
        (offset + STEP).min(px(0.))
    } else if distance_from_end < EDGE && offset > -max {
        (offset - STEP).max(-max)
    } else {
        offset
    }
}
