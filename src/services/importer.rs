use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use std::fs::File as StdFile;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

use super::scanner::scan_directory;
use crate::database::entities::{FileState, ReplicaState};
pub use crate::database::storage::file::TagRef;
use crate::database::storage::file::{ContentTimes, CreateFileParams, UpsertReplicaParams};
use crate::database::storage::query::FileQueryer;
use crate::database::storage::{self};

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
    pub tags: Vec<TagRef>,
    pub state: FileState,
}

fn to_utc(time: std::time::SystemTime) -> Option<DateTime<Utc>> {
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
        ctime,
        utime,
    })
}

pub async fn import_file(
    db: &DatabaseConnection,
    path: impl AsRef<Path>,
    device_id: &str,
    default_state: FileState,
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
        ctime,
        utime,
    } = compute_file_metadata(path)?;

    // generate thumbnail for images on import
    if mime.starts_with("image/") {
        super::thumbnail::ensure_thumbnail(path, &hash);
    }

    // Deduplication check
    let existing_file = storage::file::find_file_by_hash(db, &hash).await?;

    let (file_id, is_duplicate) = if let Some(existing) = existing_file {
        // found duplicated file

        // Upgrade state for pending or ignore file if wanna be managed by default
        if default_state == FileState::Managed
            && matches!(existing.state, FileState::Pending | FileState::Ignore)
        {
            storage::file::set_file_state(db, existing.id, default_state).await?;
        }

        storage::file::upsert_replica(
            db,
            UpsertReplicaParams {
                file_id: existing.id,
                file_name: &file_name,
                device_id,
                uri: &uri,
                state: ReplicaState::Local,
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
                hash: &hash,
                size,
                mime: Some(mime),
                ext,
                state: default_state,
                content_times: ContentTimes {
                    created_at: ctime,
                    modified_at: utime,
                },
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
                state: ReplicaState::Local,
                ctime,
                utime,
                file_size: Some(size),
            },
        )
        .await?;
        (new_id, false)
    };

    Ok((file_id, is_duplicate))
}

/// Outcome of inporting paths the user handed over directly
#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub imported: usize,
    pub failed: Vec<String>,
}

/// Import from paths, can handle file or dir paths. Used to handle dragging input.
pub async fn import_from_paths(
    db: &DatabaseConnection,
    paths: &[PathBuf],
    device_id: &str,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();

    for path in paths {
        let result = if path.is_dir() {
            scan_directory(db, path, device_id, FileState::Managed)
                .await
                .map(|scan| scan.total_found)
        } else {
            import_file(db, path, device_id, FileState::Managed)
                .await
                .map(|_| 1)
        };

        match result {
            Ok(count) => report.imported += count,
            Err(err) => {
                tracing::warn!(path = %path.display(), error = %err, "import failed");
                report.failed.push(path.to_string_lossy().into_owned());
            }
        }
    }

    Ok(report)
}

/// Fetch full UI detail for a given file_id (including tags)
pub async fn get_file_detail(db: &DatabaseConnection, file_id: Uuid) -> Result<Option<FileInfo>> {
    let Some(record) = storage::file::get_file_record_by_id(db, file_id).await? else {
        return Ok(None);
    };

    let tags = storage::file::get_file_tags(db, file_id).await?;

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
        state: record.state,
    }))
}

/// List files filtered by query from database
pub async fn list_files(db: &DatabaseConnection, query: &FileQueryer) -> Result<Vec<FileInfo>> {
    let records = query.execute(db).await?;
    let file_ids: Vec<uuid::Uuid> = records.iter().map(|r| r.file_id).collect();
    let mut tags_by_file = storage::file::get_files_tags_batch(db, &file_ids).await?;

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
                state: r.state,
            }
        })
        .collect();

    Ok(result)
}
