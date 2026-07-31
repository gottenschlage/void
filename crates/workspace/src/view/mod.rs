mod branches;
#[cfg(target_os = "macos")]
mod macos_title_bar;
mod onboarding;
mod sidebar;
mod title_bar;
mod workspace;

use gpui::{App, KeyBinding};
use ui::{Backspace, Copy, Cut, Delete, Left, Paste, Right, SelectAll, SelectLeft, SelectRight};

pub use workspace::{UI_FONT_SIZE, WorkspaceView};

/// Registers key bindings owned by workspace views and dialogs.
pub fn init(cx: &mut App) {
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
        KeyBinding::new("enter", onboarding::Submit, Some("WorkspaceOnboarding")),
        KeyBinding::new("enter", branches::ConfirmBranch, Some("BranchDialog")),
        KeyBinding::new("escape", branches::CancelBranch, Some("BranchDialog")),
        KeyBinding::new(
            "enter",
            branches::ConfirmDeletion,
            Some("BranchDeletionDialog"),
        ),
        KeyBinding::new(
            "escape",
            branches::CancelDeletion,
            Some("BranchDeletionDialog"),
        ),
    ]);
}
