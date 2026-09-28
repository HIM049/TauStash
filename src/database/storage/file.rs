use std::collections::HashMap;
use std::path::Path;

use crate::database::{
    entities::{FileState, ReplicaState, TagNamespace, file, file_replica, file_tag, tag},
    utils,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    prelude::DateTimeUtc,
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
    pub hash: &'a str,
    pub size: i64,
    pub mime: Option<String>,
    pub ext: Option<String>,
    pub state: FileState,
    /// Seeded from the first replica; the maintenance job keeps it current
    pub content_times: ContentTimes,
}

/// Insert a new file record
pub async fn create_file(
    db: &DatabaseConnection,
    params: CreateFileParams<'_>,
) -> anyhow::Result<file::Model> {
    let CreateFileParams {
        id,
        hash,
        size,
        mime,
        ext,
        state,
        content_times,
    } = params;

    let now = Utc::now();
    let new_file = file::ActiveModel {
        id: Set(id),
        hash: Set(hash.to_string()),
        size: Set(size),
        mime: Set(mime),
        ext: Set(ext),
        media_meta: Set(None),
        state: Set(state),
        content_created_at: Set(content_times.created_at),
        content_modified_at: Set(content_times.modified_at),
        exif_taken_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
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
    let now = Utc::now();
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
        active.last_verified_at = Set(Some(now));
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
            last_verified_at: Set(Some(now)),
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
    let now = Utc::now();
    let mut active: file_replica::ActiveModel = replica.into();
    active.state = Set(ReplicaState::Missing);
    active.last_verified_at = Set(Some(now));
    active.update(db).await?;
    Ok(())
}

pub async fn set_file_state(
    db: &DatabaseConnection,
    file_id: Uuid,
    state: FileState,
) -> anyhow::Result<()> {
    let Some(file) = file::Entity::find_by_id(file_id).one(db).await? else {
        anyhow::bail!("file not found");
    };
    let mut file: file::ActiveModel = file.into();

    file.state = Set(state);
    file.updated_at = Set(Utc::now());
    file.update(db).await?;

    Ok(())
}

/// Get detail of a single file by id
pub async fn get_file_record_by_id(
    db: &DatabaseConnection,
    file_id: Uuid,
) -> anyhow::Result<Option<DbFileRecord>> {
    let Some(f) = file::Entity::find_by_id(file_id).one(db).await? else {
        return Ok(None);
    };

    let replica = file_replica::Entity::find()
        .filter(file_replica::Column::FileId.eq(f.id))
        .one(db)
        .await?;

    let (path, file_name) = match replica {
        Some(r) => (r.uri, r.file_name),
        None => ("(None)".to_string(), "unknown".to_string()),
    };

    Ok(Some(DbFileRecord {
        file_id: f.id,
        file_name,
        path,
        hash: f.hash,
        size: f.size,
        mime: f
            .mime
            .unwrap_or_else(|| "application/octet-stream".to_string()),
        ext: f.ext,
    }))
}

/// Tag filter options for database queries
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagFilter {
    All,
    Untagged,
    Tag {
        namespace: TagNamespace,
        name: String,
    },
}

/// List file records filtered by tag criteria directly in database
pub async fn list_file_records_by_filter(
    db: &DatabaseConnection,
    filter: &TagFilter,
) -> anyhow::Result<Vec<DbFileRecord>> {
    let target_file_ids: Option<Vec<Uuid>> = match filter {
        TagFilter::All => None,
        TagFilter::Untagged => {
            let tagged_ids: Vec<Uuid> = file_tag::Entity::find()
                .all(db)
                .await?
                .into_iter()
                .map(|ft| ft.file_id)
                .collect();
            let untagged = file::Entity::find()
                .filter(file::Column::Id.is_not_in(tagged_ids))
                .all(db)
                .await?;
            Some(untagged.into_iter().map(|f| f.id).collect())
        }
        TagFilter::Tag { namespace, name } => {
            let matched_tags = tag::Entity::find()
                .filter(tag::Column::Namespace.eq(*namespace))
                .filter(tag::Column::Name.eq(name.as_str()))
                .all(db)
                .await?;

            // if cannot find target tag
            if matched_tags.is_empty() {
                anyhow::bail!("tag not found: namespace={namespace:?}, name={name}");
            }

            let matched_tag_ids: Vec<Uuid> = matched_tags.into_iter().map(|t| t.id).collect();
            let links = file_tag::Entity::find()
                .filter(file_tag::Column::TagId.is_in(matched_tag_ids))
                .all(db)
                .await?;
            Some(links.into_iter().map(|l| l.file_id).collect())
        }
    };

    let files = match target_file_ids {
        Some(ids) => {
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            file::Entity::find()
                .filter(file::Column::Id.is_in(ids))
                .order_by_desc(file::Column::CreatedAt)
                .all(db)
                .await?
        }
        None => {
            // no filters
            file::Entity::find()
                .order_by_desc(file::Column::CreatedAt)
                .all(db)
                .await?
        }
    };

    if files.is_empty() {
        return Ok(Vec::new());
    }

    let file_ids: Vec<Uuid> = files.iter().map(|f| f.id).collect();
    // TODO: read replica by order
    let replicas = file_replica::Entity::find()
        .filter(file_replica::Column::FileId.is_in(file_ids))
        .all(db)
        .await?;

    let replica_map = replicas
        .into_iter()
        .filter(|r| r.state != ReplicaState::Missing)
        .map(|r| (r.file_id, r))
        .collect::<HashMap<_, _>>();

    let result = files
        .into_iter()
        .filter_map(|f| {
            let replica = replica_map.get(&f.id)?;

            Some(DbFileRecord {
                file_id: f.id,
                file_name: replica.file_name.clone(),
                path: replica.uri.clone(),
                hash: f.hash,
                size: f.size,
                mime: f
                    .mime
                    .unwrap_or_else(|| "application/octet-stream".to_string()),
                ext: f.ext,
            })
        })
        .collect();

    Ok(result)
}

/// List all files ordered by create_at descending
pub async fn list_file_records(db: &DatabaseConnection) -> anyhow::Result<Vec<DbFileRecord>> {
    list_file_records_by_filter(db, &TagFilter::All).await
}

/// Retrieve all tag names for multiple files
pub async fn get_files_tags_batch(
    db: &DatabaseConnection,
    file_ids: &[Uuid],
) -> anyhow::Result<HashMap<Uuid, Vec<String>>> {
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

    let tag_map: HashMap<Uuid, String> = tags.into_iter().map(|t| (t.id, t.name)).collect();

    let mut result: HashMap<Uuid, Vec<String>> = HashMap::new();
    for link in links {
        if let Some(tag_name) = tag_map.get(&link.tag_id) {
            result
                .entry(link.file_id)
                .or_default()
                .push(tag_name.clone());
        }
    }

    Ok(result)
}

// /// Delete a file and its associated tags and replicas from database
// pub async fn delete_file(db: &DatabaseConnection, file_id: Uuid) -> anyhow::Result<()> {
//     file_tag::Entity::delete_many()
//         .filter(file_tag::Column::FileId.eq(file_id))
//         .exec(db)
//         .await?;

//     file_replica::Entity::delete_many()
//         .filter(file_replica::Column::FileId.eq(file_id))
//         .exec(db)
//         .await?;

//     file::Entity::delete_by_id(file_id).exec(db).await?;

//     Ok(())
// }

/// Retrieve all tag names for a given file
pub async fn get_file_tags(db: &DatabaseConnection, file_id: Uuid) -> anyhow::Result<Vec<String>> {
    let links = file_tag::Entity::find()
        .filter(file_tag::Column::FileId.eq(file_id))
        .all(db)
        .await?;

    if links.is_empty() {
        return Ok(Vec::new());
    }

    let tag_ids: Vec<Uuid> = links.into_iter().map(|l| l.tag_id).collect();
    let tags = tag::Entity::find()
        .filter(tag::Column::Id.is_in(tag_ids))
        .all(db)
        .await?;

    Ok(tags.into_iter().map(|t| t.name).collect())
}
