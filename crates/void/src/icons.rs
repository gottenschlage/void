use gpui::{Svg, prelude::*, px, rgb, svg};

use crate::theme;

pub(crate) fn icon(path: &'static str) -> Svg {
    icon_sized(path, 16., theme::TEXT_MUTED)
}

pub(crate) fn icon_sized(path: &'static str, size: f32, color: u32) -> Svg {
    svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(rgb(color))
}
