use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::*;

use crate::services::importer::{self, FileInfo};
use crate::{AppState, components};

pub struct OverallView {
    pub file_list: Vec<FileInfo>,
    scroll_handle: UniformListScrollHandle,
}

impl OverallView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            file_list: Vec::new(),
            scroll_handle: UniformListScrollHandle::new(),
        };
        view.reload_files(cx);
        view
    }

    fn reload_files(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        cx.spawn(async move |this, cx| {
            let res = importer::list_all_files(&db).await;
            this.update(cx, |this, cx| {
                if let Ok(files) = res {
                    this.file_list = files;
                }
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }
}

impl Render for OverallView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.file_list.is_empty() {
            return div()
                .v_flex()
                .size_full()
                .gap_2()
                .child(div().text_xl().child("文件总览"))
                .child(
                    div()
                        .size_full()
                        .h_flex()
                        .items_center()
                        .justify_center()
                        .p_8()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("库中暂无文件记录"),
                );
        }

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
            .gap_2()
            .child(div().text_xl().child("文件总览"))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .size_full()
                    .child(
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
                        .size_full()
                        .track_scroll(&self.scroll_handle),
                    )
                    .vertical_scrollbar(&self.scroll_handle),
            )
    }
}
