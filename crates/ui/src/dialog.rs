use gpui::{App, Div, Styled, div};
use theme::ActiveTheme;

/// The elevated, bordered panel shared by modal dialogs, e.g. the new-branch
/// dialog. Larger radius and shadow than [`crate::popover`], since a dialog
/// is a standalone panel rather than an anchored dropdown.
pub fn dialog(cx: &App) -> Div {
    div()
        .rounded_lg()
        .bg(cx.theme().colors().elevated_surface_background)
        .border_1()
        .border_color(cx.theme().colors().border)
        .shadow_lg()
}
