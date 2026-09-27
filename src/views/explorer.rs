use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::services::importer::{self, FileInfo};
use crate::{AppState, components};

pub struct OverallView {
    pub file_list: Vec<FileInfo>,
}

impl OverallView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            file_list: Vec::new(),
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let grid_cards = self.file_list.iter().map(|item| {
            let extension = item.mime.split('/').nth(1).unwrap_or("file").to_uppercase();
            components::file_card::FileCard::new(
                SharedString::new(item.file_name.as_str()),
                extension,
                item.size,
            )
        });

        div()
            .v_flex()
            .size_full()
            .gap_2()
            .child(div().text_xl().child("文件总览"))
            .child(
                div()
                    .id("file-container")
                    .size_full()
                    .overflow_y_scrollbar()
                    .when_else(
                        self.file_list.is_empty(),
                        |d| {
                            d.size_full()
                                .h_flex()
                                .items_center()
                                .justify_center()
                                .p_8()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child("库中暂无文件记录")
                        },
                        |d| {
                            d.child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_3()
                                    .w_full()
                                    .children(grid_cards),
                            )
                        },
                    ),
            )
    }
}
