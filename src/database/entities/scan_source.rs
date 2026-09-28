use sea_orm::entity::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum ScanMode {
    #[sea_orm(string_value = "watch")]
    Watch,
    #[sea_orm(string_value = "schedule")]
    Schedule,
    #[sea_orm(string_value = "manual")]
    Manual,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "scan_source")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub path: String,
    pub mode: ScanMode,
    pub is_active: bool,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
