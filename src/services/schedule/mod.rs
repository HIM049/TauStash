pub mod cleanup;
pub mod maintain;
pub mod scanner;
pub mod schedule;
pub mod state;

use std::sync::Arc;

pub use cleanup::*;
pub use maintain::*;
pub use scanner::*;
pub use schedule::*;
pub use state::*;

use crate::config::AppConfig;

pub fn run_schedule(
    schedule: &ScheduleService,
    config: &AppConfig,
    db: &Arc<sea_orm::DatabaseConnection>,
) {
    if config.schedule_enable {
        let state = ScheduleState::load_default();

        let dbc = db.clone();
        let device_id = config.device_id.clone();
        let scan_interval =
            std::time::Duration::from_secs(config.schedule_scan_min as u64 * 60);
        let scan_delay = state.compute_initial_delay(
            "scan_all_sources",
            scan_interval,
            std::time::Duration::from_secs(15),
        );
        schedule.schedule(
            "scan_all_sources",
            scan_delay,
            Some(scan_interval),
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
        let maintain_interval =
            std::time::Duration::from_secs(config.schedule_maintain_min as u64 * 60);
        let maintain_delay = state.compute_initial_delay(
            "verify_replicas",
            maintain_interval,
            std::time::Duration::from_secs(30),
        );
        schedule.schedule(
            "verify_replicas",
            maintain_delay,
            Some(maintain_interval),
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
        let cleanup_interval =
            std::time::Duration::from_secs(config.schedule_cleanup_hours as u64 * 3600);
        let cleanup_delay = state.compute_initial_delay(
            "cleanup_expired_replicas",
            cleanup_interval,
            std::time::Duration::from_secs(60),
        );
        schedule.schedule(
            "cleanup_expired_replicas",
            cleanup_delay,
            Some(cleanup_interval),
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
