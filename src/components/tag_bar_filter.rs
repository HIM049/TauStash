use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::base::ElementExt;
use gpui_kit::component::*;
use gpui_kit::*;
use uuid::Uuid;

use crate::components::motion_slot::MotionSlot;
use crate::components::tag_chip::{TagChip, TagChipState};
use crate::database::storage::query::{FileQueryer, TagFacetItem};
use crate::services::MacroTag;
use rust_i18n::t;

#[derive(Clone, Debug)]
pub enum TagFilterEvent {
    All,
    Untagged,
    Tag { tag: MacroTag, is_multi: bool },
}

#[derive(Clone, Debug)]
enum FilterEntry {
    Untagged { count: usize },
    Tag(TagFacetItem),
}

impl FilterEntry {
    fn count(&self) -> usize {
        match self {
            Self::Untagged { count } => *count,
            Self::Tag(item) => item.count,
        }
    }
}

fn chip_state(is_active: bool, count: usize) -> TagChipState {
    match (is_active, count) {
        (true, _) => TagChipState::Active,
        (false, 0) => TagChipState::Muted,
        (false, _) => TagChipState::Default,
    }
}

pub struct TagFilterBar {
    base_query: FileQueryer,
    excluded_tags: HashSet<MacroTag>,
    base_entries: Vec<FilterEntry>,
    entries: Vec<FilterEntry>,
    total_files_count: usize,
    loading: bool,

    active_tags: HashSet<MacroTag>,
}

impl TagFilterBar {
    pub fn new(base_query: FileQueryer) -> Self {
        Self::with_excluded_tags(base_query, Vec::new())
    }

    pub fn with_excluded_tags(
        base_query: FileQueryer,
        excluded_tags: impl IntoIterator<Item = MacroTag>,
    ) -> Self {
        Self {
            base_query,
            excluded_tags: excluded_tags.into_iter().collect(),
            base_entries: Vec::new(),
            entries: Vec::new(),
            total_files_count: 0,
            loading: false,
            active_tags: HashSet::new(),
        }
    }

    pub fn base_query(&self) -> &FileQueryer {
        &self.base_query
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn set_stats(
        &mut self,
        tags_summary: Vec<TagFacetItem>,
        untagged_files_count: usize,
        total_files_count: usize,
    ) {
        let mut entries: Vec<_> = tags_summary.into_iter().map(FilterEntry::Tag).collect();
        if !self.excluded_tags.contains(&MacroTag::Untagged) {
            let index = entries.partition_point(|entry| entry.count() > untagged_files_count);
            entries.insert(
                index,
                FilterEntry::Untagged {
                    count: untagged_files_count,
                },
            );
        }

        self.base_entries = entries.clone();
        self.entries = entries;
        self.total_files_count = total_files_count;
    }

    pub fn update_intersect_stats(
        &mut self,
        intersect_summary: Vec<TagFacetItem>,
        untagged_files_count: usize,
    ) {
        let counts: HashMap<Uuid, usize> = intersect_summary
            .into_iter()
            .map(|item| (item.tag_id, item.count))
            .collect();

        let mut entries = self.base_entries.clone();
        for entry in &mut entries {
            match entry {
                FilterEntry::Untagged { count } => *count = untagged_files_count,
                FilterEntry::Tag(item) => {
                    item.count = counts.get(&item.tag_id).copied().unwrap_or(0);
                }
            }
        }

        // Stable: valid entries move to the front but keep their base order
        entries.sort_by_key(|entry| entry.count() == 0);
        self.entries = entries;
    }

    pub fn reset_to_base(&mut self) {
        self.entries = self.base_entries.clone();
    }

    pub fn clear_stats(&mut self) {
        self.base_entries.clear();
        self.entries.clear();
        self.total_files_count = 0;
        self.loading = false;
    }

    pub fn current_tags(&self) -> Vec<MacroTag> {
        self.active_tags.iter().cloned().collect()
    }

    pub fn tags_summary(&self) -> impl Iterator<Item = &TagFacetItem> {
        self.entries.iter().filter_map(|entry| match entry {
            FilterEntry::Tag(item) => Some(item),
            FilterEntry::Untagged { .. } => None,
        })
    }

    pub fn select_all(&mut self) {
        self.active_tags.clear();
    }

    pub fn toggle_untagged(&mut self) {
        if self.active_tags.contains(&MacroTag::Untagged) {
            self.active_tags.remove(&MacroTag::Untagged);
        } else {
            self.active_tags.clear();
            self.active_tags.insert(MacroTag::Untagged);
        }
    }

    pub fn toggle_tag(&mut self, tag: &MacroTag, is_multi: bool) {
        self.active_tags.remove(&MacroTag::Untagged);

        if is_multi {
            if self.active_tags.contains(tag) {
                self.active_tags.remove(tag);
            } else {
                self.active_tags.insert(tag.clone());
            }
        } else {
            if self.active_tags.len() == 1 && self.active_tags.contains(tag) {
                self.active_tags.clear();
            } else {
                self.active_tags.clear();
                self.active_tags.insert(tag.clone());
            }
        }
    }

    /// Renders the filter bar with decoupled toggle and pointer callbacks.
    pub fn render(
        &self,
        on_toggle: impl Fn(TagFilterEvent, &mut Window, &mut App) + 'static,
        on_pointer: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let is_all = self.active_tags.is_empty();
        let is_untagged = self.active_tags.contains(&MacroTag::Untagged);

        let on_toggle = Rc::new(on_toggle);
        let on_pointer = Rc::new(on_pointer);
        let viewport_origin = Rc::new(Cell::new(Point::default()));
        let mut chips = Vec::with_capacity(self.entries.len() + 1);

        let chip = TagChip::new("tag-filter-all", t!("tag_filter.all"))
            .count(self.total_files_count)
            .state(if is_all {
                TagChipState::Active
            } else {
                TagChipState::Default
            })
            .on_click({
                let on_toggle = on_toggle.clone();
                move |_, window, cx| {
                    cx.stop_propagation();
                    on_toggle(TagFilterEvent::All, window, cx);
                }
            });
        chips.push(
            MotionSlot::new("tag-slot-all", viewport_origin.clone(), chip).into_any_element(),
        );

        for entry in &self.entries {
            let item = match entry {
                FilterEntry::Untagged { count } => {
                    let chip = TagChip::new("tag-filter-untagged", t!("tag.untagged"))
                        .count(*count)
                        .state(chip_state(is_untagged, *count))
                        .on_click({
                            let on_toggle = on_toggle.clone();
                            move |_, window, cx| {
                                cx.stop_propagation();
                                on_toggle(TagFilterEvent::Untagged, window, cx);
                            }
                        });
                    chips.push(
                        MotionSlot::new("tag-slot-untagged", viewport_origin.clone(), chip)
                            .into_any_element(),
                    );
                    continue;
                }
                FilterEntry::Tag(item) => item,
            };

            let tag_id = item.tag_id;
            let tag_name = item.name.clone();
            let macro_tag = MacroTag::standard(tag_id, &tag_name);
            if self.excluded_tags.contains(&MacroTag::standard_id(tag_id)) {
                continue;
            }

            let is_active = self.active_tags.contains(&MacroTag::standard_id(tag_id));

            let chip = TagChip::new(format!("tag-filter-{}", tag_id), item.name.clone())
                .count(item.count)
                .state(chip_state(is_active, item.count))
                .on_click({
                    let on_toggle = on_toggle.clone();
                    let macro_tag = macro_tag.clone();
                    move |_, window, cx| {
                        cx.stop_propagation();
                        let modifiers = window.modifiers();
                        let is_multi = modifiers.control || modifiers.platform;
                        on_toggle(
                            TagFilterEvent::Tag {
                                tag: macro_tag.clone(),
                                is_multi,
                            },
                            window,
                            cx,
                        );
                    }
                });

            chips.push(
                MotionSlot::new(
                    format!("tag-slot-{}", tag_id),
                    viewport_origin.clone(),
                    chip,
                )
                .into_any_element(),
            );
        }

        // The canvas from on_prepaint must stay before the scroller: it prepaints first,
        // so the slots read this frame's origin, and sits outside the scroll offset.
        div()
            .id("filter-bar-area")
            .relative()
            .w_full()
            .min_w_0()
            .on_hover({
                let on_pointer = on_pointer.clone();
                move |hovered, window, cx| on_pointer(*hovered, window, cx)
            })
            .on_mouse_move(move |_, window, cx| on_pointer(true, window, cx))
            .on_prepaint(move |bounds, _, _| viewport_origin.set(bounds.origin))
            .child(
                div()
                    .id("filter-bar")
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .min_h(px(34.0))
                    .overflow_x_scroll()
                    .pl_6()
                    .py_1()
                    .children(chips),
            )
            .into_any_element()
    }
}
