use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScheduleState(pub HashMap<String, DateTime<Utc>>);

impl ScheduleState {
    pub fn load(path: impl AsRef<Path>) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn load_default() -> Self {
        Self::load(paths::schedule_state_path())
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }
        let json = serde_json::to_string_pretty(&self.0)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn save_default(&self) -> anyhow::Result<()> {
        self.save(paths::schedule_state_path())
    }

    pub fn record_run(&mut self, task_id: &str, path: impl AsRef<Path>) {
        self.0.insert(task_id.to_string(), Utc::now());
        if let Err(e) = self.save(path) {
            tracing::error!("Failed to persist schedule state: {e:#}");
        }
    }

    pub fn record_run_default(&mut self, task_id: &str) {
        self.record_run(task_id, paths::schedule_state_path());
    }

    pub fn last_run(&self, task_id: &str) -> Option<DateTime<Utc>> {
        self.0.get(task_id).copied()
    }

    /// Computes the initial delay on application startup.
    pub fn compute_initial_delay(
        &self,
        task_id: &str,
        interval: std::time::Duration,
        min_delay: std::time::Duration,
    ) -> std::time::Duration {
        if let Some(last_run) = self.0.get(task_id) {
            let elapsed = Utc::now().signed_duration_since(*last_run);
            let elapsed_secs = elapsed.num_seconds().max(0) as u64;
            let interval_secs = interval.as_secs();

            if elapsed_secs >= interval_secs {
                min_delay
            } else {
                std::time::Duration::from_secs(interval_secs - elapsed_secs).max(min_delay)
            }
        } else {
            min_delay
        }
    }
}
