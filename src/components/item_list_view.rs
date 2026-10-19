use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use uuid::Uuid;

use crate::components::utils::format_file_size;
use crate::services::file::FileInfo;

const ROW_HEIGHT: f32 = 36.0;

pub type FileListSelectListener = Rc<dyn Fn((Uuid, bool, bool), &mut Window, &mut App) + 'static>;
pub type FileListActionHandler = Rc<dyn Fn(Uuid, &mut Window, &mut App) + 'static>;
pub type FileListOpenHandler = Rc<dyn Fn(&str, &mut Window, &mut App) + 'static>;

/// A Windows Explorer-style details list view with column headers.
#[derive(IntoElement)]
pub struct ItemListView {
    id: ElementId,
    file_list: Vec<FileInfo>,
    selected_files: HashSet<Uuid>,
    cursor_id: Option<Uuid>,
    scroll_handle: UniformListScrollHandle,
    empty_placeholder: SharedString,
    on_select: Option<FileListSelectListener>,
    on_secondary_select: Option<FileListActionHandler>,
    on_open: Option<FileListOpenHandler>,
}

impl ItemListView {
    pub fn new(id: impl Into<ElementId>, file_list: Vec<FileInfo>) -> Self {
        Self {
            id: id.into(),
            file_list,
            selected_files: HashSet::new(),
            cursor_id: None,
            scroll_handle: UniformListScrollHandle::new(),
            empty_placeholder: "当前筛选下暂无匹配文件".into(),
            on_select: None,
            on_secondary_select: None,
            on_open: None,
        }
    }

    pub fn selected_files(mut self, files: HashSet<Uuid>) -> Self {
        self.selected_files = files;
        self
    }

    pub fn cursor_id(mut self, cursor_id: Option<Uuid>) -> Self {
        self.cursor_id = cursor_id;
        self
    }

    pub fn scroll_handle(mut self, handle: &UniformListScrollHandle) -> Self {
        self.scroll_handle = handle.clone();
        self
    }

    pub fn empty_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.empty_placeholder = placeholder.into();
        self
    }

    pub fn on_select(
        mut self,
        listener: impl Fn((Uuid, bool, bool), &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Rc::new(listener));
        self
    }

    pub fn on_secondary_select(
        mut self,
        listener: impl Fn(Uuid, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_secondary_select = Some(Rc::new(listener));
        self
    }

    pub fn on_open(mut self, listener: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_open = Some(Rc::new(listener));
        self
    }
}

impl RenderOnce for ItemListView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl gpui_kit::prelude::IntoElement {
        let border_color = cx.theme().border;
        if self.file_list.is_empty() {
            return div()
                .id(self.id)
                .h_flex()
                .items_center()
                .justify_center()
                .size_full()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(self.empty_placeholder)
                .into_any_element();
        }

        let header = div()
            .h(px(32.0))
            .w_full()
            .h_flex()
            .items_center()
            .px_3()
            .gap_2()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground)
            // .border_b_1()
            // .border_color(cx.theme().border.opacity(0.6))
            .child(
                div()
                    .border_r_1()
                    .border_color(border_color)
                    .flex_1()
                    .min_w(px(220.0))
                    .child("名称"),
            )
            .child(
                div()
                    .border_r_1()
                    .border_color(border_color)
                    .w(px(65.0))
                    .flex_shrink_0()
                    .child("类型"),
            )
            .child(
                div()
                    .border_r_1()
                    .border_color(border_color)
                    .w(px(65.0))
                    .flex_shrink_0()
                    .child("大小"),
            );

        let total = self.file_list.len();
        let file_list = self.file_list;
        let selected_files = self.selected_files;
        let cursor_id = self.cursor_id;
        let on_select = self.on_select;
        let on_secondary_select = self.on_secondary_select;
        let on_open = self.on_open;
        let scroll_handle = self.scroll_handle;

        let rows_id = ElementId::Name(format!("{}-rows", self.id).into());

        let list = uniform_list(rows_id, total, move |visible_range, _window, cx| {
            visible_range
                .map(|idx| {
                    let item = &file_list[idx];
                    let file_id = item.file_id;
                    let is_selected = selected_files.contains(&file_id);
                    let is_cursor = cursor_id == Some(file_id);
                    let path = item.path.clone();
                    let extension = item
                        .ext
                        .as_deref()
                        .unwrap_or_else(|| item.mime.split('/').nth(1).unwrap_or("file"))
                        .to_uppercase();

                    let select = on_select.clone();
                    let sec_select = on_secondary_select.clone();
                    let open = on_open.clone();

                    div()
                        .id(ElementId::Name(format!("file-row-{file_id}").into()))
                        .h(px(ROW_HEIGHT))
                        .w_full()
                        .h_flex()
                        .items_center()
                        .px_3()
                        .gap_2()
                        .border_1()
                        .when(is_selected, |this| this.bg(cx.theme().accent.opacity(0.9)))
                        .when(is_cursor, |this| {
                            this.border_color(cx.theme().border)
                                .bg(cx.theme().accent.opacity(0.6))
                        })
                        .hover(|s| {
                            if is_selected {
                                s.border_color(cx.theme().border)
                                    .bg(cx.theme().accent.opacity(0.6))
                            } else {
                                s.bg(cx.theme().accent.opacity(0.9))
                            }
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(220.0))
                                .h_flex()
                                .items_center()
                                .gap_2()
                                .overflow_hidden()
                                .child(
                                    Icon::new(IconName::File)
                                        .size_4()
                                        .text_color(cx.theme().primary),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .truncate()
                                        .text_color(cx.theme().foreground)
                                        .child(item.file_name.clone()),
                                ),
                        )
                        .child(
                            div()
                                .w(px(65.0))
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(extension),
                        )
                        .child(
                            div()
                                .w(px(65.0))
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .text_right()
                                .child(format_file_size(item.size)),
                        )
                        .when_some(sec_select, |this, on_sec| {
                            this.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                                cx.stop_propagation();
                                on_sec(file_id, window, cx);
                            })
                        })
                        .on_click(
                            move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
                                cx.stop_propagation();
                                if event.click_count() == 2 {
                                    if let Some(ref dbl) = open {
                                        dbl(&path, window, cx);
                                    }
                                } else if event.click_count() == 1 {
                                    if let Some(ref select) = select {
                                        let modifiers = window.modifiers();
                                        let is_ctrl = modifiers.control || modifiers.platform;
                                        let is_shift = modifiers.shift;
                                        select((file_id, is_ctrl, is_shift), window, cx);
                                    }
                                }
                            },
                        )
                        .into_any_element()
                })
                .collect()
        })
        .pb_6()
        .track_scroll(&scroll_handle)
        .size_full();

        div()
            .id(self.id)
            .v_flex()
            .size_full()
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .flex_1()
                    .size_full()
                    .relative()
                    .child(list)
                    .vertical_scrollbar(&scroll_handle),
            )
            .into_any_element()
    }
}
