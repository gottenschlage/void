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

//! PTY-backed terminal sessions and branch-local tab ownership.
//!
//! A [`BranchTerminalPanel`] owns every terminal process for one open branch.
//! Releasing the panel releases its Zed `terminal::Terminal` entities, whose
//! lifecycle terminates the corresponding process trees.

use gpui::{App, KeyBinding, actions};

mod panel;
mod session;
mod settings;
mod tabs;
mod terminal_element;

pub use panel::BranchTerminalPanel;
use session::TerminalSession;
pub use settings::TerminalSettings;
use settings::init as init_settings;
pub use tabs::TerminalId;
use tabs::TerminalTabs;

actions!(
    void_terminal,
    [
        Copy,
        Paste,
        IncreaseTerminalFontSize,
        DecreaseTerminalFontSize,
        ResetTerminalFontSize
    ]
);

/// Registers terminal-specific platform keybindings.
pub fn init(cx: &mut App) {
    init_settings(cx);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-c", Copy, Some("Terminal")),
        KeyBinding::new("cmd-v", Paste, Some("Terminal")),
        KeyBinding::new("cmd-=", IncreaseTerminalFontSize, Some("Terminal")),
        KeyBinding::new("cmd-+", IncreaseTerminalFontSize, Some("Terminal")),
        KeyBinding::new("cmd--", DecreaseTerminalFontSize, Some("Terminal")),
        KeyBinding::new("cmd-0", ResetTerminalFontSize, Some("Terminal")),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-c", Copy, Some("Terminal")),
        KeyBinding::new("ctrl-shift-v", Paste, Some("Terminal")),
        KeyBinding::new("ctrl-=", IncreaseTerminalFontSize, Some("Terminal")),
        KeyBinding::new("ctrl-+", IncreaseTerminalFontSize, Some("Terminal")),
        KeyBinding::new("ctrl--", DecreaseTerminalFontSize, Some("Terminal")),
        KeyBinding::new("ctrl-0", ResetTerminalFontSize, Some("Terminal")),
    ]);
}
