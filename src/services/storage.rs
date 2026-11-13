use anyhow::Result;
use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use std::fs::{self, File, FileTimes};
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::database::{storage, utils};
use crate::services::importer::{self, BatchReport};
use crate::services::macrotag::MacroTag;
use crate::services::tags_queryer::MtagService;

/// Directory holding one content: `<root>/ab/cd/<hash>`
pub fn content_dir(root: &Path, hash: &str) -> PathBuf {
    root.join(&hash[0..2]).join(&hash[2..4]).join(hash)
}

/// Create the store root if needed and return it in the same form as replica uris.
pub fn resolve_root(root: &Path) -> Result<PathBuf> {
    fs::create_dir_all(root)?;
    Ok(standardized(&root.canonicalize()?))
}

fn standardized(path: &Path) -> PathBuf {
    PathBuf::from(utils::standardize_path(path.to_string_lossy().into_owned()))
}

/// Move a file into the store and import it, cancel if hash different
pub async fn move_into_store(
    db: &DatabaseConnection,
    store_root: &Path,
    src: &Path,
    device_id: &str,
    tags: &[MacroTag],
    imported_at: DateTime<Utc>,
    expected_hash: Option<&str>,
) -> Result<Uuid> {
    if !src.is_file() {
        anyhow::bail!("Path does not exist or is not a file: {}", src.display());
    }

    let root = resolve_root(store_root)?;
    let src = standardized(&src.canonicalize()?);
    if src.starts_with(&root) {
        anyhow::bail!("File is already in the store: {}", src.display());
    }
    let src_uri = src.to_string_lossy().into_owned();
    let Some(file_name) = src.file_name() else {
        anyhow::bail!("File has no name: {}", src.display());
    };

    let metadata = importer::compute_file_metadata(&src)?;
    if let Some(expected) = expected_hash
        && metadata.hash != expected
    {
        anyhow::bail!("File content changed since import: {}", src.display());
    }

    if let Some(existing) = storage::file::find_file_by_hash(db, &metadata.hash).await? {
        let stored = storage::file::find_replicas_by_path(
            db,
            existing.id,
            device_id,
            &root.to_string_lossy(),
        )
        .await?;
        if stored.iter().any(|r| Path::new(&r.uri).is_file()) {
            fs::remove_file(&src)?;
            storage::file::delete_replica_by_uri(db, device_id, &src_uri).await?;
            MtagService::apply_changes(db, &[existing.id], tags, &[]).await?;
            return Ok(existing.id);
        }
    }

    let target = content_dir(&root, &metadata.hash).join(file_name);
    move_file(&src, &target, metadata.size as u64)?;

    let (file_id, _) =
        importer::import_with_metadata(db, &target, metadata, device_id, &[], imported_at).await?;
    MtagService::apply_changes(db, &[file_id], tags, &[]).await?;
    storage::file::delete_replica_by_uri(db, device_id, &src_uri).await?;

    Ok(file_id)
}

/// Copy a file into the store and import the copy, leaving the source untouched.
/// Returns `None` when the content already has a replica in the store.
pub async fn copy_into_store(
    db: &DatabaseConnection,
    store_root: &Path,
    src: &Path,
    device_id: &str,
    expected_hash: Option<&str>,
) -> Result<Option<Uuid>> {
    if !src.is_file() {
        anyhow::bail!("Path does not exist or is not a file: {}", src.display());
    }

    let root = resolve_root(store_root)?;
    let src = standardized(&src.canonicalize()?);
    if src.starts_with(&root) {
        anyhow::bail!("File is already in the store: {}", src.display());
    }
    let Some(file_name) = src.file_name() else {
        anyhow::bail!("File has no name: {}", src.display());
    };

    let metadata = importer::compute_file_metadata(&src)?;
    if let Some(expected) = expected_hash
        && metadata.hash != expected
    {
        anyhow::bail!("File content changed since import: {}", src.display());
    }

    if let Some(existing) = storage::file::find_file_by_hash(db, &metadata.hash).await? {
        let stored = storage::file::find_replicas_by_path(
            db,
            existing.id,
            device_id,
            &root.to_string_lossy(),
        )
        .await?;
        if stored.iter().any(|r| Path::new(&r.uri).is_file()) {
            return Ok(None);
        }
    }

    let target = content_dir(&root, &metadata.hash).join(file_name);
    if !(target.is_file() && fs::metadata(&target)?.len() == metadata.size as u64) {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        copy_verified(&src, &target)?;
    }

    let (file_id, _) =
        importer::import_with_metadata(db, &target, metadata, device_id, &[], Utc::now()).await?;
    Ok(Some(file_id))
}

/// Move files or dirs into the store
pub async fn move_from_paths(
    db: &DatabaseConnection,
    store_root: &Path,
    paths: &[PathBuf],
    device_id: &str,
    tags: &[MacroTag],
) -> Result<BatchReport> {
    let mut report = BatchReport::default();
    let paths = importer::flat_paths(paths.to_vec());
    let imported_at = Utc::now();

    for path in paths {
        report.total_found += 1;
        match move_into_store(db, store_root, &path, device_id, tags, imported_at, None).await {
            Ok(_) => report.newly_imported += 1,
            Err(e) => {
                tracing::warn!("failed to move {} into store: {}", path.display(), e);
                report.failed.push(path.to_string_lossy().into_owned());
            }
        }
    }

    Ok(report)
}

/// Move by rename or cp & rm by whether in same volume
fn move_file(src: &Path, target: &Path, size: u64) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    if target.is_file() && fs::metadata(target)?.len() == size {
        return fs::remove_file(src);
    }

    match fs::rename(src, target) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => copy_then_remove(src, target),
        Err(e) => Err(e),
    }
}

/// Copy, Check, Remove the source.
fn copy_then_remove(src: &Path, target: &Path) -> io::Result<()> {
    copy_verified(src, target)?;
    fs::remove_file(src)
}

/// Copy through a temp file, keep times, check size, then rename into place.
fn copy_verified(src: &Path, target: &Path) -> io::Result<()> {
    let mut tmp_name = target.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp: PathBuf = target.with_file_name(tmp_name);

    let src_meta = fs::metadata(src)?;
    fs::copy(src, &tmp)?;

    let file = File::options().write(true).open(&tmp)?;
    file.set_times(rewrite_time(&src_meta)?)?;
    file.sync_all()?;
    if file.metadata()?.len() != src_meta.len() {
        fs::remove_file(&tmp)?;
        return Err(io::Error::other("size mismatch after copy"));
    }

    fs::rename(&tmp, target)?;

    #[cfg(unix)]
    if let Some(parent) = target.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

/// Update times for copied file
fn rewrite_time(meta: &fs::Metadata) -> io::Result<FileTimes> {
    let times = FileTimes::new()
        .set_accessed(meta.accessed()?)
        .set_modified(meta.modified()?);

    #[cfg(target_os = "windows")]
    let times = {
        use std::os::windows::fs::FileTimesExt;
        times.set_created(meta.created()?)
    };

    #[cfg(target_os = "macos")]
    let times = {
        use std::os::macos::fs::FileTimesExt;
        times.set_created(meta.created()?)
    };

    Ok(times)
}
