use std::path::PathBuf;

use gpui_kit::component::button::Button;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ElementExt as _, *};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::database::entities::FileState;
use crate::database::storage::file::TagFilter;
use crate::database::storage::tag::TagSummaryItem;
use crate::database::storage::{self};
use crate::services::importer::{self, FileInfo};
use crate::{AppState, components};

pub struct OverallView {
    pub file_list: Vec<FileInfo>,
    scroll_handle: UniformListScrollHandle,
    selected_filter: TagFilter,
    total_files_count: usize,
    untagged_files_count: usize,
    tags_summary: Vec<TagSummaryItem>,
    selected_file_id: Option<uuid::Uuid>,
    container_width: Pixels,

    importing: bool,
    error_message: Option<String>,
}

impl OverallView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            file_list: Vec::new(),
            scroll_handle: UniformListScrollHandle::new(),
            selected_filter: TagFilter::All,
            total_files_count: 0,
            untagged_files_count: 0,
            tags_summary: Vec::new(),
            selected_file_id: None,
            container_width: px(800.0),
            importing: false,
            error_message: None,
        };
        view.reload_all(cx);
        view
    }

    pub fn handle_drag(
        &mut self,
        window: &mut Window,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.importing || paths.is_empty() {
            return;
        }

        self.importing = true;
        self.error_message = None;
        window.push_notification(Notification::new().message("正在导入文件"), cx);
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let device_id = cx.global::<AppState>().device_id.clone();

        cx.spawn_in(window, async move |this, cx: &mut AsyncWindowContext| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(importer::import_from_paths(&db, &paths, &device_id))
                })
                .await;

            let msg = this
                .update(cx, |this, cx| {
                    this.importing = false;
                    let message: SharedString = if let Ok(report) = res {
                        if report.failed.is_empty() {
                            format!("{} 个内容导入成功", report.imported).into()
                        } else {
                            format!("{} 内容径导入失败", report.failed.len()).into()
                        }
                    } else {
                        "导入成功".into()
                    };

                    this.reload_all(cx);
                    message
                })
                .ok();

            if let Some(msg) = msg {
                cx.update(|w, cx| {
                    w.push_notification(Notification::new().message(msg), cx);
                })
                .unwrap();
            }
        })
        .detach();
    }

    /// refresh file list
    pub fn reload_files(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let current_filter = self.selected_filter.clone();

        cx.spawn(async move |this, cx| {
            let files_res = cx
                .background_spawn(async move {
                    rt.block_on(importer::list_files_filtered(
                        &db,
                        &current_filter,
                        Some(FileState::Managed),
                    ))
                })
                .await;

            this.update(cx, |this, cx| {
                if let Ok(files) = files_res {
                    this.file_list = files;
                }
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }

    /// refresh tags stats
    pub fn reload_stats(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn(async move |this, cx| {
            let stats_res = cx
                .background_spawn(async move {
                    rt.block_on(storage::tag::get_tags_stats(&db, Some(FileState::Managed)))
                })
                .await;
            let Ok(stats) = stats_res else {
                return;
            };

            this.update(cx, |this, cx| {
                this.tags_summary = stats.tags_summary;
                this.untagged_files_count = stats.untagged_files;
                this.total_files_count = stats.total_files;
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }

    /// refresh both stats and file list
    pub fn reload_all(&mut self, cx: &mut Context<Self>) {
        self.reload_files(cx);
        self.reload_stats(cx);
    }

    fn set_filter(&mut self, filter: TagFilter, cx: &mut Context<Self>) {
        if self.selected_filter == filter {
            return;
        }

        self.selected_filter = filter;
        self.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);

        self.reload_files(cx);
        cx.notify();
    }

    fn render_tag_chip(
        &self,
        id_str: SharedString,
        filter: TagFilter,
        label: impl Into<SharedString>,
        count: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_selected = self.selected_filter == filter;
        let label = label.into();

        let (bg_color, text_color, border_color) = if is_selected {
            (
                cx.theme().primary,
                cx.theme().primary_foreground,
                cx.theme().primary,
            )
        } else {
            (
                cx.theme().secondary,
                cx.theme().foreground,
                cx.theme().border,
            )
        };

        div()
            .id(id_str)
            .h_flex()
            .items_center()
            .gap_1p5()
            .px_3()
            .py_1()
            .rounded_full()
            .border_1()
            .border_color(border_color)
            .bg(bg_color)
            .text_color(text_color)
            .text_xs()
            .cursor_pointer()
            .hover(|style| {
                if !is_selected {
                    style.bg(cx.theme().accent)
                } else {
                    style
                }
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_filter(filter.clone(), cx);
            }))
            .child(label)
            .child(
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded_full()
                    .text_xs()
                    .when_else(
                        is_selected,
                        |d| d.bg(cx.theme().background.opacity(0.2)),
                        |d| {
                            d.bg(cx.theme().background)
                                .text_color(cx.theme().muted_foreground)
                        },
                    )
                    .child(count.to_string()),
            )
    }

    fn render_tag_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let total_count = if self.total_files_count > 0 {
            self.total_files_count
        } else {
            self.file_list.len()
        };

        let mut chips = Vec::with_capacity(self.tags_summary.len() + 2);
        chips.push(
            self.render_tag_chip("tag-all".into(), TagFilter::All, "全部", total_count, cx)
                .into_any_element(),
        );
        chips.push(
            self.render_tag_chip(
                "tag-untagged".into(),
                TagFilter::Untagged,
                "未标记",
                self.untagged_files_count,
                cx,
            )
            .into_any_element(),
        );

        for tag_item in &self.tags_summary {
            let id_str: SharedString =
                format!("tag-{:?}-{}", tag_item.namespace, tag_item.name).into();
            chips.push(
                self.render_tag_chip(
                    id_str,
                    TagFilter::Tag {
                        namespace: tag_item.namespace,
                        name: tag_item.name.clone(),
                    },
                    SharedString::new(tag_item.name.clone()),
                    tag_item.count,
                    cx,
                )
                .into_any_element(),
            );
        }

        div()
            .h_flex()
            .items_center()
            .gap_2()
            .w_full()
            .overflow_x_scrollbar()
            .py_1()
            .children(chips)
    }
}

impl Render for OverallView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width_entity = cx.entity().clone();
        let current_width = self.container_width;

        let select_entity = cx.entity().clone();

        div()
            .id("explorer-main")
            .on_drop(cx.listener(|this, e: &ExternalPaths, w, cx| {
                this.handle_drag(w, e.paths().to_vec(), cx);
            }))
            .v_flex()
            .size_full()
            .gap_3()
            .p_6()
            .pb_0()
            .pr_0()
            .on_prepaint(move |bounds, _, cx| {
                let new_w = bounds.size.width;
                if (new_w - current_width).abs() > px(2.0) {
                    width_entity.update(cx, |this, cx| {
                        this.container_width = new_w;
                        cx.notify();
                    });
                }
            })
            .child(
                div()
                    .v_flex()
                    .gap_4()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_xl().font_bold().child("文件总览"))
                            .child(
                                Button::new("refresh")
                                    .icon(IconName::RefreshCw)
                                    .on_click(cx.listener(|this, _, _, cx| this.reload_all(cx))),
                            ),
                    )
                    .child(self.render_tag_bar(cx)),
            )
            .child(
                components::file_grid::FileGrid::new(self.file_list.clone())
                    .container_width(self.container_width)
                    .selected_file_id(self.selected_file_id)
                    .scroll_handle(&self.scroll_handle)
                    .on_select(move |file_id, _, cx| {
                        select_entity.update(cx, |this, cx| {
                            this.selected_file_id = Some(file_id);
                            cx.notify();
                        });
                    })
                    .on_open(|path, _, _| components::utils::open_path(path)),
            )
    }
}
