use std::collections::{HashMap, HashSet};

use crate::{
    Branch, BranchId, NewRepository, Repository, RepositoryId, Workspace, WorkspaceDb,
    inspect_git_repository,
};
use gpui::{
    Anchor, App, Context, DragMoveEvent, Entity, EventEmitter, MouseButton, MouseDownEvent,
    MouseUpEvent, PathPromptOptions, Role, ScrollHandle, Subscription, Task, Window, anchored,
    deferred, div, point, prelude::*, px,
};
use theme::ActiveTheme;

use crate::{
    git::RepositoryLiveDiff,
    view::branches::{BranchSelected, HEADER_HEIGHT},
};
use ui::{ListRow, auto_scroll_toward_edge, icon, move_item, popover};

mod menu;
mod repository;

pub(crate) struct AddBranchRequested {
    pub repository: Repository,
    pub position: i64,
}

pub(crate) struct DeleteBranchRequested {
    pub repository: Repository,
    pub branch: Branch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchArchived {
    pub branch_id: BranchId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SidebarModelEvent {
    RepositoryAdded(Repository),
    RepositoryPinned {
        repository_id: RepositoryId,
        is_pinned: bool,
    },
    RepositoryArchived(RepositoryId),
    RepositoryRestored(RepositoryId),
    RepositoriesReordered(Vec<RepositoryId>),
    BranchesReordered {
        repository_id: RepositoryId,
        branch_ids: Vec<BranchId>,
    },
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
    add_repository_task: Option<Task<()>>,
    repository_tasks: HashMap<RepositoryId, Task<()>>,
    branch_tasks: HashMap<BranchId, Task<()>>,
    reorder_repositories_task: Option<Task<()>>,
    reorder_branch_tasks: HashMap<RepositoryId, Task<()>>,
    error: Option<String>,
    active_branch_id: Option<crate::BranchId>,
    live_diffs: HashMap<RepositoryId, Entity<RepositoryLiveDiff>>,
    live_diff_subscriptions: HashMap<RepositoryId, Subscription>,
    repository_list_scroll: ScrollHandle,
}

impl EventEmitter<AddBranchRequested> for Sidebar {}
impl EventEmitter<DeleteBranchRequested> for Sidebar {}
impl EventEmitter<BranchSelected> for Sidebar {}
impl EventEmitter<BranchArchived> for Sidebar {}
impl EventEmitter<SidebarModelEvent> for Sidebar {}

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
            add_repository_task: None,
            repository_tasks: HashMap::new(),
            branch_tasks: HashMap::new(),
            reorder_repositories_task: None,
            reorder_branch_tasks: HashMap::new(),
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

    pub(crate) fn select_branch(&mut self, branch_id: crate::BranchId, cx: &mut Context<Self>) {
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

    pub(crate) fn expand_repository(&mut self, repository_id: RepositoryId) {
        self.expanded_repositories.insert(repository_id);
    }

    pub(crate) fn sync_records(
        &mut self,
        repositories: Vec<Repository>,
        branches: &[Branch],
        cx: &mut Context<Self>,
    ) {
        self.repositories = repositories
            .into_iter()
            .map(|repository| {
                let repository_branches = branches
                    .iter()
                    .filter(|branch| branch.repository_id == repository.id)
                    .cloned()
                    .collect();
                SidebarRepository::new(repository, repository_branches)
            })
            .collect();
        cx.notify();
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

        let task = cx.spawn_in(window, async move |this, cx| {
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

            let _ = this.update(cx, |this, cx| {
                this.is_adding_repository = false;
                match result {
                    Ok(Some(repository)) => {
                        this.expanded_repositories.insert(repository.id);
                        this.error = None;
                        cx.emit(SidebarModelEvent::RepositoryAdded(repository));
                    }
                    Ok(None) => {}
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        });
        self.add_repository_task = Some(task);
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
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = database
                .set_repository_pinned(repository_id, is_pinned)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.updating_repositories.remove(&repository_id);
                match result {
                    Ok(()) => {
                        cx.emit(SidebarModelEvent::RepositoryPinned {
                            repository_id,
                            is_pinned,
                        });
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not update repository: {error:#}"));
                    }
                }
                cx.notify();
            });
        });
        self.repository_tasks.insert(repository_id, task);
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
        cx.emit(SidebarModelEvent::RepositoriesReordered(
            reordered_ids.clone(),
        ));
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        let workspace_id = self.workspace.id;
        let task = cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = database
                .reorder_repositories(workspace_id, reordered_ids)
                .await
            {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(format!("Could not reorder repositories: {error:#}"));
                    cx.notify();
                });
            }
        });
        self.reorder_repositories_task = Some(task);
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
            .iter()
            .find(|entry| entry.repository.id == dragged.repository_id)
        else {
            return;
        };
        let mut reordered_branches = repository
            .branches
            .iter()
            .filter(|branch| branch.archived_at.is_none())
            .cloned()
            .collect::<Vec<_>>();
        let Some(source_index) = reordered_branches
            .iter()
            .position(|branch| branch.id == dragged.id)
        else {
            return;
        };
        let Some(target_index) = reordered_branches
            .iter()
            .position(|branch| branch.id == target_id)
        else {
            return;
        };
        if source_index == target_index {
            return;
        }

        move_item(&mut reordered_branches, source_index, target_index);
        let branch_ids = reordered_branches
            .iter()
            .map(|branch| branch.id)
            .collect::<Vec<_>>();
        let repository_id = repository.repository.id;
        cx.emit(SidebarModelEvent::BranchesReordered {
            repository_id,
            branch_ids: branch_ids.clone(),
        });
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = database.reorder_branches(repository_id, branch_ids).await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(format!("Could not reorder branches: {error:#}"));
                    cx.notify();
                });
            }
        });
        self.reorder_branch_tasks.insert(repository_id, task);
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
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = database.archive_repository(repository_id).await;
            let _ = this.update(cx, |this, cx| {
                this.updating_repositories.remove(&repository_id);
                match result {
                    Ok(()) => {
                        this.expanded_repositories.remove(&repository_id);
                        cx.emit(SidebarModelEvent::RepositoryArchived(repository_id));
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not archive repository: {error:#}"));
                    }
                }
                cx.notify();
            });
        });
        self.repository_tasks.insert(repository_id, task);
    }

    fn archive_branch(&mut self, branch_id: BranchId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.updating_branches.insert(branch_id) {
            return;
        }
        self.error = None;
        cx.notify();

        let database = cx.global::<WorkspaceDb>().clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = database.archive_branch(branch_id).await;
            let _ = this.update(cx, |this, cx| {
                this.updating_branches.remove(&branch_id);
                match result {
                    Ok(()) => {
                        cx.emit(BranchArchived { branch_id });
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not archive branch: {error:#}"));
                    }
                }
                cx.notify();
            });
        });
        self.branch_tasks.insert(branch_id, task);
    }

    fn delete_branch(&mut self, branch_id: BranchId, _: &mut Window, cx: &mut Context<Self>) {
        if self.updating_branches.contains(&branch_id) {
            return;
        }
        let Some((repository, branch)) = self.repositories.iter().find_map(|entry| {
            entry
                .branches
                .iter()
                .find(|branch| branch.id == branch_id)
                .map(|branch| (entry.repository.clone(), branch.clone()))
        }) else {
            return;
        };

        cx.emit(DeleteBranchRequested { repository, branch });
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
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = database.unarchive_repository(repository_id).await;
            let _ = this.update(cx, |this, cx| {
                this.updating_repositories.remove(&repository_id);
                match result {
                    Ok(()) => {
                        this.expanded_repositories.insert(repository_id);
                        cx.emit(SidebarModelEvent::RepositoryRestored(repository_id));
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not restore repository: {error:#}"));
                    }
                }
                cx.notify();
            });
        });
        self.repository_tasks.insert(repository_id, task);
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
