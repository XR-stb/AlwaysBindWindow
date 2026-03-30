/// Screenshot-style lasso selection overlay.
///
/// Drag a rectangle → windows inside get selected → release = done.
/// ESC or right-click = cancel.

use crate::group::TrackedWindow;
use crate::platform;
use log::info;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::*;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::*;
#[cfg(target_os = "windows")]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows::core::w;

pub struct PickerResult {
    pub selected_windows: Vec<TrackedWindow>,
    pub cancelled: bool,
}

#[cfg(target_os = "windows")]
struct OverlayState {
    all_windows: Vec<(TrackedWindow, RECT)>,
    dragging: bool,
    start_x: i32,
    start_y: i32,
    current_x: i32,
    current_y: i32,
    done: bool,
    cancelled: bool,
    selected_hwnds: Vec<isize>,
    screen_x: i32,
    screen_y: i32,
}

#[cfg(target_os = "windows")]
static mut OVERLAY_STATE: Option<*mut OverlayState> = None;

#[cfg(target_os = "windows")]
impl OverlayState {
    fn drag_rect_screen(&self) -> RECT {
        RECT {
            left: self.start_x.min(self.current_x),
            top: self.start_y.min(self.current_y),
            right: self.start_x.max(self.current_x),
            bottom: self.start_y.max(self.current_y),
        }
    }

    fn drag_rect_client(&self) -> RECT {
        let r = self.drag_rect_screen();
        RECT {
            left: r.left - self.screen_x,
            top: r.top - self.screen_y,
            right: r.right - self.screen_x,
            bottom: r.bottom - self.screen_y,
        }
    }

    fn find_intersecting_windows(&self) -> Vec<isize> {
        let drag = self.drag_rect_screen();
        if (drag.right - drag.left) < 20 || (drag.bottom - drag.top) < 20 {
            return Vec::new();
        }

        let mut selected = Vec::new();

        // all_windows is in EnumWindows order (Z-order, top to bottom).
        // For each window, check if it has any VISIBLE area within the drag rect.
        // "Visible" = not covered by a higher-z window that's also in the drag area.

        // Collect windows that intersect the drag rect, preserving z-order
        let candidates: Vec<(isize, RECT)> = self.all_windows.iter()
            .filter(|(_, wr)| rects_intersect(&drag, wr))
            .map(|(w, wr)| (w.hwnd, *wr))
            .collect();

        // For each candidate, check if it has visible pixels in the drag area
        // by sampling points within its intersection with the drag rect.
        // If ANY sample point's topmost window (via z-order) is this window, it's visible.
        for (i, &(hwnd, ref wr)) in candidates.iter().enumerate() {
            // Compute intersection of window rect and drag rect
            let ix_left = drag.left.max(wr.left);
            let ix_top = drag.top.max(wr.top);
            let ix_right = drag.right.min(wr.right);
            let ix_bottom = drag.bottom.min(wr.bottom);
            if ix_left >= ix_right || ix_top >= ix_bottom { continue; }

            // Sample a grid of points within the intersection
            let w = ix_right - ix_left;
            let h = ix_bottom - ix_top;
            let step_x = (w / 4).max(1);
            let step_y = (h / 4).max(1);

            let mut visible = false;
            'outer: for sy in (ix_top..ix_bottom).step_by(step_y as usize) {
                for sx in (ix_left..ix_right).step_by(step_x as usize) {
                    // Check: is this point NOT covered by any higher-z candidate?
                    let mut covered = false;
                    for &(upper_hwnd, ref upper_wr) in &candidates[..i] {
                        if upper_hwnd != hwnd
                            && sx >= upper_wr.left && sx < upper_wr.right
                            && sy >= upper_wr.top && sy < upper_wr.bottom
                        {
                            covered = true;
                            break;
                        }
                    }
                    if !covered {
                        visible = true;
                        break 'outer;
                    }
                }
            }

            if visible {
                selected.push(hwnd);
            }
        }

        selected
    }
}

#[cfg(target_os = "windows")]
fn rects_intersect(a: &RECT, b: &RECT) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

#[cfg(target_os = "windows")]
pub fn run_picker_overlay() -> PickerResult {
    let raw_windows = platform::enumerate_windows();
    if raw_windows.len() < 2 {
        return PickerResult { selected_windows: Vec::new(), cancelled: true };
    }

    let mut all_windows: Vec<(TrackedWindow, RECT)> = Vec::new();
    unsafe {
        for w in raw_windows {
            let wh = HWND(w.hwnd as *mut _);
            let mut r = RECT::default();
            if GetWindowRect(wh, &mut r).is_ok() {
                all_windows.push((w, r));
            }
        }
    }

    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap();
        let class_name = w!("ABW_LassoOverlay");

        // Unregister first in case it was left over from a previous call
        let _ = UnregisterClassW(class_name, HINSTANCE(hinstance.0));

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(lasso_wndproc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let screen_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let overlay_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            class_name, w!(""),
            WS_POPUP | WS_VISIBLE,
            screen_x, screen_y, screen_w, screen_h,
            None, None, HINSTANCE(hinstance.0), None,
        ).unwrap();

        let _ = SetLayeredWindowAttributes(overlay_hwnd, COLORREF(0), 100, LWA_ALPHA);

        // Force overlay to get focus and input
        let _ = SetForegroundWindow(overlay_hwnd);
        let _ = SetFocus(overlay_hwnd);
        let _ = SetActiveWindow(overlay_hwnd);
        // Process pending messages to let the window fully initialize
        {
            let mut init_msg = MSG::default();
            while PeekMessageW(&mut init_msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&init_msg);
                DispatchMessageW(&init_msg);
            }
        }
        // Small delay to let the overlay fully render
        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut state = OverlayState {
            all_windows,
            dragging: false,
            start_x: 0, start_y: 0,
            current_x: 0, current_y: 0,
            done: false, cancelled: false,
            selected_hwnds: Vec::new(),
            screen_x, screen_y,
        };
        OVERLAY_STATE = Some(&mut state as *mut OverlayState);

        let _ = InvalidateRect(overlay_hwnd, None, true);

        // Manual message pump — NO PostQuitMessage involved
        let mut msg = MSG::default();
        while !state.done {
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT { break; }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                // Yield CPU when no messages
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        let _ = DestroyWindow(overlay_hwnd);
        OVERLAY_STATE = None;

        if state.cancelled || state.selected_hwnds.len() < 2 {
            if !state.cancelled && !state.selected_hwnds.is_empty() {
                println!("Only {} window — need at least 2. Try again.", state.selected_hwnds.len());
            }
            return PickerResult { selected_windows: Vec::new(), cancelled: true };
        }

        let selected: Vec<TrackedWindow> = state.all_windows.iter()
            .filter(|(w, _)| state.selected_hwnds.contains(&w.hwnd))
            .map(|(w, _)| w.clone())
            .collect();

        info!("Lasso selected {} windows", selected.len());
        PickerResult { selected_windows: selected, cancelled: false }
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn lasso_wndproc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut cr = RECT::default();
            let _ = GetClientRect(hwnd, &mut cr);

            let bg = CreateSolidBrush(COLORREF(0x00201818));
            FillRect(hdc, &cr, bg);
            let _ = DeleteObject(bg);

            if let Some(sp) = OVERLAY_STATE {
                let state = &*sp;
                for (w, wr) in &state.all_windows {
                    let r = RECT {
                        left: wr.left - state.screen_x, top: wr.top - state.screen_y,
                        right: wr.right - state.screen_x, bottom: wr.bottom - state.screen_y,
                    };
                    let sel = state.selected_hwnds.contains(&w.hwnd);
                    let fb = CreateSolidBrush(if sel { COLORREF(0x00104010) } else { COLORREF(0x00181212) });
                    FillRect(hdc, &r, fb);
                    let _ = DeleteObject(fb);

                    let pen = CreatePen(PS_SOLID, if sel { 3 } else { 1 },
                        if sel { COLORREF(0x0000FF00) } else { COLORREF(0x00444444) });
                    let op = SelectObject(hdc, pen);
                    let ob = SelectObject(hdc, GetStockObject(NULL_BRUSH));
                    Rectangle(hdc, r.left, r.top, r.right, r.bottom);
                    SelectObject(hdc, ob);
                    SelectObject(hdc, op);
                    let _ = DeleteObject(pen);

                    if sel {
                        let _ = SetBkMode(hdc, TRANSPARENT);
                        let _ = SetTextColor(hdc, COLORREF(0x0000FF88));
                        let label = format!("+ {}", w.process_name);
                        let lw: Vec<u16> = label.encode_utf16().collect();
                        let _ = TextOutW(hdc, r.left + 6, r.top + 4, &lw);
                    }
                }
                if state.dragging {
                    let dr = state.drag_rect_client();
                    let lp = CreatePen(PS_DASH, 2, COLORREF(0x0000CCFF));
                    let op = SelectObject(hdc, lp);
                    let ob = SelectObject(hdc, GetStockObject(NULL_BRUSH));
                    Rectangle(hdc, dr.left, dr.top, dr.right, dr.bottom);
                    SelectObject(hdc, ob);
                    SelectObject(hdc, op);
                    let _ = DeleteObject(lp);
                }
                let _ = SetBkMode(hdc, TRANSPARENT);
                let _ = SetTextColor(hdc, COLORREF(0x0000DDFF));
                let hint = if state.dragging {
                    format!("Selecting... ({} windows)", state.selected_hwnds.len())
                } else {
                    "Drag to select | ESC = cancel".to_string()
                };
                let hw: Vec<u16> = hint.encode_utf16().collect();
                let _ = TextOutW(hdc, (cr.right / 2 - 140).max(10), 16, &hw);
            }
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            if let Some(sp) = OVERLAY_STATE {
                let s = &mut *sp;
                s.start_x = (lparam.0 & 0xFFFF) as i16 as i32 + s.screen_x;
                s.start_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 + s.screen_y;
                s.current_x = s.start_x;
                s.current_y = s.start_y;
                s.dragging = true;
                s.selected_hwnds.clear();
                let _ = SetCapture(hwnd);
            }
            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            if let Some(sp) = OVERLAY_STATE {
                let s = &mut *sp;
                if s.dragging {
                    s.current_x = (lparam.0 & 0xFFFF) as i16 as i32 + s.screen_x;
                    s.current_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 + s.screen_y;
                    s.selected_hwnds = s.find_intersecting_windows();
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
            LRESULT(0)
        }

        WM_LBUTTONUP => {
            if let Some(sp) = OVERLAY_STATE {
                let s = &mut *sp;
                if s.dragging {
                    let _ = ReleaseCapture();
                    s.dragging = false;
                    s.current_x = (lparam.0 & 0xFFFF) as i16 as i32 + s.screen_x;
                    s.current_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 + s.screen_y;
                    s.selected_hwnds = s.find_intersecting_windows();
                    println!("[DEBUG] LButtonUp: selected {} windows, drag rect=({},{})→({},{})",
                        s.selected_hwnds.len(),
                        s.start_x, s.start_y, s.current_x, s.current_y);
                    if s.selected_hwnds.len() >= 2 {
                        // Immediately hide the overlay so user sees it close
                        let _ = ShowWindow(hwnd, SW_HIDE);
                        s.done = true;
                    } else {
                        s.selected_hwnds.clear();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
            }
            LRESULT(0)
        }

        WM_RBUTTONDOWN => {
            if let Some(sp) = OVERLAY_STATE {
                let s = &mut *sp;
                let _ = ShowWindow(hwnd, SW_HIDE);
                s.cancelled = true;
                s.done = true;
            }
            LRESULT(0)
        }

        WM_KEYDOWN => {
            if wparam.0 as u32 == 0x1B {
                if let Some(sp) = OVERLAY_STATE {
                    let s = &mut *sp;
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    s.cancelled = true;
                    s.done = true;
                }
            }
            LRESULT(0)
        }

        // IMPORTANT: Do NOT call PostQuitMessage here!
        // That would kill the parent event loop.
        WM_DESTROY => LRESULT(0),

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(not(target_os = "windows"))]
pub fn run_picker_overlay() -> PickerResult {
    PickerResult { selected_windows: Vec::new(), cancelled: true }
}
