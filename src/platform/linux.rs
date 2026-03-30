/// Linux (X11) platform implementation.
///
/// Uses x11rb for window enumeration and manipulation.
/// Foreground tracking via _NET_ACTIVE_WINDOW property change.
/// Move sync via cursor polling (same approach as Windows/macOS).

use crate::group::{GroupManager, TrackedWindow};
use log::info;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

static SUPPRESSED: AtomicBool = AtomicBool::new(false);
static MOVE_CACHE_DIRTY: AtomicBool = AtomicBool::new(false);
static MOVE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

fn get_atom(conn: &RustConnection, name: &str) -> u32 {
    conn.intern_atom(false, name.as_bytes())
        .ok().and_then(|c| c.reply().ok()).map(|r| r.atom).unwrap_or(0)
}

fn get_property_u32(conn: &RustConnection, window: u32, atom: u32) -> Option<Vec<u32>> {
    let reply = conn.get_property(false, window, atom, AtomEnum::ANY, 0, 1024)
        .ok()?.reply().ok()?;
    if reply.format == 32 {
        Some(reply.value32()?.collect())
    } else {
        None
    }
}

fn get_property_string(conn: &RustConnection, window: u32, atom: u32) -> Option<String> {
    let reply = conn.get_property(false, window, atom, AtomEnum::ANY, 0, 1024)
        .ok()?.reply().ok()?;
    Some(String::from_utf8_lossy(&reply.value).trim_end_matches('\0').to_string())
}

fn get_window_geometry(conn: &RustConnection, window: u32) -> Option<(i32, i32, u32, u32)> {
    let geo = conn.get_geometry(window).ok()?.reply().ok()?;
    // Translate to root coordinates
    let trans = conn.translate_coordinates(window, geo.root, 0, 0).ok()?.reply().ok()?;
    Some((trans.dst_x as i32, trans.dst_y as i32, geo.width as u32, geo.height as u32))
}

pub fn enumerate_windows() -> Vec<TrackedWindow> {
    let mut result = Vec::new();
    let Ok((conn, screen_num)) = RustConnection::connect(None) else { return result; };
    let screen = &conn.setup().roots[screen_num];

    let net_client_list = get_atom(&conn, "_NET_CLIENT_LIST");
    let net_wm_name = get_atom(&conn, "_NET_WM_NAME");
    let wm_name = get_atom(&conn, "WM_NAME");
    let net_wm_pid = get_atom(&conn, "_NET_WM_PID");

    let Some(clients) = get_property_u32(&conn, screen.root, net_client_list) else { return result; };

    for &wid in &clients {
        // Get window name
        let title = get_property_string(&conn, wid, net_wm_name)
            .or_else(|| get_property_string(&conn, wid, wm_name))
            .unwrap_or_default();
        if title.is_empty() { continue; }

        // Get PID and try to get process name
        let pid = get_property_u32(&conn, wid, net_wm_pid)
            .and_then(|v| v.first().copied())
            .unwrap_or(0);
        let process_name = if pid > 0 {
            std::fs::read_to_string(format!("/proc/{}/comm", pid))
                .unwrap_or_default().trim().to_string()
        } else {
            String::new()
        };

        // Get WM_CLASS
        let wm_class_atom = get_atom(&conn, "WM_CLASS");
        let class_name = get_property_string(&conn, wid, wm_class_atom).unwrap_or_default();

        result.push(TrackedWindow {
            hwnd: wid as isize,
            process_name: if process_name.is_empty() { class_name.clone() } else { process_name },
            title,
            class_name,
        });
    }
    result
}

fn get_active_window(conn: &RustConnection, root: u32) -> Option<u32> {
    let atom = get_atom(conn, "_NET_ACTIVE_WINDOW");
    get_property_u32(conn, root, atom)?.first().copied()
}

fn activate_window(conn: &RustConnection, root: u32, window: u32) {
    let net_active = get_atom(conn, "_NET_ACTIVE_WINDOW");
    let event = ClientMessageEvent::new(
        32, window, net_active,
        ClientMessageData::from([1u32, 0, 0, 0, 0]),
    );
    let _ = conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, event);
    let _ = conn.flush();
}

fn move_window(conn: &RustConnection, window: u32, x: i32, y: i32) {
    let _ = conn.configure_window(window, &ConfigureWindowAux::new().x(x).y(y));
    let _ = conn.flush();
}

fn minimize_window(conn: &RustConnection, root: u32, window: u32) {
    let wm_change_state = get_atom(conn, "WM_CHANGE_STATE");
    let event = ClientMessageEvent::new(
        32, window, wm_change_state,
        ClientMessageData::from([3u32 /* IconicState */, 0, 0, 0, 0]),
    );
    let _ = conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, event);
    let _ = conn.flush();
}

fn get_cursor_pos(conn: &RustConnection, root: u32) -> (i32, i32) {
    conn.query_pointer(root).ok()
        .and_then(|c| c.reply().ok())
        .map(|r| (r.root_x as i32, r.root_y as i32))
        .unwrap_or((0, 0))
}

fn is_mouse_button_pressed(conn: &RustConnection, root: u32) -> bool {
    conn.query_pointer(root).ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.mask.contains(KeyButMask::BUTTON1))
        .unwrap_or(false)
}

/// Foreground monitor + move sync (combined in one loop for Linux)
fn monitor_loop(gm: Arc<Mutex<GroupManager>>) {
    let Ok((conn, screen_num)) = RustConnection::connect(None) else {
        log::error!("Failed to connect to X11");
        return;
    };
    let root = conn.setup().roots[screen_num].root;

    let mut last_active: u32 = 0;
    let mut drag_active = false;
    let mut drag_cursor_start = (0i32, 0i32);
    let mut drag_sibling_starts: HashMap<u32, (i32, i32)> = HashMap::new();

    loop {
        std::thread::sleep(std::time::Duration::from_millis(16));

        if MOVE_CACHE_DIRTY.swap(false, Ordering::SeqCst) {
            drag_active = false;
            drag_sibling_starts.clear();
            last_active = 0;
            continue;
        }
        if SUPPRESSED.load(Ordering::SeqCst) { continue; }

        let active = get_active_window(&conn, root).unwrap_or(0);
        if active == 0 { continue; }

        // Foreground change detection
        if active != last_active {
            last_active = active;
            MOVE_CACHE_DIRTY.store(true, Ordering::SeqCst);

            let hv = active as isize;
            let mut gm_lock = match gm.try_lock() { Ok(g) => g, Err(_) => continue };

            let mut siblings = gm_lock.get_sibling_hwnds(hv);
            if siblings.is_empty() {
                // Try auto-bind
                let windows = enumerate_windows();
                if let Some(tw) = windows.iter().find(|w| w.hwnd == hv) {
                    if let Some(_gid) = gm_lock.try_auto_bind(tw) {
                        siblings = gm_lock.get_sibling_hwnds(hv);
                    }
                }
            }
            if !siblings.is_empty() {
                info!("FG sync (X11): {} siblings", siblings.len());
                SUPPRESSED.store(true, Ordering::SeqCst);
                drop(gm_lock);

                // Activate siblings
                for &sh in &siblings {
                    activate_window(&conn, root, sh as u32);
                }
                // Re-activate the original
                activate_window(&conn, root, active);

                SUPPRESSED.store(false, Ordering::SeqCst);
            }
            continue;
        }

        // Move sync
        let hv = active as isize;
        let gm_lock = match gm.try_lock() { Ok(g) => g, Err(_) => continue };
        let gid = match gm_lock.find_group_for_hwnd(hv) { Some(g) => g.to_string(), None => continue };
        if !gm_lock.groups.iter().any(|g| g.id == gid && g.sync_move) { continue; }
        let siblings: Vec<isize> = gm_lock.get_sibling_hwnds(hv);
        if siblings.is_empty() { continue; }
        drop(gm_lock);

        let (cx, cy) = get_cursor_pos(&conn, root);
        let lb = is_mouse_button_pressed(&conn, root);

        if lb && !drag_active {
            drag_active = true;
            drag_cursor_start = (cx, cy);
            drag_sibling_starts.clear();
            for &sh in &siblings {
                if sh == hv { continue; }
                if let Some((wx, wy, _, _)) = get_window_geometry(&conn, sh as u32) {
                    drag_sibling_starts.insert(sh as u32, (wx, wy));
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
            for (&wid, &(sx, sy)) in &drag_sibling_starts {
                move_window(&conn, wid, sx + cdx, sy + cdy);
            }
            MOVE_IN_PROGRESS.store(false, Ordering::SeqCst);
        }
    }
}

pub fn start_monitor(gm: Arc<Mutex<GroupManager>>) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting Linux (X11) monitor...");

    let gm_clone = gm.clone();
    std::thread::spawn(move || {
        monitor_loop(gm_clone);
    });

    info!("Linux monitor running");

    // Keep alive
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
