use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub hash: String,
    pub size: i64,
    pub mime: Option<String>,
    pub ext: Option<String>,
    pub media_meta: Option<String>,
    #[sea_orm(indexed)]
    pub content_created_at: Option<DateTimeUtc>,
    #[sea_orm(indexed)]
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(has_many)]
    pub file_replicas: HasMany<super::file_replica::Entity>,
    #[sea_orm(has_many, via = "file_tag")]
    pub tags: HasMany<super::tag::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
