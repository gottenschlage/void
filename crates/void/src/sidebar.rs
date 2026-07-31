use std::collections::{HashMap, HashSet};

use gpui::{
    Anchor, App, Context, DragMoveEvent, Entity, EventEmitter, MouseButton, MouseDownEvent,
    MouseUpEvent, PathPromptOptions, PromptLevel, Role, ScrollHandle, Subscription, Window,
    anchored, deferred, div, point, prelude::*, px,
};
use theme::ActiveTheme;
use workspace::{
    Branch, BranchId, NewRepository, Repository, RepositoryId, Workspace, WorkspaceDb,
    delete_git_worktree, inspect_git_repository,
};

use crate::{
    branch_context_header::RepositoryLiveDiff,
    branch_header::{BranchSelected, HEADER_HEIGHT},
};
use ui::{ListRow, auto_scroll_toward_edge, icon, move_item, popover};

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
    live_diffs: HashMap<RepositoryId, Entity<RepositoryLiveDiff>>,
    live_diff_subscriptions: HashMap<RepositoryId, Subscription>,
    repository_list_scroll: ScrollHandle,
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
            live_diffs: HashMap::new(),
            live_diff_subscriptions: HashMap::new(),
            repository_list_scroll: ScrollHandle::new(),
        }
    }

    pub(crate) fn observe_live_diff(
        &mut self,
        repository_id: RepositoryId,
        live_diff: Entity<RepositoryLiveDiff>,
        cx: &mut Context<Self>,
    ) {
        if self.live_diffs.contains_key(&repository_id) {
            return;
        }

        let subscription = cx.observe(&live_diff, |_, _, cx| cx.notify());
        self.live_diffs.insert(repository_id, live_diff);
        self.live_diff_subscriptions
            .insert(repository_id, subscription);
    }

    pub(crate) fn forget_live_diff(&mut self, repository_id: RepositoryId) {
        self.live_diffs.remove(&repository_id);
        self.live_diff_subscriptions.remove(&repository_id);
    }

    pub(crate) fn select_branch(&mut self, branch_id: workspace::BranchId, cx: &mut Context<Self>) {
        if self.active_branch_id != Some(branch_id) {
            self.active_branch_id = Some(branch_id);
            cx.notify();
        }
    }

    /// Scrolls the repository list toward the cursor's edge while a
    /// repository or branch row is being dragged, so a target scrolled out
    /// of view can still be reached.
    fn scroll_toward_drag<T: 'static>(
        &mut self,
        event: &DragMoveEvent<T>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        auto_scroll_toward_edge(
            &self.repository_list_scroll,
            event.event.position,
            event.bounds,
        );
        cx.notify();
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

    /// Prompts for confirmation, then permanently deletes a branch: its
    /// worktree, its local Git branch, and its persisted record.
    fn delete_branch(&mut self, branch_id: BranchId, window: &mut Window, cx: &mut Context<Self>) {
        if self.updating_branches.contains(&branch_id) {
            return;
        }
        let Some((repository_path, branch)) = self.repositories.iter().find_map(|entry| {
            entry
                .branches
                .iter()
                .find(|branch| branch.id == branch_id)
                .map(|branch| (entry.repository.path.clone(), branch.clone()))
        }) else {
            return;
        };

        let prompt = format!("Delete branch \"{}\"?", branch.name);
        let answer = window.prompt(
            PromptLevel::Critical,
            &prompt,
            Some("This deletes the branch and its worktree. This cannot be undone."),
            &["Delete", "Cancel"],
            cx,
        );

        let database = cx.global::<WorkspaceDb>().clone();
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            if this
                .update(cx, |this, cx| {
                    this.updating_branches.insert(branch_id);
                    this.error = None;
                    cx.notify();
                })
                .is_err()
            {
                return;
            }

            let branch_name = branch.name.clone();
            let worktree_path = branch.path.clone();
            let result = executor
                .spawn(async move {
                    delete_git_worktree(&repository_path, &worktree_path, &branch_name)
                })
                .await
                .map_err(|error| format!("Could not delete the worktree: {error:#}"));
            let result = match result {
                Ok(()) => database
                    .delete_branch(branch_id)
                    .await
                    .map_err(|error| format!("Could not delete branch: {error:#}")),
                Err(error) => Err(error),
            };

            let _ = this.update(cx, |this, cx| {
                this.updating_branches.remove(&branch_id);
                match result {
                    Ok(()) => {
                        for entry in this.repositories.iter_mut() {
                            entry.branches.retain(|branch| branch.id != branch_id);
                        }
                        if this.active_branch_id == Some(branch_id) {
                            this.active_branch_id = None;
                        }
                        cx.emit(BranchArchived { branch_id });
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
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
                            .size(px(22.))
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
                                    .h(px(28.))
                                    .items_center()
                                    .pl(px(24.))
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
                                    cx.listener(move |this, _, _, cx| {
                                        this.select_branch(branch_id, cx);
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
                                                .w(px(18.))
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
                                        .w(px(48.))
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
                                                        .size(px(22.))
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
                                                        .size(px(22.))
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

impl gpui::Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_none()
            .flex_col()
            .w(px(240.))
            .h_full()
            .bg(cx.theme().colors().surface_background)
            .border_r_1()
            .border_color(cx.theme().colors().border_variant)
            .text_color(cx.theme().colors().text)
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
                            header.bg(cx.theme().colors().element_active)
                        })
                        .when(!self.menu_open, |header| {
                            header.hover(|header| header.bg(cx.theme().colors().element_hover))
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
                                        .bg(cx.theme().colors().element_background)
                                        .child(icon("icons/laptop.svg", cx)),
                                )
                                .child(format!("{}'s workspace", self.workspace.name)),
                        )
                        .child(icon("icons/chevrons-up-down.svg", cx))
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
                        .bg(cx.theme().colors().element_background)
                        .text_xs()
                        .text_color(cx.theme().status().error)
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
                    .track_scroll(&self.repository_list_scroll)
                    .on_drag_move::<DraggedRepository>(cx.listener(Self::scroll_toward_drag))
                    .on_drag_move::<DraggedSidebarBranch>(cx.listener(Self::scroll_toward_drag))
                    .p_2()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .h(px(28.))
                            .items_center()
                            .px_2()
                            .text_sm()
                            .text_color(cx.theme().colors().text_muted)
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
        .h(px(32.))
        .min_w(px(160.))
        .items_center()
        .px_2()
        .bg(cx.theme().colors().elevated_surface_background)
        .border_1()
        .border_color(cx.theme().colors().border)
        .shadow_md()
        .text_color(cx.theme().colors().text)
        .child(label)
}
