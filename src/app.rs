use gpui_fps::fps_monitor;
use gpui_kit::component::sidebar::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::views::explorer::OverallView;
use crate::views::pending::PendingView;
use crate::views::scanner::ScannerView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTab {
    Explorer,
    Scanner,
    Pending,
}

pub struct MyApp {
    active_tab: NavTab,
    sidebar_collapsed: bool,
    explorer_view: Entity<OverallView>,
    scanner_view: Entity<ScannerView>,
    pending_view: Entity<PendingView>,
}

impl MyApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let explorer_view = cx.new(|cx| OverallView::new(cx));
        let scanner_view = cx.new(|cx| ScannerView::new(window, cx));
        let pending_view = cx.new(|cx| PendingView::new(cx));

        Self {
            active_tab: NavTab::Explorer,
            sidebar_collapsed: false,
            explorer_view,
            scanner_view,
            pending_view,
        }
    }

    fn set_active_tab(&mut self, tab: NavTab, cx: &mut Context<Self>) {
        self.active_tab = tab;
        cx.notify();
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Sidebar::new("app-sidebar")
            .collapsible(SidebarCollapsible::Icon)
            .collapsed(self.sidebar_collapsed)
            .w(px(220.0))
            .header(
                SidebarHeader::new().child(h_flex().items_center().gap_2().px_1().when(
                    !self.sidebar_collapsed,
                    |this| {
                        this.font_weight(FontWeight::BOLD)
                            .text_sm()
                            .child("PiCargo")
                    },
                )),
            )
            .child(
                SidebarGroup::new("导航").child(
                    SidebarMenu::new()
                        .child(
                            SidebarMenuItem::new("文件总览")
                                .icon(IconName::Folder)
                                .active(self.active_tab == NavTab::Explorer)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_active_tab(NavTab::Explorer, cx);
                                })),
                        )
                        .child(
                            SidebarMenuItem::new("审核队列")
                                .icon(IconName::Check)
                                .active(self.active_tab == NavTab::Pending)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_active_tab(NavTab::Pending, cx);
                                })),
                        )
                        .child(
                            SidebarMenuItem::new("扫描状态")
                                .icon(IconName::Search)
                                .active(self.active_tab == NavTab::Scanner)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_active_tab(NavTab::Scanner, cx);
                                })),
                        ),
                ),
            )
            .footer(
                div().child(
                    SidebarToggleButton::new()
                        .collapsed(self.sidebar_collapsed)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sidebar_collapsed = !this.sidebar_collapsed;
                            cx.notify();
                        })),
                ),
            )
    }
}

impl Render for MyApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main_content = match self.active_tab {
            NavTab::Explorer => self.explorer_view.clone().into_any_element(),
            NavTab::Scanner => self.scanner_view.clone().into_any_element(),
            NavTab::Pending => self.pending_view.clone().into_any_element(),
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
