//! Void's native application entry point.

mod application;
mod assets;
mod branch_context_header;
mod branch_dialog;
mod branch_header;
#[cfg(target_os = "macos")]
mod macos_title_bar;
mod sidebar;
mod theme;
mod updater;

fn main() {
    application::run();
}
