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
mod logging;
mod menu;
mod paths;
mod theme;
mod update;
mod views;
use app::MyApp;

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

        let window_bounds =
            WindowBounds::Windowed(Bounds::centered(None, size(px(970.0), px(780.0)), cx));

        cx.on_action(|_: &OpenSettings, cx| {
            AppState::open_settings_window(cx);
        });

        cx.on_action(|_: &QuitApp, cx| {
            cx.quit();
        });

        cx.spawn(async move |cx| {
            let window = cx
                .open_window(
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
                    |window, cx| {
                        let view = cx.new(|cx| MyApp::new(window, cx));
                        cx.new(|cx| Root::new(view, window, cx))
                    },
                )
                .expect("Failed to open window");

            if check_update {
                match update::check_update(http_client).await {
                    Ok(Some(url)) => update::show_update_dialog(window.into(), url, cx),
                    Ok(None) => {}
                    Err(e) => tracing::warn!("Failed to check for updates: {:#}", e),
                }
            }
        })
        .detach();
    });
}
