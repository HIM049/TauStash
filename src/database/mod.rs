pub mod entities;
pub mod schema;
pub mod storage;

use sea_orm::{Database, DatabaseConnection, DbErr};
use std::path::Path;

/// Connect to a SQLite database file
pub async fn init_db(db_path: impl AsRef<Path>) -> Result<DatabaseConnection, DbErr> {
    let path_str = db_path.as_ref().to_string_lossy();

    // Ensure parent directory exists
    if let Some(parent) = db_path.as_ref().parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let db =
        Database::connect(format!("sqlite://{}?mode=rwc", path_str.replace('\\', "/"))).await?;

    schema::create_tables(&db).await?;
    Ok(db)
}
