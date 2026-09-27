use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::database::storage::{self, TagFilter, TagSummaryItem};
use crate::services::importer::{self, FileInfo};
use crate::{AppState, components};

pub struct OverallView {
    pub file_list: Vec<FileInfo>,
    scroll_handle: UniformListScrollHandle,
    selected_filter: TagFilter,
    total_files_count: usize,
    untagged_files_count: usize,
    tags_summary: Vec<TagSummaryItem>,
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
        };
        view.reload_all(cx);
        view
    }

    /// refresh file list
    pub fn reload_files(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let current_filter = self.selected_filter.clone();

        cx.spawn(async move |this, cx| {
            let files_res = importer::list_files_filtered(&db, &current_filter).await;

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

        cx.spawn(async move |this, cx| {
            let Ok(stats) = storage::get_tags_stats(&db).await else {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport_width = window.viewport_size().width;
        let available_width = (viewport_width - px(280.0)).max(px(152.0));
        let cols = (available_width.as_f32() / 148.0).floor().max(1.0) as usize;

        let rows: Vec<Vec<FileInfo>> = self
            .file_list
            .chunks(cols)
            .map(|chunk| chunk.to_vec())
            .collect();
        let rows_count = rows.len();

        div()
            .v_flex()
            .size_full()
            .gap_3()
            .p_6()
            .pb_0()
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .child(div().text_xl().font_bold().child("文件总览"))
                    .child(self.render_tag_bar(cx)),
            )
            .child(div().relative().flex_1().size_full().when_else(
                rows_count == 0,
                |d| {
                    d.h_flex()
                        .items_center()
                        .justify_center()
                        .size_full()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("当前筛选下暂无匹配文件")
                },
                |d| {
                    d.child(
                        uniform_list("file-grid-list", rows_count, {
                            let rows = rows.clone();
                            move |visible_range, _, _cx| {
                                visible_range
                                    .map(|row_idx| {
                                        let row_files = &rows[row_idx];
                                        div()
                                            .h(px(162.0)) // 卡片 150px + gap 12px
                                            .h_flex()
                                            .gap_3()
                                            .children(row_files.iter().map(|item| {
                                                let extension = item
                                                    .mime
                                                    .split('/')
                                                    .nth(1)
                                                    .unwrap_or("file")
                                                    .to_uppercase();
                                                components::file_card::FileCard::new(
                                                    SharedString::new(item.file_name.as_str()),
                                                    extension,
                                                    item.size,
                                                )
                                            }))
                                    })
                                    .collect()
                            }
                        })
                        .pb_6()
                        .size_full()
                        .track_scroll(&self.scroll_handle),
                    )
                    .vertical_scrollbar(&self.scroll_handle)
                },
            ))
    }
}
