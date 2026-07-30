use gpui::{
    Anchor, Context, EventEmitter, Focusable, MouseButton, MouseDownEvent, MouseUpEvent, Role,
    Window, actions, anchored, deferred, div, prelude::*, px, rgb,
};
use workspace::{
    Branch, NewBranch, Repository, VoidPaths, WorkspaceDb, create_git_worktree, local_git_branches,
    validate_git_branch_request,
};

use crate::{icons::icon, text_input::TextInput, theme};

actions!(branch_dialog, [ConfirmBranch, CancelBranch]);

pub(crate) enum BranchDialogEvent {
    Dismissed,
    Created(Branch),
}

pub(crate) struct BranchDialog {
    repository: Repository,
    branch_position: i64,
    name_input: gpui::Entity<TextInput>,
    base_branches: Vec<String>,
    selected_base: Option<String>,
    base_menu_open: bool,
    is_loading_branches: bool,
    is_creating: bool,
    focus_name_input: bool,
    error: Option<String>,
}

impl EventEmitter<BranchDialogEvent> for BranchDialog {}

impl BranchDialog {
    pub(crate) fn new(
        repository: Repository,
        branch_position: i64,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| {
            let mut input = TextInput::new("Branch name", cx);
            input.set_text(generated_branch_name(), cx);
            input
        });
        let repository_path = repository.path.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { local_git_branches(&repository_path) })
                .await;
            this.update(cx, |this, cx| {
                this.is_loading_branches = false;
                match result {
                    Ok(branches) => {
                        this.selected_base = branches.first().cloned();
                        this.base_branches = branches;
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            })
        })
        .detach();

        Self {
            repository,
            branch_position,
            name_input,
            base_branches: Vec::new(),
            selected_base: None,
            base_menu_open: false,
            is_loading_branches: true,
            is_creating: false,
            focus_name_input: true,
            error: None,
        }
    }

    fn click_dismiss(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(cx);
    }

    fn cancel(&mut self, _: &CancelBranch, _: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(cx);
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.is_creating {
            cx.emit(BranchDialogEvent::Dismissed);
        }
    }

    fn toggle_base_menu(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.is_loading_branches && !self.base_branches.is_empty() {
            self.base_menu_open = !self.base_menu_open;
            cx.notify();
        }
    }

    fn dismiss_base_menu(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.base_menu_open = false;
        cx.notify();
    }

    fn regenerate_name(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.name_input
            .update(cx, |input, cx| input.set_text(generated_branch_name(), cx));
    }

    fn click_create(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.create_branch(cx);
    }

    fn confirm(&mut self, _: &ConfirmBranch, _: &mut Window, cx: &mut Context<Self>) {
        self.create_branch(cx);
    }

    fn create_branch(&mut self, cx: &mut Context<Self>) {
        if self.is_creating {
            return;
        }
        let name = self.name_input.read(cx).text().trim().to_owned();
        if name.is_empty() {
            self.error = Some("Enter a branch name".into());
            cx.notify();
            return;
        }
        let Some(base_ref) = self.selected_base.clone() else {
            self.error = Some("Select a base branch".into());
            cx.notify();
            return;
        };

        self.is_creating = true;
        self.error = None;
        self.base_menu_open = false;
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        let paths = cx.global::<VoidPaths>().clone();
        let repository = self.repository.clone();
        let position = self.branch_position;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result: Result<Branch, String> = async {
                let repository_path = repository.path.clone();
                let requested_name = name.clone();
                let validation_base = base_ref.clone();
                executor
                    .spawn(async move {
                        validate_git_branch_request(
                            &repository_path,
                            &requested_name,
                            &validation_base,
                        )
                    })
                    .await
                    .map_err(|error| error.to_string())?;

                let branch = database
                    .reserve_branch(
                        NewBranch {
                            repository_id: repository.id,
                            requested_name: name,
                            base_ref,
                            position,
                            is_pinned: false,
                        },
                        &paths,
                    )
                    .await
                    .map_err(|error| format!("Could not reserve branch: {error:#}"))?;

                let repository_path = repository.path.clone();
                let branch_name = branch.name.clone();
                let worktree_path = branch.path.clone();
                let base_ref = branch.base_ref.clone();
                if let Err(error) = executor
                    .spawn(async move {
                        create_git_worktree(
                            &repository_path,
                            &branch_name,
                            &worktree_path,
                            &base_ref,
                        )
                    })
                    .await
                {
                    let archive_result = database.archive_branch(branch.id).await;
                    return Err(match archive_result {
                        Ok(()) => error.to_string(),
                        Err(archive_error) => format!(
                            "{error}; also failed to archive the reservation: {archive_error:#}"
                        ),
                    });
                }

                Ok(branch)
            }
            .await;

            this.update(cx, |this, cx| {
                this.is_creating = false;
                match result {
                    Ok(branch) => cx.emit(BranchDialogEvent::Created(branch)),
                    Err(error) => {
                        this.error = Some(error);
                        cx.notify();
                    }
                }
            })
        })
        .detach();
    }

    fn render_base_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        deferred(
            anchored().anchor(Anchor::BottomLeft).child(
                div()
                    .id("base-branches")
                    .w_full()
                    .max_h(px(220.))
                    .overflow_y_scroll()
                    .p_1()
                    .rounded_md()
                    .bg(rgb(theme::ELEVATED_SURFACE))
                    .border_1()
                    .border_color(rgb(theme::BORDER))
                    .shadow_md()
                    .on_mouse_down_out(cx.listener(Self::dismiss_base_menu))
                    .children(self.base_branches.iter().cloned().enumerate().map(
                        |(index, branch)| {
                            let selected_branch = branch.clone();
                            div()
                                .id(("base-branch", index))
                                .h(px(28.))
                                .flex()
                                .items_center()
                                .px_2()
                                .rounded_sm()
                                .text_sm()
                                .cursor_pointer()
                                .hover(|row| row.bg(rgb(theme::ELEMENT_HOVER)))
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.selected_base = Some(selected_branch.clone());
                                        this.base_menu_open = false;
                                        cx.notify();
                                    }),
                                )
                                .child(branch)
                        },
                    )),
            ),
        )
        .with_priority(3)
    }
}

impl gpui::Render for BranchDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_name_input {
            self.focus_name_input = false;
            let focus_handle = self.name_input.focus_handle(cx);
            window.defer(cx, move |window, cx| window.focus(&focus_handle, cx));
        }

        div()
            .key_context("BranchDialog")
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .flex()
            .flex_col()
            .w(px(480.))
            .gap_5()
            .p_5()
            .rounded_lg()
            .bg(rgb(theme::ELEVATED_SURFACE))
            .border_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::TEXT))
            .text_sm()
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_lg().child("New branch"))
                    .child(
                        div()
                            .id("close-branch-dialog")
                            .flex()
                            .size(px(24.))
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|button| button.bg(rgb(theme::ELEMENT_HOVER)))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::click_dismiss))
                            .child(icon("icons/x.svg")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child("Base branch"),
                    )
                    .child(
                        div()
                            .id("base-branch-menu")
                            .relative()
                            .flex()
                            .h(px(32.))
                            .items_center()
                            .justify_between()
                            .px_2()
                            .rounded_sm()
                            .bg(rgb(theme::ELEMENT))
                            .border_1()
                            .border_color(rgb(theme::BORDER))
                            .when(
                                !self.is_loading_branches && !self.base_branches.is_empty(),
                                |menu| {
                                    menu.cursor_pointer().on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(Self::toggle_base_menu),
                                    )
                                },
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(icon("icons/git-branch.svg"))
                                    .child(if self.is_loading_branches {
                                        "Loading branches…".to_owned()
                                    } else {
                                        self.selected_base
                                            .clone()
                                            .unwrap_or_else(|| "No local branches".into())
                                    }),
                            )
                            .child(icon("icons/chevrons-up-down.svg"))
                            .when(self.base_menu_open, |menu| {
                                menu.child(self.render_base_menu(cx))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child("Branch name"),
                    )
                    .child(
                        div().relative().child(self.name_input.clone()).child(
                            div()
                                .id("regenerate-branch-name")
                                .absolute()
                                .right_1()
                                .top(px(4.))
                                .flex()
                                .size(px(24.))
                                .items_center()
                                .justify_center()
                                .rounded_sm()
                                .cursor_pointer()
                                .hover(|button| button.bg(rgb(theme::ELEMENT_HOVER)))
                                .on_mouse_up(MouseButton::Left, cx.listener(Self::regenerate_name))
                                .child(icon("icons/dices.svg")),
                        ),
                    ),
            )
            .when_some(self.error.clone(), |dialog, error| {
                dialog.child(div().text_xs().text_color(rgb(theme::ERROR)).child(error))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        div()
                            .id("cancel-branch")
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label("Cancel")
                            .flex()
                            .h(px(32.))
                            .items_center()
                            .px_3()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(theme::BORDER))
                            .text_sm()
                            .when(!self.is_creating, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|button| button.bg(rgb(theme::ELEMENT_HOVER)))
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(Self::click_dismiss),
                                    )
                            })
                            .child("Cancel"),
                    )
                    .child(
                        div()
                            .id("create-branch")
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label("Create branch")
                            .flex()
                            .h(px(32.))
                            .items_center()
                            .px_3()
                            .rounded_sm()
                            .bg(rgb(theme::ACCENT))
                            .text_sm()
                            .text_color(rgb(theme::EDITOR_BACKGROUND))
                            .when(!self.is_creating, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|button| button.opacity(0.9))
                                    .on_mouse_up(MouseButton::Left, cx.listener(Self::click_create))
                            })
                            .child(if self.is_creating {
                                "Creating branch…"
                            } else {
                                "Create branch"
                            }),
                    ),
            )
    }
}

fn generated_branch_name() -> String {
    petname::petname(2, "-").unwrap_or_else(|| "calm-raven".to_owned())
}
