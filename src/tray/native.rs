use super::TrayCommand;
use rust_i18n::t;
use tokio::sync::mpsc::UnboundedSender;
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};

const MENU_SHOW: &str = "show";
const MENU_QUIT: &str = "quit";

/// Tray icon handle; dropping it will removes the icon.
pub type Tray = TrayIcon;

/// Init tray icon
pub fn init(tx: UnboundedSender<TrayCommand>) -> anyhow::Result<Tray> {
    let image = image::load_from_memory(include_bytes!("../../assets/app_icon.png"))?
        .resize(32, 32, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let (width, height) = image.dimensions();
    let icon = Icon::from_rgba(image.into_raw(), width, height)?;

    let menu = Menu::new();
    menu.append_items(&[
        &MenuItem::with_id(MENU_SHOW, t!("tray.show"), true, None),
        &MenuItem::with_id(MENU_QUIT, t!("app_menu.quit"), true, None),
    ])?;

    let click_tx = tx.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = click_tx.send(TrayCommand::Show);
        }
    }));
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let command = if event.id == MENU_SHOW {
            TrayCommand::Show
        } else if event.id == MENU_QUIT {
            TrayCommand::Quit
        } else {
            return;
        };
        let _ = tx.send(command);
    }));

    Ok(TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("TauStash")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?)
}
