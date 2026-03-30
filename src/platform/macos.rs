/// macOS platform implementation.
///
/// Window enumeration: CGWindowListCopyWindowInfo
/// Window operations: AppleScript via osascript (reliable, no entitlement needed)
/// Move sync: polling thread same as Windows
/// Foreground tracking: NSWorkspace notifications via polling

use crate::group::{GroupManager, TrackedWindow};
use log::info;
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use core_graphics::display::*;
use core_foundation::base::*;
use core_foundation::dictionary::*;
use core_foundation::number::*;
use core_foundation::string::*;

static SUPPRESSED: AtomicBool = AtomicBool::new(false);
static MOVE_CACHE_DIRTY: AtomicBool = AtomicBool::new(false);
static MOVE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Get window list from CGWindowListCopyWindowInfo
pub fn enumerate_windows() -> Vec<TrackedWindow> {
    let mut result = Vec::new();

    unsafe {
        let window_list = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        );
        if window_list.is_null() { return result; }

        let count = CFArrayGetCount(window_list as _);
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(window_list as _, i) as CFDictionaryRef;
            if dict.is_null() { continue; }

            // Get window ID
            let wid = get_cf_number(dict, "kCGWindowNumber").unwrap_or(0) as isize;
            if wid == 0 { continue; }

            // Get owner (process) name
            let owner = get_cf_string(dict, "kCGWindowOwnerName").unwrap_or_default();
            if owner.is_empty() { continue; }

            // Get window name
            let name = get_cf_string(dict, "kCGWindowName").unwrap_or_default();

            // Get layer — only layer 0 is normal windows
            let layer = get_cf_number(dict, "kCGWindowLayer").unwrap_or(-1);
            if layer != 0 { continue; }

            result.push(TrackedWindow {
                hwnd: wid,
                process_name: owner,
                title: if name.is_empty() { "(untitled)".to_string() } else { name },
                class_name: String::new(),
            });
        }
        CFRelease(window_list as _);
    }
    result
}

unsafe fn get_cf_string(dict: CFDictionaryRef, key: &str) -> Option<String> {
    let cf_key = CFString::new(key);
    let mut value: *const core_foundation::base::__CFType = std::ptr::null();
    if CFDictionaryGetValueIfPresent(dict, cf_key.as_CFTypeRef() as _, &mut value as *mut _ as _) != 0 {
        if !value.is_null() {
            let cf_str = CFString::wrap_under_get_rule(value as CFStringRef);
            return Some(cf_str.to_string());
        }
    }
    None
}

unsafe fn get_cf_number(dict: CFDictionaryRef, key: &str) -> Option<i64> {
    let cf_key = CFString::new(key);
    let mut value: *const core_foundation::base::__CFType = std::ptr::null();
    if CFDictionaryGetValueIfPresent(dict, cf_key.as_CFTypeRef() as _, &mut value as *mut _ as _) != 0 {
        if !value.is_null() {
            let cf_num = CFNumber::wrap_under_get_rule(value as CFNumberRef);
            return cf_num.to_i64();
        }
    }
    None
}

/// Get window position using CGWindowListCopyWindowInfo for a specific window
fn get_window_pos_by_id(wid: isize) -> Option<(i32, i32)> {
    unsafe {
        let window_list = CGWindowListCopyWindowInfo(
            kCGWindowListOptionIncludingWindow,
            wid as u32,
        );
        if window_list.is_null() { return None; }
        let count = CFArrayGetCount(window_list as _);
        if count == 0 { CFRelease(window_list as _); return None; }

        let dict = CFArrayGetValueAtIndex(window_list as _, 0) as CFDictionaryRef;
        let bounds_key = CFString::new("kCGWindowBounds");
        let mut bounds_val: *const core_foundation::base::__CFType = std::ptr::null();
        if CFDictionaryGetValueIfPresent(dict, bounds_key.as_CFTypeRef() as _, &mut bounds_val as *mut _ as _) != 0 {
            let bounds_dict = bounds_val as CFDictionaryRef;
            let x = get_cf_number(bounds_dict, "X").unwrap_or(0) as i32;
            let y = get_cf_number(bounds_dict, "Y").unwrap_or(0) as i32;
            CFRelease(window_list as _);
            return Some((x, y));
        }
        CFRelease(window_list as _);
        None
    }
}

/// Move a window using AppleScript (works without accessibility permissions for most apps)
fn move_window_applescript(owner: &str, x: i32, y: i32) {
    let script = format!(
        r#"tell application "System Events"
            tell process "{}"
                try
                    set position of front window to {{{}, {}}}
                end try
            end tell
        end tell"#,
        owner, x, y
    );
    let _ = Command::new("osascript").args(["-e", &script]).output();
}

/// Bring all windows of an app to front
fn activate_app(owner: &str) {
    let script = format!(
        r#"tell application "{}" to activate"#,
        owner
    );
    let _ = Command::new("osascript").args(["-e", &script]).output();
}

/// Minimize a window
fn minimize_window_applescript(owner: &str) {
    let script = format!(
        r#"tell application "System Events"
            tell process "{}"
                try
                    set miniaturized of front window to true
                end try
            end tell
        end tell"#,
        owner
    );
    let _ = Command::new("osascript").args(["-e", &script]).output();
}

/// Foreground + minimize/restore monitor thread
fn event_monitor_loop(gm: Arc<Mutex<GroupManager>>) {
    let mut last_fg_app = String::new();

    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if SUPPRESSED.load(Ordering::SeqCst) { continue; }

        // Get frontmost application
        let output = Command::new("osascript")
            .args(["-e", r#"tell application "System Events" to get name of first application process whose frontmost is true"#])
            .output();
        let fg_app = match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(_) => continue,
        };

        if fg_app == last_fg_app || fg_app.is_empty() { continue; }
        last_fg_app = fg_app.clone();

        // Check if any tracked window belongs to this app
        let gm_lock = match gm.try_lock() { Ok(g) => g, Err(_) => continue };
        // Find if any group has a window from this process
        let mut group_to_activate = None;
        for (hwnd, gid) in &gm_lock.active_bindings {
            // We need to find the process name for this hwnd
            let windows = enumerate_windows();
            for w in &windows {
                if w.hwnd == *hwnd && w.process_name == fg_app {
                    group_to_activate = Some(gid.clone());
                    break;
                }
            }
            if group_to_activate.is_some() { break; }
        }

        if let Some(gid) = group_to_activate {
            let sibling_hwnds: Vec<isize> = gm_lock.active_bindings.iter()
                .filter(|(_, g)| g.as_str() == gid)
                .map(|(h, _)| *h)
                .collect();
            drop(gm_lock);

            // Activate all apps in the group
            SUPPRESSED.store(true, Ordering::SeqCst);
            let windows = enumerate_windows();
            let mut activated_apps: Vec<String> = Vec::new();
            for &h in &sibling_hwnds {
                if let Some(w) = windows.iter().find(|w| w.hwnd == h) {
                    if !activated_apps.contains(&w.process_name) && w.process_name != fg_app {
                        activate_app(&w.process_name);
                        activated_apps.push(w.process_name.clone());
                    }
                }
            }
            // Re-activate the original app to keep it on top
            activate_app(&fg_app);
            SUPPRESSED.store(false, Ordering::SeqCst);
            MOVE_CACHE_DIRTY.store(true, Ordering::SeqCst);

            info!("FG sync (macOS): {} + {} apps", fg_app, activated_apps.len());
        }
    }
}

/// Move sync polling thread (same approach as Windows — cursor-based)
fn move_sync_loop(gm: Arc<Mutex<GroupManager>>) {
    // macOS move sync using cursor position
    let mut tracked_fg_app = String::new();
    let mut drag_active = false;
    let mut drag_cursor_start: (i32, i32) = (0, 0);
    let mut drag_sibling_starts: HashMap<isize, (i32, i32, String)> = HashMap::new(); // hwnd -> (x, y, owner)

    fn get_cursor_mac() -> (i32, i32) {
        // Use CoreGraphics
        let event = unsafe { core_graphics::event::CGEvent::new(core_graphics::event_source::CGEventSource::new(
            core_graphics::event_source::CGEventSourceStateID::CombinedSessionState
        ).ok().as_ref().unwrap()) };
        if let Some(e) = event {
            let loc = e.location();
            return (loc.x as i32, loc.y as i32);
        }
        (0, 0)
    }

    fn is_mouse_down() -> bool {
        unsafe {
            let state = core_graphics::event::CGEventSource::button_state(
                core_graphics::event_source::CGEventSourceStateID::CombinedSessionState,
                core_graphics::event::CGMouseButton::Left,
            );
            state
        }
    }

    loop {
        std::thread::sleep(std::time::Duration::from_millis(8));

        if MOVE_CACHE_DIRTY.swap(false, Ordering::SeqCst) {
            tracked_fg_app.clear();
            drag_active = false;
            drag_sibling_starts.clear();
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        if SUPPRESSED.load(Ordering::SeqCst) { continue; }

        let (cx, cy) = get_cursor_mac();

        // Get frontmost app
        let output = Command::new("osascript")
            .args(["-e", r#"tell application "System Events" to get name of first application process whose frontmost is true"#])
            .output();
        let fg_app = match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(_) => continue,
        };

        let gm_lock = match gm.try_lock() { Ok(g) => g, Err(_) => continue };
        // Find if fg app is in a group with sync_move
        let mut fg_group_id = None;
        let windows = enumerate_windows();
        for w in &windows {
            if w.process_name == fg_app {
                if let Some(gid) = gm_lock.find_group_for_hwnd(w.hwnd) {
                    if gm_lock.groups.iter().any(|g| g.id == gid && g.sync_move) {
                        fg_group_id = Some(gid.to_string());
                        break;
                    }
                }
            }
        }
        let Some(gid) = fg_group_id else { continue };
        let siblings: Vec<isize> = gm_lock.active_bindings.iter()
            .filter(|(_, g)| g.as_str() == gid)
            .map(|(h, _)| *h)
            .collect();
        drop(gm_lock);

        if siblings.len() < 2 { continue; }

        let lb = is_mouse_down();

        if lb && !drag_active {
            drag_active = true;
            drag_cursor_start = (cx, cy);
            drag_sibling_starts.clear();
            for &h in &siblings {
                if let Some(w) = windows.iter().find(|w| w.hwnd == h) {
                    if let Some((wx, wy)) = get_window_pos_by_id(h) {
                        drag_sibling_starts.insert(h, (wx, wy, w.process_name.clone()));
                    }
                }
            }
            continue;
        }

        if !lb && drag_active {
            drag_active = false;
            drag_sibling_starts.clear();
            continue;
        }

        if drag_active && !drag_sibling_starts.is_empty() {
            let cdx = cx - drag_cursor_start.0;
            let cdy = cy - drag_cursor_start.1;
            if cdx == 0 && cdy == 0 { continue; }

            MOVE_IN_PROGRESS.store(true, Ordering::SeqCst);
            for (&h, (sx, sy, owner)) in &drag_sibling_starts {
                // Don't move the window the user is dragging
                if windows.iter().any(|w| w.hwnd == h && w.process_name == fg_app) { continue; }
                move_window_applescript(owner, sx + cdx, sy + cdy);
            }
            MOVE_IN_PROGRESS.store(false, Ordering::SeqCst);
        }
    }
}

pub fn start_monitor(gm: Arc<Mutex<GroupManager>>) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting macOS monitor...");

    // Event monitor thread (fg tracking)
    let gm_ev = gm.clone();
    std::thread::spawn(move || {
        event_monitor_loop(gm_ev);
    });

    // Move sync thread
    let gm_mv = gm.clone();
    std::thread::spawn(move || {
        info!("macOS move sync thread started");
        move_sync_loop(gm_mv);
    });

    info!("macOS monitor running (fg poll + move sync)");

    // Keep thread alive
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
