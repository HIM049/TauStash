use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult)]
pub struct TagIdList(pub Vec<Uuid>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GroupRule {
    #[default]
    None,
    CreateTime,
    UpdateTime,
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrderRule {
    #[default]
    CreateTime,
    FileName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ViewOption {
    #[default]
    Grid,
    List,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult, Default)]
pub struct CollectionConfig {
    pub icon: String,
    pub cover: String,
    pub view: ViewOption,
    pub group_by: GroupRule,
    pub order_by: OrderRule,
    /// Default should be DESC
    pub is_order_revert: bool,
    pub is_hide_tagbar: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum CollectionType {
    #[sea_orm(string_value = "view")]
    View,
    #[sea_orm(string_value = "folder")]
    Folder,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "collection")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub order: u32,
    pub parent_id: Option<Uuid>,

    pub filters: TagIdList,

    #[sea_orm(indexed)]
    pub typ: CollectionType,
    pub config: CollectionConfig,

    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,
}

impl ActiveModelBehavior for ActiveModel {}
