//! Rendering and interactions for the updater's compact status surface.

use gpui::{Context, IntoElement, MouseButton, Render, Window, div, prelude::*, px};
use theme::ActiveTheme;

use crate::updater::{UpdateStatus, Updater};

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusAction {
    Restart,
    Retry,
}

impl Render for Updater {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (label, action) = match &self.status {
            UpdateStatus::Downloading { version, progress } => (
                progress.map_or_else(
                    || format!("Downloading Void {version}…"),
                    |value| format!("Downloading Void {version}… {:.0}%", value * 100.0),
                ),
                None,
            ),
            UpdateStatus::Installing { version } => (format!("Installing Void {version}…"), None),
            UpdateStatus::Ready { version } => (
                format!("Restart to update to {version}"),
                Some(StatusAction::Restart),
            ),
            UpdateStatus::Errored { message } => (
                format!("Update failed: {message} — Retry"),
                Some(StatusAction::Retry),
            ),
            UpdateStatus::Disabled | UpdateStatus::Idle | UpdateStatus::Checking => {
                (String::new(), None)
            }
        };
        if label.is_empty() {
            return div().into_any_element();
        }
        div()
            .id("void-update-status")
            .absolute()
            .right(px(12.))
            .bottom(px(12.))
            .max_w(px(520.))
            .rounded_sm()
            .bg(cx.theme().colors().surface_background)
            .border_1()
            .border_color(cx.theme().colors().border)
            .px_3()
            .py_2()
            .text_xs()
            .when(action == Some(StatusAction::Restart), |button| {
                button
                    .cursor_pointer()
                    .hover(|button| button.bg(cx.theme().colors().element_hover))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::restart))
            })
            .when(action == Some(StatusAction::Retry), |button| {
                button
                    .cursor_pointer()
                    .hover(|button| button.bg(cx.theme().colors().element_hover))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::retry))
            })
            .child(label)
            .into_any_element()
    }
}
