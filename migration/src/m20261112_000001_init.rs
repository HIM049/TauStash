use sea_orm_migration::prelude::*;

/// Schema of the 1.0 release.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(File::Table)
                    .col(ColumnDef::new(File::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(File::Hash).string().not_null().unique_key())
                    .col(ColumnDef::new(File::Size).big_integer().not_null())
                    .col(ColumnDef::new(File::Mime).string())
                    .col(ColumnDef::new(File::Ext).string())
                    .col(ColumnDef::new(File::MediaMeta).string())
                    .col(ColumnDef::new(File::State).string().not_null())
                    .col(ColumnDef::new(File::FileName).string().not_null())
                    .col(ColumnDef::new(File::ContentCreatedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(File::ContentModifiedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(File::ExifTakenAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(File::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(File::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(File::DeletedAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await?;
        create_index(manager, "idx-file-state", File::Table, &[File::State], false).await?;
        create_index(
            manager,
            "idx-file-content_created_at",
            File::Table,
            &[File::ContentCreatedAt],
            false,
        )
        .await?;
        create_index(
            manager,
            "idx-file-content_modified_at",
            File::Table,
            &[File::ContentModifiedAt],
            false,
        )
        .await?;
        create_index(manager, "idx-file-created_at", File::Table, &[File::CreatedAt], false)
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FileReplica::Table)
                    .col(
                        ColumnDef::new(FileReplica::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(FileReplica::DeviceId).string().not_null())
                    .col(ColumnDef::new(FileReplica::Uri).string().not_null())
                    .col(ColumnDef::new(FileReplica::State).string().not_null())
                    .col(ColumnDef::new(FileReplica::FileName).string().not_null())
                    .col(ColumnDef::new(FileReplica::Ctime).timestamp_with_time_zone())
                    .col(ColumnDef::new(FileReplica::Utime).timestamp_with_time_zone())
                    .col(ColumnDef::new(FileReplica::FileSize).big_integer())
                    .col(ColumnDef::new(FileReplica::DeletedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(FileReplica::FileId).uuid().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(FileReplica::Table, FileReplica::FileId)
                            .to(File::Table, File::Id),
                    )
                    .to_owned(),
            )
            .await?;
        create_index(
            manager,
            "idx-file_replica-state",
            FileReplica::Table,
            &[FileReplica::State],
            false,
        )
        .await?;
        create_index(
            manager,
            "idx-file_replica-file_id",
            FileReplica::Table,
            &[FileReplica::FileId],
            false,
        )
        .await?;
        create_index(
            manager,
            "idx-file_replica-idx_replicas_device_uri",
            FileReplica::Table,
            &[FileReplica::DeviceId, FileReplica::Uri],
            true,
        )
        .await?;

        manager
            .create_table(
                Table::create()
                    .table(Tag::Table)
                    .col(ColumnDef::new(Tag::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Tag::Namespace).string().not_null())
                    .col(ColumnDef::new(Tag::Name).string().not_null())
                    .col(ColumnDef::new(Tag::Color).string())
                    .col(
                        ColumnDef::new(Tag::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Tag::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Tag::DeletedAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await?;
        create_index(
            manager,
            "idx-tag-idx_tags_namespace_name",
            Tag::Table,
            &[Tag::Namespace, Tag::Name],
            true,
        )
        .await?;

        manager
            .create_table(
                Table::create()
                    .table(FileTag::Table)
                    .col(ColumnDef::new(FileTag::FileId).uuid().not_null())
                    .col(ColumnDef::new(FileTag::TagId).uuid().not_null())
                    .primary_key(
                        Index::create()
                            .name("pk-file_tag")
                            .col(FileTag::FileId)
                            .col(FileTag::TagId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(FileTag::Table, FileTag::FileId)
                            .to(File::Table, File::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(FileTag::Table, FileTag::TagId)
                            .to(Tag::Table, Tag::Id),
                    )
                    .to_owned(),
            )
            .await?;
        create_index(
            manager,
            "idx-file_tag-tag_id",
            FileTag::Table,
            &[FileTag::TagId],
            false,
        )
        .await?;

        manager
            .create_table(
                Table::create()
                    .table(ScanSource::Table)
                    .col(
                        ColumnDef::new(ScanSource::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ScanSource::Path)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(ScanSource::Mode).string().not_null())
                    .col(ColumnDef::new(ScanSource::IsActive).boolean().not_null())
                    .col(ColumnDef::new(ScanSource::InitialTags).json())
                    .col(
                        ColumnDef::new(ScanSource::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ScanSource::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Collection::Table)
                    .col(ColumnDef::new(Collection::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Collection::Name).string().not_null())
                    .col(ColumnDef::new(Collection::Icon).string().not_null())
                    .col(ColumnDef::new(Collection::Cover).string().not_null())
                    .col(ColumnDef::new(Collection::Color).string().not_null())
                    .col(ColumnDef::new(Collection::Order).unsigned().not_null())
                    .col(ColumnDef::new(Collection::ParentId).uuid())
                    .col(ColumnDef::new(Collection::Typ).string().not_null())
                    .col(ColumnDef::new(Collection::Config).json().not_null())
                    .col(
                        ColumnDef::new(Collection::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Collection::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Collection::DeletedAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await?;
        create_index(
            manager,
            "idx-collection-typ",
            Collection::Table,
            &[Collection::Typ],
            false,
        )
        .await?;

        manager
            .create_table(
                Table::create()
                    .table(FileThumbnail::Table)
                    .col(
                        ColumnDef::new(FileThumbnail::FileId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(FileThumbnail::Path).string().not_null())
                    .col(ColumnDef::new(FileThumbnail::Size).big_integer().not_null())
                    .col(
                        ColumnDef::new(FileThumbnail::CreateAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(FileThumbnail::Table, FileThumbnail::FileId)
                            .to(File::Table, File::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            FileThumbnail::Table.into_iden(),
            Collection::Table.into_iden(),
            ScanSource::Table.into_iden(),
            FileTag::Table.into_iden(),
            Tag::Table.into_iden(),
            FileReplica::Table.into_iden(),
            File::Table.into_iden(),
        ] {
            manager.drop_table(Table::drop().table(table).to_owned()).await?;
        }
        Ok(())
    }
}

async fn create_index<T, C>(
    manager: &SchemaManager<'_>,
    name: &str,
    table: T,
    cols: &[C],
    unique: bool,
) -> Result<(), DbErr>
where
    T: IntoIden,
    C: IntoIden + Copy,
{
    let mut index = Index::create();
    index.name(name).table(table);
    for col in cols {
        index.col(*col);
    }
    if unique {
        index.unique();
    }
    manager.create_index(index.to_owned()).await
}

#[derive(DeriveIden, Clone, Copy)]
enum File {
    Table,
    Id,
    Hash,
    Size,
    Mime,
    Ext,
    MediaMeta,
    State,
    FileName,
    ContentCreatedAt,
    ContentModifiedAt,
    ExifTakenAt,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
}

#[derive(DeriveIden, Clone, Copy)]
enum FileReplica {
    Table,
    Id,
    DeviceId,
    Uri,
    State,
    FileName,
    Ctime,
    Utime,
    FileSize,
    DeletedAt,
    FileId,
}

#[derive(DeriveIden, Clone, Copy)]
enum Tag {
    Table,
    Id,
    Namespace,
    Name,
    Color,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
}

#[derive(DeriveIden, Clone, Copy)]
enum FileTag {
    Table,
    FileId,
    TagId,
}

#[derive(DeriveIden, Clone, Copy)]
enum ScanSource {
    Table,
    Id,
    Path,
    Mode,
    IsActive,
    InitialTags,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden, Clone, Copy)]
enum Collection {
    Table,
    Id,
    Name,
    Icon,
    Cover,
    Color,
    Order,
    ParentId,
    Typ,
    Config,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
}

#[derive(DeriveIden, Clone, Copy)]
enum FileThumbnail {
    Table,
    FileId,
    Path,
    Size,
    CreateAt,
}
