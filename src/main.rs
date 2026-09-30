pub mod database;
pub mod services;

use gpui_kit::component::*;
use gpui_kit::*;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt, prelude::*, registry};

mod app;
mod components;
mod views;
use app::MyApp;

actions!([Select, BatchSelect]);

pub struct AppState {
    pub db: Arc<sea_orm::DatabaseConnection>,
    pub device_id: SharedString,
    pub rt: Arc<tokio::runtime::Runtime>,
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

    let db = rt.block_on(async {
        database::init_db("./app_data/picargo.db")
            .await
            .expect("Failed to initialize database")
    });
    let db = Arc::new(db);
    let global = AppState {
        db: db.clone(),
        device_id: SharedString::from("1ec1f661-9da6-4ca9-a6d8-ce46c22719c6"),
        rt: Arc::new(rt),
    };

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    app.run(move |cx| {
        gpui_kit::init(cx);

        cx.set_global(global);
        let window_bounds =
            WindowBounds::Windowed(Bounds::centered(None, size(px(980.0), px(780.0)), cx));

        cx.bind_keys([KeyBinding::new("ctrl", Select, None)]);
        cx.bind_keys([KeyBinding::new("shift", BatchSelect, None)]);

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    titlebar: Some(TitlebarOptions {
                        title: Some("PiCargo".into()),
                        appears_transparent: false,
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
