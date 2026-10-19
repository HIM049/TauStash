pub mod database;
pub mod services;

use gpui_kit::component::*;
use gpui_kit::*;
use services::schedule::ScheduleService;
use services::store::{CollectionStore, FileStore, TagStore, ViewStore};
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt, prelude::*, registry};

mod app;
mod components;
mod config;
mod views;
use app::MyApp;

use crate::config::AppConfig;

actions!([Select, BatchSelect]);

pub struct AppState {
    pub db: Arc<sea_orm::DatabaseConnection>,
    pub rt: Arc<tokio::runtime::Runtime>,
    pub schedule: Arc<ScheduleService>,
    pub config: AppConfig,
    pub views: Entity<ViewStore>,
    pub collections: Entity<CollectionStore>,
    pub tags: Entity<TagStore>,
    pub file: Entity<FileStore>,
}

impl Global for AppState {}

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

    if config.schedule_enable {
        let dbc = db.clone();
        let device_id = config.device_id.clone();
        schedule.schedule(
            "scan_all_sources",
            std::time::Duration::from_secs(15),
            Some(std::time::Duration::from_secs(
                config.schedule_scan_min as u64 * 60,
            )),
            move || {
                let db = dbc.clone();
                let device_id = device_id.clone();
                async move {
                    let report = services::scanner::scan_all_sources(&db, &device_id)
                        .await
                        .unwrap_or_default();
                    report.has_changes()
                }
            },
        );
        let dbc = db.clone();
        schedule.schedule(
            "verify_replicas",
            std::time::Duration::from_secs(30),
            Some(std::time::Duration::from_secs(
                config.schedule_maintain_min as u64 * 60,
            )),
            move || {
                let db = dbc.clone();
                async move {
                    services::schedule::verify_active_replicas(&db)
                        .await
                        .unwrap_or(0)
                        > 0
                }
            },
        );
    }

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

        cx.bind_keys([KeyBinding::new("ctrl", Select, None)]);
        cx.bind_keys([KeyBinding::new("shift", BatchSelect, None)]);

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
