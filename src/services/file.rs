use anyhow::Result;
use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::database::entities::FileState;
use crate::database::storage::{self, query::FileQueryer};
use crate::services::macrotag::MacroTag;

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub file_id: Uuid,
    pub path: String,
    pub file_name: String,
    pub hash: String,
    pub size: i64,
    pub mime: String,
    pub ext: Option<String>,
    pub is_duplicate: bool,
    pub tags: Vec<MacroTag>,
    pub state: FileState,
    pub content_created_at: Option<DateTime<Utc>>,
    pub content_modified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FileInfo {
    pub fn has_tag(&self, tag: &MacroTag) -> bool {
        match tag {
            MacroTag::State(s) => self.state == *s,
            MacroTag::Standard(meta) => self.tags.iter().any(|t| match t {
                MacroTag::Standard(m) => m.id == meta.id,
                _ => false,
            }),
            MacroTag::Ext(ext) => self
                .ext
                .as_deref()
                .map(|e| e.eq_ignore_ascii_case(ext))
                .unwrap_or(false),
            MacroTag::HasReplica(_) => self.is_duplicate,
            _ => false,
        }
    }
}

/// Fetch full UI detail for a given file_id (including tags)
pub async fn get_file_detail(db: &DatabaseConnection, file_id: Uuid) -> Result<Option<FileInfo>> {
    let Some(record) = storage::file::get_file_by_id(db, file_id).await? else {
        return Ok(None);
    };

    let tags: Vec<MacroTag> = storage::file::get_file_tags(db, file_id)
        .await?
        .into_iter()
        .map(|t| MacroTag::Standard(t.into()))
        .collect();

    Ok(Some(FileInfo {
        file_id: record.file_id,
        path: record.path,
        file_name: record.file_name,
        hash: record.hash,
        size: record.size,
        mime: record.mime,
        ext: record.ext,
        is_duplicate: false,
        tags,
        state: record.state,
        content_created_at: record.content_created_at,
        content_modified_at: record.content_modified_at,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }))
}

/// List files filtered by query from database
pub async fn list_files(db: &DatabaseConnection, query: &FileQueryer) -> Result<Vec<FileInfo>> {
    let records = query.execute(db).await?;
    let file_ids: Vec<uuid::Uuid> = records.iter().map(|r| r.file_id).collect();
    let mut tags_by_file = storage::file::get_files_tags_batch(db, &file_ids).await?;

    let result = records
        .into_iter()
        .map(|r| {
            let tags: Vec<MacroTag> = tags_by_file
                .remove(&r.file_id)
                .unwrap_or_default()
                .into_iter()
                .map(|t| MacroTag::Standard(t.into()))
                .collect();
            FileInfo {
                file_id: r.file_id,
                path: r.path,
                file_name: r.file_name,
                hash: r.hash,
                size: r.size,
                mime: r.mime,
                ext: r.ext,
                is_duplicate: false,
                tags,
                state: r.state,
                content_created_at: r.content_created_at,
                content_modified_at: r.content_modified_at,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }
        })
        .collect();

    Ok(result)
}
