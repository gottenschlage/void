//! Shared UI scale and proportional component sizes.

use gpui::{App, Global, KeyBinding, Rems, actions, rems};

/// Void's default root rem size in pixels.
pub const DEFAULT_UI_SCALE: f32 = 14.0;
const MIN_UI_SCALE: f32 = 12.0;
const MAX_UI_SCALE: f32 = 24.0;

#[derive(Clone, Copy)]
struct UiScale(f32);

impl Global for UiScale {}

actions!(ui_scale, [IncreaseUiScale, DecreaseUiScale, ResetUiScale]);

/// A proportional component size relative to the root UI scale.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ComponentSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

/// Converts a measurement from Void's 14 px design baseline into rems.
pub fn scaled(value: f32) -> Rems {
    rems(value / DEFAULT_UI_SCALE)
}

impl ComponentSize {
    /// Returns the base size represented by this step.
    pub fn rems(self) -> Rems {
        rems(match self {
            Self::Xs => 0.75,
            Self::Sm => 0.875,
            Self::Md => 1.0,
            Self::Lg => 1.125,
            Self::Xl => 1.25,
        })
    }
}

/// Registers the root scale and platform zoom shortcuts.
pub fn init(cx: &mut App) {
    cx.set_global(UiScale(DEFAULT_UI_SCALE));
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-=", IncreaseUiScale, None),
        KeyBinding::new("cmd-+", IncreaseUiScale, None),
        KeyBinding::new("cmd--", DecreaseUiScale, None),
        KeyBinding::new("cmd-0", ResetUiScale, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-=", IncreaseUiScale, None),
        KeyBinding::new("ctrl-+", IncreaseUiScale, None),
        KeyBinding::new("ctrl--", DecreaseUiScale, None),
        KeyBinding::new("ctrl-0", ResetUiScale, None),
    ]);
}

/// Returns the current root rem size in pixels.
pub fn ui_scale(cx: &App) -> f32 {
    cx.global::<UiScale>().0
}

/// Increases the transient root UI scale by one pixel.
pub fn increase_ui_scale(cx: &mut App) {
    adjust_ui_scale(cx, 1.0);
}

/// Decreases the transient root UI scale by one pixel.
pub fn decrease_ui_scale(cx: &mut App) {
    adjust_ui_scale(cx, -1.0);
}

/// Restores the root UI scale to its configured default.
pub fn reset_ui_scale(cx: &mut App) {
    cx.set_global(UiScale(DEFAULT_UI_SCALE));
    cx.refresh_windows();
}

fn adjust_ui_scale(cx: &mut App, delta: f32) {
    let scale = (ui_scale(cx) + delta).clamp(MIN_UI_SCALE, MAX_UI_SCALE);
    cx.set_global(UiScale(scale));
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    use super::ComponentSize;

    #[test]
    fn component_sizes_are_strictly_ordered() {
        let sizes = [
            ComponentSize::Xs,
            ComponentSize::Sm,
            ComponentSize::Md,
            ComponentSize::Lg,
            ComponentSize::Xl,
        ]
        .map(|size| size.rems().0);

        assert!(sizes.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
