use crate::group::TrackedWindow;
use crate::platform;
use log::info;

/// Get a list of all pickable windows (visible, real windows)
pub fn get_pickable_windows() -> Vec<TrackedWindow> {
    let mut windows = platform::enumerate_windows();
    // Filter out our own window and system windows
    windows.retain(|w| {
        !w.process_name.to_lowercase().contains("always-bind-window")
            && !w.process_name.to_lowercase().contains("explorer.exe")
            && !w.title.is_empty()
    });
    // Sort by process name then title
    windows.sort_by(|a, b| {
        a.process_name
            .to_lowercase()
            .cmp(&b.process_name.to_lowercase())
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    windows
}

/// Print windows to console (for debugging / CLI mode)
pub fn print_windows(windows: &[TrackedWindow]) {
    info!("Available windows:");
    for (i, w) in windows.iter().enumerate() {
        info!(
            "  [{}] {} - \"{}\" (class: {}, hwnd: {})",
            i, w.process_name, w.title, w.class_name, w.hwnd
        );
    }
}
