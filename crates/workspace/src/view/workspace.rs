//! Workspace UI coordination and resource ownership.

use std::collections::HashMap;

use gpui::{
    Animation, AnimationExt, Context, Entity, Focusable, Subscription, Task, Window, deferred, div,
    prelude::*, px, rgb,
};
use theme::ActiveTheme as _;
use ui::TextInput;
use void_terminal::{BranchTerminalPanel, TerminalSettings};

use super::{
    branches::{
        BranchClosed, BranchContextHeader, BranchDeletionDialog, BranchDeletionDialogEvent,
        BranchDialog, BranchDialogEvent, BranchHeader, BranchMoved, BranchSelected,
    },
    sidebar::{
        AddBranchRequested, BranchArchived, DeleteBranchRequested, Sidebar, SidebarModelEvent,
        SidebarRepository,
    },
    title_bar::{SIDEBAR_WIDTH, TITLEBAR_TRANSITION, interpolate_width},
};
use crate::git::RepositoryLiveDiff;
use crate::{Branch, BranchId, RepositoryId, WorkspaceModel};

/// Font used for workspace chrome.
pub(crate) const UI_FONT: &str = "JetBrains Mono";
/// Font size used for workspace chrome and the window rem size.
pub const UI_FONT_SIZE: f32 = 13.0;

/// Coordinates the persisted workspace model and its GPUI-owned resources.
pub struct WorkspaceView {
    pub(super) workspace_model: Option<WorkspaceModel>,
    pub(super) sidebar: Option<Entity<Sidebar>>,
    pub(super) branch_dialog: Option<Entity<BranchDialog>>,
    pub(super) branch_deletion_dialog: Option<Entity<BranchDeletionDialog>>,
    pub(super) sidebar_subscription: Option<Subscription>,
    pub(super) sidebar_branch_subscription: Option<Subscription>,
    pub(super) sidebar_branch_archive_subscription: Option<Subscription>,
    pub(super) sidebar_branch_delete_subscription: Option<Subscription>,
    pub(super) sidebar_model_subscription: Option<Subscription>,
    pub(super) branch_dialog_subscription: Option<Subscription>,
    pub(super) branch_deletion_dialog_subscription: Option<Subscription>,
    pub(super) branch_header: Option<Entity<BranchHeader>>,
    pub(super) branch_header_subscription: Option<Subscription>,
    pub(super) branch_close_subscription: Option<Subscription>,
    pub(super) branch_move_subscription: Option<Subscription>,
    pub(super) repository_live_diffs: HashMap<RepositoryId, Entity<RepositoryLiveDiff>>,
    pub(super) branch_context_headers: HashMap<BranchId, Entity<BranchContextHeader>>,
    pub(super) branch_panels: HashMap<BranchId, Entity<BranchTerminalPanel>>,
    pub(super) branch_release_tasks: HashMap<BranchId, Task<()>>,
    pub(super) name_input: Entity<TextInput>,
    pub(super) create_workspace_task: Option<Task<()>>,
    pub(super) error: Option<String>,
    pub(super) is_creating: bool,
    pub(super) sidebar_open: bool,
    pub(super) sidebar_animation_generation: usize,
    pub(super) should_move_window: bool,
    pub(super) was_fullscreen: bool,
    pub(super) window_bounds_subscription: Option<Subscription>,
}

impl WorkspaceView {
    /// Creates the workspace surface from the model loaded during startup.
    pub fn new(
        workspace_model: Option<WorkspaceModel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| TextInput::new("Workspace name", cx));
        if workspace_model.is_none() {
            name_input.focus_handle(cx).focus(window, cx);
        }
        let sidebar = workspace_model.as_ref().map(|model| {
            let repositories = model
                .repositories()
                .iter()
                .cloned()
                .map(|repository| {
                    let branches = model
                        .branches()
                        .iter()
                        .filter(|branch| branch.repository_id == repository.id)
                        .cloned()
                        .collect();
                    SidebarRepository::new(repository, branches)
                })
                .collect();
            cx.new(|_| Sidebar::new(model.workspace().clone(), repositories))
        });
        let branch_header = workspace_model
            .as_ref()
            .map(|_| cx.new(|_| BranchHeader::new(Vec::new())));
        let mut this = Self {
            workspace_model,
            sidebar: None,
            branch_dialog: None,
            branch_deletion_dialog: None,
            sidebar_subscription: None,
            sidebar_branch_subscription: None,
            sidebar_branch_archive_subscription: None,
            sidebar_branch_delete_subscription: None,
            sidebar_model_subscription: None,
            branch_dialog_subscription: None,
            branch_deletion_dialog_subscription: None,
            branch_header: None,
            branch_header_subscription: None,
            branch_close_subscription: None,
            branch_move_subscription: None,
            repository_live_diffs: HashMap::new(),
            branch_context_headers: HashMap::new(),
            branch_panels: HashMap::new(),
            branch_release_tasks: HashMap::new(),
            name_input,
            create_workspace_task: None,
            error: None,
            is_creating: false,
            sidebar_open: super::title_bar::default_sidebar_open(),
            sidebar_animation_generation: 0,
            should_move_window: false,
            was_fullscreen: window.is_fullscreen(),
            window_bounds_subscription: None,
        };
        if let Some(sidebar) = sidebar {
            this.attach_sidebar(sidebar, window, cx);
        }
        let branches = this
            .workspace_model
            .as_ref()
            .map(|model| model.active_branches().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for branch in branches {
            this.register_branch_diff(&branch, cx);
        }
        if let Some(branch_header) = branch_header {
            this.attach_branch_header(branch_header, window, cx);
        }
        this.sync_traffic_lights(window);
        this.window_bounds_subscription =
            Some(cx.observe_window_bounds(window, |this, window, cx| {
                let fullscreen = window.is_fullscreen();
                if fullscreen != this.was_fullscreen {
                    this.was_fullscreen = fullscreen;
                    this.sync_traffic_lights(window);
                    cx.notify();
                }
            }));
        this
    }

    pub(super) fn attach_sidebar(
        &mut self,
        sidebar: Entity<Sidebar>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_subscription = Some(cx.subscribe_in(
            &sidebar,
            window,
            |this, _, request: &AddBranchRequested, window, cx| {
                this.open_branch_dialog(request, window, cx);
            },
        ));
        self.sidebar_branch_subscription = Some(cx.subscribe_in(
            &sidebar,
            window,
            |this, _, selection: &BranchSelected, window, cx| {
                this.select_branch(selection.branch_id, window, cx);
            },
        ));
        self.sidebar_branch_archive_subscription = Some(cx.subscribe_in(
            &sidebar,
            window,
            |this, _, event: &BranchArchived, window, cx| {
                this.release_branch_resources(event.branch_id, cx);
                if let Some(model) = this.workspace_model.as_mut() {
                    model.archive_branch(event.branch_id);
                }
                this.sync_branch_header(cx);
                this.sync_sidebar(cx);
                this.sync_active_branch(window, cx);
                cx.notify();
            },
        ));
        self.sidebar_branch_delete_subscription = Some(cx.subscribe_in(
            &sidebar,
            window,
            |this, _, request: &DeleteBranchRequested, window, cx| {
                this.open_branch_deletion_dialog(request, window, cx);
            },
        ));
        self.sidebar_model_subscription = Some(cx.subscribe_in(
            &sidebar,
            window,
            |this, _, event: &SidebarModelEvent, window, cx| {
                this.handle_sidebar_model_event(event, window, cx);
            },
        ));
        self.sidebar = Some(sidebar);
        self.sync_sidebar(cx);
    }

    fn handle_sidebar_model_event(
        &mut self,
        event: &SidebarModelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SidebarModelEvent::RepositoryAdded(repository) => {
                if let Some(model) = self.workspace_model.as_mut() {
                    model.add_repository(repository.clone());
                }
            }
            SidebarModelEvent::RepositoryPinned {
                repository_id,
                is_pinned,
            } => {
                if let Some(model) = self.workspace_model.as_mut() {
                    model.set_repository_pinned(*repository_id, *is_pinned);
                }
            }
            SidebarModelEvent::RepositoryArchived(repository_id) => {
                let branch_ids = self
                    .workspace_model
                    .as_ref()
                    .map(|model| {
                        model
                            .branches()
                            .iter()
                            .filter(|branch| branch.repository_id == *repository_id)
                            .map(|branch| branch.id)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for branch_id in branch_ids {
                    self.release_branch_context(branch_id);
                    self.unregister_branch_diff(branch_id, cx);
                    self.branch_panels.remove(&branch_id);
                }
                if let Some(model) = self.workspace_model.as_mut() {
                    model.archive_repository(*repository_id);
                }
                self.sync_branch_header(cx);
                self.sync_active_branch(window, cx);
            }
            SidebarModelEvent::RepositoryRestored(repository_id) => {
                if let Some(model) = self.workspace_model.as_mut() {
                    model.restore_repository(*repository_id);
                }
            }
            SidebarModelEvent::RepositoriesReordered(repository_ids) => {
                if let Some(model) = self.workspace_model.as_mut() {
                    model.reorder_repositories(repository_ids);
                }
            }
            SidebarModelEvent::BranchesReordered {
                repository_id,
                branch_ids,
            } => {
                if let Some(model) = self.workspace_model.as_mut() {
                    model.reorder_branches(*repository_id, branch_ids);
                }
            }
        }
        self.sync_sidebar(cx);
    }

    pub(super) fn attach_branch_header(
        &mut self,
        branch_header: Entity<BranchHeader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.branch_header_subscription = Some(cx.subscribe_in(
            &branch_header,
            window,
            |this, _, selection: &BranchSelected, window, cx| {
                this.select_branch(selection.branch_id, window, cx);
            },
        ));
        self.branch_close_subscription = Some(cx.subscribe_in(
            &branch_header,
            window,
            |this, _, event: &BranchClosed, window, cx| {
                if let Some(model) = this.workspace_model.as_mut() {
                    model.close_branch(event.branch_id);
                }
                this.sync_branch_header(cx);
                this.release_branch_context(event.branch_id);
                this.branch_panels.remove(&event.branch_id);
                this.sync_active_branch(window, cx);
                cx.notify();
            },
        ));
        self.branch_move_subscription = Some(cx.subscribe(
            &branch_header,
            |this, _, event: &BranchMoved, cx| {
                if let Some(model) = this.workspace_model.as_mut() {
                    model.move_open_branch(event.branch_id, event.target_index);
                }
                this.sync_branch_header(cx);
            },
        ));
        self.branch_header = Some(branch_header);
        self.sync_branch_header(cx);
    }

    fn select_branch(&mut self, branch_id: BranchId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(model) = self.workspace_model.as_mut() else {
            return;
        };
        if !model.open_branch(branch_id) {
            return;
        }
        let Some(branch) = model.branch(branch_id).cloned() else {
            return;
        };
        if let Some(sidebar) = self.sidebar.as_ref() {
            sidebar.update(cx, |sidebar, cx| sidebar.select_branch(branch_id, cx));
        }
        self.sync_branch_header(cx);
        let live_diff = self.register_branch_diff(&branch, cx);
        self.branch_context_headers
            .entry(branch_id)
            .or_insert_with(|| {
                cx.new(|cx| BranchContextHeader::new(branch.clone(), live_diff, cx))
            });
        let panel = self.branch_panels.entry(branch_id).or_insert_with(|| {
            cx.new(|_| BranchTerminalPanel::new(branch.path.clone(), TerminalSettings::default()))
        });
        panel.update(cx, |panel, cx| panel.activate(window, cx));
        cx.notify();
    }

    fn active_branch_id(&self) -> Option<BranchId> {
        self.workspace_model
            .as_ref()
            .and_then(WorkspaceModel::active_branch_id)
    }

    fn sync_branch_header(&self, cx: &mut Context<Self>) {
        let Some(header) = self.branch_header.as_ref() else {
            return;
        };
        let Some(model) = self.workspace_model.as_ref() else {
            return;
        };
        let branches = model.open_branches().cloned().collect();
        let active_branch_id = model.active_branch_id();
        header.update(cx, |header, cx| {
            header.sync(branches, active_branch_id, cx);
        });
    }

    fn sync_sidebar(&self, cx: &mut Context<Self>) {
        let Some(sidebar) = self.sidebar.as_ref() else {
            return;
        };
        let Some(model) = self.workspace_model.as_ref() else {
            return;
        };
        let repositories = model.repositories().to_vec();
        let branches = model.branches().to_vec();
        sidebar.update(cx, |sidebar, cx| {
            sidebar.sync_records(repositories, &branches, cx);
        });
    }

    fn sync_active_branch(&self, window: &mut Window, cx: &mut Context<Self>) {
        let active_branch_id = self.active_branch_id();
        if let Some(sidebar) = self.sidebar.as_ref() {
            if let Some(branch_id) = active_branch_id {
                sidebar.update(cx, |sidebar, cx| sidebar.select_branch(branch_id, cx));
            } else {
                sidebar.update(cx, Sidebar::clear_selection);
            }
        }
        if let Some(branch_id) = active_branch_id
            && let Some(panel) = self.branch_panels.get(&branch_id)
        {
            panel.update(cx, |panel, cx| panel.activate(window, cx));
        }
    }

    fn release_branch_context(&mut self, branch_id: BranchId) {
        self.branch_context_headers.remove(&branch_id);
    }

    fn release_branch_resources(&mut self, branch_id: BranchId, cx: &mut Context<Self>) {
        self.release_branch_context(branch_id);
        self.unregister_branch_diff(branch_id, cx);
        self.branch_panels.remove(&branch_id);
    }

    fn release_branch_resources_and_notify(
        &mut self,
        branch_id: BranchId,
        released: async_channel::Sender<()>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        self.release_branch_context(branch_id);
        self.unregister_branch_diff(branch_id, cx);
        let Some(panel) = self.branch_panels.remove(&branch_id) else {
            let _ = released.try_send(());
            return Task::ready(());
        };

        let (panel_released_tx, panel_released_rx) = async_channel::bounded(1);
        let release_subscription = cx.observe_release(&panel, move |_, _, _| {
            let _ = panel_released_tx.try_send(());
        });
        drop(panel);

        cx.spawn(async move |_, _| {
            let _release_subscription = release_subscription;
            if panel_released_rx.recv().await.is_ok() {
                let _ = released.send(()).await;
            }
        })
    }

    fn register_branch_diff(
        &mut self,
        branch: &Branch,
        cx: &mut Context<Self>,
    ) -> Entity<RepositoryLiveDiff> {
        let live_diff = self
            .repository_live_diffs
            .entry(branch.repository_id)
            .or_insert_with(|| cx.new(|_| RepositoryLiveDiff::new()))
            .clone();
        live_diff.update(cx, |live_diff, cx| live_diff.register(branch, cx));
        if let Some(sidebar) = self.sidebar.as_ref() {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.observe_live_diff(branch.repository_id, live_diff.clone(), cx);
            });
        }
        live_diff
    }

    fn unregister_branch_diff(&mut self, branch_id: BranchId, cx: &mut Context<Self>) {
        let Some(repository_id) = self
            .workspace_model
            .as_ref()
            .and_then(|model| model.branch(branch_id))
            .map(|branch| branch.repository_id)
        else {
            return;
        };
        let Some(live_diff) = self.repository_live_diffs.get(&repository_id).cloned() else {
            return;
        };
        live_diff.update(cx, |live_diff, cx| live_diff.unregister(branch_id, cx));
        if live_diff.read(cx).is_empty() {
            self.repository_live_diffs.remove(&repository_id);
            if let Some(sidebar) = self.sidebar.as_ref() {
                sidebar.update(cx, |sidebar, _| sidebar.forget_live_diff(repository_id));
            }
        }
    }

    fn open_branch_deletion_dialog(
        &mut self,
        request: &DeleteBranchRequested,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.branch_deletion_dialog.is_some() || self.branch_dialog.is_some() {
            return;
        }
        let dialog = cx.new(|cx| {
            BranchDeletionDialog::new(request.repository.clone(), request.branch.clone(), cx)
        });
        self.branch_deletion_dialog_subscription = Some(cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &BranchDeletionDialogEvent, window, cx| match event {
                BranchDeletionDialogEvent::Dismissed => {
                    this.branch_deletion_dialog = None;
                    this.branch_deletion_dialog_subscription = None;
                    cx.notify();
                }
                BranchDeletionDialogEvent::Started {
                    branch_id,
                    released,
                } => {
                    let task =
                        this.release_branch_resources_and_notify(*branch_id, released.clone(), cx);
                    this.branch_release_tasks.insert(*branch_id, task);
                    cx.notify();
                }
                BranchDeletionDialogEvent::ProvenanceRecorded {
                    branch_id,
                    provenance,
                } => {
                    if let Some(model) = this.workspace_model.as_mut() {
                        model.record_worktree_provenance(*branch_id, provenance.clone());
                    }
                    this.sync_sidebar(cx);
                }
                BranchDeletionDialogEvent::Failed(branch_id) => {
                    this.branch_release_tasks.remove(branch_id);
                    this.select_branch(*branch_id, window, cx);
                }
                BranchDeletionDialogEvent::PartiallyDeleted(branch_id) => {
                    this.branch_release_tasks.remove(branch_id);
                    cx.notify();
                }
                BranchDeletionDialogEvent::Deleted(branch_id) => {
                    this.branch_release_tasks.remove(branch_id);
                    if let Some(model) = this.workspace_model.as_mut() {
                        model.delete_branch(*branch_id);
                    }
                    this.sync_branch_header(cx);
                    this.sync_sidebar(cx);
                    this.sync_active_branch(window, cx);
                    this.branch_deletion_dialog = None;
                    this.branch_deletion_dialog_subscription = None;
                    cx.notify();
                }
            },
        ));
        self.branch_deletion_dialog = Some(dialog);
        cx.notify();
    }

    fn open_branch_dialog(
        &mut self,
        request: &AddBranchRequested,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dialog =
            cx.new(|cx| BranchDialog::new(request.repository.clone(), request.position, cx));
        self.branch_dialog_subscription = Some(cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &BranchDialogEvent, window, cx| match event {
                BranchDialogEvent::Dismissed => {
                    this.branch_dialog = None;
                    this.branch_dialog_subscription = None;
                    cx.notify();
                }
                BranchDialogEvent::Created(branch) => {
                    let branch_id = branch.id;
                    if let Some(model) = this.workspace_model.as_mut() {
                        model.add_branch(branch.clone());
                    }
                    if let Some(sidebar) = this.sidebar.as_ref() {
                        sidebar.update(cx, |sidebar, _| {
                            sidebar.expand_repository(branch.repository_id);
                        });
                    }
                    this.sync_sidebar(cx);
                    this.select_branch(branch_id, window, cx);
                    this.branch_dialog = None;
                    this.branch_dialog_subscription = None;
                    cx.notify();
                }
            },
        ));
        self.branch_dialog = Some(dialog);
        cx.notify();
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_branch_id = self.active_branch_id();
        let active_panel =
            active_branch_id.and_then(|branch_id| self.branch_panels.get(&branch_id).cloned());
        let active_context_header = active_branch_id
            .and_then(|branch_id| self.branch_context_headers.get(&branch_id).cloned());
        let sidebar_open = self.sidebar_open;
        let generation = self.sidebar_animation_generation;
        let sidebar_end_width = if sidebar_open { SIDEBAR_WIDTH } else { 0.0 };
        let sidebar_start_width = if generation == 0 {
            sidebar_end_width
        } else if sidebar_open {
            0.0
        } else {
            SIDEBAR_WIDTH
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .text_color(cx.theme().colors().text)
            .font_family(UI_FONT)
            .text_size(px(UI_FONT_SIZE))
            .child(self.render_titlebar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when_some(self.sidebar.clone(), |body, sidebar| {
                        body.child(
                            div()
                                .id(("sidebar-width", generation))
                                .flex()
                                .flex_none()
                                .h_full()
                                .overflow_hidden()
                                .child(sidebar)
                                .with_animation(
                                    ("sidebar-body-width", generation),
                                    Animation::new(TITLEBAR_TRANSITION),
                                    move |element, delta| {
                                        element.w(px(interpolate_width(
                                            sidebar_start_width,
                                            sidebar_end_width,
                                            delta,
                                        )))
                                    },
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .min_h_0()
                                .when_some(active_context_header, |content, header| {
                                    content.child(header)
                                })
                                .child(
                                    div()
                                        .flex()
                                        .flex_1()
                                        .min_h_0()
                                        .items_center()
                                        .justify_center()
                                        .text_color(cx.theme().colors().text_muted)
                                        .when_some(active_panel, |content, panel| {
                                            content.items_stretch().justify_start().child(panel)
                                        })
                                        .when(active_branch_id.is_none(), |content| {
                                            content.child("Select a branch")
                                        }),
                                ),
                        )
                    })
                    .when(self.workspace_model.is_none(), |body| {
                        body.items_center()
                            .justify_center()
                            .child(self.render_onboarding(cx))
                    }),
            )
            .when_some(self.branch_dialog.clone(), |view, dialog| {
                view.child(
                    deferred(
                        div()
                            .absolute()
                            .inset_0()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(0x000000).opacity(0.48))
                            .child(dialog),
                    )
                    .with_priority(2),
                )
            })
            .when_some(self.branch_deletion_dialog.clone(), |view, dialog| {
                view.child(
                    deferred(
                        div()
                            .absolute()
                            .inset_0()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(0x000000).opacity(0.48))
                            .child(dialog),
                    )
                    .with_priority(2),
                )
            })
    }
}
