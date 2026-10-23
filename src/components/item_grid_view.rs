use std::collections::HashSet;
use std::rc::Rc;

use gpui::list;
use gpui_kit::component::input::InputState;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::*;
use uuid::Uuid;

use crate::components;
use crate::components::grid_card::{CardImage, GridCard};

pub trait CardItem: 'static + Clone {
    fn card_id(&self) -> Uuid;
    fn card_title(&self) -> SharedString;
    fn card_image(&self) -> CardImage;
}

pub type GridSelectListener = Rc<dyn Fn((Uuid, bool, bool), &mut Window, &mut App) + 'static>;
pub type GridActionHandler = Rc<dyn Fn(Uuid, &mut Window, &mut App) + 'static>;
pub type GridOpenHandler<T> = Rc<dyn Fn(T, &mut Window, &mut App) + 'static>;

#[derive(Clone)]
enum GridVirtualRow {
    Header(String),
    CardRow { start: usize, end: usize },
}

struct GridListState {
    list: ListState,
    count: usize,
}

pub struct ItemGridView<T: CardItem> {
    id: ElementId,
    items: Vec<T>,
    selected_ids: HashSet<Uuid>,
    cursor_id: Option<Uuid>,
    editing_id: Option<Uuid>,
    name_input: Option<Entity<InputState>>,
    card_slot_width: f32,
    row_height: f32,
    scroll_handle: UniformListScrollHandle,
    empty_placeholder: SharedString,
    group_by: Option<fn(&T) -> (String, String)>,
    on_select: Option<GridSelectListener>,
    on_secondary_select: Option<GridActionHandler>,
    on_open: Option<GridOpenHandler<T>>,
}

impl<T: CardItem> ItemGridView<T> {
    pub fn new(id: impl Into<ElementId>, items: Vec<T>) -> Self {
        Self {
            id: id.into(),
            items,
            selected_ids: HashSet::new(),
            cursor_id: None,
            editing_id: None,
            name_input: None,
            card_slot_width: 142.0,
            row_height: 154.0,
            scroll_handle: UniformListScrollHandle::new(),
            empty_placeholder: "暂无内容".into(),
            group_by: None,
            on_select: None,
            on_secondary_select: None,
            on_open: None,
        }
    }

    pub fn selected_ids(mut self, selected_ids: HashSet<Uuid>) -> Self {
        self.selected_ids = selected_ids;
        self
    }

    pub fn cursor_id(mut self, cursor_id: Option<Uuid>) -> Self {
        self.cursor_id = cursor_id;
        self
    }

    pub fn editing_id(mut self, editing_id: Option<Uuid>) -> Self {
        self.editing_id = editing_id;
        self
    }

    pub fn name_input(mut self, name_input: Option<Entity<InputState>>) -> Self {
        self.name_input = name_input;
        self
    }

    pub fn group_by(mut self, group_by: fn(&T) -> (String, String)) -> Self {
        self.group_by = Some(group_by);
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

    pub fn on_open(mut self, listener: impl Fn(T, &mut Window, &mut App) + 'static) -> Self {
        self.on_open = Some(Rc::new(listener));
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

    pub fn empty_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.empty_placeholder = placeholder.into();
        self
    }

    pub fn scroll_handle(mut self, handle: &UniformListScrollHandle) -> Self {
        self.scroll_handle = handle.clone();
        self
    }
}

impl<T: CardItem> IntoElement for ItemGridView<T> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<T: CardItem> Element for ItemGridView<T> {
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
        let available = (bounds.size.width.as_f32() - 16.0).max(slot);
        let cols = ((available / slot) as usize).max(1);
        let total = self.items.len();
        let rows_count = total.div_ceil(cols);

        let visible_rows = (bounds.size.height.as_f32() / row_h.max(1.0)).ceil() as usize;
        let single_screen_items = cols * (visible_rows + 4);
        let cache_capacity = (single_screen_items * 3).max(100);

        let virtual_rows: Vec<GridVirtualRow> = if let Some(key_fn) = self.group_by {
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

            for (i, item) in self.items.iter().enumerate() {
                let (id, title) = key_fn(item);

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

            if gid.is_some() {
                push_group(&gtitle, start, self.items.len());
            }
            rows
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

        let selected_ids = self.selected_ids.clone();
        let cursor_id = self.cursor_id;
        let editing_id = self.editing_id;
        let name_input = self.name_input.clone();
        let on_select = self.on_select.clone();
        let on_secondary_select = self.on_secondary_select.clone();
        let on_open = self.on_open.clone();

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
                                        let item = &items[idx];
                                        let id = item.card_id();
                                        let is_selected = selected_ids.contains(&id);
                                        let is_cursor = cursor_id == Some(id);
                                        let is_editing = editing_id == Some(id);

                                        let select = on_select.clone();
                                        let sec_select = on_secondary_select.clone();
                                        let open = on_open.clone();
                                        let item_clone = item.clone();

                                        let card = GridCard::new(item.card_id(), item.card_title(), item.card_image())
                                            .selected(is_selected)
                                            .cursor(is_cursor)
                                            .editing(is_editing)
                                            .input(if is_editing {
                                                name_input.clone()
                                            } else {
                                                None
                                            })
                                            .on_click(move |_, window, cx| {
                                                cx.stop_propagation();
                                                if is_editing {
                                                    return;
                                                }
                                                if let Some(ref select) = select {
                                                    let modifiers = window.modifiers();
                                                    let is_ctrl =
                                                        modifiers.control || modifiers.platform;
                                                    let is_shift = modifiers.shift;
                                                    select((id, is_ctrl, is_shift), window, cx);
                                                }
                                            })
                                            .on_double_click(move |_, window, cx| {
                                                cx.stop_propagation();
                                                if let Some(ref dbl) = open {
                                                    dbl(item_clone.clone(), window, cx);
                                                }
                                            })
                                            .on_secondary_click(move |window, cx| {
                                                if let Some(ref sec) = sec_select {
                                                    sec(id, window, cx);
                                                }
                                            });

                                        cells.push(card.into_any_element());
                                    }

                                    div()
                                        .h(px(row_h))
                                        .h_flex()
                                        .gap_1()
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
