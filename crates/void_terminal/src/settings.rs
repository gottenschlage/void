//! Terminal presentation and process defaults.

use gpui::{App, Global};
use terminal::terminal_settings::{AlternateScroll, CursorShape};
use theme::ActiveTheme as _;

pub(crate) const DEFAULT_TERMINAL_FONT_SIZE: f32 = 13.0;
const MIN_TERMINAL_FONT_SIZE: f32 = 8.0;
const MAX_TERMINAL_FONT_SIZE: f32 = 32.0;

#[derive(Clone, Copy)]
struct TerminalFontSizeAdjustment(f32);

impl Global for TerminalFontSizeAdjustment {}

pub(crate) fn init(cx: &mut App) {
    cx.set_global(TerminalFontSizeAdjustment(0.0));
}

pub(crate) fn terminal_font_size(configured_size: f32, cx: &App) -> f32 {
    let adjustment = cx
        .try_global::<TerminalFontSizeAdjustment>()
        .map_or(0.0, |adjustment| adjustment.0);
    adjusted_terminal_font_size(configured_size, adjustment)
}

pub(crate) fn increase_terminal_font_size(cx: &mut App) {
    adjust_terminal_font_size(cx, 1.0);
}

pub(crate) fn decrease_terminal_font_size(cx: &mut App) {
    adjust_terminal_font_size(cx, -1.0);
}

pub(crate) fn reset_terminal_font_size(cx: &mut App) {
    cx.set_global(TerminalFontSizeAdjustment(0.0));
    cx.refresh_windows();
}

fn adjust_terminal_font_size(cx: &mut App, delta: f32) {
    let current = terminal_font_size(DEFAULT_TERMINAL_FONT_SIZE, cx);
    let next = (current + delta).clamp(MIN_TERMINAL_FONT_SIZE, MAX_TERMINAL_FONT_SIZE);
    cx.set_global(TerminalFontSizeAdjustment(
        next - DEFAULT_TERMINAL_FONT_SIZE,
    ));
    cx.refresh_windows();
}

fn adjusted_terminal_font_size(configured_size: f32, adjustment: f32) -> f32 {
    (configured_size + adjustment).clamp(MIN_TERMINAL_FONT_SIZE, MAX_TERMINAL_FONT_SIZE)
}

/// Code-configurable terminal presentation and process defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub cursor_shape: CursorShape,
    pub cursor_blinks: bool,
    pub cursor_color: u32,
    pub foreground: u32,
    pub max_scroll_history_lines: usize,
    pub background: Option<u32>,
    pub selection_background: u32,
    pub selection_foreground: u32,
    pub alternate_scroll: AlternateScroll,
    pub option_as_meta: bool,
    pub ansi: [u32; 16],
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: "JetBrains Mono".into(),
            font_size: DEFAULT_TERMINAL_FONT_SIZE,
            line_height: 18. / 13.,
            cursor_shape: CursorShape::Underline,
            cursor_blinks: true,
            cursor_color: 0xcccccc,
            foreground: 0xcccccc,
            max_scroll_history_lines: 10_000,
            background: None,
            selection_background: 0x264f78,
            selection_foreground: 0xffffff,
            alternate_scroll: AlternateScroll::On,
            option_as_meta: false,
            ansi: [
                0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5,
                0x666666, 0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff,
            ],
        }
    }
}

impl TerminalSettings {
    pub(crate) fn themed(&self, cx: &App) -> Self {
        let colors = cx.theme().colors();
        Self {
            cursor_color: rgb_value(colors.terminal_foreground),
            foreground: rgb_value(colors.terminal_foreground),
            background: Some(rgb_value(colors.terminal_background)),
            selection_background: rgb_value(colors.element_selection_background),
            selection_foreground: rgb_value(colors.terminal_foreground),
            ansi: [
                colors.terminal_ansi_black,
                colors.terminal_ansi_red,
                colors.terminal_ansi_green,
                colors.terminal_ansi_yellow,
                colors.terminal_ansi_blue,
                colors.terminal_ansi_magenta,
                colors.terminal_ansi_cyan,
                colors.terminal_ansi_white,
                colors.terminal_ansi_bright_black,
                colors.terminal_ansi_bright_red,
                colors.terminal_ansi_bright_green,
                colors.terminal_ansi_bright_yellow,
                colors.terminal_ansi_bright_blue,
                colors.terminal_ansi_bright_magenta,
                colors.terminal_ansi_bright_cyan,
                colors.terminal_ansi_bright_white,
            ]
            .map(rgb_value),
            ..self.clone()
        }
    }
}

fn rgb_value(color: gpui::Hsla) -> u32 {
    u32::from(color.to_rgb()) >> 8
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_TERMINAL_FONT_SIZE, MIN_TERMINAL_FONT_SIZE, TerminalSettings,
        adjusted_terminal_font_size,
    };
    use terminal::terminal_settings::CursorShape;

    #[test]
    fn desktop_defaults_are_stable() {
        let settings = TerminalSettings::default();
        assert_eq!(settings.font_family, "JetBrains Mono");
        assert_eq!(settings.font_size, 13.);
        assert_eq!(settings.cursor_shape, CursorShape::Underline);
        assert!(settings.cursor_blinks);
        assert_eq!(settings.max_scroll_history_lines, 10_000);
    }

    #[test]
    fn adjusted_font_size_is_clamped() {
        assert_eq!(
            [
                adjusted_terminal_font_size(13., -20.),
                adjusted_terminal_font_size(13., 30.),
            ],
            [MIN_TERMINAL_FONT_SIZE, MAX_TERMINAL_FONT_SIZE]
        );
    }
}
