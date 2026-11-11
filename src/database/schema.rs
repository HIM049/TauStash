use gpui_kit::assets::IconNamed;
use gpui_kit::component::IconName;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use uuid::Uuid;

use super::entities::*;
use crate::services::collection::{CandidateTagRule, CollectionConfig, ViewOption};
use crate::services::{FileState, MacroTag};
use rust_i18n::t;

pub async fn seed_default_views(db: &DatabaseConnection) -> anyhow::Result<()> {
    let now = chrono::Utc::now();

    let explorer_config = CollectionConfig {
        filters: vec![MacroTag::State(FileState::Managed)],
        enable_drag: true,
        ..Default::default()
    };

    collection::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(t!("default_view.explorer").into_owned()),
        icon: Set(IconName::File.path().to_string()),
        cover: Set(String::new()),
        color: Set(String::new()),
        order: Set(0),
        parent_id: Set(None),
        typ: Set(collection::CollectionType::View),
        config: Set(serde_json::to_value(&explorer_config)?),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
    }
    .insert(db)
    .await?;

    let pending_config = CollectionConfig {
        filters: vec![MacroTag::State(FileState::Pending)],
        pinned_tags: vec![
            MacroTag::State(FileState::Managed),
            MacroTag::State(FileState::Ignored),
            MacroTag::State(FileState::Deleted),
        ],
        include_view_tags: false,
        candidate_filter: Some(CandidateTagRule {
            namespaces: vec![TagNamespace::User],
            limit: 12,
        }),
        view: ViewOption::List,
        ..Default::default()
    };

    collection::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(t!("default_view.review").into_owned()),
        icon: Set(IconName::Check.path().to_string()),
        cover: Set(String::new()),
        color: Set(String::new()),
        order: Set(1),
        parent_id: Set(None),
        typ: Set(collection::CollectionType::View),
        config: Set(serde_json::to_value(&pending_config)?),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
    }
    .insert(db)
    .await?;

    Ok(())
}
