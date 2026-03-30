/// Hotkey settings dialog — modern styled Win32 window for recording key combos

#[cfg(target_os = "windows")]
pub use win_impl::*;

#[cfg(not(target_os = "windows"))]
pub fn show_hotkey_dialog(
    _bind: &crate::settings::HotkeyConfig,
    _unbind_cursor: &crate::settings::HotkeyConfig,
    _unbind_all: &crate::settings::HotkeyConfig,
) -> Option<HotkeyDialogResult> {
    println!("Hotkey settings dialog is only supported on Windows.");
    None
}

#[derive(Debug, Clone)]
pub struct HotkeyDialogResult {
    pub bind: crate::settings::HotkeyConfig,
    pub unbind_cursor: crate::settings::HotkeyConfig,
    pub unbind_all: crate::settings::HotkeyConfig,
}

#[cfg(target_os = "windows")]
mod win_impl {
    use super::HotkeyDialogResult;
    use crate::i18n::t;
    use crate::settings::HotkeyConfig;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows::core::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::HiDpi::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::*;

    // ── Colors ──────────────────────────────────────────────────
    const CLR_BG: u32          = 0x00FFFFFF;        // white body background
    const CLR_HEADER_BG: u32   = 0x00443322;        // dark warm header (BGR: #223344)
    const CLR_HEADER_TXT: u32  = 0x00FFFFFF;        // white text on header
    const CLR_LABEL: u32       = 0x00555555;        // dark gray label
    const CLR_EDIT_BG: u32     = 0x00F5F5F5;        // light gray edit bg
    const CLR_EDIT_BORDER: u32 = 0x00CCCCCC;        // default edit border
    const CLR_EDIT_FOCUS: u32  = 0x00D4BC00;        // accent blue-teal border on focus (BGR for #00BCD4)
    const CLR_EDIT_TEXT: u32   = 0x00333333;        // edit text
    const CLR_EDIT_REC: u32    = 0x00D4BC00;        // recording text color (teal)
    const CLR_BTN_PRIMARY: u32 = 0x00D4BC00;        // primary button bg (#00BCD4 in BGR)
    const CLR_BTN_PRI_HOV: u32 = 0x00E8D040;       // primary hover
    const CLR_BTN_PRI_TXT: u32 = 0x00FFFFFF;        // white text
    const CLR_BTN_SEC: u32     = 0x00EEEEEE;        // secondary button bg
    const CLR_BTN_SEC_HOV: u32 = 0x00DDDDDD;        // secondary hover
    const CLR_BTN_SEC_TXT: u32 = 0x00444444;        // secondary text
    const CLR_HINT: u32        = 0x00999999;         // hint text color
    const CLR_SEPARATOR: u32   = 0x00E0E0E0;        // separator line

    // ── Control IDs ─────────────────────────────────────────────
    const IDC_EDIT_BIND: i32 = 1001;
    const IDC_EDIT_UNBIND_CURSOR: i32 = 1002;
    const IDC_EDIT_UNBIND_ALL: i32 = 1003;
    const IDC_BTN_SAVE: i32 = 2001;
    const IDC_BTN_CANCEL: i32 = 2002;
    const IDC_BTN_RESET: i32 = 2003;

    // Window layout constants (will be DPI-scaled)
    const WIN_W: i32 = 480;
    const WIN_H: i32 = 340;

    // WM_MOUSELEAVE is not exported by the windows crate
    const WM_MOUSELEAVE_MSG: u32 = 0x02A3;
    const HEADER_H: i32 = 56;
    const MARGIN: i32 = 24;
    const ROW_H: i32 = 36;
    const ROW_GAP: i32 = 16;
    const LABEL_W: i32 = 130;
    const EDIT_H: i32 = 32;
    const BTN_W: i32 = 100;
    const BTN_H: i32 = 34;
    const CORNER_R: i32 = 6;

    thread_local! {
        static DIALOG_STATE: RefCell<DialogState> = RefCell::new(DialogState::default());
    }

    static ACTIVE_FIELD: AtomicIsize = AtomicIsize::new(0);

    #[derive(Default)]
    struct DialogState {
        hwnd_main: isize,
        hwnd_edit_bind: isize,
        hwnd_edit_unbind_cursor: isize,
        hwnd_edit_unbind_all: isize,
        bind: HotkeyConfig,
        unbind_cursor: HotkeyConfig,
        unbind_all: HotkeyConfig,
        saved: bool,
        h_font: isize,
        h_font_label: isize,
        h_font_header: isize,
        h_font_hint: isize,
        scale: f32,
        hover_btn: i32,    // which button is hovered (IDC_BTN_xxx or 0)
    }

    // ── helpers ─────────────────────────────────────────────────

    fn vk_to_key_name(vk: u16) -> Option<&'static str> {
        match VIRTUAL_KEY(vk) {
            VK_A => Some("A"), VK_B => Some("B"), VK_C => Some("C"), VK_D => Some("D"),
            VK_E => Some("E"), VK_F => Some("F"), VK_G => Some("G"), VK_H => Some("H"),
            VK_I => Some("I"), VK_J => Some("J"), VK_K => Some("K"), VK_L => Some("L"),
            VK_M => Some("M"), VK_N => Some("N"), VK_O => Some("O"), VK_P => Some("P"),
            VK_Q => Some("Q"), VK_R => Some("R"), VK_S => Some("S"), VK_T => Some("T"),
            VK_U => Some("U"), VK_V => Some("V"), VK_W => Some("W"), VK_X => Some("X"),
            VK_Y => Some("Y"), VK_Z => Some("Z"),
            VK_0 => Some("0"), VK_1 => Some("1"), VK_2 => Some("2"), VK_3 => Some("3"),
            VK_4 => Some("4"), VK_5 => Some("5"), VK_6 => Some("6"), VK_7 => Some("7"),
            VK_8 => Some("8"), VK_9 => Some("9"),
            VK_F1 => Some("F1"), VK_F2 => Some("F2"), VK_F3 => Some("F3"), VK_F4 => Some("F4"),
            VK_F5 => Some("F5"), VK_F6 => Some("F6"), VK_F7 => Some("F7"), VK_F8 => Some("F8"),
            VK_F9 => Some("F9"), VK_F10 => Some("F10"), VK_F11 => Some("F11"), VK_F12 => Some("F12"),
            VK_SPACE => Some("Space"), VK_RETURN => Some("Enter"), VK_TAB => Some("Tab"),
            VK_ESCAPE => Some("Escape"), VK_BACK => Some("Backspace"), VK_DELETE => Some("Delete"),
            VK_INSERT => Some("Insert"), VK_HOME => Some("Home"), VK_END => Some("End"),
            VK_PRIOR => Some("PageUp"), VK_NEXT => Some("PageDown"),
            VK_UP => Some("Up"), VK_DOWN => Some("Down"), VK_LEFT => Some("Left"), VK_RIGHT => Some("Right"),
            VK_OEM_1 => Some(";"), VK_OEM_PLUS => Some("="), VK_OEM_COMMA => Some(","),
            VK_OEM_MINUS => Some("-"), VK_OEM_PERIOD => Some("."), VK_OEM_2 => Some("/"),
            VK_OEM_3 => Some("`"), VK_OEM_4 => Some("["), VK_OEM_5 => Some("\\"),
            VK_OEM_6 => Some("]"), VK_OEM_7 => Some("'"),
            _ => None,
        }
    }

    fn modifiers_string_from_state() -> String {
        let mut parts = Vec::new();
        unsafe {
            if GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 { parts.push("Ctrl"); }
            if GetAsyncKeyState(VK_MENU.0 as i32) < 0 { parts.push("Alt"); }
            if GetAsyncKeyState(VK_SHIFT.0 as i32) < 0 { parts.push("Shift"); }
            if GetAsyncKeyState(VK_LWIN.0 as i32) < 0 || GetAsyncKeyState(VK_RWIN.0 as i32) < 0 {
                parts.push("Win");
            }
        }
        parts.join("+")
    }

    fn set_edit_text(hwnd: HWND, text: &str) {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr())); }
    }

    fn format_config(cfg: &HotkeyConfig) -> String {
        if cfg.modifiers.is_empty() {
            cfg.key.clone()
        } else {
            format!("{}+{}", cfg.modifiers, cfg.key)
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn create_font(height: i32, weight: i32, face: &str) -> HFONT {
        unsafe {
            CreateFontW(
                height, 0, 0, 0, weight,
                0, 0, 0, 0, 0, 0, 0, 0,
                PCWSTR(to_wide(face).as_ptr()),
            )
        }
    }

    fn rgb(bgr: u32) -> COLORREF { COLORREF(bgr) }

    fn fill_rect_color(hdc: HDC, rc: &RECT, color: u32) {
        unsafe {
            let brush = CreateSolidBrush(rgb(color));
            FillRect(hdc, rc, brush);
            let _ = DeleteObject(brush);
        }
    }

    fn fill_rounded_rect(hdc: HDC, rc: &RECT, radius: i32, color: u32) {
        unsafe {
            let brush = CreateSolidBrush(rgb(color));
            let pen = CreatePen(PS_NULL, 0, rgb(0));
            let old_brush = SelectObject(hdc, brush);
            let old_pen = SelectObject(hdc, pen);
            let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius * 2, radius * 2);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(brush);
            let _ = DeleteObject(pen);
        }
    }

    fn draw_rounded_border(hdc: HDC, rc: &RECT, radius: i32, color: u32, width: i32) {
        unsafe {
            let pen = CreatePen(PS_SOLID, width, rgb(color));
            let null_brush = GetStockObject(NULL_BRUSH);
            let old_pen = SelectObject(hdc, pen);
            let old_brush = SelectObject(hdc, null_brush);
            let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius * 2, radius * 2);
            SelectObject(hdc, old_pen);
            SelectObject(hdc, old_brush);
            let _ = DeleteObject(pen);
        }
    }

    fn draw_text_centered(hdc: HDC, rc: &RECT, text: &str, font: HFONT, color: u32) {
        unsafe {
            let old_font = SelectObject(hdc, font);
            let _ = SetTextColor(hdc, rgb(color));
            let _ = SetBkMode(hdc, TRANSPARENT);
            let wide = to_wide(text);
            let mut r = *rc;
            let _ = DrawTextW(hdc, &mut wide[..wide.len()-1].to_vec(), &mut r,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            SelectObject(hdc, old_font);
        }
    }

    fn draw_text_left(hdc: HDC, rc: &RECT, text: &str, font: HFONT, color: u32) {
        unsafe {
            let old_font = SelectObject(hdc, font);
            let _ = SetTextColor(hdc, rgb(color));
            let _ = SetBkMode(hdc, TRANSPARENT);
            let wide = to_wide(text);
            let mut r = *rc;
            let _ = DrawTextW(hdc, &mut wide[..wide.len()-1].to_vec(), &mut r,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            SelectObject(hdc, old_font);
        }
    }

    fn point_in_rect(x: i32, y: i32, rc: &RECT) -> bool {
        x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom
    }

    // ── Edit subclass for key capture ───────────────────────────

    unsafe extern "system" fn edit_subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _uid: usize,
        _data: usize,
    ) -> LRESULT {
        match msg {
            WM_SETFOCUS => {
                ACTIVE_FIELD.store(hwnd.0 as isize, Ordering::SeqCst);
                set_edit_text(hwnd, t("hk_dlg.recording"));
                // Trigger repaint of parent for border highlight
                let parent = GetParent(hwnd).unwrap_or(HWND(std::ptr::null_mut()));
                let _ = InvalidateRect(parent, None, true);
                return LRESULT(0);
            }
            WM_KILLFOCUS => {
                let active = ACTIVE_FIELD.load(Ordering::SeqCst);
                if active == hwnd.0 as isize {
                    ACTIVE_FIELD.store(0, Ordering::SeqCst);
                    DIALOG_STATE.with(|ds| {
                        let state = ds.borrow();
                        let hv = hwnd.0 as isize;
                        if hv == state.hwnd_edit_bind {
                            set_edit_text(hwnd, &format_config(&state.bind));
                        } else if hv == state.hwnd_edit_unbind_cursor {
                            set_edit_text(hwnd, &format_config(&state.unbind_cursor));
                        } else if hv == state.hwnd_edit_unbind_all {
                            set_edit_text(hwnd, &format_config(&state.unbind_all));
                        }
                    });
                }
                let parent = GetParent(hwnd).unwrap_or(HWND(std::ptr::null_mut()));
                let _ = InvalidateRect(parent, None, true);
                return LRESULT(0);
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let active = ACTIVE_FIELD.load(Ordering::SeqCst);
                if active == hwnd.0 as isize {
                    let vk = (wparam.0 & 0xFFFF) as u16;

                    if matches!(VIRTUAL_KEY(vk), VK_CONTROL | VK_SHIFT | VK_MENU | VK_LCONTROL
                        | VK_RCONTROL | VK_LSHIFT | VK_RSHIFT | VK_LMENU | VK_RMENU
                        | VK_LWIN | VK_RWIN) {
                        let mods = modifiers_string_from_state();
                        if !mods.is_empty() {
                            set_edit_text(hwnd, &format!("{}+...", mods));
                        }
                        return LRESULT(0);
                    }

                    if VIRTUAL_KEY(vk) == VK_ESCAPE {
                        ACTIVE_FIELD.store(0, Ordering::SeqCst);
                        DIALOG_STATE.with(|ds| {
                            let state = ds.borrow();
                            let hv = hwnd.0 as isize;
                            if hv == state.hwnd_edit_bind {
                                set_edit_text(hwnd, &format_config(&state.bind));
                            } else if hv == state.hwnd_edit_unbind_cursor {
                                set_edit_text(hwnd, &format_config(&state.unbind_cursor));
                            } else if hv == state.hwnd_edit_unbind_all {
                                set_edit_text(hwnd, &format_config(&state.unbind_all));
                            }
                        });
                        let parent = GetParent(hwnd).unwrap_or(HWND(std::ptr::null_mut()));
                        let _ = SetFocus(parent);
                        return LRESULT(0);
                    }

                    if let Some(key_name) = vk_to_key_name(vk) {
                        let mods = modifiers_string_from_state();
                        let cfg = HotkeyConfig::new(&mods, key_name);
                        let display = format_config(&cfg);
                        set_edit_text(hwnd, &display);

                        DIALOG_STATE.with(|ds| {
                            let mut state = ds.borrow_mut();
                            let hv = hwnd.0 as isize;
                            if hv == state.hwnd_edit_bind {
                                state.bind = cfg;
                            } else if hv == state.hwnd_edit_unbind_cursor {
                                state.unbind_cursor = cfg;
                            } else if hv == state.hwnd_edit_unbind_all {
                                state.unbind_all = cfg;
                            }
                        });

                        ACTIVE_FIELD.store(0, Ordering::SeqCst);
                        let parent = GetParent(hwnd).unwrap_or(HWND(std::ptr::null_mut()));
                        let _ = SetFocus(parent);
                    }
                    return LRESULT(0);
                }
            }
            WM_CHAR | WM_SYSCHAR => return LRESULT(0),
            // Hide the default edit painting — we paint ourselves
            WM_ERASEBKGND => return LRESULT(1),
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    // ── Button rect calculator ──────────────────────────────────

    fn get_btn_rects(scale: f32) -> [(i32, RECT); 3] {
        let s = |v: i32| -> i32 { (v as f32 * scale) as i32 };
        let body_top = s(HEADER_H);
        let btn_y = body_top + s(MARGIN) + (s(ROW_H) + s(ROW_GAP)) * 3 + s(20);
        let btn_w = s(BTN_W);
        let btn_h = s(BTN_H);
        let total = btn_w * 3 + s(16) * 2;
        let x0 = (s(WIN_W) - total) / 2;

        [
            (IDC_BTN_RESET, RECT { left: x0, top: btn_y, right: x0 + btn_w, bottom: btn_y + btn_h }),
            (IDC_BTN_CANCEL, RECT { left: x0 + btn_w + s(16), top: btn_y, right: x0 + btn_w * 2 + s(16), bottom: btn_y + btn_h }),
            (IDC_BTN_SAVE, RECT { left: x0 + (btn_w + s(16)) * 2, top: btn_y, right: x0 + btn_w * 3 + s(16) * 2, bottom: btn_y + btn_h }),
        ]
    }

    fn get_edit_rects(scale: f32) -> [(i32, RECT); 3] {
        let s = |v: i32| -> i32 { (v as f32 * scale) as i32 };
        let body_top = s(HEADER_H);
        let x_edit = s(MARGIN) + s(LABEL_W) + s(12);
        let edit_w = s(WIN_W) - x_edit - s(MARGIN);
        let mut y = body_top + s(MARGIN);
        let ids = [IDC_EDIT_BIND, IDC_EDIT_UNBIND_CURSOR, IDC_EDIT_UNBIND_ALL];
        let mut result = [(0, RECT::default()); 3];
        for (i, &id) in ids.iter().enumerate() {
            let edit_y = y + (s(ROW_H) - s(EDIT_H)) / 2;
            result[i] = (id, RECT { left: x_edit, top: edit_y, right: x_edit + edit_w, bottom: edit_y + s(EDIT_H) });
            y += s(ROW_H) + s(ROW_GAP);
        }
        result
    }

    // ── Main window proc ────────────────────────────────────────

    unsafe extern "system" fn dlg_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CREATE => {
                DIALOG_STATE.with(|ds| {
                    let mut state = ds.borrow_mut();
                    state.hwnd_main = hwnd.0 as isize;

                    let dpi = GetDpiForWindow(hwnd);
                    state.scale = dpi as f32 / 96.0;
                    let scale = state.scale;
                    let s = |v: i32| -> i32 { (v as f32 * scale) as i32 };

                    // Create hidden edit controls (we draw their borders ourselves)
                    let body_top = s(HEADER_H);
                    let x_edit = s(MARGIN) + s(LABEL_W) + s(12);
                    let edit_w = s(WIN_W) - x_edit - s(MARGIN);
                    let inset = s(4); // inset so our custom border surrounds the edit

                    let ids = [IDC_EDIT_BIND, IDC_EDIT_UNBIND_CURSOR, IDC_EDIT_UNBIND_ALL];
                    let values = [
                        format_config(&state.bind),
                        format_config(&state.unbind_cursor),
                        format_config(&state.unbind_all),
                    ];
                    let h_font = HFONT(state.h_font as *mut _);

                    let mut y = body_top + s(MARGIN);
                    for (i, &id) in ids.iter().enumerate() {
                        let edit_y = y + (s(ROW_H) - s(EDIT_H)) / 2;
                        let edit = CreateWindowExW(
                            WINDOW_EX_STYLE(0), // No WS_EX_CLIENTEDGE - we draw border ourselves
                            PCWSTR(to_wide("EDIT").as_ptr()),
                            PCWSTR(to_wide(&values[i]).as_ptr()),
                            WS_CHILD | WS_VISIBLE | WS_TABSTOP
                                | WINDOW_STYLE(ES_CENTER as u32 | ES_READONLY as u32),
                            x_edit + inset, edit_y + inset,
                            edit_w - inset * 2, s(EDIT_H) - inset * 2,
                            hwnd, HMENU(id as *mut _), HINSTANCE(std::ptr::null_mut()), None,
                        ).unwrap_or(HWND(std::ptr::null_mut()));
                        let _ = SendMessageW(edit, WM_SETFONT, WPARAM(h_font.0 as usize), LPARAM(1));
                        let _ = SetWindowSubclass(edit, Some(edit_subclass_proc), id as usize, 0);
                        y += s(ROW_H) + s(ROW_GAP);
                    }

                    let e1 = GetDlgItem(hwnd, IDC_EDIT_BIND);
                    let e2 = GetDlgItem(hwnd, IDC_EDIT_UNBIND_CURSOR);
                    let e3 = GetDlgItem(hwnd, IDC_EDIT_UNBIND_ALL);
                    state.hwnd_edit_bind = e1.map(|h| h.0 as isize).unwrap_or(0);
                    state.hwnd_edit_unbind_cursor = e2.map(|h| h.0 as isize).unwrap_or(0);
                    state.hwnd_edit_unbind_all = e3.map(|h| h.0 as isize).unwrap_or(0);
                });
                LRESULT(0)
            }

            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);

                DIALOG_STATE.with(|ds| {
                    let state = ds.borrow();
                    let scale = state.scale;
                    let s = |v: i32| -> i32 { (v as f32 * scale) as i32 };

                    let h_font = HFONT(state.h_font as *mut _);
                    let h_font_label = HFONT(state.h_font_label as *mut _);
                    let h_font_header = HFONT(state.h_font_header as *mut _);
                    let h_font_hint = HFONT(state.h_font_hint as *mut _);

                    let mut client = RECT::default();
                    let _ = GetClientRect(hwnd, &mut client);

                    // ── Header band ──
                    let header_rc = RECT { left: 0, top: 0, right: client.right, bottom: s(HEADER_H) };
                    fill_rect_color(hdc, &header_rc, CLR_HEADER_BG);

                    // Header title
                    let title_rc = RECT { left: s(MARGIN), top: s(8), right: client.right - s(MARGIN), bottom: s(HEADER_H) - s(8) };
                    // Title icon + text
                    let title_text = format!("\u{2328}  {}", t("hk_dlg.title")); // ⌨ keyboard icon
                    draw_text_left(hdc, &title_rc, &title_text, h_font_header, CLR_HEADER_TXT);

                    // ── Body background ──
                    let body_rc = RECT { left: 0, top: s(HEADER_H), right: client.right, bottom: client.bottom };
                    fill_rect_color(hdc, &body_rc, CLR_BG);

                    // ── Rows: labels + edit borders ──
                    let body_top = s(HEADER_H);
                    let labels = [
                        t("hk_dlg.bind_label"),
                        t("hk_dlg.unbind_cursor_label"),
                        t("hk_dlg.unbind_all_label"),
                    ];

                    let active_hwnd = ACTIVE_FIELD.load(Ordering::SeqCst);
                    let edit_hwnds = [state.hwnd_edit_bind, state.hwnd_edit_unbind_cursor, state.hwnd_edit_unbind_all];
                    let edit_rects = get_edit_rects(scale);

                    let mut y = body_top + s(MARGIN);
                    for (i, label) in labels.iter().enumerate() {
                        // Label with icon
                        let icon = match i {
                            0 => "\u{1F517}",  // 🔗 link
                            1 => "\u{2702}",    // ✂ scissors
                            _ => "\u{1F5D1}",   // 🗑 wastebasket
                        };
                        let label_text = format!("{}  {}", icon, label);
                        let label_rc = RECT {
                            left: s(MARGIN), top: y,
                            right: s(MARGIN) + s(LABEL_W), bottom: y + s(ROW_H),
                        };
                        draw_text_left(hdc, &label_rc, &label_text, h_font_label, CLR_LABEL);

                        // Custom edit border (rounded rect)
                        let (_, erc) = edit_rects[i];
                        let is_focused = active_hwnd == edit_hwnds[i] && active_hwnd != 0;
                        let border_color = if is_focused { CLR_EDIT_FOCUS } else { CLR_EDIT_BORDER };
                        let border_w = if is_focused { 2 } else { 1 };

                        fill_rounded_rect(hdc, &erc, s(CORNER_R), CLR_EDIT_BG);
                        draw_rounded_border(hdc, &erc, s(CORNER_R), border_color, border_w);

                        y += s(ROW_H) + s(ROW_GAP);
                    }

                    // ── Hint below header ──
                    // Draw right-aligned hint text at top of body
                    let hint_bottom_rc = RECT {
                        left: s(MARGIN),
                        top: y - s(ROW_GAP) + s(4),
                        right: client.right - s(MARGIN),
                        bottom: y - s(ROW_GAP) + s(20),
                    };
                    draw_text_left(hdc, &hint_bottom_rc, &format!("\u{1F4A1} {}", t("hk_dlg.hint")), h_font_hint, CLR_HINT);

                    // ── Separator line ──
                    let sep_y = y + s(8);
                    let sep_rc = RECT { left: s(MARGIN), top: sep_y, right: client.right - s(MARGIN), bottom: sep_y + 1 };
                    fill_rect_color(hdc, &sep_rc, CLR_SEPARATOR);

                    // ── Buttons ──
                    let btn_rects = get_btn_rects(scale);
                    let hover = state.hover_btn;

                    for &(id, ref rc) in &btn_rects {
                        let (bg, bg_hover, txt_color) = if id == IDC_BTN_SAVE {
                            (CLR_BTN_PRIMARY, CLR_BTN_PRI_HOV, CLR_BTN_PRI_TXT)
                        } else {
                            (CLR_BTN_SEC, CLR_BTN_SEC_HOV, CLR_BTN_SEC_TXT)
                        };
                        let bg_actual = if hover == id { bg_hover } else { bg };
                        fill_rounded_rect(hdc, rc, s(CORNER_R), bg_actual);

                        let label = match id {
                            IDC_BTN_RESET => t("hk_dlg.reset"),
                            IDC_BTN_CANCEL => t("hk_dlg.cancel"),
                            IDC_BTN_SAVE => t("hk_dlg.save"),
                            _ => "",
                        };
                        draw_text_centered(hdc, rc, label, h_font, txt_color);
                    }
                });

                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }

            // Make edit controls blend with our custom background
            WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => {
                let hdc = HDC(wparam.0 as *mut _);
                let child = HWND(lparam.0 as *mut _);
                let active_hwnd = ACTIVE_FIELD.load(Ordering::SeqCst);
                let is_recording = active_hwnd == child.0 as isize && active_hwnd != 0;

                let _ = SetBkColor(hdc, rgb(CLR_EDIT_BG));
                let _ = SetTextColor(hdc, rgb(if is_recording { CLR_EDIT_REC } else { CLR_EDIT_TEXT }));

                // Return a brush matching edit bg
                let brush = CreateSolidBrush(rgb(CLR_EDIT_BG));
                return LRESULT(brush.0 as isize);
            }

            WM_MOUSEMOVE => {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

                let new_hover = DIALOG_STATE.with(|ds| {
                    let state = ds.borrow();
                    let btn_rects = get_btn_rects(state.scale);
                    for &(id, ref rc) in &btn_rects {
                        if point_in_rect(x, y, rc) {
                            return id;
                        }
                    }
                    0
                });

                let old_hover = DIALOG_STATE.with(|ds| ds.borrow().hover_btn);
                if new_hover != old_hover {
                    DIALOG_STATE.with(|ds| ds.borrow_mut().hover_btn = new_hover);
                    let _ = unsafe { InvalidateRect(hwnd, None, false) };
                }

                // Track mouse leave
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                unsafe { let _ = TrackMouseEvent(&mut tme); }

                LRESULT(0)
            }

            WM_MOUSELEAVE_MSG => {
                DIALOG_STATE.with(|ds| ds.borrow_mut().hover_btn = 0);
                let _ = unsafe { InvalidateRect(hwnd, None, false) };
                LRESULT(0)
            }

            WM_LBUTTONDOWN => {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

                let clicked = DIALOG_STATE.with(|ds| {
                    let state = ds.borrow();
                    let btn_rects = get_btn_rects(state.scale);
                    for &(id, ref rc) in &btn_rects {
                        if point_in_rect(x, y, rc) {
                            return id;
                        }
                    }
                    0
                });

                match clicked {
                    IDC_BTN_SAVE => {
                        DIALOG_STATE.with(|ds| ds.borrow_mut().saved = true);
                        let _ = unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
                    }
                    IDC_BTN_CANCEL => {
                        let _ = unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
                    }
                    IDC_BTN_RESET => {
                        let defaults = crate::settings::Settings::default();
                        DIALOG_STATE.with(|ds| {
                            let mut state = ds.borrow_mut();
                            state.bind = defaults.hotkey_bind.clone();
                            state.unbind_cursor = defaults.hotkey_unbind_cursor.clone();
                            state.unbind_all = defaults.hotkey_unbind_all.clone();

                            let e1 = HWND(state.hwnd_edit_bind as *mut _);
                            let e2 = HWND(state.hwnd_edit_unbind_cursor as *mut _);
                            let e3 = HWND(state.hwnd_edit_unbind_all as *mut _);
                            set_edit_text(e1, &format_config(&state.bind));
                            set_edit_text(e2, &format_config(&state.unbind_cursor));
                            set_edit_text(e3, &format_config(&state.unbind_all));
                        });
                        let _ = unsafe { InvalidateRect(hwnd, None, true) };
                    }
                    _ => {}
                }
                LRESULT(0)
            }

            WM_SETCURSOR => {
                // Change cursor to hand over buttons
                let hover = DIALOG_STATE.with(|ds| ds.borrow().hover_btn);
                if hover != 0 {
                    unsafe { let _ = SetCursor(LoadCursorW(None, IDC_HAND).unwrap_or_default()); }
                    return LRESULT(1);
                }
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }

            WM_CLOSE => {
                unsafe { let _ = DestroyWindow(hwnd); }
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0); }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    pub fn show_hotkey_dialog(
        bind: &HotkeyConfig,
        unbind_cursor: &HotkeyConfig,
        unbind_all: &HotkeyConfig,
    ) -> Option<HotkeyDialogResult> {
        // Create fonts
        let h_font = create_font(-15, 400, "Segoe UI");
        let h_font_label = create_font(-14, 600, "Segoe UI Semibold");
        let h_font_header = create_font(-20, 700, "Segoe UI");
        let h_font_hint = create_font(-12, 400, "Segoe UI");

        DIALOG_STATE.with(|ds| {
            let mut state = ds.borrow_mut();
            state.bind = bind.clone();
            state.unbind_cursor = unbind_cursor.clone();
            state.unbind_all = unbind_all.clone();
            state.saved = false;
            state.hwnd_main = 0;
            state.hwnd_edit_bind = 0;
            state.hwnd_edit_unbind_cursor = 0;
            state.hwnd_edit_unbind_all = 0;
            state.h_font = h_font.0 as isize;
            state.h_font_label = h_font_label.0 as isize;
            state.h_font_header = h_font_header.0 as isize;
            state.h_font_hint = h_font_hint.0 as isize;
            state.hover_btn = 0;
            state.scale = 1.0; // will be updated in WM_CREATE
        });
        ACTIVE_FIELD.store(0, Ordering::SeqCst);

        unsafe {
            let class_name = to_wide("ABW_HotkeyDialog");
            let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), HINSTANCE(std::ptr::null_mut()));

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(dlg_wnd_proc),
                hInstance: HINSTANCE(std::ptr::null_mut()),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()), // We paint everything ourselves
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            RegisterClassExW(&wc);

            let title = to_wide(t("hk_dlg.title"));

            let scr_w = GetSystemMetrics(SM_CXSCREEN);
            let scr_h = GetSystemMetrics(SM_CYSCREEN);
            let x = (scr_w - WIN_W) / 2;
            let y = (scr_h - WIN_H) / 2;

            let hwnd = CreateWindowExW(
                WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                x, y, WIN_W, WIN_H,
                HWND(std::ptr::null_mut()),
                HMENU(std::ptr::null_mut()),
                HINSTANCE(std::ptr::null_mut()),
                None,
            ).unwrap_or(HWND(std::ptr::null_mut()));

            if hwnd.0.is_null() {
                return None;
            }

            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);
            let _ = SetForegroundWindow(hwnd);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if !IsDialogMessageW(hwnd, &msg).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            let _ = DeleteObject(h_font);
            let _ = DeleteObject(h_font_label);
            let _ = DeleteObject(h_font_header);
            let _ = DeleteObject(h_font_hint);

            DIALOG_STATE.with(|ds| {
                let state = ds.borrow();
                if state.saved {
                    Some(HotkeyDialogResult {
                        bind: state.bind.clone(),
                        unbind_cursor: state.unbind_cursor.clone(),
                        unbind_all: state.unbind_all.clone(),
                    })
                } else {
                    None
                }
            })
        }
    }
}
