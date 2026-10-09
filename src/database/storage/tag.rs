use crate::database::entities::{TagNamespace, file_tag, tag};
use crate::database::storage::file::TagRef;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set,
};
use uuid::Uuid;

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

/// Attach tags to files
pub async fn link(
    db: &DatabaseConnection,
    file_ids: &[Uuid],
    tag_ids: &[Uuid],
) -> anyhow::Result<()> {
    if file_ids.is_empty() || tag_ids.is_empty() {
        return Ok(());
    }

    let links: Vec<file_tag::ActiveModel> = tag_ids
        .iter()
        .flat_map(|tag_id| {
            file_ids.iter().map(move |file_id| file_tag::ActiveModel {
                file_id: Set(*file_id),
                tag_id: Set(*tag_id),
            })
        })
        .collect();

    file_tag::Entity::insert_many(links)
        .on_conflict_do_nothing()
        .exec(db)
        .await?;

    Ok(())
}

/// Detach tags from files
pub async fn unlink(
    db: &DatabaseConnection,
    file_ids: &[Uuid],
    tag_ids: &[Uuid],
) -> anyhow::Result<()> {
    if file_ids.is_empty() || tag_ids.is_empty() {
        return Ok(());
    }

    file_tag::Entity::delete_many()
        .filter(file_tag::Column::FileId.is_in(file_ids.iter().copied()))
        .filter(file_tag::Column::TagId.is_in(tag_ids.iter().copied()))
        .exec(db)
        .await?;

    Ok(())
}

/// Query candidate tags from database based on namespaces and limit.
pub async fn list_candidates(
    db: &DatabaseConnection,
    namespaces: &[TagNamespace],
    limit: usize,
) -> anyhow::Result<Vec<TagRef>> {
    let mut query = tag::Entity::find();
    if !namespaces.is_empty() {
        query = query.filter(tag::Column::Namespace.is_in(namespaces.iter().copied()));
    }
    let tags = query.limit(limit as u64).all(db).await?;

    Ok(tags.into_iter().map(TagRef::from_tag).collect())
}
