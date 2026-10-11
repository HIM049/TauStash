use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::AppState;
use crate::components::tag_chip::{TagChip, TagChipState};
use crate::database::entities::TagNamespace;
use crate::services::collection::{CollectionConfig, ViewOption};
use crate::services::macrotag::MacroTag;
use crate::stores::TagStore;

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
        let has_filters = !self.draft_config.filters.is_empty();
        let search_query = self.tag_search_input.read(cx).value().trim().to_string();

        let is_query_in_results = self
            .tag_search_results
            .iter()
            .any(|t| t.display_name().eq_ignore_ascii_case(&search_query));

        let can_create_new = !search_query.is_empty() && !is_query_in_results;

        div()
            .id("collection-config-editor")
            .v_flex()
            .gap_4()
            .px_6()
            .py_3()
            // .bg(theme.background.darken(0.02))
            .on_action(cx.listener(|_this, _: &Escape, _window, cx| {
                cx.stop_propagation();
                cx.emit(ConfigEditorEvent::CancelRequested);
            }))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_xl().font_bold().child("视图属性编辑")),
                // .child(
                //     div()
                //         .h_flex()
                //         .items_center()
                //         .gap_2()
                //         .child(
                //             Button::new("cfg-editor-cancel")
                //                 .ghost()
                //                 .icon(IconName::Close)
                //                 .on_click(cx.listener(|_this, _, _window, cx| {
                //                     cx.emit(ConfigEditorEvent::CancelRequested);
                //                 })),
                //         )
                //         .child(
                //             Button::new("cfg-editor-save")
                //                 .primary()
                //                 .icon(IconName::Check)
                //                 .on_click(cx.listener(|_this, _, _window, cx| {
                //                     cx.emit(ConfigEditorEvent::SaveRequested);
                //                 })),
                //         ),
                // ),
            )
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
                            // .on_action(cx.listener(|this, _: &Enter, window, cx| {
                            //     cx.stop_propagation();
                            //     // this.save(window, cx);
                            // }))
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
                    .v_flex()
                    .gap_2()
                    .child(
                        div().h_flex().items_center().gap_2().child(
                            div()
                                .text_sm()
                                .font_medium()
                                .text_color(theme.foreground)
                                .child("基准过滤规则:"),
                        ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .min_h(px(28.0))
                            .when(!has_filters, |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("暂无规则"),
                                )
                            })
                            .children(self.draft_config.filters.iter().enumerate().map(
                                |(idx, tag)| {
                                    let tag_clone = tag.clone();
                                    TagChip::new(("base-filter", idx), tag.display_name())
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
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .gap_2p5()
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
                                    .child("添加标签至规则:"),
                            )
                            .child(
                                div()
                                    .w(px(280.0))
                                    .on_action(cx.listener(|this, _: &Enter, window, cx| {
                                        cx.stop_propagation();
                                        let q = this
                                            .tag_search_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        if !q.is_empty() {
                                            if let Some(first_tag) =
                                                this.tag_search_results.first().cloned()
                                            {
                                                this.add_filter(first_tag, cx);
                                                this.tag_search_input.update(cx, |inp, cx| {
                                                    inp.set_value("", window, cx);
                                                });
                                                this.load_search_tags("", cx);
                                            } else {
                                                this.create_and_add_tag(&q, window, cx);
                                            }
                                        }
                                    }))
                                    .child(
                                        Input::new(&self.tag_search_input).small().cleanable(true),
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
                    // 下方可选标签使用 TagChip 样式展示
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .when(self.tag_search_results.is_empty(), |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("未找到相关标签"),
                                )
                            })
                            .children(self.tag_search_results.iter().enumerate().map(
                                |(idx, tag)| {
                                    let is_added =
                                        self.draft_config.filters.iter().any(|t| t == tag);
                                    let tag_clone = tag.clone();

                                    if is_added {
                                        TagChip::new(("candidate-chip", idx), tag.display_name())
                                            .small()
                                            .state(TagChipState::Selected)
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                this.remove_filter(&tag_clone, cx);
                                            }))
                                    } else {
                                        TagChip::new(("candidate-chip", idx), tag.display_name())
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
                                    }
                                },
                            )),
                    ),
            )
    }
}
