use gpui_kit::assets::IconNamed;
use gpui_kit::component::IconName;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, Schema, Set,
};
use uuid::Uuid;

use super::entities::*;
use crate::services::collection::{CandidateTagRule, CollectionConfig, ViewOption};
use crate::services::{FileState, MacroTag};

/// Initialize all database tables and their indexes derived directly from Entity models
pub async fn create_tables(db: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);

    init_entity(db, &schema, File).await?;
    init_entity(db, &schema, FileReplica).await?;
    init_entity(db, &schema, Tag).await?;
    init_entity(db, &schema, FileTag).await?;
    init_entity(db, &schema, ScanSource).await?;
    init_entity(db, &schema, Collection).await?;

    Ok(())
}

pub async fn seed_default_views(db: &DatabaseConnection) -> anyhow::Result<()> {
    let now = chrono::Utc::now();

    let explorer_config = CollectionConfig {
        filters: vec![MacroTag::State(FileState::Managed)],
        enable_drag: true,
        ..Default::default()
    };

    collection::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set("文件总览".to_string()),
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
        name: Set("审核队列".to_string()),
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

async fn init_entity<E: EntityTrait>(
    db: &DatabaseConnection,
    schema: &Schema,
    entity: E,
) -> Result<(), DbErr> {
    // create table
    let table_stmt = schema
        .create_table_from_entity(entity)
        .if_not_exists()
        .to_owned();
    db.execute(&table_stmt).await?;

    // create indexes
    for mut idx_stmt in schema.create_index_from_entity(entity) {
        idx_stmt.if_not_exists();
        db.execute(&idx_stmt).await?;
    }

    Ok(())
}
