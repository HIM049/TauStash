use sea_orm::entity::prelude::DateTimeUtc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::database::entities::FileState;
use crate::database::entities::{TagNamespace, tag};

pub mod tags_applier;
pub mod tags_queryer;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagMetadata {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub namespace: TagNamespace,
    pub color: Option<String>,
}

impl PartialEq for TagMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TagMetadata {}

impl std::hash::Hash for TagMetadata {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl TagMetadata {
    pub fn new(id: Uuid, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            namespace: TagNamespace::User,
            color: None,
        }
    }
}

impl From<tag::Model> for TagMetadata {
    fn from(m: tag::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            namespace: m.namespace,
            color: m.color,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroTag {
    Untagged,
    Standard(TagMetadata),
    FileState(FileState),
    Ext(String),
    MediaType(String),
    HasReplica(u32),
    Deleted(DateTimeUtc),
}

impl MacroTag {
    pub fn standard(id: Uuid, name: impl Into<String>) -> Self {
        Self::Standard(TagMetadata::new(id, name))
    }

    pub fn standard_id(id: Uuid) -> Self {
        Self::Standard(TagMetadata {
            id,
            name: String::new(),
            namespace: TagNamespace::User,
            color: None,
        })
    }

    pub fn state(state: FileState) -> Self {
        Self::FileState(state)
    }

    pub fn id(&self) -> Option<Uuid> {
        match self {
            Self::Standard(meta) => Some(meta.id),
            _ => None,
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Standard(meta) => meta.name.clone(),
            Self::FileState(st) => st.display_name().to_string(),
            Self::Ext(ext) => ext.to_uppercase(),
            Self::MediaType(mime) => mime.clone(),
            Self::Untagged => "未标记".to_string(),
            Self::Deleted(_) => "已删除".to_string(),
            Self::HasReplica(n) => format!("副本数: {}", n),
        }
    }

    pub fn is_exclusive(&self) -> bool {
        matches!(self, Self::FileState(_))
    }

    pub fn is_writable(&self) -> bool {
        matches!(
            self,
            Self::Standard(_) | Self::FileState(_) | Self::Deleted(_)
        )
    }
}
