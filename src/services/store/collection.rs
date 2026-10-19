use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::database::entities::collection;
use crate::database::storage::collection as collection_store;
use crate::services::collection::CollectionConfig;

pub struct CollectionStore {
    pub collections: Vec<collection::Model>,
}

impl CollectionStore {
    pub fn new() -> Self {
        Self {
            collections: Vec::new(),
        }
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
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
        let Some(col) = self.collections.iter_mut().find(|c| c.id == id) else {
            return;
        };
        col.name = new_name.to_string();
        col.config = serde_json::to_value(new_config).unwrap_or_default();
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

    pub fn update_config(
        &mut self,
        id: Uuid,
        new_config: &CollectionConfig,
        cx: &mut Context<Self>,
    ) {
        let Some(col) = self.collections.iter_mut().find(|c| c.id == id) else {
            return;
        };
        col.config = serde_json::to_value(new_config).unwrap_or_default();
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let config_val = serde_json::to_value(new_config).unwrap_or_default();

        cx.spawn(async move |_, _cx| {
            rt.block_on(collection_store::update_config(&db, id, config_val))
                .ok();
        })
        .detach();
    }

    pub fn rename_collection(&mut self, id: Uuid, new_name: &str, cx: &mut Context<Self>) {
        let Some(col) = self.collections.iter_mut().find(|c| c.id == id) else {
            return;
        };
        col.name = new_name.to_string();
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let name_to_save = new_name.to_string();

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
        })
        .detach();
    }

    pub fn create_collection(&mut self, name: &str, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let name_to_create = name.trim().to_string();
        let config = match serde_json::to_value(CollectionConfig::default()) {
            Ok(config) => Some(config),
            Err(err) => {
                tracing::error!("序列化合集配置失败: {err:#}");
                None
            }
        };

        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(collection_store::create(
                        &db,
                        name_to_create,
                        collection::CollectionType::Folder,
                        None,
                        None,
                        config,
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

    pub fn delete_collections(&mut self, ids: &[Uuid], cx: &mut Context<Self>) {
        let id_set: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
        self.collections.retain(|c| !id_set.contains(&c.id));
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let ids_vec = ids.to_vec();

        cx.spawn(async move |_this, _cx| {
            let _ = rt.block_on(collection_store::delete(&db, &ids_vec));
        })
        .detach();
    }
}
