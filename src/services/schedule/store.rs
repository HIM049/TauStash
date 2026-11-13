use chrono::Utc;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use std::collections::HashSet;
use std::path::Path;
use tracing::{info, warn};

use crate::config::StoreAutoMode;
use crate::database::entities::{FileState, ReplicaState, file, file_replica};
use crate::database::storage::file::find_replicas_by_prefix;
use crate::services::storage;

#[derive(Debug, Clone, Default)]
pub struct StoreReport {
    pub total_candidates: usize,
    pub stored: usize,
    pub failed: usize,
}

impl StoreReport {
    pub fn has_changes(&self) -> bool {
        self.stored > 0
    }
}

/// Auto handling task for archived files
pub async fn store_archived_files(
    db: &DatabaseConnection,
    device_id: &str,
    store_path: &Path,
    mode: StoreAutoMode,
) -> anyhow::Result<StoreReport> {
    let mut report = StoreReport::default();
    if mode == StoreAutoMode::Off {
        return Ok(report);
    }

    let root = storage::resolve_root(store_path)?;

    // Files that already have an available replica in the store
    let mut in_store: HashSet<_> = find_replicas_by_prefix(db, device_id, &root.to_string_lossy())
        .await?
        .into_iter()
        .filter(|r| r.state == ReplicaState::Available)
        .map(|r| r.file_id)
        .collect();

    let candidates = file_replica::Entity::find()
        .find_also_related(file::Entity)
        .filter(file::Column::State.eq(FileState::Managed))
        .filter(file::Column::DeletedAt.is_null())
        .filter(file_replica::Column::DeviceId.eq(device_id))
        .filter(file_replica::Column::State.eq(ReplicaState::Available))
        .filter(file_replica::Column::DeletedAt.is_null())
        .all(db)
        .await?;

    for (replica, file) in candidates {
        let Some(file) = file else { continue };
        let src = Path::new(&replica.uri);
        if src.starts_with(&root) {
            continue;
        }
        if mode == StoreAutoMode::Copy && in_store.contains(&file.id) {
            continue;
        }

        report.total_candidates += 1;
        let result = match mode {
            StoreAutoMode::Copy => {
                storage::copy_into_store(db, store_path, src, device_id, Some(&file.hash))
                    .await
                    .map(|_| ())
            }
            _ => storage::move_into_store(
                db,
                store_path,
                src,
                device_id,
                &[],
                Utc::now(),
                Some(&file.hash),
            )
            .await
            .map(|_| ()),
        };

        match result {
            Ok(()) => {
                report.stored += 1;
                in_store.insert(file.id);
            }
            Err(e) => {
                warn!("Failed to store {}: {:#}", src.display(), e);
                report.failed += 1;
            }
        }
    }

    if report.total_candidates > 0 {
        info!(
            "Store archived files ({:?}): {} candidates, {} stored, {} failed",
            mode, report.total_candidates, report.stored, report.failed
        );
    }

    Ok(report)
}
