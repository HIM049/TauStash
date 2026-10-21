use sea_orm::entity::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum ReplicaState {
    #[sea_orm(string_value = "available")]
    Available,
    #[sea_orm(string_value = "missing")]
    Missing,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_replica")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique_key = "idx_replicas_device_uri")]
    pub device_id: String,
    #[sea_orm(unique_key = "idx_replicas_device_uri")]
    pub uri: String,
    #[sea_orm(indexed)]
    pub state: ReplicaState,
    pub file_name: String,
    // Filesystem times, per path on this device
    pub ctime: Option<DateTimeUtc>,
    pub utime: Option<DateTimeUtc>,
    pub file_size: Option<i64>,
    pub deleted_at: Option<DateTimeUtc>,

    #[sea_orm(belongs_to, from = "file_id", to = "id")]
    pub file: HasOne<super::file::Entity>,
    #[sea_orm(indexed)]
    pub file_id: Uuid,
}

impl ActiveModelBehavior for ActiveModel {}
