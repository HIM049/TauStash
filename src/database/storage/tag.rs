use crate::database::entities::{TagNamespace, file_tag, tag};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    QueryFilter, Set, Statement,
};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagSummaryItem {
    pub namespace: TagNamespace,
    pub name: String,
    pub color: Option<String>,
    pub count: usize,
}

/// Full aggregated statistics for the overview bar
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverviewStats {
    pub total_files: usize,
    pub untagged_files: usize,
    pub tags_summary: Vec<TagSummaryItem>,
}

/// tags statistics
pub async fn get_tags_stats(db: &DatabaseConnection) -> anyhow::Result<OverviewStats> {
    let tag_sql = r#"
        SELECT
            t.namespace,
            t.name,
            t.color,
            COUNT(DISTINCT ft.file_id) AS file_count
        FROM tag t
        INNER JOIN file_tag ft ON t.id = ft.tag_id
        INNER JOIN file f ON ft.file_id = f.id
        INNER JOIN file_replica fr ON f.id = fr.file_id
        WHERE fr.state != 'missing'
        GROUP BY t.namespace, t.name, t.color
        ORDER BY file_count DESC, t.name ASC
    "#;
    let tag_rows = db
        .query_all_raw(Statement::from_string(DbBackend::Sqlite, tag_sql))
        .await?;

    let mut tags_summary = Vec::with_capacity(tag_rows.len());
    for row in tag_rows {
        let ns_str: String = row.try_get("", "namespace")?;
        let name: String = row.try_get("", "name")?;
        let color: Option<String> = row.try_get("", "color").ok();
        let count_i64: i64 = row.try_get("", "file_count")?;

        let namespace = match ns_str.as_str() {
            "rule" => TagNamespace::Rule,
            "ai" => TagNamespace::Ai,
            _ => TagNamespace::User,
        };

        tags_summary.push(TagSummaryItem {
            namespace,
            name,
            color,
            count: count_i64 as usize,
        });
    }

    let count_sql = r#"
        SELECT
            (
                SELECT COUNT(DISTINCT f.id)
                FROM file f
                INNER JOIN file_replica fr ON f.id = fr.file_id
                WHERE fr.state != 'missing'
            ) AS total_files,
            (
                SELECT COUNT(DISTINCT f.id)
                FROM file f
                INNER JOIN file_replica fr ON f.id = fr.file_id
                WHERE fr.state != 'missing'
                  AND NOT EXISTS (
                      SELECT 1 FROM file_tag ft WHERE ft.file_id = f.id
                  )
            ) AS untagged_files
    "#;

    let count_row = db
        .query_one_raw(Statement::from_string(DbBackend::Sqlite, count_sql))
        .await?;

    let (total_files, untagged_files) = if let Some(row) = count_row {
        let total: i64 = row.try_get("", "total_files")?;
        let untagged: i64 = row.try_get("", "untagged_files")?;
        (total as usize, untagged as usize)
    } else {
        (0, 0)
    };

    Ok(OverviewStats {
        total_files,
        untagged_files,
        tags_summary,
    })
}
