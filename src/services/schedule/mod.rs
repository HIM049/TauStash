pub mod cleanup;
pub mod maintain;
pub mod scanner;
pub mod schedule;
pub mod state;
pub mod store;
pub mod thumbnail;

use std::sync::Arc;

pub use cleanup::*;
pub use maintain::*;
pub use scanner::*;
pub use schedule::*;
pub use state::*;
pub use store::*;
pub use thumbnail::*;

use crate::config::{AppConfig, StoreAutoMode};

pub fn run_schedule(
    schedule: &ScheduleService,
    config: &AppConfig,
    db: &Arc<sea_orm::DatabaseConnection>,
) {
    if config.schedule_enable {
        let state = ScheduleState::load_default();

        let dbc = db.clone();
        let device_id = config.device_id.clone();
        let scan_interval = std::time::Duration::from_secs(config.schedule_scan_min as u64 * 60);
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

        let dbc = db.clone();
        let device_id = config.device_id.clone();
        let thumb_delay = state.compute_initial_delay(
            "maintain_thumbnails",
            maintain_interval,
            std::time::Duration::from_secs(10),
        );
        schedule.schedule(
            "maintain_thumbnails",
            thumb_delay,
            Some(maintain_interval),
            move || {
                let db = dbc.clone();
                let device_id = device_id.clone();
                async move {
                    let report = super::schedule::maintain_thumbnails(&db, &device_id)
                        .await
                        .unwrap_or_default();
                    report.has_changes()
                }
            },
        );

        if config.store_enable && config.store_auto != StoreAutoMode::Off {
            let dbc = db.clone();
            let device_id = config.device_id.clone();
            let store_path = config.store_path.clone();
            let mode = config.store_auto;
            let store_delay = state.compute_initial_delay(
                "store_archived_files",
                maintain_interval,
                std::time::Duration::from_secs(45),
            );
            schedule.schedule(
                "store_archived_files",
                store_delay,
                Some(maintain_interval),
                move || {
                    let db = dbc.clone();
                    let device_id = device_id.clone();
                    let store_path = store_path.clone();
                    async move {
                        let report = super::schedule::store_archived_files(
                            &db,
                            &device_id,
                            &store_path,
                            mode,
                        )
                        .await
                        .unwrap_or_default();
                        report.has_changes()
                    }
                },
            );
        } else {
            schedule.cancel("store_archived_files");
        }
    } else {
        schedule.cancel("scan_all_sources");
        schedule.cancel("verify_replicas");
        schedule.cancel("cleanup_expired_replicas");
        schedule.cancel("maintain_thumbnails");
        schedule.cancel("store_archived_files");
    }
}
