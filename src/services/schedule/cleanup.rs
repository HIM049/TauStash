use chrono::{Duration, Utc};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, JoinType, QueryFilter, QuerySelect,
    RelationTrait,
};
use std::path::Path;

use crate::database::entities::{file, file_replica};

#[derive(Debug, Clone, Default)]
pub struct CleanupReport {
    pub total_scanned: usize,
    pub replicas_removed: usize,
    pub files_removed: usize,
    pub errors: Vec<String>,
}

/// Physically removes expired replicas and hard-deletes their database records.
pub async fn cleanup_expired_replicas(
    db: &DatabaseConnection,
    device_id: &str,
    retention_days: u32,
) -> anyhow::Result<CleanupReport> {
    let mut report = CleanupReport::default();
    let cutoff = Utc::now() - Duration::days(retention_days as i64);

    let expired_condition = Condition::any()
        .add(
            Condition::all()
                .add(file_replica::Column::DeletedAt.is_not_null())
                .add(file_replica::Column::DeletedAt.lte(cutoff)),
        )
        .add(
            Condition::all()
                .add(file::Column::DeletedAt.is_not_null())
                .add(file::Column::DeletedAt.lte(cutoff)),
        );

    let replicas = file_replica::Entity::find()
        .join(JoinType::InnerJoin, file_replica::Relation::File.def())
        .filter(file_replica::Column::DeviceId.eq(device_id))
        .filter(expired_condition)
        .all(db)
        .await?;

    report.total_scanned = replicas.len();

    for replica in replicas {
        let path = Path::new(&replica.uri);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(path) {
                tracing::warn!("Failed to physically delete file {}: {e}", path.display());
                report.errors.push(format!("{}: {e}", path.display()));
            } else {
                report.files_removed += 1;
            }
        }

        if let Err(e) = file_replica::Entity::delete_by_id(replica.id)
            .exec(db)
            .await
        {
            tracing::error!("Failed to delete replica record {}: {e}", replica.id);
            report.errors.push(format!("replica {}: {e}", replica.id));
        } else {
            report.replicas_removed += 1;
        }
    }

    tracing::debug!(
        "Cleanup pass completed: scanned {}, deleted {} replicas & {} files",
        report.total_scanned,
        report.replicas_removed,
        report.files_removed
    );

    Ok(report)
}
