use std::rc::Rc;

use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

fn format_file_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub type FileCardClickListener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct FileCard {
    id: ElementId,
    file_name: SharedString,
    extension: SharedString,
    path: Option<SharedString>,
    hash: Option<SharedString>,
    size: i64,
    selected: bool,
    on_click: Option<FileCardClickListener>,
    on_double_click: Option<FileCardClickListener>,
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
            selected: false,
            on_click: None,
            on_double_click: None,
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

        let path_for_menu = self.path.clone();
        let hash_for_menu = self.hash.clone();

        let base_div = div()
            .id(self.id)
            .v_flex()
            .items_center()
            .justify_between()
            .w(px(140.0))
            .h(px(150.0))
            .p_3()
            .when(is_selected, |s| s.bg(cx.theme().accent.opacity(0.9)))
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .cursor_pointer()
            .hover(|style| {
                if !is_selected {
                    style.bg(cx.theme().accent.opacity(0.4))
                } else {
                    style
                }
            })
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
                            .child(format_file_size(self.size)),
                    ),
            )
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
            );

        if let Some(file_path) = path_for_menu {
            let p1 = file_path.clone();
            let p2 = file_path.clone();
            let p3 = file_path.clone();
            let h1 = hash_for_menu.clone();

            base_div
                .context_menu(move |menu, _window, _cx| {
                    let open_p = p1.clone();
                    let reveal_p = p2.clone();
                    let copy_p = p3.clone();
                    let copy_h = h1.clone();

                    let mut m = menu
                        .item(
                            PopupMenuItem::new("打开")
                                .icon(IconName::ExternalLink)
                                .on_click(move |_, _, _| {
                                    std::process::Command::new("explorer")
                                        .arg(open_p.as_ref())
                                        .spawn()
                                        .ok();
                                }),
                        )
                        .item(
                            PopupMenuItem::new("在文件管理器中显示")
                                .icon(IconName::Folder)
                                .on_click(move |_, _, _| {
                                    let win_path = reveal_p.replace('/', "\\");
                                    std::process::Command::new("explorer")
                                        .arg("/select,")
                                        .arg(&win_path)
                                        .spawn()
                                        .ok();
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("复制路径")
                                .icon(IconName::Copy)
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_p.to_string(),
                                    ));
                                }),
                        );

                    if let Some(h) = copy_h {
                        m = m.item(PopupMenuItem::new("复制哈希").on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(h.to_string()));
                        }));
                    }

                    m
                })
                .into_any_element()
        } else {
            base_div.into_any_element()
        }
    }
}
