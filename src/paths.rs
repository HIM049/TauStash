use std::path::PathBuf;

const APP_DIR: &str = "picargo";

/// Platform data directory, fallback to `./app_data`
pub fn config_dir() -> PathBuf {
    app_dir(dirs::config_dir())
}

/// Platform data directory, fallback to `./app_data`
pub fn data_dir() -> PathBuf {
    app_dir(dirs::data_dir())
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn schedule_state_path() -> PathBuf {
    config_dir().join("schedule_state.json")
}

fn app_dir(base: Option<PathBuf>) -> PathBuf {
    base.map(|dir| dir.join(APP_DIR))
        .unwrap_or_else(|| PathBuf::from("app_data"))
}
