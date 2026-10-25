use std::rc::Rc;

use gpui::list;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::*;

use crate::components;
use crate::database::storage::query::FileSortField;
use crate::services::collection::GroupMode;

pub trait GroupCalc {
    fn group_calc(
        &self,
        sort_field: FileSortField,
        group_mode: GroupMode,
    ) -> Option<(String, String)>;
}

#[derive(Clone)]
enum GridVirtualRow {
    Header(String),
    CardRow { start: usize, end: usize },
}

struct GridListState {
    list: ListState,
    count: usize,
}

pub struct ItemGridView<T: GroupCalc + IntoElement + Clone + 'static> {
    id: ElementId,
    items: Vec<T>,
    card_slot_width: f32,
    row_height: f32,
    gap: Pixels,
    scroll_handle: UniformListScrollHandle,
    empty_placeholder: SharedString,
    sort_field: FileSortField,
    group_mode: GroupMode,
}

impl<T: GroupCalc + IntoElement + Clone + 'static> ItemGridView<T> {
    pub fn new(id: impl Into<ElementId>, items: Vec<T>) -> Self {
        Self {
            id: id.into(),
            items,
            card_slot_width: 142.0,
            row_height: 154.0,
            gap: px(2.0),
            scroll_handle: UniformListScrollHandle::new(),
            empty_placeholder: "暂无内容".into(),
            sort_field: FileSortField::default(),
            group_mode: GroupMode::default(),
        }
    }

    pub fn slot_size(mut self, width: f32, height: f32) -> Self {
        self.card_slot_width = width;
        self.row_height = height;
        self
    }

    pub fn card_slot_width(mut self, width: f32) -> Self {
        self.card_slot_width = width;
        self
    }

    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
        self
    }

    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self
    }

    pub fn sort_field(mut self, sort_field: FileSortField) -> Self {
        self.sort_field = sort_field;
        self
    }

    pub fn group_mode(mut self, group_mode: GroupMode) -> Self {
        self.group_mode = group_mode;
        self
    }

    pub fn empty_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.empty_placeholder = placeholder.into();
        self
    }

    pub fn scroll_handle(mut self, handle: &UniformListScrollHandle) -> Self {
        self.scroll_handle = handle.clone();
        self
    }
}

impl<T: GroupCalc + IntoElement + Clone + 'static> IntoElement for ItemGridView<T> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<T: GroupCalc + IntoElement + Clone + 'static> Element for ItemGridView<T> {
    type RequestLayoutState = ();
    type PrepaintState = Option<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
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
        if self.items.is_empty() {
            let mut empty = div()
                .h_flex()
                .items_center()
                .justify_center()
                .size_full()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(self.empty_placeholder.clone())
                .into_any_element();
            let available_space = size(
                AvailableSpace::Definite(bounds.size.width),
                AvailableSpace::Definite(bounds.size.height),
            );
            empty.prepaint_as_root(bounds.origin, available_space, window, cx);
            return Some(empty);
        }

        let slot = self.card_slot_width;
        let row_h = self.row_height;
        let gap = self.gap;
        let available = (bounds.size.width.as_f32() - 16.0).max(slot);
        let cols = ((available / slot) as usize).max(1);
        let total = self.items.len();
        let rows_count = total.div_ceil(cols);

        let visible_rows = (bounds.size.height.as_f32() / row_h.max(1.0)).ceil() as usize;
        let single_screen_items = cols * (visible_rows + 4);
        let cache_capacity = (single_screen_items * 3).max(100);

        let sort_field = self.sort_field;
        let group_mode = self.group_mode;

        let virtual_rows: Vec<GridVirtualRow> = if group_mode != GroupMode::None {
            let mut rows = Vec::new();
            let mut gid: Option<String> = None;
            let mut gtitle = String::new();
            let mut start = 0;

            let mut push_group = |title: &str, start_idx: usize, end_idx: usize| {
                rows.push(GridVirtualRow::Header(title.to_string()));
                let mut chunk_start = start_idx;
                while chunk_start < end_idx {
                    let chunk_end = (chunk_start + cols).min(end_idx);
                    rows.push(GridVirtualRow::CardRow {
                        start: chunk_start,
                        end: chunk_end,
                    });
                    chunk_start = chunk_end;
                }
            };

            let mut has_grouped = false;
            for (i, item) in self.items.iter().enumerate() {
                if let Some((id, title)) = item.group_calc(sort_field, group_mode) {
                    has_grouped = true;
                    if gid.is_none() {
                        gid = Some(id.clone());
                        gtitle = title.clone();
                        start = i;
                    }

                    if id == *gid.as_ref().unwrap() {
                        continue;
                    }

                    push_group(&gtitle, start, i);
                    gid = Some(id);
                    gtitle = title;
                    start = i;
                }
            }

            if has_grouped && gid.is_some() {
                push_group(&gtitle, start, total);
                rows
            } else {
                (0..rows_count)
                    .map(|row| {
                        let start = row * cols;
                        let end = (start + cols).min(total);
                        GridVirtualRow::CardRow { start, end }
                    })
                    .collect()
            }
        } else {
            (0..rows_count)
                .map(|row| {
                    let start = row * cols;
                    let end = (start + cols).min(total);
                    GridVirtualRow::CardRow { start, end }
                })
                .collect()
        };

        let rows_total = virtual_rows.len();
        let state_entity = window.use_keyed_state(
            SharedString::from(format!("grid-list-state:{}", self.id)),
            cx,
            |_, _| GridListState {
                list: ListState::new(rows_total, ListAlignment::Top, px(row_h * 2.0)).measure_all(),
                count: rows_total,
            },
        );
        let list_state = state_entity.read(cx).list.clone();
        if state_entity.read(cx).count != rows_total {
            state_entity.update(cx, |s, _| {
                s.list.reset(rows_total);
                s.count = rows_total;
            });
        }

        let items = std::mem::take(&mut self.items);
        let cache_id = format!("{}-image-cache", self.id);
        let v_rows = Rc::new(virtual_rows);

        let mut inner = components::lru_image_cache::lru_image_cache(cache_id, cache_capacity)
            .size_full()
            .child(
                div()
                    .relative()
                    .size_full()
                    .child(
                        list(
                            list_state.clone(),
                            move |row_ix, _window, cx| match &v_rows[row_ix] {
                                GridVirtualRow::Header(title) => div()
                                    .w_full()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .px_1()
                                    .pt_3()
                                    .pb_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(title.clone()),
                                    )
                                    .into_any_element(),
                                GridVirtualRow::CardRow { start, end } => {
                                    let mut cells: Vec<AnyElement> =
                                        Vec::with_capacity(end - start);
                                    for idx in *start..*end {
                                        cells.push(items[idx].clone().into_any_element());
                                    }

                                    div()
                                        .pb(gap)
                                        .h_flex()
                                        .gap(gap)
                                        .children(cells)
                                        .into_any_element()
                                }
                            },
                        )
                        .pb_6()
                        .size_full(),
                    )
                    .vertical_scrollbar(&list_state),
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
