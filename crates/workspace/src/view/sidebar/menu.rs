use super::*;

impl Sidebar {
    pub(super) fn render_workspace_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .position(point(px(8.0), px(HEADER_HEIGHT + 60.0)))
                .child(
                    popover(cx)
                        .w(px(224.))
                        .p_1()
                        .on_mouse_down_out(cx.listener(Self::dismiss_menu))
                        .children(
                            self.repositories
                                .iter()
                                .filter(|entry| entry.repository.archived_at.is_none())
                                .map(|entry| {
                                    let repository_id = entry.repository.id;
                                    let is_pinned = entry.repository.is_pinned;
                                    let is_updating =
                                        self.updating_repositories.contains(&repository_id);
                                    let dragged = DraggedRepository {
                                        id: repository_id,
                                        name: entry.repository.name.clone(),
                                    };
                                    div()
                                        .id(("workspace-repository", repository_id.as_i64() as u64))
                                        .group("workspace-repository")
                                        .list_row(32.)
                                        .gap_1()
                                        .when(is_updating, |row| row.opacity(0.5))
                                        .when(!is_updating, |row| {
                                            row.hover(|row| {
                                                row.bg(cx.theme().colors().element_hover)
                                            })
                                        })
                                        .drag_over::<DraggedRepository>(|row, _, _, cx| {
                                            row.border_b_2()
                                                .border_color(cx.theme().colors().text_accent)
                                        })
                                        .on_drop(cx.listener(move |this, dragged, window, cx| {
                                            this.drop_repository(
                                                dragged,
                                                repository_id,
                                                window,
                                                cx,
                                            );
                                        }))
                                        .child(
                                            div()
                                                .id((
                                                    "workspace-repository-drag-handle",
                                                    repository_id.as_i64() as u64,
                                                ))
                                                .flex_1()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .cursor_grab()
                                                .on_drag(dragged, |dragged, _, _, cx| {
                                                    cx.new(|_| dragged.clone())
                                                })
                                                .child(entry.repository.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .id((
                                                    "pin-workspace-repository",
                                                    repository_id.as_i64() as u64,
                                                ))
                                                .flex()
                                                .size(px(24.))
                                                .items_center()
                                                .justify_center()
                                                .rounded_sm()
                                                .cursor_pointer()
                                                .hover(|button| {
                                                    button.bg(cx.theme().colors().element_active)
                                                })
                                                .on_mouse_up(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        if !is_updating {
                                                            this.set_repository_pinned(
                                                                repository_id,
                                                                !is_pinned,
                                                                window,
                                                                cx,
                                                            );
                                                        }
                                                    }),
                                                )
                                                .child(icon(
                                                    if is_pinned {
                                                        "icons/pin-off.svg"
                                                    } else {
                                                        "icons/pin.svg"
                                                    },
                                                    cx,
                                                )),
                                        )
                                        .child(
                                            div()
                                                .id((
                                                    "archive-workspace-repository",
                                                    repository_id.as_i64() as u64,
                                                ))
                                                .flex()
                                                .size(px(24.))
                                                .items_center()
                                                .justify_center()
                                                .rounded_sm()
                                                .cursor_pointer()
                                                .hover(|button| {
                                                    button.bg(cx.theme().colors().element_active)
                                                })
                                                .on_mouse_up(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        if !is_updating {
                                                            this.archive_repository(
                                                                repository_id,
                                                                window,
                                                                cx,
                                                            );
                                                        }
                                                    }),
                                                )
                                                .child(icon("icons/archive.svg", cx)),
                                        )
                                }),
                        )
                        .when(
                            self.repositories
                                .iter()
                                .any(|entry| entry.repository.archived_at.is_some()),
                            |menu| {
                                menu.child(
                                    div()
                                        .h(px(1.))
                                        .mx_1()
                                        .my_1()
                                        .bg(cx.theme().colors().border_variant),
                                )
                                .children(
                                    self.repositories
                                        .iter()
                                        .filter(|entry| entry.repository.archived_at.is_some())
                                        .map(|entry| {
                                            let repository_id = entry.repository.id;
                                            let is_updating =
                                                self.updating_repositories.contains(&repository_id);
                                            div()
                                                .id((
                                                    "archived-workspace-repository",
                                                    repository_id.as_i64() as u64,
                                                ))
                                                .group("archived-workspace-repository")
                                                .list_row(32.)
                                                .gap_1()
                                                .text_color(cx.theme().colors().text_muted)
                                                .when(is_updating, |row| row.opacity(0.5))
                                                .when(!is_updating, |row| {
                                                    row.hover(|row| {
                                                        row.bg(cx.theme().colors().element_hover)
                                                    })
                                                })
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .overflow_hidden()
                                                        .whitespace_nowrap()
                                                        .child(entry.repository.name.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .id((
                                                            "restore-workspace-repository",
                                                            repository_id.as_i64() as u64,
                                                        ))
                                                        .flex()
                                                        .size(px(24.))
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_sm()
                                                        .cursor_pointer()
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
                                                                    if !is_updating {
                                                                        this.restore_repository(
                                                                            repository_id,
                                                                            window,
                                                                            cx,
                                                                        );
                                                                    }
                                                                },
                                                            ),
                                                        )
                                                        .child(icon(
                                                            "icons/archive-restore.svg",
                                                            cx,
                                                        )),
                                                )
                                        }),
                                )
                            },
                        )
                        .child(
                            div()
                                .h(px(1.))
                                .mx_1()
                                .my_1()
                                .bg(cx.theme().colors().border_variant),
                        )
                        .child(
                            div()
                                .id("add-repository")
                                .focusable()
                                .tab_stop(true)
                                .role(Role::Button)
                                .aria_label("Add repository")
                                .list_row(32.)
                                .gap_2()
                                .cursor_pointer()
                                .hover(|item| item.bg(cx.theme().colors().element_hover))
                                .on_mouse_up(MouseButton::Left, cx.listener(Self::add_repository))
                                .child(icon("icons/plus.svg", cx))
                                .child(if self.is_adding_repository {
                                    "Adding repository…"
                                } else {
                                    "Add repository"
                                }),
                        ),
                ),
        )
        .with_priority(1)
    }
}
