use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
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

/// Number of cards that fit on one row at the given width.
fn columns_for(width: Pixels) -> usize {
    let available = (width - px(16.0)).max(px(CARD_SLOT));
    (available.as_f32() / CARD_SLOT).floor().max(1.0) as usize
}

/// Virtualized grid of file cards.
pub struct FileGrid {
    file_list: Vec<FileInfo>,
    selected_files: HashSet<Uuid>,
    scroll_handle: UniformListScrollHandle,
    on_select: Option<FileGridSelectListener>,
    on_secondary_select: Option<FileGridSelectListener>,
    on_open: Option<FileGridOpenListener>,
}

impl FileGrid {
    pub fn new(file_list: Vec<FileInfo>) -> Self {
        Self {
            file_list,
            selected_files: HashSet::new(),
            scroll_handle: UniformListScrollHandle::new(),
            on_select: None,
            on_secondary_select: None,
            on_open: None,
        }
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
}

impl IntoElement for FileGrid {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FileGrid {
    type RequestLayoutState = ();
    type PrepaintState = Option<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        let layout_id = window.request_layout(style, [], cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if self.file_list.is_empty() {
            let mut empty = div()
                .h_flex()
                .items_center()
                .justify_center()
                .size_full()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("当前筛选下暂无匹配文件")
                .into_any_element();
            let available_space = size(
                AvailableSpace::Definite(bounds.size.width),
                AvailableSpace::Definite(bounds.size.height),
            );
            empty.prepaint_as_root(bounds.origin, available_space, window, cx);
            return Some(empty);
        }

        let cols = columns_for(bounds.size.width);
        let rows_count = self.file_list.len().div_ceil(cols);
        let total = self.file_list.len();

        let file_list = self.file_list.clone();
        let selected_files = self.selected_files.clone();
        let on_select = self.on_select.clone();
        let on_secondary_select = self.on_secondary_select.clone();
        let on_open = self.on_open.clone();
        let scroll_handle = self.scroll_handle.clone();

        let mut inner = components::lru_image_cache::lru_image_cache_default("file-grid-image-cache")
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
            .into_any_element();

        let available_space = size(
            AvailableSpace::Definite(bounds.size.width),
            AvailableSpace::Definite(bounds.size.height),
        );
        inner.prepaint_as_root(bounds.origin, available_space, window, cx);
        Some(inner)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(mut inner) = prepaint.take() {
            inner.paint(window, cx);
        }
    }
}

/// One row of cards. Takes owned rows so the result carries no borrow.
fn render_row(
    row_files: &[FileInfo],
    selected_files: &HashSet<Uuid>,
    on_select: &Option<FileGridSelectListener>,
    on_secondary_select: &Option<FileGridSelectListener>,
    on_open: &Option<FileGridOpenListener>,
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
                    listener(&path_open, window, cx);
                }
            })
        }))
        .into_any_element()
}
