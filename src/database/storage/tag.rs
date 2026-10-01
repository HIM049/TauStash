use crate::database::entities::{TagNamespace, file_tag, tag};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

/// Look up a tag by `(namespace, name)`, the `tag` table's unique key.
pub async fn find_by_name(
    db: &DatabaseConnection,
    namespace: TagNamespace,
    name: &str,
) -> anyhow::Result<Option<tag::Model>> {
    let found = tag::Entity::find()
        .filter(tag::Column::Namespace.eq(namespace))
        .filter(tag::Column::Name.eq(name))
        .one(db)
        .await?;
    Ok(found)
}

/// Create a tag. Callers wanting upsert behaviour should use [`find_or_create`],
/// since the unique index is the only guard against a duplicate row.
pub async fn create(
    db: &DatabaseConnection,
    namespace: TagNamespace,
    name: String,
    color: Option<String>,
) -> anyhow::Result<tag::Model> {
    let now = Utc::now();
    let new_tag = tag::ActiveModel {
        id: Set(Uuid::now_v7()),
        namespace: Set(namespace),
        name: Set(name),
        color: Set(color),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
    };
    Ok(new_tag.insert(db).await?)
}

/// Resolve a tag by name, creating it when this namespace has no such tag.
pub async fn find_or_create(
    db: &DatabaseConnection,
    namespace: TagNamespace,
    name: String,
) -> anyhow::Result<tag::Model> {
    match find_by_name(db, namespace, &name).await? {
        Some(existing) => Ok(existing),
        None => create(db, namespace, name, None).await,
    }
}

/// Attach a tag to many files.
pub async fn link(db: &DatabaseConnection, file_ids: &[Uuid], tag_id: Uuid) -> anyhow::Result<()> {
    if file_ids.is_empty() {
        return Ok(());
    }

    let links = file_ids.iter().map(|file_id| file_tag::ActiveModel {
        file_id: Set(*file_id),
        tag_id: Set(tag_id),
    });

    file_tag::Entity::insert_many(links)
        .on_conflict_do_nothing()
        .exec(db)
        .await?;

    Ok(())
}

/// Detach a tag from many files.
pub async fn unlink(
    db: &DatabaseConnection,
    file_ids: &[Uuid],
    tag_id: Uuid,
) -> anyhow::Result<()> {
    if file_ids.is_empty() {
        return Ok(());
    }

    file_tag::Entity::delete_many()
        .filter(file_tag::Column::FileId.is_in(file_ids.iter().copied()))
        .filter(file_tag::Column::TagId.eq(tag_id))
        .exec(db)
        .await?;

    Ok(())
}
