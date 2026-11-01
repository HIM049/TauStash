use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use crate::database::entities::file_thumbnail;
use crate::database::{
    entities::{FileState, ReplicaState, file, file_replica, file_tag, tag},
    utils,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    prelude::DateTimeUtc, sea_query::Expr,
};
use uuid::Uuid;

/// Raw record representation when querying files with their primary replica
#[derive(Debug, Clone)]
pub struct DbFileRecord {
    pub file_id: Uuid,
    pub file_name: String,
    pub path: String,
    pub hash: String,
    pub size: i64,
    pub mime: String,
    pub ext: Option<String>,
    pub state: FileState,
    pub thumbnail: Option<PathBuf>,
    pub content_created_at: Option<DateTimeUtc>,
    pub content_modified_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

/// Parameters for creating or updating a physical replica
#[derive(Debug, Clone)]
pub struct UpsertReplicaParams<'a> {
    pub file_id: Uuid,
    pub file_name: &'a str,
    pub device_id: &'a str,
    pub uri: &'a str,
    pub state: ReplicaState,
    pub ctime: Option<DateTimeUtc>,
    pub utime: Option<DateTimeUtc>,
    pub file_size: Option<i64>,
}

/// Find a file record by its BLAKE3 hash
pub async fn find_file_by_hash(
    db: &DatabaseConnection,
    hash: &str,
) -> anyhow::Result<Option<file::Model>> {
    let res = file::Entity::find()
        .filter(file::Column::Hash.eq(hash))
        .one(db)
        .await?;
    Ok(res)
}

/// Content-intrinsic time aggregate over a set of replicas
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContentTimes {
    pub created_at: Option<DateTimeUtc>,
    pub modified_at: Option<DateTimeUtc>,
}

/// Parameters for creating a file record
#[derive(Debug, Clone)]
pub struct CreateFileParams<'a> {
    pub id: Uuid,
    pub file_name: &'a str,
    pub hash: &'a str,
    pub size: i64,
    pub mime: Option<String>,
    pub ext: Option<String>,
    pub state: FileState,
    /// Seeded from the first replica; the maintenance job keeps it current
    pub content_times: ContentTimes,
    /// shared time for batch
    pub imported_at: DateTimeUtc,
}

/// Insert a new file record
pub async fn create_file(
    db: &DatabaseConnection,
    params: CreateFileParams<'_>,
) -> anyhow::Result<file::Model> {
    let CreateFileParams {
        id,
        file_name,
        hash,
        size,
        mime,
        ext,
        state,
        content_times,
        imported_at,
    } = params;

    let new_file = file::ActiveModel {
        id: Set(id),
        file_name: Set(file_name.to_string()),
        hash: Set(hash.to_string()),
        size: Set(size),
        mime: Set(mime),
        ext: Set(ext),
        media_meta: Set(None),
        state: Set(state),
        content_created_at: Set(content_times.created_at),
        content_modified_at: Set(content_times.modified_at),
        exif_taken_at: Set(None),
        created_at: Set(imported_at),
        updated_at: Set(Utc::now()),
        deleted_at: Set(None),
    };
    let inserted = new_file.insert(db).await?;
    Ok(inserted)
}

/// Find a replica by device_id and URI
pub async fn find_replica(
    db: &DatabaseConnection,
    device_id: &str,
    uri: &str,
) -> anyhow::Result<Option<file_replica::Model>> {
    let res = file_replica::Entity::find()
        .filter(file_replica::Column::DeviceId.eq(device_id))
        .filter(file_replica::Column::Uri.eq(uri))
        .one(db)
        .await?;
    Ok(res)
}

/// Insert or update a replica record
pub async fn upsert_replica(
    db: &DatabaseConnection,
    params: UpsertReplicaParams<'_>,
) -> anyhow::Result<file_replica::Model> {
    let path = params.uri.to_string();
    let uri = utils::standardize_path(path);
    let existing = find_replica(db, params.device_id, &uri).await?;

    if let Some(replica) = existing {
        let mut active: file_replica::ActiveModel = replica.into();
        active.file_id = Set(params.file_id);
        active.file_name = Set(params.file_name.to_string());
        active.state = Set(params.state);
        active.ctime = Set(params.ctime);
        active.utime = Set(params.utime);
        active.file_size = Set(params.file_size);
        active.deleted_at = Set(None);
        let updated = active.update(db).await?;
        Ok(updated)
    } else {
        let replica = file_replica::ActiveModel {
            file_id: Set(params.file_id),
            file_name: Set(params.file_name.to_string()),
            device_id: Set(params.device_id.to_string()),
            uri: Set(uri),
            state: Set(params.state),
            ctime: Set(params.ctime),
            utime: Set(params.utime),
            file_size: Set(params.file_size),
            deleted_at: Set(None),
            ..Default::default()
        };
        let inserted = replica.insert(db).await?;
        Ok(inserted)
    }
}

/// Preload all replicas located under a given directory root
pub async fn find_replicas_by_prefix(
    db: &DatabaseConnection,
    device_id: &str,
    uri_prefix: &str,
) -> anyhow::Result<Vec<file_replica::Model>> {
    let anchored = if uri_prefix.is_empty() {
        uri_prefix.to_string()
    } else {
        Path::new(uri_prefix)
            .join("")
            .to_string_lossy()
            .into_owned()
    };

    let replicas = file_replica::Entity::find()
        .filter(file_replica::Column::DeviceId.eq(device_id))
        .filter(file_replica::Column::Uri.starts_with(&anchored))
        .all(db)
        .await?;
    Ok(replicas)
}

/// Update replica state to Missing
pub async fn mark_replica_missing(
    db: &DatabaseConnection,
    replica: file_replica::Model,
) -> anyhow::Result<()> {
    let mut active: file_replica::ActiveModel = replica.into();
    active.state = Set(ReplicaState::Missing);
    active.update(db).await?;
    Ok(())
}

pub async fn update_state(
    db: &DatabaseConnection,
    file_ids: &[Uuid],
    state: FileState,
) -> anyhow::Result<()> {
    if file_ids.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    let mut update = file::Entity::update_many()
        .col_expr(file::Column::State, Expr::value(state))
        .col_expr(file::Column::UpdatedAt, Expr::value(now))
        .filter(file::Column::Id.is_in(file_ids.iter().copied()));

    update = if state == FileState::Deleted {
        update
            // Skip files already deleted
            .filter(file::Column::State.ne(FileState::Deleted))
            .col_expr(file::Column::DeletedAt, Expr::value(now))
    } else {
        update.col_expr(file::Column::DeletedAt, Expr::value(None::<DateTimeUtc>))
    };
    update.exec(db).await?;

    Ok(())
}

/// Reset target `state` on the files filter by id and current state, returning them to pending.
pub async fn clear_state(
    db: &DatabaseConnection,
    file_ids: &[Uuid],
    state: FileState,
) -> anyhow::Result<()> {
    if file_ids.is_empty() {
        return Ok(());
    }

    file::Entity::update_many()
        .filter(file::Column::Id.is_in(file_ids.iter().copied()))
        .filter(file::Column::State.eq(state))
        .col_expr(file::Column::State, Expr::value(FileState::Pending))
        .col_expr(file::Column::UpdatedAt, Expr::value(Utc::now()))
        .col_expr(file::Column::DeletedAt, Expr::value(None::<DateTimeUtc>))
        .exec(db)
        .await?;

    Ok(())
}

pub async fn rename_file(
    db: &DatabaseConnection,
    file_id: Uuid,
    new_name: &str,
) -> anyhow::Result<()> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        anyhow::bail!("filename cannot be empty");
    }
    if new_name
        .chars()
        .any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        anyhow::bail!("illegal filename");
    }

    let now = Utc::now();
    file::Entity::update_many()
        .filter(file::Column::Id.eq(file_id))
        .col_expr(file::Column::FileName, Expr::value(new_name.to_string()))
        .col_expr(file::Column::UpdatedAt, Expr::value(now))
        .exec(db)
        .await?;

    Ok(())
}

/// Retrieve all tag names for multiple files
pub async fn get_files_tags_batch(
    db: &DatabaseConnection,
    file_ids: &[Uuid],
) -> anyhow::Result<HashMap<Uuid, Vec<tag::Model>>> {
    if file_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let links = file_tag::Entity::find()
        .filter(file_tag::Column::FileId.is_in(file_ids.to_vec()))
        .all(db)
        .await?;

    if links.is_empty() {
        return Ok(HashMap::new());
    }

    let tag_ids: Vec<Uuid> = links.iter().map(|l| l.tag_id).collect();
    let tags = tag::Entity::find()
        .filter(tag::Column::Id.is_in(tag_ids))
        .all(db)
        .await?;

    let tag_map: HashMap<_, _> = tags.into_iter().map(|t| (t.id, t)).collect();

    let mut result: HashMap<Uuid, Vec<tag::Model>> = HashMap::new();
    for link in links {
        if let Some(t) = tag_map.get(&link.tag_id) {
            result.entry(link.file_id).or_default().push(t.clone());
        }
    }

    Ok(result)
}

pub async fn upsert_thumbnail(
    db: &DatabaseConnection,
    file_id: Uuid,
    thumb_path: PathBuf,
    size: i64,
) -> anyhow::Result<file_thumbnail::Model> {
    let thumb = file_thumbnail::ActiveModel {
        file_id: Set(file_id),
        path: Set(thumb_path.to_string_lossy().to_string()),
        size: Set(size as i64),
        create_at: Set(Utc::now()),
    };

    let result = file_thumbnail::Entity::find()
        .filter(file_thumbnail::Column::FileId.eq(file_id))
        .one(db)
        .await?;

    let model = if result.is_some() {
        file_thumbnail::Entity::update(thumb).exec(db).await?
    } else {
        thumb.insert(db).await?
    };

    Ok(model)
}

// /// Retrieve all tag records for a given file
// pub async fn get_file_tags(
//     db: &DatabaseConnection,
//     file_id: Uuid,
// ) -> anyhow::Result<Vec<tag::Model>> {
//     let links = file_tag::Entity::find()
//         .filter(file_tag::Column::FileId.eq(file_id))
//         .all(db)
//         .await?;

//     if links.is_empty() {
//         return Ok(Vec::new());
//     }

//     let tag_ids: Vec<Uuid> = links.into_iter().map(|l| l.tag_id).collect();
//     let tags = tag::Entity::find()
//         .filter(tag::Column::Id.is_in(tag_ids))
//         .all(db)
//         .await?;

//     Ok(tags)
// }
