#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
mod macos;

use crate::group::GroupManager;
use std::sync::{Arc, Mutex};

/// Start the platform-specific window monitor
pub fn start_monitor(group_manager: Arc<Mutex<GroupManager>>) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        windows::start_monitor(group_manager)
    }
    #[cfg(target_os = "macos")]
    {
        macos::start_monitor(group_manager)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err("Unsupported platform".into())
    }
}

/// Enumerate all visible windows on the current desktop
pub fn enumerate_windows() -> Vec<crate::group::TrackedWindow> {
    #[cfg(target_os = "windows")]
    {
        windows::enumerate_windows()
    }
    #[cfg(target_os = "macos")]
    {
        macos::enumerate_windows()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Vec::new()
    }
}
