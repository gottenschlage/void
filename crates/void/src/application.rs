//! GPUI application startup and the initial workspace.

use std::collections::HashMap;

use gpui::{
    App, Bounds, Context, Entity, Focusable, KeyBinding, MouseButton, MouseUpEvent, Role,
    Subscription, Window, WindowBounds, WindowOptions, actions, deferred, div, prelude::*, px, rgb,
    size,
};
use gpui_platform::application;
use void_terminal::{BranchTerminalPanel, TerminalSettings};
use workspace::{Branch, BranchId, VoidPaths, Workspace, WorkspaceDb};

use crate::{
    assets::Assets,
    branch_dialog::{BranchDialog, BranchDialogEvent, CancelBranch, ConfirmBranch},
    branch_header::{BranchClosed, BranchHeader, BranchSelected},
    sidebar::{AddBranchRequested, BranchArchived, Sidebar, SidebarRepository},
    text_input::{
        Backspace, Copy, Cut, Delete, Left, Paste, Right, SelectAll, SelectLeft, SelectRight,
        TextInput,
    },
    theme,
};

const INITIAL_WINDOW_WIDTH: f32 = 1_300.0;
const INITIAL_WINDOW_HEIGHT: f32 = 850.0;

actions!(workspace_onboarding, [Submit]);

/// Starts GPUI, opens Void's database, and creates the initial window.
pub(crate) fn run() {
    let paths = match VoidPaths::discover() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("failed to resolve Void's application-data directory: {error:#}");
            return;
        }
    };
    let workspace_db = match gpui::block_on(WorkspaceDb::open_default(&paths)) {
        Ok(database) => database,
        Err(error) => {
            eprintln!("failed to open Void's database: {error:#}");
            return;
        }
    };
    let initial_workspace = match workspace_db.first_workspace() {
        Ok(workspace) => workspace,
        Err(error) => {
            eprintln!("failed to load Void's workspace: {error:#}");
            return;
        }
    };
    let mut initial_repositories = Vec::new();
    let mut initial_branches = Vec::new();
    if let Some(workspace) = initial_workspace.as_ref() {
        let repositories = match workspace_db.repositories(workspace.id) {
            Ok(repositories) => repositories,
            Err(error) => {
                eprintln!("failed to load Void's repositories: {error:#}");
                return;
            }
        };
        for repository in repositories {
            let branches = match workspace_db.branches(repository.id) {
                Ok(branches) => branches,
                Err(error) => {
                    eprintln!("failed to load branches for {}: {error:#}", repository.name);
                    return;
                }
            };
            initial_branches.extend(
                branches
                    .iter()
                    .filter(|branch| branch.archived_at.is_none())
                    .cloned(),
            );
            initial_repositories.push(SidebarRepository::new(repository, branches));
        }
    }

    application().with_assets(Assets).run(move |cx: &mut App| {
        settings::init(cx);
        ::theme::init(::theme::LoadThemes::JustBase, cx);
        void_terminal::init(cx);
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, Some("TextInput")),
            KeyBinding::new("delete", Delete, Some("TextInput")),
            KeyBinding::new("left", Left, Some("TextInput")),
            KeyBinding::new("right", Right, Some("TextInput")),
            KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
            KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
            KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
            KeyBinding::new("cmd-c", Copy, Some("TextInput")),
            KeyBinding::new("cmd-v", Paste, Some("TextInput")),
            KeyBinding::new("cmd-x", Cut, Some("TextInput")),
            KeyBinding::new("ctrl-a", SelectAll, Some("TextInput")),
            KeyBinding::new("ctrl-c", Copy, Some("TextInput")),
            KeyBinding::new("ctrl-v", Paste, Some("TextInput")),
            KeyBinding::new("ctrl-x", Cut, Some("TextInput")),
            KeyBinding::new("enter", Submit, Some("WorkspaceOnboarding")),
            KeyBinding::new("enter", ConfirmBranch, Some("BranchDialog")),
            KeyBinding::new("escape", CancelBranch, Some("BranchDialog")),
        ]);
        cx.set_global(workspace_db);
        cx.set_global(paths);

        let bounds = Bounds::centered(
            None,
            size(px(INITIAL_WINDOW_WIDTH), px(INITIAL_WINDOW_HEIGHT)),
            cx,
        );

        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..WindowOptions::default()
            },
            move |window, cx| {
                window.set_rem_size(px(theme::UI_FONT_SIZE));
                let name_input = cx.new(|cx| TextInput::new("Workspace name", cx));
                if initial_workspace.is_none() {
                    name_input.focus_handle(cx).focus(window, cx);
                }
                let sidebar = initial_workspace.as_ref().map(|workspace| {
                    cx.new(|_| Sidebar::new(workspace.clone(), initial_repositories))
                });
                let branch_header = initial_workspace
                    .as_ref()
                    .map(|_| cx.new(|_| BranchHeader::new(Vec::new())));
                let branches = initial_branches
                    .into_iter()
                    .map(|branch| (branch.id, branch))
                    .collect();
                cx.new(|cx| {
                    VoidRoot::new(
                        initial_workspace,
                        sidebar,
                        branch_header,
                        branches,
                        name_input,
                        window,
                        cx,
                    )
                })
            },
        );

        if let Err(error) = window {
            eprintln!("failed to open Void's initial window: {error:#}");
            cx.quit();
            return;
        }

        cx.activate(true);
    });
}

struct VoidRoot {
    workspace: Option<Workspace>,
    sidebar: Option<Entity<Sidebar>>,
    branch_dialog: Option<Entity<BranchDialog>>,
    sidebar_subscription: Option<Subscription>,
    sidebar_branch_subscription: Option<Subscription>,
    sidebar_branch_archive_subscription: Option<Subscription>,
    branch_dialog_subscription: Option<Subscription>,
    branch_header: Option<Entity<BranchHeader>>,
    branch_header_subscription: Option<Subscription>,
    branch_close_subscription: Option<Subscription>,
    branches: HashMap<BranchId, Branch>,
    branch_panels: HashMap<BranchId, Entity<BranchTerminalPanel>>,
    active_branch_id: Option<BranchId>,
    name_input: Entity<TextInput>,
    error: Option<String>,
    is_creating: bool,
}

impl VoidRoot {
    fn new(
        workspace: Option<Workspace>,
        sidebar: Option<Entity<Sidebar>>,
        branch_header: Option<Entity<BranchHeader>>,
        branches: HashMap<BranchId, Branch>,
        name_input: Entity<TextInput>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            workspace,
            sidebar: None,
            branch_dialog: None,
            sidebar_subscription: None,
            sidebar_branch_subscription: None,
            sidebar_branch_archive_subscription: None,
            branch_dialog_subscription: None,
            branch_header: None,
            branch_header_subscription: None,
            branch_close_subscription: None,
            branches,
            branch_panels: HashMap::new(),
            active_branch_id: None,
            name_input,
            error: None,
            is_creating: false,
        };
        if let Some(sidebar) = sidebar {
            this.attach_sidebar(sidebar, window, cx);
        }
        if let Some(branch_header) = branch_header {
            this.attach_branch_header(branch_header, window, cx);
        }
        if let Some(branch_id) = this
            .branch_header
            .as_ref()
            .and_then(|header| header.read(cx).active_branch_id())
        {
            this.select_branch(branch_id, window, cx);
        }
        this
    }

    fn attach_sidebar(
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
            |this, _, event: &BranchArchived, _, cx| {
                if this.active_branch_id == Some(event.branch_id) {
                    this.active_branch_id = None;
                }
                this.branches.remove(&event.branch_id);
                this.branch_panels.remove(&event.branch_id);
                if let Some(branch_header) = this.branch_header.as_ref() {
                    branch_header.update(cx, |header, cx| header.archive(event.branch_id, cx));
                }
                cx.notify();
            },
        ));
        self.sidebar = Some(sidebar);
    }

    fn attach_branch_header(
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
            |this, header, event: &BranchClosed, window, cx| {
                if this.active_branch_id == Some(event.branch_id) {
                    this.active_branch_id = header.read(cx).active_branch_id();
                    if let Some(sidebar) = this.sidebar.as_ref() {
                        if let Some(branch_id) = this.active_branch_id {
                            sidebar.update(cx, |sidebar, cx| sidebar.select_branch(branch_id, cx));
                        } else {
                            sidebar.update(cx, Sidebar::clear_selection);
                        }
                    }
                }
                this.branch_panels.remove(&event.branch_id);
                if let Some(branch_id) = this.active_branch_id
                    && let Some(panel) = this.branch_panels.get(&branch_id)
                {
                    panel.update(cx, |panel, cx| panel.activate(window, cx));
                }
                cx.notify();
            },
        ));
        self.branch_header = Some(branch_header);
    }

    fn select_branch(&mut self, branch_id: BranchId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(branch) = self.branches.get(&branch_id).cloned() else {
            return;
        };
        self.active_branch_id = Some(branch_id);
        if let Some(sidebar) = self.sidebar.as_ref() {
            sidebar.update(cx, |sidebar, cx| sidebar.select_branch(branch_id, cx));
        }
        if let Some(branch_header) = self.branch_header.as_ref() {
            branch_header.update(cx, |header, cx| header.open(branch, cx));
        }
        let panel = self.branch_panels.entry(branch_id).or_insert_with(|| {
            cx.new(|_| {
                BranchTerminalPanel::new(
                    self.branches[&branch_id].path.clone(),
                    TerminalSettings::default(),
                )
            })
        });
        panel.update(cx, |panel, cx| panel.activate(window, cx));
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
                    this.branches.insert(branch_id, branch.clone());
                    if let Some(sidebar) = this.sidebar.as_ref() {
                        sidebar.update(cx, |sidebar, cx| sidebar.add_branch(branch.clone(), cx));
                    }
                    if let Some(branch_header) = this.branch_header.as_ref() {
                        branch_header
                            .update(cx, |header, cx| header.add_branch(branch.clone(), cx));
                    }
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

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        self.create_workspace(window, cx);
    }

    fn click_create(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.create_workspace(window, cx);
    }

    fn create_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace.is_some() || self.is_creating {
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
        let this = cx.entity();
        window
            .spawn(cx, async move |cx| {
                let result = database.create_workspace(name.clone()).await;
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.is_creating = false;
                        match result {
                            Ok(id) => {
                                let workspace = Workspace { id, name };
                                let sidebar =
                                    cx.new(|_| Sidebar::new(workspace.clone(), Vec::new()));
                                let branch_header = cx.new(|_| BranchHeader::new(Vec::new()));
                                this.attach_sidebar(sidebar, window, cx);
                                this.attach_branch_header(branch_header, window, cx);
                                this.workspace = Some(workspace);
                                this.branches.clear();
                                this.branch_panels.clear();
                            }
                            Err(error) => {
                                this.error = Some(format!("Could not create workspace: {error:#}"));
                            }
                        }
                        cx.notify();
                    })
                })
            })
            .detach();
    }

    fn render_onboarding(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("WorkspaceOnboarding")
            .on_action(cx.listener(Self::submit))
            .flex()
            .flex_col()
            .w(px(400.))
            .gap_4()
            .p_6()
            .rounded_lg()
            .bg(rgb(theme::ELEVATED_SURFACE))
            .border_1()
            .border_color(rgb(theme::BORDER))
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
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child("Name the workspace you will use in Void."),
                    ),
            )
            .child(self.name_input.clone())
            .when_some(self.error.clone(), |view, error| {
                view.child(div().text_xs().text_color(rgb(theme::ERROR)).child(error))
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
                        "Creating workspace…"
                    } else {
                        "Create workspace"
                    }),
            )
    }
}

impl Render for VoidRoot {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_panel = self
            .active_branch_id
            .and_then(|branch_id| self.branch_panels.get(&branch_id).cloned());
        div()
            .flex()
            .size_full()
            .bg(rgb(theme::EDITOR_BACKGROUND))
            .text_color(rgb(theme::TEXT))
            .font_family(theme::UI_FONT)
            .text_size(px(theme::UI_FONT_SIZE))
            .when_some(self.sidebar.clone(), |view, sidebar| {
                view.child(sidebar).child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .when_some(self.branch_header.clone(), |content, header| {
                            content.child(header)
                        })
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .min_h_0()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(theme::TEXT_MUTED))
                                .when_some(active_panel, |content, panel| {
                                    content.items_stretch().justify_start().child(panel)
                                })
                                .when(self.active_branch_id.is_none(), |content| {
                                    content.child("Select a branch")
                                }),
                        ),
                )
            })
            .when(self.workspace.is_none(), |view| {
                view.items_center()
                    .justify_center()
                    .child(self.render_onboarding(cx))
            })
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
    }
}
