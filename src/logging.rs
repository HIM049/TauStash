use std::fs::{self, File, OpenOptions};
use std::sync::OnceLock;
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*, registry, reload};

use crate::paths;

static FILTER: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

pub fn init() {
    let (filter, handle) = reload::Layer::new(filter(false));
    let file_layer = open_log_file().map(|file| fmt::layer().with_ansi(false).with_writer(file));
    registry()
        .with(filter)
        .with(fmt::layer())
        .with(file_layer)
        .init();
    let _ = FILTER.set(handle);

    // Panic log hook
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("{info}\n{}", std::backtrace::Backtrace::force_capture());
        default_hook(info);
    }));
}

fn open_log_file() -> Option<File> {
    let path = paths::log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => Some(file),
        Err(err) => {
            eprintln!("Failed to open log file {}: {err}", path.display());
            None
        }
    }
}

/// Switch this crate between debug and info logs.
pub fn set_debug(debug: bool) {
    let Some(handle) = FILTER.get() else {
        return;
    };
    if let Err(err) = handle.reload(filter(debug)) {
        tracing::error!("Failed to reload log filter: {err}");
    }
}

fn filter(debug: bool) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(if debug {
            "info,taustash=debug,sqlx=warn"
        } else {
            "info,sqlx=warn"
        })
    })
}
