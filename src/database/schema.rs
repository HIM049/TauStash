use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, Schema};

use super::entities::*;

/// Initialize all database tables and their indexes derived directly from Entity models
pub async fn create_tables(db: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);

    init_entity(db, &schema, File).await?;
    init_entity(db, &schema, FileReplica).await?;
    init_entity(db, &schema, Tag).await?;
    init_entity(db, &schema, FileTag).await?;
    init_entity(db, &schema, ScanSource).await?;

    Ok(())
}

async fn init_entity<E: EntityTrait>(
    db: &DatabaseConnection,
    schema: &Schema,
    entity: E,
) -> Result<(), DbErr> {
    // create table
    let table_stmt = schema
        .create_table_from_entity(entity)
        .if_not_exists()
        .to_owned();
    db.execute(&table_stmt).await?;

    // create indexes
    for mut idx_stmt in schema.create_index_from_entity(entity) {
        idx_stmt.if_not_exists();
        db.execute(&idx_stmt).await?;
    }

    Ok(())
}
