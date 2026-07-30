//! Void's native application entry point.

mod application;
mod assets;
mod branch_dialog;
mod branch_header;
mod icons;
#[cfg(target_os = "macos")]
mod macos_title_bar;
mod sidebar;
mod text_input;
mod theme;
mod updater;

fn main() {
    application::run();
}
