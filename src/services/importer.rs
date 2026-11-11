use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use std::fs::File as StdFile;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use uuid::Uuid;
use walkdir::WalkDir;

pub use super::scanner::BatchReport;
use crate::database::entities::{FileState, ReplicaState};
use crate::database::storage::file::{ContentTimes, CreateFileParams, UpsertReplicaParams};
use crate::database::storage::{self};
use crate::services::macrotag::MacroTag;
use crate::services::mime;
use crate::services::tags_queryer::MtagService;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    pub hash: String,
    pub size: i64,
    pub mime: String,
    pub ext: Option<String>,
    /// Filesystem creation time
    pub ctime: Option<DateTime<Utc>>,
    /// Filesystem modification time
    pub utime: Option<DateTime<Utc>>,
}

pub fn to_utc(time: std::time::SystemTime) -> Option<DateTime<Utc>> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos()))
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

    let ctime = metadata.created().ok().and_then(to_utc);
    let utime = metadata.modified().ok().and_then(to_utc);

    let mut file =
        StdFile::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 65536];

    let head_len = file.read(&mut buffer)?;
    let mime = mime::detect(path, &buffer[..head_len]);
    hasher.update(&buffer[..head_len]);

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
        ctime,
        utime,
    })
}

pub async fn import_file(
    db: &DatabaseConnection,
    path: impl AsRef<Path>,
    device_id: &str,
    initial_tags: &[MacroTag],
    imported_at: DateTime<Utc>,
) -> Result<(Uuid, bool)> {
    let path = path.as_ref();
    if !path.exists() || !path.is_file() {
        anyhow::bail!("Path does not exist or is not a file: {}", path.display());
    }

    let metadata = compute_file_metadata(path)?;
    import_with_metadata(db, path, metadata, device_id, initial_tags, imported_at).await
}

/// Import a file whose metadata was computed beforehand, e.g. before moving it.
pub async fn import_with_metadata(
    db: &DatabaseConnection,
    path: impl AsRef<Path>,
    metadata: FileMetadata,
    device_id: &str,
    initial_tags: &[MacroTag],
    imported_at: DateTime<Utc>,
) -> Result<(Uuid, bool)> {
    let path = path.as_ref();
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let uri = canonical_path.to_string_lossy().to_string();
    let file_name = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let FileMetadata {
        hash,
        size,
        mime,
        ext,
        ctime,
        utime,
    } = metadata;

    // Deduplication check
    let existing_file = storage::file::find_file_by_hash(db, &hash).await?;

    let (file_id, is_duplicate) = if let Some(existing) = existing_file {
        // found duplicated file

        storage::file::upsert_replica(
            db,
            UpsertReplicaParams {
                file_id: existing.id,
                file_name: &file_name,
                device_id,
                uri: &uri,
                state: ReplicaState::Available,
                ctime,
                utime,
                file_size: Some(size),
            },
        )
        .await?;
        (existing.id, true)
    } else {
        // not found duplicated, new file
        // create file record and replica for it

        let new_id = Uuid::now_v7();
        storage::file::create_file(
            db,
            CreateFileParams {
                id: new_id,
                file_name: &file_name,
                hash: &hash,
                size,
                mime: Some(mime.clone()),
                ext,
                state: FileState::Pending,
                content_times: ContentTimes {
                    created_at: ctime,
                    modified_at: utime,
                },
                imported_at,
            },
        )
        .await?;
        storage::file::upsert_replica(
            db,
            UpsertReplicaParams {
                file_id: new_id,
                file_name: &file_name,
                device_id,
                uri: &uri,
                state: ReplicaState::Available,
                ctime,
                utime,
                file_size: Some(size),
            },
        )
        .await?;

        if !initial_tags.is_empty() {
            MtagService::apply_changes(db, &[new_id], initial_tags, &[]).await?;
        }

        (new_id, false)
    };

    Ok((file_id, is_duplicate))
}

/// Import from paths, can handle file or dir paths. Used to handle dragging input.
pub async fn import_from_paths(
    db: &DatabaseConnection,
    paths: &[PathBuf],
    device_id: &str,
    tags: &[MacroTag],
) -> Result<BatchReport> {
    let mut report = BatchReport::default();
    let paths = flat_paths(paths.to_vec());
    let imported_at = Utc::now();

    for path in paths {
        match import_file(db, &path, device_id, &[], imported_at).await {
            Ok((id, is_duplicate)) => {
                if let Err(e) = MtagService::apply_changes(db, &[id], tags, &[]).await {
                    tracing::warn!("failed to tag {}: {}", path.display(), e);
                    report.failed.push(path.to_string_lossy().into_owned());
                    continue;
                }
                report.total_found += 1;
                if is_duplicate {
                    report.skipped_unchanged += 1;
                } else {
                    report.newly_imported += 1;
                }
            }
            Err(e) => {
                tracing::warn!("failed to import {}: {}", path.display(), e);
                report.failed.push(path.to_string_lossy().into_owned());
            }
        }
    }

    Ok(report)
}

/// Expand dirs into the files under them. Symlinked dirs are not entered.
pub fn flat_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut list = vec![];
    for path in paths {
        for entry in WalkDir::new(path) {
            match entry {
                Ok(e) if !e.path().is_dir() => list.push(e.into_path()),
                Ok(_) => {}
                Err(e) => tracing::error!("failed to read dir, skipped: {}", e),
            }
        }
    }
    list
}
