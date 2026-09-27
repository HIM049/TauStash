use std::collections::HashMap;

use anyhow::Result;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

use super::entities::{ReplicaState, TagNamespace, file, file_replica, file_tag, tag};

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
    pub mtime: Option<i64>,
    pub file_size: Option<i64>,
}

/// Find a file record by its BLAKE3 hash
pub async fn find_file_by_hash(db: &DatabaseConnection, hash: &str) -> Result<Option<file::Model>> {
    let res = file::Entity::find()
        .filter(file::Column::Hash.eq(hash))
        .one(db)
        .await?;
    Ok(res)
}

/// Insert a new file record
pub async fn insert_file(
    db: &DatabaseConnection,
    id: Uuid,
    hash: &str,
    size: i64,
    mime: Option<String>,
    ext: Option<String>,
) -> Result<file::Model> {
    let now = Utc::now();
    let new_file = file::ActiveModel {
        id: Set(id),
        hash: Set(hash.to_string()),
        size: Set(size),
        mime: Set(mime),
        ext: Set(ext),
        media_meta: Set(None),
        content_created_at: Set(Some(now)),
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
) -> Result<Option<file_replica::Model>> {
    let res = file_replica::Entity::find()
        .filter(file_replica::Column::DeviceId.eq(device_id))
        .filter(file_replica::Column::Uri.eq(uri))
        .one(db)
        .await?;
    Ok(res)
}

/// Insert or update a replica record, including file_name
pub async fn upsert_replica(
    db: &DatabaseConnection,
    params: UpsertReplicaParams<'_>,
) -> Result<file_replica::Model> {
    let now = Utc::now();
    let existing = find_replica(db, params.device_id, params.uri).await?;

    if let Some(replica) = existing {
        let mut active: file_replica::ActiveModel = replica.into();
        active.file_name = Set(params.file_name.to_string());
        active.state = Set(params.state);
        active.mtime = Set(params.mtime);
        active.file_size = Set(params.file_size);
        active.last_verified_at = Set(Some(now));
        active.deleted_at = Set(None);
        let updated = active.update(db).await?;
        Ok(updated)
    } else {
        let replica = file_replica::ActiveModel {
            file_name: Set(params.file_name.to_string()),
            device_id: Set(params.device_id.to_string()),
            uri: Set(params.uri.to_string()),
            state: Set(params.state),
            mtime: Set(params.mtime),
            file_size: Set(params.file_size),
            last_verified_at: Set(Some(now)),
            deleted_at: Set(None),
            file_id: Set(params.file_id),
            ..Default::default()
        };
        let inserted = replica.insert(db).await?;
        Ok(inserted)
    }
}

/// Preload all replicas whose URI starts with a given directory prefix
pub async fn find_replicas_by_prefix(
    db: &DatabaseConnection,
    device_id: &str,
    uri_prefix: &str,
) -> Result<Vec<file_replica::Model>> {
    let replicas = file_replica::Entity::find()
        .filter(file_replica::Column::DeviceId.eq(device_id))
        .filter(file_replica::Column::Uri.starts_with(uri_prefix))
        .all(db)
        .await?;
    Ok(replicas)
}

/// Update replica state to Missing
pub async fn mark_replica_missing(
    db: &DatabaseConnection,
    replica: file_replica::Model,
) -> Result<()> {
    let now = Utc::now();
    let mut active: file_replica::ActiveModel = replica.into();
    active.state = Set(ReplicaState::Missing);
    active.last_verified_at = Set(Some(now));
    active.update(db).await?;
    Ok(())
}

/// Get detail of a single file by id
pub async fn get_file_record_by_id(
    db: &DatabaseConnection,
    file_id: Uuid,
) -> Result<Option<DbFileRecord>> {
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

/// List all files in the database ordered by creation date descending
pub async fn list_file_records(db: &DatabaseConnection) -> Result<Vec<DbFileRecord>> {
    let files = file::Entity::find()
        .order_by_desc(file::Column::CreatedAt)
        .all(db)
        .await?;
    let replicas = file_replica::Entity::find().all(db).await?;
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

/// Retrieve all tag names for multiple files in a single batch query
pub async fn get_files_tags_batch(
    db: &DatabaseConnection,
    file_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<String>>> {
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

/// Attach a tag to a file (create tag if not existing)
pub async fn add_tag(db: &DatabaseConnection, file_id: Uuid, tag_name: &str) -> Result<tag::Model> {
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
pub async fn remove_tag(db: &DatabaseConnection, file_id: Uuid, tag_name: &str) -> Result<()> {
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

/// Delete a file and its associated tags and replicas from database
// pub async fn delete_file(db: &DatabaseConnection, file_id: Uuid) -> Result<()> {
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
pub async fn get_file_tags(db: &DatabaseConnection, file_id: Uuid) -> Result<Vec<String>> {
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
