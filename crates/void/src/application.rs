//! GPUI application startup and the initial workspace onboarding.

use gpui::{
    App, Bounds, Context, Entity, Focusable, KeyBinding, MouseButton, MouseUpEvent, Role,
    Subscription, Window, WindowBounds, WindowOptions, actions, deferred, div, prelude::*, px, rgb,
    size,
};
use gpui_platform::application;
use workspace::{VoidPaths, Workspace, WorkspaceDb};

use crate::{
    assets::Assets,
    branch_dialog::{BranchDialog, BranchDialogEvent, CancelBranch, ConfirmBranch},
    sidebar::{AddBranchRequested, Sidebar, SidebarRepository},
    text_input::{
        Backspace, Copy, Cut, Delete, Left, Paste, Right, SelectAll, SelectLeft, SelectRight,
        TextInput,
    },
    theme,
};

const INITIAL_WINDOW_WIDTH: f32 = 1_300.0;
const INITIAL_WINDOW_HEIGHT: f32 = 800.0;

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
            initial_repositories.push(SidebarRepository::new(repository, branches));
        }
    }

    application().with_assets(Assets).run(move |cx: &mut App| {
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
                let name_input = cx.new(|cx| TextInput::new("Workspace name", cx));
                if initial_workspace.is_none() {
                    name_input.focus_handle(cx).focus(window, cx);
                }
                let sidebar = initial_workspace.as_ref().map(|workspace| {
                    cx.new(|_| Sidebar::new(workspace.clone(), initial_repositories))
                });
                cx.new(|cx| VoidRoot::new(initial_workspace, sidebar, name_input, cx))
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
    branch_dialog_subscription: Option<Subscription>,
    name_input: Entity<TextInput>,
    error: Option<String>,
    is_creating: bool,
}

impl VoidRoot {
    fn new(
        workspace: Option<Workspace>,
        sidebar: Option<Entity<Sidebar>>,
        name_input: Entity<TextInput>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            workspace,
            sidebar: None,
            branch_dialog: None,
            sidebar_subscription: None,
            branch_dialog_subscription: None,
            name_input,
            error: None,
            is_creating: false,
        };
        if let Some(sidebar) = sidebar {
            this.attach_sidebar(sidebar, cx);
        }
        this
    }

    fn attach_sidebar(&mut self, sidebar: Entity<Sidebar>, cx: &mut Context<Self>) {
        self.sidebar_subscription = Some(cx.subscribe(
            &sidebar,
            |this, _, request: &AddBranchRequested, cx| {
                this.open_branch_dialog(request, cx);
            },
        ));
        self.sidebar = Some(sidebar);
    }

    fn open_branch_dialog(&mut self, request: &AddBranchRequested, cx: &mut Context<Self>) {
        let dialog =
            cx.new(|cx| BranchDialog::new(request.repository.clone(), request.position, cx));
        self.branch_dialog_subscription = Some(cx.subscribe(
            &dialog,
            |this, _, event: &BranchDialogEvent, cx| match event {
                BranchDialogEvent::Dismissed => {
                    this.branch_dialog = None;
                    this.branch_dialog_subscription = None;
                    cx.notify();
                }
                BranchDialogEvent::Created(branch) => {
                    if let Some(sidebar) = this.sidebar.as_ref() {
                        sidebar.update(cx, |sidebar, cx| sidebar.add_branch(branch.clone(), cx));
                    }
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

    fn create_workspace(&mut self, _: &mut Window, cx: &mut Context<Self>) {
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
        cx.spawn(async move |this, cx| {
            let result = database.create_workspace(name.clone()).await;
            this.update(cx, |this, cx| {
                this.is_creating = false;
                match result {
                    Ok(id) => {
                        let workspace = Workspace { id, name };
                        let sidebar = cx.new(|_| Sidebar::new(workspace.clone(), Vec::new()));
                        this.attach_sidebar(sidebar, cx);
                        this.workspace = Some(workspace);
                    }
                    Err(error) => {
                        this.error = Some(format!("Could not create workspace: {error:#}"));
                    }
                }
                cx.notify();
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
        div()
            .flex()
            .size_full()
            .bg(rgb(theme::EDITOR_BACKGROUND))
            .text_color(rgb(theme::TEXT))
            .text_sm()
            .when_some(self.sidebar.clone(), |view, sidebar| {
                view.child(sidebar).child(
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(theme::TEXT_MUTED))
                        .child("Select a branch"),
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
