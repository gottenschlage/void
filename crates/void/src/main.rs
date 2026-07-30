//! Void's native application entry point.

mod application;
mod assets;
mod branch_dialog;
mod branch_header;
mod icons;
mod sidebar;
mod text_input;
mod theme;
mod updater;

fn main() {
    application::run();
}
