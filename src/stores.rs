use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::app::{EXPLORER_VIEW_ID, PENDING_VIEW_ID};
use crate::database::entities::{TagNamespace, collection};
use crate::database::storage::{collection as collection_store, tag as tag_store};
use crate::services::FileState;
use crate::services::collection::{CandidateTagRule, CollectionConfig};
use crate::services::macrotag::MacroTag;

pub struct ViewStore {
    pub views: Vec<collection::Model>,
    pub loading: bool,
}

impl ViewStore {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut store = Self {
            views: prefab_views(),
            loading: false,
        };
        store.reload(cx);
        store
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn(async move |this, cx| {
            let items = cx
                .background_spawn(async move {
                    rt.block_on(collection_store::list_by_type(
                        &db,
                        collection::CollectionType::View,
                    ))
                })
                .await;

            this.update(cx, |this, cx| {
                this.loading = false;
                if let Ok(db_views) = items {
                    let mut all_views = prefab_views();
                    all_views.extend(db_views);
                    this.views = all_views;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn update_view(
        &mut self,
        id: Uuid,
        new_name: &str,
        new_config: &CollectionConfig,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.views.iter_mut().find(|v| v.id == id) {
            view.name = new_name.to_string();
            view.config = serde_json::to_value(new_config).unwrap_or_default();
        }
        cx.notify();

        if id == EXPLORER_VIEW_ID || id == PENDING_VIEW_ID {
            return;
        }

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let name_to_save = new_name.to_string();
        let config_val = serde_json::to_value(new_config).unwrap_or_default();

        cx.spawn(async move |_this, _cx| {
            let _ = rt.block_on(collection_store::update_metadata(
                &db,
                id,
                Some(name_to_save),
                None,
                None,
                None,
                None,
            ));
            let _ = rt.block_on(collection_store::update_config(&db, id, config_val));
        })
        .detach();
    }
}

pub struct CollectionStore {
    pub collections: Vec<collection::Model>,
    pub loading: bool,
}

impl CollectionStore {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut store = Self {
            collections: Vec::new(),
            loading: false,
        };
        store.reload(cx);
        store
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
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
                if let Ok(db_collections) = items {
                    this.collections = db_collections;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn update_collection(
        &mut self,
        id: Uuid,
        new_name: &str,
        new_config: &CollectionConfig,
        cx: &mut Context<Self>,
    ) {
        if let Some(col) = self.collections.iter_mut().find(|c| c.id == id) {
            col.name = new_name.to_string();
            col.config = serde_json::to_value(new_config).unwrap_or_default();
        }
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let name_to_save = new_name.to_string();
        let config_val = serde_json::to_value(new_config).unwrap_or_default();

        cx.spawn(async move |_this, _cx| {
            let _ = rt.block_on(collection_store::update_metadata(
                &db,
                id,
                Some(name_to_save),
                None,
                None,
                None,
                None,
            ));
            let _ = rt.block_on(collection_store::update_config(&db, id, config_val));
        })
        .detach();
    }

    pub fn create_collection(&mut self, name: &str, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let name_to_create = name.trim().to_string();

        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(collection_store::create(
                        &db,
                        name_to_create,
                        collection::CollectionType::Folder,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    ))
                })
                .await;

            if let Ok(created) = res {
                this.update(cx, |this, cx| {
                    this.collections.push(created);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn delete_collection(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.collections.retain(|c| c.id != id);
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn(async move |_this, _cx| {
            let _ = rt.block_on(collection_store::delete(&db, &[id]));
        })
        .detach();
    }
}

pub struct TagStore {
    pub cached_tags: Vec<MacroTag>,
    pub loading: bool,
}

impl TagStore {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut store = Self {
            cached_tags: Vec::new(),
            loading: false,
        };
        store.reload(cx);
        store
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn(async move |this, cx| {
            let items = cx
                .background_spawn(async move {
                    rt.block_on(tag_store::list_candidates(&db, &[TagNamespace::User], 50))
                })
                .await;

            this.update(cx, |this, cx| {
                this.loading = false;
                if let Ok(tags) = items {
                    this.cached_tags = tags
                        .into_iter()
                        .map(|t| MacroTag::Standard(t.into()))
                        .collect();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn search(&self, query: &str) -> Vec<MacroTag> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.cached_tags.clone();
        }
        self.cached_tags
            .iter()
            .filter(|t| t.display_name().to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    pub fn find_or_create(&mut self, namespace: TagNamespace, name: &str, cx: &mut Context<Self>) {
        self.find_or_create_then(namespace, name, |_tag, _cx| {}, cx);
    }

    pub fn find_or_create_then<F>(
        &mut self,
        namespace: TagNamespace,
        name: &str,
        then: F,
        cx: &mut Context<Self>,
    ) where
        F: FnOnce(MacroTag, &mut App) + 'static,
    {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let name_str = name.trim().to_string();

        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(tag_store::find_or_create(&db, namespace, name_str))
                })
                .await;

            if let Ok(model) = res {
                this.update(cx, |this, cx| {
                    let mtag = MacroTag::Standard(model.into());
                    if !this.cached_tags.contains(&mtag) {
                        this.cached_tags.push(mtag.clone());
                    }
                    cx.notify();
                    then(mtag, cx);
                })
                .ok();
            }
        })
        .detach();
    }
}

#[derive(Clone)]
pub struct AppStores {
    pub views: Entity<ViewStore>,
    pub collections: Entity<CollectionStore>,
    pub tags: Entity<TagStore>,
}

impl AppStores {
    pub fn new(cx: &mut App) -> Self {
        Self {
            views: cx.new(ViewStore::new),
            collections: cx.new(CollectionStore::new),
            tags: cx.new(TagStore::new),
        }
    }
}

fn prefab_views() -> Vec<collection::Model> {
    let now = chrono::Utc::now();
    let explorer_config = CollectionConfig {
        filters: vec![MacroTag::FileState(FileState::Managed)],
        ..Default::default()
    };
    let pending_config = CollectionConfig {
        filters: vec![MacroTag::FileState(FileState::Pending)],
        pinned_tags: vec![
            MacroTag::FileState(FileState::Managed),
            MacroTag::FileState(FileState::Ignore),
            MacroTag::FileState(FileState::Blocked),
        ],
        include_view_tags: false,
        candidate_filter: Some(CandidateTagRule {
            namespaces: vec![TagNamespace::User],
            limit: 12,
        }),
        view: crate::services::collection::ViewOption::List,
        ..Default::default()
    };

    vec![
        collection::Model {
            id: EXPLORER_VIEW_ID,
            name: "文件总览".to_string(),
            icon: "file".to_string(),
            cover: String::new(),
            color: String::new(),
            order: 0,
            parent_id: None,
            typ: collection::CollectionType::View,
            config: serde_json::to_value(explorer_config).unwrap_or_default(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        },
        collection::Model {
            id: PENDING_VIEW_ID,
            name: "审核队列".to_string(),
            icon: "check".to_string(),
            cover: String::new(),
            color: String::new(),
            order: 1,
            parent_id: None,
            typ: collection::CollectionType::View,
            config: serde_json::to_value(pending_config).unwrap_or_default(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        },
    ]
}
