use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum TagNamespace {
    #[default]
    #[sea_orm(string_value = "user")]
    User,
    #[sea_orm(string_value = "rule")]
    Rule,
    #[sea_orm(string_value = "ai")]
    Ai,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "tag")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique_key = "idx_tags_namespace_name")]
    pub namespace: TagNamespace,
    #[sea_orm(unique_key = "idx_tags_namespace_name")]
    pub name: String,
    pub color: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,

    #[sea_orm(has_many, via = "file_tag")]
    pub files: HasMany<super::file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
