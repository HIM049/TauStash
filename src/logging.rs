use std::sync::OnceLock;
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*, registry, reload};

static FILTER: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

pub fn init() {
    let (filter, handle) = reload::Layer::new(filter(false));
    registry().with(filter).with(fmt::layer()).init();
    let _ = FILTER.set(handle);
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
            "info,picargo=debug,sqlx=warn"
        } else {
            "info,sqlx=warn"
        })
    })
}
