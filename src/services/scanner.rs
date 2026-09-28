use anyhow::Result;
use sea_orm::DatabaseConnection;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

use super::importer;
use crate::database::entities::{ReplicaState, file_replica};
use crate::database::{storage, utils};

#[derive(Debug, Clone, Default)]
pub struct ScanReport {
    pub total_found: usize,
    pub newly_imported: usize,
    pub skipped_unchanged: usize,
    pub marked_missing: usize,
}

/// Scan a directory and import
pub async fn scan_directory(
    db: &DatabaseConnection,
    dir_path: impl AsRef<Path>,
    device_id: &str,
) -> Result<ScanReport> {
    let dir_path = dir_path.as_ref();
    if !dir_path.exists() || !dir_path.is_dir() {
        anyhow::bail!(
            "Path does not exist or is not a directory: {}",
            dir_path.display()
        );
    }

    let canonical_root = dir_path
        .canonicalize()
        .unwrap_or_else(|_| dir_path.to_path_buf());
    let root_str = utils::standardize_path(canonical_root.to_string_lossy().to_string());

    // Preload all replicas by root prefix
    let existing_replicas =
        storage::file::find_replicas_by_prefix(db, device_id, &root_str).await?;

    let mut replica_map: HashMap<String, file_replica::Model> = existing_replicas
        .into_iter()
        .map(|r| (r.uri.clone(), r))
        .collect();

    let mut report = ScanReport::default();
    let mut observed_uris = HashSet::new();

    // Walk directory and check
    for entry in WalkDir::new(&canonical_root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if name.starts_with('.') && name != "." {
                return false;
            }
            if name == "node_modules" || name == "$RECYCLE.BIN" || name == "target" {
                return false;
            }
            true
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        report.total_found += 1;
        let p = entry.path();
        let canonical_file = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let path = canonical_file.to_string_lossy().to_string();
        let uri = utils::standardize_path(path);
        observed_uris.insert(uri.clone());

        // check metadata
        let meta = match std::fs::metadata(p) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let current_size = meta.len() as i64;
        let current_mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        if let Some(existing) = replica_map.get(&uri) {
            let mtime_matched = existing.mtime == current_mtime;
            let size_matched = existing.file_size == Some(current_size);

            if mtime_matched && size_matched && existing.state == ReplicaState::Local {
                // skip unchanged
                report.skipped_unchanged += 1;
                continue;
            }
        }

        // import new file
        if importer::import_file(db, p, device_id).await.is_ok() {
            report.newly_imported += 1;
        }
    }

    // compare missing files
    for (uri, replica) in replica_map.drain() {
        if !observed_uris.contains(&uri)
            && replica.state != ReplicaState::Missing
            && storage::file::mark_replica_missing(db, replica)
                .await
                .is_ok()
        {
            report.marked_missing += 1;
        }
    }

    Ok(report)
}
