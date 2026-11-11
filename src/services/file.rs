use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use gpui_kit::assets::IconName;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::database::entities::FileState;
use crate::database::storage::file::DbFileRecord;
use crate::database::storage::query::FileSortField;
use crate::database::storage::{self, query::FileQueryer};
use crate::services::collection::{GroupMode, TimeGroup};
use crate::services::macrotag::MacroTag;
use rust_i18n::t;

#[derive(Clone, Debug, Default)]
pub struct OrderMeta {
    pub content_created_at: Option<DateTime<Utc>>,
    pub content_modified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub size: i64,
}

impl OrderMeta {
    pub fn calc_group(
        &self,
        sort_field: FileSortField,
        group_mode: GroupMode,
    ) -> Option<(String, String)> {
        match group_mode {
            GroupMode::None => None,
            GroupMode::Size => Some(format_size_group(self.size)),
            GroupMode::Batch => Some(format_batch_group(self.created_at)),
            GroupMode::Time(group) => match sort_field {
                FileSortField::ContentCreatedAt => {
                    Some(format_date_group(self.content_created_at, group))
                }
                FileSortField::ContentModifiedAt => {
                    Some(format_date_group(self.content_modified_at, group))
                }
                FileSortField::CreatedAt => Some(format_date_group(Some(self.created_at), group)),
                FileSortField::UpdatedAt => Some(format_date_group(Some(self.updated_at), group)),
                FileSortField::Size => Some(format_size_group(self.size)),
            },
        }
    }
}

impl From<&DbFileRecord> for OrderMeta {
    fn from(r: &DbFileRecord) -> Self {
        Self {
            content_created_at: r.content_created_at,
            content_modified_at: r.content_modified_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
            size: r.size,
        }
    }
}

fn format_date_group(dt: Option<DateTime<Utc>>, group: TimeGroup) -> (String, String) {
    match dt {
        Some(dt) => {
            let local = dt.with_timezone(&chrono::Local);
            match group {
                TimeGroup::Day => {
                    let key = local.format("%Y-%m-%d").to_string();
                    let title = local.format(&t!("group.day")).to_string();
                    (key, title)
                }
                TimeGroup::Month => {
                    let key = local.format("%Y-%m").to_string();
                    let title = local.format(&t!("group.month")).to_string();
                    (key, title)
                }
                TimeGroup::Year => {
                    let key = local.format("%Y").to_string();
                    let title = local.format(&t!("group.year")).to_string();
                    (key, title)
                }
            }
        }
        None => ("other".to_string(), t!("group.other").into_owned()),
    }
}

fn format_batch_group(dt: DateTime<Utc>) -> (String, String) {
    let title = dt
        .with_timezone(&chrono::Local)
        .format(&t!("group.batch"))
        .to_string();
    (dt.to_rfc3339(), title)
}

fn format_size_group(size: i64) -> (String, String) {
    if size >= 1024 * 1024 * 1024 {
        ("0_gt_1gb".to_string(), t!("group.size_huge").into_owned())
    } else if size >= 100 * 1024 * 1024 {
        (
            "1_100mb_1gb".to_string(),
            t!("group.size_large").into_owned(),
        )
    } else if size >= 10 * 1024 * 1024 {
        (
            "2_10mb_100mb".to_string(),
            t!("group.size_medium").into_owned(),
        )
    } else if size >= 1024 * 1024 {
        ("3_1mb_10mb".to_string(), t!("group.size_small").into_owned())
    } else {
        ("4_lt_1mb".to_string(), t!("group.size_tiny").into_owned())
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub file_id: Uuid,
    pub path: String,
    pub file_name: String,
    pub hash: String,
    pub mime: String,
    pub ext: Option<String>,
    pub is_duplicate: bool,
    pub tags: Vec<MacroTag>,
    pub state: FileState,
    pub thumb: Option<PathBuf>,
    pub meta: OrderMeta,
}

impl FileInfo {
    pub fn from_query(record: DbFileRecord, tags: Vec<MacroTag>) -> Self {
        let meta = OrderMeta::from(&record);
        Self {
            file_id: record.file_id,
            path: record.path,
            file_name: record.file_name,
            hash: record.hash,
            mime: record.mime,
            ext: record.ext,
            is_duplicate: false,
            tags,
            state: record.state,
            thumb: record.thumbnail,
            meta,
        }
    }

    /// match icon by mime types
    pub fn type_icon(&self) -> IconName {
        let Some((kind, subtype)) = self.mime.split_once('/') else {
            return IconName::File;
        };
        match kind {
            "image" => IconName::FileImage,
            "video" => IconName::FileVideoCamera,
            "audio" => IconName::FileMusic,
            "font" => IconName::FileType,
            "model" => IconName::FileAxis3d,
            "text" => match subtype {
                "plain" | "markdown" => IconName::FileText,
                "csv" => IconName::FileSpreadsheet,
                "x-shellscript" => IconName::FileTerminal,
                _ => IconName::FileCode,
            },
            "application" => match subtype {
                "json" => IconName::FileBraces,
                "x-sh" => IconName::FileTerminal,
                "pdf" | "msword" | "rtf" | "epub+zip" => IconName::FileText,
                "zip" | "gzip" | "x-tar" | "x-rar-compressed" | "vnd.rar" | "x-7z-compressed"
                | "x-bzip2" | "x-xz" => IconName::FileArchive,
                "vnd.microsoft.portable-executable" | "x-executable" | "x-mach-binary" => {
                    IconName::FileCog
                }
                "vnd.sqlite3" => IconName::Database,
                "vnd.debian.binary-package"
                | "x-rpm"
                | "x-ole-storage"
                | "vnd.android.package-archive" => IconName::FileBox,
                s if s.starts_with("font-") => IconName::FileType,
                s if s.contains("wordprocessingml") || s.contains("opendocument.text") => {
                    IconName::FileText
                }
                s if s.contains("spreadsheet") || s.contains("ms-excel") => {
                    IconName::FileSpreadsheet
                }
                s if s.contains("presentation") || s.contains("ms-powerpoint") => {
                    IconName::FileChartPie
                }
                _ => IconName::File,
            },
            _ => IconName::File,
        }
    }

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

// /// Fetch full UI detail for a given file_id (including tags)
// pub async fn get_file_detail(db: &DatabaseConnection, file_id: Uuid) -> Result<Option<FileInfo>> {
//     let Some(record) = storage::file::get_file_by_id(db, file_id).await? else {
//         return Ok(None);
//     };

//     let tags: Vec<MacroTag> = storage::file::get_file_tags(db, file_id)
//         .await?
//         .into_iter()
//         .map(|t| MacroTag::Standard(t.into()))
//         .collect();

//     Ok(Some(FileInfo::from_query(record, tags)))
// }

/// List files filtered by query from database
pub async fn list_files(db: &DatabaseConnection, query: &FileQueryer) -> Result<Vec<FileInfo>> {
    let records = query.execute(db).await?;
    let file_ids: Vec<uuid::Uuid> = records.iter().map(|r| r.file_id).collect();
    let mut tags_by_file = storage::file::get_files_tags_batch(db, &file_ids).await?;

    let result = records
        .into_iter()
        .map(|r: DbFileRecord| {
            let tags: Vec<MacroTag> = tags_by_file
                .remove(&r.file_id)
                .unwrap_or_default()
                .into_iter()
                .map(|t| MacroTag::Standard(t.into()))
                .collect();

            FileInfo::from_query(r, tags)
        })
        .collect();

    Ok(result)
}
