use std::rc::Rc;

use gpui_kit::component::{Theme, ThemeMode, ThemeSet};
use gpui_kit::{App, Window};

use crate::config;

const THEME: &str = include_str!("../themes/taustash_theme.json");

/// load themes
pub fn init(cx: &mut App) {
    let theme_set = serde_json::from_str::<ThemeSet>(THEME).expect("invalid bundled theme file");
    let theme = Theme::global_mut(cx);
    for config in theme_set.themes {
        if config.mode.is_dark() {
            theme.dark_theme = Rc::new(config);
        } else {
            theme.light_theme = Rc::new(config);
        }
    }
}

/// Apply the configured theme
pub fn apply(mode: config::ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    match mode {
        config::ThemeMode::System => Theme::sync_system_appearance(window, cx),
        config::ThemeMode::Light => Theme::change(ThemeMode::Light, window, cx),
        config::ThemeMode::Dark => Theme::change(ThemeMode::Dark, window, cx),
    }
}
