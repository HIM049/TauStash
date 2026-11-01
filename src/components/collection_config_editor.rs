use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonGroup, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::form::{field, h_form};
use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::AppState;
use crate::components::tag_chip::{TagChip, TagChipState};
use crate::database::entities::collection::CollectionType;
use crate::database::entities::{FileState, TagNamespace};
use crate::database::storage::query::FileSortField;
use crate::services::collection::{
    CONTEXT_MENU_STATES, CollectionConfig, GroupMode, TimeGroup, ViewOption,
};
use crate::services::macrotag::MacroTag;
use crate::services::store::TagStore;

const EXPAND_TRANSITION: Duration = Duration::from_millis(300);

pub const ALL_SORT_FIELDS: &[FileSortField] = &[
    FileSortField::ContentCreatedAt,
    FileSortField::ContentModifiedAt,
    FileSortField::CreatedAt,
    FileSortField::UpdatedAt,
    FileSortField::Size,
];

pub const TIME_GROUP_MODES: &[GroupMode] = &[
    GroupMode::None,
    GroupMode::Batch,
    GroupMode::Time(TimeGroup::Day),
    GroupMode::Time(TimeGroup::Month),
    GroupMode::Time(TimeGroup::Year),
];

pub const SIZE_GROUP_MODES: &[GroupMode] = &[GroupMode::None, GroupMode::Batch, GroupMode::Size];

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

pub fn sort_field_label(sort: FileSortField) -> &'static str {
    match sort {
        FileSortField::ContentCreatedAt => "文件创建时间",
        FileSortField::ContentModifiedAt => "文件修改时间",
        FileSortField::CreatedAt => "入库时间",
        FileSortField::UpdatedAt => "更新时间",
        FileSortField::Size => "文件大小",
    }
}

pub fn group_mode_label(mode: GroupMode) -> &'static str {
    match mode {
        GroupMode::None => "无分组",
        GroupMode::Time(TimeGroup::Day) => "每日",
        GroupMode::Time(TimeGroup::Month) => "每月",
        GroupMode::Time(TimeGroup::Year) => "每年",
        GroupMode::Size => "按大小区间",
        GroupMode::Batch => "导入批次",
    }
}

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
    ExpandToggled,
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
    expanded: bool,
    expand_toggled_at: Option<Instant>,
    extend_section_height: Rc<Cell<Pixels>>,
    scroll_handle: ScrollHandle,
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
            expanded: false,
            expand_toggled_at: None,
            extend_section_height: Rc::new(Cell::new(px(0.))),
            scroll_handle: ScrollHandle::new(),
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

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Whether the current expanded state came from a toggle, rather than the initial open.
    pub fn expand_toggled(&self) -> bool {
        self.expand_toggled_at.is_some()
    }

    fn toggle_expanded(&mut self, cx: &mut Context<Self>) {
        self.expanded = !self.expanded;
        self.expand_toggled_at = Some(Instant::now());
        self.confirm_delete = false;
        cx.emit(ConfigEditorEvent::ExpandToggled);
        cx.notify();
    }

    pub fn add_pinned_tag(&mut self, tag: MacroTag, cx: &mut Context<Self>) {
        if !self.draft_config.pinned_tags.iter().any(|t| t == &tag) {
            self.draft_config.pinned_tags.push(tag);
            cx.emit(ConfigEditorEvent::DraftChanged);
            cx.notify();
        }
    }

    pub fn remove_pinned_tag(&mut self, tag: &MacroTag, cx: &mut Context<Self>) {
        self.draft_config.pinned_tags.retain(|t| t != tag);
        cx.emit(ConfigEditorEvent::DraftChanged);
        cx.notify();
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

    fn render_config_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_form()
            .small()
            .columns(2)
            .label_width(px(88.0))
            .gap_x_8()
            .gap_y_3()
            .child(
                field().label("名称:").items_center().child(
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
                                        .icon(Icon::default().path(self.current_icon.clone())),
                                )
                                .content({
                                    let editor = cx.entity().clone();
                                    let current = self.current_icon.clone();
                                    move |_state, _window, cx| {
                                        div().p_2().v_flex().gap_1p5().child(
                                            div().grid().grid_cols(4).gap_1().children(
                                                PRESET_ICONS.iter().enumerate().map(
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
                                                                    editor.update(
                                                                        cx,
                                                                        |this, cx| {
                                                                            this.set_icon(
                                                                                &path_str, cx,
                                                                            );
                                                                        },
                                                                    );
                                                                    state.dismiss(window, cx);
                                                                }
                                                            }))
                                                    },
                                                ),
                                            ),
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
                field().label("视图:").items_center().child(
                    ButtonGroup::new("cfg-view-group")
                        .small()
                        .child(
                            Button::new("cfg-view-grid")
                                .icon(IconName::LayoutGrid)
                                .label("网格")
                                .selected(self.draft_config.view == ViewOption::Grid),
                        )
                        .child(
                            Button::new("cfg-view-list")
                                .icon(IconName::List)
                                .label("列表")
                                .selected(self.draft_config.view == ViewOption::List),
                        )
                        .child(
                            Button::new("cfg-view-gallery")
                                .icon(IconName::Image)
                                .label("画廊")
                                .selected(self.draft_config.view == ViewOption::Gallery),
                        )
                        .on_click(cx.listener(|this, clicks: &Vec<usize>, _window, cx| {
                            let views = [ViewOption::Grid, ViewOption::List, ViewOption::Gallery];
                            let Some(&view) = clicks.first().and_then(|&ix| views.get(ix)) else {
                                return;
                            };
                            this.draft_config.view = view;
                            cx.emit(ConfigEditorEvent::DraftChanged);
                            cx.notify();
                        })),
                ),
            )
            .child(
                field().label("排序:").items_center().child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("cfg-sort-dropdown")
                                .small()
                                .w(px(130.0))
                                .label(sort_field_label(self.draft_config.sort_field))
                                .dropdown_caret(true)
                                .dropdown_menu({
                                    let editor = cx.entity().clone();
                                    let current_sort = self.draft_config.sort_field;
                                    move |menu, _, _| {
                                        ALL_SORT_FIELDS.iter().fold(menu, |menu, &field| {
                                            let editor = editor.clone();
                                            menu.item(
                                                PopupMenuItem::new(sort_field_label(field))
                                                    .checked(current_sort == field)
                                                    .on_click(move |_, _, cx| {
                                                        editor.update(cx, |this, cx| {
                                                            this.draft_config.sort_field = field;
                                                            if field == FileSortField::Size {
                                                                if matches!(
                                                                    this.draft_config.group_mode,
                                                                    GroupMode::Time(_)
                                                                        | GroupMode::Batch
                                                                ) {
                                                                    this.draft_config.group_mode =
                                                                        GroupMode::Size;
                                                                }
                                                            } else if this.draft_config.group_mode
                                                                == GroupMode::Size
                                                                || (this.draft_config.group_mode
                                                                    == GroupMode::Batch
                                                                    && field
                                                                        != FileSortField::CreatedAt)
                                                            {
                                                                this.draft_config.group_mode =
                                                                    GroupMode::Time(TimeGroup::Day);
                                                            }
                                                            cx.emit(
                                                                ConfigEditorEvent::DraftChanged,
                                                            );
                                                            cx.notify();
                                                        });
                                                    }),
                                            )
                                        })
                                    }
                                }),
                        )
                        .child(
                            Button::new("cfg-sort-direction")
                                .small()
                                .label(if self.draft_config.is_desc {
                                    "降序 ↓"
                                } else {
                                    "升序 ↑"
                                })
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.draft_config.is_desc = !this.draft_config.is_desc;
                                    cx.emit(ConfigEditorEvent::DraftChanged);
                                    cx.notify();
                                })),
                        ),
                ),
            )
            .child(
                field().label("分组:").items_center().child(
                    Button::new("cfg-group-dropdown")
                        .small()
                        .w(px(130.0))
                        .label(group_mode_label(self.draft_config.group_mode))
                        .dropdown_caret(true)
                        .dropdown_menu({
                            let editor = cx.entity().clone();
                            let current_mode = self.draft_config.group_mode;
                            let options = if self.draft_config.sort_field == FileSortField::Size {
                                SIZE_GROUP_MODES
                            } else {
                                TIME_GROUP_MODES
                            };
                            move |menu, _, _| {
                                options.iter().fold(menu, |menu, &mode| {
                                    let editor = editor.clone();
                                    menu.item(
                                        PopupMenuItem::new(group_mode_label(mode))
                                            .checked(current_mode == mode)
                                            .on_click(move |_, _, cx| {
                                                editor.update(cx, |this, cx| {
                                                    this.draft_config.group_mode = mode;
                                                    // A batch is only contiguous when sorted by import time
                                                    if mode == GroupMode::Batch {
                                                        this.draft_config.sort_field =
                                                            FileSortField::CreatedAt;
                                                    }
                                                    cx.emit(ConfigEditorEvent::DraftChanged);
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                })
                            }
                        }),
                ),
            )
    }

    fn render_extend_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        h_form()
            .small()
            .columns(2)
            .label_width(px(88.0))
            .gap_x_8()
            .gap_y_3()
            .child(
                field().label("侧边视图:").items_center().child(
                    Switch::new("cfg-collection-type-switch")
                        .small()
                        .checked(self.collection_type == CollectionType::View)
                        .on_click(cx.listener(|this, checked: &bool, _window, cx| {
                            this.collection_type = if *checked {
                                CollectionType::View
                            } else {
                                CollectionType::Folder
                            };
                            cx.emit(ConfigEditorEvent::DraftChanged);
                            cx.notify();
                        })),
                ),
            )
            .child(
                field().label("隐藏标签栏:").items_center().child(
                    Switch::new("cfg-hide-tagbar-switch")
                        .small()
                        .checked(self.draft_config.is_hide_tagbar)
                        .on_click(cx.listener(|this, checked: &bool, _window, cx| {
                            this.draft_config.is_hide_tagbar = *checked;
                            cx.emit(ConfigEditorEvent::DraftChanged);
                            cx.notify();
                        })),
                ),
            )
            .child(
                field().label("拖拽导入:").items_center().child(
                    Switch::new("cfg-enable-drag-switch")
                        .small()
                        .checked(self.draft_config.enable_drag)
                        .on_click(cx.listener(|this, checked: &bool, _window, cx| {
                            this.draft_config.enable_drag = *checked;
                            cx.emit(ConfigEditorEvent::DraftChanged);
                            cx.notify();
                        })),
                ),
            )
            .child(
                field().label("固定标签:").items_center().child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_flex()
                        .items_center()
                        .gap_1p5()
                        .children(self.draft_config.pinned_tags.iter().enumerate().map(
                            |(idx, tag)| {
                                let tag_clone = tag.clone();
                                TagChip::new(("pinned-chip", idx), tag.display_name())
                                    .small()
                                    .trailing_icon(
                                        Icon::new(IconName::Close)
                                            .size_3()
                                            .text_color(theme.muted_foreground),
                                    )
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        this.remove_pinned_tag(&tag_clone, cx);
                                    }))
                            },
                        ))
                        .child(
                            Button::new("btn-add-pinned-tag")
                                .small()
                                .ghost()
                                .icon(IconName::Plus)
                                .dropdown_menu({
                                    let editor = cx.entity().clone();
                                    let current_pinned = self.draft_config.pinned_tags.clone();
                                    move |menu, _, _| {
                                        preset_macro_tags()
                                            .into_iter()
                                            .filter(|t| !current_pinned.contains(t))
                                            .fold(menu, |menu, tag| {
                                                let editor = editor.clone();
                                                menu.item(
                                                    PopupMenuItem::new(tag.display_name())
                                                        .on_click(move |_, _, cx| {
                                                            editor.update(cx, |this, cx| {
                                                                this.add_pinned_tag(
                                                                    tag.clone(),
                                                                    cx,
                                                                );
                                                            });
                                                        }),
                                                )
                                            })
                                    }
                                }),
                        ),
                ),
            )
            .child(
                field().label("右键操作:").items_center().child(
                    div().h_flex().items_center().gap_3().children(
                        CONTEXT_MENU_STATES.iter().map(|&state| {
                            Checkbox::new(("cfg-context-state", state as usize))
                                .small()
                                .label(state.display_name())
                                .checked(self.draft_config.context_states.contains(&state))
                                .on_click(cx.listener(move |this, checked: &bool, _window, cx| {
                                    let states = &mut this.draft_config.context_states;
                                    states.retain(|s| *s != state);
                                    if *checked {
                                        states.push(state);
                                    }
                                    cx.emit(ConfigEditorEvent::DraftChanged);
                                    cx.notify();
                                }))
                        }),
                    ),
                ),
            )
            .child(
                field()
                    .label(if self.confirm_delete {
                        "确认删除:"
                    } else {
                        "操作:"
                    })
                    .items_center()
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
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.confirm_delete = false;
                                        cx.emit(ConfigEditorEvent::DeleteRequested);
                                    })),
                            )
                            .child(
                                Button::new("btn-cancel-delete")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Close)
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.confirm_delete = false;
                                        cx.notify();
                                    })),
                            )
                            .into_any_element()
                    }),
            )
    }

    fn render_filter_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
            .v_flex()
            .gap_3()
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
                                        Input::new(&self.tag_search_input).small().cleanable(true),
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
                        this.child(div().w(px(1.)).h_5().mx_0p5().bg(theme.border.darken(0.2)))
                    })
                    .children(unselected_macro_chips)
                    .when(has_macro && has_standard, |this| {
                        this.child(div().w(px(1.)).h_5().mx_0p5().bg(theme.border.darken(0.2)))
                    })
                    .children(unselected_standard_chips),
            )
    }
}

impl Render for CollectionConfigEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let expanded = self.expanded;
        // Keep the section mounted until its collapse animation has played out.
        let collapsing = !expanded
            && self
                .expand_toggled_at
                .is_some_and(|at| at.elapsed() < EXPAND_TRANSITION);
        let section_height = self.extend_section_height.clone();

        let btn_variant = ButtonCustomVariant::new(cx)
            .color(cx.theme().transparent)
            .foreground(cx.theme().muted_foreground)
            .hover(cx.theme().muted.darken(0.05))
            .active(cx.theme().muted.darken(0.10));

        div()
            .id("collection-config-editor")
            .v_flex()
            .gap_2()
            .py_3()
            .min_h_0()
            .on_action(cx.listener(|this, _: &Escape, _window, cx| {
                cx.stop_propagation();
                if this.expanded {
                    this.toggle_expanded(cx);
                } else {
                    cx.emit(ConfigEditorEvent::CancelRequested);
                }
            }))
            .child(
                div()
                    .h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .px_6()
                    .gap_2()
                    .child(
                        Button::new("scope-back")
                            .custom(btn_variant)
                            .icon(IconName::Close)
                            .on_click(cx.listener(|_this, _, _window, cx| {
                                cx.emit(ConfigEditorEvent::CancelRequested);
                            })),
                    )
                    .child(div().text_xl().font_bold().child("视图属性编辑"))
                    .child(
                        Button::new("cfg-expand-toggle")
                            .ml_auto()
                            .small()
                            .icon(if self.expanded {
                                IconName::ChevronUp
                            } else {
                                IconName::ChevronDown
                            })
                            .label(if self.expanded {
                                "收起更多"
                            } else {
                                "更多设置"
                            })
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.toggle_expanded(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .relative()
                    .v_flex()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("cfg-editor-scroll")
                            .v_flex()
                            .min_h_0()
                            .track_scroll(&self.scroll_handle)
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .flex_none()
                                    .px_6()
                                    .v_flex()
                                    .child(
                                        div()
                                            .v_flex()
                                            .gap_3()
                                            .child(self.render_config_form(cx))
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h(px(1.))
                                                    .my_1()
                                                    .bg(cx.theme().border.opacity(0.4)),
                                            )
                                            .child(self.render_filter_editor(cx)),
                                    )
                                    .when(expanded || collapsing, |this| {
                                        this.child(
                                            div()
                                                .overflow_hidden()
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .pt_3()
                                                        .v_flex()
                                                        .gap_3()
                                                        .child(
                                                            div()
                                                                .w_full()
                                                                .h(px(1.))
                                                                .my_1()
                                                                .bg(cx.theme().border.opacity(0.4)),
                                                        )
                                                        .child(self.render_extend_form(cx))
                                                        .on_prepaint({
                                                            let section_height =
                                                                section_height.clone();
                                                            move |bounds, _, _| {
                                                                section_height
                                                                    .set(bounds.size.height);
                                                            }
                                                        }),
                                                )
                                                .with_animation(
                                                    ("cfg-more-section", expanded as usize),
                                                    Animation::new(EXPAND_TRANSITION)
                                                        .with_easing(ease_out_quint()),
                                                    move |this, delta| {
                                                        if expanded && delta >= 1.0 {
                                                            return this;
                                                        }
                                                        let progress = if expanded {
                                                            delta
                                                        } else {
                                                            1.0 - delta
                                                        };
                                                        this.max_h(section_height.get() * progress)
                                                            .opacity(progress)
                                                    },
                                                ),
                                        )
                                    }),
                            ),
                    )
                    .vertical_scrollbar(&self.scroll_handle),
            )
    }
}
