use std::collections::HashSet;

use gpui::{
    Anchor, Context, EventEmitter, MouseButton, MouseDownEvent, MouseUpEvent, PathPromptOptions,
    Role, Window, anchored, deferred, div, prelude::*, px, rgb,
};
use workspace::{
    Branch, NewRepository, Repository, RepositoryId, Workspace, WorkspaceDb, inspect_git_repository,
};

use crate::{icons::icon, theme};

pub(crate) struct AddBranchRequested {
    pub repository: Repository,
    pub position: i64,
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
    error: Option<String>,
}

impl EventEmitter<AddBranchRequested> for Sidebar {}

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
            error: None,
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

    fn render_workspace_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        deferred(
            anchored().anchor(Anchor::BottomLeft).child(
                div()
                    .mt_1()
                    .w(px(224.))
                    .p_1()
                    .rounded_md()
                    .bg(rgb(theme::ELEVATED_SURFACE))
                    .border_1()
                    .border_color(rgb(theme::BORDER))
                    .shadow_md()
                    .on_mouse_down_out(cx.listener(Self::dismiss_menu))
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
                            .child("Add repository"),
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
                    .h(px(30.))
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
                        .ml(px(15.))
                        .pl(px(8.))
                        .border_l_1()
                        .border_color(rgb(theme::BORDER_VARIANT))
                        .when(active_branches.is_empty(), |branches| {
                            branches.child(
                                div()
                                    .flex()
                                    .h(px(28.))
                                    .items_center()
                                    .px_2()
                                    .text_xs()
                                    .text_color(rgb(theme::TEXT_PLACEHOLDER))
                                    .child("No branches yet"),
                            )
                        })
                        .children(active_branches.into_iter().map(|branch| {
                            div()
                                .id(("branch", branch.id.as_i64() as u64))
                                .flex()
                                .h(px(28.))
                                .items_center()
                                .justify_between()
                                .px_2()
                                .rounded_sm()
                                .text_sm()
                                .hover(|row| row.bg(rgb(theme::ELEMENT_HOVER)))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(icon("icons/git-branch.svg"))
                                        .child(branch.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme::TEXT_PLACEHOLDER))
                                        .child(format!("#{}", branch.number)),
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
            .w(px(260.))
            .h_full()
            .bg(rgb(theme::SURFACE))
            .border_r_1()
            .border_color(rgb(theme::BORDER_VARIANT))
            .text_color(rgb(theme::TEXT))
            .text_sm()
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(rgb(theme::BORDER_VARIANT))
                    .child(
                        div()
                            .id("workspace-menu")
                            .relative()
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label("Workspace menu")
                            .flex()
                            .h(px(36.))
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
                                            .size(px(24.))
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
                    .child(
                        div()
                            .flex()
                            .h(px(28.))
                            .items_center()
                            .px_2()
                            .text_xs()
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child("PROJECTS"),
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
