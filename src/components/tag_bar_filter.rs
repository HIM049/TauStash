use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use uuid::Uuid;

use crate::components::tag_chip::{TagChip, TagChipState};
use crate::database::storage::query::{FileQueryer, TagFacetItem};
use crate::services::MacroTag;

#[derive(Clone, Debug)]
pub enum TagFilterEvent {
    All,
    Untagged,
    Tag(MacroTag),
}

pub struct TagFilterBar {
    base_query: FileQueryer,
    excluded_tags: HashSet<MacroTag>,
    base_summary: Vec<TagFacetItem>,
    tags_summary: Vec<TagFacetItem>,
    total_files_count: usize,
    untagged_files_count: usize,
    loading: bool,
    padding_x: bool,

    active_tags: HashSet<MacroTag>,
}

impl TagFilterBar {
    pub fn new(base_query: FileQueryer) -> Self {
        Self::with_excluded_tags(base_query, Vec::new())
    }

    pub fn padding_x(mut self) -> Self {
        self.padding_x = true;
        self
    }

    pub fn with_excluded_tags(
        base_query: FileQueryer,
        excluded_tags: impl IntoIterator<Item = MacroTag>,
    ) -> Self {
        Self {
            base_query,
            excluded_tags: excluded_tags.into_iter().collect(),
            base_summary: Vec::new(),
            tags_summary: Vec::new(),
            total_files_count: 0,
            untagged_files_count: 0,
            loading: false,
            padding_x: false,
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
        self.base_summary = tags_summary.clone();
        self.tags_summary = tags_summary;
        self.untagged_files_count = untagged_files_count;
        self.total_files_count = total_files_count;
    }

    pub fn update_intersect_stats(&mut self, intersect_summary: Vec<TagFacetItem>) {
        let counts: HashMap<Uuid, usize> = intersect_summary
            .into_iter()
            .map(|item| (item.tag_id, item.count))
            .collect();

        let mut merged = self.base_summary.clone();
        for item in &mut merged {
            item.count = counts.get(&item.tag_id).copied().unwrap_or(0);
        }

        merged.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        self.tags_summary = merged;
    }

    pub fn reset_to_base(&mut self) {
        self.tags_summary = self.base_summary.clone();
    }

    pub fn clear_stats(&mut self) {
        self.base_summary.clear();
        self.tags_summary.clear();
        self.untagged_files_count = 0;
        self.total_files_count = 0;
        self.loading = false;
    }

    pub fn current_tags(&self) -> Vec<MacroTag> {
        self.active_tags.iter().cloned().collect()
    }

    pub fn set_base_query(&mut self, base_query: FileQueryer) {
        self.base_query = base_query;
    }

    pub fn set_excluded_tags(&mut self, tags: impl IntoIterator<Item = MacroTag>) {
        self.excluded_tags = tags.into_iter().collect();
    }

    pub fn tags_summary(&self) -> &[TagFacetItem] {
        &self.tags_summary
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

    pub fn toggle_tag(&mut self, tag: &MacroTag) {
        self.active_tags.remove(&MacroTag::Untagged);

        if self.active_tags.contains(tag) {
            self.active_tags.remove(tag);
        } else {
            self.active_tags.insert(tag.clone());
        }
    }

    /// Renders the filter bar with a decoupled toggle callback.
    pub fn render(
        &self,
        on_toggle: impl Fn(TagFilterEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let is_all = self.active_tags.is_empty();
        let is_untagged = self.active_tags.contains(&MacroTag::Untagged);

        let on_toggle = Rc::new(on_toggle);
        let mut chips = Vec::with_capacity(self.tags_summary.len() + 2);

        let chip = TagChip::new("tag-filter-all", "全部")
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
        chips.push(chip.into_any_element());

        if !self.excluded_tags.contains(&MacroTag::Untagged) {
            let chip = TagChip::new("tag-filter-untagged", "未标记")
                .count(self.untagged_files_count)
                .state(if is_untagged {
                    TagChipState::Active
                } else {
                    TagChipState::Default
                })
                .on_click({
                    let on_toggle = on_toggle.clone();
                    move |_, window, cx| {
                        cx.stop_propagation();
                        on_toggle(TagFilterEvent::Untagged, window, cx);
                    }
                });
            chips.push(chip.into_any_element());
        }

        for item in &self.tags_summary {
            let tag_id = item.tag_id;
            let tag_name = item.name.clone();
            let macro_tag = MacroTag::standard(tag_id, &tag_name);
            if self.excluded_tags.contains(&MacroTag::standard_id(tag_id)) {
                continue;
            }

            let is_active = self.active_tags.contains(&MacroTag::standard_id(tag_id));

            let chip = TagChip::new(format!("tag-filter-{}", tag_id), item.name.clone())
                .count(item.count)
                .state(if is_active {
                    TagChipState::Active
                } else {
                    TagChipState::Default
                })
                .on_click({
                    let on_toggle = on_toggle.clone();
                    let macro_tag = macro_tag.clone();
                    move |_, window, cx| {
                        cx.stop_propagation();
                        on_toggle(TagFilterEvent::Tag(macro_tag.clone()), window, cx);
                    }
                });

            chips.push(chip.into_any_element());
        }

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
            .when(self.padding_x, |this| this.px_6())
            .children(chips)
            .into_any_element()
    }
}
