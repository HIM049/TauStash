use anyhow::{Context, Result};
use sea_orm::DatabaseConnection;
use std::fs::File as StdFile;
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;
use uuid::Uuid;

use crate::database::entities::ReplicaState;
use crate::database::storage::{self, UpsertReplicaParams};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    pub hash: String,
    pub size: i64,
    pub mime: String,
    pub ext: Option<String>,
    pub mtime: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub file_id: Uuid,
    pub path: String,
    pub file_name: String,
    pub hash: String,
    pub size: i64,
    pub mime: String,
    pub ext: Option<String>,
    pub is_duplicate: bool,
    pub tags: Vec<String>,
}

/// Compute file metadata and BLAKE3 hash
pub fn compute_file_metadata(path: &Path) -> Result<FileMetadata> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("Failed to read metadata for: {}", path.display()))?;

    let size = metadata.len() as i64;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());

    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let mime = mime_guess::from_path(path)
        .first()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

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
    Ok(FileMetadata {
        hash,
        size,
        mime,
        ext,
        mtime,
    })
}

/// Import a file into storage and Replica tables.
pub async fn import_file(
    db: &DatabaseConnection,
    path: impl AsRef<Path>,
    device_id: &str,
) -> Result<(Uuid, bool)> {
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

    let FileMetadata {
        hash,
        size,
        mime,
        ext,
        mtime,
    } = compute_file_metadata(path)?;

    // Deduplication check
    let existing_file = storage::find_file_by_hash(db, &hash).await?;

    let (file_id, is_duplicate) = if let Some(existing) = existing_file {
        storage::upsert_replica(
            db,
            UpsertReplicaParams {
                file_id: existing.id,
                file_name: &file_name,
                device_id,
                uri: &uri,
                state: ReplicaState::Local,
                mtime,
                file_size: Some(size),
            },
        )
        .await?;
        (existing.id, true)
    } else {
        let new_id = Uuid::now_v7();
        storage::insert_file(db, new_id, &hash, size, Some(mime), ext).await?;
        storage::upsert_replica(
            db,
            UpsertReplicaParams {
                file_id: new_id,
                file_name: &file_name,
                device_id,
                uri: &uri,
                state: ReplicaState::Local,
                mtime,
                file_size: Some(size),
            },
        )
        .await?;
        (new_id, false)
    };

    Ok((file_id, is_duplicate))
}

/// Fetch full UI detail for a given file_id (including tags)
pub async fn get_file_detail(db: &DatabaseConnection, file_id: Uuid) -> Result<Option<FileInfo>> {
    let Some(record) = storage::get_file_record_by_id(db, file_id).await? else {
        return Ok(None);
    };

    let tags = storage::get_file_tags(db, file_id).await?;

    Ok(Some(FileInfo {
        file_id: record.file_id,
        path: record.path,
        file_name: record.file_name,
        hash: record.hash,
        size: record.size,
        mime: record.mime,
        ext: record.ext,
        is_duplicate: false,
        tags,
    }))
}

/// List files filtered by tag criteria directly from database
pub async fn list_files_filtered(
    db: &DatabaseConnection,
    filter: &storage::TagFilter,
) -> Result<Vec<FileInfo>> {
    let records = storage::list_file_records_by_filter(db, filter).await?;
    let file_ids: Vec<uuid::Uuid> = records.iter().map(|r| r.file_id).collect();
    let mut tags_by_file = storage::get_files_tags_batch(db, &file_ids).await?;

    let result = records
        .into_iter()
        .map(|r| {
            let tags = tags_by_file.remove(&r.file_id).unwrap_or_default();
            FileInfo {
                file_id: r.file_id,
                path: r.path,
                file_name: r.file_name,
                hash: r.hash,
                size: r.size,
                mime: r.mime,
                ext: r.ext,
                is_duplicate: false,
                tags,
            }
        })
        .collect();

    Ok(result)
}

/// List all files and formatted for UI
pub async fn list_all_files(db: &DatabaseConnection) -> Result<Vec<FileInfo>> {
    list_files_filtered(db, &storage::TagFilter::All).await
}
