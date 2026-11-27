use gpui_kit::App;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

/// Show or hide the app in the Dock and app switcher; showing also brings it to front.
pub fn set_dock_visible(visible: bool, cx: &App) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let policy = if visible {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    NSApplication::sharedApplication(mtm).setActivationPolicy(policy);
    if visible {
        cx.activate(true);
    }
}
