use super::TrayCommand;
use tokio::sync::mpsc::UnboundedSender;

/// Placeholder handle on platforms without a tray backend.
pub struct Tray;

/// Always fails, no tray backend on this platform.
pub fn init(_: UnboundedSender<TrayCommand>) -> anyhow::Result<Tray> {
    anyhow::bail!("Tray is not supported on this platform")
}
