use gpui::{App, Hsla, Svg, prelude::*, px, svg};
use theme::ActiveTheme as _;

pub fn icon(path: &'static str, cx: &App) -> Svg {
    icon_sized(path, 16., cx.theme().colors().text_muted)
}

pub fn icon_sized(path: &'static str, size: f32, color: impl Into<Hsla>) -> Svg {
    svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(color.into())
}
