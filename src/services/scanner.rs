use anyhow::Result;
use chrono::DateTime;
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

use super::importer;
use crate::database::entities::{FileState, ReplicaState, file_replica, scan_source};
use crate::database::{storage, utils};
use crate::services::macrotag::MacroTag;

#[derive(Debug, Clone, Default)]
pub struct BatchReport {
    pub total_found: usize,
    pub newly_imported: usize,
    pub skipped_unchanged: usize,
    pub failed: Vec<String>,
}

impl BatchReport {
    /// Whether this batch caused any repository changes
    pub fn has_changes(&self) -> bool {
        self.newly_imported > 0
    }

    /// Merge results from another batch report
    pub fn merge(&mut self, other: BatchReport) {
        self.total_found += other.total_found;
        self.newly_imported += other.newly_imported;
        self.skipped_unchanged += other.skipped_unchanged;
        self.failed.extend(other.failed);
    }
}

#[derive(Debug, Clone)]
pub struct ScanTask {
    pub path: PathBuf,
    pub initial_tags: Vec<MacroTag>,
}

/// Filter out any non-writable tags, ensuring only valid writable tags are persisted or loaded
pub fn sanitize_initial_tags(tags: Vec<MacroTag>) -> Vec<MacroTag> {
    tags.into_iter().filter(|t| t.is_writable()).collect()
}

/// Build sorted scan task from sources
pub fn build_scan_rules(sources: &[scan_source::Model]) -> Vec<ScanTask> {
    let mut rules = Vec::with_capacity(sources.len());
    for s in sources {
        let initial_tags: Vec<MacroTag> = s
            .initial_tags
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .map(sanitize_initial_tags)
            .unwrap_or_default();
        let p = PathBuf::from(&s.path);
        let canonical_p = p.canonicalize().unwrap_or(p);
        rules.push(ScanTask {
            path: canonical_p,
            initial_tags,
        });
    }
    rules.sort_by_key(|r| r.path.components().count());
    rules
}

/// Resolve merged initial tags for a file path
pub fn resolve_tags_by_path(rules: &[ScanTask], file_path: &Path) -> Vec<MacroTag> {
    let canonical_file = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    let mut merged_tags: Vec<MacroTag> = Vec::new();

    for rule in rules {
        if canonical_file.starts_with(&rule.path) {
            for tag in &rule.initial_tags {
                if tag.is_exclusive() {
                    let disc = std::mem::discriminant(tag);
                    merged_tags.retain(|t| std::mem::discriminant(t) != disc);
                    merged_tags.push(tag.clone());
                } else if !merged_tags.contains(tag) {
                    merged_tags.push(tag.clone());
                }
            }
        }
    }

    merged_tags
}

/// Given a list of scan sources, return only the top-level (root) sources
pub fn filter_root_sources(mut sources: Vec<scan_source::Model>) -> Vec<scan_source::Model> {
    let paths: Vec<PathBuf> = sources
        .iter()
        .map(|s| {
            let p = PathBuf::from(&s.path);
            p.canonicalize().unwrap_or(p)
        })
        .collect();

    let mut is_root = vec![true; sources.len()];
    for i in 0..sources.len() {
        for j in 0..sources.len() {
            if i != j && paths[i].starts_with(&paths[j]) {
                is_root[i] = false;
                break;
            }
        }
    }

    let mut iter = is_root.into_iter();
    sources.retain(|_| iter.next().unwrap_or(false));
    sources
}

/// Scan a directory using scan rules and import files
pub async fn scan_task(
    db: &DatabaseConnection,
    dir_path: impl AsRef<Path>,
    device_id: &str,
    rules: &[ScanTask],
) -> Result<BatchReport> {
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

    let replica_map: HashMap<String, file_replica::Model> = existing_replicas
        .into_iter()
        .map(|r| (r.uri.clone(), r))
        .collect();

    let mut report = BatchReport::default();

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
        // is not file
        if !entry.file_type().is_file() {
            continue;
        }

        report.total_found += 1;
        let p = entry.path();
        let canonical_file = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let path = canonical_file.to_string_lossy().to_string();
        let uri = utils::standardize_path(path);

        // check metadata
        let meta = match std::fs::metadata(p) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let current_size = meta.len() as i64;
        let current_utime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos()));

        // try match with existing file
        if let Some(existing) = replica_map.get(&uri) {
            let utime_unchange = existing.utime == current_utime;
            let size_unchange = existing.file_size == Some(current_size);

            if utime_unchange && size_unchange && existing.state == ReplicaState::Available {
                // skip unchanged
                report.skipped_unchanged += 1;
                continue;
            }
        }

        let initial_tags = resolve_tags_by_path(rules, p);
        if importer::import_file(db, p, device_id, &initial_tags)
            .await
            .is_ok()
        {
            report.newly_imported += 1;
        }
    }

    Ok(report)
}

/// Scan a directory and import
pub async fn scan_directory(
    db: &DatabaseConnection,
    dir_path: impl AsRef<Path>,
    device_id: &str,
    default_state: FileState,
) -> Result<BatchReport> {
    let sources = storage::scan_source::list_active_scans(db)
        .await
        .unwrap_or_default();
    let mut rules = build_scan_rules(&sources);
    if rules.is_empty() {
        let p = dir_path.as_ref();
        let canonical_p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        rules.push(ScanTask {
            path: canonical_p,
            initial_tags: vec![MacroTag::State(default_state)],
        });
    }
    scan_task(db, dir_path, device_id, &rules).await
}

pub async fn scan_all_sources(db: &DatabaseConnection, device_id: &str) -> Result<BatchReport> {
    let sources = storage::scan_source::list_active_scans(db).await?;
    if sources.is_empty() {
        return Ok(BatchReport::default());
    }

    let rules = build_scan_rules(&sources);
    let root_sources = filter_root_sources(sources);

    let mut report = BatchReport::default();
    for source in root_sources {
        match scan_task(db, &source.path, device_id, &rules).await {
            Ok(rep) => {
                report.merge(rep);
            }
            Err(e) => {
                tracing::error!("Failed to scan directory {}: {}", source.path, e);
                report.failed.push(source.path);
            }
        }
    }

    Ok(report)
}
