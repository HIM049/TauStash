use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use std::fs::File as StdFile;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

pub use super::scanner::BatchReport;
use crate::database::entities::{FileState, ReplicaState};
use crate::database::storage::file::{ContentTimes, CreateFileParams, UpsertReplicaParams};
use crate::database::storage::{self};
use crate::services::macrotag::MacroTag;
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

pub async fn import_file_with_tags(
    db: &DatabaseConnection,
    path: impl AsRef<Path>,
    device_id: &str,
    initial_tags: &[MacroTag],
) -> Result<(Uuid, bool)> {
    let path = path.as_ref();
    if !path.exists() || !path.is_file() {
        anyhow::bail!("Path does not exist or is not a file: {}", path.display());
    }

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
    } = compute_file_metadata(path)?;

    // generate thumbnail for images on import
    if mime.starts_with("image/") {
        super::thumbnail::ensure_thumbnail(path, &hash);
    }

    let default_state = initial_tags
        .iter()
        .find_map(|t| match t {
            MacroTag::State(s) => Some(*s),
            _ => None,
        })
        .unwrap_or_default();

    // Deduplication check
    let existing_file = storage::file::find_file_by_hash(db, &hash).await?;

    let (file_id, is_duplicate) = if let Some(existing) = existing_file {
        // found duplicated file

        // Upgrade state for pending or ignore file if wanna be managed by default
        if default_state == FileState::Managed
            && matches!(existing.state, FileState::Pending | FileState::Ignored)
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
                file_name: &file_name,
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
) -> Result<BatchReport> {
    let mut report = BatchReport::default();
    let sources = storage::scan_source::list_active_scans(db)
        .await
        .unwrap_or_default();
    let mut rules = super::scanner::build_scan_rules(&sources);

    for path in paths {
        if path.is_dir() {
            let p = path.as_path();
            let canonical_p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
            rules.push(super::scanner::ScanTask {
                path: canonical_p,
                initial_tags: vec![MacroTag::State(FileState::Managed)],
            });
            rules.sort_by_key(|r| r.path.components().count());

            match super::scanner::scan_task(db, path, device_id, &rules).await {
                Ok(rep) => report.merge(rep),
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "import failed");
                    report.failed.push(path.to_string_lossy().into_owned());
                }
            }
        } else {
            let tags = super::scanner::resolve_tags_by_path(&rules, path);
            match import_file_with_tags(db, path, device_id, &tags).await {
                Ok((_id, is_duplicate)) => {
                    report.total_found += 1;
                    if is_duplicate {
                        report.skipped_unchanged += 1;
                    } else {
                        report.newly_imported += 1;
                    }
                }
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "import failed");
                    report.failed.push(path.to_string_lossy().into_owned());
                }
            }
        }
    }

    Ok(report)
}
