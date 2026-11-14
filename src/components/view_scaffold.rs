use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub type BackHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;
pub type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
pub type SubClickHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;
pub type DropHandler = Box<dyn Fn(&ExternalPaths, &mut Window, &mut App) + 'static>;
pub type DragMoveHandler =
    Box<dyn Fn(&DragMoveEvent<ExternalPaths>, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct ViewScaffold {
    id: ElementId,
    style: StyleRefinement,
    title: SharedString,
    on_back: Option<BackHandler>,
    actions: Vec<AnyElement>,
    sub_bar: Option<AnyElement>,
    content: Option<AnyElement>,
    on_click: Option<ClickHandler>,
    on_sub_click: Option<SubClickHandler>,
    on_drop: Option<DropHandler>,
    on_drag_move: Option<DragMoveHandler>,
    drop_label: Option<SharedString>,
}

impl Styled for ViewScaffold {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ViewScaffold {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            title: title.into(),
            on_back: None,
            actions: Vec::new(),
            sub_bar: None,
            content: None,
            on_click: None,
            on_sub_click: None,
            on_drop: None,
            on_drag_move: None,
            drop_label: None,
        }
    }

    pub fn on_back(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_back = Some(Box::new(handler));
        self
    }

    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    pub fn actions(mut self, actions: Vec<impl IntoElement>) -> Self {
        self.actions = actions.into_iter().map(|a| a.into_any_element()).collect();
        self
    }

    pub fn sub_bar(mut self, sub_bar: impl IntoElement) -> Self {
        self.sub_bar = Some(sub_bar.into_any_element());
        self
    }

    pub fn child(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn on_sub_click(
        mut self,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_sub_click = Some(Box::new(handler));
        self
    }

    pub fn on_drop(
        mut self,
        handler: impl Fn(&ExternalPaths, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_drop = Some(Box::new(handler));
        self
    }

    pub fn on_dragging(
        mut self,
        handler: impl Fn(&DragMoveEvent<ExternalPaths>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_drag_move = Some(Box::new(handler));
        self
    }

    /// Show a full-size hint with this label while files are dragged over.
    pub fn drop_label(mut self, label: impl Into<SharedString>) -> Self {
        self.drop_label = Some(label.into());
        self
    }

    fn render_drop_overlay(label: SharedString, theme: &Theme) -> impl IntoElement {
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .opacity(0.)
            .p_8()
            .bg(theme.drop_target)
            .drag_over::<ExternalPaths>(|style, _, _, _| style.opacity(1.))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .items_center()
                    .size_full()
                    .border_8()
                    .border_dashed()
                    .border_color(theme.drag_border)
                    .rounded_3xl()
                    .opacity(0.8)
                    .text_color(theme.muted)
                    .text_2xl()
                    .font_extrabold()
                    .font_medium()
                    .child(label),
            )
    }
}

impl RenderOnce for ViewScaffold {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .id(self.id.clone())
            .relative()
            .size_full()
            .bg(theme.background)
            .v_flex()
            .gap_2()
            .when_some(self.on_click, |this, on_click| {
                this.on_click(move |event, window, cx| on_click(event, window, cx))
            })
            .when_some(self.on_sub_click, |this, on_click| {
                this.on_mouse_down(MouseButton::Right, move |e, w, cx| on_click(e, w, cx))
            })
            .when_some(self.on_drop, |this, on_drop| {
                this.on_drop(move |paths, window, cx| on_drop(paths, window, cx))
            })
            .when_some(self.on_drag_move, |this, on_move| {
                this.on_drag_move::<ExternalPaths>(move |e, window, cx| on_move(e, window, cx))
            })
            .child(
                div()
                    .id((self.id.clone(), "top-bar"))
                    .v_flex()
                    .gap_2()
                    .w_full()
                    // .bg(theme.muted)
                    .child(
                        div()
                            .id((self.id.clone(), "header"))
                            .h_flex()
                            .w_full()
                            .pt_6()
                            .px_6()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .when_some(self.on_back, |this, on_back| {
                                        this.child(
                                            Button::new((self.id.clone(), "back-btn"))
                                                .ghost()
                                                .small()
                                                .icon(IconName::ChevronLeft)
                                                .on_click(move |_, window, cx| {
                                                    on_back(window, cx);
                                                }),
                                        )
                                    })
                                    .child(div().text_xl().font_bold().child(self.title)),
                            )
                            .child(
                                div()
                                    .id((self.id.clone(), "actions"))
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .children(self.actions),
                            ),
                    )
                    .when_some(self.sub_bar, |this, sub_bar| {
                        this.child(
                            div()
                                .id((self.id.clone(), "sub-bar"))
                                .w_full()
                                .child(sub_bar),
                        )
                    }),
            )
            .child(
                div()
                    .id((self.id.clone(), "content"))
                    // .bg(theme.muted)
                    .border_t_1()
                    .border_color(theme.border)
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .when_some(self.content, |this, content| this.child(content)),
            )
            .when_some(self.drop_label, |this, label| {
                this.child(Self::render_drop_overlay(label, theme))
            })
            .refine_style(&self.style)
    }
}
