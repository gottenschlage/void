//! GPUI application startup and the initial root view.
//!
//! This module intentionally contains only the minimum application shell. Product
//! surfaces belong in focused crates as their responsibilities become concrete.

use gpui::{
    App, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;
use workspace::{VoidPaths, WorkspaceDb};

const INITIAL_WINDOW_WIDTH: f32 = 1_300.0;
const INITIAL_WINDOW_HEIGHT: f32 = 800.0;

/// Starts GPUI, creates Void's first window, and hands its root view to GPUI.
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

    application().run(move |cx: &mut App| {
        cx.set_global(workspace_db);

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
            |_, cx| cx.new(|_| VoidRoot),
        );

        if let Err(error) = window {
            eprintln!("failed to open Void's initial window: {error:#}");
            cx.quit();
            return;
        }

        cx.activate(true);
    });
}

/// Temporary root view used to verify the native GPUI application shell.
struct VoidRoot;

impl Render for VoidRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(rgb(0x111318))
            .text_color(rgb(0xe6e8eb))
            .child(div().text_xl().child("Void"))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x9298a1))
                    .child("GPUI application scaffold"),
            )
    }
}
