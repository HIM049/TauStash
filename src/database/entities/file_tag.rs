use sea_orm::entity::prelude::*;

/// Mapping table for file and tag

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "file_tag")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false, indexed)]
    pub tag_id: Uuid,
    #[sea_orm(belongs_to, from = "file_id", to = "id")]
    pub file: BelongsTo<super::file::Entity>,
    #[sea_orm(belongs_to, from = "tag_id", to = "id")]
    pub tag: BelongsTo<super::tag::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
