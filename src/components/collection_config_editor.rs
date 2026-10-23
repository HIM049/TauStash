use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::AppState;
use crate::components::tag_chip::{TagChip, TagChipState};
use crate::database::entities::collection::CollectionType;
use crate::database::entities::{FileState, TagNamespace};
use crate::services::collection::{CollectionConfig, ViewOption};
use crate::services::macrotag::MacroTag;
use crate::services::store::TagStore;

pub const PRESET_ICONS: &[IconName] = &[
    IconName::File,
    IconName::Folder,
    IconName::Trash,
    IconName::Archive,
    IconName::Inbox,
    IconName::Star,
    IconName::Heart,
    IconName::Bookmark,
    IconName::Tag,
    IconName::Sparkles,
    IconName::Clock,
    IconName::Image,
    IconName::Camera,
    IconName::Film,
    IconName::Check,
    IconName::Palette,
];

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
    DeleteRequested,
}

pub struct CollectionConfigEditor {
    name_input: Entity<InputState>,
    tag_search_input: Entity<InputState>,
    current_icon: String,
    draft_config: CollectionConfig,
    collection_type: CollectionType,
    tag_search_results: Vec<MacroTag>,
    tag_store: Option<Entity<TagStore>>,
    confirm_delete: bool,
}

impl EventEmitter<ConfigEditorEvent> for CollectionConfigEditor {}

impl CollectionConfigEditor {
    pub fn new(
        name: impl Into<String>,
        icon: impl Into<String>,
        config: CollectionConfig,
        collection_type: CollectionType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial_name = name.into();

        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial_name, window, cx);
            state
        });

        let tag_search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索标签"));

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
            current_icon: icon.into(),
            draft_config: config,
            collection_type,
            tag_search_results: Vec::new(),
            tag_store: None,
            confirm_delete: false,
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

    pub fn current_icon(&self) -> &str {
        &self.current_icon
    }

    pub fn set_icon(&mut self, icon: impl Into<String>, cx: &mut Context<Self>) {
        self.current_icon = icon.into();
        cx.emit(ConfigEditorEvent::DraftChanged);
        cx.notify();
    }

    pub fn current_config(&self) -> &CollectionConfig {
        &self.draft_config
    }

    pub fn current_type(&self) -> CollectionType {
        self.collection_type
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
                    .gap_3()
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap_x_8()
                            .gap_y_3()
                            .items_center()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .w(px(72.0))
                                            .text_sm()
                                            .font_medium()
                                            .text_color(theme.foreground)
                                            .child("名称:"),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                Popover::new("icon-picker-popover")
                                                    .trigger(
                                                        Button::new("btn-icon-trigger")
                                                            .small()
                                                            .icon(Icon::default().path(self.current_icon.clone()))
                                                    )
                                                    .content({
                                                        let editor = cx.entity().clone();
                                                        let current = self.current_icon.clone();
                                                        move |_state, _window, cx| {
                                                            div()
                                                                .p_2()
                                                                .v_flex()
                                                                .gap_1p5()
                                                                .child(
                                                                    div()
                                                                        .grid()
                                                                        .grid_cols(4)
                                                                        .gap_1()
                                                                        .children(PRESET_ICONS.iter().enumerate().map(
                                                                            |(idx, &icon)| {
                                                                                let path = icon.path();
                                                                                let is_selected = current == path.as_ref();
                                                                                let path_str = path.to_string();
                                                                                let editor = editor.clone();
                                                                                Button::new(("preset-icon", idx))
                                                                                    .icon(icon)
                                                                                    .small()
                                                                                    .when(is_selected, |b| b.secondary())
                                                                                    .when(!is_selected, |b| b.ghost())
                                                                                    .on_click(cx.listener({
                                                                                        let editor = editor.clone();
                                                                                        move |state, _, window, cx| {
                                                                                            editor.update(cx, |this, cx| {
                                                                                                this.set_icon(&path_str, cx);
                                                                                            });
                                                                                            state.dismiss(window, cx);
                                                                                        }
                                                                                    }))
                                                                            },
                                                                        )),
                                                                )
                                                        }
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(Input::new(&self.name_input).small()),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .w(px(96.0))
                                            .text_sm()
                                            .font_medium()
                                            .text_color(theme.foreground)
                                            .child("侧边视图:"),
                                    )
                                    .child(
                                        Switch::new("cfg-collection-type-switch")
                                            .small()
                                            .checked(self.collection_type == CollectionType::View)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _window, cx| {
                                                    this.collection_type = if *checked {
                                                        CollectionType::View
                                                    } else {
                                                        CollectionType::Folder
                                                    };
                                                    cx.emit(ConfigEditorEvent::DraftChanged);
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .w(px(72.0))
                                            .text_sm()
                                            .font_medium()
                                            .text_color(theme.foreground)
                                            .child("视图:"),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Button::new("cfg-view-grid")
                                                    .icon(IconName::LayoutGrid)
                                                    .small()
                                                    .label("网格")
                                                    .when(
                                                        self.draft_config.view == ViewOption::Grid,
                                                        |b| b.secondary(),
                                                    )
                                                    .when(
                                                        self.draft_config.view != ViewOption::Grid,
                                                        |b| b.ghost(),
                                                    )
                                                    .on_click(cx.listener(
                                                        |this, _, _window, cx| {
                                                            this.draft_config.view =
                                                                ViewOption::Grid;
                                                            cx.emit(
                                                                ConfigEditorEvent::DraftChanged,
                                                            );
                                                            cx.notify();
                                                        },
                                                    )),
                                            )
                                            .child(
                                                Button::new("cfg-view-list")
                                                    .icon(IconName::List)
                                                    .small()
                                                    .label("列表")
                                                    .when(
                                                        self.draft_config.view == ViewOption::List,
                                                        |b| b.secondary(),
                                                    )
                                                    .when(
                                                        self.draft_config.view != ViewOption::List,
                                                        |b| b.ghost(),
                                                    )
                                                    .on_click(cx.listener(
                                                        |this, _, _window, cx| {
                                                            this.draft_config.view =
                                                                ViewOption::List;
                                                            cx.emit(
                                                                ConfigEditorEvent::DraftChanged,
                                                            );
                                                            cx.notify();
                                                        },
                                                    )),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .w(px(96.0))
                                            .text_sm()
                                            .font_medium()
                                            .text_color(theme.foreground)
                                            .when_else(
                                                self.confirm_delete,
                                                |this| this.child("确认删除:"),
                                                |this| this.child("操作:"),
                                            ), // .child("操作:"),
                                    )
                                    .child(if !self.confirm_delete {
                                        Button::new("btn-delete-collection")
                                            .small()
                                            .icon(IconName::Trash)
                                            .label("删除合集")
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.confirm_delete = true;
                                                cx.notify();
                                            }))
                                            .into_any_element()
                                    } else {
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                Button::new("btn-confirm-delete")
                                                    .danger()
                                                    .small()
                                                    .icon(IconName::Check)
                                                    .label("确认")
                                                    .on_click(cx.listener(
                                                        |this, _, _window, cx| {
                                                            this.confirm_delete = false;
                                                            cx.emit(
                                                                ConfigEditorEvent::DeleteRequested,
                                                            );
                                                        },
                                                    )),
                                            )
                                            .child(
                                                Button::new("btn-cancel-delete")
                                                    .ghost()
                                                    .small()
                                                    .icon(IconName::Close)
                                                    .on_click(cx.listener(
                                                        |this, _, _window, cx| {
                                                            this.confirm_delete = false;
                                                            cx.notify();
                                                        },
                                                    )),
                                            )
                                            .into_any_element()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(1.))
                            .my_1()
                            .bg(theme.border.opacity(0.4)),
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
                                            .w(px(320.0))
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
                                        .label(&q)
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
                            .min_h(px(32.0))
                            .max_h(px(140.0))
                            .overflow_y_scroll()
                            .p_2()
                            .bg(theme.muted.opacity(0.3))
                            .children(selected_chips)
                            .when(has_selected && (has_macro || has_standard), |this| {
                                this.child(
                                    div().w(px(1.)).h_5().mx_0p5().bg(theme.border.darken(0.2)),
                                )
                            })
                            .children(unselected_macro_chips)
                            .when(has_macro && has_standard, |this| {
                                this.child(
                                    div().w(px(1.)).h_5().mx_0p5().bg(theme.border.darken(0.2)),
                                )
                            })
                            .children(unselected_standard_chips),
                    ),
            )
    }
}
