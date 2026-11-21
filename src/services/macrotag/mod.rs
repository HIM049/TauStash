use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

pub use crate::database::entities::FileState;
use crate::database::entities::{TagNamespace, scan_source, tag};
use rust_i18n::t;

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

/// A scan source referenced by id, named after its folder.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceMeta {
    pub id: i32,
    pub name: String,
}

impl PartialEq for SourceMeta {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for SourceMeta {}

impl std::hash::Hash for SourceMeta {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl From<&scan_source::Model> for SourceMeta {
    fn from(m: &scan_source::Model) -> Self {
        let name = Path::new(&m.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| m.path.clone());
        Self { id: m.id, name }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroTag {
    Untagged,
    Standard(TagMetadata),
    Ext(String),
    MediaType(String),
    HasReplica(u32),
    State(FileState),
    From(SourceMeta),
    // Pending,
    // Managed,
    // Ignored,
    // Deleted(Option<DateTimeUtc>),
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

    pub fn id(&self) -> Option<Uuid> {
        match self {
            Self::Standard(meta) => Some(meta.id),
            _ => None,
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Standard(meta) => meta.name.clone(),
            Self::Ext(ext) => ext.to_uppercase(),
            Self::MediaType(mime) => mime.clone(),
            Self::Untagged => t!("tag.untagged").into_owned(),
            Self::HasReplica(n) => t!("tag.replica_count", count = n).into_owned(),
            Self::State(s) => s.display_name().to_string(),
            Self::From(source) => t!("tag.from", name = source.name).into_owned(),
        }
    }

    pub fn is_exclusive(&self) -> bool {
        matches!(self, Self::State(_))
    }

    pub fn is_writable(&self) -> bool {
        matches!(
            self,
            Self::Standard(_)
                | Self::State(FileState::Managed)
                | Self::State(FileState::Ignored)
                | Self::State(FileState::Deleted)
        )
    }
}
