use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::database::storage;
use crate::services::FileState;
use crate::services::tags_queryer::MtagService;

impl MtagService {
    pub async fn apply_changes(
        db: &DatabaseConnection,
        file_ids: &[Uuid],
        ensure: &[super::MacroTag],
        remove: &[super::MacroTag],
    ) -> anyhow::Result<()> {
        if file_ids.is_empty() {
            return Ok(());
        }

        // Process removals first
        for tag in remove {
            match tag {
                super::MacroTag::Standard(meta) => {
                    storage::tag::unlink(db, file_ids, &[meta.id]).await?;
                }
                super::MacroTag::State(FileState::Managed)
                | super::MacroTag::State(FileState::Ignored) => {
                    storage::file::set_file_states(db, file_ids, super::FileState::Pending).await?;
                }
                super::MacroTag::State(FileState::Deleted) => {
                    storage::file::unmark_files_deleted(db, file_ids).await?;
                }
                _ => {}
            }
        }

        // Process additions / ensures
        for tag in ensure {
            match tag {
                super::MacroTag::Standard(meta) => {
                    let tag_record =
                        storage::tag::find_or_create(db, meta.namespace, meta.name.clone()).await?;
                    storage::tag::link(db, file_ids, &[tag_record.id]).await?;
                }

                super::MacroTag::State(FileState::Deleted) => {
                    storage::file::mark_files_deleted(db, file_ids).await?;
                }
                super::MacroTag::State(s) => {
                    storage::file::set_file_states(db, file_ids, *s).await?;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
