//! Bundled Vercel theme initialization and system-appearance synchronization.

use anyhow::{Context as _, ensure};
use gpui::{App, WindowAppearance};
use theme::{Appearance, GlobalTheme, SystemAppearance, ThemeRegistry};

const VERCEL_THEME: &[u8] = include_bytes!("../assets/themes/vercel-theme.json");
const VERCEL_DARK: &str = "Vercel Dark";
const VERCEL_LIGHT: &str = "Vercel Light";

/// Registers Void's two bundled themes and activates the system variant.
pub(crate) fn init(cx: &mut App) -> anyhow::Result<()> {
    ::theme::init(::theme::LoadThemes::JustBase, cx);

    let content = theme_settings::deserialize_user_theme(VERCEL_THEME)
        .context("parsing the bundled Vercel theme")?;
    let family = theme_settings::refine_theme_family(content);
    ensure!(
        family.themes.len() == 2
            && family.themes.iter().any(|theme| {
                theme.name.as_ref() == VERCEL_LIGHT && theme.appearance == Appearance::Light
            })
            && family.themes.iter().any(|theme| {
                theme.name.as_ref() == VERCEL_DARK && theme.appearance == Appearance::Dark
            }),
        "the bundled Vercel theme must contain exactly its light and dark variants"
    );

    ThemeRegistry::default_global(cx).insert_theme_families([family]);
    activate(SystemAppearance::global(cx).0, cx)
}

/// Follows a native appearance change.
pub(crate) fn sync_system_appearance(
    appearance: WindowAppearance,
    cx: &mut App,
) -> anyhow::Result<()> {
    let appearance = Appearance::from(appearance);
    *SystemAppearance::global_mut(cx) = SystemAppearance(appearance);
    activate(appearance, cx)
}

fn activate(appearance: Appearance, cx: &mut App) -> anyhow::Result<()> {
    let name = match appearance {
        Appearance::Light => VERCEL_LIGHT,
        Appearance::Dark => VERCEL_DARK,
    };
    let theme = ThemeRegistry::global(cx)
        .get(name)
        .with_context(|| format!("loading bundled theme {name:?}"))?;
    GlobalTheme::update_theme(cx, theme);
    cx.refresh_windows();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Appearance, VERCEL_DARK, VERCEL_LIGHT, VERCEL_THEME};

    #[test]
    fn bundled_theme_contains_only_light_and_dark() {
        let family = theme_settings::deserialize_user_theme(VERCEL_THEME)
            .expect("the bundled theme should parse");
        let themes = theme_settings::refine_theme_family(family)
            .themes
            .into_iter()
            .map(|theme| (theme.name.to_string(), theme.appearance))
            .collect::<Vec<_>>();

        assert_eq!(
            themes,
            [
                (VERCEL_DARK.to_owned(), Appearance::Dark),
                (VERCEL_LIGHT.to_owned(), Appearance::Light),
            ]
        );
    }
}
