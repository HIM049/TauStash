use gpui_fps::fps_monitor;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use sea_orm::ActiveEnum;

use uuid::Uuid;

use crate::AppState;
use crate::components::app_sidebar::{AppSidebar, NavTarget};
use crate::database::entities::{FileState, collection};
use crate::database::storage::collection as collection_store;
use crate::services::collection::CollectionConfig;
use crate::services::macrotag::MacroTag;
use crate::views::collection_explorer::CollectionExplorerView;
use crate::views::generic_view::GenericView;
use crate::views::scanner::ScannerView;

pub const EXPLORER_VIEW_ID: Uuid = Uuid::from_u128(1);
pub const PENDING_VIEW_ID: Uuid = Uuid::from_u128(2);

pub fn prefab_views() -> Vec<collection::Model> {
    let now = chrono::Utc::now();
    let explorer_config = CollectionConfig {
        filters: vec![MacroTag::FileState(FileState::Managed.to_value())],
        ..Default::default()
    };
    let pending_config = CollectionConfig {
        filters: vec![MacroTag::FileState(FileState::Pending.to_value())],
        pinned_tags: vec![
            MacroTag::FileState(FileState::Managed.to_value()),
            MacroTag::FileState(FileState::Ignore.to_value()),
            MacroTag::FileState(FileState::Blocked.to_value()),
        ],
        view: crate::services::collection::ViewOption::List,
        ..Default::default()
    };

    vec![
        collection::Model {
            id: EXPLORER_VIEW_ID,
            name: "文件总览".to_string(),
            icon: "file".to_string(),
            cover: String::new(),
            color: String::new(),
            order: 0,
            parent_id: None,
            typ: collection::CollectionType::View,
            config: serde_json::to_value(explorer_config).unwrap_or_default(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        },
        collection::Model {
            id: PENDING_VIEW_ID,
            name: "审核队列".to_string(),
            icon: "check".to_string(),
            cover: String::new(),
            color: String::new(),
            order: 1,
            parent_id: None,
            typ: collection::CollectionType::View,
            config: serde_json::to_value(pending_config).unwrap_or_default(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentNav {
    Tab(NavTab),
    View(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTab {
    Collections,
    Scanner,
}

pub struct MyApp {
    current_nav: CurrentNav,
    sidebar_collapsed: bool,
    views: Vec<collection::Model>,
    current_view: Option<Entity<GenericView>>,
    scanner_view: Entity<ScannerView>,
    collection_view: Entity<CollectionExplorerView>,
}

impl MyApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prefabs = prefab_views();
        let initial_model = prefabs[0].clone();
        let current_view =
            Some(cx.new(|cx| GenericView::from_collection(&initial_model, cx).enable_drag()));
        let scanner_view = cx.new(|cx| ScannerView::new(window, cx));
        let collection_view = cx.new(|cx| CollectionExplorerView::new(window, cx));

        let mut app = Self {
            current_nav: CurrentNav::View(EXPLORER_VIEW_ID),
            sidebar_collapsed: false,
            views: prefabs,
            current_view,
            scanner_view,
            collection_view,
        };
        app.load_views(cx);
        app
    }

    /// Load registered views from database on startup.
    fn load_views(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn(async move |this, cx| {
            let items = cx
                .background_spawn(async move {
                    rt.block_on(collection_store::list_by_type(
                        &db,
                        collection::CollectionType::View,
                    ))
                })
                .await;

            if let Ok(db_views) = items {
                this.update(cx, |this, cx| {
                    let mut all_views = prefab_views();
                    all_views.extend(db_views);
                    this.views = all_views;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn select_nav(&mut self, target: NavTarget, cx: &mut Context<Self>) {
        match target {
            NavTarget::Fixed(tab) => {
                self.current_nav = CurrentNav::Tab(tab);
                cx.notify();
            }
            NavTarget::View(id) => {
                if let Some(model) = self.views.iter().find(|v| v.id == id).cloned() {
                    self.current_nav = CurrentNav::View(id);
                    self.current_view = Some(cx.new(|cx| {
                        let mut view = GenericView::from_collection(&model, cx);
                        if id == EXPLORER_VIEW_ID {
                            view = view.enable_drag();
                        }
                        view
                    }));
                    cx.notify();
                }
            }
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = match &self.current_nav {
            CurrentNav::Tab(tab) => NavTarget::Fixed(*tab),
            CurrentNav::View(id) => NavTarget::View(*id),
        };

        AppSidebar::new(active)
            .collapsed(self.sidebar_collapsed)
            .views(self.views.clone())
            .on_select(cx.listener(|this, target: &NavTarget, _, cx| {
                this.select_nav(target.clone(), cx);
            }))
            .on_toggle_collapse(cx.listener(|this, _, _window, cx| {
                this.sidebar_collapsed = !this.sidebar_collapsed;
                cx.notify();
            }))
    }
}

impl Render for MyApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main_content = match &self.current_nav {
            CurrentNav::Tab(NavTab::Scanner) => self.scanner_view.clone().into_any_element(),
            CurrentNav::Tab(NavTab::Collections) => {
                self.collection_view.clone().into_any_element()
            }
            CurrentNav::View(_) => {
                if let Some(view) = &self.current_view {
                    view.clone().into_any_element()
                } else {
                    div().into_any_element()
                }
            }
        };

        div()
            .h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .relative()
            .size_full()
            .child(self.render_sidebar(cx))
            .child(div().h_full().flex_1().child(main_content))
            .when(true, |this| this.child(fps_monitor(window, cx)))
    }
}
