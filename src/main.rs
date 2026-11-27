#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

pub mod database;
pub mod services;

use gpui_kit::component::*;
use gpui_kit::*;
use services::store::{CollectionStore, FileStore, TagStore, ViewStore};
use std::sync::Arc;

mod app;
mod components;
mod config;
mod instance;
mod logging;
mod menu;
mod paths;
mod theme;
mod update;
mod views;
use app::MyApp;

mod tray;

use crate::{
    config::AppConfig,
    menu::{OpenSettings, QuitApp},
    services::ScheduleService,
};
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// Window app id, matching the Linux desktop file name.
const APP_ID: &str = "com.him049.taustash";

pub struct AppState {
    pub db: Arc<sea_orm::DatabaseConnection>,
    pub rt: Arc<tokio::runtime::Runtime>,
    pub schedule: Arc<ScheduleService>,
    pub config: AppConfig,
    pub views: Entity<ViewStore>,
    pub collections: Entity<CollectionStore>,
    pub tags: Entity<TagStore>,
    pub file: Entity<FileStore>,
    pub settings_window: Option<AnyWindowHandle>,
    pub main_window: Option<AnyWindowHandle>,
    pub tray: Option<tray::Tray>,
}

impl Global for AppState {}

impl AppState {
    pub fn update_config(&mut self, updater: impl FnOnce(&mut AppConfig)) {
        updater(&mut self.config);
        if let Err(e) = self.config.save(paths::config_path()) {
            tracing::error!("Failed to save config: {:#}", e);
        }
        services::schedule::run_schedule(&self.schedule, &self.config, &self.db);
    }

    pub fn open_settings_window(cx: &mut App) {
        let existing = cx.global::<AppState>().settings_window;
        if let Some(handle) = existing {
            if handle
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .is_ok()
            {
                return;
            }
        }

        let window_bounds =
            WindowBounds::Windowed(Bounds::centered(None, size(px(800.0), px(600.0)), cx));

        let (handle, _) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                titlebar: Some(TitlebarOptions {
                    title: Some(t!("settings.title").into()),
                    appears_transparent: true,
                    traffic_light_position: Some(Point {
                        x: px(9.),
                        y: px(9.),
                    }),
                }),
                app_id: Some(APP_ID.into()),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| views::settings::SettingsView::new(window, cx)),
        )
        .expect("Failed to open settings window");

        cx.update_global::<AppState, _>(|state, _| {
            state.settings_window = Some(handle);
        });
    }

    /// Activate the main window, or open it again if it was closed.
    pub fn open_main_window(cx: &mut App) -> Option<AnyWindowHandle> {
        tray::set_dock_visible(true, cx);

        let existing = cx.global::<AppState>().main_window;
        if let Some(handle) = existing
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return Some(handle);
        }

        let window_bounds =
            WindowBounds::Windowed(Bounds::centered(None, size(px(970.0), px(780.0)), cx));

        let handle = match gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                titlebar: Some(TitlebarOptions {
                    title: Some("TauStash".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                app_id: Some(APP_ID.into()),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| MyApp::new(window, cx)),
        ) {
            Ok((handle, _)) => handle,
            Err(e) => {
                tracing::error!("Failed to open main window: {:#}", e);
                return None;
            }
        };

        cx.update_global::<AppState, _>(|state, _| {
            state.main_window = Some(handle);
        });
        Some(handle)
    }

    /// Open a collection in a new window of its own.
    pub fn open_view_window(model: &database::entities::collection::Model, cx: &mut App) {
        let window_bounds =
            WindowBounds::Windowed(Bounds::centered(None, size(px(970.0), px(780.0)), cx));

        let model = model.clone();
        if let Err(e) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                titlebar: Some(TitlebarOptions {
                    title: Some(model.name.clone().into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                app_id: Some(APP_ID.into()),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| views::view_window::ViewWindow::new(&model, window, cx)),
        ) {
            tracing::error!("Failed to open view window: {:#}", e);
        }
    }
}

fn main() {
    logging::init();

    let listener = match instance::acquire() {
        Ok(instance::Instance::Primary(listener)) => Some(listener),
        Ok(instance::Instance::Secondary) => return,
        Err(e) => {
            tracing::warn!("Single instance check failed: {:#}", e);
            None
        }
    };

    let rt = tokio::runtime::Runtime::new().expect("Failed to start Tokio runtime");
    let _guard = rt.enter();

    let config = config::AppConfig::load_or_default(paths::config_path());
    logging::set_debug(config.debug_log);
    tracing::debug!("Debug log enabled");
    rust_i18n::set_locale(&config.locale);

    let db = rt.block_on(async {
        database::init_db(&config.db_path)
            .await
            .expect("Failed to initialize database")
    });
    let db = Arc::new(db);

    let (activate_tx, mut activate_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<String>>();
    if let Some(listener) = listener {
        instance::listen(listener, activate_tx);
    }

    let (file_change_tx, mut file_change_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    let schedule = Arc::new(ScheduleService::new(move || {
        let _ = file_change_tx.send(());
    }));

    services::schedule::run_schedule(&schedule, &config, &db);

    let http = reqwest_client::ReqwestClient::user_agent(&format!(
        "TauStash/{}",
        env!("CARGO_PKG_VERSION")
    ))
    .expect("Failed to create HTTP client");

    let app = gpui_kit::application().with_assets(gpui_kit::assets::AllAssets);
    app.on_reopen(|cx| {
        AppState::open_main_window(cx);
    });
    app.run(move |cx| {
        gpui_kit::init(cx);
        cx.set_http_client(Arc::new(http));
        menu::update_menu(cx);
        Theme::global_mut(cx).notification.placement = Anchor::BottomLeft;
        theme::init(cx);
        theme::apply(config.theme_mode, None, cx);

        let views = cx.new(|_| ViewStore::new(&db, &rt));
        let collections = cx.new(|_| CollectionStore::new());
        let tags = cx.new(|_| TagStore::new());
        let file = cx.new(|_| FileStore::new());

        let collections_store = collections.clone();
        let tags_store = tags.clone();
        let file_store = file.clone();

        let check_update = config.check_update;
        let http_client = cx.http_client();

        let global = AppState {
            db: db.clone(),
            rt: Arc::new(rt),
            schedule,
            config,
            views,
            collections,
            tags,
            file,
            settings_window: None,
            main_window: None,
            tray: None,
        };
        cx.set_global(global);

        collections_store.update(cx, |c, cx| c.reload(cx));
        tags_store.update(cx, |t, cx| t.reload(cx));

        cx.spawn(async move |cx| {
            while let Some(()) = file_change_rx.recv().await {
                cx.update(|cx| {
                    file_store.update(cx, |_, cx| cx.notify());
                });
            }
        })
        .detach();

        cx.spawn(async move |cx| {
            while let Some(_) = activate_rx.recv().await {
                cx.update(|cx| {
                    AppState::open_main_window(cx);
                });
            }
        })
        .detach();

        cx.on_action(|_: &OpenSettings, cx| {
            AppState::open_settings_window(cx);
        });

        cx.on_action(|_: &QuitApp, cx| {
            cx.quit();
        });

        if cx.global::<AppState>().config.keep_in_background {
            let (tray_tx, mut tray_rx) = tokio::sync::mpsc::unbounded_channel();
            let t = tray::init(tray_tx)
                .inspect_err(|e| tracing::error!("Failed to create tray icon: {:#}", e))
                .ok();

            cx.spawn(async move |cx| {
                while let Some(command) = tray_rx.recv().await {
                    cx.update(|cx| match command {
                        tray::TrayCommand::Show => {
                            AppState::open_main_window(cx);
                        }
                        tray::TrayCommand::Quit => cx.quit(),
                    });
                }
            })
            .detach();

            cx.update_global(|g: &mut AppState, _| g.tray = t);
        }

        // Quitting is decided here, so closing every window can leave the app in the background.
        cx.set_quit_mode(QuitMode::Explicit);
        cx.on_window_closed(|cx, _| {
            let g = cx.global::<AppState>();
            if !cx.windows().is_empty() {
                return;
            }
            if !g.config.keep_in_background || g.tray.is_none() {
                cx.quit();
            } else {
                tray::set_dock_visible(false, cx);
            }
        })
        .detach();

        let window = AppState::open_main_window(cx);

        if check_update && let Some(window) = window {
            cx.spawn(
                async move |cx| match update::check_update(http_client).await {
                    Ok(Some(url)) => update::show_update_dialog(window, url, cx),
                    Ok(None) => {}
                    Err(e) => tracing::warn!("Failed to check for updates: {:#}", e),
                },
            )
            .detach();
        }
    });
}
