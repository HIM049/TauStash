use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::components::collection_config_editor::{CollectionConfigEditor, ConfigEditorEvent};
use crate::components::generic_file_view::{FileContentEvent, GenericFileView};
use crate::components::platform;
use crate::components::tag_bar_editor::TagBatchEditor;
use crate::components::tag_bar_filter::{TagFilterBar, TagFilterEvent};
use crate::components::view_scaffold::ViewScaffold;
use crate::database::entities::collection::{self, CollectionType};
use crate::database::storage::collection as collection_store;
use crate::database::storage::query::SortDirection;
use crate::services::collection::{CollectionConfig, ViewOption};
use crate::services::file;
use crate::services::importer;
use crate::services::macrotag::{FileState, MacroTag};
use crate::services::storage;
use crate::services::tags_queryer::MtagService;
use rust_i18n::t;

#[derive(Clone, Default)]
pub enum BarMode {
    #[default]
    Normal,
    BatchEditing,
    Configuring(Entity<CollectionConfigEditor>),
}

pub struct GenericView {
    collection_id: Option<Uuid>,
    collection_type: CollectionType,
    config: CollectionConfig,
    title: SharedString,
    icon: Option<SharedString>,
    on_back: Option<Rc<dyn Fn(&mut Window, &mut App) + 'static>>,

    content_view: Entity<GenericFileView>,
    filter_bar: TagFilterBar,
    filter_bar_hovered: bool,
    show_intersection: bool,
    batch_editor: Entity<TagBatchEditor>,
    bar_mode: BarMode,
    main_height: Rc<Cell<Pixels>>,
    editor_panel_height: Rc<Cell<Pixels>>,
    current_filter_tags: Vec<MacroTag>,
    preview_filter_tags: Option<Vec<MacroTag>>,
    candidate_tags: Vec<MacroTag>,
    // for any working state
    working: bool,
    /// Dropping now moves files into the store
    drop_move: bool,
}

impl GenericView {
    pub fn new(
        title: impl Into<SharedString>,
        config: CollectionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let base_query = MtagService::new()
            .tags(&config.filters)
            .build()
            .unwrap_or_default();

        let filter_bar = TagFilterBar::with_excluded_tags(base_query, config.filters.clone());

        let overall_entity = cx.entity().downgrade();
        let batch_editor = cx.new(|cx| {
            TagBatchEditor::new(window, cx)
                .on_commit({
                    let entity = overall_entity.clone();
                    move |ensure, remove, _, cx| {
                        if let Some(entity) = entity.upgrade() {
                            let ensure = ensure.to_vec();
                            let remove = remove.to_vec();
                            entity.update(cx, |this, cx| {
                                this.commit_tag_changes(ensure, remove, cx);
                            });
                        }
                    }
                })
                .on_cancel({
                    let entity = overall_entity.clone();
                    move |_window, cx| {
                        if let Some(entity) = entity.upgrade() {
                            entity.update(cx, |this, cx| {
                                this.clear_selection(cx);
                            });
                        }
                    }
                })
        });

        let content_view = cx.new(|cx| {
            GenericFileView::new(
                config.view,
                config.sort_field,
                config.group_mode,
                config.context_states.clone(),
                window,
                cx,
            )
        });
        cx.subscribe(
            &content_view,
            |this, _, event: &FileContentEvent, cx| match event {
                FileContentEvent::SelectionChanged => {
                    this.sync_batch_editor(cx);
                }
                FileContentEvent::SetStateRequested(state) => {
                    this.commit_tag_changes(vec![MacroTag::State(*state)], vec![], cx);
                }
                FileContentEvent::RefreshRequested => {
                    let file = cx.global::<AppState>().file.clone();
                    file.update(cx, |_, cx| cx.notify());
                }
            },
        )
        .detach();

        let tags_entity = cx.global::<AppState>().tags.clone();
        cx.observe(&tags_entity, |this, tag_store, cx| {
            this.candidate_tags = tag_store.read(cx).cached_tags.clone();
            cx.notify();
        })
        .detach();
        let file_entity = cx.global::<AppState>().file.clone();
        cx.observe(&file_entity, |this, _, cx| {
            this.reload_all(cx);
        })
        .detach();
        let candidate_tags = tags_entity.read(cx).cached_tags.clone();

        // Ctrl released in another window never reaches this one
        cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.leave_filter_bar(cx);
            }
        })
        .detach();

        let mut view = Self {
            collection_id: None,
            collection_type: CollectionType::View,
            config: config.clone(),
            title: title.into(),
            icon: None,
            on_back: None,

            content_view,
            filter_bar,
            filter_bar_hovered: false,
            show_intersection: false,
            batch_editor,
            bar_mode: BarMode::Normal,
            main_height: Rc::new(Cell::new(px(0.))),
            editor_panel_height: Rc::new(Cell::new(px(0.))),
            current_filter_tags: Vec::new(),
            preview_filter_tags: None,
            candidate_tags,
            working: false,
            drop_move: false,
        };

        view.reload_files(cx);
        view.reload_candidates(cx);
        view.reload_filter_stats(cx);
        view
    }

    pub fn from_collection(
        model: &collection::Model,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let config: CollectionConfig =
            serde_json::from_value(model.config.clone()).unwrap_or_default();

        let mut view = Self::new(model.name.clone(), config, window, cx);
        view.icon = Some(model.icon.clone().into());
        view.collection_id = Some(model.id);
        view.collection_type = model.typ;
        view
    }

    pub fn collection_id(&self) -> Option<Uuid> {
        self.collection_id
    }

    pub fn on_back(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_back = Some(Rc::new(handler));
        self
    }

    pub fn icon(mut self, icon: impl Into<Option<SharedString>>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn pinned_tags(mut self, tags: Vec<MacroTag>) -> Self {
        self.config.pinned_tags = tags;
        self
    }

    pub fn handle_drag(
        &mut self,
        window: &mut Window,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if !self.config.enable_drag || self.working || paths.is_empty() {
            return;
        }

        let config = &cx.global::<AppState>().config;
        let store_root = Self::drop_moves(window, cx).then(|| config.store_path.clone());
        let device_id = config.device_id.clone();
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        self.working = true;
        window.push_notification(Notification::new().message(t!("import.in_progress")), cx);
        cx.notify();

        let filters = &self.config.filters;
        let mut tags: Vec<MacroTag> = filters
            .iter()
            .filter(|t| t.is_writable())
            .cloned()
            .collect();
        // State fallback to managed
        if !filters.iter().any(|t| matches!(t, MacroTag::State(_))) {
            tags.push(MacroTag::State(FileState::Managed));
        }

        let is_move = store_root.is_some();
        cx.spawn_in(window, async move |this, cx: &mut AsyncWindowContext| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(async {
                        match store_root {
                            Some(root) => {
                                storage::move_from_paths(&db, &root, &paths, &device_id, &tags)
                                    .await
                            }
                            None => {
                                importer::import_from_paths(&db, &paths, &device_id, &tags).await
                            }
                        }
                    })
                })
                .await;

            let msg = this
                .update(cx, |this, cx| {
                    this.working = false;
                    let message: SharedString = if let Ok(report) = res {
                        if report.failed.is_empty() && is_move {
                            t!("import.moved", count = report.newly_imported).into()
                        } else if report.failed.is_empty() {
                            t!("import.done", count = report.newly_imported).into()
                        } else {
                            t!("import.failed_count", count = report.failed.len()).into()
                        }
                    } else {
                        t!("import.failed").into()
                    };

                    let file = cx.global::<AppState>().file.clone();
                    file.update(cx, |_, cx| cx.notify());
                    cx.global::<AppState>()
                        .schedule
                        .trigger_now("maintain_thumbnails");
                    message
                })
                .ok();

            if let Some(msg) = msg {
                cx.update(|w, cx| {
                    w.push_notification(Notification::new().message(msg), cx);
                })
                .unwrap();
            }
        })
        .detach();
    }

    /// Shift moves dropped files into the store when it is enabled.
    fn drop_moves(window: &Window, cx: &App) -> bool {
        cx.global::<AppState>().config.store_enable
            && platform::shift_pressed().unwrap_or(window.modifiers().shift)
    }

    fn sync_drop_move(&mut self, window: &Window, cx: &mut Context<Self>) {
        let drop_move = Self::drop_moves(window, cx);
        if self.drop_move != drop_move {
            self.drop_move = drop_move;
            cx.notify();
        }
    }

    pub fn toggle_view(&mut self, cx: &mut Context<Self>) {
        self.config.view = match self.config.view {
            ViewOption::List => ViewOption::Grid,
            _ => ViewOption::List,
        };
        self.content_view.update(cx, |view, cx| {
            view.set_view_mode(self.config.view, cx);
        });
        self.persist_config(cx);
        cx.notify();
    }

    fn persist_config(&self, cx: &mut Context<Self>) {
        if let Some(id) = self.collection_id {
            let views = cx.global::<AppState>().views.clone();
            let collections = cx.global::<AppState>().collections.clone();
            let config = self.config.clone();

            views.update(cx, |s, cx| {
                s.update_config(id, &config, cx);
            });
            collections.update(cx, |s, cx| {
                s.update_config(id, &config, cx);
            });
        }
    }

    pub fn apply_filter(&mut self, tags: Vec<MacroTag>, cx: &mut Context<Self>) {
        if self.current_filter_tags == tags {
            return;
        }

        self.current_filter_tags = tags;
        self.content_view.update(cx, |view, _cx| {
            view.scroll_handle().scroll_to_item(0, ScrollStrategy::Top);
        });

        self.reload_files(cx);
        self.filter_stats(cx);
        cx.notify();
    }

    fn handle_filter_bar_pointer(
        &mut self,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.filter_bar_hovered = hovered;
        // Modifier events follow the focus path: without focus, pressing Ctrl goes unseen
        if hovered && window.focused(cx).is_none() {
            let focus_handle = self.content_view.read(cx).focus_handle().clone();
            focus_handle.focus(window, cx);
        }
        self.sync_intersection(window, cx);
    }

    /// Intersection counts only predict a Ctrl click. Requires the pointer on the bar
    /// because Ctrl alone also means file multi-select and shortcuts.
    fn sync_intersection(&mut self, window: &Window, cx: &mut Context<Self>) {
        let show = self.filter_bar_hovered
            && matches!(self.bar_mode, BarMode::Normal)
            && window.modifiers().secondary();
        if show != self.show_intersection {
            self.show_intersection = show;
            self.filter_stats(cx);
        }
    }

    fn leave_filter_bar(&mut self, cx: &mut Context<Self>) {
        self.filter_bar_hovered = false;
        if self.show_intersection {
            self.show_intersection = false;
            self.filter_stats(cx);
        }
    }

    pub fn filter_stats(&mut self, cx: &mut Context<Self>) {
        let config_filters = self
            .preview_filter_tags
            .as_ref()
            .unwrap_or(&self.config.filters);

        if config_filters.is_empty() {
            return;
        }

        if self.preview_filter_tags.is_some() || !self.show_intersection {
            self.filter_bar.reset_to_base();
            cx.notify();
            return;
        }

        let active_standard_tags: Vec<MacroTag> = self
            .current_filter_tags
            .iter()
            .filter(|t| matches!(t, MacroTag::Standard(_)))
            .cloned()
            .collect();

        let mts = MtagService::new()
            .tags(config_filters)
            .tags(&active_standard_tags);
        let query = match mts.build() {
            Ok(q) => q,
            Err(err) => {
                tracing::error!("Failed to query secondary stats: {err:#}");
                return;
            }
        };

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let expected_tags = self.current_filter_tags.clone();

        cx.spawn(async move |this, cx| {
            let stats_res = cx
                .background_spawn(async move { rt.block_on(query.query_stats(&db)) })
                .await;

            this.update(cx, |this, cx| {
                if this.current_filter_tags != expected_tags || !this.show_intersection {
                    return;
                }
                if let Ok(stats) = stats_res {
                    this.filter_bar
                        .update_intersect_stats(stats.tags_summary, stats.untagged_files);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn reload_files(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let config_filters = if let Some(f) = self.preview_filter_tags.as_ref() {
            f
        } else {
            &self.config.filters
        };

        let filter_tags = if self.preview_filter_tags.is_some() {
            &vec![]
        } else {
            &self.current_filter_tags
        };

        if config_filters.is_empty() && filter_tags.is_empty() {
            self.content_view.update(cx, |view, cx| {
                view.set_files(Vec::new(), cx);
            });
            cx.notify();
            return;
        }

        let (sort_field, is_desc) = if let BarMode::Configuring(ref editor) = self.bar_mode {
            let cfg = editor.read(cx).current_config();
            (cfg.sort_field, cfg.is_desc)
        } else {
            (self.config.sort_field, self.config.is_desc)
        };

        let sort_direction = if is_desc {
            SortDirection::Desc
        } else {
            SortDirection::Asc
        };

        let mts = MtagService::new().tags(&config_filters).tags(&filter_tags);
        cx.spawn(async move |this, cx| {
            let files_res = cx
                .background_spawn(async move {
                    let mut query = mts.build()?;
                    query = query.sort(sort_field, sort_direction);

                    rt.block_on(file::list_files(&db, &query))
                })
                .await;

            this.update(cx, |this, cx| {
                if let Ok(files) = files_res {
                    this.content_view.update(cx, |view, cx| {
                        view.set_files(files, cx);
                    });
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn reload_candidates(&mut self, cx: &mut Context<Self>) {
        self.candidate_tags = cx.global::<AppState>().tags.read(cx).cached_tags.clone();
        cx.notify();
    }

    /// Reconstructs the filter_bar based on active filter rules (preview or persisted)
    pub fn reload_filter_bar(&mut self) {
        let filters = self
            .preview_filter_tags
            .as_ref()
            .unwrap_or(&self.config.filters);

        let base_query = MtagService::new().tags(filters).build().unwrap_or_default();

        self.filter_bar = TagFilterBar::with_excluded_tags(base_query, filters.clone());
    }

    /// refresh file list and tell filter_bar to reload its facet stats
    pub fn reload_all(&mut self, cx: &mut Context<Self>) {
        self.reload_files(cx);
        self.reload_candidates(cx);
        self.reload_filter_bar();
        self.reload_filter_stats(cx);
    }

    pub fn reload_filter_stats(&mut self, cx: &mut Context<Self>) {
        if self.filter_bar.loading() {
            return;
        }

        let config_filters = self
            .preview_filter_tags
            .as_ref()
            .unwrap_or(&self.config.filters);

        if config_filters.is_empty() {
            self.filter_bar.clear_stats();
            cx.notify();
            return;
        }

        self.filter_bar.set_loading(true);
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let query = self.filter_bar.base_query().clone();

        cx.spawn(async move |this, cx| {
            let stats_res = cx
                .background_spawn(async move { rt.block_on(query.query_stats(&db)) })
                .await;

            this.update(cx, |this, cx| {
                this.filter_bar.set_loading(false);
                if let Ok(stats) = stats_res {
                    this.filter_bar.set_stats(
                        stats.tags_summary,
                        stats.untagged_files,
                        stats.total_files,
                    );
                    if !this.current_filter_tags.is_empty() {
                        this.filter_stats(cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn start_configuring(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_selection(cx);

        let tag_store = cx.global::<AppState>().tags.clone();
        let collection_type = self.collection_type;
        let icon_str = self.icon.clone().unwrap_or_else(|| IconName::Folder.path());
        let editor = cx.new(|cx| {
            CollectionConfigEditor::new(
                self.title.to_string(),
                icon_str,
                self.config.clone(),
                collection_type,
                window,
                cx,
            )
            .tag_store(tag_store, cx)
        });

        cx.subscribe(
            &editor,
            |this, editor, event: &ConfigEditorEvent, cx| match event {
                ConfigEditorEvent::DraftChanged => {
                    let draft_config = editor.read(cx).current_config().clone();
                    this.preview_filter_tags = Some(draft_config.filters.clone());
                    this.reload_filter_bar();
                    this.content_view.update(cx, |view, cx| {
                        view.set_view_mode(draft_config.view, cx);
                        view.set_grouping(draft_config.sort_field, draft_config.group_mode, cx);
                    });
                    this.reload_files(cx);
                    this.reload_filter_stats(cx);
                }
                ConfigEditorEvent::CancelRequested => {
                    this.cancel_configuration(cx);
                }
                ConfigEditorEvent::SaveRequested => {
                    let name = editor.read(cx).current_name(cx);
                    let icon = editor.read(cx).current_icon().to_string();
                    let config = editor.read(cx).current_config().clone();
                    let collection_type = editor.read(cx).current_type();
                    this.apply_config(&name, &icon, &config, collection_type, None, cx);
                }
                ConfigEditorEvent::DeleteRequested => {
                    this.delete_current_collection(cx);
                }
                ConfigEditorEvent::ExpandToggled => {
                    cx.notify();
                }
            },
        )
        .detach();

        self.bar_mode = BarMode::Configuring(editor);
        self.leave_filter_bar(cx);
        cx.notify();
    }

    pub fn delete_current_collection(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.collection_id else {
            self.bar_mode = BarMode::Normal;
            cx.notify();
            return;
        };

        let views = cx.global::<AppState>().views.clone();
        let collections = cx.global::<AppState>().collections.clone();

        views.update(cx, |v, cx| {
            v.delete_view(id, cx);
        });
        collections.update(cx, |c, cx| {
            c.delete_collections(&[id], cx);
        });

        self.bar_mode = BarMode::Normal;
        self.preview_filter_tags = None;
        cx.notify();

        let is_view = self.collection_type == CollectionType::View;
        let msg = if is_view {
            t!("generic_view.view_deleted")
        } else {
            t!("generic_view.collection_deleted")
        };
        let on_back = self.on_back.clone();
        if let Some(handle) = cx.windows().first().copied() {
            cx.update_window(handle, move |_view, window, cx| {
                window.push_notification(Notification::new().message(msg), cx);
                if let Some(on_back) = on_back {
                    on_back(window, cx);
                }
            })
            .ok();
        }
    }

    pub fn apply_config(
        &mut self,
        new_name: &str,
        new_icon: &str,
        new_config: &CollectionConfig,
        new_type: CollectionType,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let new_name = if new_name.trim().is_empty() {
            self.title.to_string()
        } else {
            new_name.trim().to_string()
        };
        self.title = new_name.clone().into();
        self.icon = Some(new_icon.to_string().into());
        self.config = new_config.clone();
        self.bar_mode = BarMode::Normal;
        self.preview_filter_tags = None;
        self.reload_filter_bar();

        self.content_view.update(cx, |view, cx| {
            view.set_view_mode(self.config.view, cx);
            view.set_grouping(self.config.sort_field, self.config.group_mode, cx);
            view.set_context_states(self.config.context_states.clone());
        });
        if let Some(id) = self.collection_id {
            let views = cx.global::<AppState>().views.clone();
            let collections = cx.global::<AppState>().collections.clone();
            let db = cx.global::<AppState>().db.clone();
            let rt = cx.global::<AppState>().rt.clone();
            let type_changed = self.collection_type != new_type;
            self.collection_type = new_type;

            views.update(cx, |s, cx| {
                s.update_view(id, &new_name, new_icon, &self.config, cx);
            });
            collections.update(cx, |s, cx| {
                s.update_collection(id, &new_name, new_icon, &self.config, cx);
            });

            if type_changed {
                if new_type == CollectionType::Folder {
                    views.update(cx, |v, cx| {
                        v.views.retain(|item| item.id != id);
                        cx.notify();
                    });
                } else {
                    collections.update(cx, |c, cx| {
                        c.collections.retain(|item| item.id != id);
                        cx.notify();
                    });
                }

                cx.spawn(async move |_this, cx| {
                    let _ = rt.block_on(collection_store::update_type(&db, id, new_type));
                    cx.update(|cx| {
                        views.update(cx, |v, cx| v.reload(cx));
                        collections.update(cx, |c, cx| c.reload(cx));
                    });
                })
                .detach();
            } else {
                self.reload_all(cx);
            }
        }

        if let Some(window) = window {
            window.push_notification(Notification::new().message(t!("generic_view.config_saved")), cx);
        }
        cx.notify();
    }

    pub fn cancel_configuration(&mut self, cx: &mut Context<Self>) {
        self.bar_mode = BarMode::Normal;
        self.preview_filter_tags = None;
        self.reload_filter_bar();
        self.content_view.update(cx, |view, cx| {
            view.set_view_mode(self.config.view, cx);
            view.set_grouping(self.config.sort_field, self.config.group_mode, cx);
        });
        self.reload_all(cx);
        cx.notify();
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.content_view.update(cx, |view, cx| {
            view.clear_selection(cx);
        });
        if !matches!(self.bar_mode, BarMode::Configuring(_)) {
            self.bar_mode = BarMode::Normal;
        }
        cx.notify();
    }

    pub fn sync_batch_editor(&mut self, cx: &mut Context<Self>) {
        if matches!(self.bar_mode, BarMode::Configuring(_)) {
            return;
        }

        let selected_items = self.content_view.read(cx).selected_items();

        if selected_items.is_empty() {
            self.bar_mode = BarMode::Normal;
            cx.notify();
            return;
        }

        self.bar_mode = BarMode::BatchEditing;
        self.leave_filter_bar(cx);

        let mut candidate_tags: Vec<MacroTag> = Vec::new();

        if self.config.include_view_tags {
            for s in self.filter_bar.tags_summary().take(8) {
                candidate_tags.push(MacroTag::standard(s.tag_id, s.name.clone()));
            }
        }

        for tag in &self.candidate_tags {
            if !candidate_tags.iter().any(|t| t == tag) {
                candidate_tags.push(tag.clone());
            }
        }

        let pinned_tags = self.config.pinned_tags.clone();

        self.batch_editor.update(cx, |editor, cx| {
            editor.set_selection(&selected_items, &candidate_tags, &pinned_tags, cx);
        });

        cx.notify();
    }

    fn commit_tag_changes(
        &mut self,
        ensure: Vec<MacroTag>,
        remove: Vec<MacroTag>,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let file_ids: Vec<Uuid> = self.content_view.read(cx).selected_ids();
        self.clear_selection(cx);

        cx.spawn(async |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(async {
                        MtagService::apply_changes(&db, &file_ids, &ensure, &remove).await?;
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await;

            this.update(cx, |_, cx| {
                if let Err(e) = res {
                    tracing::error!("Failed to apply tags in batch: {:#}", e);
                }
                let file = cx.global::<AppState>().file.clone();
                let tags = cx.global::<AppState>().tags.clone();
                file.update(cx, |_, cx| cx.notify());
                tags.update(cx, |s, cx| s.reload(cx));
            })
            .ok();
        })
        .detach();
    }
}

impl Render for GenericView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let configuring_editor = match &self.bar_mode {
            BarMode::Configuring(editor) => Some(editor.clone()),
            _ => None,
        };

        let secondary_bar = match &self.bar_mode {
            BarMode::Normal => {
                if self.config.is_hide_tagbar {
                    None
                } else {
                    Some(self.filter_bar.render(
                        cx.processor(|this, event, _window, cx| {
                            match event {
                                TagFilterEvent::All => this.filter_bar.select_all(),
                                TagFilterEvent::Untagged => this.filter_bar.toggle_untagged(),
                                TagFilterEvent::Tag { tag, is_multi } => {
                                    this.filter_bar.toggle_tag(&tag, is_multi)
                                }
                            }
                            let current_tags = this.filter_bar.current_tags();
                            this.apply_filter(current_tags, cx);
                        }),
                        cx.processor(|this, hovered, window, cx| {
                            this.handle_filter_bar_pointer(hovered, window, cx);
                        }),
                    ))
                }
            }
            BarMode::BatchEditing => Some(self.batch_editor.clone().into_any_element()),
            BarMode::Configuring(_) => None,
        };

        let edit_action = if let Some(editor) = configuring_editor.as_ref() {
            let editor = editor.clone();
            Button::new("config-save")
                .primary()
                .icon(IconName::Check)
                .on_click(cx.listener({
                    let editor = editor.clone();
                    move |this, _, window, cx| {
                        cx.stop_propagation();
                        let name = editor.read(cx).current_name(cx);
                        let icon = editor.read(cx).current_icon().to_string();
                        let config = editor.read(cx).current_config().clone();
                        let collection_type = editor.read(cx).current_type();
                        this.apply_config(&name, &icon, &config, collection_type, Some(window), cx);
                    }
                }))
        } else {
            Button::new("config-toggle")
                .icon(IconName::SlidersHorizontal)
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.start_configuring(window, cx);
                }))
        };

        let mut scaffold = ViewScaffold::new("generic-view-scaffold", self.title.clone())
            .actions(vec![
                Button::new("refresh")
                    .icon(IconName::RefreshCw)
                    .on_click(cx.listener(|this, _, _, cx| this.reload_all(cx))),
                Button::new("view-toggle")
                    .icon(if matches!(self.config.view, ViewOption::List) {
                        IconName::LayoutGrid
                    } else {
                        IconName::List
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_view(cx);
                    })),
                edit_action,
            ])
            .on_click(cx.listener(|this, _, window, cx| {
                this.content_view.update(cx, |view, cx| {
                    if view.is_renaming() {
                        view.commit_rename(window, cx);
                    }
                });
                this.clear_selection(cx);
            }))
            .on_sub_click(cx.listener(|this, _, window, cx| {
                this.content_view.update(cx, |view, cx| {
                    if view.is_renaming() {
                        view.commit_rename(window, cx);
                    }
                });
                this.clear_selection(cx);
            }))
            .when(configuring_editor.is_some(), |this| {
                this.rounded_t_3xl().shadow(vec![
                    BoxShadow::new(px(0.), px(0.), hsla(0., 0., 0., 0.2))
                        .blur_radius(px(6.))
                        .spread_radius(px(0.)),
                ])
            })
            .child(
                div()
                    .id("explorer-content")
                    .size_full()
                    .pl_3()
                    .overflow_hidden()
                    .child(self.content_view.clone()),
            );

        if let Some(on_back) = self.on_back.clone() {
            scaffold = scaffold.on_back(move |w, cx| on_back(w, cx));
        }

        if let Some(bar) = secondary_bar {
            scaffold = scaffold.sub_bar(bar);
        }

        scaffold = scaffold.on_drop(cx.listener(|this, e: &ExternalPaths, w, cx| {
            this.handle_drag(w, e.paths().to_vec(), cx);
        }));
        scaffold = scaffold.on_dragging(cx.listener(
            |this, _: &DragMoveEvent<ExternalPaths>, window, cx| this.sync_drop_move(window, cx),
        ));
        if self.config.enable_drag && !self.working {
            scaffold = scaffold.drop_label(if self.drop_move {
                t!("generic_view.drop_move", title = self.title)
            } else {
                t!("generic_view.drop_import", title = self.title)
            });
        }

        div()
            .id("explorer-main")
            .on_modifiers_changed(cx.listener(|this, _: &ModifiersChangedEvent, window, cx| {
                this.sync_intersection(window, cx);
            }))
            .bg(cx.theme().muted)
            .overflow_hidden()
            .v_flex()
            .size_full()
            .gap_3()
            .when_some(configuring_editor.as_ref(), |this, editor| {
                let expanded = editor.read(cx).is_expanded();
                let toggled = editor.read(cx).expand_toggled();
                let (from, to) = if expanded { (0.5, 0.8) } else { (0.8, 0.5) };
                let animation_id: ElementId = if toggled {
                    ("config-editor-expand", expanded as usize).into()
                } else {
                    "config-editor-slide".into()
                };
                let panel_height = self.editor_panel_height.clone();
                let opened_cap = self.main_height.get() * 0.5;

                this.child(
                    div()
                        .w_full()
                        .v_flex()
                        .max_h(relative(to))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .with_animation(
                            animation_id,
                            Animation::new(Duration::from_millis(300))
                                .with_easing(ease_out_quint()),
                            {
                                let panel_height = panel_height.clone();
                                move |this, delta| {
                                    if delta >= 1.0 {
                                        this.opacity(1.0)
                                    } else if toggled {
                                        this.max_h(relative(from + (to - from) * delta))
                                    } else {
                                        this.max_h(panel_height.get() * delta).opacity(delta)
                                    }
                                }
                            },
                        )
                        .child(
                            div()
                                .v_flex()
                                .min_h_0()
                                .child(editor.clone().into_any_element())
                                .when(!toggled, |this| {
                                    this.flex_none().max_h(opened_cap).on_prepaint(
                                        move |bounds, _, _| {
                                            panel_height.set(bounds.size.height);
                                        },
                                    )
                                }),
                        ),
                )
            })
            .child(scaffold)
            .on_prepaint({
                let main_height = self.main_height.clone();
                move |bounds, _, _| main_height.set(bounds.size.height)
            })
    }
}
