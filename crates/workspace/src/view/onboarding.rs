use gpui::{Context, MouseButton, MouseUpEvent, Role, Window, actions, div, prelude::*, px};
use theme::ActiveTheme as _;

use super::{branches::BranchHeader, sidebar::Sidebar, workspace::WorkspaceView};
use crate::{Workspace, WorkspaceDb, WorkspaceModel};

actions!(workspace_onboarding, [Submit]);

impl WorkspaceView {
    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        self.create_workspace(window, cx);
    }

    fn click_create(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.create_workspace(window, cx);
    }

    fn create_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_model.is_some() || self.is_creating {
            return;
        }

        let name = self.name_input.read(cx).text().trim().to_owned();
        if name.is_empty() {
            self.error = Some("Enter a workspace name".into());
            cx.notify();
            return;
        }

        self.error = None;
        self.is_creating = true;
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        let this = cx.weak_entity();
        let task = window.spawn(cx, async move |cx| {
            let result = database.create_workspace(name.clone()).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.is_creating = false;
                match result {
                    Ok(id) => {
                        let workspace = Workspace { id, name };
                        let sidebar = cx.new(|_| Sidebar::new(workspace.clone(), Vec::new()));
                        let branch_header = cx.new(|_| BranchHeader::new(Vec::new()));
                        this.workspace_model =
                            Some(WorkspaceModel::new(workspace, Vec::new(), Vec::new()));
                        this.attach_sidebar(sidebar, window, cx);
                        this.attach_branch_header(branch_header, window, cx);
                        this.branch_panels.clear();
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not create workspace: {error:#}"));
                    }
                }
                cx.notify();
            });
        });
        self.create_workspace_task = Some(task);
    }

    pub(super) fn render_onboarding(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("WorkspaceOnboarding")
            .on_action(cx.listener(Self::submit))
            .flex()
            .flex_col()
            .w(px(400.))
            .gap_4()
            .p_6()
            .rounded_lg()
            .bg(cx.theme().colors().elevated_surface_background)
            .border_1()
            .border_color(cx.theme().colors().border)
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_lg().child("Create your workspace"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().colors().text_muted)
                            .child("Name the workspace you will use in Void."),
                    ),
            )
            .child(self.name_input.clone())
            .when_some(self.error.clone(), |view, error| {
                view.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().status().error)
                        .child(error),
                )
            })
            .child(
                div()
                    .id("create-workspace")
                    .focusable()
                    .tab_stop(true)
                    .role(Role::Button)
                    .aria_label("Create workspace")
                    .flex()
                    .h(px(32.))
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .bg(cx.theme().colors().text_accent)
                    .text_sm()
                    .text_color(cx.theme().colors().editor_background)
                    .when(!self.is_creating, |button| {
                        button
                            .cursor_pointer()
                            .hover(|button| button.opacity(0.9))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::click_create))
                    })
                    .child(if self.is_creating {
                        "Creating workspace…"
                    } else {
                        "Create workspace"
                    }),
            )
    }
}
