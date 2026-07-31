use gpui::{Context, Entity, Render, Subscription, Window, div, prelude::*, px};
use theme::ActiveTheme;

use crate::{Branch, git::RepositoryLiveDiff};

const HEADER_HEIGHT: f32 = 37.5;

pub(crate) struct BranchContextHeader {
    branch: Branch,
    live_diff: Entity<RepositoryLiveDiff>,
    _live_diff_subscription: Subscription,
}

impl BranchContextHeader {
    pub(crate) fn new(
        branch: Branch,
        live_diff: Entity<RepositoryLiveDiff>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&live_diff, |_, _, cx| cx.notify());
        Self {
            branch,
            live_diff,
            _live_diff_subscription: subscription,
        }
    }
}

impl Render for BranchContextHeader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stat = self.live_diff.read(cx).stat(self.branch.id);
        div()
            .flex()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .items_center()
            .justify_between()
            .px_4()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .child(
                        div()
                            .text_color(cx.theme().colors().text_muted)
                            .child(format!("#{} {}/", self.branch.number, self.branch.base_ref)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_color(cx.theme().colors().text)
                            .child(self.branch.name.clone()),
                    ),
            )
            .when_some(stat, |header, stat| {
                header.child(
                    div()
                        .flex()
                        .flex_none()
                        .gap_2()
                        .child(
                            div()
                                .text_color(cx.theme().colors().version_control_added)
                                .child(format!("+{}", stat.added)),
                        )
                        .child(
                            div()
                                .text_color(cx.theme().colors().version_control_deleted)
                                .child(format!("-{}", stat.deleted)),
                        ),
                )
            })
    }
}
