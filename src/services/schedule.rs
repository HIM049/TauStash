use sea_orm::DatabaseConnection;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::database::entities::{FileState, ReplicaState, file, file_replica};
use crate::database::storage::{self, file::ContentTimes};
use crate::services::{importer, thumbnail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInfo {
    pub id: String,
    pub next_run: Instant,
    pub interval: Option<Duration>,
}

pub type TaskAction =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync + 'static>;

#[derive(Clone)]
pub struct ScheduledTask {
    pub id: String,
    pub next_run: Instant,
    pub interval: Option<Duration>,
    pub action: TaskAction,
}

impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.next_run == other.next_run
    }
}

impl Eq for ScheduledTask {}

impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.next_run.cmp(&self.next_run)
    }
}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct ScheduleService {
    worker_handle: Option<JoinHandle<()>>,
    queue: Arc<std::sync::Mutex<BinaryHeap<Reverse<ScheduledTask>>>>,
    notify: Arc<Notify>,
}

impl ScheduleService {
    pub fn new(on_file_changed: impl Fn() + Send + Sync + 'static) -> Self {
        let queue = Arc::new(std::sync::Mutex::new(BinaryHeap::new()));
        let notify = Arc::new(Notify::new());

        let queue_clone = Arc::clone(&queue);
        let notify_clone = Arc::clone(&notify);
        let on_file_changed = Arc::new(on_file_changed);

        let worker_handle = tokio::spawn(async move {
            Self::worker_loop(queue_clone, notify_clone, on_file_changed).await;
        });

        Self {
            worker_handle: Some(worker_handle),
            queue,
            notify,
        }
    }

    pub fn schedule<F, Fut>(
        &self,
        id: impl Into<String>,
        delay: Duration,
        interval: Option<Duration>,
        action: F,
    ) where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        let id = id.into();
        let action: TaskAction = Arc::new(move || Box::pin(action()));
        let task = ScheduledTask {
            id,
            next_run: Instant::now() + delay,
            interval,
            action,
        };

        let mut q = self.queue.lock().unwrap();
        let remaining: Vec<_> = q.drain().filter(|Reverse(t)| t.id != task.id).collect();
        for item in remaining {
            q.push(item);
        }
        q.push(Reverse(task));
        drop(q);
        self.notify.notify_one();
    }

    pub fn trigger_now(&self, id: &str) {
        let mut q = self.queue.lock().unwrap();
        let mut updated = Vec::new();
        for Reverse(mut task) in q.drain() {
            if task.id == id {
                task.next_run = Instant::now();
            }
            updated.push(Reverse(task));
        }
        for item in updated {
            q.push(item);
        }
        drop(q);
        self.notify.notify_one();
    }

    pub fn cancel(&self, id: &str) {
        let mut q = self.queue.lock().unwrap();
        let remaining: Vec<_> = q.drain().filter(|Reverse(t)| t.id != id).collect();
        for item in remaining {
            q.push(item);
        }
        drop(q);
        self.notify.notify_one();
    }

    pub fn pending_tasks(&self) -> Vec<TaskInfo> {
        let q = self.queue.lock().unwrap();
        q.iter()
            .map(|Reverse(t)| TaskInfo {
                id: t.id.clone(),
                next_run: t.next_run,
                interval: t.interval,
            })
            .collect()
    }

    pub fn stop(&mut self) {
        if let Some(handle) = self.worker_handle.take() {
            handle.abort();
        }
    }

    async fn worker_loop(
        queue: Arc<std::sync::Mutex<BinaryHeap<Reverse<ScheduledTask>>>>,
        notify: Arc<Notify>,
        on_file_changed: Arc<dyn Fn() + Send + Sync + 'static>,
    ) {
        loop {
            let sleep_duration = {
                let q = queue.lock().unwrap();
                match q.peek() {
                    Some(Reverse(task)) => {
                        let now = Instant::now();
                        if task.next_run <= now {
                            Duration::ZERO
                        } else {
                            task.next_run - now
                        }
                    }
                    None => Duration::from_secs(86400 * 365),
                }
            };

            tokio::select! {
                _ = tokio::time::sleep(sleep_duration), if sleep_duration > Duration::ZERO => {}
                _ = notify.notified() => { continue; }
            }

            let task_to_run = {
                let mut q = queue.lock().unwrap();
                if let Some(Reverse(task)) = q.peek() {
                    if task.next_run <= Instant::now() {
                        q.pop().map(|Reverse(t)| t)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            let Some(mut task) = task_to_run else {
                continue;
            };

            tracing::debug!("Running scheduled task: {}", task.id);
            let has_changes = (task.action)().await;

            if has_changes {
                on_file_changed();
            }

            if let Some(interval) = task.interval {
                task.next_run = Instant::now() + interval;
                let mut q = queue.lock().unwrap();
                q.push(Reverse(task));
            }
        }
    }
}

impl Drop for ScheduleService {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Clone, Default)]
pub struct MaintenanceReport {
    pub total: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub refound: usize,
    pub updated: usize,
}

impl MaintenanceReport {
    pub fn has_changes(&self) -> bool {
        self.missing > 0 || self.refound > 0 || self.updated > 0
    }
}

/// Update creation time by MIN(ctime) and modification time by MAX(utime)
/// from active local replicas into the file table.
pub async fn update_content_times(db: &DatabaseConnection) -> anyhow::Result<usize> {
    use sea_orm::ConnectionTrait;

    let sql = r#"
        UPDATE file
        SET
            content_created_at = (
                SELECT MIN(ctime) FROM file_replica
                WHERE file_replica.file_id = file.id
                  AND file_replica.deleted_at IS NULL
                  AND file_replica.state = 'available'
            ),
            content_modified_at = (
                SELECT MAX(utime) FROM file_replica
                WHERE file_replica.file_id = file.id
                  AND file_replica.deleted_at IS NULL
                  AND file_replica.state = 'available'
            )
        WHERE file.deleted_at IS NULL
          AND EXISTS (
              SELECT 1 FROM file_replica
              WHERE file_replica.file_id = file.id
                AND file_replica.deleted_at IS NULL
                AND file_replica.state = 'available'
          );
    "#;

    let res = db.execute_unprepared(sql).await?;
    Ok(res.rows_affected() as usize)
}

/// Run verification and metadata calibration on replicas.
pub async fn maintain_storage(
    db: &DatabaseConnection,
    device_id: &str,
) -> anyhow::Result<MaintenanceReport> {
    let mut report = MaintenanceReport::default();

    let replicas = file_replica::Entity::find()
        .filter(file_replica::Column::DeviceId.eq(device_id))
        .filter(file_replica::Column::DeletedAt.is_null())
        .all(db)
        .await?;

    for replica in replicas {
        report.total += 1;
        let path = Path::new(&replica.uri);

        let Ok(metadata) = std::fs::metadata(path) else {
            if replica.state != ReplicaState::Missing {
                if storage::file::mark_replica_missing(db, replica)
                    .await
                    .is_ok()
                {
                    report.missing += 1;
                }
            }
            continue;
        };

        let current_size = metadata.len() as i64;
        let current_ctime = metadata.created().ok().and_then(importer::to_utc);
        let current_utime = metadata.modified().ok().and_then(importer::to_utc);

        let refound = replica.state == ReplicaState::Missing;
        let size_matches = replica.file_size == Some(current_size);
        let utime_matches = replica.utime == current_utime;
        let ctime_matches = replica.ctime == current_ctime;

        // UNCHANGED file
        if size_matches && utime_matches && ctime_matches {
            // if UNCHANGED but MISSED before
            if refound {
                let mut active: file_replica::ActiveModel = replica.into();
                active.state = Set(ReplicaState::Available);
                active.ctime = Set(current_ctime);
                active.update(db).await?;
                report.refound += 1;
            } else {
                report.unchanged += 1;
            }
            continue;
        }

        let new_file_meta = match importer::compute_file_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to recompute metadata for {}: {e}", path.display());
                continue;
            }
        };

        let current_file = file::Entity::find_by_id(replica.file_id).one(db).await?;
        let is_unchanged = current_file
            .as_ref()
            .map(|f| f.hash == new_file_meta.hash)
            .unwrap_or(false);

        if is_unchanged {
            // Content unchanged
            let mut active: file_replica::ActiveModel = replica.into();
            active.state = Set(ReplicaState::Available);
            active.ctime = Set(new_file_meta.ctime);
            active.utime = Set(new_file_meta.utime);
            active.file_size = Set(Some(new_file_meta.size));
            active.update(db).await?;

            if refound {
                report.refound += 1;
            } else {
                report.updated += 1;
            }
        } else {
            // Hash mismatch
            let target_file = file::Entity::find()
                .filter(file::Column::Hash.eq(&new_file_meta.hash))
                .one(db)
                .await?;

            let target_file_id = if let Some(target) = target_file {
                target.id
            } else {
                let new_id = Uuid::now_v7();
                let initial_times = ContentTimes {
                    created_at: new_file_meta.ctime,
                    modified_at: new_file_meta.utime,
                };
                let new_file = storage::file::create_file(
                    db,
                    storage::file::CreateFileParams {
                        id: new_id,
                        file_name: &replica.file_name,
                        hash: &new_file_meta.hash,
                        size: new_file_meta.size,
                        mime: Some(new_file_meta.mime.clone()),
                        ext: new_file_meta.ext.clone(),
                        state: FileState::Managed,
                        content_times: initial_times,
                    },
                )
                .await?;

                if new_file_meta.mime.starts_with("image/") {
                    let _ = thumbnail::ensure_thumbnail(path, &new_file_meta.hash);
                }
                new_file.id
            };

            let mut active: file_replica::ActiveModel = replica.into();
            active.file_id = Set(target_file_id);
            active.state = Set(ReplicaState::Available);
            active.ctime = Set(new_file_meta.ctime);
            active.utime = Set(new_file_meta.utime);
            active.file_size = Set(Some(new_file_meta.size));
            active.update(db).await?;
            report.updated += 1;
        }
    }

    update_content_times(db).await?;

    Ok(report)
}
