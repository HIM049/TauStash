use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::components::utils::{format_file_size, open_path};
use crate::database::entities::FileState;

pub type FileRowClickListener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
pub type FileRowActionListener = Rc<dyn Fn(FileState, &mut Window, &mut App) + 'static>;

/// A single row in the review queue.
#[derive(IntoElement)]
pub struct FileRow {
    id: ElementId,
    file_name: SharedString,
    extension: SharedString,
    path: Option<SharedString>,
    hash: Option<SharedString>,
    size: i64,
    selected: bool,
    busy: bool,
    on_click: Option<FileRowClickListener>,
    on_double_click: Option<FileRowClickListener>,
    on_action: Option<FileRowActionListener>,
}

impl FileRow {
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
            busy: false,
            on_click: None,
            on_double_click: None,
            on_action: None,
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

    /// Disables the action buttons while a write for this row is in flight.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
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

    /// Receives the `FileState` chosen by one of the action buttons.
    pub fn on_action(
        mut self,
        listener: impl Fn(FileState, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_action = Some(Rc::new(listener));
        self
    }
}

impl RenderOnce for FileRow {
    fn render(
        self,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> impl gpui_kit::prelude::IntoElement {
        let is_selected = self.selected;
        let busy = self.busy;

        let on_click_handler = self.on_click;
        let on_double_click_handler = self.on_double_click;
        let on_action_handler = self.on_action;

        let path_for_menu = self.path.clone();
        let hash_for_menu = self.hash.clone();
        let path_for_text = self.path.clone();

        let archive = on_action_handler.clone();
        let ignore = on_action_handler.clone();
        let delete = on_action_handler;

        let row = div()
            .id(self.id)
            .h_flex()
            .items_center()
            .gap_3()
            .w_full()
            .h(px(56.0))
            .px_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .cursor_pointer()
            .when(is_selected, |s| s.bg(cx.theme().accent.opacity(0.9)))
            .hover(|style| {
                if !is_selected {
                    style.bg(cx.theme().accent.opacity(0.4))
                } else {
                    style
                }
            })
            // Leading icon + extension
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .w(px(104.0))
                    .flex_shrink_0()
                    .child(
                        Icon::new(IconName::File)
                            .size_4()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.extension),
                    ),
            )
            // Name over full path
            .child(
                div()
                    .v_flex()
                    .items_start()
                    .justify_center()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().foreground)
                            .overflow_hidden()
                            .text_ellipsis()
                            .w_full()
                            .child(self.file_name),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .overflow_hidden()
                            .text_ellipsis()
                            .w_full()
                            .child(path_for_text.unwrap_or_else(|| "(None)".into())),
                    ),
            )
            // Size
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .w(px(76.0))
                    .flex_shrink_0()
                    .text_right()
                    .child(format_file_size(self.size)),
            )
            // Review actions
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .child(
                        Button::new("row-archive")
                            .icon(IconName::Inbox)
                            .primary()
                            .compact()
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                if let Some(handler) = &archive {
                                    handler(FileState::Managed, window, cx);
                                }
                            }),
                    )
                    .child(
                        Button::new("row-ignore")
                            .icon(IconName::EyeOff)
                            .secondary()
                            .compact()
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                if let Some(handler) = &ignore {
                                    handler(FileState::Ignore, window, cx);
                                }
                            }),
                    )
                    .child(
                        Button::new("row-delete")
                            .icon(IconName::Delete)
                            .danger()
                            .compact()
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                if let Some(handler) = &delete {
                                    handler(FileState::Blocked, window, cx);
                                }
                            }),
                    ),
            )
            .on_click(move |event: &ClickEvent, window, cx| {
                if event.click_count() == 2 {
                    if let Some(ref dbl) = on_double_click_handler {
                        dbl(event, window, cx);
                    }
                } else if event.click_count() == 1
                    && let Some(ref click) = on_click_handler
                {
                    click(event, window, cx);
                }
            });

        if let Some(file_path) = path_for_menu {
            let open_p = file_path.clone();
            let copy_p = file_path;
            let copy_h = hash_for_menu;

            row.context_menu(move |menu, _window, _cx| {
                let open_p = open_p.clone();
                let copy_p = copy_p.clone();
                let copy_h = copy_h.clone();

                let mut m = menu
                    .item(
                        PopupMenuItem::new("打开")
                            .icon(IconName::ExternalLink)
                            .on_click(move |_, _, _| {
                                open_path(&open_p);
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
            row.into_any_element()
        }
    }
}
