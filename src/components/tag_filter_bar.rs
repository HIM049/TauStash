use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::components::tag_chip::{TagChip, TagChipState};
use crate::database::storage::query::{FileQueryer, TagFacetItem};
use crate::services::tags_queryer::MacroTag;

pub struct TagFilterBar {
    base_query: FileQueryer,
    tags_summary: Vec<TagFacetItem>,
    total_files_count: usize,
    untagged_files_count: usize,
    loading: bool,

    active_tags: HashSet<MacroTag>,
    on_change: Option<Rc<dyn Fn(&[MacroTag], &mut Window, &mut App) + 'static>>,
}

impl TagFilterBar {
    pub fn new(base_query: FileQueryer, cx: &mut Context<Self>) -> Self {
        let mut bar = Self {
            base_query,
            tags_summary: Vec::new(),
            total_files_count: 0,
            untagged_files_count: 0,
            loading: false,
            active_tags: HashSet::new(),
            on_change: None,
        };
        bar.reload_stats(cx);
        bar
    }

    pub fn on_change(&mut self, handler: impl Fn(&[MacroTag], &mut Window, &mut App) + 'static) {
        self.on_change = Some(Rc::new(handler));
    }

    pub fn current_tags(&self) -> Vec<MacroTag> {
        self.active_tags.iter().cloned().collect()
    }

    pub fn set_base_query(&mut self, base_query: FileQueryer, cx: &mut Context<Self>) {
        self.base_query = base_query;
        self.reload_stats(cx);
    }

    pub fn reload_stats(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.loading = true;

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let query = self.base_query.clone();

        cx.spawn(async move |this, cx| {
            let stats_res = cx
                .background_spawn(async move { rt.block_on(query.query_stats(&db)) })
                .await;

            this.update(cx, |this, cx| {
                this.loading = false;
                if let Ok(stats) = stats_res {
                    this.tags_summary = stats.tags_summary;
                    this.untagged_files_count = stats.untagged_files;
                    this.total_files_count = stats.total_files;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_tags.clear();
        cx.notify();
        self.notify_change(window, cx);
    }

    fn toggle_untagged(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_tags.contains(&MacroTag::Untagged) {
            self.active_tags.remove(&MacroTag::Untagged);
        } else {
            self.active_tags.clear();
            self.active_tags.insert(MacroTag::Untagged);
        }

        cx.notify();
        self.notify_change(window, cx);
    }

    fn toggle_tag(&mut self, tag_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        self.active_tags.remove(&MacroTag::Untagged);

        let tag = MacroTag::StandardTag(tag_id);
        if self.active_tags.contains(&tag) {
            self.active_tags.remove(&tag);
        } else {
            self.active_tags.insert(tag);
        }

        cx.notify();
        self.notify_change(window, cx);
    }

    fn notify_change(&self, window: &mut Window, cx: &mut App) {
        if let Some(ref cb) = self.on_change {
            let cb = cb.clone();
            let tags = self.current_tags();
            cb(&tags, window, cx);
        }
    }
}

impl Render for TagFilterBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_all = self.active_tags.is_empty();
        let is_untagged = self.active_tags.contains(&MacroTag::Untagged);

        let mut chips = Vec::with_capacity(self.tags_summary.len() + 2);

        let chip = TagChip::new("tag-filter-all", "全部")
            .count(self.total_files_count)
            .state(if is_all {
                TagChipState::Active
            } else {
                TagChipState::Default
            })
            .on_click(cx.listener(|this, _ev, window, cx| {
                this.select_all(window, cx);
            }));
        chips.push(chip.into_any_element());

        let chip = TagChip::new("tag-filter-untagged", "未标记")
            .count(self.untagged_files_count)
            .state(if is_untagged {
                TagChipState::Active
            } else {
                TagChipState::Default
            })
            .on_click(cx.listener(|this, _ev, window, cx| {
                this.toggle_untagged(window, cx);
            }));
        chips.push(chip.into_any_element());

        for item in &self.tags_summary {
            let tag_id = item.tag_id;
            let is_active = self.active_tags.contains(&MacroTag::StandardTag(tag_id));

            let chip = TagChip::new(format!("tag-filter-{}", tag_id), item.name.clone())
                .count(item.count)
                .state(if is_active {
                    TagChipState::Active
                } else {
                    TagChipState::Default
                })
                .on_click(cx.listener(move |this, _ev, window, cx| {
                    this.toggle_tag(tag_id, window, cx);
                }));

            chips.push(chip.into_any_element());
        }

        div()
            .h_flex()
            .items_center()
            .gap_2()
            .w_full()
            .overflow_x_scrollbar()
            .py_1()
            .children(chips)
    }
}
