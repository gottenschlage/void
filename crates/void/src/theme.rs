//! Void's active theme: a refinement of Zed's bundled "One Dark" theme.
//!
//! `::theme::init` (called at startup in `application.rs`) loads Zed's real
//! [`theme::Theme`]/[`theme::ThemeColors`] system and installs Zed's own
//! default dark theme as the active [`theme::GlobalTheme`]. Void has no theme
//! extension or `theme_settings` crate, so rather than authoring a full
//! 150+ field `Theme` from scratch, [`init`] takes that verified default and
//! refines only the tokens Void's UI renders — surfaces, borders, elements,
//! and text — leaving syntax, status, vim, and diff colors as Zed's defaults
//! until Void has a use for them.
//!
//! Call sites read colors through [`theme::ActiveTheme::theme`], e.g.
//! `cx.theme().colors().border`, never through hardcoded constants.

use std::sync::Arc;

use gpui::{App, rgb};
use refineable::Refineable as _;
use theme::{
    DEFAULT_DARK_THEME, GlobalTheme, Theme, ThemeColorsRefinement, ThemeRegistry, ThemeStyles,
};

/// The font used for UI chrome: sidebar, tabs, titlebar, menus.
pub(crate) const UI_FONT: &str = "JetBrains Mono";
pub(crate) const UI_FONT_SIZE: f32 = 13.0;

/// Installs Void's palette as the active theme.
///
/// Must run after `::theme::init`, which registers `DEFAULT_DARK_THEME` and
/// makes it the active [`GlobalTheme`].
pub(crate) fn init(cx: &mut App) {
    let registry = ThemeRegistry::default_global(cx);
    let base = registry
        .get(DEFAULT_DARK_THEME)
        .expect("theme::init registers Zed's bundled default dark theme");

    let transparent = base.colors().border_transparent;
    let colors = base.colors().clone().refined(ThemeColorsRefinement {
        border: Some(rgb(0x2e2e2e).into()),
        border_variant: Some(rgb(0x2e2e2e).into()),
        border_focused: Some(transparent),
        elevated_surface_background: Some(rgb(0x000000).into()),
        surface_background: Some(rgb(0x000000).into()),
        background: Some(rgb(0x0a0a0a).into()),
        element_background: Some(rgb(0x1f1f1f).into()),
        element_hover: Some(rgb(0x1a1a1a).into()),
        element_active: Some(rgb(0x1f1f1f).into()),
        text: Some(rgb(0xededed).into()),
        text_muted: Some(rgb(0xa0a0a0).into()),
        text_placeholder: Some(rgb(0x737373).into()),
        text_accent: Some(rgb(0x3794ff).into()),
        panel_background: Some(rgb(0x000000).into()),
        title_bar_background: Some(rgb(0x000000).into()),
        title_bar_inactive_background: Some(rgb(0x000000).into()),
        tab_bar_background: Some(rgb(0x000000).into()),
        tab_active_background: Some(rgb(0x000000).into()),
        tab_inactive_background: Some(rgb(0x111111).into()),
        editor_background: Some(rgb(0x0a0a0a).into()),
        terminal_background: Some(rgb(0x0a0a0a).into()),
        status_bar_background: Some(rgb(0x000000).into()),
        toolbar_background: Some(rgb(0x000000).into()),
        ..Default::default()
    });

    let theme = Theme {
        id: "void-dark".to_string(),
        name: "Void Dark".into(),
        appearance: base.appearance(),
        styles: ThemeStyles {
            colors,
            ..base.styles.clone()
        },
    };

    GlobalTheme::update_theme(cx, Arc::new(theme));
}
