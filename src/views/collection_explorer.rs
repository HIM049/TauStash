use std::collections::HashSet;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Enter, Escape, InputState};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::components::collection_card::CollectionCard;
use crate::database::entities::collection;
use crate::database::storage::collection as collection_store;
use crate::services::collection::CollectionConfig;

const DEFAULT_FOLDER_NAME: &str = "未命名合集";

/// Width one card occupies, including the gap that follows it.
const CARD_SLOT: f32 = 142.0;
/// Height of one grid row: the 150px card plus the gap.
const ROW_HEIGHT: f32 = 154.0;

/// Number of cards that fit on one row at the given width.
fn columns_for(width: Pixels) -> usize {
    let available = (width - px(48.0)).max(px(152.0));
    (available.as_f32() / CARD_SLOT).floor().max(1.0) as usize
}

pub struct CollectionExplorerView {
    folders: Vec<collection::Model>,
    scroll_handle: UniformListScrollHandle,
    selected: HashSet<Uuid>,
    last_select: Option<Uuid>,
    container_width: Pixels,
    loading: bool,

    name_input: Entity<InputState>,
    focus_handle: FocusHandle,
    editing_id: Option<Uuid>,
}

impl CollectionExplorerView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx));
        let focus_handle = cx.focus_handle();

        let mut view = Self {
            folders: Vec::new(),
            scroll_handle: UniformListScrollHandle::new(),
            selected: HashSet::new(),
            last_select: None,
            container_width: px(800.0),
            loading: false,
            name_input,
            focus_handle,
            editing_id: None,
        };
        view.reload(cx);
        view
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.editing_id = None;

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn(async move |this, cx| {
            let items = cx
                .background_spawn(async move {
                    rt.block_on(collection_store::list_by_type(
                        &db,
                        collection::CollectionType::Folder,
                    ))
                })
                .await;

            this.update(cx, |this, cx| {
                this.loading = false;
                if let Ok(folders) = items {
                    this.folders = folders;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn selected_ids(&self) -> &HashSet<Uuid> {
        &self.selected
    }

    fn select_only(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.selected.len() == 1 && self.selected.contains(&id) {
            return;
        }
        self.selected.clear();
        self.selected.insert(id);
        cx.notify();
    }

    fn handle_select(&mut self, id: Uuid, is_ctrl: bool, is_shift: bool, cx: &mut Context<Self>) {
        if is_ctrl {
            if self.selected.contains(&id) {
                self.selected.remove(&id);
            } else {
                self.selected.insert(id);
            }
            self.last_select = Some(id);
            cx.notify();
            return;
        }

        if is_shift && let Some(anchor) = self.last_select {
            if let (Some(from), Some(to)) = (self.index_of(anchor), self.index_of(id)) {
                self.selected.clear();
                let (start, end) = (from.min(to), from.max(to));
                for folder in &self.folders[start..=end] {
                    self.selected.insert(folder.id);
                }
                cx.notify();
                return;
            }
        }

        self.select_only(id, cx);
        self.last_select = Some(id);
    }

    fn index_of(&self, id: Uuid) -> Option<usize> {
        self.folders.iter().position(|f| f.id == id)
    }

    fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            return;
        }
        self.selected.clear();
        self.last_select = None;
        cx.notify();
    }

    /// Insert a transient dummy item at the head of the list and enter edit mode.
    fn start_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dummy_id = Uuid::nil();
        if self.editing_id == Some(dummy_id) {
            return;
        }

        // Clean uncommitted draft
        self.folders.retain(|f| f.id != dummy_id);

        let now = chrono::Utc::now();
        let dummy = collection::Model {
            id: dummy_id,
            name: DEFAULT_FOLDER_NAME.to_string(),
            icon: String::new(),
            cover: String::new(),
            color: String::new(),
            order: 0,
            parent_id: None,
            typ: collection::CollectionType::Folder,
            config: serde_json::Value::Null,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        self.folders.insert(0, dummy);
        self.start_rename(dummy_id, DEFAULT_FOLDER_NAME, window, cx);
    }

    /// Enter inline renaming mode for an existing (or dummy) collection.
    pub fn start_rename(
        &mut self,
        id: Uuid,
        initial_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing_id = Some(id);
        self.selected.clear();
        self.selected.insert(id);

        self.name_input.update(cx, |input, cx| {
            input.set_value(initial_name, window, cx);
            input.select_all(window, cx);
        });

        let focus = self.name_input.read(cx).focus_handle(cx);
        focus.focus(window, cx);

        cx.notify();
    }

    /// Cancel editing/creation and immediately reload from database to purge the dummy item.
    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_none() {
            return;
        }
        self.name_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.selected.clear();
        self.last_select = None;
        self.reload(cx);
    }

    /// Commit the inline edit (either creating a new collection or renaming an existing one).
    fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target_id) = self.editing_id else {
            return;
        };

        let name = self.name_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.cancel_edit(window, cx);
            return;
        }

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        if target_id == Uuid::nil() {
            // New collection creation
            let config = match serde_json::to_value(CollectionConfig::default()) {
                Ok(config) => config,
                Err(err) => {
                    tracing::error!("序列化合集配置失败: {err:#}");
                    return;
                }
            };

            cx.spawn_in(window, async move |this, cx| {
                let created = cx
                    .background_spawn(async move {
                        rt.block_on(collection_store::create(
                            &db,
                            name,
                            collection::CollectionType::Folder,
                            None,
                            None,
                            Some(config),
                            None,
                            None,
                            None,
                        ))
                    })
                    .await;

                this.update(cx, |this, cx| match created {
                    Ok(_) => this.reload(cx),
                    Err(err) => {
                        tracing::error!("创建合集失败: {err:#}");
                        this.reload(cx);
                    }
                })
                .ok();
            })
            .detach();
        } else {
            // Existing collection rename
            cx.spawn_in(window, async move |this, cx| {
                let updated = cx
                    .background_spawn(async move {
                        rt.block_on(collection_store::update_metadata(
                            &db,
                            target_id,
                            Some(name),
                            None,
                            None,
                            None,
                            None,
                        ))
                    })
                    .await;

                this.update(cx, |this, cx| match updated {
                    Ok(_) => this.reload(cx),
                    Err(err) => {
                        tracing::error!("重命名合集失败: {err:#}");
                        this.reload(cx);
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// When right-clicking an item, if it is not selected, clear other selections and select it.
    fn handle_secondary_click(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if !self.selected.contains(&id) {
            self.select_only(id, cx);
            self.last_select = Some(id);
        }
    }

    /// Soft delete all currently selected collections and reload.
    pub fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            return;
        }

        let ids: Vec<Uuid> = self.selected.iter().copied().collect();
        self.selected.clear();
        self.last_select = None;

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn_in(window, async move |this, cx| {
            let res = cx
                .background_spawn(async move { rt.block_on(collection_store::delete(&db, &ids)) })
                .await;

            this.update(cx, |this, cx| match res {
                Ok(_) => this.reload(cx),
                Err(err) => {
                    tracing::error!("删除失败: {err:#}");
                }
            })
            .ok();
        })
        .detach();
    }

    fn build_context_menu(
        &self,
        id: Uuid,
        name: &str,
        menu: PopupMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PopupMenu {
        let is_multi = self.selected.len() > 1 && self.selected.contains(&id);
        let mut m = menu;
        let view_entity = cx.entity().clone();

        if is_multi {
            let count = self.selected.len();
            m = m.item(
                PopupMenuItem::new(format!("删除所选 ({count})"))
                    .icon(IconName::Delete)
                    .on_click(move |_, window, cx| {
                        view_entity.update(cx, |this, cx| {
                            this.delete_selected(window, cx);
                        });
                    }),
            );
        } else {
            let rename_name = name.to_string();
            let rename_entity = view_entity.clone();
            m = m
                .item(
                    PopupMenuItem::new("重命名")
                        .icon(IconName::Replace)
                        .on_click(move |_, window, cx| {
                            rename_entity.update(cx, |this, cx| {
                                this.start_rename(id, &rename_name, window, cx);
                            });
                        }),
                )
                .separator()
                .item(PopupMenuItem::new("删除").icon(IconName::Delete).on_click(
                    move |_, window, cx| {
                        view_entity.update(cx, |this, cx| {
                            this.delete_selected(window, cx);
                        });
                    },
                ));
        }

        m
    }

    fn render_empty(&self, cx: &mut App) -> AnyElement {
        div()
            .h_flex()
            .items_center()
            .justify_center()
            .size_full()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(if self.loading {
                "加载中..."
            } else {
                "暂无合集"
            })
            .into_any_element()
    }

    fn render_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let cols = columns_for(self.container_width);
        let name_input = self.name_input.clone();
        let folders = self.folders.clone();
        let total = folders.len();
        let rows_count = total.div_ceil(cols);
        let selected = self.selected.clone();
        let editing_id = self.editing_id;
        let entity = cx.entity().clone();

        div()
            .relative()
            .flex_1()
            .size_full()
            .child(
                uniform_list(
                    "collection-grid-list",
                    rows_count,
                    move |visible_range, _, _| {
                        visible_range
                            .map(|row| {
                                let start = row * cols;
                                let end = (start + cols).min(total);
                                let selected = &selected;

                                let mut cells: Vec<AnyElement> = Vec::with_capacity(cols);
                                for slot in start..end {
                                    let Some(folder) = folders.get(slot) else {
                                        continue;
                                    };
                                    let id = folder.id;
                                    let is_selected = selected.contains(&id);
                                    let is_editing = editing_id == Some(id);

                                    let cell_entity = entity.clone();
                                    let sec_entity = entity.clone();
                                    let menu_entity = entity.clone();
                                    let folder_name = folder.name.clone();

                                    cells.push(
                                        CollectionCard::new(
                                            ElementId::Name(format!("collection-card-{id}").into()),
                                            SharedString::new(folder.name.as_str()),
                                        )
                                        .icon(folder.icon.as_str())
                                        .cover(folder.cover.as_str())
                                        .selected(is_selected)
                                        .editing(is_editing)
                                        .input(if is_editing {
                                            Some(name_input.clone())
                                        } else {
                                            None
                                        })
                                        .on_click(move |_, window, cx| {
                                            if is_editing {
                                                return;
                                            }
                                            let modifiers = window.modifiers();
                                            let is_ctrl = modifiers.control || modifiers.platform;
                                            let is_shift = modifiers.shift;

                                            cell_entity.update(cx, |this, cx| {
                                                this.handle_select(id, is_ctrl, is_shift, cx);
                                            });
                                        })
                                        .on_secondary_click(move |_window, cx| {
                                            sec_entity.update(cx, |this, cx| {
                                                this.handle_secondary_click(id, cx);
                                            });
                                        })
                                        .context_menu(move |menu, window, cx| {
                                            menu_entity.update(cx, |this, cx| {
                                                this.build_context_menu(
                                                    id,
                                                    &folder_name,
                                                    menu,
                                                    window,
                                                    cx,
                                                )
                                            })
                                        })
                                        .into_any_element(),
                                    );
                                }

                                div()
                                    .h(px(ROW_HEIGHT))
                                    .h_flex()
                                    .gap_1()
                                    .children(cells)
                                    .into_any_element()
                            })
                            .collect()
                    },
                )
                .pb_6()
                .size_full()
                .track_scroll(&self.scroll_handle),
            )
            .vertical_scrollbar(&self.scroll_handle)
            .into_any_element()
    }
}

impl Render for CollectionExplorerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width_entity = cx.entity().clone();
        let current_width = self.container_width;

        div()
            .id("collection-explorer-main")
            .v_flex()
            .size_full()
            .gap_3()
            .p_6()
            .pb_0()
            .pr_0()
            .track_focus(&self.focus_handle)
            .on_click(cx.listener(|this, _, window, cx| {
                if this.editing_id.is_some() {
                    this.commit_edit(window, cx);
                } else {
                    this.clear_selection(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Enter, window, cx| {
                this.commit_edit(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.cancel_edit(window, cx);
            }))
            .on_prepaint(move |bounds, _, cx| {
                let new_w = bounds.size.width;
                if (new_w - current_width).abs() > px(2.0) {
                    width_entity.update(cx, |this, cx| {
                        this.container_width = new_w;
                        cx.notify();
                    });
                }
            })
            .child(
                div()
                    .id("collection-top-bar")
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(div().text_xl().font_bold().child("全部合集"))
                    .child(
                        Button::new("collection-refresh")
                            .icon(IconName::RefreshCw)
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.reload(cx);
                            })),
                    )
                    .child(Button::new("collection-new").icon(IconName::Plus).on_click(
                        cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.start_create(window, cx);
                        }),
                    )),
            )
            .child(if self.folders.is_empty() {
                self.render_empty(cx)
            } else {
                self.render_grid(cx)
            })
    }
}
