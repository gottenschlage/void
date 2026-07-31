#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

//! Void's native application entry point.

mod application;
mod assets;
mod theme;

fn main() {
    application::run();
}
