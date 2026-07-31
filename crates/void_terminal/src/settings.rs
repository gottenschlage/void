//! Terminal presentation and process defaults.

use terminal::terminal_settings::{AlternateScroll, CursorShape};

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
            font_size: 13.,
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

#[cfg(test)]
mod tests {
    use super::TerminalSettings;
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
}
