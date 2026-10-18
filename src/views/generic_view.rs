use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::components::collection_config_editor::{CollectionConfigEditor, ConfigEditorEvent};
use crate::components::file_content_view::{FileContentEvent, FilePoolView};
use crate::components::tag_bar_editor::TagBatchEditor;
use crate::components::tag_bar_filter::{TagFilterBar, TagFilterEvent};
use crate::components::view_scaffold::ViewScaffold;
use crate::database::entities::collection;
use crate::services::collection::{CollectionConfig, ViewOption};
use crate::services::file;
use crate::services::importer;
use crate::services::macrotag::{FileState, MacroTag};
use crate::services::store::AppStores;
use crate::services::tags_queryer::MtagService;

#[derive(Clone, Default)]
pub enum BarMode {
    #[default]
    Normal,
    BatchEditing,
    Configuring(Entity<CollectionConfigEditor>),
}

pub struct GenericView {
    collection_id: Option<Uuid>,
    config: CollectionConfig,
    title: SharedString,
    icon: Option<SharedString>,
    on_back: Option<Rc<dyn Fn(&mut Window, &mut App) + 'static>>,

    stores: Option<AppStores>,
    content_view: Entity<FilePoolView>,
    filter_bar: TagFilterBar,
    batch_editor: Entity<TagBatchEditor>,
    bar_mode: BarMode,
    current_filter_tags: Vec<MacroTag>,
    preview_filter_tags: Option<Vec<MacroTag>>,
    candidate_tags: Vec<MacroTag>,
    // for any working state
    working: bool,
    enable_drag: bool,
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

        let is_empty_filter = config.filters.is_empty();
        let filter_bar =
            TagFilterBar::with_excluded_tags(base_query, config.filters.clone(), is_empty_filter);

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

        let content_view = cx.new(|cx| FilePoolView::new(config.view, window, cx));
        cx.subscribe(
            &content_view,
            |this, _, event: &FileContentEvent, cx| match event {
                FileContentEvent::SelectionChanged => {
                    this.sync_batch_editor(cx);
                }
                FileContentEvent::DeleteRequested(ids) => {
                    this.delete_files(ids, cx);
                }
                FileContentEvent::RefreshRequested => {
                    if let Some(stores) = &this.stores {
                        stores.file.update(cx, |_, cx| cx.notify());
                    } else {
                        this.reload_all(cx);
                    }
                }
            },
        )
        .detach();

        let mut view = Self {
            collection_id: None,
            config: config.clone(),
            title: title.into(),
            icon: None,
            on_back: None,
            stores: None,

            content_view,
            filter_bar,
            batch_editor,
            bar_mode: BarMode::Normal,
            current_filter_tags: Vec::new(),
            preview_filter_tags: None,
            candidate_tags: Vec::new(),
            working: false,
            enable_drag: false,
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
        let icon = if model.icon.is_empty() {
            Some("folder".into())
        } else {
            Some(model.icon.clone().into())
        };

        let mut view = Self::new(model.name.clone(), config, window, cx);
        view.icon = icon;
        view.collection_id = Some(model.id);
        view
    }

    pub fn on_back(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_back = Some(Rc::new(handler));
        self
    }

    pub fn stores(mut self, stores: AppStores, cx: &mut Context<Self>) -> Self {
        cx.observe(&stores.tags, |this, tag_store, cx| {
            this.candidate_tags = tag_store.read(cx).cached_tags.clone();
            cx.notify();
        })
        .detach();
        cx.observe(&stores.file, |this, _, cx| {
            this.reload_all(cx);
        })
        .detach();
        self.candidate_tags = stores.tags.read(cx).cached_tags.clone();
        self.stores = Some(stores);
        self
    }

    pub fn icon(mut self, icon: impl Into<Option<SharedString>>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn enable_drag(mut self) -> Self {
        self.enable_drag = true;
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
        if !self.enable_drag || self.working || paths.is_empty() {
            return;
        }

        self.working = true;
        window.push_notification(Notification::new().message("正在导入文件"), cx);
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let device_id = cx.global::<AppState>().config.device_id.clone();

        cx.spawn_in(window, async move |this, cx: &mut AsyncWindowContext| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(importer::import_from_paths(&db, &paths, &device_id))
                })
                .await;

            let msg = this
                .update(cx, |this, cx| {
                    this.working = false;
                    let message: SharedString = if let Ok(report) = res {
                        if report.failed.is_empty() {
                            format!("{} 个内容导入成功", report.newly_imported).into()
                        } else {
                            format!("{} 内容径导入失败", report.failed.len()).into()
                        }
                    } else {
                        "导入成功".into()
                    };

                    if let Some(stores) = &this.stores {
                        stores.file.update(cx, |_, cx| cx.notify());
                    } else {
                        this.reload_all(cx);
                    }
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

    pub fn toggle_view(&mut self, cx: &mut Context<Self>) {
        self.config.view = match self.config.view {
            ViewOption::Grid => ViewOption::List,
            ViewOption::List => ViewOption::Grid,
        };
        self.content_view.update(cx, |view, cx| {
            view.set_view_mode(self.config.view, cx);
        });
        self.persist_config(cx);
        cx.notify();
    }

    fn persist_config(&self, cx: &mut Context<Self>) {
        if let Some(id) = self.collection_id {
            let db = cx.global::<AppState>().db.clone();
            let rt = cx.global::<AppState>().rt.clone();
            if let Ok(config_val) = serde_json::to_value(&self.config) {
                cx.spawn(async move |_this, _cx| {
                    let _ = rt.block_on(crate::database::storage::collection::update_config(
                        &db, id, config_val,
                    ));
                })
                .detach();
            }
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
        cx.notify();
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

        let mts = MtagService::new().tags(&config_filters).tags(&filter_tags);
        cx.spawn(async move |this, cx| {
            let files_res = cx
                .background_spawn(async move {
                    let query = mts.build()?;

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
            .unwrap();
        })
        .detach();
    }

    pub fn reload_candidates(&mut self, cx: &mut Context<Self>) {
        if let Some(stores) = &self.stores {
            self.candidate_tags = stores.tags.read(cx).cached_tags.clone();
            cx.notify();
            return;
        }

        if let Some(ref rule) = self.config.candidate_filter {
            let db = cx.global::<AppState>().db.clone();
            let rt = cx.global::<AppState>().rt.clone();
            let namespaces = rule.namespaces.clone();
            let limit = rule.limit;

            cx.spawn(async move |this, cx| {
                let tags_res = cx
                    .background_spawn(async move {
                        rt.block_on(crate::database::storage::tag::list_candidates(
                            &db,
                            &namespaces,
                            limit,
                        ))
                    })
                    .await;

                this.update(cx, |this, cx| {
                    if let Ok(tags) = tags_res {
                        this.candidate_tags = tags
                            .into_iter()
                            .map(|t| MacroTag::Standard(t.into()))
                            .collect();
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        } else {
            self.candidate_tags.clear();
        }
    }

    /// refresh file list and tell filter_bar to reload its facet stats
    pub fn reload_all(&mut self, cx: &mut Context<Self>) {
        self.reload_files(cx);
        self.reload_candidates(cx);
        let is_empty = self.config.filters.is_empty();
        let excluded = self.config.filters.clone();
        self.filter_bar.set_excluded_tags(excluded, is_empty);
        self.reload_filter_stats(cx);
    }

    pub fn reload_filter_stats(&mut self, cx: &mut Context<Self>) {
        if self.filter_bar.loading() {
            return;
        }

        if self.filter_bar.is_empty_filter() {
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
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn start_configuring(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_selection(cx);

        let tag_store = self.stores.as_ref().map(|s| s.tags.clone());
        let editor = cx.new(|cx| {
            let mut ed = CollectionConfigEditor::new(
                self.title.to_string(),
                self.config.clone(),
                window,
                cx,
            );
            if let Some(tag_store) = tag_store {
                ed = ed.tag_store(tag_store, cx);
            }
            ed
        });

        cx.subscribe(
            &editor,
            |this, editor, event: &ConfigEditorEvent, cx| match event {
                ConfigEditorEvent::DraftChanged => {
                    let config = editor.read(cx).current_config().filters.clone();
                    this.preview_filter_tags = Some(config);
                    this.reload_files(cx);
                }
                ConfigEditorEvent::CancelRequested => {
                    this.cancel_configuration(cx);
                }
                ConfigEditorEvent::SaveRequested => {
                    let name = editor.read(cx).current_name(cx);
                    let config = editor.read(cx).current_config().clone();
                    this.apply_saved_configuration(&name, &config, None, cx);
                }
            },
        )
        .detach();

        self.bar_mode = BarMode::Configuring(editor);
        cx.notify();
    }

    pub fn apply_saved_configuration(
        &mut self,
        new_name: &str,
        new_config: &CollectionConfig,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.title = new_name.to_string().into();
        self.config = new_config.clone();
        self.content_view.update(cx, |view, cx| {
            view.set_view_mode(self.config.view, cx);
        });
        self.bar_mode = BarMode::Normal;
        self.preview_filter_tags = None;
        if let Some(stores) = &self.stores {
            if let Some(id) = self.collection_id {
                stores.views.update(cx, |s, cx| {
                    s.update_view(id, new_name, &self.config, cx);
                });
                stores.collections.update(cx, |s, cx| {
                    s.update_collection(id, new_name, &self.config, cx);
                });
            }
        } else if let Some(id) = self.collection_id {
            let is_prefab = id == crate::app::EXPLORER_VIEW_ID || id == crate::app::PENDING_VIEW_ID;
            if !is_prefab {
                let db = cx.global::<AppState>().db.clone();
                let rt = cx.global::<AppState>().rt.clone();
                let name_to_save = new_name.to_string();
                let config_val = serde_json::to_value(&self.config).unwrap_or_default();

                cx.spawn(async move |_this, _cx| {
                    let _ = rt.block_on(crate::database::storage::collection::update_metadata(
                        &db,
                        id,
                        Some(name_to_save),
                        None,
                        None,
                        None,
                        None,
                    ));
                    let _ = rt.block_on(crate::database::storage::collection::update_config(
                        &db, id, config_val,
                    ));
                })
                .detach();
            }
        }

        self.reload_all(cx);
        if let Some(window) = window {
            window.push_notification(Notification::new().message("合集配置已保存"), cx);
        }
        cx.notify();
    }

    pub fn cancel_configuration(&mut self, cx: &mut Context<Self>) {
        self.bar_mode = BarMode::Normal;
        self.preview_filter_tags = None;
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

        let mut candidate_tags: Vec<MacroTag> = Vec::new();

        if self.config.include_view_tags {
            for s in self.filter_bar.tags_summary().iter().take(8) {
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

    pub fn delete_files(&mut self, ids: &[Uuid], cx: &mut Context<Self>) {
        if ids.is_empty() {
            return;
        }

        let ensure = vec![MacroTag::State(FileState::Deleted)];
        let remove = vec![];

        self.commit_tag_changes(ensure, remove, cx);
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

            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    tracing::error!("批量应用标签失败: {:#}", e);
                }
                if let Some(stores) = &this.stores {
                    stores.file.update(cx, |_, cx| cx.notify());
                    stores.tags.update(cx, |s, cx| s.reload(cx));
                } else {
                    this.reload_all(cx);
                }
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
                    Some(
                        self.filter_bar
                            .render(cx.processor(|this, event, _window, cx| {
                                match event {
                                    TagFilterEvent::All => this.filter_bar.select_all(),
                                    TagFilterEvent::Untagged => this.filter_bar.toggle_untagged(),
                                    TagFilterEvent::Tag(tag) => this.filter_bar.toggle_tag(&tag),
                                }
                                let current_tags = this.filter_bar.current_tags();
                                this.apply_filter(current_tags, cx);
                            })),
                    )
                }
            }
            BarMode::BatchEditing => Some(self.batch_editor.clone().into_any_element()),
            BarMode::Configuring(_) => None,
        };

        let mut scaffold =
            ViewScaffold::new("generic-view-scaffold", self.title.clone()).actions(vec![
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
            ]);

        if let Some(on_back) = self.on_back.clone() {
            scaffold = scaffold.on_back(move |w, cx| on_back(w, cx));
        }

        if let Some(editor) = configuring_editor.as_ref() {
            let editor = editor.clone();
            scaffold = scaffold.action(
                Button::new("config-save")
                    .primary()
                    .icon(IconName::Check)
                    .on_click(cx.listener({
                        let editor = editor.clone();
                        move |this, _, window, cx| {
                            let name = editor.read(cx).current_name(cx);
                            let config = editor.read(cx).current_config().clone();
                            this.apply_saved_configuration(&name, &config, Some(window), cx);
                        }
                    })),
            );
        } else {
            scaffold = scaffold.action(
                Button::new("config-toggle")
                    .icon(IconName::SlidersHorizontal)
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.start_configuring(window, cx);
                    })),
            );
        }

        if let Some(bar) = secondary_bar {
            scaffold = scaffold.sub_bar(bar);
        }

        scaffold = scaffold
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

        div()
            .id("explorer-main")
            .bg(cx.theme().muted)
            .overflow_hidden()
            .v_flex()
            .size_full()
            .gap_3()
            .on_click(cx.listener(|this, _, _window, cx| {
                this.clear_selection(cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _window, cx| {
                    this.clear_selection(cx);
                }),
            )
            .on_drop(cx.listener(|this, e: &ExternalPaths, w, cx| {
                this.handle_drag(w, e.paths().to_vec(), cx);
            }))
            .when_some(configuring_editor.as_ref(), |this, editor| {
                this.child(
                    div()
                        // .id("config-editor-main")
                        // .on_click(|_, _, cx| cx.stop_propagation())
                        .w_full()
                        .flex_shrink_0()
                        .overflow_hidden()
                        .with_animation(
                            "config-editor-slide",
                            Animation::new(Duration::from_millis(300))
                                .with_easing(ease_out_quint()),
                            |this, delta| {
                                if delta < 1.0 {
                                    this.max_h(px(delta * 400.0)).opacity(delta)
                                } else {
                                    this.opacity(1.0)
                                }
                            },
                        )
                        .child(editor.clone().into_any_element()),
                )
            })
            .child(scaffold)
    }
}
