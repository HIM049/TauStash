use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::components::item_grid_view::GroupCalc;
use crate::database::entities::collection;
use crate::database::storage::query::FileSortField;
use crate::services::collection::GroupMode;
use crate::services::{FileInfo, OrderMeta};

pub type FileCardClickListener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
pub type FileCardActionHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

#[derive(Clone, Debug)]
pub enum CardImage {
    Image(PathBuf),
    Icon {
        name: SharedString,
        label: Option<SharedString>,
    },
}

#[derive(Clone, IntoElement)]
pub struct GridCard {
    id: ElementId,
    title: SharedString,
    image: CardImage,
    is_selected: bool,
    is_editing: bool,
    is_cursor: bool,
    input: Option<Entity<InputState>>,
    on_click: Option<FileCardClickListener>,
    on_double_click: Option<FileCardClickListener>,
    on_secondary_click: Option<FileCardActionHandler>,
    meta: Option<OrderMeta>,
}

impl GridCard {
    pub fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        image: CardImage,
        meta: Option<OrderMeta>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            image,
            is_selected: false,
            is_editing: false,
            is_cursor: false,
            input: None,
            on_click: None,
            on_double_click: None,
            on_secondary_click: None,
            meta,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }

    pub fn editing(mut self, editing: bool) -> Self {
        self.is_editing = editing;
        self
    }

    pub fn cursor(mut self, cursor: bool) -> Self {
        self.is_cursor = cursor;
        self
    }

    pub fn input(mut self, input: impl Into<Option<Entity<InputState>>>) -> Self {
        self.input = input.into();
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

impl From<&FileInfo> for GridCard {
    fn from(file: &FileInfo) -> Self {
        let image = if file.mime.starts_with("image/") {
            let p = crate::services::thumbnail::get_thumbnail_path(&file.hash);
            CardImage::Image(p)
        } else {
            let ext = file.ext.as_deref().unwrap_or("FILE");
            CardImage::Icon {
                name: IconName::File.path(),
                label: Some(ext.into()),
            }
        };

        Self::new(
            file.file_id,
            file.file_name.clone(),
            image,
            Some(file.meta.clone()),
        )
    }
}

impl From<&collection::Model> for GridCard {
    fn from(folder: &collection::Model) -> Self {
        let image = if !folder.cover.is_empty() {
            CardImage::Image(PathBuf::from(&folder.cover))
        } else {
            CardImage::Icon {
                name: folder.icon.clone().into(),
                label: None,
            }
        };

        Self::new(folder.id, folder.name.clone(), image, None)
    }
}

impl GroupCalc for GridCard {
    fn group_calc(
        &self,
        sort_field: FileSortField,
        group_mode: GroupMode,
    ) -> Option<(String, String)> {
        self.meta
            .as_ref()
            .map(|m| m.calc_group(sort_field, group_mode))
            .flatten()
    }
}

impl RenderOnce for GridCard {
    fn render(
        self,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> impl gpui_kit::prelude::IntoElement {
        let is_selected = self.is_selected;
        let is_editing = self.is_editing;

        let on_click_handler = self.on_click;
        let on_double_click_handler = self.on_double_click;
        let on_secondary_click = self.on_secondary_click;

        let image = match &self.image {
            CardImage::Image(path) => div()
                .flex_1()
                .min_h_0()
                .w_full()
                .h_flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .rounded_md()
                .child(
                    img(ImageSource::from(path.clone()))
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                ),
            CardImage::Icon { name: icon, label } => div()
                .flex_1()
                .min_h_0()
                .w_full()
                .v_flex()
                .items_center()
                .justify_center()
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
                            Icon::default()
                                .path(icon)
                                .size_5()
                                .text_color(cx.theme().primary),
                        ),
                )
                .when_some(label.clone(), |this, label| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(label.clone()),
                    )
                }),
        };

        let label_zone = if is_editing && let Some(ref input) = self.input {
            div()
                .id("file-card-input-wrap")
                .w_full()
                .h(px(34.0))
                .flex_shrink_0()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(Input::new(input).small().focus_bordered(false))
                .into_any_element()
        } else {
            div()
                .w_full()
                .h(px(34.0))
                .flex_shrink_0()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .line_clamp(2)
                .text_ellipsis()
                .text_center()
                .child(self.title)
                .into_any_element()
        };

        div()
            .id(self.id)
            .v_flex()
            .items_center()
            .gap_1p5()
            .w(px(140.0))
            .h(px(150.0))
            .p_2p5()
            .rounded(cx.theme().radius)
            .border_1()
            .when(is_selected, |this| this.bg(cx.theme().accent.opacity(0.9)))
            .when(self.is_cursor, |this| {
                this.border_color(cx.theme().border)
            })
            .hover(|s| {
                if is_selected {
                    s.border_color(cx.theme().border)
                        .bg(cx.theme().accent.opacity(0.6))
                } else {
                    s.border_color(cx.theme().accent.opacity(0.9))
                        .bg(cx.theme().accent.opacity(0.9))
                }
            })
            .when_some(on_secondary_click.clone(), |this, on_sec| {
                this.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                    cx.stop_propagation();
                    on_sec(window, cx);
                })
            })
            .on_click(
                move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
                    if is_editing {
                        return;
                    }
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
            )
            .child(image)
            .child(label_zone)
    }
}
