use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const DEFAULT_CONFIG_PATH: &str = "./app_data/config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub device_id: String,
    pub db_path: PathBuf,
    pub schedule_enable: bool,
    pub schedule_scan_min: u32,
    pub schedule_maintain_min: u32,
    pub fps_monitor: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_id: Uuid::new_v4().to_string(),
            db_path: PathBuf::from("./app_data/picargo.db"),
            schedule_enable: true,
            schedule_scan_min: 30,
            schedule_maintain_min: 60,
            fps_monitor: false,
        }
    }
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
