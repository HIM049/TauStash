use crate::database::entities::{TagNamespace, file_tag, tag};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

/// Attach a tag to a file, create tag if not existing
pub async fn add_tag(
    db: &DatabaseConnection,
    file_id: Uuid,
    tag_name: &str,
) -> anyhow::Result<tag::Model> {
    let clean_name = tag_name.trim();
    if clean_name.is_empty() {
        anyhow::bail!("Tag name cannot be empty");
    }

    let now = Utc::now();

    let existing_tag = tag::Entity::find()
        .filter(tag::Column::Namespace.eq(TagNamespace::User))
        .filter(tag::Column::Name.eq(clean_name))
        .one(db)
        .await?;

    let tag_model = if let Some(t) = existing_tag {
        t
    } else {
        let new_tag_id = Uuid::now_v7();
        let new_tag = tag::ActiveModel {
            id: Set(new_tag_id),
            namespace: Set(TagNamespace::User),
            name: Set(clean_name.to_string()),
            color: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
        };
        new_tag.insert(db).await?
    };

    let existing_link = file_tag::Entity::find()
        .filter(file_tag::Column::FileId.eq(file_id))
        .filter(file_tag::Column::TagId.eq(tag_model.id))
        .one(db)
        .await?;

    if existing_link.is_none() {
        let link = file_tag::ActiveModel {
            file_id: Set(file_id),
            tag_id: Set(tag_model.id),
        };
        link.insert(db).await?;
    }

    Ok(tag_model)
}

/// Detach a tag from a file
pub async fn remove_tag(
    db: &DatabaseConnection,
    file_id: Uuid,
    tag_name: &str,
) -> anyhow::Result<()> {
    let clean_name = tag_name.trim();
    let existing_tag = tag::Entity::find()
        .filter(tag::Column::Namespace.eq(TagNamespace::User))
        .filter(tag::Column::Name.eq(clean_name))
        .one(db)
        .await?;

    if let Some(t) = existing_tag {
        file_tag::Entity::delete_many()
            .filter(file_tag::Column::FileId.eq(file_id))
            .filter(file_tag::Column::TagId.eq(t.id))
            .exec(db)
            .await?;
    }

    Ok(())
}
