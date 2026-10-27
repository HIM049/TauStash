use sea_orm::entity::prelude::*;

/// Mapping table for file and tag

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "file_thumbnail")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_id: Uuid,
    pub path: String,
    pub size: i64,
    pub create_at: DateTimeUtc,

    #[sea_orm(belongs_to, from = "file_id", to = "id")]
    pub file: BelongsTo<super::file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
