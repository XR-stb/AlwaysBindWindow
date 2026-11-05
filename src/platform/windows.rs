use crate::group::{GroupManager, TrackedWindow};
use log::info;
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
use windows::Win32::Graphics::Gdi::{MonitorFromRect, GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};

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

/// Keep a useful portion of the title bar inside a monitor's work area.
/// Negative monitor coordinates are valid; use monitor bounds, not screen zero.
fn reachable_position(hwnd: HWND, x: i32, y: i32, width: i32) -> (i32, i32) {
    unsafe {
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let title_height = 32 * dpi / 96;
        let title = RECT { left: x, top: y, right: x + width, bottom: y + title_height };
        let monitor = MonitorFromRect(&title, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() { return (x, y); }
        crate::window_geometry::reachable_position(x, y, width, 96 * dpi / 96, title_height,
            (info.rcWork.left, info.rcWork.top, info.rcWork.right, info.rcWork.bottom))
    }
}

pub fn recover_windows(hwnds: &[isize]) -> usize {
    let mut recovered = 0;
    unsafe {
        for &handle in hwnds {
            let hwnd = HWND(handle as *mut _);
            if !IsWindow(hwnd).as_bool() || IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() { continue; }
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() { continue; }
            let (x, y) = reachable_position(hwnd, rect.left, rect.top, rect.right - rect.left);
            if (x, y) != (rect.left, rect.top) && SetWindowPos(hwnd, None, x, y, 0, 0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE).is_ok() {
                recovered += 1;
            }
        }
    }
    if recovered > 0 { MOVE_CACHE_DIRTY.store(true, Ordering::SeqCst); }
    recovered
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
            let gm = match gm_arc.try_lock() { Ok(g) => g, Err(_) => return };
            // Groups hold only the exact windows the user lassoed; no auto-binding by process
            // name, so other windows of the same app stay independent.
            let sibs = gm.get_sibling_hwnds(hv);
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

/// Which part of the window frame the pointer grabbed when the drag began.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragGesture {
    /// Title-bar or client drag: positional deltas translate the group.
    Move,
    /// Edge or corner resize: position changes are layout, not group translation.
    Resize,
}

fn get_window_rect(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).ok().map(|_| rect)
    }
}

fn lbutton_down() -> bool {
    unsafe { (GetAsyncKeyState(0x01 /* VK_LBUTTON */) as u16 & 0x8000) != 0 }
}

fn rect_size(rect: &RECT) -> (i32, i32) {
    (rect.right - rect.left, rect.bottom - rect.top)
}

/// Classifies the pointer against the frame metrics of the window. The caption
/// strip always counts as a move: crossing a monitor with a different DPI resizes
/// the dragged window mid-drag, and that size change must not stop the follow.
fn gesture_for_pointer(pointer: POINT, rect: RECT, frame: i32, caption: i32) -> DragGesture {
    let frame = frame.max(1);
    let caption = caption.max(1);
    let on_caption_x = pointer.x >= rect.left + frame && pointer.x < rect.right - frame;
    let on_caption_y = pointer.y >= rect.top + frame && pointer.y < rect.top + frame + caption;
    if on_caption_x && on_caption_y {
        return DragGesture::Move;
    }
    let grip = frame + 3;
    let near_window = pointer.x >= rect.left - 2
        && pointer.x <= rect.right + 2
        && pointer.y >= rect.top - 2
        && pointer.y <= rect.bottom + 2;
    let on_edge = pointer.x <= rect.left + grip
        || pointer.x >= rect.right - grip
        || pointer.y <= rect.top + grip
        || pointer.y >= rect.bottom - grip;
    if near_window && on_edge {
        DragGesture::Resize
    } else {
        DragGesture::Move
    }
}

fn frame_metrics(hwnd: HWND) -> (i32, i32) {
    unsafe {
        let dpi = GetDpiForWindow(hwnd).max(96);
        let frame = GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi)
            + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
        let caption = GetSystemMetricsForDpi(SM_CYCAPTION, dpi);
        (frame.max(1), caption.max(1))
    }
}

fn drag_gesture_at(hwnd: HWND, rect: RECT) -> DragGesture {
    let mut pointer = POINT::default();
    if unsafe { GetCursorPos(&mut pointer) }.is_err() {
        return DragGesture::Move;
    }
    let (frame, caption) = frame_metrics(hwnd);
    gesture_for_pointer(pointer, rect, frame, caption)
}

fn capture_sibling_positions(siblings: &[isize]) -> HashMap<isize, (i32, i32)> {
    let mut positions = HashMap::new();
    for &sh in siblings {
        let swh = HWND(sh as *mut _);
        if unsafe { !IsWindow(swh).as_bool() || IsIconic(swh).as_bool() } {
            continue;
        }
        if let Some((sx, sy)) = get_window_pos(swh) {
            if is_valid_pos(sx, sy) {
                positions.insert(sh, (sx, sy));
            }
        }
    }
    positions
}

/// Targets are always derived from the drag reference snapshot plus the
/// foreground window's total displacement. Tests pin this: a missed poll must
/// never accumulate permanent drift.
fn translated_targets(
    start_positions: &HashMap<isize, (i32, i32)>,
    fg_start: (i32, i32),
    fg_now: (i32, i32),
) -> Vec<(isize, i32, i32)> {
    let dx = fg_now.0 - fg_start.0;
    let dy = fg_now.1 - fg_start.1;
    start_positions
        .iter()
        .map(|(&hwnd, &(x, y))| (hwnd, x + dx, y + dy))
        .collect()
}

fn translate_siblings(
    start_positions: &HashMap<isize, (i32, i32)>,
    fg_start: (i32, i32),
    fg_now: (i32, i32),
) {
    // Apply even when the total translation is zero: the foreground window may have
    // returned to its starting point while a follower is still at a stale target from
    // the previous poll. Re-applying the drag-reference targets is what makes the
    // group converge exactly.
    MOVE_IN_PROGRESS.store(true, Ordering::SeqCst);
    unsafe {
        for (sh, target_x, target_y) in translated_targets(start_positions, fg_start, fg_now) {
            let swh = HWND(sh as *mut _);
            if !IsWindow(swh).as_bool() || IsIconic(swh).as_bool() {
                continue;
            }
            let Some(rect) = get_window_rect(swh) else {
                continue;
            };
            let (tx, ty) = reachable_position(swh, target_x, target_y, rect.right - rect.left);
            let _ = SetWindowPos(
                swh,
                HWND(std::ptr::null_mut()),
                tx,
                ty,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    MOVE_IN_PROGRESS.store(false, Ordering::SeqCst);
}

/// Move sync: while VK_LBUTTON is held on a grouped fg window, follows the fg
/// window's own physical translation (not the cursor), so dragging inside a
/// window (e.g. panning a chart) or a resize does not move its siblings.
///
/// Translation is always computed from a reference snapshot to the current
/// foreground position. Accumulating per-poll deltas (the previous approach) lost
/// movement whenever a poll was skipped; that happens on every cross-DPI
/// transition because Windows resizes the dragged window while it crosses.
fn move_sync_loop(gm: Arc<Mutex<GroupManager>>) {
    let mut tracked_fg: isize = 0;
    // Sibling offsets relative to fg: sibling_pos = fg_pos + offset
    let mut offsets: HashMap<isize, (i32, i32)> = HashMap::new();
    // State for current drag
    let mut drag_active: bool = false;
    let mut drag_gesture: DragGesture = DragGesture::Move;
    // Reference point: fg top-left and sibling positions when the drag started,
    // or when a non-DPI layout change (restore/maximize) rebased it.
    let mut drag_start_fg: (i32, i32) = (0, 0);
    let mut drag_start_dpi: u32 = 0;
    // fg window rect at the previous poll of the current drag
    let mut drag_last_fg: RECT = RECT::default();
    // Sibling absolute positions at the drag reference point
    let mut drag_sibling_start: HashMap<isize, (i32, i32)> = HashMap::new();

    loop {
        std::thread::sleep(std::time::Duration::from_millis(8));

        if MOVE_CACHE_DIRTY.swap(false, Ordering::SeqCst) {
            tracked_fg = 0;
            offsets.clear();
            drag_active = false;
            drag_sibling_start.clear();
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        if SUPPRESSED.load(Ordering::SeqCst) {
            continue;
        }

        let fg = unsafe { GetForegroundWindow() };
        if fg.0.is_null() {
            continue;
        }
        let fg_h = fg.0 as isize;
        if unsafe { IsIconic(fg).as_bool() } {
            if fg_h == tracked_fg {
                tracked_fg = 0;
                offsets.clear();
                drag_active = false;
            }
            continue;
        }

        let gm_lock = match gm.try_lock() {
            Ok(g) => g,
            Err(_) => continue,
        };
        let gid = match gm_lock.find_group_for_hwnd(fg_h) {
            Some(g) => g.to_string(),
            None => {
                if tracked_fg != 0 {
                    tracked_fg = 0;
                    offsets.clear();
                    drag_active = false;
                }
                continue;
            }
        };
        if !gm_lock.movement_enabled(&gid) {
            // Independent movement invalidates every cached drag position.
            tracked_fg = 0;
            offsets.clear();
            drag_active = false;
            drag_sibling_start.clear();
            continue;
        }
        let siblings = gm_lock.get_sibling_hwnds(fg_h);
        if siblings.is_empty() {
            continue;
        }
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
                        if unsafe { !IsWindow(swh).as_bool() || IsIconic(swh).as_bool() } {
                            continue;
                        }
                        if let Some((sx, sy)) = get_window_pos(swh) {
                            if is_valid_pos(sx, sy) {
                                offsets.insert(sh, (sx - fx, sy - fy));
                            }
                        }
                    }
                }
            }
            continue;
        }

        let lb = lbutton_down();

        // Drag just started: left button pressed, have offsets
        if lb && !drag_active && !offsets.is_empty() {
            let Some(rect) = get_window_rect(fg) else {
                continue;
            };
            drag_active = true;
            drag_gesture = drag_gesture_at(fg, rect);
            drag_start_fg = (rect.left, rect.top);
            drag_start_dpi = unsafe { GetDpiForWindow(fg) };
            drag_last_fg = rect;
            drag_sibling_start = capture_sibling_positions(&siblings);
            continue;
        }

        // Drag ended: left button released. Flush the final translation first so a
        // flick released while still moving does not leave the group stretched.
        if !lb && drag_active {
            if drag_gesture == DragGesture::Move {
                if let Some(rect) = get_window_rect(fg) {
                    let dpi = unsafe { GetDpiForWindow(fg) };
                    let layout_jump =
                        rect_size(&rect) != rect_size(&drag_last_fg) && dpi == drag_start_dpi;
                    if !layout_jump {
                        translate_siblings(
                            &drag_sibling_start,
                            drag_start_fg,
                            (rect.left, rect.top),
                        );
                    }
                }
            }
            drag_active = false;
            drag_sibling_start.clear();
            // Re-capture offsets at new positions
            if let Some((fx, fy)) = get_window_pos(fg) {
                if is_valid_pos(fx, fy) {
                    offsets.clear();
                    for &sh in &siblings {
                        let swh = HWND(sh as *mut _);
                        if unsafe { !IsWindow(swh).as_bool() || IsIconic(swh).as_bool() } {
                            continue;
                        }
                        if let Some((sx, sy)) = get_window_pos(swh) {
                            if is_valid_pos(sx, sy) {
                                offsets.insert(sh, (sx - fx, sy - fy));
                            }
                        }
                    }
                }
            }
            continue;
        }

        // During drag: translate siblings by how far the fg window itself moved
        if drag_active {
            let Some(rect) = get_window_rect(fg) else {
                continue;
            };
            let previous = drag_last_fg;
            drag_last_fg = rect;
            if rect.left == previous.left && rect.top == previous.top {
                continue;
            }
            if drag_gesture == DragGesture::Resize {
                continue;
            }
            let dpi = unsafe { GetDpiForWindow(fg) };
            let dpi_changed = dpi != drag_start_dpi;
            if dpi_changed {
                drag_start_dpi = dpi;
            }
            let size_changed = rect_size(&rect) != rect_size(&previous);
            if size_changed && !dpi_changed {
                // A size change without a DPI change is a restore/maximize/layout
                // jump, not pointer translation: keep the group where it is and
                // rebase the reference point for the rest of the drag.
                drag_start_fg = (rect.left, rect.top);
                drag_sibling_start = capture_sibling_positions(&siblings);
                continue;
            }
            translate_siblings(
                &drag_sibling_start,
                drag_start_fg,
                (rect.left, rect.top),
            );
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

#[cfg(test)]
mod drag_sync_tests {
    use super::*;
    use std::collections::HashMap;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT { left, top, right, bottom }
    }

    fn point(x: i32, y: i32) -> POINT {
        POINT { x, y }
    }

    #[test]
    fn caption_strip_is_a_move_even_next_to_the_top_resize_border() {
        let window = rect(100, 100, 900, 600);
        // Three pixels below the frame is still caption: a DPI-change resize in
        // the middle of a title-bar drag must not be classified as an edge resize.
        assert_eq!(
            gesture_for_pointer(point(500, 111), window, 8, 23),
            DragGesture::Move
        );
    }

    #[test]
    fn edges_and_corners_are_resizes() {
        let window = rect(100, 100, 900, 600);
        for edge in [
            point(101, 300),
            point(899, 300),
            point(500, 101),
            point(500, 599),
            point(101, 101),
            point(899, 599),
        ] {
            assert_eq!(
                gesture_for_pointer(edge, window, 8, 23),
                DragGesture::Resize
            );
        }
    }

    #[test]
    fn client_area_drag_counts_as_a_move() {
        let window = rect(100, 100, 900, 600);
        assert_eq!(
            gesture_for_pointer(point(500, 300), window, 8, 23),
            DragGesture::Move
        );
    }

    #[test]
    fn translation_targets_come_from_the_drag_reference_and_current_position() {
        let mut starts = HashMap::new();
        starts.insert(2_isize, (500, 300));
        starts.insert(3_isize, (900, 300));
        let reference = (100, 100);

        let mut after_first_tick = translated_targets(&starts, reference, (160, 100));
        after_first_tick.sort();
        assert_eq!(after_first_tick, vec![(2, 560, 300), (3, 960, 300)]);

        // Even if an intermediate poll is missed, the next tick derives targets
        // from the reference snapshot, so no movement is lost.
        let mut after_skipped_tick = translated_targets(&starts, reference, (220, 120));
        after_skipped_tick.sort();
        assert_eq!(after_skipped_tick, vec![(2, 620, 320), (3, 1020, 320)]);
    }

    #[test]
    fn returning_to_the_drag_reference_restores_the_original_targets() {
        let mut starts = HashMap::new();
        starts.insert(2_isize, (500, 300));
        let back = translated_targets(&starts, (100, 100), (100, 100));
        assert_eq!(back, vec![(2, 500, 300)]);
    }
}
