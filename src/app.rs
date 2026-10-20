use gpui_fps::fps_monitor;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::AppState;
use crate::components::app_sidebar::{AppSidebar, NavTarget};
use crate::views::collection_explorer::CollectionExplorerView;
use crate::views::generic_view::GenericView;
use crate::views::scanner::ScannerView;
use uuid::Uuid;

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
    current_view: Option<Entity<GenericView>>,
    scanner_view: Entity<ScannerView>,
    collection_view: Entity<CollectionExplorerView>,
}

impl MyApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let views_entity = cx.global::<AppState>().views.clone();
        cx.observe(&views_entity, |_, _, cx| {
            cx.notify();
        })
        .detach();

        cx.observe_global::<AppState>(|_, cx| {
            cx.notify();
        })
        .detach();

        let initial_model = views_entity.read(cx).views.first().cloned();
        let current_view = initial_model
            .as_ref()
            .map(|model| cx.new(|cx| GenericView::from_collection(model, window, cx)));
        let scanner_view = cx.new(|cx| ScannerView::new(window, cx));
        let collection_view = cx.new(|cx| CollectionExplorerView::new(window, cx));

        let current_nav = match &initial_model {
            Some(model) => CurrentNav::View(model.id),
            None => CurrentNav::Tab(NavTab::Collections),
        };

        Self {
            current_nav,
            sidebar_collapsed: false,
            current_view,
            scanner_view,
            collection_view,
        }
    }

    fn select_nav(&mut self, target: NavTarget, window: &mut Window, cx: &mut Context<Self>) {
        match target {
            NavTarget::Fixed(tab) => {
                self.current_nav = CurrentNav::Tab(tab);
                cx.notify();
            }
            NavTarget::View(id) => {
                let views = cx.global::<AppState>().views.read(cx).views.clone();
                if let Some(model) = views.iter().find(|v| v.id == id).cloned() {
                    self.current_nav = CurrentNav::View(id);
                    self.current_view =
                        Some(cx.new(|cx| GenericView::from_collection(&model, window, cx)));
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

        let views = cx.global::<AppState>().views.read(cx).views.clone();

        AppSidebar::new(active)
            .collapsed(self.sidebar_collapsed)
            .views(views)
            .on_select(cx.listener(|this, target: &NavTarget, window, cx| {
                this.select_nav(target.clone(), window, cx);
            }))
            .on_toggle_collapse(cx.listener(|this, _, _window, cx| {
                this.sidebar_collapsed = !this.sidebar_collapsed;
                cx.notify();
            }))
            .on_settings(cx.listener(|_this, _event: &ClickEvent, window, cx| {
                window.dispatch_action(Box::new(crate::OpenSettings), cx);
            }))
    }
}

impl Render for MyApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_fps = cx.global::<AppState>().config.fps_monitor;
        let main_content = match &self.current_nav {
            CurrentNav::Tab(NavTab::Scanner) => self.scanner_view.clone().into_any_element(),
            CurrentNav::Tab(NavTab::Collections) => self.collection_view.clone().into_any_element(),
            CurrentNav::View(_) => {
                if let Some(view) = &self.current_view {
                    view.clone().into_any_element()
                } else {
                    div().into_any_element()
                }
            }
        };
        div()
            .v_flex()
            .size_full()
            .child(TitleBar::new().text_xs().font_semibold().child("PiCargo"))
            .child(
                div()
                    .h_flex()
                    .size_full()
                    .bg(cx.theme().secondary)
                    .text_color(cx.theme().foreground)
                    .relative()
                    .size_full()
                    .child(self.render_sidebar(cx))
                    .child(div().h_full().flex_1().min_w_0().child(main_content))
                    .when(show_fps, |this| {
                        this.child(fps_monitor(window, cx).anchor(Anchor::BottomRight))
                    }),
            )
    }
}
