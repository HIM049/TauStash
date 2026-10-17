use std::rc::Rc;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub type CollectionCardClickListener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
pub type CollectionCardActionHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct CollectionCard {
    id: ElementId,
    name: SharedString,
    icon: SharedString,
    cover: SharedString,
    selected: bool,
    editing: bool,
    input: Option<Entity<InputState>>,
    on_click: Option<CollectionCardClickListener>,
    on_double_click: Option<CollectionCardClickListener>,
    on_secondary_click: Option<CollectionCardActionHandler>,
}

impl CollectionCard {
    pub fn new(id: impl Into<ElementId>, name: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: SharedString::default(),
            cover: SharedString::default(),
            selected: false,
            editing: false,
            input: None,
            on_click: None,
            on_double_click: None,
            on_secondary_click: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn cover(mut self, cover: impl Into<SharedString>) -> Self {
        self.cover = cover.into();
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn editing(mut self, editing: bool) -> Self {
        self.editing = editing;
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

impl RenderOnce for CollectionCard {
    fn render(
        self,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> impl gpui_kit::prelude::IntoElement {
        let is_selected = self.selected;
        let is_editing = self.editing;
        let on_click = self.on_click;
        let on_double_click = self.on_double_click;
        let on_secondary_click = self.on_secondary_click;

        let icon_path = if self.icon.is_empty() {
            format!("icons/{}.svg", "folder")
        } else {
            format!("icons/{}.svg", self.icon.to_string())
        };

        let cover_path: Option<SharedString> = match self.cover.to_string() {
            path if path.is_empty() || !std::path::Path::new(&path).is_file() => None,
            path => Some(path.into()),
        };

        // rename input or name label
        let label_zone = if is_editing && let Some(ref input) = self.input {
            div()
                .id("collection-card-input-wrap")
                .w_full()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(Input::new(input).small().focus_ring(false))
                .into_any_element()
        } else {
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .overflow_hidden()
                .text_ellipsis()
                .w_full()
                .text_center()
                .child(self.name)
                .into_any_element()
        };

        div()
            .id(self.id)
            .v_flex()
            .items_center()
            .justify_between()
            .w(px(140.0))
            .h(px(150.0))
            .p_3()
            .rounded(cx.theme().radius)
            .cursor_pointer()
            .when(is_selected, |s| s.bg(cx.theme().accent.opacity(0.9)))
            .when(!is_editing && !is_selected, |d| {
                d.hover(|style| style.bg(cx.theme().accent.opacity(0.6)))
            })
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .when_none(&cover_path, |this| {
                        this.h_flex().items_center().justify_center().child(
                            Icon::default()
                                .path(icon_path)
                                .size_10()
                                .text_color(cx.theme().primary),
                        )
                    })
                    .when_some(cover_path, |this, path| {
                        this.size_full()
                            .flex_1()
                            .overflow_hidden()
                            .rounded_md()
                            .bg(cx.theme().secondary.opacity(0.3))
                            .child(
                                gpui::img(gpui::ImageSource::from(path))
                                    .size_full()
                                    .object_fit(gpui::ObjectFit::Cover),
                            )
                    }),
            )
            .child(label_zone)
            .when_some(on_secondary_click.clone(), |this, on_sec| {
                this.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                    cx.stop_propagation();
                    on_sec(window, cx);
                })
            })
            .on_click(
                move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                    if event.click_count() == 2 {
                        if let Some(ref dbl) = on_double_click {
                            dbl(event, window, cx);
                        }
                    } else if event.click_count() == 1
                        && let Some(ref click) = on_click
                    {
                        click(event, window, cx);
                    }
                },
            )
    }
}
