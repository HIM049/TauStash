use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub device_id: String,
    pub db_path: PathBuf,
    pub schedule_enable: bool,
    pub schedule_scan_min: u32,
    pub schedule_maintain_min: u32,
    pub schedule_cleanup_hours: u32,
    pub retention_days: u32,
    pub fps_monitor: bool,
    pub debug_log: bool,
    pub store_enable: bool,
    pub store_auto: StoreAutoMode,
    pub store_path: PathBuf,
    pub locale: String,
}

/// How archived files are brought into the store by the schedule
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreAutoMode {
    #[default]
    Off,
    Copy,
    Move,
}

impl StoreAutoMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Copy, Self::Move];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Copy => "copy",
            Self::Move => "move",
        }
    }

    pub fn from_str(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|mode| mode.as_str() == value)
            .unwrap_or_default()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_id: Uuid::new_v4().to_string(),
            db_path: paths::data_dir().join("taustash.db"),
            schedule_enable: true,
            schedule_scan_min: 30,
            schedule_maintain_min: 60,
            schedule_cleanup_hours: 24,
            retention_days: 30,
            fps_monitor: false,
            debug_log: false,
            store_enable: false,
            store_auto: StoreAutoMode::Off,
            store_path: paths::data_dir().join("store"),
            locale: system_locale(),
        }
    }
}

/// System locale if it exactly matches an available one, otherwise English.
fn system_locale() -> String {
    sys_locale::get_locale()
        .filter(|locale| rust_i18n::available_locales!().iter().any(|l| l == locale))
        .unwrap_or_else(|| "en".to_string())
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let config = serde_json::from_str::<Self>(&content)?;
        Ok(config)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if path.exists() {
            match Self::load(path) {
                Ok(config) => return config,
                Err(err) => {
                    tracing::warn!(
                        "Failed to load config at {:?}: {:#}, fallback to default",
                        path,
                        err
                    );
                }
            }
        }

        let default_config = Self::default();
        if let Err(err) = default_config.save(path) {
            tracing::warn!(
                "Failed to save initial default config to {:?}: {:#}",
                path,
                err
            );
        }
        default_config
    }
}
