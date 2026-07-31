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

//! Authenticated stable-channel updates for signed Apple-silicon app bundles.
//!
//! [`Updater`] owns polling, download, and installation. Releasing the entity
//! cancels its active attempt; mounted disk images retain independent cleanup
//! ownership so cancellation cannot leak a mount.

mod download;
mod macos;
mod manifest;
mod status_view;
mod updater;

pub use updater::Updater;
