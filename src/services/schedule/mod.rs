pub mod cleanup;
pub mod maintain;
pub mod scanner;
pub mod schedule;

use std::sync::Arc;

pub use cleanup::*;
pub use maintain::*;
pub use scanner::*;
pub use schedule::*;

use crate::config::AppConfig;

pub fn run_schedule(
    schedule: &ScheduleService,
    config: &AppConfig,
    db: &Arc<sea_orm::DatabaseConnection>,
) {
    if config.schedule_enable {
        let dbc = db.clone();
        let device_id = config.device_id.clone();
        schedule.schedule(
            "scan_all_sources",
            std::time::Duration::from_secs(15),
            Some(std::time::Duration::from_secs(
                config.schedule_scan_min as u64 * 60,
            )),
            move || {
                let db = dbc.clone();
                let device_id = device_id.clone();
                async move {
                    let report = super::scanner::scan_all_sources(&db, &device_id)
                        .await
                        .unwrap_or_default();
                    report.has_changes()
                }
            },
        );
        let dbc = db.clone();
        let device_id = config.device_id.clone();
        schedule.schedule(
            "verify_replicas",
            std::time::Duration::from_secs(30),
            Some(std::time::Duration::from_secs(
                config.schedule_maintain_min as u64 * 60,
            )),
            move || {
                let db = dbc.clone();
                let device_id = device_id.clone();
                async move {
                    let report = super::schedule::maintain_storage(&db, &device_id)
                        .await
                        .unwrap_or_default();
                    report.has_changes()
                }
            },
        );
        let dbc = db.clone();
        let device_id = config.device_id.clone();
        let retention_days = config.retention_days;
        schedule.schedule(
            "cleanup_expired_replicas",
            std::time::Duration::from_secs(60),
            Some(std::time::Duration::from_secs(
                config.schedule_cleanup_hours as u64 * 3600,
            )),
            move || {
                let db = dbc.clone();
                let device_id = device_id.clone();
                async move {
                    super::schedule::cleanup_expired_replicas(&db, &device_id, retention_days)
                        .await
                        .unwrap_or_default();
                    false
                }
            },
        );
    } else {
        schedule.cancel("scan_all_sources");
        schedule.cancel("verify_replicas");
        schedule.cancel("cleanup_expired_replicas");
    }
}
