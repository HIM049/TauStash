use crate::database::entities::collection::{self, CollectionType};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

/// Create a new collection (folder or view).
pub async fn create(
    db: &DatabaseConnection,
    name: impl Into<String>,
    typ: CollectionType,
    parent_id: Option<Uuid>,
    order: Option<u32>,
    config: Option<serde_json::Value>,
    cover: Option<String>,
    icon: Option<String>,
    color: Option<String>,
) -> anyhow::Result<collection::Model> {
    let now = Utc::now();
    let new_collection = collection::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(name.into()),
        icon: Set(icon.unwrap_or_default()),
        cover: Set(cover.unwrap_or_default()),
        color: Set(color.unwrap_or_default()),
        order: Set(order.unwrap_or(0)),
        parent_id: Set(parent_id),
        typ: Set(typ),
        config: Set(config.unwrap_or_default()),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
    };

    Ok(new_collection.insert(db).await?)
}

pub async fn find_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> anyhow::Result<Option<collection::Model>> {
    let item = collection::Entity::find_by_id(id)
        .filter(collection::Column::DeletedAt.is_null())
        .one(db)
        .await?;
    Ok(item)
}

/// List all active collections ordered by order weight ascending, then creation time.
pub async fn list_all_active(db: &DatabaseConnection) -> anyhow::Result<Vec<collection::Model>> {
    let items = collection::Entity::find()
        .filter(collection::Column::DeletedAt.is_null())
        .order_by_asc(collection::Column::Order)
        .order_by_asc(collection::Column::CreatedAt)
        .all(db)
        .await?;
    Ok(items)
}

/// List active child collections under a specific parent (or root nodes if parent_id is None).
pub async fn list_by_parent(
    db: &DatabaseConnection,
    parent_id: Option<Uuid>,
) -> anyhow::Result<Vec<collection::Model>> {
    let mut query = collection::Entity::find().filter(collection::Column::DeletedAt.is_null());

    query = match parent_id {
        Some(pid) => query.filter(collection::Column::ParentId.eq(pid)),
        None => query.filter(collection::Column::ParentId.is_null()),
    };

    let items = query
        .order_by_asc(collection::Column::Order)
        .order_by_asc(collection::Column::CreatedAt)
        .all(db)
        .await?;
    Ok(items)
}

/// List active collections filtered by collection type (Folder or View).
pub async fn list_by_type(
    db: &DatabaseConnection,
    typ: CollectionType,
) -> anyhow::Result<Vec<collection::Model>> {
    let items = collection::Entity::find()
        .filter(collection::Column::DeletedAt.is_null())
        .filter(collection::Column::Typ.eq(typ))
        .order_by_asc(collection::Column::Order)
        .order_by_asc(collection::Column::CreatedAt)
        .all(db)
        .await?;
    Ok(items)
}

/// Update basic display metadata (name, icon, color, order).
pub async fn update_metadata(
    db: &DatabaseConnection,
    id: Uuid,
    name: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    cover: Option<String>,
    order: Option<u32>,
) -> anyhow::Result<collection::Model> {
    let model = find_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Collection with id '{id}' not found"))?;

    let mut active: collection::ActiveModel = model.into();

    if let Some(n) = name {
        active.name = Set(n);
    }
    if let Some(i) = icon {
        active.icon = Set(i);
    }
    if let Some(c) = color {
        active.color = Set(c);
    }
    if let Some(c) = cover {
        active.cover = Set(c);
    }
    if let Some(o) = order {
        active.order = Set(o);
    }
    active.updated_at = Set(Utc::now());

    Ok(active.update(db).await?)
}

/// Update the display/sorting order of a collection.
pub async fn update_order(
    db: &DatabaseConnection,
    id: Uuid,
    order: u32,
) -> anyhow::Result<collection::Model> {
    update_metadata(db, id, None, None, None, None, Some(order)).await
}

/// Update the configuration settings payload of a collection.
pub async fn update_config(
    db: &DatabaseConnection,
    id: Uuid,
    config: serde_json::Value,
) -> anyhow::Result<collection::Model> {
    let model = find_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Collection with id '{id}' not found"))?;

    let mut active: collection::ActiveModel = model.into();
    active.config = Set(config);
    active.updated_at = Set(Utc::now());

    Ok(active.update(db).await?)
}

/// Set collection parent
pub async fn set_parent(
    db: &DatabaseConnection,
    id: Uuid,
    new_parent_id: Option<Uuid>,
) -> anyhow::Result<collection::Model> {
    if let Some(new_pid) = new_parent_id
        && new_pid == id
    {
        anyhow::bail!("A collection cannot be its own parent");
    }

    let model = find_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Collection with id '{id}' not found"))?;

    let mut active: collection::ActiveModel = model.into();
    active.parent_id = Set(new_parent_id);
    active.updated_at = Set(Utc::now());

    Ok(active.update(db).await?)
}

/// Soft delete collections by IDs
pub async fn delete(db: &DatabaseConnection, ids: &[Uuid]) -> anyhow::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let now = Utc::now();
    collection::Entity::update_many()
        .col_expr(
            collection::Column::DeletedAt,
            sea_orm::sea_query::Expr::value(Some(now)),
        )
        .col_expr(
            collection::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .filter(collection::Column::Id.is_in(ids.iter().copied()))
        .filter(collection::Column::DeletedAt.is_null())
        .exec(db)
        .await?;

    Ok(())
}
