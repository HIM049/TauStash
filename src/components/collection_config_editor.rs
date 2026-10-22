use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::AppState;
use crate::components::tag_chip::{TagChip, TagChipState};
use crate::database::entities::{FileState, TagNamespace};
use crate::services::collection::{CollectionConfig, ViewOption};
use crate::services::macrotag::MacroTag;
use crate::services::store::TagStore;

fn preset_macro_tags() -> Vec<MacroTag> {
    vec![
        MacroTag::State(FileState::Pending),
        MacroTag::State(FileState::Managed),
        MacroTag::State(FileState::Ignored),
        MacroTag::State(FileState::Deleted),
        MacroTag::Untagged,
    ]
}

#[derive(Clone, Debug)]
pub enum ConfigEditorEvent {
    DraftChanged,
    CancelRequested,
    SaveRequested,
}

pub struct CollectionConfigEditor {
    name_input: Entity<InputState>,
    tag_search_input: Entity<InputState>,
    draft_config: CollectionConfig,
    tag_search_results: Vec<MacroTag>,
    tag_store: Option<Entity<TagStore>>,
}

impl EventEmitter<ConfigEditorEvent> for CollectionConfigEditor {}

impl CollectionConfigEditor {
    pub fn new(
        initial_name: impl Into<String>,
        initial_config: CollectionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial_name = initial_name.into();
        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial_name, window, cx);
            state
        });

        let tag_search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("搜索全库标签添加至基准规则..."));

        cx.subscribe(
            &tag_search_input,
            move |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = input.read(cx).value().trim().to_string();
                    this.load_search_tags(&text, cx);
                }
            },
        )
        .detach();

        let mut editor = Self {
            name_input,
            tag_search_input,
            draft_config: initial_config,
            tag_search_results: Vec::new(),
            tag_store: None,
        };

        editor.load_search_tags("", cx);
        editor
    }

    pub fn on_search_action(
        this: &mut CollectionConfigEditor,
        _: &Enter,
        window: &mut Window,
        cx: &mut Context<CollectionConfigEditor>,
    ) {
        cx.stop_propagation();
        let q = this.tag_search_input.read(cx).value().trim().to_string();
        if q.is_empty() {
            return;
        }
        // match macro tag
        let matching_macro = preset_macro_tags().into_iter().find(|t| {
            !this.draft_config.filters.contains(t)
                && t.display_name().to_lowercase().contains(&q.to_lowercase())
        });
        if let Some(macro_tag) = matching_macro {
            this.add_filter(macro_tag, cx);
            this.tag_search_input.update(cx, |inp, cx| {
                inp.set_value("", window, cx);
            });
            this.load_search_tags("", cx);
            return;
        }

        // match standard tag
        let matching_standard = this
            .tag_search_results
            .iter()
            .find(|t| {
                !this.draft_config.filters.contains(t)
                    && t.display_name().to_lowercase().contains(&q.to_lowercase())
            })
            .cloned();
        if let Some(tag) = matching_standard {
            this.add_filter(tag, cx);
            this.tag_search_input.update(cx, |inp, cx| {
                inp.set_value("", window, cx);
            });
            this.load_search_tags("", cx);
        } else {
            this.create_and_add_tag(&q, window, cx);
        }
    }

    pub fn current_name(&self, cx: &App) -> String {
        self.name_input.read(cx).value().trim().to_string()
    }

    pub fn current_config(&self) -> &CollectionConfig {
        &self.draft_config
    }

    pub fn tag_store(mut self, tag_store: Entity<TagStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&tag_store, |this, store, cx| {
            let query = this.tag_search_input.read(cx).value().trim().to_string();
            this.tag_search_results = store.read(cx).search(&query);
            cx.notify();
        })
        .detach();
        self.tag_search_results = tag_store.read(cx).search("");
        self.tag_store = Some(tag_store);
        self
    }

    pub fn load_search_tags(&mut self, query: &str, cx: &mut Context<Self>) {
        if let Some(tag_store) = &self.tag_store {
            self.tag_search_results = tag_store.read(cx).search(query);
            cx.notify();
            return;
        }

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let query_str = query.to_string();

        cx.spawn(async move |this, cx| {
            let tags_res = cx
                .background_spawn(async move {
                    rt.block_on(crate::database::storage::tag::search_tags(
                        &db, &query_str, 24,
                    ))
                })
                .await;

            this.update(cx, |this, cx| {
                if let Ok(tags) = tags_res {
                    this.tag_search_results = tags
                        .into_iter()
                        .map(|t| MacroTag::Standard(t.into()))
                        .collect();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn add_filter(&mut self, tag: MacroTag, cx: &mut Context<Self>) {
        if tag.is_exclusive() {
            self.draft_config.filters.retain(|t| !t.is_exclusive());
        }
        if !self.draft_config.filters.iter().any(|t| t == &tag) {
            self.draft_config.filters.push(tag);
            cx.emit(ConfigEditorEvent::DraftChanged);
            cx.notify();
        }
    }

    pub fn remove_filter(&mut self, tag: &MacroTag, cx: &mut Context<Self>) {
        self.draft_config.filters.retain(|t| t != tag);
        cx.emit(ConfigEditorEvent::DraftChanged);
        cx.notify();
    }

    pub fn create_and_add_tag(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }

        self.tag_search_input.update(cx, |inp, cx| {
            inp.set_value("", window, cx);
        });

        if let Some(tag_store) = self.tag_store.clone() {
            let weak_this = cx.entity().downgrade();
            tag_store.update(cx, |store, cx| {
                store.find_or_create_then(
                    TagNamespace::User,
                    &name,
                    move |tag, cx| {
                        if let Some(this) = weak_this.upgrade() {
                            this.update(cx, |this, cx| {
                                this.add_filter(tag, cx);
                            });
                        }
                    },
                    cx,
                );
            });
            return;
        }

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn(async move |this, cx| {
            let tag_res = cx
                .background_spawn(async move {
                    rt.block_on(crate::database::storage::tag::find_or_create(
                        &db,
                        TagNamespace::User,
                        name,
                    ))
                })
                .await;

            this.update(cx, |this, cx| {
                if let Ok(model) = tag_res {
                    let tag = MacroTag::Standard(model.into());
                    this.add_filter(tag, cx);
                    this.load_search_tags("", cx);
                }
            })
            .ok();
        })
        .detach();
    }
}

impl Render for CollectionConfigEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let search_query = self.tag_search_input.read(cx).value().trim().to_string();

        let is_query_in_results = self
            .tag_search_results
            .iter()
            .any(|t| t.display_name().eq_ignore_ascii_case(&search_query));
        let is_query_in_macro = preset_macro_tags()
            .iter()
            .any(|t| t.display_name().eq_ignore_ascii_case(&search_query));
        let is_query_in_filters = self
            .draft_config
            .filters
            .iter()
            .any(|t| t.display_name().eq_ignore_ascii_case(&search_query));
        let can_create_new = !search_query.is_empty()
            && !is_query_in_results
            && !is_query_in_macro
            && !is_query_in_filters;

        let btn_variant = ButtonCustomVariant::new(cx)
            .color(cx.theme().transparent)
            .foreground(cx.theme().muted_foreground)
            .hover(cx.theme().muted.darken(0.05))
            .active(cx.theme().muted.darken(0.10));

        // SELECTED tags
        let selected_chips: Vec<_> = self
            .draft_config
            .filters
            .iter()
            .enumerate()
            .map(|(idx, tag)| {
                let tag_clone = tag.clone();
                TagChip::new(("selected-chip", idx), tag.display_name())
                    .small()
                    .state(TagChipState::Active)
                    .trailing_icon(
                        Icon::new(IconName::Close)
                            .size_3()
                            .text_color(theme.primary_foreground.opacity(0.85)),
                    )
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        this.remove_filter(&tag_clone, cx);
                    }))
            })
            .collect();

        // UNSELECTED tags
        let unselected_standard_tags: Vec<MacroTag> = self
            .tag_search_results
            .iter()
            .filter(|t| !self.draft_config.filters.contains(t))
            .filter(|t| {
                if search_query.is_empty() {
                    true
                } else {
                    t.display_name()
                        .to_lowercase()
                        .contains(&search_query.to_lowercase())
                }
            })
            .cloned()
            .collect();

        // UNSELECTED macro tags
        let unselected_macro_tags: Vec<MacroTag> = preset_macro_tags()
            .into_iter()
            .filter(|t| !self.draft_config.filters.contains(t))
            .filter(|t| {
                if search_query.is_empty() {
                    true
                } else {
                    t.display_name()
                        .to_lowercase()
                        .contains(&search_query.to_lowercase())
                }
            })
            .collect();

        let unselected_macro_chips: Vec<_> = unselected_macro_tags
            .iter()
            .enumerate()
            .map(|(idx, tag)| {
                let tag_clone = tag.clone();
                TagChip::new(("unselected-macro", idx), tag.display_name())
                    .small()
                    .state(TagChipState::Default)
                    .icon(
                        Icon::new(IconName::Plus)
                            .size_3()
                            .text_color(theme.muted_foreground),
                    )
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        this.add_filter(tag_clone.clone(), cx);
                    }))
            })
            .collect();

        let unselected_standard_chips: Vec<_> = unselected_standard_tags
            .iter()
            .enumerate()
            .map(|(idx, tag)| {
                let tag_clone = tag.clone();
                TagChip::new(("unselected-standard", idx), tag.display_name())
                    .small()
                    .state(TagChipState::Default)
                    .icon(
                        Icon::new(IconName::Plus)
                            .size_3()
                            .text_color(theme.muted_foreground),
                    )
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        this.add_filter(tag_clone.clone(), cx);
                    }))
            })
            .collect();

        let has_selected = !selected_chips.is_empty();
        let has_macro = !unselected_macro_chips.is_empty();
        let has_standard = !unselected_standard_chips.is_empty();

        div()
            .id("collection-config-editor")
            .v_flex()
            .gap_2()
            .py_3()
            .on_action(cx.listener(|_this, _: &Escape, _window, cx| {
                cx.stop_propagation();
                cx.emit(ConfigEditorEvent::CancelRequested);
            }))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .px_3()
                    .gap_2()
                    .child(
                        Button::new("scope-back")
                            .custom(btn_variant)
                            .icon(IconName::Close)
                            .on_click(cx.listener(|_this, _, _window, cx| {
                                cx.emit(ConfigEditorEvent::CancelRequested);
                            })),
                    )
                    .child(div().text_xl().font_bold().child("视图属性编辑")),
            )
            .child(
                div()
                    .px_6()
                    .v_flex()
                    .gap_2()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_sm()
                                    .font_medium()
                                    .text_color(theme.foreground)
                                    .child("名称:"),
                            )
                            .child(
                                div()
                                    .w(px(240.0))
                                    .child(Input::new(&self.name_input).small()),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_medium()
                                    .text_color(theme.foreground)
                                    .child("默认视图:"),
                            )
                            .child(
                                Button::new("cfg-view-grid")
                                    .icon(IconName::LayoutGrid)
                                    .small()
                                    .when(self.draft_config.view == ViewOption::Grid, |b| {
                                        b.secondary()
                                    })
                                    .when(self.draft_config.view != ViewOption::Grid, |b| b.ghost())
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.draft_config.view = ViewOption::Grid;
                                        cx.emit(ConfigEditorEvent::DraftChanged);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("cfg-view-list")
                                    .icon(IconName::List)
                                    .small()
                                    .when(self.draft_config.view == ViewOption::List, |b| {
                                        b.secondary()
                                    })
                                    .when(self.draft_config.view != ViewOption::List, |b| b.ghost())
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.draft_config.view = ViewOption::List;
                                        cx.emit(ConfigEditorEvent::DraftChanged);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .text_color(theme.foreground)
                                            .child("视图过滤规则:"),
                                    )
                                    .child(
                                        div()
                                            .w(px(280.0))
                                            .on_action(cx.listener(Self::on_search_action))
                                            .child(
                                                Input::new(&self.tag_search_input)
                                                    .small()
                                                    .cleanable(true),
                                            ),
                                    ),
                            )
                            .when(can_create_new, |this| {
                                let q = search_query.clone();
                                this.child(
                                    Button::new("create-new-tag")
                                        .ghost()
                                        .small()
                                        .icon(IconName::Plus)
                                        .label(format!("新建并添加 \"{}\"", q))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.create_and_add_tag(&q, window, cx);
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id("tag-cloud-container")
                            .h_flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .w_full()
                            .min_h(px(28.0))
                            .max_h(px(200.0))
                            .overflow_y_scroll()
                            .children(selected_chips)
                            .child(div().w(px(1.)).h_5().mx_0p5().bg(theme.border.darken(0.2)))
                            .children(unselected_macro_chips)
                            .child(div().w(px(1.)).h_5().mx_0p5().bg(theme.border.darken(0.2)))
                            .children(unselected_standard_chips),
                    ),
            )
    }
}
