use gpui_kit::component::*;
use gpui_kit::*;

use crate::AppState;
use crate::services::importer::{self, FileInfo};

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
        // empty message
        if self.file_list.is_empty() {
            return div()
                .size_full()
                .h_flex()
                .items_center()
                .justify_center()
                .p_8()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("库中暂无文件记录")
                .into_any_element();
        }

        let grid_cards = self.file_list.iter().map(|item| {
            let extension = item.mime.split('/').nth(1).unwrap_or("file").to_uppercase();

            div()
                .v_flex()
                .items_center()
                .justify_between()
                .w(px(140.0))
                .h(px(150.0))
                .p_3()
                .bg(cx.theme().secondary)
                .border_1()
                .border_color(cx.theme().border)
                .rounded(cx.theme().radius)
                .child(
                    div()
                        .v_flex()
                        .items_center()
                        .justify_center()
                        .flex_1()
                        .gap_1p5()
                        .child(
                            div()
                                .w_10()
                                .h_10()
                                .rounded_md()
                                .bg(cx.theme().background)
                                .h_flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::new(IconName::File)
                                        .size_5()
                                        .text_color(cx.theme().primary),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(extension),
                        ),
                )
                .child(
                    div()
                        .v_flex()
                        .items_center()
                        .w_full()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().foreground)
                                .overflow_hidden()
                                .text_ellipsis()
                                .w_full()
                                .text_center()
                                .child(item.file_name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{:.1} MB", item.size as f64 / 1024.0 / 1024.0)),
                        ),
                )
        });

        div()
            .flex()
            .flex_wrap()
            .gap_3()
            .w_full()
            .children(grid_cards)
            .into_any_element()
    }
}
