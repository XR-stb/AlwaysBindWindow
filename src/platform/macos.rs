// macOS platform implementation
// TODO: Implement using Accessibility API (AXObserver, AXUIElement)
// This requires the "accessibility" entitlement and user permission

use crate::group::{GroupManager, TrackedWindow};
use std::sync::{Arc, Mutex};

pub fn start_monitor(_group_manager: Arc<Mutex<GroupManager>>) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Implement macOS window monitoring
    // Key APIs:
    // - AXObserverCreate / AXObserverAddNotification for window events
    // - kAXFocusedWindowChangedNotification for focus tracking
    // - CGWindowListCopyWindowInfo for window enumeration
    // - AXUIElementPerformAction + kAXRaiseAction for bringing to front
    // - AXUIElementSetAttributeValue + kAXMinimizedAttribute for minimize/restore
    Err("macOS support is not yet implemented. Coming soon!".into())
}

pub fn enumerate_windows() -> Vec<TrackedWindow> {
    // TODO: Use CGWindowListCopyWindowInfo
    Vec::new()
}

pub fn bring_windows_to_front(_hwnds: &[isize]) {
    // TODO: Use AXUIElementPerformAction
}

pub fn minimize_windows(_hwnds: &[isize]) {
    // TODO: Use AXUIElementSetAttributeValue
}

pub fn restore_windows(_hwnds: &[isize]) {
    // TODO: Use AXUIElementSetAttributeValue
}
