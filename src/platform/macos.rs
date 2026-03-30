/// macOS platform — uses osascript + CoreGraphics window list only.
/// No CGEvent dependency (avoids CI issues).

use crate::group::{GroupManager, TrackedWindow};
use log::info;
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static SUPPRESSED: AtomicBool = AtomicBool::new(false);
static MOVE_CACHE_DIRTY: AtomicBool = AtomicBool::new(false);

/// Enumerate windows via osascript (reliable, no framework dependency issues)
pub fn enumerate_windows() -> Vec<TrackedWindow> {
    let script = r#"
        set output to ""
        tell application "System Events"
            set procs to every application process whose visible is true
            repeat with p in procs
                set pname to name of p
                try
                    set wins to every window of p
                    repeat with w in wins
                        set wname to name of w
                        set wpos to position of w
                        set wid to id of w
                        set output to output & pname & "|" & wname & "|" & (item 1 of wpos) & "," & (item 2 of wpos) & linefeed
                    end repeat
                end try
            end repeat
        end tell
        return output
    "#;
    let output = Command::new("osascript").args(["-e", script]).output();
    let Ok(out) = output else { return Vec::new(); };
    let text = String::from_utf8_lossy(&out.stdout);

    let mut result = Vec::new();
    let mut id_counter: isize = 1;
    for line in text.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 2 {
            let process_name = parts[0].to_string();
            let title = parts[1].to_string();
            if process_name.is_empty() || title.is_empty() { continue; }
            result.push(TrackedWindow {
                hwnd: id_counter,
                process_name,
                title,
                class_name: String::new(),
            });
            id_counter += 1;
        }
    }
    result
}

fn get_frontmost_app() -> String {
    let out = Command::new("osascript")
        .args(["-e", r#"tell application "System Events" to get name of first application process whose frontmost is true"#])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

fn activate_app(name: &str) {
    let script = format!(r#"tell application "{}" to activate"#, name);
    let _ = Command::new("osascript").args(["-e", &script]).output();
}

fn move_window_of_app(owner: &str, x: i32, y: i32) {
    let script = format!(
        r#"tell application "System Events" to tell process "{}" to try
            set position of front window to {{{}, {}}}
        end try"#,
        owner, x, y
    );
    let _ = Command::new("osascript").args(["-e", &script]).output();
}

fn get_mouse_pos() -> (i32, i32) {
    // Use python (always available on macOS) to get mouse position from Quartz
    let out = Command::new("python3")
        .args(["-c", "from Quartz.CoreGraphics import CGEventGetLocation, CGEventCreate; e=CGEventCreate(None); l=CGEventGetLocation(e); print(f'{int(l.x)},{int(l.y)}')"])
        .output();
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        let parts: Vec<&str> = s.trim().split(',').collect();
        if parts.len() == 2 {
            let x = parts[0].parse::<i32>().unwrap_or(0);
            let y = parts[1].parse::<i32>().unwrap_or(0);
            return (x, y);
        }
    }
    (0, 0)
}

fn is_mouse_down() -> bool {
    let out = Command::new("python3")
        .args(["-c", "from Quartz.CoreGraphics import CGEventSourceButtonState, kCGEventSourceStateCombinedSessionState, kCGMouseButtonLeft; print(CGEventSourceButtonState(kCGEventSourceStateCombinedSessionState, kCGMouseButtonLeft))"])
        .output();
    if let Ok(o) = out {
        return String::from_utf8_lossy(&o.stdout).trim() == "True";
    }
    false
}

fn get_window_pos_of_app(owner: &str) -> Option<(i32, i32)> {
    let script = format!(
        r#"tell application "System Events" to tell process "{}" to try
            get position of front window
        end try"#, owner
    );
    let out = Command::new("osascript").args(["-e", &script]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = s.trim().split(", ").collect();
    if parts.len() == 2 {
        let x = parts[0].parse::<i32>().ok()?;
        let y = parts[1].parse::<i32>().ok()?;
        return Some((x, y));
    }
    None
}

/// Combined event + move monitor
fn monitor_loop(gm: Arc<Mutex<GroupManager>>) {
    let mut last_fg = String::new();
    let mut drag_active = false;
    let mut drag_start = (0i32, 0i32);
    let mut drag_siblings: HashMap<String, (i32, i32)> = HashMap::new(); // owner -> start pos

    loop {
        std::thread::sleep(std::time::Duration::from_millis(50));

        if MOVE_CACHE_DIRTY.swap(false, Ordering::SeqCst) {
            drag_active = false;
            drag_siblings.clear();
            last_fg.clear();
            continue;
        }
        if SUPPRESSED.load(Ordering::SeqCst) { continue; }

        let fg = get_frontmost_app();
        if fg.is_empty() { continue; }

        // Foreground change
        if fg != last_fg {
            last_fg = fg.clone();
            drag_active = false;
            drag_siblings.clear();

            let gm_lock = match gm.try_lock() { Ok(g) => g, Err(_) => continue };
            // Find group containing fg app
            let windows = enumerate_windows();
            let mut fg_group = None;
            for w in &windows {
                if w.process_name == fg {
                    if let Some(gid) = gm_lock.find_group_for_hwnd(w.hwnd) {
                        fg_group = Some(gid.to_string());
                        break;
                    }
                }
            }
            let Some(gid) = fg_group else { continue; };

            // Get all processes in this group
            let group_hwnds: Vec<isize> = gm_lock.active_bindings.iter()
                .filter(|(_, g)| g.as_str() == gid).map(|(h, _)| *h).collect();
            drop(gm_lock);

            let mut other_apps: Vec<String> = Vec::new();
            for &h in &group_hwnds {
                if let Some(w) = windows.iter().find(|w| w.hwnd == h) {
                    if w.process_name != fg && !other_apps.contains(&w.process_name) {
                        other_apps.push(w.process_name.clone());
                    }
                }
            }

            if !other_apps.is_empty() {
                SUPPRESSED.store(true, Ordering::SeqCst);
                for app in &other_apps { activate_app(app); }
                activate_app(&fg); // bring back fg on top
                SUPPRESSED.store(false, Ordering::SeqCst);
                info!("FG sync (macOS): {} + {} apps", fg, other_apps.len());
            }
            continue;
        }

        // Move sync (simplified — lower framerate on macOS due to osascript overhead)
        let lb = is_mouse_down();
        if lb && !drag_active {
            drag_active = true;
            drag_start = get_mouse_pos();
            drag_siblings.clear();
            // Snapshot sibling positions
            let gm_lock = match gm.try_lock() { Ok(g) => g, Err(_) => continue };
            let windows = enumerate_windows();
            for w in &windows {
                if w.process_name != fg {
                    if let Some(gid) = gm_lock.find_group_for_hwnd(w.hwnd) {
                        if gm_lock.groups.iter().any(|g| g.id == gid && g.sync_move) {
                            if let Some(pos) = get_window_pos_of_app(&w.process_name) {
                                drag_siblings.insert(w.process_name.clone(), pos);
                            }
                        }
                    }
                }
            }
        }

        if !lb && drag_active {
            drag_active = false;
            drag_siblings.clear();
        }

        if drag_active && !drag_siblings.is_empty() {
            let (cx, cy) = get_mouse_pos();
            let dx = cx - drag_start.0;
            let dy = cy - drag_start.1;
            if dx != 0 || dy != 0 {
                for (owner, (sx, sy)) in &drag_siblings {
                    move_window_of_app(owner, sx + dx, sy + dy);
                }
            }
        }
    }
}

pub fn start_monitor(gm: Arc<Mutex<GroupManager>>) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting macOS monitor...");

    let gm_clone = gm.clone();
    std::thread::spawn(move || {
        monitor_loop(gm_clone);
    });

    info!("macOS monitor running");
    loop { std::thread::sleep(std::time::Duration::from_secs(3600)); }
}
