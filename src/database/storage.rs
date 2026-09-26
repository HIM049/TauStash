use anyhow::{Context, Result};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use std::fs::File as StdFile;
use std::io::Read;
use std::path::Path;
use uuid::Uuid;

use super::entities::{ReplicaState, TagNamespace, file, file_replica, file_tag, tag};

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub file_id: Uuid,
    pub path: String,
    pub file_name: String,
    pub hash: String,
    pub size: i64,
    pub mime: String,
    pub is_duplicate: bool,
    pub tags: Vec<String>,
}

/// Compute metadata and BLAKE3 hash for a local file
pub fn compute_file_info(path: &Path) -> Result<(String, i64, Option<String>, Option<String>)> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("Failed to read metadata for: {}", path.display()))?;

    let size = metadata.len() as i64;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());

    let mime = mime_guess::from_path(path).first().map(|m| m.to_string());

    // Stream BLAKE3 hash
    let mut file =
        StdFile::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 65536];

    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    let hash = hasher.finalize().to_hex().to_string();
    Ok((hash, size, mime, ext))
}

/// Import a local file into the database (Create)
pub async fn import_file(
    db: &DatabaseConnection,
    path: impl AsRef<Path>,
    device_id: &str,
) -> Result<FileInfo> {
    let path = path.as_ref();
    if !path.exists() || !path.is_file() {
        anyhow::bail!("Path does not exist or is not a file: {}", path.display());
    }

    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let uri = canonical_path.to_string_lossy().to_string();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let (hash, size, mime, ext) = compute_file_info(path)?;
    let now = Utc::now();

    // Check if the file already exists by hash (CAS)
    let existing_file = file::Entity::find()
        .filter(file::Column::Hash.eq(&hash))
        .one(db)
        .await?;

    let (file_id, is_duplicate) = if let Some(existing) = existing_file {
        let fid = existing.id;

        // Ensure this specific location is registered as a replica
        let existing_replica = file_replica::Entity::find()
            .filter(file_replica::Column::DeviceId.eq(device_id))
            .filter(file_replica::Column::Uri.eq(&uri))
            .one(db)
            .await?;

        if existing_replica.is_none() {
            let replica = file_replica::ActiveModel {
                device_id: Set(device_id.to_string()),
                uri: Set(uri.clone()),
                state: Set(ReplicaState::Local),
                last_verified_at: Set(Some(now)),
                deleted_at: Set(None),
                file_id: Set(fid),
                ..Default::default()
            };
            replica.insert(db).await?;
        }

        (fid, true)
    } else {
        // Create new file record
        let new_id = Uuid::now_v7();
        let new_file = file::ActiveModel {
            id: Set(new_id),
            hash: Set(hash.clone()),
            size: Set(size),
            mime: Set(mime.clone()),
            ext: Set(ext.clone()),
            media_meta: Set(None),
            content_created_at: Set(Some(now)),
            created_at: Set(now),
            updated_at: Set(now),
        };
        new_file.insert(db).await?;

        // Create replica
        let replica = file_replica::ActiveModel {
            device_id: Set(device_id.to_string()),
            uri: Set(uri.clone()),
            state: Set(ReplicaState::Local),
            last_verified_at: Set(Some(now)),
            deleted_at: Set(None),
            file_id: Set(new_id),
            ..Default::default()
        };
        replica.insert(db).await?;

        (new_id, false)
    };

    // Load current tags for this file
    let tags = get_file_tags(db, file_id).await?;

    Ok(FileInfo {
        file_id,
        path: uri,
        file_name,
        hash,
        size,
        mime: mime.unwrap_or_else(|| "application/octet-stream".to_string()),
        is_duplicate,
        tags,
    })
}

/// List all files in the database with their replica path and tags (Read - List)
pub async fn list_files(db: &DatabaseConnection) -> Result<Vec<FileInfo>> {
    let files = file::Entity::find()
        .order_by_desc(file::Column::CreatedAt)
        .all(db)
        .await?;

    let mut result = Vec::with_capacity(files.len());
    for f in files {
        // Find replica uri
        let replica = file_replica::Entity::find()
            .filter(file_replica::Column::FileId.eq(f.id))
            .one(db)
            .await?;

        let uri = replica
            .map(|r| r.uri)
            .unwrap_or_else(|| "(None)".to_string());
        let file_name = Path::new(&uri)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let tags = get_file_tags(db, f.id).await?;

        result.push(FileInfo {
            file_id: f.id,
            path: uri,
            file_name,
            hash: f.hash,
            size: f.size,
            mime: f
                .mime
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            is_duplicate: false,
            tags,
        });
    }

    Ok(result)
}

/// Attach a tag to a file (Update - Add Tag)
pub async fn add_tag(db: &DatabaseConnection, file_id: Uuid, tag_name: &str) -> Result<tag::Model> {
    let clean_name = tag_name.trim();
    if clean_name.is_empty() {
        anyhow::bail!("Tag name cannot be empty");
    }

    let now = Utc::now();

    // Find or create tag
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

    // Check if association already exists
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

/// Detach a tag from a file (Update - Remove Tag)
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

/// Delete a file and its relations from database (Delete)
pub async fn delete_file(db: &DatabaseConnection, file_id: Uuid) -> Result<()> {
    // 1. Delete associated tags
    file_tag::Entity::delete_many()
        .filter(file_tag::Column::FileId.eq(file_id))
        .exec(db)
        .await?;

    // 2. Delete associated replicas
    file_replica::Entity::delete_many()
        .filter(file_replica::Column::FileId.eq(file_id))
        .exec(db)
        .await?;

    // 3. Delete file record
    file::Entity::delete_by_id(file_id).exec(db).await?;

    Ok(())
}

/// Retrieve all tags for a file
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
