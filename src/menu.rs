use gpui_kit::{App, Menu, MenuItem, actions, base::GlobalState};
use rust_i18n::t;

actions!([QuitApp, OpenSettings]);

fn build_menu() -> Vec<Menu> {
    vec![Menu::new("TauStash").items(vec![
        MenuItem::action(t!("app_menu.settings"), OpenSettings),
        MenuItem::action(t!("app_menu.quit"), QuitApp),
    ])]
}

pub fn update_menu(cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.set_menus(build_menu());

    #[cfg(not(target_os = "macos"))]
    GlobalState::global_mut(cx).set_app_menus(
        build_menu()
            .into_iter()
            .map(|t| t.owned())
            .collect::<Vec<_>>(),
    );
}
