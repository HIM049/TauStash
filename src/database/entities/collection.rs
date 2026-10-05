use sea_orm::entity::prelude::*;

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
    pub cover: String,
    pub color: String,
    pub order: u32,
    pub parent_id: Option<Uuid>,

    #[sea_orm(indexed)]
    pub typ: CollectionType,
    pub config: Json,

    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,
}

impl ActiveModelBehavior for ActiveModel {}
