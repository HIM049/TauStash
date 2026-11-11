pub mod entities;
#[cfg(test)]
mod migration_test;
pub mod schema;
pub mod storage;
pub mod utils;

use migration::{Migrator, MigratorTrait};
use sea_orm::{Database, DatabaseConnection};
use std::path::Path;

/// Connect to a SQLite database file
pub async fn init_db(db_path: impl AsRef<Path>) -> anyhow::Result<DatabaseConnection> {
    let path = db_path.as_ref();
    let is_new = !path.exists();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let path_str = path.to_string_lossy();
    let db =
        Database::connect(format!("sqlite://{}?mode=rwc", path_str.replace('\\', "/"))).await?;

    Migrator::up(&db, None).await?;

    if is_new {
        schema::seed_default_views(&db).await?;
    }

    Ok(db)
}
