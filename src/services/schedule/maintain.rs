use chrono::Utc;
use sea_orm::DatabaseConnection;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait,
    Set,
};
use std::path::Path;
use uuid::Uuid;

use crate::database::entities::{FileState, ReplicaState, file, file_replica};
use crate::database::storage::{self, file::ContentTimes};
use crate::services::importer;

#[derive(Debug, Clone, Default)]
pub struct MaintenanceReport {
    pub total: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub refound: usize,
    pub updated: usize,
}

impl MaintenanceReport {
    pub fn has_changes(&self) -> bool {
        self.missing > 0 || self.refound > 0 || self.updated > 0
    }
}

/// Update creation time by MIN(ctime) and modification time by MAX(utime)
/// from active local replicas into the file table.
pub async fn update_content_times(db: &DatabaseConnection) -> anyhow::Result<usize> {
    use sea_orm::ConnectionTrait;

    let sql = r#"
        UPDATE file
        SET
            content_created_at = (
                SELECT MIN(ctime) FROM file_replica
                WHERE file_replica.file_id = file.id
                  AND file_replica.deleted_at IS NULL
                  AND file_replica.state = 'available'
            ),
            content_modified_at = (
                SELECT MAX(utime) FROM file_replica
                WHERE file_replica.file_id = file.id
                  AND file_replica.deleted_at IS NULL
                  AND file_replica.state = 'available'
            )
        WHERE file.deleted_at IS NULL
          AND EXISTS (
              SELECT 1 FROM file_replica
              WHERE file_replica.file_id = file.id
                AND file_replica.deleted_at IS NULL
                AND file_replica.state = 'available'
          );
    "#;

    let res = db.execute_unprepared(sql).await?;
    Ok(res.rows_affected() as usize)
}

/// Run verification and metadata calibration on replicas.
pub async fn maintain_storage(
    db: &DatabaseConnection,
    device_id: &str,
) -> anyhow::Result<MaintenanceReport> {
    let mut report = MaintenanceReport::default();
    let imported_at = Utc::now();

    let replicas = file_replica::Entity::find()
        .join(JoinType::InnerJoin, file_replica::Relation::File.def())
        .filter(file_replica::Column::DeviceId.eq(device_id))
        .filter(file_replica::Column::DeletedAt.is_null())
        .filter(file::Column::DeletedAt.is_null())
        .all(db)
        .await?;

    for replica in replicas {
        report.total += 1;
        let path = Path::new(&replica.uri);

        let Ok(metadata) = std::fs::metadata(path) else {
            if replica.state != ReplicaState::Missing {
                if storage::file::mark_replica_missing(db, replica)
                    .await
                    .is_ok()
                {
                    report.missing += 1;
                }
            }
            continue;
        };

        let current_size = metadata.len() as i64;
        let current_ctime = metadata.created().ok().and_then(importer::to_utc);
        let current_utime = metadata.modified().ok().and_then(importer::to_utc);

        let refound = replica.state == ReplicaState::Missing;
        let size_matches = replica.file_size == Some(current_size);
        let utime_matches = replica.utime == current_utime;
        let ctime_matches = replica.ctime == current_ctime;

        // UNCHANGED file
        if size_matches && utime_matches && ctime_matches {
            // if UNCHANGED but MISSED before
            if refound {
                let mut active: file_replica::ActiveModel = replica.into();
                active.state = Set(ReplicaState::Available);
                active.ctime = Set(current_ctime);
                active.update(db).await?;
                report.refound += 1;
            } else {
                report.unchanged += 1;
            }
            continue;
        }

        let new_file_meta = match importer::compute_file_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to recompute metadata for {}: {e}", path.display());
                continue;
            }
        };

        let current_file = file::Entity::find_by_id(replica.file_id).one(db).await?;
        let is_unchanged = current_file
            .as_ref()
            .map(|f| f.hash == new_file_meta.hash)
            .unwrap_or(false);

        if is_unchanged {
            // Content unchanged
            let mut active: file_replica::ActiveModel = replica.into();
            active.state = Set(ReplicaState::Available);
            active.ctime = Set(new_file_meta.ctime);
            active.utime = Set(new_file_meta.utime);
            active.file_size = Set(Some(new_file_meta.size));
            active.update(db).await?;

            if refound {
                report.refound += 1;
            } else {
                report.updated += 1;
            }
        } else {
            // Hash mismatch
            let target_file = file::Entity::find()
                .filter(file::Column::Hash.eq(&new_file_meta.hash))
                .one(db)
                .await?;

            let target_file_id = if let Some(target) = target_file {
                target.id
            } else {
                let new_id = Uuid::now_v7();
                let initial_times = ContentTimes {
                    created_at: new_file_meta.ctime,
                    modified_at: new_file_meta.utime,
                };
                let new_file = storage::file::create_file(
                    db,
                    storage::file::CreateFileParams {
                        id: new_id,
                        file_name: &replica.file_name,
                        hash: &new_file_meta.hash,
                        size: new_file_meta.size,
                        mime: Some(new_file_meta.mime.clone()),
                        ext: new_file_meta.ext.clone(),
                        state: FileState::Managed,
                        content_times: initial_times,
                        imported_at,
                    },
                )
                .await?;

                new_file.id
            };

            let mut active: file_replica::ActiveModel = replica.into();
            active.file_id = Set(target_file_id);
            active.state = Set(ReplicaState::Available);
            active.ctime = Set(new_file_meta.ctime);
            active.utime = Set(new_file_meta.utime);
            active.file_size = Set(Some(new_file_meta.size));
            active.update(db).await?;
            report.updated += 1;
        }
    }

    update_content_times(db).await?;

    Ok(report)
}
