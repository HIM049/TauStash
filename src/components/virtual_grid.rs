use std::rc::Rc;

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::*;

use crate::components;

pub type RenderCardFn<T> = Rc<dyn Fn(&T, usize, &mut Window, &mut App) -> AnyElement>;

pub struct VirtualGrid<T> {
    id: ElementId,
    items: Vec<T>,
    card_slot_width: f32,
    row_height: f32,
    scroll_handle: UniformListScrollHandle,
    empty_placeholder: SharedString,
    render_card: RenderCardFn<T>,
}

impl<T: 'static + Clone> VirtualGrid<T> {
    pub fn new(
        id: impl Into<ElementId>,
        items: Vec<T>,
        render_card: impl Fn(&T, usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            items,
            card_slot_width: 142.0,
            row_height: 154.0,
            scroll_handle: UniformListScrollHandle::new(),
            empty_placeholder: "暂无内容".into(),
            render_card: Rc::new(render_card),
        }
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

impl<T: 'static + Clone> IntoElement for VirtualGrid<T> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<T: 'static + Clone> Element for VirtualGrid<T> {
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

        let items = self.items.clone();
        let render_card = self.render_card.clone();
        let scroll_handle = self.scroll_handle.clone();
        let list_id = ElementId::Name(format!("{}-list", self.id).into());
        let cache_id = format!("{}-image-cache", self.id);

        let mut inner = components::lru_image_cache::lru_image_cache_default(cache_id)
            .size_full()
            .child(
                div()
                    .relative()
                    .size_full()
                    .child(
                        uniform_list(list_id, rows_count, move |visible_range, window, cx| {
                            visible_range
                                .map(|row| {
                                    let start = row * cols;
                                    let end = (start + cols).min(total);
                                    let mut cells: Vec<AnyElement> =
                                        Vec::with_capacity(end - start);
                                    for idx in start..end {
                                        cells.push(render_card(&items[idx], idx, window, cx));
                                    }

                                    div()
                                        .h(px(row_h))
                                        .h_flex()
                                        .gap_1()
                                        .children(cells)
                                        .into_any_element()
                                })
                                .collect()
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
