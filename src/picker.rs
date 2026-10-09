use crate::group::TrackedWindow;
use crate::platform;
use log::info;

#[cfg(target_os = "windows")]
#[path = "picker_dialog.rs"]
mod dialog;

#[cfg(target_os = "windows")]
pub use dialog::show_window_picker;

#[cfg(not(target_os = "windows"))]
pub fn show_window_picker(_global_sync_move: bool) -> crate::overlay::PickerResult {
    crate::overlay::PickerResult { selected_windows: Vec::new(), cancelled: true, movement: Default::default() }
}

/// Get a list of all pickable windows (visible, real windows)
pub fn get_pickable_windows() -> Vec<TrackedWindow> {
    let mut windows = platform::enumerate_windows();
    // Filter out our own window and system windows
    windows.retain(|w| {
        #[cfg(target_os = "windows")]
        unsafe {
            use windows::Win32::{Foundation::HWND, System::Threading::GetCurrentProcessId,
                UI::WindowsAndMessaging::GetWindowThreadProcessId};
            let mut pid = 0;
            GetWindowThreadProcessId(HWND(w.hwnd as *mut _), Some(&mut pid));
            if pid == GetCurrentProcessId() { return false; }
        }
        // Keep File Explorer folders; exclude only shell surfaces, not explorer.exe.
        !matches!(w.class_name.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
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
