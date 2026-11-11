use image::ImageFormat;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, JoinType, QueryFilter, QuerySelect,
    RelationTrait, sea_query::Condition,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::database::entities::{ReplicaState, file, file_replica, file_thumbnail};
use crate::services::thumbnail;

pub const SUPPORTED_IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "ico", "tiff", "tif",
];

/// Whether the format can be read by embedded decoder
pub fn is_format_support(mime: Option<&str>, ext: Option<&str>) -> bool {
    mime.and_then(ImageFormat::from_mime_type)
        .or_else(|| ext.and_then(ImageFormat::from_extension))
        .is_some_and(|format| format.reading_enabled())
}

#[derive(Debug, Clone, Default)]
pub struct ThumbnailReport {
    pub total_candidates: usize,
    pub generated: usize,
    pub failed: usize,
    pub missing_source: usize,
}

impl ThumbnailReport {
    pub fn has_changes(&self) -> bool {
        self.generated > 0
    }
}

/// Maintain missing thumbnails for all files without a thumbnail record.
pub async fn maintain_thumbnails(
    db: &DatabaseConnection,
    device_id: &str,
) -> anyhow::Result<ThumbnailReport> {
    let mut report = ThumbnailReport::default();

    let is_mime_ok = file::Column::Mime.starts_with("image/");
    let is_ext_ok = file::Column::Ext.is_in(SUPPORTED_IMAGE_EXTS.iter().copied());
    let format_condition = Condition::any().add(is_mime_ok).add(is_ext_ok);

    let candidate_files = file::Entity::find()
        .join(JoinType::LeftJoin, file::Relation::FileThumbnail.def())
        .filter(file::Column::DeletedAt.is_null())
        .filter(file_thumbnail::Column::FileId.is_null())
        .filter(format_condition)
        .all(db)
        .await?;

    if candidate_files.is_empty() {
        return Ok(report);
    }

    let file_ids: Vec<Uuid> = candidate_files.iter().map(|f| f.id).collect();

    let replicas = file_replica::Entity::find()
        .filter(file_replica::Column::FileId.is_in(file_ids))
        .filter(file_replica::Column::DeletedAt.is_null())
        .filter(file_replica::Column::State.eq(ReplicaState::Available))
        .filter(file_replica::Column::DeviceId.eq(device_id))
        .all(db)
        .await?;

    let replica_paths: HashMap<Uuid, PathBuf> = replicas
        .into_iter()
        .map(|f| (f.file_id, PathBuf::from(f.uri)))
        .filter(|(_, path)| path.is_file())
        .collect();

    for file_record in candidate_files {
        if !is_format_support(file_record.mime.as_deref(), file_record.ext.as_deref()) {
            continue;
        }
        report.total_candidates += 1;

        let Some(source_path) = replica_paths.get(&file_record.id) else {
            warn!(
                "No accessible replica found for file_id: {} (name: {})",
                file_record.id, file_record.file_name
            );
            report.missing_source += 1;
            continue;
        };

        if let Err(e) =
            thumbnail::create_thumbnail(db, file_record.id, &file_record.hash, source_path.clone())
                .await
        {
            error!(
                "Failed to create thumbnail for file_id {}: {e}",
                file_record.id
            );
            report.failed += 1;
        } else {
            report.generated += 1;
        }
    }

    info!(
        "Thumbnail maintenance completed: total={}, generated={}, failed={}, missing_source={}",
        report.total_candidates, report.generated, report.failed, report.missing_source
    );

    Ok(report)
}
