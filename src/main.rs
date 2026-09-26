pub mod database;

use gpui_kit::component::*;
use gpui_kit::*;
use std::sync::Arc;

mod app;
use app::DebugApp;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("Failed to start Tokio runtime");
    let _guard = rt.enter();

    let db = rt.block_on(async {
        database::init_db("./app_data/picargo.db")
            .await
            .expect("Failed to initialize database")
    });
    let db = Arc::new(db);

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);

    app.run(move |cx| {
        gpui_kit::init(cx);

        let window_bounds =
            WindowBounds::Windowed(Bounds::centered(None, size(px(720.0), px(820.0)), cx));

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    titlebar: Some(TitlebarOptions {
                        title: Some("PiCargo 调试工作台".into()),
                        appears_transparent: false,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| DebugApp::new(db, window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Failed to open window");
        })
        .detach();
    });
}
