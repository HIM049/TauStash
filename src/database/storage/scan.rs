use std::path::PathBuf;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::database::entities::{self, ScanMode, scan_source};

fn standardize_path(path: String) -> String {
    // "\\?\C:\Users\me\Pictures" "\\?\UNC\nas\media\photos"
    if let Some(path) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_string()
    }
}

/// Register a directory to be scanned
pub async fn create_scan(
    db: &DatabaseConnection,
    path: PathBuf,
    mode: ScanMode,
) -> anyhow::Result<scan_source::Model> {
    let path = path.canonicalize()?.to_string_lossy().into_owned();
    let path = standardize_path(path);

    let now = Utc::now();
    let scan_source = entities::scan_source::ActiveModel {
        path: Set(path),
        mode: Set(mode),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let model = scan_source.insert(db).await?;
    Ok(model)
}

/// Fetch a single scan source by id
pub async fn get_scan_by_id(
    db: &DatabaseConnection,
    id: i32,
) -> anyhow::Result<Option<scan_source::Model>> {
    let res = scan_source::Entity::find_by_id(id).one(db).await?;
    Ok(res)
}

/// List every scan source, oldest first
pub async fn list_scans(db: &DatabaseConnection) -> anyhow::Result<Vec<scan_source::Model>> {
    let sources = scan_source::Entity::find()
        .order_by_asc(scan_source::Column::Id)
        .all(db)
        .await?;
    Ok(sources)
}

/// List scan sources using a given mode
pub async fn list_scans_by_mode(
    db: &DatabaseConnection,
    mode: ScanMode,
) -> anyhow::Result<Vec<scan_source::Model>> {
    let sources = scan_source::Entity::find()
        .filter(scan_source::Column::Mode.eq(mode))
        .order_by_asc(scan_source::Column::Id)
        .all(db)
        .await?;
    Ok(sources)
}

/// Update the mode of a scan source, leaving the rest untouched
pub async fn update_scan_mode(
    db: &DatabaseConnection,
    id: i32,
    mode: ScanMode,
) -> anyhow::Result<scan_source::Model> {
    let Some(existing) = get_scan_by_id(db, id).await? else {
        anyhow::bail!("Scan source not found: id={id}");
    };

    let mut active: scan_source::ActiveModel = existing.into();
    active.mode = Set(mode);
    active.updated_at = Set(Utc::now());
    let updated = active.update(db).await?;

    Ok(updated)
}

pub async fn set_scan_active(
    db: &DatabaseConnection,
    id: i32,
    is_active: bool,
) -> anyhow::Result<scan_source::Model> {
    let Some(existing) = get_scan_by_id(db, id).await? else {
        anyhow::bail!("Scan source not found: id={id}");
    };

    let mut active: scan_source::ActiveModel = existing.into();
    active.is_active = Set(is_active);
    active.updated_at = Set(Utc::now());
    let updated = active.update(db).await?;
    Ok(updated)
}

pub async fn update_scan_path(
    db: &DatabaseConnection,
    id: i32,
    path: PathBuf,
) -> anyhow::Result<scan_source::Model> {
    let path = path.canonicalize()?.to_string_lossy().into_owned();
    let path = standardize_path(path);

    let Some(existing) = get_scan_by_id(db, id).await? else {
        anyhow::bail!("Scan source not found: id={id}");
    };

    let mut active: scan_source::ActiveModel = existing.into();
    active.path = Set(path);
    active.updated_at = Set(Utc::now());
    let updated = active.update(db).await?;
    Ok(updated)
}

pub async fn delete_scan(db: &DatabaseConnection, id: i32) -> anyhow::Result<()> {
    if get_scan_by_id(db, id).await?.is_none() {
        anyhow::bail!("Scan source not found: id={id}");
    }

    scan_source::Entity::delete_by_id(id).exec(db).await?;
    Ok(())
}
