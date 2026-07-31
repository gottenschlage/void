use gpui::{App, Div, Styled, div};
use theme::ActiveTheme;

/// The elevated, bordered surface shared by dropdown menus: a workspace
/// switcher, a base-branch picker, or any other anchored, dismissable list.
pub fn popover(cx: &App) -> Div {
    div()
        .rounded_md()
        .bg(cx.theme().colors().elevated_surface_background)
        .border_1()
        .border_color(cx.theme().colors().border)
        .shadow_md()
}
