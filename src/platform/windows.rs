use crate::group::{GroupManager, TrackedWindow};
use log::{debug, info};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::System::Threading::*;
use windows::Win32::Graphics::Dwm::*;

static GROUP_MANAGER: OnceLock<Arc<Mutex<GroupManager>>> = OnceLock::new();
static SUPPRESSED: AtomicBool = AtomicBool::new(false);
static LAST_FG_SYNC_MS: AtomicIsize = AtomicIsize::new(0);
static START_INSTANT: OnceLock<Instant> = OnceLock::new();
static MOVE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static MOVE_CACHE_DIRTY: AtomicBool = AtomicBool::new(false);

const FG_DEBOUNCE_MS: isize = 150;

fn elapsed_ms() -> isize {
    START_INSTANT.get().map(|s| s.elapsed().as_millis() as isize).unwrap_or(0)
}

fn get_process_name(hwnd: HWND) -> String {
    unsafe {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 { return String::new(); }
        if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; 512];
            let mut size = buf.len() as u32;
            let r = QueryFullProcessImageNameW(handle, PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buf.as_mut_ptr()), &mut size);
            let _ = CloseHandle(handle);
            if r.is_ok() {
                let path = String::from_utf16_lossy(&buf[..size as usize]);
                return path.rsplit('\\').next().unwrap_or("").to_string();
            }
        }
        String::new()
    }
}
fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 { return String::new(); }
        let mut buf = vec![0u16; (len + 1) as usize];
        GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string()
    }
}
fn get_class_name(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buf);
        if len == 0 { return String::new(); }
        String::from_utf16_lossy(&buf[..len as usize])
    }
}
fn is_real_window(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() { return false; }
        let ex = WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32);
        if ex.contains(WS_EX_TOOLWINDOW) { return false; }
        if get_window_title(hwnd).is_empty() { return false; }
        let mut cloaked: u32 = 0;
        let r = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _, std::mem::size_of::<u32>() as u32);
        if r.is_ok() && cloaked != 0 { return false; }
        true
    }
}
fn get_window_pos(hwnd: HWND) -> Option<(i32, i32)> {
    unsafe {
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_ok() { Some((r.left, r.top)) } else { None }
    }
}
/// Returns true if coordinates look valid (not minimized at -32000)
fn is_valid_pos(x: i32, y: i32) -> bool {
    x > -10000 && y > -10000
}
fn get_zorder_sorted(group_hwnds: &[isize]) -> Vec<isize> {
    unsafe {
        let mut sorted = Vec::new();
        let mut cur = GetWindow(GetForegroundWindow(), GW_HWNDFIRST).unwrap_or_default();
        while !cur.0.is_null() {
            let h = cur.0 as isize;
            if group_hwnds.contains(&h) { sorted.push(h); }
            if sorted.len() == group_hwnds.len() { break; }
            cur = GetWindow(cur, GW_HWNDNEXT).unwrap_or_default();
        }
        sorted
    }
}

pub fn enumerate_windows() -> Vec<TrackedWindow> {
    let mut w = Vec::new();
    unsafe { let _ = EnumWindows(Some(enum_cb), LPARAM(&mut w as *mut _ as isize)); }
    w
}
unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
    if !is_real_window(hwnd) { return TRUE; }
    let v = &mut *(lp.0 as *mut Vec<TrackedWindow>);
    v.push(TrackedWindow { hwnd: hwnd.0 as isize,
        process_name: get_process_name(hwnd), title: get_window_title(hwnd),
        class_name: get_class_name(hwnd) });
    TRUE
}

fn bring_group_to_front(activated_hwnd: isize, all_group_hwnds: &[isize]) {
    unsafe {
        SUPPRESSED.store(true, Ordering::SeqCst);

        let mut zorder = get_zorder_sorted(all_group_hwnds);
        zorder.retain(|&h| h != activated_hwnd);
        zorder.insert(0, activated_hwnd);

        for &h in &zorder {
            let wh = HWND(h as *mut _);
            if !IsWindow(wh).as_bool() { continue; }
            if IsIconic(wh).as_bool() {
                let _ = ShowWindow(wh, SW_RESTORE);
            }
            let _ = ShowWindow(wh, SW_SHOWNA);
        }

        for &h in zorder.iter().rev() {
            let wh = HWND(h as *mut _);
            if !IsWindow(wh).as_bool() { continue; }
            let _ = SetWindowPos(wh, HWND_TOP, 0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        for i in 1..zorder.len() {
            let above = HWND(zorder[i - 1] as *mut _);
            let wh = HWND(zorder[i] as *mut _);
            let _ = SetWindowPos(wh, above, 0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }

        let _ = SetForegroundWindow(HWND(activated_hwnd as *mut _));

        MOVE_CACHE_DIRTY.store(true, Ordering::SeqCst);
        SUPPRESSED.store(false, Ordering::SeqCst);
        LAST_FG_SYNC_MS.store(elapsed_ms(), Ordering::SeqCst);
    }
}

pub fn minimize_windows(hwnds: &[isize]) {
    unsafe {
        SUPPRESSED.store(true, Ordering::SeqCst);
        MOVE_CACHE_DIRTY.store(true, Ordering::SeqCst);
        for &h in hwnds {
            let wh = HWND(h as *mut _);
            if IsWindow(wh).as_bool() && !IsIconic(wh).as_bool() {
                let _ = ShowWindow(wh, SW_MINIMIZE);
            }
        }
        SUPPRESSED.store(false, Ordering::SeqCst);
        LAST_FG_SYNC_MS.store(elapsed_ms(), Ordering::SeqCst);
    }
}

pub fn restore_windows(hwnds: &[isize]) {
    unsafe {
        SUPPRESSED.store(true, Ordering::SeqCst);
        MOVE_CACHE_DIRTY.store(true, Ordering::SeqCst);
        for &h in hwnds {
            let wh = HWND(h as *mut _);
            if IsWindow(wh).as_bool() && IsIconic(wh).as_bool() {
                let _ = ShowWindow(wh, SW_RESTORE);
            }
        }
        SUPPRESSED.store(false, Ordering::SeqCst);
        LAST_FG_SYNC_MS.store(elapsed_ms(), Ordering::SeqCst);
    }
}

unsafe extern "system" fn win_event_callback(
    _hook: HWINEVENTHOOK, event: u32, hwnd: HWND,
    id_object: i32, _id_child: i32, _thread_id: u32, _timestamp: u32,
) {
    if id_object != 0 || hwnd.0.is_null() { return; }
    if SUPPRESSED.load(Ordering::SeqCst) || MOVE_IN_PROGRESS.load(Ordering::SeqCst) { return; }

    let hv = hwnd.0 as isize;
    let Some(gm_arc) = GROUP_MANAGER.get() else { return; };

    match event {
        0x0003 => {
            let now = elapsed_ms();
            if now - LAST_FG_SYNC_MS.load(Ordering::SeqCst) < FG_DEBOUNCE_MS { return; }
            let mut gm = match gm_arc.try_lock() { Ok(g) => g, Err(_) => return };
            let mut sibs = gm.get_sibling_hwnds(hv);
            if sibs.is_empty() && is_real_window(hwnd) {
                let tw = TrackedWindow { hwnd: hv, process_name: get_process_name(hwnd),
                    title: get_window_title(hwnd), class_name: get_class_name(hwnd) };
                if let Some(gid) = gm.try_auto_bind(&tw) {
                    debug!("Auto-bound to {}", gid);
                    sibs = gm.get_sibling_hwnds(hv);
                }
            }
            if sibs.is_empty() { return; }
            let mut all = sibs; all.push(hv);
            info!("FG sync: {} windows", all.len());
            drop(gm);
            bring_group_to_front(hv, &all);
        }
        0x0016 => {
            let now = elapsed_ms();
            if now - LAST_FG_SYNC_MS.load(Ordering::SeqCst) < FG_DEBOUNCE_MS { return; }
            let gm = match gm_arc.try_lock() { Ok(g) => g, Err(_) => return };
            let gid = match gm.find_group_for_hwnd(hv) { Some(g) => g.to_string(), None => return };
            if !gm.groups.iter().any(|g| g.id == gid && g.sync_minimize) { return; }
            let sibs = gm.get_sibling_hwnds(hv);
            if sibs.is_empty() { return; }
            info!("Min sync: {}", sibs.len());
            drop(gm);
            minimize_windows(&sibs);
        }
        0x0017 => {
            let now = elapsed_ms();
            if now - LAST_FG_SYNC_MS.load(Ordering::SeqCst) < FG_DEBOUNCE_MS { return; }
            let gm = match gm_arc.try_lock() { Ok(g) => g, Err(_) => return };
            let gid = match gm.find_group_for_hwnd(hv) { Some(g) => g.to_string(), None => return };
            if !gm.groups.iter().any(|g| g.id == gid && g.sync_minimize) { return; }
            let sibs = gm.get_sibling_hwnds(hv);
            if sibs.is_empty() { return; }
            info!("Restore sync: {}", sibs.len());
            drop(gm);
            restore_windows(&sibs);
        }
        0x8001 => {
            if let Ok(mut gm) = gm_arc.try_lock() { gm.unbind_window(hv); }
        }
        _ => {}
    }
}

/// Move sync: detects drag via GetAsyncKeyState(VK_LBUTTON) + cursor delta.
/// Uses fixed offsets from fg window position (captured once at drag start).
/// Repositions siblings as: target = fg_pos_at_start + offset + cursor_delta
fn move_sync_loop(gm: Arc<Mutex<GroupManager>>) {
    let mut tracked_fg: isize = 0;
    // Sibling offsets relative to fg: sibling_pos = fg_pos + offset
    let mut offsets: HashMap<isize, (i32, i32)> = HashMap::new();
    // State for current drag
    let mut drag_active: bool = false;
    let mut drag_cursor_start: (i32, i32) = (0, 0);
    // Sibling absolute positions at drag start
    let mut drag_sibling_start: HashMap<isize, (i32, i32)> = HashMap::new();

    const MAX_DELTA: i32 = 300;

    fn get_cursor() -> (i32, i32) {
        unsafe { let mut p = POINT::default(); let _ = GetCursorPos(&mut p); (p.x, p.y) }
    }
    fn lbutton_down() -> bool {
        unsafe { (GetAsyncKeyState(0x01 /* VK_LBUTTON */) as u16 & 0x8000) != 0 }
    }

    loop {
        std::thread::sleep(std::time::Duration::from_millis(8));

        if MOVE_CACHE_DIRTY.swap(false, Ordering::SeqCst) {
            tracked_fg = 0; offsets.clear(); drag_active = false; drag_sibling_start.clear();
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        if SUPPRESSED.load(Ordering::SeqCst) { continue; }

        let fg = unsafe { GetForegroundWindow() };
        if fg.0.is_null() { continue; }
        let fg_h = fg.0 as isize;
        if unsafe { IsIconic(fg).as_bool() } {
            if fg_h == tracked_fg { tracked_fg = 0; offsets.clear(); drag_active = false; }
            continue;
        }

        let gm_lock = match gm.try_lock() { Ok(g) => g, Err(_) => continue };
        let gid = match gm_lock.find_group_for_hwnd(fg_h) {
            Some(g) => g.to_string(),
            None => {
                if tracked_fg != 0 { tracked_fg = 0; offsets.clear(); drag_active = false; }
                continue;
            }
        };
        if !gm_lock.groups.iter().any(|g| g.id == gid && g.sync_move) { continue; }
        let siblings = gm_lock.get_sibling_hwnds(fg_h);
        if siblings.is_empty() { continue; }
        drop(gm_lock);

        // New fg — capture offsets
        if fg_h != tracked_fg {
            tracked_fg = fg_h;
            offsets.clear();
            drag_active = false;
            drag_sibling_start.clear();
            if let Some((fx, fy)) = get_window_pos(fg) {
                if is_valid_pos(fx, fy) {
                    for &sh in &siblings {
                        let swh = HWND(sh as *mut _);
                        if unsafe { !IsWindow(swh).as_bool() || IsIconic(swh).as_bool() } { continue; }
                        if let Some((sx, sy)) = get_window_pos(swh) {
                            if is_valid_pos(sx, sy) { offsets.insert(sh, (sx - fx, sy - fy)); }
                        }
                    }
                }
            }
            continue;
        }

        let (cx, cy) = get_cursor();
        let lb = lbutton_down();

        // Drag just started: left button pressed, have offsets
        if lb && !drag_active && !offsets.is_empty() {
            drag_active = true;
            drag_cursor_start = (cx, cy);
            // Snapshot sibling positions at drag start
            drag_sibling_start.clear();
            for &sh in &siblings {
                let swh = HWND(sh as *mut _);
                if unsafe { !IsWindow(swh).as_bool() || IsIconic(swh).as_bool() } { continue; }
                if let Some((sx, sy)) = get_window_pos(swh) {
                    if is_valid_pos(sx, sy) { drag_sibling_start.insert(sh, (sx, sy)); }
                }
            }
            continue;
        }

        // Drag ended: left button released
        if !lb && drag_active {
            drag_active = false;
            drag_sibling_start.clear();
            // Re-capture offsets at new positions
            if let Some((fx, fy)) = get_window_pos(fg) {
                if is_valid_pos(fx, fy) {
                    offsets.clear();
                    for &sh in &siblings {
                        let swh = HWND(sh as *mut _);
                        if unsafe { !IsWindow(swh).as_bool() || IsIconic(swh).as_bool() } { continue; }
                        if let Some((sx, sy)) = get_window_pos(swh) {
                            if is_valid_pos(sx, sy) { offsets.insert(sh, (sx - fx, sy - fy)); }
                        }
                    }
                }
            }
            continue;
        }

        // During drag: move siblings by cursor delta from drag start
        if drag_active {
            let cdx = cx - drag_cursor_start.0;
            let cdy = cy - drag_cursor_start.1;

            if cdx == 0 && cdy == 0 { continue; }

            MOVE_IN_PROGRESS.store(true, Ordering::SeqCst);
            unsafe {
                for &sh in &siblings {
                    let swh = HWND(sh as *mut _);
                    if !IsWindow(swh).as_bool() || IsIconic(swh).as_bool() { continue; }
                    if let Some(&(start_x, start_y)) = drag_sibling_start.get(&sh) {
                        let tx = start_x + cdx;
                        let ty = start_y + cdy;
                        let _ = SetWindowPos(swh, HWND(std::ptr::null_mut()),
                            tx, ty, 0, 0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                    }
                }
            }
            MOVE_IN_PROGRESS.store(false, Ordering::SeqCst);
        }
    }
}

pub fn start_monitor(gm: Arc<Mutex<GroupManager>>) -> Result<(), Box<dyn std::error::Error>> {
    GROUP_MANAGER.set(gm.clone()).map_err(|_| "init")?;
    START_INSTANT.set(Instant::now()).map_err(|_| "init")?;

    let gm_move = gm.clone();
    std::thread::spawn(move || {
        info!("Move sync thread started");
        move_sync_loop(gm_move);
    });

    info!("Starting event monitor...");
    unsafe {
        SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND,
            None, Some(win_event_callback), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
        SetWinEventHook(EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND,
            None, Some(win_event_callback), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
        SetWinEventHook(EVENT_OBJECT_DESTROY, EVENT_OBJECT_DESTROY,
            None, Some(win_event_callback), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);

        info!("Hooks: fg + min/max + destroy | Move: poll thread");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
