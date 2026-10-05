use sea_orm::entity::prelude::DateTimeUtc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod tags_applier;
pub mod tags_queryer;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroTag {
    StandardTag(Uuid),
    Untagged,
    Ext(String),
    MediaType(String),
    FileState(String),
    HasReplica(u32),
    Deleted(DateTimeUtc),
}
