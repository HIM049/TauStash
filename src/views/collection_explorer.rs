use std::collections::HashSet;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Enter, Escape, InputState};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::components::grid_card::GridCard;
use crate::components::item_grid_view::ItemGridView;
use crate::components::view_scaffold::ViewScaffold;
use crate::database::entities::collection;
use crate::views::generic_view::GenericView;
use rust_i18n::t;

pub struct CollectionExplorerView {
    folders: Vec<collection::Model>,
    scroll_handle: UniformListScrollHandle,
    selected: HashSet<Uuid>,
    last_select: Option<Uuid>,
    cursor_id: Option<Uuid>,

    name_input: Entity<InputState>,
    focus_handle: FocusHandle,
    editing_id: Option<Uuid>,

    active_folder_view: Option<Entity<GenericView>>,
}

impl CollectionExplorerView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx));
        let focus_handle = cx.focus_handle();

        let collections_entity = cx.global::<AppState>().collections.clone();
        cx.observe(&collections_entity, |this, _store, cx| {
            this.reload(cx);
        })
        .detach();

        Self {
            folders: collections_entity.read(cx).collections.clone(),
            scroll_handle: UniformListScrollHandle::new(),
            selected: HashSet::new(),
            last_select: None,
            cursor_id: None,
            name_input,
            focus_handle,
            editing_id: None,
            active_folder_view: None,
        }
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.folders = cx
            .global::<AppState>()
            .collections
            .read(cx)
            .collections
            .clone();
        self.editing_id = None;
        if let Some(cursor) = self.cursor_id {
            if !self.folders.iter().any(|f| f.id == cursor) {
                self.cursor_id = None;
            }
        }
        if let Some(active_view) = &self.active_folder_view {
            if let Some(open_id) = active_view.read(cx).collection_id() {
                if !self.folders.iter().any(|f| f.id == open_id) {
                    self.active_folder_view = None;
                }
            }
        }
        cx.notify();
    }

    pub fn open_collection(
        &mut self,
        folder: &collection::Model,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this_entity = cx.entity().clone();
        let view = cx.new(|cx| {
            let view = GenericView::from_collection(folder, window, cx);
            view.on_back(move |_window, cx| {
                this_entity.update(cx, |this, cx| {
                    this.active_folder_view = None;
                    this.reload(cx);
                });
            })
        });
        self.active_folder_view = Some(view);
        cx.notify();
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
        self.cursor_id = Some(id);

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
        if self.selected.is_empty() && self.last_select.is_none() && self.cursor_id.is_none() {
            return;
        }
        self.selected.clear();
        self.last_select = None;
        self.cursor_id = None;
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
            name: t!("collection_explorer.untitled").into_owned(),
            icon: IconName::Folder.path().into(),
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
        self.start_rename(dummy_id, &t!("collection_explorer.untitled"), window, cx);
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
        self.last_select = Some(id);
        self.cursor_id = Some(id);

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

        let collections = cx.global::<AppState>().collections.clone();
        if target_id == Uuid::nil() {
            // New collection creation
            collections.update(cx, |s, cx| {
                s.create_collection(&name, cx);
            });
            self.name_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            self.editing_id = None;
        } else {
            // Existing collection rename
            collections.update(cx, |s, cx| {
                s.rename_collection(target_id, &name, cx);
            });
            self.name_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            self.editing_id = None;
        }
    }

    /// When right-clicking an item, if it is not selected, clear other selections and select it.
    fn handle_secondary_click(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.cursor_id = Some(id);
        if !self.selected.contains(&id) {
            self.select_only(id, cx);
            self.last_select = Some(id);
        }
    }

    /// Soft delete all currently selected collections and reload.
    pub fn delete_selected(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            return;
        }

        let ids: Vec<Uuid> = self.selected.iter().copied().collect();
        self.selected.clear();
        self.last_select = None;
        self.cursor_id = None;

        let collections = cx.global::<AppState>().collections.clone();
        collections.update(cx, |s, cx| {
            s.delete_collections(&ids, cx);
        });
    }

    fn build_context_menu(
        &self,
        mut menu: PopupMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PopupMenu {
        if let Some(id) = self.last_select {
            let is_multi = self.selected.len() > 1 && self.selected.contains(&id);
            if is_multi {
                let count = self.selected.len();
                menu = menu.item(
                    PopupMenuItem::new(t!("collection_explorer.delete_count", count = count))
                        .icon(IconName::Delete)
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.delete_selected(w, cx);
                        })),
                );
            } else {
                let folder_name = self
                    .folders
                    .iter()
                    .find(|f| f.id == id)
                    .map(|f| f.name.clone())
                    .unwrap_or_default();
                let folder = self.folders.iter().find(|f| f.id == id).cloned();
                menu = menu
                    .when_some(folder, |menu, folder| {
                        menu.item(
                            PopupMenuItem::new(t!("collection_explorer.open_in_new_window"))
                                .icon(IconName::ExternalLink)
                                .on_click(move |_, _, cx| {
                                    AppState::open_view_window(&folder, cx);
                                }),
                        )
                    })
                    .item(
                        PopupMenuItem::new(t!("common.rename"))
                            .icon(IconName::Replace)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.start_rename(id, &folder_name, w, cx);
                            })),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new(t!("collection_explorer.delete"))
                            .icon(IconName::Delete)
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.delete_selected(w, cx);
                            })),
                    );
            }
        } else {
            menu = menu
                .item(
                    PopupMenuItem::new(t!("common.refresh"))
                        .icon(IconName::RefreshCw)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reload(cx);
                        })),
                )
                .separator()
                .item(
                    PopupMenuItem::new(t!("collection_explorer.new"))
                        .icon(IconName::Plus)
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.start_create(w, cx);
                        })),
                );
        }

        menu
    }
}

impl Render for CollectionExplorerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(folder_view) = &self.active_folder_view {
            return folder_view.clone().into_any_element();
        }

        let menu_entity = cx.entity().clone();

        div()
            .id("collection-explorer-main")
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Enter, window, cx| {
                this.commit_edit(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.cancel_edit(window, cx);
            }))
            .context_menu({
                let menu_entity = menu_entity.clone();
                move |menu, window, cx| {
                    menu_entity.update(cx, |this, cx| this.build_context_menu(menu, window, cx))
                }
            })
            .child(
                ViewScaffold::new("collection-explorer-scaffold", t!("common.all_collections"))
                    .actions(vec![
                        Button::new("collection-refresh")
                            .icon(IconName::RefreshCw)
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.reload(cx);
                            })),
                        Button::new("collection-new")
                            .icon(IconName::Plus)
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.start_create(window, cx);
                            })),
                    ])
                    .child({
                        let selected = self.selected.clone();
                        let cursor_id = self.cursor_id;
                        let editing_id = self.editing_id;
                        let name_input = self.name_input.clone();
                        let entity = cx.entity().clone();

                        let cards: Vec<GridCard> = self
                            .folders
                            .iter()
                            .map(|folder| {
                                let id = folder.id;
                                let is_selected = selected.contains(&id);
                                let is_cursor = cursor_id == Some(id);
                                let is_editing = editing_id == Some(id);

                                let cell_entity = entity.clone();
                                let sec_entity = entity.clone();
                                let dbl_entity = entity.clone();
                                let dbl_folder = folder.clone();

                                GridCard::from(folder)
                                    .selected(is_selected)
                                    .cursor(is_cursor)
                                    .editing(is_editing)
                                    .input(if is_editing {
                                        Some(name_input.clone())
                                    } else {
                                        None
                                    })
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
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
                                    .on_double_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        dbl_entity.update(cx, |this, cx| {
                                            this.open_collection(&dbl_folder, window, cx);
                                        });
                                    })
                                    .on_secondary_click(move |_window, cx| {
                                        sec_entity.update(cx, |this, cx| {
                                            this.handle_secondary_click(id, cx);
                                        });
                                    })
                            })
                            .collect();

                        div().size_full().pl_6().child(
                            ItemGridView::new("collection-grid", cards)
                                .scroll_handle(&self.scroll_handle)
                                .empty_placeholder(t!("collection_explorer.empty")),
                        )
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.editing_id.is_some() {
                            this.commit_edit(window, cx);
                        } else {
                            this.clear_selection(cx);
                        }
                    }))
                    .on_sub_click(cx.listener(|this, _, window, cx| {
                        if this.editing_id.is_some() {
                            this.commit_edit(window, cx);
                        } else {
                            this.clear_selection(cx);
                        }
                    })),
            )
            .into_any_element()
    }
}
