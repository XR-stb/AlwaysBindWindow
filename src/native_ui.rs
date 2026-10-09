//! Shared desktop palette, controls and a consistent logical-pixel coordinate space.
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

// COLORREF uses BGR, while DESIGN.md records RGB.
pub const BG: u32 = 0x00F8F7F5; // #F5F7F8
pub const SURFACE: u32 = 0x00FFFFFF;
pub const TEXT: u32 = 0x00382B20; // #202B38
pub const MUTED: u32 = 0x0079695B; // #5B6979
pub const BORDER: u32 = 0x00E5DED8; // #D8DEE5
pub const ACCENT: u32 = 0x00786D00; // #006D78
pub const ACCENT_HOVER: u32 = 0x008B7F00;
pub const SELECTED: u32 = 0x00F5F3E5; // #E5F3F5

pub struct DpiScope(DPI_AWARENESS_CONTEXT);
impl DpiScope {
    pub unsafe fn new() -> Self {
        let controls = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&controls);
        // Windows scales the entire dialog when moved between monitors. Never
        // mix physical window dimensions with independently scaled controls.
        Self(SetThreadDpiAwarenessContext(
            DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
        ))
    }
}
impl Drop for DpiScope {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}

pub unsafe fn fill(dc: HDC, rect: &RECT, color: u32) {
    let _ = SetDCBrushColor(dc, COLORREF(color));
    FillRect(dc, rect, HBRUSH(GetStockObject(DC_BRUSH).0));
}

pub unsafe fn rounded(dc: HDC, rect: &RECT, radius: i32, color: u32, border: u32) {
    let brush = CreateSolidBrush(COLORREF(color));
    let pen = CreatePen(PS_SOLID, 1, COLORREF(border));
    let old_brush = SelectObject(dc, brush);
    let old_pen = SelectObject(dc, pen);
    let _ = RoundRect(
        dc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius * 2,
        radius * 2,
    );
    SelectObject(dc, old_pen);
    SelectObject(dc, old_brush);
    let _ = DeleteObject(pen);
    let _ = DeleteObject(brush);
}

pub unsafe fn text(
    dc: HDC,
    rect: &RECT,
    value: &str,
    font: HFONT,
    color: u32,
    flags: DRAW_TEXT_FORMAT,
) {
    let old = SelectObject(dc, font);
    let _ = SetBkMode(dc, TRANSPARENT);
    let _ = SetTextColor(dc, COLORREF(color));
    let mut value: Vec<u16> = value.encode_utf16().collect();
    let mut rect = *rect;
    DrawTextW(dc, &mut value, &mut rect, flags);
    SelectObject(dc, old);
}

pub unsafe fn font(size: i32, weight: i32, dpi: i32) -> HFONT {
    CreateFontW(
        -size * dpi / 96,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        1,
        0,
        0,
        5,
        0,
        w!("Segoe UI"),
    )
}

pub unsafe fn style_button(hwnd: HWND) {
    let _ = SetWindowSubclass(hwnd, Some(button_proc), 1, 0);
}

unsafe extern "system" fn button_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            let _ = SetPropW(hwnd, w!("ABW.Hover"), HANDLE(1 as *mut _));
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tracking);
            let _ = InvalidateRect(hwnd, None, false);
        }
        0x02A3 => {
            let _ = RemovePropW(hwnd, w!("ABW.Hover"));
            let _ = InvalidateRect(hwnd, None, false);
        }
        WM_SETCURSOR if IsWindowEnabled(hwnd).as_bool() => {
            SetCursor(LoadCursorW(None, IDC_HAND).unwrap_or_default());
            return LRESULT(1);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

pub unsafe fn draw_button(item: &DRAWITEMSTRUCT, primary: bool) {
    let dpi = GetDpiForWindow(item.hwndItem).max(96) as i32;
    let disabled = (item.itemState.0 & ODS_DISABLED.0) != 0;
    let hot = !GetPropW(item.hwndItem, w!("ABW.Hover")).0.is_null();
    let pressed = (item.itemState.0 & ODS_SELECTED.0) != 0;
    let background = if disabled {
        BORDER
    } else if primary {
        if hot || pressed {
            ACCENT_HOVER
        } else {
            ACCENT
        }
    } else if hot || pressed {
        SELECTED
    } else {
        SURFACE
    };
    fill(item.hDC, &item.rcItem, BG);
    let mut rect = item.rcItem;
    rect.left += 1;
    rect.top += 1;
    rect.right -= 1;
    rect.bottom -= 1;
    rounded(
        item.hDC,
        &rect,
        7 * dpi / 96,
        background,
        if primary && !disabled {
            background
        } else {
            BORDER
        },
    );
    let mut label = vec![0u16; (GetWindowTextLengthW(item.hwndItem) + 1) as usize];
    let len = GetWindowTextW(item.hwndItem, &mut label);
    let font = HFONT(SendMessageW(item.hwndItem, WM_GETFONT, WPARAM(0), LPARAM(0)).0 as *mut _);
    text(
        item.hDC,
        &rect,
        &String::from_utf16_lossy(&label[..len as usize]),
        font,
        if disabled {
            MUTED
        } else if primary {
            SURFACE
        } else {
            TEXT
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    if (item.itemState.0 & ODS_FOCUS.0) != 0 {
        rect.left += 4;
        rect.top += 4;
        rect.right -= 4;
        rect.bottom -= 4;
        let _ = DrawFocusRect(item.hDC, &rect);
    }
}

pub unsafe fn draw_window_row(item: &DRAWITEMSTRUCT) {
    fill(item.hDC, &item.rcItem, SURFACE);
    if item.itemID == u32::MAX {
        return;
    }
    let dpi = GetDpiForWindow(item.hwndItem).max(96) as i32;
    let s = |n| n * dpi / 96;
    let len = SendMessageW(
        item.hwndItem,
        LB_GETTEXTLEN,
        WPARAM(item.itemID as usize),
        LPARAM(0),
    )
    .0;
    if len < 0 {
        return;
    }
    let mut value = vec![0u16; len as usize + 1];
    SendMessageW(
        item.hwndItem,
        LB_GETTEXT,
        WPARAM(item.itemID as usize),
        LPARAM(value.as_mut_ptr() as isize),
    );
    let value = String::from_utf16_lossy(&value[..len as usize]);
    let (title, app) = value.split_once('\t').unwrap_or((&value, ""));
    let selected = (item.itemState.0 & ODS_SELECTED.0) != 0;
    let mut row = item.rcItem;
    row.left += s(6);
    row.right -= s(6);
    row.top += s(3);
    row.bottom -= s(3);
    if selected {
        rounded(item.hDC, &row, s(8), SELECTED, SELECTED);
    }
    let check = RECT {
        left: row.left + s(14),
        top: row.top + s(18),
        right: row.left + s(34),
        bottom: row.top + s(38),
    };
    rounded(
        item.hDC,
        &check,
        s(4),
        if selected { ACCENT } else { SURFACE },
        if selected { ACCENT } else { BORDER },
    );
    if selected {
        let pen = CreatePen(PS_SOLID, s(2).max(1), COLORREF(SURFACE));
        let old = SelectObject(item.hDC, pen);
        let _ = MoveToEx(item.hDC, check.left + s(4), check.top + s(10), None);
        let _ = LineTo(item.hDC, check.left + s(8), check.top + s(14));
        let _ = LineTo(item.hDC, check.left + s(16), check.top + s(6));
        SelectObject(item.hDC, old);
        let _ = DeleteObject(pen);
    }
    let body_font =
        HFONT(SendMessageW(item.hwndItem, WM_GETFONT, WPARAM(0), LPARAM(0)).0 as *mut _);
    let mut text_rect = RECT {
        left: row.left + s(48),
        top: row.top + s(7),
        right: row.right - s(14),
        bottom: row.top + s(30),
    };
    text(
        item.hDC,
        &text_rect,
        title,
        body_font,
        TEXT,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    text_rect.top += s(25);
    text_rect.bottom += s(23);
    text(
        item.hDC,
        &text_rect,
        app,
        body_font,
        MUTED,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    if (item.itemState.0 & ODS_FOCUS.0) != 0 {
        let _ = DrawFocusRect(item.hDC, &row);
    }
}

pub unsafe fn centered_rect(
    width: i32,
    height: i32,
    dpi: u32,
    style: WINDOW_STYLE,
    ex: WINDOW_EX_STYLE,
) -> RECT {
    let mut point = POINT::default();
    let _ = GetCursorPos(&mut point);
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = GetMonitorInfoW(monitor, &mut info);
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: width * dpi as i32 / 96,
        bottom: height * dpi as i32 / 96,
    };
    let _ = AdjustWindowRectExForDpi(&mut rect, style, false, ex, dpi);
    let w = (rect.right - rect.left).min(info.rcWork.right - info.rcWork.left);
    let h = (rect.bottom - rect.top).min(info.rcWork.bottom - info.rcWork.top);
    let x = info.rcWork.left + (info.rcWork.right - info.rcWork.left - w) / 2;
    let y = info.rcWork.top + (info.rcWork.bottom - info.rcWork.top - h) / 2;
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

/// Shared native dropdown for group overrides in both selection workflows.
pub unsafe fn init_movement_choices(combo: HWND, global: bool) {
    use crate::{group::MovementMode, i18n::t};
    for mode in MovementMode::ALL {
        let label = if mode == MovementMode::Inherit {
            format!(
                "{} · {}",
                mode.label(),
                if global {
                    t("movement.on")
                } else {
                    t("movement.off")
                }
            )
        } else {
            mode.label().to_string()
        };
        let wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
        SendMessageW(
            combo,
            CB_ADDSTRING,
            WPARAM(0),
            LPARAM(wide.as_ptr() as isize),
        );
    }
    SendMessageW(combo, CB_SETCURSEL, WPARAM(0), LPARAM(0));
}

pub unsafe fn movement_choice(combo: HWND) -> crate::group::MovementMode {
    crate::group::MovementMode::ALL
        .get(SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as usize)
        .copied()
        .unwrap_or_default()
}
