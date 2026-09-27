use gpui_kit::component::*;
use gpui_kit::*;

#[derive(IntoElement)]
pub struct FileCard {
    file_name: SharedString,
    extension: SharedString,
    size: i64,
}

impl FileCard {
    pub fn new(
        file_name: impl Into<SharedString>,
        extension: impl Into<SharedString>,
        size: i64,
    ) -> Self {
        Self {
            file_name: file_name.into(),
            extension: extension.into(),
            size,
        }
    }
}

impl RenderOnce for FileCard {
    fn render(
        self,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> impl gpui_kit::prelude::IntoElement {
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
                            .child(self.extension),
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
                            .child(self.file_name),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{:.1} MB", self.size as f64 / 1024.0 / 1024.0)),
                    ),
            )
    }
}
