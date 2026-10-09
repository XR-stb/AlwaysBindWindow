//! Native multi-select picker. Selection is by window handle, never by process.
use crate::native_ui as ui;
use crate::{group::TrackedWindow, i18n::t, overlay::PickerResult};
use std::cell::RefCell;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{SetWindowTheme, DRAWITEMSTRUCT, ODT_BUTTON, ODT_LISTBOX};
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

const ID_LIST: usize = 100;
const ID_REFRESH: usize = 101;
const ID_MOVEMENT: usize = 102;
const ID_BIND: usize = 1;
const ID_CANCEL: usize = 2;

#[derive(Default)]
struct State {
    windows: Vec<TrackedWindow>,
    result: Option<Vec<TrackedWindow>>,
    result_movement: crate::group::MovementMode,
    done: bool,
    list: HWND,
    heading: HWND,
    hint: HWND,
    label: HWND,
    status: HWND,
    bind: HWND,
    refresh: HWND,
    cancel: HWND,
    movement_label: HWND,
    movement: HWND,
    dpi: i32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn same_window(a: &TrackedWindow, b: &TrackedWindow) -> bool {
    a.hwnd == b.hwnd && a.process_name == b.process_name && a.class_name == b.class_name
}

unsafe fn selected(state: &State) -> Vec<TrackedWindow> {
    state
        .windows
        .iter()
        .enumerate()
        .filter(|(i, _)| SendMessageW(state.list, LB_GETSEL, WPARAM(*i), LPARAM(0)).0 > 0)
        .map(|(_, window)| window.clone())
        .collect()
}

unsafe fn update_status(state: &State, notice: Option<&str>) {
    let count = selected(state).len();
    let text = notice.map(str::to_owned).unwrap_or_else(|| {
        if state.windows.is_empty() {
            t("picker.empty").to_string()
        } else {
            format!("{}{} — {}", t("picker.selected"), count, t("msg.need2"))
        }
    });
    let _ = SetWindowTextW(state.status, PCWSTR(wide(&text).as_ptr()));
    let _ = EnableWindow(state.bind, count >= 2);
}

unsafe fn refresh(state: &mut State, notice: Option<&str>) -> windows::core::Result<()> {
    let previous = selected(state);
    let available = super::get_pickable_windows();
    SendMessageW(state.list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
    state.windows.clear();
    let mut failed = false;
    for window in available {
        // Keep the full title in the native list for accessibility; paint two lines.
        let text = format!(
            "{}\t{}",
            window.title.replace('\t', " "),
            window.process_name
        );
        let text = wide(&text);
        let row = SendMessageW(
            state.list,
            LB_ADDSTRING,
            WPARAM(0),
            LPARAM(text.as_ptr() as isize),
        )
        .0;
        if row < 0 {
            failed = true;
            break;
        }
        if previous.iter().any(|old| same_window(old, &window)) {
            SendMessageW(state.list, LB_SETSEL, WPARAM(1), LPARAM(row));
        }
        state.windows.push(window);
    }
    update_status(state, notice);
    if failed {
        Err(windows::core::Error::from_win32())
    } else {
        Ok(())
    }
}

unsafe fn layout(hwnd: HWND, state: &State) {
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let scale = |n| n * state.dpi / 96;
    let margin = scale(24);
    let width = (rect.right - 2 * margin).max(1);
    let footer = rect.bottom - scale(64);
    for (control, x, y, w, h) in [
        (state.heading, margin, scale(20), width, scale(36)),
        (state.hint, margin, scale(62), width, scale(44)),
        (state.label, margin, scale(110), width, scale(22)),
        (
            state.list,
            margin,
            scale(142),
            width,
            (footer - scale(246)).max(1),
        ),
        (
            state.movement_label,
            margin,
            footer - scale(96),
            scale(150),
            scale(30),
        ),
        (
            state.movement,
            margin + scale(160),
            footer - scale(100),
            (width - scale(160)).max(1),
            scale(140),
        ),
        (state.status, margin, footer - scale(50), width, scale(44)),
        (state.refresh, margin, footer, scale(100), scale(38)),
        (
            state.bind,
            rect.right - margin - scale(272),
            footer,
            scale(160),
            scale(38),
        ),
        (
            state.cancel,
            rect.right - margin - scale(100),
            footer,
            scale(100),
            scale(38),
        ),
    ] {
        if !control.0.is_null() {
            let _ = MoveWindow(control, x, y, w, h, true);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            ui::fill(dc, &rect, ui::BG);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC => {
            let dc = HDC(wp.0 as *mut _);
            let _ = SetTextColor(dc, COLORREF(ui::MUTED));
            let _ = SetBkColor(dc, COLORREF(ui::BG));
            let _ = SetDCBrushColor(dc, COLORREF(ui::BG));
            LRESULT(GetStockObject(DC_BRUSH).0 as isize)
        }
        WM_CTLCOLORLISTBOX => {
            let dc = HDC(wp.0 as *mut _);
            let _ = SetDCBrushColor(dc, COLORREF(ui::SURFACE));
            LRESULT(GetStockObject(DC_BRUSH).0 as isize)
        }
        WM_DRAWITEM => {
            let item = &*(lp.0 as *const DRAWITEMSTRUCT);
            if item.CtlType == ODT_BUTTON {
                ui::draw_button(item, item.CtlID == ID_BIND as u32);
            } else if item.CtlType == ODT_LISTBOX {
                ui::draw_window_row(item);
            }
            LRESULT(1)
        }
        WM_COMMAND => {
            let id = wp.0 & 0xffff;
            let notification = (wp.0 >> 16) as u32;
            // Native controls send focus notifications synchronously from SetFocus.
            // Ignore them before borrowing dialog state to avoid reentrant borrows.
            if (id == ID_LIST && notification != LBN_SELCHANGE)
                || (id != ID_LIST && notification != BN_CLICKED)
            {
                return LRESULT(0);
            }
            STATE.with(|cell| {
                let mut state = cell.borrow_mut();
                match id {
                    ID_LIST if notification == LBN_SELCHANGE => update_status(&state, None),
                    ID_REFRESH => {
                        if refresh(&mut state, None).is_err() {
                            update_status(&state, Some(t("picker.error")));
                        }
                        let _ = SetFocus(state.list);
                    }
                    ID_BIND => {
                        let chosen = selected(&state);
                        if chosen.len() < 2 {
                            return;
                        }
                        let available = super::get_pickable_windows();
                        if chosen
                            .iter()
                            .any(|old| !available.iter().any(|new| same_window(old, new)))
                        {
                            if refresh(&mut state, Some(t("picker.stale"))).is_err() {
                                update_status(&state, Some(t("picker.error")));
                            }
                            let _ = SetFocus(state.list);
                        } else {
                            state.result_movement = ui::movement_choice(state.movement);
                            state.result = Some(chosen);
                            state.done = true;
                        }
                    }
                    ID_CANCEL => state.done = true,
                    _ => {}
                }
            });
            LRESULT(0)
        }
        WM_SIZE => {
            STATE.with(|cell| layout(hwnd, &cell.borrow()));
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lp.0 as *mut MINMAXINFO);
            let dpi = GetDpiForSystem() as i32;
            info.ptMinTrackSize = POINT {
                x: 520 * dpi / 96,
                y: 440 * dpi / 96,
            };
            LRESULT(0)
        }
        WM_CLOSE => {
            STATE.with(|cell| cell.borrow_mut().done = true);
            LRESULT(0)
        }
        // Never post WM_QUIT: this is a nested pump inside the tray event loop.
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn control(
    parent: HWND,
    class: PCWSTR,
    text: &str,
    style: WINDOW_STYLE,
    id: usize,
    font: HFONT,
) -> windows::core::Result<HWND> {
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        class,
        PCWSTR(wide(text).as_ptr()),
        WS_CHILD | WS_VISIBLE | style,
        0,
        0,
        1,
        1,
        parent,
        HMENU(id as *mut _),
        None,
        None,
    )?;
    SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    Ok(hwnd)
}

pub fn show_window_picker(global_sync_move: bool) -> PickerResult {
    let cancelled = || PickerResult {
        selected_windows: Vec::new(),
        cancelled: true,
        movement: Default::default(),
    };
    unsafe {
        // Match the existing native settings dialog's system-DPI coordinate space.
        let _dpi_scope = ui::DpiScope::new();
        let dpi = GetDpiForSystem() as i32;
        STATE.with(|cell| {
            *cell.borrow_mut() = State {
                dpi,
                ..State::default()
            }
        });
        let class = w!("ABW_WindowPicker");
        let instance = HINSTANCE(GetModuleHandleW(None).unwrap_or_default().0);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_CLIPCHILDREN;
        let bounds = ui::centered_rect(760, 560, dpi as u32, style, WS_EX_DLGMODALFRAME);
        let window = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class,
            PCWSTR(wide(t("picker.title")).as_ptr()),
            style,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            None,
            None,
            instance,
            None,
        );
        let Ok(hwnd) = window else {
            let _ = MessageBoxW(
                None,
                PCWSTR(wide(t("picker.error")).as_ptr()),
                w!("AlwaysBindWindow"),
                MB_OK | MB_ICONERROR,
            );
            return cancelled();
        };
        let font = ui::font(15, 400, dpi);
        let heading_font = ui::font(24, 600, dpi);
        let setup: windows::core::Result<()> = STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.heading = control(
                hwnd,
                w!("STATIC"),
                t("picker.title"),
                WINDOW_STYLE(0),
                0,
                heading_font,
            )?;
            state.hint = control(
                hwnd,
                w!("STATIC"),
                t("picker.hint"),
                WINDOW_STYLE(0),
                0,
                font,
            )?;
            state.label = control(
                hwnd,
                w!("STATIC"),
                t("picker.list"),
                WINDOW_STYLE(0),
                0,
                font,
            )?;
            state.list = control(
                hwnd,
                w!("LISTBOX"),
                "",
                WS_TABSTOP
                    | WS_VSCROLL
                    | WINDOW_STYLE(
                        (LBS_MULTIPLESEL
                            | LBS_NOTIFY
                            | LBS_NOINTEGRALHEIGHT
                            | LBS_OWNERDRAWFIXED
                            | LBS_HASSTRINGS) as u32,
                    ),
                ID_LIST,
                font,
            )?;
            SendMessageW(
                state.list,
                LB_SETITEMHEIGHT,
                WPARAM(0),
                LPARAM((64 * dpi / 96) as isize),
            );
            let _ = SetWindowTheme(state.list, w!("Explorer"), PCWSTR::null());
            state.movement_label = control(
                hwnd,
                w!("STATIC"),
                t("movement.label"),
                WINDOW_STYLE(0),
                0,
                font,
            )?;
            state.movement = control(
                hwnd,
                w!("COMBOBOX"),
                "",
                WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
                ID_MOVEMENT,
                font,
            )?;
            ui::init_movement_choices(state.movement, global_sync_move);
            state.status = control(hwnd, w!("STATIC"), "", WINDOW_STYLE(0), 0, font)?;
            state.refresh = control(
                hwnd,
                w!("BUTTON"),
                t("picker.refresh"),
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                ID_REFRESH,
                font,
            )?;
            state.bind = control(
                hwnd,
                w!("BUTTON"),
                t("picker.bind"),
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                ID_BIND,
                font,
            )?;
            state.cancel = control(
                hwnd,
                w!("BUTTON"),
                t("picker.cancel"),
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                ID_CANCEL,
                font,
            )?;
            for button in [state.refresh, state.bind, state.cancel] {
                ui::style_button(button);
            }
            refresh(&mut state, None)?;
            layout(hwnd, &state);
            Ok(())
        });
        if let Err(err) = setup {
            log::error!("Window picker setup failed: {}", err);
            let _ = MessageBoxW(
                hwnd,
                PCWSTR(wide(t("picker.error")).as_ptr()),
                w!("AlwaysBindWindow"),
                MB_OK | MB_ICONERROR,
            );
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            STATE.with(|cell| {
                let _ = SetFocus(cell.borrow().list);
            });
            let mut msg = MSG::default();
            while !STATE.with(|cell| cell.borrow().done) {
                let status = GetMessageW(&mut msg, None, 0, 0).0;
                if status <= 0 {
                    if status == 0 {
                        PostQuitMessage(msg.wParam.0 as i32);
                    }
                    break;
                }
                if !IsDialogMessageW(hwnd, &msg).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        let _ = DestroyWindow(hwnd);
        let _ = DeleteObject(font);
        let _ = DeleteObject(heading_font);
        STATE.with(|cell| {
            let movement = cell.borrow().result_movement;
            let result = cell.borrow_mut().result.take();
            *cell.borrow_mut() = State::default();
            result
                .map(|selected_windows| PickerResult {
                    selected_windows,
                    cancelled: false,
                    movement,
                })
                .unwrap_or_else(cancelled)
        })
    }
}
