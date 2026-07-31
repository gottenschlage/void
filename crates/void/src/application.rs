//! GPUI application startup and top-level composition.

use anyhow::Context as _;
use gpui::{
    App, Bounds, Context, Entity, Render, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, point, prelude::*, px, size,
};
use gpui_platform::application;
use void_update::Updater;
use workspace::{VoidPaths, WorkspaceDb, WorkspaceModel, WorkspaceView};

use crate::{assets::Assets, theme};

const INITIAL_WINDOW_WIDTH: f32 = 1_300.0;
const INITIAL_WINDOW_HEIGHT: f32 = 850.0;
const TRAFFIC_LIGHT_X: f32 = 16.0;
const TRAFFIC_LIGHT_Y: f32 = 11.0;

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
    let initial_model = match load_initial_model(&workspace_db) {
        Ok(model) => model,
        Err(error) => {
            eprintln!("failed to load Void's workspace: {error:#}");
            return;
        }
    };

    application().with_assets(Assets).run(move |cx: &mut App| {
        settings::init(cx);
        if let Err(error) = theme::init(cx) {
            eprintln!("failed to initialize Void's theme: {error:#}");
            cx.quit();
            return;
        }
        ui::init(cx);
        void_terminal::init(cx);
        workspace::init(cx);
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
                let workspace = cx.new(|cx| WorkspaceView::new(initial_model, window, cx));
                let updater =
                    cx.new(|cx| Updater::new(env!("CARGO_PKG_VERSION"), cx.app_path().ok(), cx));
                updater.update(cx, |updater, cx| updater.start(cx));
                cx.new(|cx| AppView::new(workspace, updater, window, cx))
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

fn load_initial_model(database: &WorkspaceDb) -> anyhow::Result<Option<WorkspaceModel>> {
    let Some(workspace) = database.first_workspace()? else {
        return Ok(None);
    };
    let repositories = database.repositories(workspace.id)?;
    let mut branches = Vec::new();
    for repository in &repositories {
        branches.extend(
            database
                .branches(repository.id)
                .with_context(|| format!("failed to load branches for {}", repository.name))?,
        );
    }
    Ok(Some(WorkspaceModel::new(workspace, repositories, branches)))
}

struct AppView {
    workspace: Entity<WorkspaceView>,
    updater: Entity<Updater>,
    _appearance_subscription: Subscription,
}

impl AppView {
    fn new(
        workspace: Entity<WorkspaceView>,
        updater: Entity<Updater>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance_subscription = cx.observe_window_appearance(window, |_, window, cx| {
            if let Err(error) = theme::sync_system_appearance(window.appearance(), cx) {
                eprintln!("failed to follow the system appearance: {error:#}");
            }
        });
        Self {
            workspace,
            updater,
            _appearance_subscription: appearance_subscription,
        }
    }

    fn increase_ui_scale(
        &mut self,
        _: &ui::IncreaseUiScale,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        ui::increase_ui_scale(cx);
    }

    fn decrease_ui_scale(
        &mut self,
        _: &ui::DecreaseUiScale,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        ui::decrease_ui_scale(cx);
    }

    fn reset_ui_scale(&mut self, _: &ui::ResetUiScale, _: &mut Window, cx: &mut Context<Self>) {
        ui::reset_ui_scale(cx);
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(px(ui::ui_scale(cx)));
        div()
            .size_full()
            .on_action(cx.listener(Self::increase_ui_scale))
            .on_action(cx.listener(Self::decrease_ui_scale))
            .on_action(cx.listener(Self::reset_ui_scale))
            .child(self.workspace.clone())
            .child(self.updater.clone())
    }
}
