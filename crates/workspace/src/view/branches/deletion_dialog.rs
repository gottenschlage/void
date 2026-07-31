use crate::{
    Branch, GitDeleteMode, ManagedWorktreeError, Repository, VoidPaths, WorkspaceDb,
    WorktreeProvenanceCheck, delete_managed_branch, remove_managed_worktree,
    validate_managed_worktree,
};
use gpui::{
    Context, EventEmitter, Focusable, MouseButton, MouseUpEvent, PromptLevel, Role, Subscription,
    Task, Window, actions, div, prelude::*, px,
};
use theme::ActiveTheme;

use ui::{TextInput, dialog};

actions!(branch_deletion_dialog, [ConfirmDeletion, CancelDeletion]);

pub(crate) enum BranchDeletionDialogEvent {
    Dismissed,
    Started {
        branch_id: crate::BranchId,
        released: async_channel::Sender<()>,
    },
    ProvenanceRecorded {
        branch_id: crate::BranchId,
        provenance: crate::WorktreeProvenance,
    },
    Failed(crate::BranchId),
    PartiallyDeleted(crate::BranchId),
    Deleted(crate::BranchId),
}

pub(crate) struct BranchDeletionDialog {
    repository: Repository,
    branch: Branch,
    confirmation_input: gpui::Entity<TextInput>,
    _input_subscription: Subscription,
    deletion_task: Option<Task<()>>,
    is_deleting: bool,
    focus_input: bool,
    error: Option<String>,
}

impl EventEmitter<BranchDeletionDialogEvent> for BranchDeletionDialog {}

impl BranchDeletionDialog {
    pub(crate) fn new(repository: Repository, branch: Branch, cx: &mut Context<Self>) -> Self {
        let confirmation_input = cx.new(|cx| TextInput::new("Branch name", cx));
        let input_subscription = cx.observe(&confirmation_input, |_, _, cx| cx.notify());
        Self {
            repository,
            branch,
            confirmation_input,
            _input_subscription: input_subscription,
            deletion_task: None,
            is_deleting: false,
            focus_input: true,
            error: None,
        }
    }

    fn confirmation_matches(&self, cx: &gpui::App) -> bool {
        branch_confirmation_matches(&self.branch.name, self.confirmation_input.read(cx).text())
    }

    fn cancel(&mut self, _: &CancelDeletion, _: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(cx);
    }

    fn click_cancel(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(cx);
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.is_deleting {
            cx.emit(BranchDeletionDialogEvent::Dismissed);
        }
    }

    fn confirm(&mut self, _: &ConfirmDeletion, window: &mut Window, cx: &mut Context<Self>) {
        self.start_deletion(window, cx);
    }

    fn click_delete(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.start_deletion(window, cx);
    }

    fn start_deletion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_deleting || !self.confirmation_matches(cx) {
            return;
        }

        if self.branch.repository_id != self.repository.id {
            self.error = Some(
                "Refusing to delete: the branch does not belong to the selected repository".into(),
            );
            cx.notify();
            return;
        }

        let paths = cx.global::<VoidPaths>().clone();
        let expected_path = match paths.branch_worktree(&self.repository.name, &self.branch.name) {
            Ok(path) if path == self.branch.path => path,
            Ok(path) => {
                self.error = Some(format!(
                    "Refusing to delete: the recorded worktree path does not match {}",
                    path.display()
                ));
                cx.notify();
                return;
            }
            Err(error) => {
                self.error = Some(format!("Refusing to delete: {error:#}"));
                cx.notify();
                return;
            }
        };

        self.is_deleting = true;
        self.error = None;
        let branch_id = self.branch.id;
        let (released_tx, released_rx) = async_channel::bounded(1);
        cx.emit(BranchDeletionDialogEvent::Started {
            branch_id,
            released: released_tx,
        });
        cx.notify();

        let repository_path = self.repository.path.clone();
        let managed_worktrees_root = paths.worktrees();
        let branch_name = self.branch.name.clone();
        let recorded_provenance = self.branch.worktree_provenance.clone();
        let database = cx.global::<WorkspaceDb>().clone();
        let executor = cx.background_executor().clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            if released_rx.recv().await.is_err() {
                finish_with_error(
                    &this,
                    branch_id,
                    "Could not verify that branch resources were released".into(),
                    cx,
                );
                return;
            }

            let validation_repository = repository_path.clone();
            let validation_root = managed_worktrees_root.clone();
            let validation_path = expected_path.clone();
            let validation_branch = branch_name.clone();
            let validation_provenance = recorded_provenance.clone();
            let mut validated = match executor
                .spawn(async move {
                    validate_managed_worktree(
                        &validation_repository,
                        &validation_root,
                        &validation_path,
                        &validation_branch,
                        validation_provenance.as_ref().map_or(
                            WorktreeProvenanceCheck::CaptureCurrent,
                            WorktreeProvenanceCheck::Recorded,
                        ),
                    )
                })
                .await
            {
                Ok(validated) => validated,
                Err(error) => {
                    finish_with_error(&this, branch_id, error.to_string(), cx);
                    return;
                }
            };

            if recorded_provenance.is_none() {
                let provenance = validated.provenance().clone();
                if let Err(error) = database
                    .record_worktree_provenance(branch_id, provenance.clone())
                    .await
                {
                    finish_with_error(
                        &this,
                        branch_id,
                        format!("Could not adopt the legacy worktree: {error:#}"),
                        cx,
                    );
                    return;
                }

                let _ = this.update(cx, |dialog, cx| {
                    dialog.branch.worktree_provenance = Some(provenance.clone());
                    cx.emit(BranchDeletionDialogEvent::ProvenanceRecorded {
                        branch_id,
                        provenance: provenance.clone(),
                    });
                });

                let validation_repository = repository_path.clone();
                let validation_root = managed_worktrees_root.clone();
                let validation_path = expected_path.clone();
                let validation_branch = branch_name.clone();
                validated = match executor
                    .spawn(async move {
                        validate_managed_worktree(
                            &validation_repository,
                            &validation_root,
                            &validation_path,
                            &validation_branch,
                            WorktreeProvenanceCheck::Recorded(&provenance),
                        )
                    })
                    .await
                {
                    Ok(validated) => validated,
                    Err(error) => {
                        finish_with_error(&this, branch_id, error.to_string(), cx);
                        return;
                    }
                };
            }

            let remove_result = {
                let validated = validated.clone();
                executor
                    .spawn(async move { remove_managed_worktree(&validated, GitDeleteMode::Safe) })
                    .await
            };
            let removed = match remove_result {
                Ok(removed) => removed,
                Err(ManagedWorktreeError::DirtyWorktree) => {
                    let answer = match cx.update(|window, cx| {
                        window.prompt(
                            PromptLevel::Critical,
                            &format!(
                                "Worktree for {:?} contains modified or untracked files. Force delete it?",
                                branch_name
                            ),
                            Some("Force deletion permanently discards these files."),
                            &["Force Delete", "Cancel"],
                            cx,
                        )
                    }) {
                        Ok(answer) => answer,
                        Err(error) => {
                            finish_with_error(&this, branch_id, error.to_string(), cx);
                            return;
                        }
                    };
                    if answer.await != Ok(0) {
                        finish_canceled(&this, branch_id, cx);
                        return;
                    }

                    let validation_repository = repository_path.clone();
                    let validation_root = managed_worktrees_root.clone();
                    let validation_path = expected_path.clone();
                    let validation_branch = branch_name.clone();
                    let provenance = validated.provenance().clone();
                    let revalidated = match executor
                        .spawn(async move {
                            validate_managed_worktree(
                                &validation_repository,
                                &validation_root,
                                &validation_path,
                                &validation_branch,
                                WorktreeProvenanceCheck::Recorded(&provenance),
                            )
                        })
                        .await
                    {
                        Ok(validated) => validated,
                        Err(error) => {
                            finish_with_error(&this, branch_id, error.to_string(), cx);
                            return;
                        }
                    };
                    match executor
                        .spawn(async move { remove_managed_worktree(&revalidated, GitDeleteMode::Force) })
                        .await
                    {
                        Ok(removed) => removed,
                        Err(error) => {
                            finish_with_error(&this, branch_id, error.to_string(), cx);
                            return;
                        }
                    }
                }
                Err(error) => {
                    finish_with_error(&this, branch_id, error.to_string(), cx);
                    return;
                }
            };

            let removed_for_delete = removed.clone();
            let branch_result = executor
                .spawn(async move { delete_managed_branch(&removed_for_delete, GitDeleteMode::Safe) })
                .await;
            if let Err(error) = branch_result {
                if matches!(error, ManagedWorktreeError::UnmergedBranch { .. }) {
                    let answer = match cx.update(|window, cx| {
                        window.prompt(
                            PromptLevel::Critical,
                            &format!(
                                "Branch {:?} is not fully merged. Force delete it?",
                                branch_name
                            ),
                            Some("The worktree has been removed. Force deletion removes the remaining local branch."),
                            &["Force Delete", "Cancel"],
                            cx,
                        )
                    }) {
                        Ok(answer) => answer,
                        Err(prompt_error) => {
                            finish_partial_error(
                                &this,
                                branch_id,
                                format!(
                                    "The worktree was removed, but the branch was retained: {prompt_error}"
                                ),
                                cx,
                            );
                            return;
                        }
                    };
                    if answer.await != Ok(0) {
                        finish_partial_error(
                            &this,
                            branch_id,
                            "The worktree was removed, but the unmerged branch was retained. Confirm deletion again to finish.".into(),
                            cx,
                        );
                        return;
                    }
                    if let Err(force_error) = executor
                        .spawn(async move { delete_managed_branch(&removed, GitDeleteMode::Force) })
                        .await
                    {
                        finish_partial_error(
                            &this,
                            branch_id,
                            format!(
                                "The worktree was removed, but the branch could not be force deleted: {force_error}"
                            ),
                            cx,
                        );
                        return;
                    }
                } else {
                    finish_partial_error(
                        &this,
                        branch_id,
                        format!(
                            "The worktree was removed, but the branch could not be deleted: {error}"
                        ),
                        cx,
                    );
                    return;
                }
            }

            if let Err(error) = database.delete_branch(branch_id).await {
                finish_partial_error(
                    &this,
                    branch_id,
                    format!(
                        "The Git worktree and branch were deleted, but Void could not remove its database record: {error:#}"
                    ),
                    cx,
                );
                return;
            }

            let _ = this.update(cx, |this, cx| {
                this.is_deleting = false;
                cx.emit(BranchDeletionDialogEvent::Deleted(branch_id));
            });
        });
        self.deletion_task = Some(task);
    }
}

fn finish_with_error(
    dialog: &gpui::WeakEntity<BranchDeletionDialog>,
    branch_id: crate::BranchId,
    error: String,
    cx: &mut gpui::AsyncWindowContext,
) {
    let _ = dialog.update(cx, |dialog, cx| {
        dialog.is_deleting = false;
        dialog.error = Some(error);
        cx.emit(BranchDeletionDialogEvent::Failed(branch_id));
        cx.notify();
    });
}

fn finish_partial_error(
    dialog: &gpui::WeakEntity<BranchDeletionDialog>,
    branch_id: crate::BranchId,
    error: String,
    cx: &mut gpui::AsyncWindowContext,
) {
    let _ = dialog.update(cx, |dialog, cx| {
        dialog.is_deleting = false;
        dialog.error = Some(error);
        cx.emit(BranchDeletionDialogEvent::PartiallyDeleted(branch_id));
        cx.notify();
    });
}

fn finish_canceled(
    dialog: &gpui::WeakEntity<BranchDeletionDialog>,
    branch_id: crate::BranchId,
    cx: &mut gpui::AsyncWindowContext,
) {
    let _ = dialog.update(cx, |dialog, cx| {
        dialog.is_deleting = false;
        cx.emit(BranchDeletionDialogEvent::Failed(branch_id));
        cx.notify();
    });
}

fn branch_confirmation_matches(branch_name: &str, confirmation: &str) -> bool {
    confirmation == branch_name
}

impl gpui::Render for BranchDeletionDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_input {
            self.focus_input = false;
            self.confirmation_input.focus_handle(cx).focus(window, cx);
        }
        let can_delete = self.confirmation_matches(cx) && !self.is_deleting;

        dialog(cx)
            .key_context("BranchDeletionDialog")
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .flex()
            .flex_col()
            .w(px(440.))
            .gap_4()
            .p_5()
            .text_color(cx.theme().colors().text)
            .child(
                div()
                    .text_lg()
                    .child(format!("Permanently delete {:?}?", self.branch.name)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().colors().text_muted)
                    .child(format!(
                        "Type {} to confirm. This deletes its worktree and local branch.",
                        self.branch.name
                    )),
            )
            .child(self.confirmation_input.clone())
            .when_some(self.error.clone(), |dialog, error| {
                dialog.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().status().error)
                        .child(error),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        div()
                            .id("cancel-branch-deletion")
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label("Cancel branch deletion")
                            .h(px(32.))
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .when(!self.is_deleting, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|button| button.bg(cx.theme().colors().element_hover))
                                    .on_mouse_up(MouseButton::Left, cx.listener(Self::click_cancel))
                            })
                            .child("Cancel"),
                    )
                    .child(
                        div()
                            .id("confirm-branch-deletion")
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label("Permanently delete branch")
                            .h(px(32.))
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .bg(cx.theme().status().error)
                            .text_color(cx.theme().colors().editor_background)
                            .when(can_delete, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|button| button.opacity(0.9))
                                    .on_mouse_up(MouseButton::Left, cx.listener(Self::click_delete))
                            })
                            .when(!can_delete, |button| button.opacity(0.5))
                            .child(if self.is_deleting {
                                "Deleting…"
                            } else {
                                "Delete permanently"
                            }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::branch_confirmation_matches;

    #[test]
    fn exact_branch_name_confirms_deletion() {
        assert!(branch_confirmation_matches("feature/auth", "feature/auth"));
    }

    #[test]
    fn surrounding_whitespace_does_not_confirm_deletion() {
        assert!(!branch_confirmation_matches(
            "feature/auth",
            " feature/auth "
        ));
    }
}
