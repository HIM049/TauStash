#[cfg(any(windows, target_os = "macos"))]
mod native;
#[cfg(any(windows, target_os = "macos"))]
pub use native::{Tray, init};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{Tray, init};

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod unsupported;
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub use unsupported::{Tray, init};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::set_dock_visible;

/// Show or hide the app in the Dock; only macOS has one.
#[cfg(not(target_os = "macos"))]
pub fn set_dock_visible(_: bool, _: &gpui_kit::App) {}

pub enum TrayCommand {
    Show,
    Quit,
}
