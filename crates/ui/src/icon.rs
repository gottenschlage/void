use gpui::{App, Hsla, Svg, prelude::*, svg};
use theme::ActiveTheme as _;

use crate::scaled;

pub fn icon(path: &'static str, cx: &App) -> Svg {
    icon_sized(path, 16., cx.theme().colors().text_muted)
}

pub fn icon_sized(path: &'static str, size: f32, color: impl Into<Hsla>) -> Svg {
    svg()
        .path(path)
        .size(scaled(size))
        .flex_none()
        .text_color(color.into())
}
