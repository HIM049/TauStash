use std::rc::Rc;

use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::components::utils::format_file_size;

pub type FileCardClickListener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
pub type FileCardActionHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct FileCard {
    id: ElementId,
    file_name: SharedString,
    extension: SharedString,
    path: Option<SharedString>,
    hash: Option<SharedString>,
    size: i64,
    mime: Option<SharedString>,
    selected: bool,
    on_click: Option<FileCardClickListener>,
    on_double_click: Option<FileCardClickListener>,
    on_secondary_click: Option<FileCardActionHandler>,
}

impl FileCard {
    pub fn new(
        id: impl Into<ElementId>,
        file_name: impl Into<SharedString>,
        extension: impl Into<SharedString>,
        size: i64,
    ) -> Self {
        Self {
            id: id.into(),
            file_name: file_name.into(),
            extension: extension.into(),
            path: None,
            hash: None,
            size,
            mime: None,
            selected: false,
            on_click: None,
            on_double_click: None,
            on_secondary_click: None,
        }
    }

    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn hash(mut self, hash: impl Into<SharedString>) -> Self {
        self.hash = Some(hash.into());
        self
    }

    pub fn mime(mut self, mime: impl Into<SharedString>) -> Self {
        self.mime = Some(mime.into());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(listener));
        self
    }

    pub fn on_double_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_double_click = Some(Rc::new(listener));
        self
    }

    pub fn on_secondary_click(
        mut self,
        listener: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_secondary_click = Some(Rc::new(listener));
        self
    }
}

impl RenderOnce for FileCard {
    fn render(
        self,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> impl gpui_kit::prelude::IntoElement {
        let is_selected = self.selected;

        let on_click_handler = self.on_click;
        let on_double_click_handler = self.on_double_click;
        let on_secondary_click = self.on_secondary_click;

        let is_image = self
            .mime
            .as_ref()
            .map(|m| m.starts_with("image/"))
            .unwrap_or(false);

        let thumb_path = if is_image && let Some(ref hash) = self.hash {
            let p = crate::services::thumbnail::get_thumbnail_path(hash.as_str());
            if p.is_file() { Some(p) } else { None }
        } else {
            None
        };

        div()
            .id(self.id)
            .v_flex()
            .items_center()
            .justify_between()
            .w(px(140.0))
            .h(px(150.0))
            .p_3()
            .rounded(cx.theme().radius)
            .when(is_selected, |s| s.bg(cx.theme().accent.opacity(0.9)))
            .when(!is_selected, |d| {
                d.hover(|style| style.bg(cx.theme().accent.opacity(0.6)))
            })
            .when_none(&thumb_path, |this| {
                this.child(
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
            })
            .when_some(thumb_path, |this, thumb_path| {
                this.child(
                    div()
                        .size_full()
                        .flex_1()
                        .overflow_hidden()
                        .rounded_md()
                        .bg(cx.theme().secondary.opacity(0.3))
                        .child(
                            img(ImageSource::from(thumb_path))
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        ),
                )
            })
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
                            .child(format_file_size(self.size)),
                    ),
            )
            .when_some(on_secondary_click.clone(), |this, on_sec| {
                this.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                    cx.stop_propagation();
                    on_sec(window, cx);
                })
            })
            .on_click(
                move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
                    if event.click_count() == 2 {
                        if let Some(ref dbl_click) = on_double_click_handler {
                            dbl_click(event, window, cx);
                        }
                    } else if event.click_count() == 1
                        && let Some(ref click) = on_click_handler
                    {
                        click(event, window, cx);
                    }
                },
            )
    }
}
