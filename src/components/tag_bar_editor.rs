use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Enter, Escape, Input, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use uuid::Uuid;

use crate::components::tag_chip::{TagChip, TagChipState};
use crate::services::file::FileInfo;
use crate::services::macrotag::MacroTag;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditState {
    All,
    Partial,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedBatchItem {
    pub tag: MacroTag,
    pub original: EditState,
    pub target: EditState,
    pub is_pinned: bool,
}

pub type CommitListener = Rc<dyn Fn(&[MacroTag], &[MacroTag], &mut Window, &mut App) + 'static>;
pub type CancelListener = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

pub struct TagBatchEditor {
    focus_handle: FocusHandle,
    selected_count: usize,
    tracked_items: Vec<TrackedBatchItem>,

    is_adding: bool,
    padding_x: bool,
    input_state: Entity<InputState>,

    on_commit: Option<CommitListener>,
    on_cancel: Option<CancelListener>,
}

impl TagBatchEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| InputState::new(window, cx).placeholder("输入标签名称"));

        Self {
            focus_handle: cx.focus_handle(),
            selected_count: 0,
            tracked_items: Vec::new(),
            is_adding: false,
            padding_x: false,
            input_state,
            on_commit: None,
            on_cancel: None,
        }
    }

    pub fn padding_x(mut self) -> Self {
        self.padding_x = true;
        self
    }

    pub fn on_commit(
        mut self,
        handler: impl Fn(&[MacroTag], &[MacroTag], &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_commit = Some(Rc::new(handler));
        self
    }

    pub fn on_cancel(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(handler));
        self
    }

    /// Construct from files directly
    pub fn from_files(
        window: &mut Window,
        selected_files: &[FileInfo],
        candidate_tags: &[MacroTag],
        pinned_tags: &[MacroTag],
        cx: &mut Context<Self>,
    ) -> Self {
        let mut editor = Self::new(window, cx);
        editor.set_files(selected_files, candidate_tags, pinned_tags, cx);
        editor
    }

    /// Set selection from FileInfo slice
    pub fn set_files(
        &mut self,
        selected_files: &[FileInfo],
        candidate_tags: &[MacroTag],
        pinned_tags: &[MacroTag],
        cx: &mut Context<Self>,
    ) {
        let slices: Vec<&[MacroTag]> = selected_files.iter().map(|f| f.tags.as_slice()).collect();
        self.set_tags(&slices, candidate_tags, pinned_tags, cx);
    }

    /// Update batch selection using raw tag slices
    pub fn set_tags(
        &mut self,
        items_tags: &[&[MacroTag]],
        candidate_tags: &[MacroTag],
        pinned_tags: &[MacroTag],
        cx: &mut Context<Self>,
    ) {
        let total = items_tags.len();
        self.selected_count = total;
        let mut tracked_items: Vec<TrackedBatchItem> = Vec::new();

        // Process pinned tags
        for pinned in pinned_tags {
            let count = items_tags
                .iter()
                .filter(|tags| tags.contains(pinned))
                .count();
            let original = if count == total && total > 0 {
                EditState::All
            } else if count > 0 {
                EditState::Partial
            } else {
                EditState::None
            };
            tracked_items.push(TrackedBatchItem {
                tag: pinned.clone(),
                original,
                target: original,
                is_pinned: true,
            });
        }

        // Count existing tags across selected items
        let mut tag_counts: HashMap<MacroTag, usize> = HashMap::new();
        for tags in items_tags {
            for tag in *tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        let mut sorted_tags: Vec<(MacroTag, usize)> = tag_counts.into_iter().collect();
        sorted_tags.sort_by(|(tag_a, count_a), (tag_b, count_b)| {
            count_b
                .cmp(count_a)
                .then_with(|| tag_a.display_name().cmp(&tag_b.display_name()))
        });

        for (tag, count) in sorted_tags {
            if !tracked_items.iter().any(|t| t.tag == tag) {
                let (original, target) = if count == total && total > 0 {
                    (EditState::All, EditState::All)
                } else {
                    (EditState::Partial, EditState::Partial)
                };
                tracked_items.push(TrackedBatchItem {
                    tag,
                    original,
                    target,
                    is_pinned: false,
                });
            }
        }

        // Add candidate tags
        for candidate in candidate_tags {
            if !tracked_items.iter().any(|t| t.tag == *candidate) {
                tracked_items.push(TrackedBatchItem {
                    tag: candidate.clone(),
                    original: EditState::None,
                    target: EditState::None,
                    is_pinned: false,
                });
            }
        }

        self.tracked_items = tracked_items;
        self.is_adding = false;
        cx.notify();
    }

    pub fn set_selection(
        &mut self,
        selected_files: &[FileInfo],
        candidate_tags: &[MacroTag],
        pinned_tags: &[MacroTag],
        cx: &mut Context<Self>,
    ) {
        self.set_files(selected_files, candidate_tags, pinned_tags, cx);
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle.focus(window, cx);
    }

    pub fn compute_changes(&self) -> (Vec<MacroTag>, Vec<MacroTag>) {
        let mut ensure = Vec::new();
        let mut remove = Vec::new();

        for item in &self.tracked_items {
            if item.tag.is_exclusive() {
                if item.target == EditState::All && item.original != EditState::All {
                    ensure.push(item.tag.clone());
                }
            } else {
                match (item.original, item.target) {
                    (EditState::All, EditState::None) => {
                        remove.push(item.tag.clone());
                    }
                    (EditState::Partial, EditState::All) => {
                        ensure.push(item.tag.clone());
                    }
                    (EditState::Partial, EditState::None) => {
                        remove.push(item.tag.clone());
                    }
                    (EditState::None, EditState::All) => {
                        ensure.push(item.tag.clone());
                    }
                    _ => {}
                }
            }
        }

        (ensure, remove)
    }

    pub fn has_changes(&self) -> bool {
        let (ensure, remove) = self.compute_changes();
        !ensure.is_empty() || !remove.is_empty()
    }

    fn toggle_tag(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(item) = self.tracked_items.get_mut(index) {
            if item.tag.is_exclusive() {
                if item.target == EditState::All {
                    item.target = EditState::None;
                } else {
                    item.target = EditState::All;
                    // Mutually exclusive: clear other exclusive targets
                    for (i, other) in self.tracked_items.iter_mut().enumerate() {
                        if i != index && other.tag.is_exclusive() {
                            other.target = EditState::None;
                        }
                    }
                }
            } else {
                item.target = match item.original {
                    EditState::All => match item.target {
                        EditState::All => EditState::None,
                        _ => EditState::All,
                    },
                    EditState::Partial => match item.target {
                        EditState::Partial => EditState::All,
                        EditState::All => EditState::None,
                        EditState::None => EditState::Partial,
                    },
                    EditState::None => match item.target {
                        EditState::All => EditState::None,
                        _ => EditState::All,
                    },
                };
            }
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
                .tracked_items
                .iter_mut()
                .find(|t| t.tag.display_name() == name)
            {
                item.target = EditState::All;
            } else {
                self.tracked_items.push(TrackedBatchItem {
                    tag: MacroTag::standard(Uuid::now_v7(), name),
                    original: EditState::None,
                    target: EditState::All,
                    is_pinned: false,
                });
            }
        }
        self.input_state.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.is_adding = false;
        cx.notify();
    }

    fn commit(&self, window: &mut Window, cx: &mut App) {
        if let Some(ref cb) = self.on_commit {
            let (ensure, remove) = self.compute_changes();
            cb(&ensure, &remove, window, cx);
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
        let (ensure, remove) = self.compute_changes();
        let has_changes = !ensure.is_empty() || !remove.is_empty();

        let mut chips = Vec::with_capacity(self.tracked_items.len() + 2);
        let mut prev_was_pinned = false;

        for (idx, item) in self.tracked_items.iter().enumerate() {
            if prev_was_pinned && !item.is_pinned {
                chips.push(
                    div()
                        .h(px(24.0))
                        .w(px(1.0))
                        .bg(theme.border)
                        .mx_1()
                        .flex_shrink_0()
                        .into_any_element(),
                );
            }
            prev_was_pinned = item.is_pinned;

            let chip_state = match item.target {
                EditState::All => TagChipState::Selected,
                EditState::Partial => TagChipState::PartialSelected,
                EditState::None => TagChipState::NoneSelected,
            };

            let chip = TagChip::new(("batch-item", idx), item.tag.display_name())
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
            .pl_6()
            .py_1()
            .when(self.padding_x, |this| this.px_6())
            .gap_2()
            .bg(theme.accent.opacity(0.08))
            .border_color(theme.border)
            .min_h(px(34.0))
            .overflow_x_scroll()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .flex_1()
                    .children(chips)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .when(self.is_adding, |this| {
                                this.w(px(140.0))
                                    .on_action(cx.listener(|this, _: &Enter, window, cx| {
                                        this.confirm_new_tag(window, cx);
                                    }))
                                    .on_action(cx.listener(|this, _: &Escape, _window, cx| {
                                        this.is_adding = false;
                                        cx.notify();
                                    }))
                                    .child(Input::new(&self.input_state).small())
                            })
                            .when(!self.is_adding, |this| {
                                this.child(
                                    Button::new("add-tag-btn")
                                        .icon(IconName::Plus)
                                        .ghost()
                                        .rounded_full()
                                        .on_click(cx.listener(|this, _ev, window, cx| {
                                            cx.stop_propagation();
                                            this.is_adding = true;
                                            this.input_state.update(cx, |input, cx| {
                                                input.focus(window, cx);
                                            });
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    // .child(
                    //     Button::new("cancel-batch-btn")
                    //         .label("取消")
                    //         .ghost()
                    //         .on_click(cx.listener(|this, _ev, window, cx| {
                    //             this.cancel(window, cx);
                    //         })),
                    // )
                    .when(has_changes, |this| {
                        this.child(
                            Button::new("apply-batch-btn")
                                .primary()
                                .icon(IconName::Check)
                                .on_click(cx.listener(|this, _ev, window, cx| {
                                    this.commit(window, cx);
                                })),
                        )
                    }),
            )
    }
}
