//! Multi-monitor selection overlay with an opaque native confirmation toolbar.
use super::PickerResult;
use crate::window_selection::{self as selection, Bounds};
use crate::{
    group::{MovementMode, TrackedWindow},
    i18n::t,
    native_ui as ui,
};
use std::cell::RefCell;
use windows::core::{w, PCWSTR};
use windows::Win32::System::SystemServices::{SS_ENDELLIPSIS, SS_NOPREFIX};
use windows::Win32::UI::{
    Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*,
};
use windows::Win32::{Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW};

const ID_BIND: usize = 1;
const ID_CANCEL: usize = 2;
const ID_CLEAR: usize = 3;
const ID_MODE: usize = 4;

struct State {
    windows: Vec<TrackedWindow>,
    bounds: Vec<(isize, Bounds)>,
    selected: Vec<isize>,
    drag: Option<(POINT, POINT)>,
    origin: POINT,
    overlay: HWND,
    toolbar: HWND,
    status: HWND,
    bind: HWND,
    controls: [HWND; 5],
    expanded: bool,
    last_status: String,
    movement: MovementMode,
    done: bool,
    cancelled: bool,
    dpi: i32,
}

thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn drag_bounds(start: POINT, end: POINT) -> Bounds {
    Bounds {
        left: start.x.min(end.x),
        top: start.y.min(end.y),
        right: start.x.max(end.x),
        bottom: start.y.max(end.y),
    }
}

fn preview(state: &State) -> Vec<isize> {
    let mut selected = state.selected.clone();
    if let Some((start, end)) = state.drag {
        if (start.x - end.x).abs() >= (6 * state.dpi / 96).max(4)
            || (start.y - end.y).abs() >= (6 * state.dpi / 96).max(4)
        {
            selection::append(
                &mut selected,
                selection::intersecting(&state.bounds, drag_bounds(start, end)),
            );
        }
    }
    selected
}

unsafe fn round_tip(hwnd: HWND, width: i32, height: i32, dpi: i32) {
    let radius = 16 * dpi / 96;
    let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius);
    if !region.0.is_null() && SetWindowRgn(hwnd, region, true) == 0 {
        let _ = DeleteObject(region);
    }
}

unsafe fn update(notice: Option<&str>) {
    // Native calls may reenter the window proc: release the borrow first.
    let data = STATE.with(|cell| {
        let mut slot = cell.borrow_mut();
        slot.as_mut().map(|s| {
            let count = s.selected.len();
            let expanded = count >= 2;
            let text = notice.map(str::to_owned).unwrap_or_else(|| {
                if count == 0 {
                    t("overlay.hint").to_string()
                } else if count == 1 {
                    t("overlay.one_selected").to_string()
                } else {
                    format!(
                        "{}{}  ·  {}",
                        t("picker.selected"),
                        count,
                        t("overlay.ready")
                    )
                }
            });
            let changed_text = text != s.last_status;
            s.last_status = text.clone();
            let changed_layout = expanded != s.expanded;
            s.expanded = expanded;
            (
                s.overlay,
                s.toolbar,
                s.status,
                s.bind,
                s.controls,
                s.dpi,
                expanded,
                changed_layout,
                text,
                changed_text,
            )
        })
    });
    if let Some((
        overlay,
        toolbar,
        status,
        bind,
        controls,
        dpi,
        expanded,
        changed_layout,
        text,
        changed_text,
    )) = data
    {
        if changed_text {
            let _ = SetWindowTextW(status, PCWSTR(wide(&text).as_ptr()));
        }
        if changed_layout {
            if !expanded {
                let _ = SetFocus(overlay);
            }
            for control in controls {
                let _ = ShowWindow(control, if expanded { SW_SHOWNA } else { SW_HIDE });
            }
            let _ = EnableWindow(bind, expanded);
            let mut rect = RECT::default();
            let _ = GetWindowRect(toolbar, &mut rect);
            let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(monitor, &mut info);
            let height = if expanded { 108 } else { 56 } * dpi / 96;
            let top = (rect.bottom - height).max(info.rcWork.top);
            let _ = SetWindowPos(
                toolbar,
                None,
                rect.left,
                top,
                rect.right - rect.left,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            round_tip(toolbar, rect.right - rect.left, height, dpi);
            let _ = InvalidateRect(toolbar, None, false);
        }
        let _ = InvalidateRect(overlay, None, false);
    }
}

struct PaintBuffer {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    width: i32,
    height: i32,
}

impl PaintBuffer {
    unsafe fn new(target: HDC, width: i32, height: i32) -> Option<Self> {
        let dc = CreateCompatibleDC(target);
        if dc.0.is_null() {
            return None;
        }
        let bitmap = CreateCompatibleBitmap(target, width, height);
        if bitmap.0.is_null() {
            let _ = DeleteDC(dc);
            return None;
        }
        let previous = SelectObject(dc, bitmap);
        Some(Self {
            dc,
            bitmap,
            previous,
            width,
            height,
        })
    }
}

impl Drop for PaintBuffer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.dc);
        }
    }
}

thread_local! { static BUFFER: RefCell<Option<PaintBuffer>> = const { RefCell::new(None) }; }

unsafe fn paint_overlay(dc: HDC, rect: &RECT) {
    ui::fill(dc, rect, ui::TEXT);
    STATE.with(|cell| {
        let slot = cell.borrow();
        let Some(s) = slot.as_ref() else {
            return;
        };
        let selected = preview(s);
        let pen = CreatePen(PS_SOLID, (2 * s.dpi / 96).max(1), COLORREF(ui::SURFACE));
        let old_pen = SelectObject(dc, pen);
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        for (_, r) in s
            .bounds
            .iter()
            .rev()
            .filter(|(id, _)| selected.contains(id))
        {
            let _ = Rectangle(
                dc,
                r.left - s.origin.x,
                r.top - s.origin.y,
                r.right - s.origin.x,
                r.bottom - s.origin.y,
            );
        }
        SelectObject(dc, old_pen);
        let _ = DeleteObject(pen);
        if let Some((start, end)) = s.drag {
            let r = drag_bounds(start, end);
            let pen = CreatePen(PS_DASH, 1, COLORREF(ui::SURFACE));
            let old_pen = SelectObject(dc, pen);
            let _ = Rectangle(
                dc,
                r.left - s.origin.x,
                r.top - s.origin.y,
                r.right - s.origin.x,
                r.bottom - s.origin.y,
            );
            SelectObject(dc, old_pen);
            let _ = DeleteObject(pen);
        }
        SelectObject(dc, old_brush);
    });
}

unsafe fn confirm() {
    let available = crate::picker::get_pickable_windows();
    let stale = STATE.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return false;
        };
        if s.drag.is_some() || s.selected.len() < 2 {
            return false;
        }
        let old_count = s.selected.len();
        s.selected.retain(|id| {
            s.windows.iter().find(|w| w.hwnd == *id).is_some_and(|old| {
                available.iter().any(|new| {
                    new.hwnd == old.hwnd
                        && new.process_name == old.process_name
                        && new.class_name == old.class_name
                })
            })
        });
        if s.selected.len() != old_count {
            return true;
        }
        s.cancelled = false;
        s.done = true;
        false
    });
    update(if stale {
        Some(t("overlay.stale"))
    } else {
        None
    });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let is_overlay =
                STATE.with(|cell| cell.borrow().as_ref().is_some_and(|s| s.overlay == hwnd));
            if is_overlay {
                BUFFER.with(|cell| {
                    let mut slot = cell.borrow_mut();
                    if !slot
                        .as_ref()
                        .is_some_and(|b| b.width == rect.right && b.height == rect.bottom)
                    {
                        *slot = PaintBuffer::new(dc, rect.right, rect.bottom);
                    }
                    if let Some(buffer) = slot.as_ref() {
                        paint_overlay(buffer.dc, &rect);
                        let dirty = ps.rcPaint;
                        let _ = BitBlt(
                            dc,
                            dirty.left,
                            dirty.top,
                            dirty.right - dirty.left,
                            dirty.bottom - dirty.top,
                            buffer.dc,
                            dirty.left,
                            dirty.top,
                            SRCCOPY,
                        );
                    } else {
                        paint_overlay(dc, &rect);
                    }
                });
            } else {
                ui::fill(dc, &rect, ui::BG);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let dpi = STATE.with(|cell| {
                cell.borrow()
                    .as_ref()
                    .filter(|s| s.toolbar == hwnd)
                    .map(|s| s.dpi)
            });
            if let Some(dpi) = dpi {
                let mut point = POINT::default();
                let mut rect = RECT::default();
                let _ = GetCursorPos(&mut point);
                let _ = GetWindowRect(hwnd, &mut rect);
                if point.y < rect.top + 48 * dpi / 96 {
                    return LRESULT(HTCAPTION as isize);
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CTLCOLORSTATIC => {
            let dc = HDC(wp.0 as *mut _);
            let _ = SetTextColor(dc, COLORREF(ui::TEXT));
            let _ = SetBkColor(dc, COLORREF(ui::BG));
            let _ = SetDCBrushColor(dc, COLORREF(ui::BG));
            LRESULT(GetStockObject(DC_BRUSH).0 as isize)
        }
        WM_DRAWITEM => {
            let item = &*(lp.0 as *const DRAWITEMSTRUCT);
            if item.CtlType == ODT_BUTTON {
                ui::draw_button(item, item.CtlID == ID_BIND as u32);
            }
            LRESULT(1)
        }
        WM_COMMAND => {
            let id = wp.0 & 0xffff;
            let notification = (wp.0 >> 16) as u32;
            if id == ID_MODE && notification == CBN_SELCHANGE {
                let choice = ui::movement_choice(HWND(lp.0 as *mut _));
                STATE.with(|cell| {
                    if let Some(s) = cell.borrow_mut().as_mut() {
                        s.movement = choice;
                    }
                });
            } else if notification == BN_CLICKED {
                match id {
                    ID_BIND => confirm(),
                    ID_CANCEL => STATE.with(|cell| {
                        if let Some(s) = cell.borrow_mut().as_mut() {
                            s.done = true;
                        }
                    }),
                    ID_CLEAR => {
                        STATE.with(|cell| {
                            if let Some(s) = cell.borrow_mut().as_mut() {
                                s.selected.clear();
                            }
                        });
                        update(None);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
            let is_overlay =
                STATE.with(|cell| cell.borrow().as_ref().is_some_and(|s| s.overlay == hwnd));
            if !is_overlay {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            if msg == WM_MOUSEMOVE
                && !STATE.with(|cell| cell.borrow().as_ref().is_some_and(|s| s.drag.is_some()))
            {
                return LRESULT(0);
            }
            // Cursor screen coordinates avoid signed 16-bit LPARAM truncation on wide desktops.
            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            STATE.with(|cell| {
                let mut slot = cell.borrow_mut();
                let Some(s) = slot.as_mut() else {
                    return;
                };
                match msg {
                    WM_LBUTTONDOWN => s.drag = Some((point, point)),
                    WM_MOUSEMOVE => {
                        if let Some((_, end)) = s.drag.as_mut() {
                            *end = point;
                        }
                    }
                    WM_LBUTTONUP => {
                        if let Some((start, _)) = s.drag.take() {
                            let threshold = (6 * s.dpi / 96).max(4);
                            if (start.x - point.x).abs() < threshold
                                && (start.y - point.y).abs() < threshold
                            {
                                if let Some(id) = selection::hit_test(&s.bounds, point.x, point.y) {
                                    selection::toggle(&mut s.selected, id);
                                }
                            } else {
                                selection::append(
                                    &mut s.selected,
                                    selection::intersecting(&s.bounds, drag_bounds(start, point)),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            });
            if msg == WM_LBUTTONDOWN {
                let _ = SetFocus(hwnd);
                SetCapture(hwnd);
            }
            if msg == WM_LBUTTONUP {
                let _ = ReleaseCapture();
            }
            if msg == WM_MOUSEMOVE {
                let _ = InvalidateRect(hwnd, None, false);
            } else {
                update(None);
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            let changed = STATE.with(|cell| {
                cell.borrow_mut()
                    .as_mut()
                    .is_some_and(|s| s.drag.take().is_some())
            });
            if changed {
                update(None);
            }
            LRESULT(0)
        }
        WM_CLOSE | WM_RBUTTONDOWN => {
            STATE.with(|cell| {
                if let Some(s) = cell.borrow_mut().as_mut() {
                    s.done = true;
                }
            });
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn run_picker_overlay(global_sync_move: bool) -> PickerResult {
    log::info!("Opening window selection overlay");
    let cancelled = || PickerResult {
        selected_windows: vec![],
        cancelled: true,
        movement: MovementMode::Inherit,
    };
    unsafe {
        let _dpi_scope = ui::DpiScope::new();
        let dpi = GetDpiForSystem() as i32;
        let pickable = crate::picker::get_pickable_windows();
        let mut windows = Vec::new();
        let mut bounds = Vec::new();
        // Preserve front-to-back order for hit testing, rather than the sorted list order.
        for window in crate::platform::enumerate_windows() {
            if !pickable.iter().any(|w| w.hwnd == window.hwnd) {
                continue;
            }
            let hwnd = HWND(window.hwnd as *mut _);
            let mut rect = RECT::default();
            if IsIconic(hwnd).as_bool() || GetWindowRect(hwnd, &mut rect).is_err() {
                continue;
            }
            bounds.push((
                window.hwnd,
                Bounds {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                },
            ));
            windows.push(window);
        }
        log::info!("Selection has {} eligible visible windows", windows.len());
        if windows.len() < 2 {
            let _ = MessageBoxW(
                None,
                PCWSTR(wide(t("msg.need2")).as_ptr()),
                w!("AlwaysBindWindow"),
                MB_OK,
            );
            return cancelled();
        }
        let instance = HINSTANCE(GetModuleHandleW(None).unwrap_or_default().0);
        let class = w!("ABW_MultiSelect");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let origin = POINT {
            x: GetSystemMetrics(SM_XVIRTUALSCREEN),
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
        };
        let Ok(overlay) = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            class,
            w!(""),
            WS_POPUP,
            origin.x,
            origin.y,
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
            None,
            None,
            instance,
            None,
        ) else {
            return cancelled();
        };
        let _ = SetLayeredWindowAttributes(overlay, COLORREF(0), 100, LWA_ALPHA);
        let font = ui::font(14, 400, dpi);
        let style = WS_POPUP | WS_CLIPCHILDREN;
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(
            MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        );
        let width = (560 * dpi / 96).min(monitor.rcWork.right - monitor.rcWork.left);
        let height = 56 * dpi / 96;
        let left = monitor.rcWork.left + (monitor.rcWork.right - monitor.rcWork.left - width) / 2;
        let bottom = monitor.rcWork.bottom - 24 * dpi / 96;
        let rect = RECT {
            left,
            top: bottom - height,
            right: left + width,
            bottom,
        };
        let Ok(toolbar) = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class,
            PCWSTR(wide(t("overlay.title")).as_ptr()),
            style,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            overlay,
            None,
            instance,
            None,
        ) else {
            let _ = DestroyWindow(overlay);
            let _ = DeleteObject(font);
            return cancelled();
        };
        round_tip(toolbar, width, height, dpi);
        let setup = (|| -> windows::core::Result<(HWND, HWND, HWND, [HWND; 5])> {
            let control = |class,
                           label: &str,
                           style,
                           id,
                           x,
                           y,
                           width,
                           height|
             -> windows::core::Result<HWND> {
                let h = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    PCWSTR(wide(label).as_ptr()),
                    WS_CHILD | WS_VISIBLE | style,
                    x * dpi / 96,
                    y * dpi / 96,
                    width * dpi / 96,
                    height * dpi / 96,
                    toolbar,
                    HMENU(id as *mut _),
                    instance,
                    None,
                )?;
                SendMessageW(h, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
                Ok(h)
            };
            let status = control(
                w!("STATIC"),
                "",
                WINDOW_STYLE(SS_ENDELLIPSIS.0 | SS_NOPREFIX.0),
                0,
                20,
                18,
                520,
                24,
            )?;
            let label = control(
                w!("STATIC"),
                t("overlay.movement"),
                WINDOW_STYLE(0),
                0,
                20,
                67,
                80,
                28,
            )?;
            let mode = control(
                w!("COMBOBOX"),
                "",
                WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
                ID_MODE,
                100,
                61,
                160,
                140,
            )?;
            ui::init_movement_choices(mode, global_sync_move);
            let bind = control(
                w!("BUTTON"),
                t("overlay.bind"),
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                ID_BIND,
                272,
                60,
                100,
                34,
            )?;
            let clear = control(
                w!("BUTTON"),
                t("overlay.clear"),
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                ID_CLEAR,
                380,
                60,
                76,
                34,
            )?;
            let cancel = control(
                w!("BUTTON"),
                t("picker.cancel"),
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                ID_CANCEL,
                464,
                60,
                76,
                34,
            )?;
            let controls = [label, mode, clear, bind, cancel];
            for h in controls {
                let _ = ShowWindow(h, SW_HIDE);
            }
            for h in [clear, bind, cancel] {
                ui::style_button(h);
            }
            Ok((status, bind, mode, controls))
        })();
        let Ok((status, bind, mode, controls)) = setup else {
            let _ = DestroyWindow(toolbar);
            let _ = DestroyWindow(overlay);
            let _ = DeleteObject(font);
            return cancelled();
        };
        STATE.with(|cell| {
            *cell.borrow_mut() = Some(State {
                windows,
                bounds,
                selected: vec![],
                drag: None,
                origin,
                overlay,
                toolbar,
                status,
                bind,
                controls,
                expanded: false,
                last_status: String::new(),
                movement: MovementMode::Inherit,
                done: false,
                cancelled: true,
                dpi,
            })
        });
        update(None);
        log::info!("Selection overlay and confirmation panel created");
        let _ = ShowWindow(overlay, SW_SHOW);
        let _ = ShowWindow(toolbar, SW_SHOWNOACTIVATE);
        let _ = SetForegroundWindow(overlay);
        let _ = SetFocus(overlay);
        let mut message = MSG::default();
        while STATE.with(|cell| cell.borrow().as_ref().is_some_and(|s| !s.done)) {
            let status = GetMessageW(&mut message, None, 0, 0).0;
            if status <= 0 {
                if status == 0 {
                    PostQuitMessage(message.wParam.0 as i32);
                }
                break;
            }
            let dropdown = SendMessageW(mode, CB_GETDROPPEDSTATE, WPARAM(0), LPARAM(0)).0 != 0;
            if message.message == WM_KEYDOWN
                && dropdown
                && (message.wParam.0 == VK_RETURN.0 as usize
                    || message.wParam.0 == VK_ESCAPE.0 as usize)
            {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
                continue;
            }
            if message.message == WM_KEYDOWN && !dropdown {
                if message.wParam.0 == VK_ESCAPE.0 as usize {
                    STATE.with(|cell| {
                        if let Some(s) = cell.borrow_mut().as_mut() {
                            s.done = true;
                        }
                    });
                    continue;
                }
                if message.wParam.0 == VK_RETURN.0 as usize {
                    if message.lParam.0 & (1 << 30) != 0 {
                        continue;
                    }
                    let focused_id = GetDlgCtrlID(GetFocus()) as usize;
                    if focused_id == ID_CANCEL || focused_id == ID_CLEAR {
                        SendMessageW(toolbar, WM_COMMAND, WPARAM(focused_id), LPARAM(0));
                    } else {
                        confirm();
                    }
                    continue;
                }
                if message.wParam.0 == VK_TAB.0 as usize && message.hwnd == overlay {
                    if STATE.with(|cell| cell.borrow().as_ref().is_some_and(|s| s.expanded)) {
                        let _ = SetFocus(mode);
                    }
                    continue;
                }
            }
            if !IsDialogMessageW(toolbar, &message).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        let result = STATE
            .with(|cell| cell.borrow_mut().take())
            .map(|s| PickerResult {
                selected_windows: s
                    .windows
                    .into_iter()
                    .filter(|w| s.selected.contains(&w.hwnd))
                    .collect(),
                cancelled: s.cancelled,
                movement: s.movement,
            })
            .unwrap_or_else(cancelled);
        BUFFER.with(|cell| cell.borrow_mut().take());
        let _ = ReleaseCapture();
        let _ = DestroyWindow(toolbar);
        let _ = DestroyWindow(overlay);
        let _ = DeleteObject(font);
        result
    }
}
