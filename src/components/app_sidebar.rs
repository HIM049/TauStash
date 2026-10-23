use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::sidebar::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use uuid::Uuid;

use crate::app::NavTab;
use crate::database::entities::collection;

/// Identifier for selecting an item in the sidebar navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    Fixed(NavTab),
    View(Uuid),
}

pub type NavSelectListener = Rc<dyn Fn(&NavTarget, &mut Window, &mut App) + 'static>;
pub type ToggleCollapseListener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
pub type SettingsClickListener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct AppSidebar {
    collapsed: bool,
    active: NavTarget,
    views: Vec<collection::Model>,
    on_select: Option<NavSelectListener>,
    on_toggle_collapse: Option<ToggleCollapseListener>,
    on_settings: Option<SettingsClickListener>,
}

impl AppSidebar {
    pub fn new(active: NavTarget) -> Self {
        Self {
            collapsed: false,
            active,
            views: Vec::new(),
            on_select: None,
            on_toggle_collapse: None,
            on_settings: None,
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn views(mut self, views: Vec<collection::Model>) -> Self {
        self.views = views;
        self
    }

    pub fn on_select(
        mut self,
        listener: impl Fn(&NavTarget, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Rc::new(listener));
        self
    }

    pub fn on_toggle_collapse(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle_collapse = Some(Rc::new(listener));
        self
    }

    pub fn on_settings(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_settings = Some(Rc::new(listener));
        self
    }
}

impl RenderOnce for AppSidebar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let on_select = self.on_select.clone();
        let on_toggle = self.on_toggle_collapse.clone();
        let on_settings = self.on_settings.clone();

        Sidebar::new("app-sidebar")
            .collapsible(SidebarCollapsible::Icon)
            .collapsed(self.collapsed)
            .w(px(220.0))
            .child(SidebarGroup::new("导航").child({
                let mut menu = SidebarMenu::new();

                for v in &self.views {
                    let view_id = v.id;
                    let view_name = v.name.clone();
                    let is_active = self.active == NavTarget::View(view_id);
                    let sel = on_select.clone();
                    let icon_path: SharedString = v.icon.clone().into();
                    let icon = Icon::default().path(icon_path);

                    menu = menu.child(
                        SidebarMenuItem::new(SharedString::from(view_name))
                            .icon(icon)
                            .active(is_active)
                            .on_click(move |_, window, cx| {
                                if let Some(cb) = sel.as_ref() {
                                    cb(&NavTarget::View(view_id), window, cx);
                                }
                            }),
                    );
                }

                let sel = on_select.clone();
                menu = menu.child(
                    SidebarMenuItem::new("全部合集")
                        .icon(IconName::Folder)
                        .active(self.active == NavTarget::Fixed(NavTab::Collections))
                        .on_click(move |_, window, cx| {
                            if let Some(cb) = sel.as_ref() {
                                cb(&NavTarget::Fixed(NavTab::Collections), window, cx);
                            }
                        }),
                );

                let sel = on_select.clone();
                menu = menu.child(
                    SidebarMenuItem::new("扫描管理")
                        .icon(IconName::Search)
                        .active(self.active == NavTarget::Fixed(NavTab::Scanner))
                        .on_click(move |_, window, cx| {
                            if let Some(cb) = sel.as_ref() {
                                cb(&NavTarget::Fixed(NavTab::Scanner), window, cx);
                            }
                        }),
                );

                menu
            }))
            .footer(
                div()
                    .h_flex()
                    .w_full()
                    .gap_1()
                    .when(self.collapsed, |this| this.v_flex().flex_col_reverse())
                    .child(
                        Button::new("sidebar-collapse")
                            .ghost()
                            .when_else(
                                self.collapsed,
                                |this| this.icon(IconName::PanelLeftOpen),
                                |this| this.icon(IconName::PanelLeftClose),
                            )
                            .on_click(move |event, window, cx| {
                                if let Some(cb) = on_toggle.as_ref() {
                                    cb(event, window, cx);
                                }
                            }),
                    )
                    .child(
                        Button::new("sidebar-settings")
                            .ghost()
                            .icon(IconName::Settings)
                            .on_click(move |event, window, cx| {
                                if let Some(cb) = on_settings.as_ref() {
                                    cb(event, window, cx);
                                }
                            }),
                    ),
            )
    }
}
