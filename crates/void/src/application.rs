//! GPUI application startup and the initial workspace.

use std::{collections::HashMap, time::Duration};

use gpui::{
    Animation, AnimationExt, App, Bounds, Context, Entity, Focusable, KeyBinding, MouseButton,
    MouseDownEvent, MouseUpEvent, Role, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowControlArea, WindowOptions, actions, deferred, div, point, prelude::*, px, rgb, size,
};
use gpui_platform::application;
use ui::{
    Backspace, Copy, Cut, Delete, Left, Paste, Right, SelectAll, SelectLeft, SelectRight,
    TextInput, icon_sized,
};
use void_terminal::{BranchTerminalPanel, TerminalSettings};
use workspace::{Branch, BranchId, RepositoryId, VoidPaths, Workspace, WorkspaceDb};

use crate::{
    assets::Assets,
    branch_context_header::{BranchContextHeader, RepositoryLiveDiff},
    branch_dialog::{BranchDialog, BranchDialogEvent, CancelBranch, ConfirmBranch},
    branch_header::{BranchClosed, BranchHeader, BranchSelected, HEADER_HEIGHT},
    sidebar::{AddBranchRequested, BranchArchived, Sidebar, SidebarRepository},
    theme,
    updater::Updater,
};
use ::theme::ActiveTheme as _;

const INITIAL_WINDOW_WIDTH: f32 = 1_300.0;
const INITIAL_WINDOW_HEIGHT: f32 = 850.0;
const SIDEBAR_WIDTH: f32 = 240.0;
const COLLAPSED_TITLEBAR_WIDTH: f32 = 48.0;
const TITLEBAR_TRANSITION: Duration = Duration::from_millis(200);
const TRAFFIC_LIGHT_X: f32 = 16.0;
const TRAFFIC_LIGHT_Y: f32 = 11.0;

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
        theme::init(cx);
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
                #[cfg(target_os = "macos")]
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(TRAFFIC_LIGHT_X), px(TRAFFIC_LIGHT_Y))),
                }),
                #[cfg(target_os = "macos")]
                is_movable: true,
                #[cfg(target_os = "macos")]
                app_owns_titlebar_drag: true,
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
    repository_live_diffs: HashMap<RepositoryId, Entity<RepositoryLiveDiff>>,
    branch_context_headers: HashMap<BranchId, Entity<BranchContextHeader>>,
    branch_panels: HashMap<BranchId, Entity<BranchTerminalPanel>>,
    active_branch_id: Option<BranchId>,
    name_input: Entity<TextInput>,
    updater: Entity<Updater>,
    error: Option<String>,
    is_creating: bool,
    sidebar_open: bool,
    sidebar_animation_generation: usize,
    should_move_window: bool,
    was_fullscreen: bool,
    window_bounds_subscription: Option<Subscription>,
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
        let updater = cx.new(|cx| Updater::new(cx.app_path(), cx));
        updater.update(cx, |updater, cx| updater.start(cx));
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
            repository_live_diffs: HashMap::new(),
            branch_context_headers: HashMap::new(),
            branch_panels: HashMap::new(),
            active_branch_id: None,
            name_input,
            updater,
            error: None,
            is_creating: false,
            sidebar_open: default_sidebar_open(),
            sidebar_animation_generation: 0,
            should_move_window: false,
            was_fullscreen: window.is_fullscreen(),
            window_bounds_subscription: None,
        };
        if let Some(sidebar) = sidebar {
            this.attach_sidebar(sidebar, window, cx);
        }
        for branch in this.branches.values().cloned().collect::<Vec<_>>() {
            this.register_branch_diff(&branch, cx);
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

    fn toggle_sidebar(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        self.sidebar_animation_generation = self.sidebar_animation_generation.wrapping_add(1);
        self.sync_traffic_lights(window);
        cx.stop_propagation();
        cx.notify();
    }

    fn stop_titlebar_drag(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.should_move_window = false;
        cx.stop_propagation();
    }

    fn titlebar_mouse_down(&mut self, _: &MouseDownEvent, _: &mut Window, _: &mut Context<Self>) {
        self.should_move_window = true;
    }

    fn titlebar_mouse_move(
        &mut self,
        _: &gpui::MouseMoveEvent,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        if self.should_move_window {
            self.should_move_window = false;
            window.start_window_move();
        }
    }

    fn sync_traffic_lights(&self, window: &Window) {
        #[cfg(target_os = "macos")]
        {
            let visible = traffic_lights_visible(self.sidebar_open, window.is_fullscreen());
            if let Err(error) = crate::macos_title_bar::set_traffic_lights_visible(window, visible)
            {
                eprintln!("could not update macOS traffic-light visibility: {error}");
            }
            if visible {
                window.set_traffic_light_position(point(px(TRAFFIC_LIGHT_X), px(TRAFFIC_LIGHT_Y)));
            }
        }
    }

    fn render_titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_open = self.sidebar_open;
        let generation = self.sidebar_animation_generation;
        let has_sidebar = self.sidebar.is_some();
        let start_width = if generation == 0 {
            titlebar_leading_width(sidebar_open)
        } else if sidebar_open {
            COLLAPSED_TITLEBAR_WIDTH
        } else {
            SIDEBAR_WIDTH
        };
        let end_width = titlebar_leading_width(sidebar_open);

        div()
            .id("void-titlebar")
            .window_control_area(WindowControlArea::Drag)
            .flex()
            .h(px(HEADER_HEIGHT))
            .flex_none()
            .bg(cx.theme().colors().surface_background)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::titlebar_mouse_down))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.should_move_window = false),
            )
            .on_mouse_move(cx.listener(Self::titlebar_mouse_move))
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
            .child(
                div()
                    .id(("titlebar-leading", generation))
                    .flex()
                    .flex_none()
                    .h_full()
                    .items_center()
                    .justify_end()
                    .pr_2()
                    .overflow_hidden()
                    .border_b_1()
                    .border_r_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        div()
                            .id("toggle-sidebar")
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label(if sidebar_open {
                                "Close sidebar"
                            } else {
                                "Open sidebar"
                            })
                            .flex()
                            .size(px(28.))
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .when(has_sidebar, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|button| button.bg(cx.theme().colors().element_hover))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(Self::stop_titlebar_drag),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(Self::toggle_sidebar),
                                    )
                            })
                            .when(!has_sidebar, |button| button.opacity(0.35))
                            .child(icon_sized(
                                "icons/panel-left.svg",
                                16.,
                                cx.theme().colors().text_muted,
                            )),
                    )
                    .with_animation(
                        ("sidebar-titlebar-width", generation),
                        Animation::new(TITLEBAR_TRANSITION),
                        move |element, delta| {
                            element.w(px(interpolate_width(start_width, end_width, delta)))
                        },
                    ),
            )
            .when_some(self.branch_header.clone(), |titlebar, header| {
                titlebar.child(div().min_w_0().flex_1().h_full().child(header))
            })
            .when(self.branch_header.is_none(), |titlebar| {
                titlebar.child(
                    div()
                        .flex_1()
                        .h_full()
                        .border_b_1()
                        .border_color(cx.theme().colors().border_variant),
                )
            })
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
                this.release_branch_context(event.branch_id);
                this.unregister_branch_diff(event.branch_id, cx);
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
                this.release_branch_context(event.branch_id);
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
            branch_header.update(cx, |header, cx| header.open(branch.clone(), cx));
        }
        let live_diff = self.register_branch_diff(&branch, cx);
        self.branch_context_headers
            .entry(branch_id)
            .or_insert_with(|| {
                cx.new(|cx| BranchContextHeader::new(branch.clone(), live_diff, cx))
            });
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

    fn release_branch_context(&mut self, branch_id: BranchId) {
        self.branch_context_headers.remove(&branch_id);
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
            .branches
            .get(&branch_id)
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
            .bg(cx.theme().colors().elevated_surface_background)
            .border_1()
            .border_color(cx.theme().colors().border)
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
                            .text_color(cx.theme().colors().text_muted)
                            .child("Name the workspace you will use in Void."),
                    ),
            )
            .child(self.name_input.clone())
            .when_some(self.error.clone(), |view, error| {
                view.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().status().error)
                        .child(error),
                )
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
                    .bg(cx.theme().colors().text_accent)
                    .text_sm()
                    .text_color(cx.theme().colors().editor_background)
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
        let active_context_header = self
            .active_branch_id
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
            .font_family(theme::UI_FONT)
            .text_size(px(theme::UI_FONT_SIZE))
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
                                        .when(self.active_branch_id.is_none(), |content| {
                                            content.child("Select a branch")
                                        }),
                                ),
                        )
                    })
                    .when(self.workspace.is_none(), |body| {
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
            .child(self.updater.clone())
    }
}

fn interpolate_width(start: f32, end: f32, delta: f32) -> f32 {
    start + (end - start) * delta.clamp(0.0, 1.0)
}

fn default_sidebar_open() -> bool {
    true
}

fn titlebar_leading_width(sidebar_open: bool) -> f32 {
    if sidebar_open {
        SIDEBAR_WIDTH
    } else {
        COLLAPSED_TITLEBAR_WIDTH
    }
}

fn traffic_lights_visible(sidebar_open: bool, fullscreen: bool) -> bool {
    sidebar_open || fullscreen
}

#[cfg(test)]
mod tests {
    use super::{
        COLLAPSED_TITLEBAR_WIDTH, SIDEBAR_WIDTH, default_sidebar_open, interpolate_width,
        titlebar_leading_width, traffic_lights_visible,
    };

    #[test]
    fn sidebar_defaults_and_width_end_states_are_stable() {
        assert!(default_sidebar_open());
        assert_eq!(titlebar_leading_width(true), SIDEBAR_WIDTH);
        assert_eq!(titlebar_leading_width(false), COLLAPSED_TITLEBAR_WIDTH);
        assert_eq!(interpolate_width(0.0, SIDEBAR_WIDTH, 0.0), 0.0);
        assert_eq!(interpolate_width(0.0, SIDEBAR_WIDTH, 1.0), SIDEBAR_WIDTH);
        assert_eq!(interpolate_width(SIDEBAR_WIDTH, 0.0, 1.0), 0.0);
    }

    #[test]
    fn interpolation_clamps_to_reduced_motion_end_state() {
        assert_eq!(
            interpolate_width(0.0, SIDEBAR_WIDTH, f32::INFINITY),
            SIDEBAR_WIDTH
        );
    }

    #[test]
    fn traffic_lights_follow_sidebar_and_fullscreen_policy() {
        assert!(traffic_lights_visible(true, false));
        assert!(!traffic_lights_visible(false, false));
        assert!(traffic_lights_visible(false, true));
    }
}
