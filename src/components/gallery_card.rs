use std::rc::Rc;

use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::components::grid_card::{CardImage, FileCardActionHandler, FileCardClickListener};
use crate::components::item_grid_view::GroupCalc;
use crate::database::storage::query::FileSortField;
use crate::services::collection::GroupMode;
use crate::services::{FileInfo, OrderMeta};

const GALLERY_CARD_SIZE: f32 = 140.0;

#[derive(Clone, IntoElement)]
pub struct GalleryCard {
    id: ElementId,
    title: SharedString,
    image: CardImage,
    is_selected: bool,
    is_cursor: bool,
    on_click: Option<FileCardClickListener>,
    on_double_click: Option<FileCardClickListener>,
    on_secondary_click: Option<FileCardActionHandler>,
    meta: OrderMeta,
}

impl GalleryCard {
    pub fn selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }

    pub fn cursor(mut self, cursor: bool) -> Self {
        self.is_cursor = cursor;
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

impl From<&FileInfo> for GalleryCard {
    fn from(file: &FileInfo) -> Self {
        let image = file
            .thumb
            .as_ref()
            .map(|t| CardImage::Image(t.clone()))
            .unwrap_or({
                let ext = file.ext.as_deref().unwrap_or("FILE");
                CardImage::Icon {
                    name: file.type_icon().path(),
                    label: Some(ext.into()),
                }
            });
        Self {
            id: file.file_id.into(),
            title: SharedString::new(file.file_name.as_str()),
            image,
            is_selected: false,
            is_cursor: false,
            on_click: None,
            on_double_click: None,
            on_secondary_click: None,
            meta: file.meta.clone(),
        }
    }
}

impl GroupCalc for GalleryCard {
    fn group_calc(
        &self,
        sort_field: FileSortField,
        group_mode: GroupMode,
    ) -> Option<(String, String)> {
        self.meta.calc_group(sort_field, group_mode)
    }
}

impl RenderOnce for GalleryCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_selected = self.is_selected;
        let on_click_handler = self.on_click;
        let on_double_click_handler = self.on_double_click;
        let on_secondary_click = self.on_secondary_click;

        div()
            .id(self.id)
            .group("gallery-card")
            .w(px(GALLERY_CARD_SIZE))
            .h(px(GALLERY_CARD_SIZE))
            .overflow_hidden()
            .bg(cx.theme().muted)
            .when(is_selected, |this| this.bg(cx.theme().accent).p_4())
            .child(match self.image {
                CardImage::Image(path) => img(ImageSource::from(path))
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .when(is_selected, |this| this.rounded_lg())
                    .group_hover("gallery-card", |style| style.opacity(0.8))
                    .into_any_element(),
                CardImage::Icon { name, .. } => div()
                    .size_full()
                    .bg(cx.theme().muted.opacity(0.4))
                    .v_flex()
                    .items_center()
                    .justify_center()
                    .gap_1p5()
                    .px_2()
                    .when(is_selected, |this| this.rounded_lg())
                    .group_hover("gallery-card", |style| style.opacity(0.8))
                    .child(
                        Icon::default()
                            .path(name)
                            .size_8()
                            .text_color(cx.theme().primary),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .text_center()
                            .line_clamp(2)
                            .text_ellipsis()
                            .child(self.title),
                    )
                    .into_any_element(),
            })
            .when_some(on_secondary_click, |this, on_sec| {
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
                    } else if event.click_count() == 1 {
                        if let Some(ref click) = on_click_handler {
                            click(event, window, cx);
                        }
                    }
                },
            )
    }
}
