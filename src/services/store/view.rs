use gpui_kit::*;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::AppState;
use crate::database::entities::collection;
use crate::database::storage::collection as collection_store;
use crate::services::collection::CollectionConfig;

pub struct ViewStore {
    pub views: Vec<collection::Model>,
}

impl ViewStore {
    pub fn new(db: &DatabaseConnection, rt: &tokio::runtime::Runtime) -> Self {
        let views = rt
            .block_on(collection_store::list_by_type(
                db,
                collection::CollectionType::View,
            ))
            .unwrap_or_default();

        Self { views }
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
                    this.views = db_views;
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
        let Some(view) = self.views.iter_mut().find(|v| v.id == id) else {
            return;
        };
        view.name = new_name.to_string();
        view.config = serde_json::to_value(new_config).unwrap_or_default();
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
        let Some(view) = self.views.iter_mut().find(|v| v.id == id) else {
            return;
        };
        view.config = serde_json::to_value(new_config).unwrap_or_default();
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let config_val = serde_json::to_value(new_config).unwrap_or_default();

        cx.spawn(async move |_this, _cx| {
            let _ = rt.block_on(collection_store::update_config(&db, id, config_val));
        })
        .detach();
    }
}
