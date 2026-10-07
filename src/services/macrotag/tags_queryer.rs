use sea_orm::{ActiveEnum, Iterable};
use uuid::Uuid;

use crate::database::entities::FileState;
use crate::database::storage::query::{FileQueryer, MatchMode};

/// Collects tags and turns them into a [`FileQueryer`].
#[derive(Clone, Debug, Default)]
pub struct MtagService {
    tags: Vec<super::MacroTag>,
    mode: MatchMode,
}

impl MtagService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tag(mut self, tag: super::MacroTag) -> Self {
        self.tags.push(tag);
        self
    }

    pub fn tags(mut self, tags: &[super::MacroTag]) -> Self {
        self.tags.extend_from_slice(tags);
        self
    }

    /// Set MatchMode Defaults to [`MatchMode::All`].
    pub fn mode(mut self, mode: MatchMode) -> Self {
        self.mode = mode;
        self
    }

    /// The tags collected so far.
    pub fn collected(&self) -> &[super::MacroTag] {
        &self.tags
    }

    /// Build the query. Each tag becomes the filter its space is stored in.
    pub fn build(self) -> anyhow::Result<FileQueryer> {
        let Self { tags, mode } = self;
        let mut query = FileQueryer::new();
        let mut extensions: Vec<String> = Vec::new();
        let mut mime_prefixes: Vec<String> = Vec::new();
        let mut named: Vec<Uuid> = Vec::new();

        for tag in tags {
            match tag {
                super::MacroTag::Ext(name) => extensions.push(name),
                super::MacroTag::MediaType(prefix) => mime_prefixes.push(prefix),
                super::MacroTag::FileState(name) => query = query.state(parse_state(&name)?),
                super::MacroTag::HasReplica(count) => query = query.replica_count(count),
                super::MacroTag::StandardTag(uuid) => named.push(uuid),
                super::MacroTag::Untagged => query = query.untagged(),
                super::MacroTag::Deleted(time) => query = query.deleted_since(time),
            }
        }

        if !extensions.is_empty() {
            query = query.extensions(extensions);
        }

        if !mime_prefixes.is_empty() {
            query = query.mime_prefixes(mime_prefixes);
        }

        query = query.include_tags(named, mode);

        Ok(query)
    }
}

pub fn parse_state(value: &str) -> anyhow::Result<FileState> {
    FileState::iter()
        .find(|state| state.to_value() == value)
        .ok_or(anyhow::anyhow!("Unknown state: {}", value))
}

impl super::MacroTag {
    pub fn is_writable(&self) -> bool {
        matches!(
            self,
            super::MacroTag::StandardTag(_)
                | super::MacroTag::FileState(_)
                | super::MacroTag::Deleted(_)
        )
    }
}
