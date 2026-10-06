use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use uuid::Uuid;

use crate::components;
use crate::services::importer::FileInfo;

/// Width one card occupies, including the gap that follows it.
const CARD_SLOT: f32 = 142.0;
/// Height of one grid row: the 150px card plus the 12px gap.
const ROW_HEIGHT: f32 = 154.0;

pub type FileGridSelectListener = Rc<dyn Fn(Uuid, &mut Window, &mut App) + 'static>;
pub type FileGridOpenListener = Rc<dyn Fn(&str, &mut Window, &mut App) + 'static>;
pub type FileGridContextMenuBuilder =
    Rc<dyn Fn(Uuid, PopupMenu, &mut Window, &mut App) -> PopupMenu + 'static>;

/// Number of cards that fit on one row at the given width.
fn columns_for(width: Pixels) -> usize {
    let available = (width - px(48.0)).max(px(152.0));
    (available.as_f32() / CARD_SLOT).floor().max(1.0) as usize
}

/// Virtualized grid of file cards.
#[derive(IntoElement)]
pub struct FileGrid {
    file_list: Vec<FileInfo>,
    container_width: Pixels,
    selected_files: HashSet<Uuid>,
    scroll_handle: UniformListScrollHandle,
    on_select: Option<FileGridSelectListener>,
    on_secondary_select: Option<FileGridSelectListener>,
    on_open: Option<FileGridOpenListener>,
    context_menu_builder: Option<FileGridContextMenuBuilder>,
}

impl FileGrid {
    pub fn new(file_list: Vec<FileInfo>) -> Self {
        Self {
            file_list,
            container_width: px(800.0),
            selected_files: HashSet::new(),
            scroll_handle: UniformListScrollHandle::new(),
            on_select: None,
            on_secondary_select: None,
            on_open: None,
            context_menu_builder: None,
        }
    }

    /// Width the grid may occupy; drives the column count.
    pub fn container_width(mut self, width: Pixels) -> Self {
        self.container_width = width;
        self
    }

    pub fn selected_files(mut self, files: HashSet<Uuid>) -> Self {
        self.selected_files = files;
        self
    }

    /// Handle used to scroll the grid from outside, e.g. to reset on filter change.
    pub fn scroll_handle(mut self, handle: &UniformListScrollHandle) -> Self {
        self.scroll_handle = handle.clone();
        self
    }

    pub fn on_select(mut self, listener: impl Fn(Uuid, &mut Window, &mut App) + 'static) -> Self {
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

    pub fn context_menu(
        mut self,
        builder: impl Fn(Uuid, PopupMenu, &mut Window, &mut App) -> PopupMenu + 'static,
    ) -> Self {
        self.context_menu_builder = Some(Rc::new(builder));
        self
    }
}

impl RenderOnce for FileGrid {
    fn render(
        self,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> impl gpui_kit::prelude::IntoElement {
        let FileGrid {
            file_list,
            container_width,
            selected_files,
            scroll_handle,
            on_select,
            on_secondary_select,
            on_open,
            context_menu_builder,
        } = self;

        if file_list.is_empty() {
            return div()
                .h_flex()
                .items_center()
                .justify_center()
                .size_full()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("当前筛选下暂无匹配文件")
                .into_any_element();
        }

        let cols = columns_for(container_width);
        let rows_count = file_list.len().div_ceil(cols);
        let total = file_list.len();

        components::lru_image_cache::lru_image_cache_default("file-grid-image-cache")
            .flex_1()
            .size_full()
            .child(
                div()
                    .relative()
                    .size_full()
                    .child(
                        uniform_list("file-grid-list", rows_count, {
                            move |visible_range, _, _| {
                                visible_range
                                    .map(|row| {
                                        let start = row * cols;
                                        let end = (start + cols).min(total);
                                        render_row(
                                            &file_list[start..end],
                                            &selected_files,
                                            &on_select,
                                            &on_secondary_select,
                                            &on_open,
                                            &context_menu_builder,
                                        )
                                    })
                                    .collect()
                            }
                        })
                        .pb_6()
                        .size_full()
                        .track_scroll(&scroll_handle),
                    )
                    .vertical_scrollbar(&scroll_handle),
            )
            .into_any_element()
    }
}

/// One row of cards. Takes owned rows so the result carries no borrow.
fn render_row(
    row_files: &[FileInfo],
    selected_files: &HashSet<Uuid>,
    on_select: &Option<FileGridSelectListener>,
    on_secondary_select: &Option<FileGridSelectListener>,
    on_open: &Option<FileGridOpenListener>,
    context_menu_builder: &Option<FileGridContextMenuBuilder>,
) -> AnyElement {
    div()
        .h(px(ROW_HEIGHT))
        .h_flex()
        .gap_1()
        .children(row_files.iter().map(|item| {
            let extension = item
                .ext
                .as_deref()
                .unwrap_or_else(|| item.mime.split('/').nth(1).unwrap_or("file"))
                .to_uppercase();
            let file_id = item.file_id;
            let is_selected = selected_files.contains(&file_id);
            let file_path = SharedString::new(item.path.as_str());
            let path_open = file_path.clone();
            let hash = SharedString::new(item.hash.as_str());
            let mime = SharedString::new(item.mime.as_str());
            let select = on_select.clone();
            let sec_select = on_secondary_select.clone();
            let open = on_open.clone();
            let menu_builder = context_menu_builder.clone();

            components::file_card::FileCard::new(
                ElementId::Name(format!("file-card-{file_id}").into()),
                SharedString::new(item.file_name.as_str()),
                extension,
                item.size,
            )
            .path(file_path)
            .hash(hash)
            .mime(mime)
            .selected(is_selected)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                if let Some(listener) = &select {
                    listener(file_id, window, cx);
                }
            })
            .on_secondary_click(move |window, cx| {
                if let Some(listener) = &sec_select {
                    listener(file_id, window, cx);
                }
            })
            .on_double_click(move |_, window, cx| {
                if let Some(listener) = &open {
                    listener(path_open.as_ref(), window, cx);
                }
            })
            .when_some(menu_builder, |card, builder| {
                card.context_menu(move |menu, window, cx| {
                    builder(file_id, menu, window, cx)
                })
            })
        }))
        .into_any_element()
}
