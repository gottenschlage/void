use super::*;

impl Sidebar {
    pub(super) fn render_repository(
        &self,
        entry: &SidebarRepository,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let repository_id = entry.repository.id;
        let is_expanded = self.expanded_repositories.contains(&repository_id);
        let active_branches = entry
            .branches
            .iter()
            .filter(|branch| branch.archived_at.is_none())
            .collect::<Vec<_>>();
        let repository = entry.repository.clone();
        let branch_position = entry
            .branches
            .iter()
            .map(|branch| branch.position)
            .max()
            .map_or(0, |position| position + 1);

        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(("repository", repository_id.as_i64() as u64))
                    .group("repository-row")
                    .list_row(32.)
                    .justify_between()
                    .cursor_pointer()
                    .hover(|row| row.bg(cx.theme().colors().element_hover))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if !this.expanded_repositories.remove(&repository_id) {
                                this.expanded_repositories.insert(repository_id);
                            }
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(icon(
                                if is_expanded {
                                    "icons/folder-open.svg"
                                } else {
                                    "icons/folder.svg"
                                },
                                cx,
                            ))
                            .child(entry.repository.name.clone()),
                    )
                    .child(
                        div()
                            .id(("add-branch", repository_id.as_i64() as u64))
                            .flex()
                            .size(scaled(22.))
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .invisible()
                            .group_hover("repository-row", |button| button.visible())
                            .hover(|button| button.bg(cx.theme().colors().element_active))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |_, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.emit(AddBranchRequested {
                                        repository: repository.clone(),
                                        position: branch_position,
                                    });
                                }),
                            )
                            .child(icon("icons/plus.svg", cx)),
                    ),
            )
            .when(is_expanded, |repository| {
                repository.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .when(active_branches.is_empty(), |branches| {
                            branches.child(
                                div()
                                    .flex()
                                    .h(scaled(28.))
                                    .items_center()
                                    .pl(scaled(24.))
                                    .pr_2()
                                    .text_sm()
                                    .text_color(cx.theme().colors().text_placeholder)
                                    .child("No branches yet"),
                            )
                        })
                        .children(active_branches.into_iter().map(|branch| {
                            let branch_id = branch.id;
                            let is_active = self.active_branch_id == Some(branch_id);
                            let stat = self
                                .live_diffs
                                .get(&repository_id)
                                .and_then(|live_diff| live_diff.read(cx).stat(branch_id));
                            let dragged = DraggedSidebarBranch {
                                id: branch_id,
                                repository_id,
                                label: format!("#{} {}", branch.number, branch.name),
                            };
                            div()
                                .id(("branch", branch.id.as_i64() as u64))
                                .group("branch-row")
                                .list_row(28.)
                                .justify_between()
                                .cursor_pointer()
                                .when(is_active, |row| row.bg(cx.theme().colors().element_active))
                                .when(!is_active, |row| {
                                    row.hover(|row| row.bg(cx.theme().colors().element_hover))
                                })
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(move |_, _, _, cx| {
                                        cx.emit(BranchSelected { branch_id });
                                    }),
                                )
                                .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                                .drag_over::<DraggedSidebarBranch>(move |row, dragged, _, cx| {
                                    if dragged.repository_id == repository_id {
                                        row.border_b_2()
                                            .border_color(cx.theme().colors().text_accent)
                                    } else {
                                        row
                                    }
                                })
                                .can_drop(move |dragged, _, _| {
                                    dragged.downcast_ref::<DraggedSidebarBranch>().is_some_and(
                                        |dragged| dragged.repository_id == repository_id,
                                    )
                                })
                                .on_drop(cx.listener(move |this, dragged, window, cx| {
                                    this.drop_branch(dragged, branch_id, window, cx);
                                }))
                                .child(
                                    div()
                                        .flex()
                                        .min_w_0()
                                        .flex_1()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .w(scaled(18.))
                                                .flex_none()
                                                .text_right()
                                                .text_xs()
                                                .text_color(cx.theme().colors().text_placeholder)
                                                .child(format!("#{}", branch.number)),
                                        )
                                        .child(div().truncate().child(branch.name.clone())),
                                )
                                .child(
                                    div()
                                        .relative()
                                        .flex()
                                        .w(scaled(48.))
                                        .flex_none()
                                        .items_center()
                                        .justify_end()
                                        .when_some(stat, |container, stat| {
                                            container.child(
                                                div()
                                                    .flex()
                                                    .gap_1()
                                                    .text_xs()
                                                    .group_hover("branch-row", |counts| {
                                                        counts.invisible()
                                                    })
                                                    .child(
                                                        div()
                                                            .text_color(
                                                                cx.theme()
                                                                    .colors()
                                                                    .version_control_added,
                                                            )
                                                            .child(format!("+{}", stat.added)),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(
                                                                cx.theme()
                                                                    .colors()
                                                                    .version_control_deleted,
                                                            )
                                                            .child(format!("-{}", stat.deleted)),
                                                    ),
                                            )
                                        })
                                        .child(
                                            div()
                                                .absolute()
                                                .right_0()
                                                .flex()
                                                .invisible()
                                                .group_hover("branch-row", |actions| {
                                                    actions.visible()
                                                })
                                                .child(
                                                    div()
                                                        .id((
                                                            "archive-branch",
                                                            branch.id.as_i64() as u64,
                                                        ))
                                                        .flex()
                                                        .size(scaled(22.))
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_sm()
                                                        .hover(|button| {
                                                            button.bg(cx
                                                                .theme()
                                                                .colors()
                                                                .element_active)
                                                        })
                                                        .on_mouse_up(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                move |this, _, window, cx| {
                                                                    cx.stop_propagation();
                                                                    this.archive_branch(
                                                                        branch_id, window, cx,
                                                                    );
                                                                },
                                                            ),
                                                        )
                                                        .child(icon("icons/archive.svg", cx)),
                                                )
                                                .child(
                                                    div()
                                                        .id((
                                                            "delete-branch",
                                                            branch.id.as_i64() as u64,
                                                        ))
                                                        .flex()
                                                        .size(scaled(22.))
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_sm()
                                                        .hover(|button| {
                                                            button.bg(cx
                                                                .theme()
                                                                .colors()
                                                                .element_active)
                                                        })
                                                        .on_mouse_up(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                move |this, _, window, cx| {
                                                                    cx.stop_propagation();
                                                                    this.delete_branch(
                                                                        branch_id, window, cx,
                                                                    );
                                                                },
                                                            ),
                                                        )
                                                        .child(icon("icons/trash-2.svg", cx)),
                                                ),
                                        ),
                                )
                        })),
                )
            })
    }
}

impl gpui::Render for DraggedRepository {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drag_preview(self.name.clone(), cx)
    }
}

impl gpui::Render for DraggedSidebarBranch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drag_preview(self.label.clone(), cx)
    }
}

fn drag_preview(label: String, cx: &App) -> gpui::Div {
    div()
        .flex()
        .h(scaled(32.))
        .min_w(scaled(160.))
        .items_center()
        .px_2()
        .bg(cx.theme().colors().elevated_surface_background)
        .border_1()
        .border_color(cx.theme().colors().border)
        .shadow_md()
        .text_color(cx.theme().colors().text)
        .child(label)
}
