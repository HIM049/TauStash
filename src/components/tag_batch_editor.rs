use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Enter, Escape, Input, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use uuid::Uuid;

use crate::components::tag_chip::{TagChip, TagChipState};
use crate::database::entities::TagNamespace;
use crate::services::importer::{FileInfo, TagRef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditState {
    All,
    Partial,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedTag {
    pub tag_ref: TagRef,
    pub original: EditState,
    pub target: EditState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagDeltaBatch {
    pub to_attach: Vec<Uuid>,
    pub to_detach: Vec<Uuid>,
    pub to_create_and_attach: Vec<String>,
}

impl TagDeltaBatch {
    pub fn is_empty(&self) -> bool {
        self.to_attach.is_empty()
            && self.to_detach.is_empty()
            && self.to_create_and_attach.is_empty()
    }
}

pub type CommitListener = Rc<dyn Fn(&TagDeltaBatch, &mut Window, &mut App) + 'static>;
pub type CancelListener = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

pub struct TagBatchEditor {
    focus_handle: FocusHandle,
    selected_count: usize,
    tracked_tags: Vec<TrackedTag>,

    is_adding: bool,
    input_state: Entity<InputState>,

    on_commit: Option<CommitListener>,
    on_cancel: Option<CancelListener>,
}

impl TagBatchEditor {
    pub fn new(
        selected_files: &[FileInfo],
        popular_tags: &[TagRef],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let total = selected_files.len();

        let mut tag_counts: HashMap<TagRef, usize> = HashMap::new();
        for file in selected_files {
            for tag in &file.tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        let mut sorted_tags: Vec<(TagRef, usize)> = tag_counts.into_iter().collect();
        sorted_tags.sort_by(|(tag_a, count_a), (tag_b, count_b)| {
            count_b
                .cmp(count_a)
                .then_with(|| tag_a.name.cmp(&tag_b.name))
        });

        let mut tracked_tags: Vec<TrackedTag> = sorted_tags
            .into_iter()
            .map(|(tag_ref, count)| {
                let (original, target) = if count == total {
                    (EditState::All, EditState::All)
                } else {
                    (EditState::Partial, EditState::Partial)
                };
                TrackedTag {
                    tag_ref,
                    original,
                    target,
                }
            })
            .collect();

        for candidate in popular_tags {
            if !tracked_tags.iter().any(|t| t.tag_ref.id == candidate.id) {
                tracked_tags.push(TrackedTag {
                    tag_ref: candidate.clone(),
                    original: EditState::None,
                    target: EditState::None,
                });
            }
        }

        let input_state = cx.new(|cx| InputState::new(window, cx).placeholder("输入标签名称"));

        Self {
            focus_handle: cx.focus_handle(),
            selected_count: total,
            tracked_tags,
            is_adding: false,
            input_state,
            on_commit: None,
            on_cancel: None,
        }
    }

    pub fn on_commit(&mut self, handler: impl Fn(&TagDeltaBatch, &mut Window, &mut App) + 'static) {
        self.on_commit = Some(Rc::new(handler));
    }

    pub fn on_cancel(&mut self, handler: impl Fn(&mut Window, &mut App) + 'static) {
        self.on_cancel = Some(Rc::new(handler));
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle.focus(window, cx);
    }

    pub fn compute_delta(&self) -> TagDeltaBatch {
        let mut delta = TagDeltaBatch::default();

        for item in &self.tracked_tags {
            match (item.original, item.target) {
                // all -> none: delete all
                (EditState::All, EditState::None) => {
                    delta.to_detach.push(item.tag_ref.id);
                }
                // part -> all: fill all
                (EditState::Partial, EditState::All) => {
                    delta.to_attach.push(item.tag_ref.id);
                }
                // part -> none: delete all
                (EditState::Partial, EditState::None) => {
                    delta.to_detach.push(item.tag_ref.id);
                }
                // none -> all: Create and Attach
                (EditState::None, EditState::All) => {
                    delta.to_create_and_attach.push(item.tag_ref.name.clone());
                }
                // ignore (All->All, Partial->Partial, None->None)
                _ => {}
            }
        }

        delta
    }

    pub fn has_changes(&self) -> bool {
        !self.compute_delta().is_empty()
    }

    fn toggle_tag(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(item) = self.tracked_tags.get_mut(index) {
            item.target = match item.original {
                // OG: all
                EditState::All => match item.target {
                    EditState::All => EditState::None,
                    _ => EditState::All,
                },
                // OG: part. 3 state
                EditState::Partial => match item.target {
                    EditState::Partial => EditState::All,
                    EditState::All => EditState::None,
                    EditState::None => EditState::Partial,
                },
                // OG: none (new)
                EditState::None => match item.target {
                    EditState::All => EditState::None,
                    _ => EditState::All,
                },
            };
            cx.notify();
        }
    }

    fn confirm_new_tag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .input_state
            .read(cx)
            .text()
            .to_string()
            .trim()
            .to_string();
        if !name.is_empty() {
            if let Some(item) = self
                .tracked_tags
                .iter_mut()
                .find(|t| t.tag_ref.name == name)
            {
                item.target = EditState::All;
            } else {
                let tag_ref = TagRef {
                    id: Uuid::now_v7(),
                    namespace: TagNamespace::User,
                    name,
                };
                self.tracked_tags.push(TrackedTag {
                    tag_ref,
                    original: EditState::None,
                    target: EditState::All,
                });
            }
        }
        self.input_state = cx.new(|cx| InputState::new(window, cx).placeholder("输入标签名称"));
        self.is_adding = false;
        cx.notify();
    }

    fn commit(&self, window: &mut Window, cx: &mut App) {
        if let Some(ref cb) = self.on_commit {
            let delta = self.compute_delta();
            cb(&delta, window, cx);
        }
    }

    fn cancel(&self, window: &mut Window, cx: &mut App) {
        if let Some(ref cb) = self.on_cancel {
            cb(window, cx);
        }
    }
}

impl Render for TagBatchEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let delta = self.compute_delta();
        let has_changes = !delta.is_empty();

        let mut chips = Vec::with_capacity(self.tracked_tags.len() + 1);

        for (idx, item) in self.tracked_tags.iter().enumerate() {
            let chip_state = match item.target {
                EditState::All => TagChipState::Selected,
                EditState::Partial => TagChipState::PartialSelected,
                EditState::None => TagChipState::NoneSelected,
            };

            let chip = TagChip::new(
                format!("tag-edit-{}", item.tag_ref.id),
                item.tag_ref.name.clone(),
            )
            .state(chip_state)
            .squared()
            .on_click(cx.listener(move |this, _ev, _window, cx| {
                cx.stop_propagation();
                this.toggle_tag(idx, cx);
            }));

            chips.push(chip.into_any_element());
        }

        div()
            .id("tools-bar")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Enter, window, cx| {
                if this.has_changes() {
                    this.commit(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.cancel(window, cx);
            }))
            .h_flex()
            .items_center()
            .justify_between()
            .w_full()
            .py_1()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .flex_1()
                    .gap_2()
                    .overflow_x_scrollbar()
                    .children(chips)
                    .when_else(
                        self.is_adding,
                        |t| {
                            t.child(
                                div()
                                    .id("new-tag-input")
                                    .h_flex()
                                    .items_center()
                                    .gap_1()
                                    .w(px(230.0))
                                    .on_click(|_, _, cx| cx.stop_propagation())
                                    .on_action(cx.listener(|this, _: &Enter, window, cx| {
                                        this.confirm_new_tag(window, cx);
                                    }))
                                    .on_action(cx.listener(|this, _: &Escape, _window, cx| {
                                        this.is_adding = false;
                                        cx.notify();
                                    }))
                                    .child(Input::new(&self.input_state).small().focus_ring(false))
                                    .child(
                                        Button::new("btn-confirm-tag")
                                            .icon(IconName::Check)
                                            .compact()
                                            .small()
                                            .on_click(cx.listener(|this, _ev, window, cx| {
                                                cx.stop_propagation();

                                                this.confirm_new_tag(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("btn-cancel-tag")
                                            .icon(IconName::Close)
                                            .compact()
                                            .small()
                                            .on_click(cx.listener(|this, _ev, _w, cx| {
                                                cx.stop_propagation();

                                                this.is_adding = false;
                                                cx.notify();
                                            })),
                                    ),
                            )
                        },
                        |t| {
                            t.child(
                                Button::new("btn-add-tag")
                                    .icon(IconName::Plus)
                                    .compact()
                                    .ghost()
                                    .rounded_full()
                                    .on_click(cx.listener(|this, _ev, window, cx| {
                                        cx.stop_propagation();

                                        this.is_adding = true;
                                        let focus = this.input_state.read(cx).focus_handle(cx);
                                        focus.focus(window, cx);
                                        cx.notify();
                                    })),
                            )
                        },
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_1p5()
                    .pr_6()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_color(theme.accent)
                            .text_xs()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child(format!("已选 {} 项", self.selected_count)),
                    )
                    .when(has_changes, |this| {
                        this.child(
                            Button::new("btn-editor-save")
                                .icon(IconName::Check)
                                .compact()
                                .primary()
                                .on_click(cx.listener(|this, _ev, window, cx| {
                                    this.commit(window, cx);
                                })),
                        )
                    }),
            )
    }
}
