use std::collections::{HashMap, HashSet};

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, QueryTrait, Statement,
    prelude::DateTimeUtc,
    sea_query::{Alias, Expr, ExprTrait, Func, Query, SqliteQueryBuilder},
};
use uuid::Uuid;

use super::file::DbFileRecord;
use crate::database::entities::{
    FileState, ReplicaState, TagNamespace, file, file_replica, file_tag, tag,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MatchMode {
    /// Match all tags include the set. (AND)
    #[default]
    All,
    /// Match any tag include the set. (OR)
    Any,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagCriterion {
    Untagged,
    IncludeTags {
        tags: HashSet<Uuid>,
        mode: MatchMode,
    },
    ExcludeTags(HashSet<Uuid>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FileSortField {
    #[default]
    CreatedAt,
    ContentCreatedAt,
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pagination {
    pub limit: u64,
    pub offset: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagFacetItem {
    pub tag_id: Uuid,
    pub namespace: TagNamespace,
    pub name: String,
    pub color: Option<String>,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FileQueryStats {
    pub total_files: usize,
    pub untagged_files: usize,
    pub tags_summary: Vec<TagFacetItem>,
}

#[derive(Clone, Debug, Default)]
pub struct FileQueryer {
    pub state: Option<FileState>,
    pub tag_criteria: Vec<TagCriterion>,
    pub search_term: Option<String>,
    pub extensions: Option<Vec<String>>, // Whitelist
    pub mime_prefixes: Option<Vec<String>>,
    pub has_replica: bool,
    pub deleted_since: Option<DateTimeUtc>,
    pub size_range: (Option<i64>, Option<i64>),
    pub date_range: Option<(Option<DateTimeUtc>, Option<DateTimeUtc>)>,
    pub sort_field: FileSortField,
    pub sort_direction: SortDirection,
    pub pagination: Option<Pagination>,
}

impl FileQueryer {
    pub fn new() -> Self {
        Self::default()
    }

    /// STATE filter
    pub fn state(mut self, state: FileState) -> Self {
        self.state = Some(state);
        self
    }

    /// TAGS filter
    pub fn include_tags(mut self, tags: impl IntoIterator<Item = Uuid>, mode: MatchMode) -> Self {
        let set: HashSet<Uuid> = tags.into_iter().collect();
        if !set.is_empty() {
            self.tag_criteria
                .push(TagCriterion::IncludeTags { tags: set, mode });
        }
        self
    }

    /// Filter by a tag
    pub fn with_tag(self, tag_id: Uuid) -> Self {
        self.include_tags([tag_id], MatchMode::All)
    }

    /// EXCLUDE tag filter
    pub fn exclude_tags(mut self, tags: impl IntoIterator<Item = Uuid>) -> Self {
        let set: HashSet<Uuid> = tags.into_iter().collect();
        if !set.is_empty() {
            self.tag_criteria.push(TagCriterion::ExcludeTags(set));
        }
        self
    }

    /// ONLY untagged file
    pub fn untagged(mut self) -> Self {
        self.tag_criteria.push(TagCriterion::Untagged);
        self
    }

    /// Filter files that have multiple physical replicas (duplicate copies)
    pub fn has_replica(mut self, has: bool) -> Self {
        self.has_replica = has;
        self
    }

    /// Filter by soft deleted timestamp
    pub fn deleted_since(mut self, since: DateTimeUtc) -> Self {
        self.deleted_since = Some(since);
        self
    }

    /// Basic search by filename
    pub fn search(mut self, term: impl Into<String>) -> Self {
        let s = term.into().trim().to_string();
        self.search_term = if s.is_empty() { None } else { Some(s) };
        self
    }

    /// EXT whitelist
    pub fn extensions<I, S>(mut self, exts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let list: Vec<String> = exts
            .into_iter()
            .map(|s| s.as_ref().trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        self.extensions = if list.is_empty() { None } else { Some(list) };
        self
    }

    /// MIME prefix filter (e.g. "image", "video")
    pub fn mime_prefixes<I, S>(mut self, prefixes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let list: Vec<String> = prefixes
            .into_iter()
            .map(|s| s.as_ref().trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        self.mime_prefixes = if list.is_empty() { None } else { Some(list) };
        self
    }

    /// Add a single MIME prefix filter (e.g. "image")
    pub fn mime_prefix(mut self, prefix: impl Into<String>) -> Self {
        let p = prefix.into().trim().to_lowercase();
        if !p.is_empty() {
            self.mime_prefixes
                .get_or_insert_with(Vec::new)
                .push(p);
        }
        self
    }

    /// SIZE filter (min..=max)
    pub fn size(mut self, min: Option<i64>, max: Option<i64>) -> Self {
        self.size_range = (min, max);
        self
    }

    /// TIME filter (start..=end)
    pub fn date_range(mut self, start: Option<DateTimeUtc>, end: Option<DateTimeUtc>) -> Self {
        self.date_range = Some((start, end));
        self
    }

    /// SORT config
    pub fn sort(mut self, field: FileSortField, dir: SortDirection) -> Self {
        self.sort_field = field;
        self.sort_direction = dir;
        self
    }

    /// Pegination settings
    pub fn paginate(mut self, limit: u64, offset: u64) -> Self {
        self.pagination = Some(Pagination { limit, offset });
        self
    }

    pub async fn count_file_query(&self, db: &DatabaseConnection) -> anyhow::Result<u64> {
        let mut unpaged_spec = self.clone();
        unpaged_spec.pagination = None;

        let select = build_file_select(&unpaged_spec);
        let count = select.count(db).await?;
        Ok(count)
    }

    /// Get all atats by query paramters
    pub async fn query_stats(&self, db: &DatabaseConnection) -> anyhow::Result<FileQueryStats> {
        let matched_cond = build_filter_conditions(self);
        let matched_subquery = file::Entity::find()
            .select_only()
            .column(file::Column::Id)
            .filter(matched_cond)
            .into_query();

        // get all counts by LEFT JOIN
        let (sql, values) = Query::select()
            .expr_as(
                Expr::col((tag::Entity, tag::Column::Id)),
                Alias::new("tag_id"),
            )
            .expr_as(
                Expr::col((tag::Entity, tag::Column::Namespace)),
                Alias::new("namespace"),
            )
            .expr_as(
                Expr::col((tag::Entity, tag::Column::Name)),
                Alias::new("name"),
            )
            .expr_as(
                Expr::col((tag::Entity, tag::Column::Color)),
                Alias::new("color"),
            )
            .expr_as(
                Func::count_distinct(Expr::col((file::Entity, file::Column::Id))),
                Alias::new("cnt"),
            )
            .from(file::Entity)
            .left_join(
                file_tag::Entity,
                Expr::col((file::Entity, file::Column::Id))
                    .equals((file_tag::Entity, file_tag::Column::FileId)),
            )
            .left_join(
                tag::Entity,
                Expr::col((file_tag::Entity, file_tag::Column::TagId))
                    .equals((tag::Entity, tag::Column::Id)),
            )
            .and_where(Expr::col((file::Entity, file::Column::Id)).in_subquery(matched_subquery))
            .group_by_col((tag::Entity, tag::Column::Id))
            .build(SqliteQueryBuilder);

        let rows = db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                &sql,
                values,
            ))
            .await?;

        let mut untagged_files = 0;
        let mut tags_summary = Vec::new();

        for row in rows {
            let cnt: i64 = row.try_get("", "cnt")?;
            let tag_id_opt = row.try_get::<Option<Uuid>>("", "tag_id").ok().flatten();

            match tag_id_opt {
                Some(tag_id) => {
                    let ns_str: String = row.try_get("", "namespace")?;
                    let name: String = row.try_get("", "name")?;
                    let color: Option<String> = row.try_get("", "color").ok();
                    let namespace = match ns_str.as_str() {
                        "rule" => TagNamespace::Rule,
                        "ai" => TagNamespace::Ai,
                        _ => TagNamespace::User,
                    };
                    tags_summary.push(TagFacetItem {
                        tag_id,
                        namespace,
                        name,
                        color,
                        count: cnt as usize,
                    });
                }
                None => {
                    untagged_files = cnt as usize;
                }
            }
        }

        // Sort tags by count and name
        tags_summary.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));

        let total_files = self.count_file_query(db).await? as usize;

        Ok(FileQueryStats {
            total_files,
            untagged_files,
            tags_summary,
        })
    }

    /// run query
    pub async fn execute(&self, db: &DatabaseConnection) -> anyhow::Result<Vec<DbFileRecord>> {
        let select = build_file_select(self);
        let files = select.all(db).await?;

        if files.is_empty() {
            return Ok(Vec::new());
        }

        let file_ids: Vec<Uuid> = files.iter().map(|f| f.id).collect();

        // load replica not missed
        let replicas = file_replica::Entity::find()
            .filter(file_replica::Column::FileId.is_in(file_ids))
            .all(db)
            .await?;

        let replica_map: HashMap<Uuid, file_replica::Model> = replicas
            .into_iter()
            .filter(|r| r.state != ReplicaState::Missing)
            .map(|r| (r.file_id, r))
            .collect();

        let records = files
            .into_iter()
            .filter_map(|f| {
                let replica = replica_map.get(&f.id)?;
                Some(DbFileRecord {
                    file_id: f.id,
                    file_name: replica.file_name.clone(),
                    path: replica.uri.clone(),
                    hash: f.hash,
                    size: f.size,
                    mime: f
                        .mime
                        .unwrap_or_else(|| "application/octet-stream".to_string()),
                    ext: f.ext,
                    state: f.state,
                })
            })
            .collect();

        Ok(records)
    }
}

fn build_filter_conditions(spec: &FileQueryer) -> sea_orm::Condition {
    let mut cond = sea_orm::Condition::all();

    let alive_replica_subquery = Query::select()
        .column(file_replica::Column::FileId)
        .from(file_replica::Entity)
        .and_where(Expr::col(file_replica::Column::State).ne("missing"))
        .to_owned();
    cond = cond.add(Expr::col(file::Column::Id).in_subquery(alive_replica_subquery));

    // STATE filter
    if let Some(state) = spec.state {
        cond = cond.add(file::Column::State.eq(state));
    }

    // EXT filter
    if let Some(ref exts) = spec.extensions {
        cond = cond.add(file::Column::Ext.is_in(exts.clone()));
    }

    // MIME PREFIX filter
    if let Some(ref prefixes) = spec.mime_prefixes {
        let mut mime_cond = sea_orm::Condition::any();
        for prefix in prefixes {
            let pattern = format!("{prefix}/%");
            mime_cond = mime_cond.add(file::Column::Mime.like(&pattern));
        }
        cond = cond.add(mime_cond);
    }

    // REPLICA COUNT filter: filter files with multiple replicas (> 1)
    if spec.has_replica {
        let replica_count_subquery = Query::select()
            .column(file_replica::Column::FileId)
            .from(file_replica::Entity)
            .and_where(Expr::col(file_replica::Column::State).ne("missing"))
            .group_by_col(file_replica::Column::FileId)
            .and_having(
                Expr::expr(Func::count(Expr::col(file_replica::Column::Id))).gt(1),
            )
            .to_owned();

        cond = cond.add(Expr::col(file::Column::Id).in_subquery(replica_count_subquery));
    }

    // DELETED filter (replicas marked deleted)
    if let Some(since) = spec.deleted_since {
        let deleted_subquery = Query::select()
            .column(file_replica::Column::FileId)
            .from(file_replica::Entity)
            .and_where(Expr::col(file_replica::Column::DeletedAt).gte(since))
            .to_owned();

        cond = cond.add(Expr::col(file::Column::Id).in_subquery(deleted_subquery));
    }

    // SIZE filter
    let (min_size, max_size) = spec.size_range;
    if let Some(min) = min_size {
        cond = cond.add(file::Column::Size.gte(min));
    }
    if let Some(max) = max_size {
        cond = cond.add(file::Column::Size.lte(max));
    }

    // TIME filter
    if let Some((start_date, end_date)) = spec.date_range {
        if let Some(start) = start_date {
            cond = cond.add(file::Column::CreatedAt.gte(start));
        }
        if let Some(end) = end_date {
            cond = cond.add(file::Column::CreatedAt.lte(end));
        }
    }

    // FILENAME search
    if let Some(ref term) = spec.search_term {
        let pattern = format!("%{term}%");

        let name_subquery = Query::select()
            .column(file_replica::Column::FileId)
            .from(file_replica::Entity)
            .and_where(Expr::col(file_replica::Column::FileName).like(&pattern))
            .to_owned();

        cond = cond.add(
            sea_orm::Condition::any().add(Expr::col(file::Column::Id).in_subquery(name_subquery)),
        );
    }

    // Tag criteria
    for criterion in &spec.tag_criteria {
        match criterion {
            TagCriterion::Untagged => {
                let tagged_subquery = Query::select()
                    .column(file_tag::Column::FileId)
                    .from(file_tag::Entity)
                    .to_owned();
                cond = cond.add(Expr::col(file::Column::Id).not_in_subquery(tagged_subquery));
            }
            TagCriterion::IncludeTags { tags, mode } => {
                if tags.is_empty() {
                    continue;
                }
                let tag_ids: Vec<Uuid> = tags.iter().copied().collect();
                match mode {
                    MatchMode::All => {
                        let all_subquery = Query::select()
                            .column(file_tag::Column::FileId)
                            .from(file_tag::Entity)
                            .and_where(Expr::col(file_tag::Column::TagId).is_in(tag_ids))
                            .group_by_col(file_tag::Column::FileId)
                            .and_having(
                                Expr::expr(Func::count_distinct(Expr::col(
                                    file_tag::Column::TagId,
                                )))
                                .eq(tags.len() as i32),
                            )
                            .to_owned();
                        cond = cond.add(Expr::col(file::Column::Id).in_subquery(all_subquery));
                    }
                    MatchMode::Any => {
                        let any_subquery = Query::select()
                            .column(file_tag::Column::FileId)
                            .from(file_tag::Entity)
                            .and_where(Expr::col(file_tag::Column::TagId).is_in(tag_ids))
                            .to_owned();
                        cond = cond.add(Expr::col(file::Column::Id).in_subquery(any_subquery));
                    }
                }
            }
            TagCriterion::ExcludeTags(tags) => {
                if !tags.is_empty() {
                    let tag_ids: Vec<Uuid> = tags.iter().copied().collect();
                    let not_subquery = Query::select()
                        .column(file_tag::Column::FileId)
                        .from(file_tag::Entity)
                        .and_where(Expr::col(file_tag::Column::TagId).is_in(tag_ids))
                        .to_owned();
                    cond = cond.add(Expr::col(file::Column::Id).not_in_subquery(not_subquery));
                }
            }
        }
    }

    cond
}

fn build_file_select(spec: &FileQueryer) -> sea_orm::Select<file::Entity> {
    let cond = build_filter_conditions(spec);
    let mut query = file::Entity::find().filter(cond);

    // SORT
    let sort_col = match spec.sort_field {
        FileSortField::CreatedAt => file::Column::CreatedAt,
        FileSortField::ContentCreatedAt => file::Column::ContentCreatedAt,
        FileSortField::Size => file::Column::Size,
    };
    query = match spec.sort_direction {
        SortDirection::Asc => query.order_by_asc(sort_col),
        SortDirection::Desc => query.order_by_desc(sort_col),
    };

    // Pagination
    if let Some(ref p) = spec.pagination {
        query = query.limit(p.limit).offset(p.offset);
    }

    query
}
