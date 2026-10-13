use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::app::{EXPLORER_VIEW_ID, PENDING_VIEW_ID};
use crate::database::entities::{TagNamespace, collection};
use crate::database::storage::collection as collection_store;
use crate::services::collection::{CandidateTagRule, CollectionConfig};
use crate::services::{FileState, MacroTag};

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

pub struct ViewStore {
    pub views: Vec<collection::Model>,
}

impl ViewStore {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut store = Self {
            views: prefab_views(),
        };
        store.reload(cx);
        store
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
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
