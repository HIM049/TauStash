use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

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
