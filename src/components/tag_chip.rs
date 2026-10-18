use std::rc::Rc;

use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TagChipState {
    // For FILTER mode
    #[default]
    Default,
    Active,

    // for editor mode
    Selected,
    PartialSelected,
    NoneSelected,
}

#[derive(IntoElement)]
pub struct TagChip {
    id: ElementId,
    name: SharedString,
    icon: Option<Icon>,
    trailing_icon: Option<Icon>,
    count: Option<usize>,
    color: Option<Hsla>,
    state: TagChipState,
    squared: bool,
    small: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl TagChip {
    pub fn new(id: impl Into<ElementId>, name: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: None,
            trailing_icon: None,
            count: None,
            color: None,
            state: TagChipState::Default,
            squared: false,
            small: false,
            on_click: None,
        }
    }

    pub fn squared(mut self) -> Self {
        self.squared = true;
        self
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn icon(mut self, icon: impl Into<Option<Icon>>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn trailing_icon(mut self, icon: impl Into<Option<Icon>>) -> Self {
        self.trailing_icon = icon.into();
        self
    }

    pub fn state(mut self, state: TagChipState) -> Self {
        self.state = state;
        self
    }

    pub fn count(mut self, count: impl Into<Option<usize>>) -> Self {
        self.count = count.into();
        self
    }

    pub fn color(mut self, color: impl Into<Option<Hsla>>) -> Self {
        self.color = color.into();
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl TagChip {
    fn get_theme(&self, theme: &Theme) -> (Hsla, Hsla, Hsla, Option<Icon>, bool) {
        if let Some(custom_color) = self.color {
            match self.state {
                TagChipState::Default => (
                    custom_color.opacity(0.2),
                    custom_color,
                    custom_color,
                    None,
                    false,
                ),
                TagChipState::Active | TagChipState::Selected => (
                    custom_color,
                    theme.primary_foreground,
                    custom_color,
                    Some(
                        Icon::new(IconName::Check)
                            .size_3()
                            .text_color(theme.primary_foreground),
                    ),
                    false,
                ),
                TagChipState::NoneSelected => (
                    custom_color.opacity(0.12),
                    custom_color.opacity(0.85),
                    custom_color.opacity(0.3),
                    Some(
                        Icon::new(IconName::Close)
                            .size_3()
                            .text_color(custom_color.opacity(0.85)),
                    ),
                    false,
                ),
                TagChipState::PartialSelected => (
                    custom_color.opacity(0.3),
                    custom_color,
                    custom_color,
                    Some(Icon::new(IconName::Minus).size_3().text_color(custom_color)),
                    false,
                ),
            }
        } else {
            match self.state {
                TagChipState::Default => {
                    (theme.secondary, theme.foreground, theme.border, None, false)
                }
                TagChipState::Active => (
                    theme.primary,
                    theme.primary_foreground,
                    theme.primary,
                    None,
                    false,
                ),
                TagChipState::NoneSelected => (
                    theme.secondary.opacity(0.3),
                    theme.muted_foreground,
                    theme.border.opacity(0.4),
                    Some(
                        Icon::new(IconName::Close)
                            .size_3()
                            .text_color(theme.muted_foreground),
                    ),
                    true,
                ),
                TagChipState::PartialSelected => (
                    theme.primary.opacity(0.15),
                    theme.foreground,
                    theme.primary,
                    Some(
                        Icon::new(IconName::Minus)
                            .size_3()
                            .text_color(theme.primary),
                    ),
                    false,
                ),
                TagChipState::Selected => (
                    theme.primary,
                    theme.primary_foreground,
                    theme.primary,
                    Some(
                        Icon::new(IconName::Check)
                            .size_3()
                            .text_color(theme.primary_foreground),
                    ),
                    false,
                ),
            }
        }
    }
}

impl RenderOnce for TagChip {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let (bg_color, text_color, _border_color, state_icon, is_dimmed) = self.get_theme(theme);
        let on_click = self.on_click.clone();
        let leading_icon = state_icon.or(self.icon);
        let is_clickable = on_click.is_some();
        let height = if self.small { px(26.0) } else { px(34.0) };

        div()
            .id(self.id)
            .h_flex()
            .items_center()
            .gap_1p5()
            .when_else(
                self.small,
                |this| this.px_2().py_0p5(),
                |this| this.px_3().py_1(),
            )
            .text_xs()
            // .border_1()
            // .border_color(border_color)
            .bg(bg_color)
            .text_color(text_color)
            .h(height)
            .when_else(
                self.squared,
                |this| this.rounded_md(),
                |this| this.rounded_full(),
            )
            .when(is_dimmed, |this| this.opacity(0.6))
            .hover(|style| match self.state {
                TagChipState::Active | TagChipState::Selected => {
                    if is_clickable {
                        style.opacity(0.85)
                    } else {
                        style
                    }
                }
                TagChipState::NoneSelected => style.opacity(0.8).bg(theme.accent),
                _ => style.bg(theme.accent),
            })
            .when_some(on_click, |this, handler| {
                this.on_click(move |ev, window, cx| {
                    handler(ev, window, cx);
                })
            })
            .when_some(leading_icon, |this, icon| this.child(icon))
            .child(div().child(self.name))
            .when_some(self.count, |this, count| {
                let badge_bg = match self.state {
                    TagChipState::Active | TagChipState::Selected => theme.background.opacity(0.2),
                    _ => theme.background,
                };
                let badge_text = match self.state {
                    TagChipState::Active | TagChipState::Selected => text_color,
                    _ => theme.muted_foreground,
                };

                this.child(
                    div()
                        .flex()
                        .justify_center()
                        .w_7()
                        .px_1p5()
                        .py_0p5()
                        .rounded_full()
                        .text_xs()
                        .bg(badge_bg)
                        .text_color(badge_text)
                        .child(count.to_string()),
                )
            })
            .when_some(self.trailing_icon, |this, icon| this.child(icon))
    }
}
