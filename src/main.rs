pub mod database;
pub mod services;

use gpui_kit::component::*;
use gpui_kit::*;
use services::store::{CollectionStore, FileStore, TagStore, ViewStore};
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt, prelude::*, registry};

mod app;
mod components;
mod config;
mod views;
use app::MyApp;

use crate::{config::AppConfig, services::ScheduleService};

actions!([OpenSettings]);

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
        if let Err(e) = self.config.save(config::DEFAULT_CONFIG_PATH) {
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
                    title: Some("Settings".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
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
}

fn main() {
    // init tracing
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,picargo=debug,sqlx=warn"));
    registry().with(fmt::layer()).with(filter).init();

    tracing::debug!("Debug log enabled");

    let rt = tokio::runtime::Runtime::new().expect("Failed to start Tokio runtime");
    let _guard = rt.enter();

    let config = config::AppConfig::load_or_default(config::DEFAULT_CONFIG_PATH);

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

    let app = gpui_kit::application().with_assets(gpui_kit::assets::AllAssets);
    app.run(move |cx| {
        gpui_kit::init(cx);
        Theme::global_mut(cx).notification.placement = Anchor::BottomLeft;

        let views = cx.new(|_| ViewStore::new(&db, &rt));
        let collections = cx.new(|_| CollectionStore::new());
        let tags = cx.new(|_| TagStore::new());
        let file = cx.new(|_| FileStore::new());

        let collections_store = collections.clone();
        let tags_store = tags.clone();
        let file_store = file.clone();

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

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    titlebar: Some(TitlebarOptions {
                        title: Some("PiCargo".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| MyApp::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Failed to open window");
        })
        .detach();
    });
}
