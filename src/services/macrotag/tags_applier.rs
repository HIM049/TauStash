use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::database::entities::TagNamespace;
use crate::database::storage::file as file_store;
use crate::database::storage::tag as tag_store;
use crate::services::tags_queryer::MtagService;
use crate::services::tags_queryer::parse_state;

impl MtagService {
    /// Attach tags to files
    pub async fn attach(
        db: &DatabaseConnection,
        file_ids: &[Uuid],
        tag_ids: &[Uuid],
    ) -> anyhow::Result<()> {
        tag_store::link(db, file_ids, tag_ids).await
    }

    /// Detach tags from files
    pub async fn detach(
        db: &DatabaseConnection,
        file_ids: &[Uuid],
        tag_ids: &[Uuid],
    ) -> anyhow::Result<()> {
        tag_store::unlink(db, file_ids, tag_ids).await
    }

    /// Attach a tag named in this namespace, creating it when new.
    pub async fn attach_by_name(
        db: &DatabaseConnection,
        file_ids: &[Uuid],
        namespace: TagNamespace,
        names: &[String],
    ) -> anyhow::Result<Vec<Uuid>> {
        if file_ids.is_empty() || names.is_empty() {
            return Ok(Vec::new());
        }

        let mut tag_ids = Vec::with_capacity(names.len());
        for name in names {
            let name = name.trim().to_string();
            if !name.is_empty() {
                let tag = tag_store::find_or_create(db, namespace, name).await?;
                tag_ids.push(tag.id);
            }

            if !tag_ids.is_empty() {
                tag_store::link(db, file_ids, &tag_ids).await?;
            }
        }

        Ok(tag_ids)
    }

    /// Write one tag onto many files, mapped onto whatever backs its space.
    pub async fn apply(
        db: &DatabaseConnection,
        file_ids: &[Uuid],
        tag: &super::MacroTag,
    ) -> anyhow::Result<()> {
        match tag {
            super::MacroTag::StandardTag(tag_id) => tag_store::link(db, file_ids, &[*tag_id]).await,
            super::MacroTag::FileState(name) => {
                let state = parse_state(name)?;
                file_store::set_file_states(db, file_ids, state).await
            }
            super::MacroTag::Deleted(date_time) => {
                file_store::mark_files_deleted(db, file_ids, *date_time).await
            }
            _ => {
                anyhow::bail!("Read-only macro tag cannot be written to files")
            }
        }
    }

    /// Apply several tags to the same batch of files.
    pub async fn apply_all(
        db: &DatabaseConnection,
        file_ids: &[Uuid],
        tags: &[super::MacroTag],
    ) -> anyhow::Result<()> {
        for tag in tags {
            Self::apply(db, file_ids, tag).await?;
        }
        Ok(())
    }
}
