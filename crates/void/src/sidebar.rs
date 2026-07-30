use std::collections::HashSet;

use gpui::{
    Anchor, Context, EventEmitter, MouseButton, MouseDownEvent, MouseUpEvent, PathPromptOptions,
    Role, Window, anchored, deferred, div, point, prelude::*, px, rgb,
};
use workspace::{
    Branch, BranchId, NewRepository, Repository, RepositoryId, Workspace, WorkspaceDb,
    inspect_git_repository,
};

use crate::branch_header::{BranchSelected, HEADER_HEIGHT};
use crate::{icons::icon, theme};

pub(crate) struct AddBranchRequested {
    pub repository: Repository,
    pub position: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchArchived {
    pub branch_id: BranchId,
}

#[derive(Clone)]
struct DraggedRepository {
    id: RepositoryId,
    name: String,
}

#[derive(Clone)]
struct DraggedSidebarBranch {
    id: BranchId,
    repository_id: RepositoryId,
    label: String,
}

pub(crate) struct SidebarRepository {
    repository: Repository,
    branches: Vec<Branch>,
}

impl SidebarRepository {
    pub(crate) fn new(repository: Repository, branches: Vec<Branch>) -> Self {
        Self {
            repository,
            branches,
        }
    }
}

pub(crate) struct Sidebar {
    workspace: Workspace,
    repositories: Vec<SidebarRepository>,
    expanded_repositories: HashSet<RepositoryId>,
    menu_open: bool,
    is_adding_repository: bool,
    updating_repositories: HashSet<RepositoryId>,
    updating_branches: HashSet<BranchId>,
    error: Option<String>,
    active_branch_id: Option<workspace::BranchId>,
}

impl EventEmitter<AddBranchRequested> for Sidebar {}
impl EventEmitter<BranchSelected> for Sidebar {}
impl EventEmitter<BranchArchived> for Sidebar {}

impl Sidebar {
    pub(crate) fn new(workspace: Workspace, repositories: Vec<SidebarRepository>) -> Self {
        let expanded_repositories = repositories
            .iter()
            .filter(|entry| entry.repository.archived_at.is_none())
            .map(|entry| entry.repository.id)
            .collect();
        Self {
            workspace,
            repositories,
            expanded_repositories,
            menu_open: false,
            is_adding_repository: false,
            updating_repositories: HashSet::new(),
            updating_branches: HashSet::new(),
            error: None,
            active_branch_id: None,
        }
    }

    pub(crate) fn select_branch(&mut self, branch_id: workspace::BranchId, cx: &mut Context<Self>) {
        if self.active_branch_id != Some(branch_id) {
            self.active_branch_id = Some(branch_id);
            cx.notify();
        }
    }

    pub(crate) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if self.active_branch_id.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn add_branch(&mut self, branch: Branch, cx: &mut Context<Self>) {
        self.expanded_repositories.insert(branch.repository_id);
        if let Some(repository) = self
            .repositories
            .iter_mut()
            .find(|entry| entry.repository.id == branch.repository_id)
        {
            repository.branches.push(branch);
        }
        cx.notify();
    }

    fn sort_repositories(&mut self) {
        self.repositories.sort_by_key(|entry| {
            (
                entry.repository.archived_at.is_some(),
                !entry.repository.is_pinned,
                entry.repository.position,
                entry.repository.id,
            )
        });
    }

    fn toggle_menu(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.menu_open = !self.menu_open;
        self.error = None;
        cx.notify();
    }

    fn dismiss_menu(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.menu_open = false;
        cx.notify();
    }

    fn add_repository(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.is_adding_repository {
            return;
        }

        self.menu_open = false;
        self.is_adding_repository = true;
        self.error = None;
        cx.notify();

        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add repository".into()),
        });
        let database = cx.global::<WorkspaceDb>().clone();
        let workspace_id = self.workspace.id;
        let position = self
            .repositories
            .iter()
            .map(|entry| entry.repository.position)
            .max()
            .map_or(0, |position| position + 1);
        let existing_paths = self
            .repositories
            .iter()
            .map(|entry| entry.repository.path.clone())
            .collect::<HashSet<_>>();
        let executor = cx.background_executor().clone();

        cx.spawn_in(window, async move |this, cx| {
            let result: Result<Option<Repository>, String> = async {
                let selected = prompt
                    .await
                    .map_err(|error| format!("Could not open the folder picker: {error}"))?
                    .map_err(|error| format!("Could not select a repository: {error:#}"))?;
                let Some(path) = selected.and_then(|mut paths| paths.pop()) else {
                    return Ok(None);
                };

                let location = executor
                    .spawn(async move { inspect_git_repository(&path) })
                    .await
                    .map_err(|error| error.to_string())?;
                if existing_paths.contains(&location.path) {
                    return Err("This repository is already in the workspace".into());
                }

                let id = database
                    .add_repository(NewRepository {
                        workspace_id,
                        name: location.name.clone(),
                        path: location.path.clone(),
                        position,
                        is_pinned: false,
                    })
                    .await
                    .map_err(|error| format!("Could not add repository: {error:#}"))?;

                Ok(Some(Repository {
                    id,
                    workspace_id,
                    name: location.name,
                    path: location.path,
                    position,
                    is_pinned: false,
                    sequence: 0,
                    archived_at: None,
                }))
            }
            .await;

            this.update(cx, |this, cx| {
                this.is_adding_repository = false;
                match result {
                    Ok(Some(repository)) => {
                        this.expanded_repositories.insert(repository.id);
                        this.repositories
                            .push(SidebarRepository::new(repository, Vec::new()));
                        this.error = None;
                    }
                    Ok(None) => {}
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            })
        })
        .detach();
    }

    fn set_repository_pinned(
        &mut self,
        repository_id: RepositoryId,
        is_pinned: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.updating_repositories.insert(repository_id) {
            return;
        }
        self.error = None;
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = database
                .set_repository_pinned(repository_id, is_pinned)
                .await;
            this.update(cx, |this, cx| {
                this.updating_repositories.remove(&repository_id);
                match result {
                    Ok(()) => {
                        if let Some(entry) = this
                            .repositories
                            .iter_mut()
                            .find(|entry| entry.repository.id == repository_id)
                        {
                            entry.repository.is_pinned = is_pinned;
                        }
                        this.sort_repositories();
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not update repository: {error:#}"));
                    }
                }
                cx.notify();
            })
        })
        .detach();
    }

    fn drop_repository(
        &mut self,
        dragged: &DraggedRepository,
        target_id: RepositoryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_ids = self
            .repositories
            .iter()
            .filter(|entry| entry.repository.archived_at.is_none())
            .map(|entry| entry.repository.id)
            .collect::<Vec<_>>();
        let Some(source_index) = active_ids.iter().position(|id| *id == dragged.id) else {
            return;
        };
        let Some(target_index) = active_ids.iter().position(|id| *id == target_id) else {
            return;
        };
        if source_index == target_index {
            return;
        }

        let mut reordered_ids = active_ids;
        move_item(&mut reordered_ids, source_index, target_index);
        for (position, repository_id) in reordered_ids.iter().copied().enumerate() {
            if let Some(entry) = self
                .repositories
                .iter_mut()
                .find(|entry| entry.repository.id == repository_id)
            {
                entry.repository.position = position as i64;
            }
        }
        self.sort_repositories();
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        let workspace_id = self.workspace.id;
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = database
                .reorder_repositories(workspace_id, reordered_ids)
                .await
            {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(format!("Could not reorder repositories: {error:#}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn drop_branch(
        &mut self,
        dragged: &DraggedSidebarBranch,
        target_id: BranchId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repository) = self
            .repositories
            .iter_mut()
            .find(|entry| entry.repository.id == dragged.repository_id)
        else {
            return;
        };
        let Some(source_index) = repository
            .branches
            .iter()
            .position(|branch| branch.id == dragged.id)
        else {
            return;
        };
        let Some(target_index) = repository
            .branches
            .iter()
            .position(|branch| branch.id == target_id)
        else {
            return;
        };
        if source_index == target_index {
            return;
        }

        move_item(&mut repository.branches, source_index, target_index);
        for (position, branch) in repository
            .branches
            .iter_mut()
            .filter(|branch| branch.archived_at.is_none())
            .enumerate()
        {
            branch.position = position as i64;
        }
        let branch_ids = repository
            .branches
            .iter()
            .filter(|branch| branch.archived_at.is_none())
            .map(|branch| branch.id)
            .collect::<Vec<_>>();
        let repository_id = repository.repository.id;
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = database.reorder_branches(repository_id, branch_ids).await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(format!("Could not reorder branches: {error:#}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn archive_repository(
        &mut self,
        repository_id: RepositoryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.updating_repositories.insert(repository_id) {
            return;
        }
        self.error = None;
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = database.archive_repository(repository_id).await;
            this.update(cx, |this, cx| {
                this.updating_repositories.remove(&repository_id);
                match result {
                    Ok(()) => {
                        if let Some(entry) = this
                            .repositories
                            .iter_mut()
                            .find(|entry| entry.repository.id == repository_id)
                        {
                            entry.repository.archived_at = Some(String::new());
                            entry.repository.is_pinned = false;
                        }
                        this.expanded_repositories.remove(&repository_id);
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not archive repository: {error:#}"));
                    }
                }
                cx.notify();
            })
        })
        .detach();
    }

    fn archive_branch(&mut self, branch_id: BranchId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.updating_branches.insert(branch_id) {
            return;
        }
        self.error = None;
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = database.archive_branch(branch_id).await;
            this.update(cx, |this, cx| {
                this.updating_branches.remove(&branch_id);
                match result {
                    Ok(()) => {
                        if let Some(branch) = this
                            .repositories
                            .iter_mut()
                            .flat_map(|entry| &mut entry.branches)
                            .find(|branch| branch.id == branch_id)
                        {
                            branch.archived_at = Some(String::new());
                            branch.is_pinned = false;
                        }
                        if this.active_branch_id == Some(branch_id) {
                            this.active_branch_id = None;
                        }
                        cx.emit(BranchArchived { branch_id });
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not archive branch: {error:#}"));
                    }
                }
                cx.notify();
            })
        })
        .detach();
    }

    fn restore_repository(
        &mut self,
        repository_id: RepositoryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.updating_repositories.insert(repository_id) {
            return;
        }
        self.error = None;
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = database.unarchive_repository(repository_id).await;
            this.update(cx, |this, cx| {
                this.updating_repositories.remove(&repository_id);
                match result {
                    Ok(()) => {
                        if let Some(entry) = this
                            .repositories
                            .iter_mut()
                            .find(|entry| entry.repository.id == repository_id)
                        {
                            entry.repository.archived_at = None;
                        }
                        this.expanded_repositories.insert(repository_id);
                        this.sort_repositories();
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not restore repository: {error:#}"));
                    }
                }
                cx.notify();
            })
        })
        .detach();
    }

    fn render_workspace_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .position(point(px(8.0), px(HEADER_HEIGHT + 60.0)))
                .child(
                    div()
                        .w(px(224.))
                        .p_1()
                        .rounded_md()
                        .bg(rgb(theme::ELEVATED_SURFACE))
                        .border_1()
                        .border_color(rgb(theme::BORDER))
                        .shadow_md()
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
                                        .flex()
                                        .h(px(32.))
                                        .items_center()
                                        .gap_1()
                                        .px_2()
                                        .rounded_sm()
                                        .text_sm()
                                        .when(is_updating, |row| row.opacity(0.5))
                                        .when(!is_updating, |row| {
                                            row.hover(|row| row.bg(rgb(theme::ELEMENT_HOVER)))
                                        })
                                        .drag_over::<DraggedRepository>(|row, _, _, _| {
                                            row.border_b_2().border_color(rgb(theme::ACCENT))
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
                                                    button.bg(rgb(theme::ELEMENT_ACTIVE))
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
                                                .child(icon(if is_pinned {
                                                    "icons/pin-off.svg"
                                                } else {
                                                    "icons/pin.svg"
                                                })),
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
                                                    button.bg(rgb(theme::ELEMENT_ACTIVE))
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
                                                .child(icon("icons/archive.svg")),
                                        )
                                }),
                        )
                        .when(
                            self.repositories
                                .iter()
                                .any(|entry| entry.repository.archived_at.is_some()),
                            |menu| {
                                menu.child(
                                    div().h(px(1.)).mx_1().my_1().bg(rgb(theme::BORDER_VARIANT)),
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
                                                .flex()
                                                .h(px(32.))
                                                .items_center()
                                                .gap_1()
                                                .px_2()
                                                .rounded_sm()
                                                .text_sm()
                                                .text_color(rgb(theme::TEXT_MUTED))
                                                .when(is_updating, |row| row.opacity(0.5))
                                                .when(!is_updating, |row| {
                                                    row.hover(|row| {
                                                        row.bg(rgb(theme::ELEMENT_HOVER))
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
                                                            button.bg(rgb(theme::ELEMENT_ACTIVE))
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
                                                        .child(icon("icons/archive-restore.svg")),
                                                )
                                        }),
                                )
                            },
                        )
                        .child(div().h(px(1.)).mx_1().my_1().bg(rgb(theme::BORDER_VARIANT)))
                        .child(
                            div()
                                .id("add-repository")
                                .focusable()
                                .tab_stop(true)
                                .role(Role::Button)
                                .aria_label("Add repository")
                                .flex()
                                .h(px(32.))
                                .items_center()
                                .gap_2()
                                .px_2()
                                .rounded_sm()
                                .text_sm()
                                .cursor_pointer()
                                .hover(|item| item.bg(rgb(theme::ELEMENT_HOVER)))
                                .on_mouse_up(MouseButton::Left, cx.listener(Self::add_repository))
                                .child(icon("icons/plus.svg"))
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

    fn render_repository(&self, entry: &SidebarRepository, cx: &mut Context<Self>) -> gpui::Div {
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
                    .flex()
                    .h(px(32.))
                    .items_center()
                    .justify_between()
                    .px_2()
                    .rounded_sm()
                    .text_sm()
                    .cursor_pointer()
                    .hover(|row| row.bg(rgb(theme::ELEMENT_HOVER)))
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
                            .child(icon(if is_expanded {
                                "icons/folder-open.svg"
                            } else {
                                "icons/folder.svg"
                            }))
                            .child(entry.repository.name.clone()),
                    )
                    .child(
                        div()
                            .id(("add-branch", repository_id.as_i64() as u64))
                            .flex()
                            .size(px(22.))
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .invisible()
                            .group_hover("repository-row", |button| button.visible())
                            .hover(|button| button.bg(rgb(theme::ELEMENT_ACTIVE)))
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
                            .child(icon("icons/plus.svg")),
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
                                    .h(px(28.))
                                    .items_center()
                                    .pl(px(24.))
                                    .pr_2()
                                    .text_sm()
                                    .text_color(rgb(theme::TEXT_PLACEHOLDER))
                                    .child("No branches yet"),
                            )
                        })
                        .children(active_branches.into_iter().map(|branch| {
                            let branch_id = branch.id;
                            let is_active = self.active_branch_id == Some(branch_id);
                            let dragged = DraggedSidebarBranch {
                                id: branch_id,
                                repository_id,
                                label: format!("#{} {}", branch.number, branch.name),
                            };
                            div()
                                .id(("branch", branch.id.as_i64() as u64))
                                .group("branch-row")
                                .flex()
                                .h(px(28.))
                                .items_center()
                                .justify_between()
                                .px_2()
                                .rounded_sm()
                                .text_sm()
                                .cursor_pointer()
                                .when(is_active, |row| row.bg(rgb(theme::ELEMENT_ACTIVE)))
                                .when(!is_active, |row| {
                                    row.hover(|row| row.bg(rgb(theme::ELEMENT_HOVER)))
                                })
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.select_branch(branch_id, cx);
                                        cx.emit(BranchSelected { branch_id });
                                    }),
                                )
                                .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                                .drag_over::<DraggedSidebarBranch>(move |row, dragged, _, _| {
                                    if dragged.repository_id == repository_id {
                                        row.border_b_2().border_color(rgb(theme::ACCENT))
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
                                                .w(px(18.))
                                                .flex_none()
                                                .text_right()
                                                .text_xs()
                                                .text_color(rgb(theme::TEXT_PLACEHOLDER))
                                                .child(format!("#{}", branch.number)),
                                        )
                                        .child(branch.name.clone()),
                                )
                                .child(
                                    div()
                                        .id(("archive-branch", branch.id.as_i64() as u64))
                                        .flex()
                                        .size(px(22.))
                                        .flex_none()
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .invisible()
                                        .group_hover("branch-row", |button| button.visible())
                                        .hover(|button| button.bg(rgb(theme::ELEMENT_ACTIVE)))
                                        .on_mouse_up(
                                            MouseButton::Left,
                                            cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.archive_branch(branch_id, window, cx);
                                            }),
                                        )
                                        .child(icon("icons/archive.svg")),
                                )
                        })),
                )
            })
    }
}

impl gpui::Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_none()
            .flex_col()
            .w(px(240.))
            .h_full()
            .bg(rgb(theme::SURFACE))
            .border_r_1()
            .border_color(rgb(theme::BORDER_VARIANT))
            .text_color(rgb(theme::TEXT))
            .text_sm()
            .child(
                div().p_2().child(
                    div()
                        .id("workspace-menu")
                        .relative()
                        .focusable()
                        .tab_stop(true)
                        .role(Role::Button)
                        .aria_label("Workspace menu")
                        .flex()
                        .h(px(48.))
                        .items_center()
                        .justify_between()
                        .px_2()
                        .rounded_sm()
                        .cursor_pointer()
                        .when(self.menu_open, |header| {
                            header.bg(rgb(theme::ELEMENT_ACTIVE))
                        })
                        .when(!self.menu_open, |header| {
                            header.hover(|header| header.bg(rgb(theme::ELEMENT_HOVER)))
                        })
                        .on_mouse_up(MouseButton::Left, cx.listener(Self::toggle_menu))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .size(px(32.))
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .bg(rgb(theme::ELEMENT))
                                        .child(icon("icons/laptop.svg")),
                                )
                                .child(self.workspace.name.clone()),
                        )
                        .child(icon("icons/chevrons-up-down.svg"))
                        .when(self.menu_open, |header| {
                            header.child(self.render_workspace_menu(cx))
                        }),
                ),
            )
            .when_some(self.error.clone(), |sidebar, error| {
                sidebar.child(
                    div()
                        .mx_2()
                        .mt_2()
                        .p_2()
                        .rounded_sm()
                        .bg(rgb(theme::ELEMENT))
                        .text_xs()
                        .text_color(rgb(theme::ERROR))
                        .child(error),
                )
            })
            .child(
                div()
                    .id("repository-list")
                    .flex()
                    .flex_1()
                    .flex_col()
                    .overflow_y_scroll()
                    .p_2()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .h(px(28.))
                            .items_center()
                            .px_2()
                            .text_sm()
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child("Projects"),
                    )
                    .children(
                        self.repositories
                            .iter()
                            .filter(|entry| entry.repository.archived_at.is_none())
                            .map(|entry| self.render_repository(entry, cx)),
                    ),
            )
    }
}

impl gpui::Render for DraggedRepository {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        drag_preview(self.name.clone())
    }
}

impl gpui::Render for DraggedSidebarBranch {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        drag_preview(self.label.clone())
    }
}

fn drag_preview(label: String) -> gpui::Div {
    div()
        .flex()
        .h(px(32.))
        .min_w(px(160.))
        .items_center()
        .px_2()
        .bg(rgb(theme::ELEVATED_SURFACE))
        .border_1()
        .border_color(rgb(theme::BORDER))
        .shadow_md()
        .text_color(rgb(theme::TEXT))
        .child(label)
}

fn move_item<T>(items: &mut Vec<T>, source_index: usize, target_index: usize) {
    if source_index == target_index || source_index >= items.len() || target_index >= items.len() {
        return;
    }
    let item = items.remove(source_index);
    items.insert(target_index, item);
}
