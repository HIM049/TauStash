use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, EntityTrait, Schema, Statement};
use std::collections::BTreeMap;

use super::entities::*;

#[tokio::test]
async fn migrations_match_entities() {
    let migrated = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&migrated, None).await.unwrap();

    let expected = Database::connect("sqlite::memory:").await.unwrap();
    create_from_entity(&expected, File).await;
    create_from_entity(&expected, FileReplica).await;
    create_from_entity(&expected, Tag).await;
    create_from_entity(&expected, FileTag).await;
    create_from_entity(&expected, ScanSource).await;
    create_from_entity(&expected, Collection).await;
    create_from_entity(&expected, FileThumbnail).await;

    assert_eq!(snapshot(&migrated).await, snapshot(&expected).await);
}

async fn create_from_entity<E: EntityTrait>(db: &DatabaseConnection, entity: E) {
    let schema = Schema::new(db.get_database_backend());
    db.execute(&schema.create_table_from_entity(entity))
        .await
        .unwrap();
    for index in schema.create_index_from_entity(entity) {
        db.execute(&index).await.unwrap();
    }
}

/// Columns, indexes and foreign keys of each app table, ignoring column order and
/// the names SQLite gives to its own unique indexes.
async fn snapshot(db: &DatabaseConnection) -> BTreeMap<String, Vec<String>> {
    let tables = text_rows(
        db,
        "SELECT name FROM sqlite_master WHERE type = 'table' \
         AND name NOT LIKE 'sqlite_%' AND name <> 'seaql_migrations' ORDER BY name"
            .to_string(),
    )
    .await;

    let mut snapshot = BTreeMap::new();
    for table in tables {
        let mut lines = text_rows(
            db,
            format!(
                "SELECT 'column ' || name || ' ' || type || ' notnull=' || \"notnull\" \
                 || ' default=' || IFNULL(dflt_value, '') || ' pk=' || pk \
                 FROM pragma_table_info('{table}') ORDER BY name"
            ),
        )
        .await;
        lines.extend(
            text_rows(
                db,
                format!(
                    "SELECT 'index ' || CASE WHEN il.name LIKE 'sqlite_autoindex%' \
                     THEN il.origin ELSE il.name END || ' unique=' || il.\"unique\" || ' (' \
                     || (SELECT group_concat(name, ',') FROM \
                        (SELECT name FROM pragma_index_info(il.name) ORDER BY seqno)) || ')' \
                     FROM pragma_index_list('{table}') il ORDER BY 1"
                ),
            )
            .await,
        );
        lines.extend(
            text_rows(
                db,
                format!(
                    "SELECT 'fk ' || \"from\" || ' -> ' || \"table\" || '.' || \"to\" \
                     || ' update=' || on_update || ' delete=' || on_delete \
                     FROM pragma_foreign_key_list('{table}') ORDER BY 1"
                ),
            )
            .await,
        );
        snapshot.insert(table, lines);
    }
    snapshot
}

async fn text_rows(db: &DatabaseConnection, sql: String) -> Vec<String> {
    let stmt = Statement::from_string(db.get_database_backend(), sql);
    db.query_all_raw(stmt)
        .await
        .unwrap()
        .iter()
        .map(|row| row.try_get_by_index::<String>(0).unwrap())
        .collect()
}
