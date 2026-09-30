use std::rc::Rc;

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::*;
use uuid::Uuid;

use crate::components;
use crate::services::importer::FileInfo;

/// Width one card occupies, including the gap that follows it.
const CARD_SLOT: f32 = 148.0;
/// Height of one grid row: the 150px card plus the 12px gap.
const ROW_HEIGHT: f32 = 162.0;

pub type FileGridSelectListener = Rc<dyn Fn(Uuid, &mut Window, &mut App) + 'static>;
pub type FileGridOpenListener = Rc<dyn Fn(&str, &mut Window, &mut App) + 'static>;

/// Virtualized grid of file cards.
#[derive(IntoElement)]
pub struct FileGrid {
    file_list: Vec<FileInfo>,
    container_width: Pixels,
    selected_file_id: Option<Uuid>,
    scroll_handle: UniformListScrollHandle,
    on_select: Option<FileGridSelectListener>,
    on_open: Option<FileGridOpenListener>,
}

impl FileGrid {
    pub fn new(file_list: Vec<FileInfo>) -> Self {
        Self {
            file_list,
            container_width: px(800.0),
            selected_file_id: None,
            scroll_handle: UniformListScrollHandle::new(),
            on_select: None,
            on_open: None,
        }
    }

    /// Width the grid may occupy; drives the column count.
    pub fn container_width(mut self, width: Pixels) -> Self {
        self.container_width = width;
        self
    }

    pub fn selected_file_id(mut self, id: Option<Uuid>) -> Self {
        self.selected_file_id = id;
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

    pub fn on_open(mut self, listener: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_open = Some(Rc::new(listener));
        self
    }

    /// Number of cards that fit on one row at the current width.
    fn columns(&self) -> usize {
        let available = (self.container_width - px(48.0)).max(px(152.0));
        (available.as_f32() / CARD_SLOT).floor().max(1.0) as usize
    }
}

impl RenderOnce for FileGrid {
    fn render(
        self,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> impl gpui_kit::prelude::IntoElement {
        if self.file_list.is_empty() {
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

        let cols = self.columns();
        let rows: Vec<Vec<FileInfo>> = self
            .file_list
            .chunks(cols)
            .map(|chunk| chunk.to_vec())
            .collect();
        let rows_count = rows.len();
        let selected_file_id = self.selected_file_id;
        let scroll_handle = self.scroll_handle;

        div()
            .relative()
            .flex_1()
            .size_full()
            .child(
                uniform_list("file-grid-list", rows_count, {
                    let on_select = self.on_select;
                    let on_open = self.on_open;
                    move |visible_range, _, _| {
                        visible_range
                            .map(|row_idx| {
                                render_row(&rows[row_idx], selected_file_id, &on_select, &on_open)
                            })
                            .collect()
                    }
                })
                .pb_6()
                .size_full()
                .track_scroll(&scroll_handle),
            )
            .vertical_scrollbar(&scroll_handle)
            .into_any_element()
    }
}

/// One row of cards. Takes owned rows so the result carries no borrow.
fn render_row(
    row_files: &[FileInfo],
    selected_file_id: Option<Uuid>,
    on_select: &Option<FileGridSelectListener>,
    on_open: &Option<FileGridOpenListener>,
) -> AnyElement {
    div()
        .h(px(ROW_HEIGHT))
        .h_flex()
        .gap_3()
        .children(row_files.iter().map(|item| {
            let extension = item
                .ext
                .as_deref()
                .unwrap_or_else(|| item.mime.split('/').nth(1).unwrap_or("file"))
                .to_uppercase();
            let file_id = item.file_id;
            let is_selected = selected_file_id == Some(file_id);
            let file_path = SharedString::new(item.path.as_str());
            let path_open = file_path.clone();
            let hash = SharedString::new(item.hash.as_str());
            let select = on_select.clone();
            let open = on_open.clone();

            components::file_card::FileCard::new(
                ElementId::Name(format!("file-card-{file_id}").into()),
                SharedString::new(item.file_name.as_str()),
                extension,
                item.size,
            )
            .path(file_path)
            .hash(hash)
            .selected(is_selected)
            .on_click(move |_, window, cx| {
                if let Some(listener) = &select {
                    listener(file_id, window, cx);
                }
            })
            .on_double_click(move |_, window, cx| {
                if let Some(listener) = &open {
                    listener(path_open.as_ref(), window, cx);
                }
            })
        }))
        .into_any_element()
}
