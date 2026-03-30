use crate::group::GroupManager;
use crate::overlay;
use crate::i18n::{self, t};
use crate::settings::{self, Settings};
use log::{info, error};
use std::sync::{Arc, Mutex};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, CheckMenuItem},
    TrayIconBuilder,
};
use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::*;

const ID_LASSO: &str = "lasso";
const ID_UNBIND_CURSOR: &str = "unbind_cursor";
const ID_UNBIND_ALL: &str = "unbind_all";
const ID_TOGGLE_LANG: &str = "toggle_lang";
const ID_TOGGLE_AUTOSTART: &str = "toggle_autostart";
const ID_QUIT: &str = "quit";

fn create_icon() -> tray_icon::Icon {
    let size = 16u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let d1 = ((x as f32 - 6.0).powi(2) + (y as f32 - 6.0).powi(2)).sqrt();
            let d2 = ((x as f32 - 10.0).powi(2) + (y as f32 - 10.0).powi(2)).sqrt();
            if (d1 > 3.0 && d1 < 5.5) || (d2 > 3.0 && d2 < 5.5) {
                rgba.extend_from_slice(&[0x00, 0xBC, 0xD4, 0xFF]);
            } else {
                rgba.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("icon")
}

fn do_lasso_bind(gm: &Arc<Mutex<GroupManager>>) {
    info!("Lasso bind triggered");
    let result = overlay::run_picker_overlay();
    while GlobalHotKeyEvent::receiver().try_recv().is_ok() {}

    if result.cancelled || result.selected_windows.is_empty() {
        info!("Lasso cancelled");
        return;
    }

    let mut unique: Vec<String> = Vec::new();
    for w in &result.selected_windows {
        if !unique.contains(&w.process_name) {
            unique.push(w.process_name.clone());
        }
    }
    let name = unique.join(" + ");
    let selected: Vec<(isize, String, String)> = result.selected_windows
        .iter().map(|w| (w.hwnd, w.process_name.clone(), w.title.clone())).collect();
    let count = selected.len();

    let mut mgr = gm.lock().unwrap();
    mgr.create_group_from_hwnds(&name, selected);
    println!("{} {} {}: {}", t("msg.bound"), count, t("msg.windows"), name);
    info!("Created group '{}' with {} windows", name, count);
}

fn do_unbind_at_cursor(gm: &Arc<Mutex<GroupManager>>) {
    #[cfg(target_os = "windows")]
    {
        let hwnd_under_cursor = unsafe {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let h = WindowFromPoint(pt);
            if !h.0.is_null() {
                let a = GetAncestor(h, GA_ROOT);
                if !a.0.is_null() { a } else { h }
            } else { h }
        };
        if hwnd_under_cursor.0.is_null() {
            println!("{}", t("msg.no_group"));
            return;
        }
        let hv = hwnd_under_cursor.0 as isize;
        let mut mgr = gm.lock().unwrap();
        if let Some(group_id) = mgr.find_group_for_hwnd(hv).map(|s| s.to_string()) {
            let group_name = mgr.groups.iter()
                .find(|g| g.id == group_id).map(|g| g.name.clone()).unwrap_or_default();
            mgr.remove_group(&group_id);
            println!("{} '{}'", t("msg.unbound_group"), group_name);
        } else {
            println!("{}", t("msg.no_group"));
        }
    }
}

fn do_unbind_all(gm: &Arc<Mutex<GroupManager>>) {
    let mut mgr = gm.lock().unwrap();
    let count = mgr.groups.len();
    mgr.groups.clear();
    mgr.active_bindings.clear();
    println!("{} ({} groups)", t("msg.unbound_all"), count);
}

struct App {
    gm: Arc<Mutex<GroupManager>>,
    settings: Settings,
    _tray: Option<tray_icon::TrayIcon>,
    _hk_mgr: Option<GlobalHotKeyManager>,
    hk_bind_id: Option<u32>,
    hk_unbind_cursor_id: Option<u32>,
    hk_unbind_all_id: Option<u32>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _: &ActiveEventLoop) {}
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
            if Some(ev.id) == self.hk_bind_id {
                do_lasso_bind(&self.gm);
            } else if Some(ev.id) == self.hk_unbind_cursor_id {
                do_unbind_at_cursor(&self.gm);
            } else if Some(ev.id) == self.hk_unbind_all_id {
                do_unbind_all(&self.gm);
            }
        }
        if let Ok(ev) = MenuEvent::receiver().try_recv() {
            match ev.id().0.as_str() {
                ID_LASSO => do_lasso_bind(&self.gm),
                ID_UNBIND_CURSOR => do_unbind_at_cursor(&self.gm),
                ID_UNBIND_ALL => do_unbind_all(&self.gm),
                ID_TOGGLE_LANG => {
                    let new_lang = if i18n::get_lang() == i18n::Lang::Zh {
                        i18n::Lang::En
                    } else {
                        i18n::Lang::Zh
                    };
                    i18n::set_lang(new_lang);
                    self.settings.lang = match new_lang {
                        i18n::Lang::Zh => "zh",
                        i18n::Lang::En => "en",
                    }.to_string();
                    let _ = settings::save(&self.settings);
                    println!("Language: {}", self.settings.lang);
                }
                ID_TOGGLE_AUTOSTART => {
                    self.settings.auto_start = !self.settings.auto_start;
                    if let Err(e) = settings::set_auto_start(self.settings.auto_start) {
                        error!("Auto-start error: {}", e);
                    }
                    let _ = settings::save(&self.settings);
                    println!("Auto-start: {}", if self.settings.auto_start { "ON" } else { "OFF" });
                }
                ID_QUIT => std::process::exit(0),
                _ => {}
            }
        }
    }
}

pub fn run_tray(gm: Arc<Mutex<GroupManager>>, settings: Settings) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let bind_str = settings::format_hotkey(&settings.hotkey_bind);
    let unbind_c_str = settings::format_hotkey(&settings.hotkey_unbind_cursor);
    let unbind_a_str = settings::format_hotkey(&settings.hotkey_unbind_all);

    let menu = Menu::new();
    let _ = menu.append(&MenuItem::with_id("title", t("app.name"), false, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(ID_LASSO,
        &format!("{}  ({})", t("menu.bind"), bind_str), true, None));
    let _ = menu.append(&MenuItem::with_id(ID_UNBIND_CURSOR,
        &format!("{}  ({})", t("menu.unbind_cursor"), unbind_c_str), true, None));
    let _ = menu.append(&MenuItem::with_id(ID_UNBIND_ALL,
        &format!("{}  ({})", t("menu.unbind_all"), unbind_a_str), true, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(ID_TOGGLE_LANG, t("menu.lang"), true, None));
    let autostart_check = CheckMenuItem::with_id(ID_TOGGLE_AUTOSTART,
        if i18n::get_lang() == i18n::Lang::Zh { "开机自启" } else { "Auto Start" },
        true, settings.auto_start, None);
    let _ = menu.append(&autostart_check);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(ID_QUIT, t("menu.quit"), true, None));

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(&format!("{}\n{} = {} | {} = {} | {} = {}",
            t("app.name"), bind_str, t("hk.bind"),
            unbind_c_str, t("hk.unbind_cursor"),
            unbind_a_str, t("hk.unbind_all")))
        .with_icon(create_icon())
        .build()?;

    // Register hotkeys
    let hk_mgr = GlobalHotKeyManager::new()?;
    let hk_bind = settings::build_hotkey(&settings.hotkey_bind);
    let hk_uc = settings::build_hotkey(&settings.hotkey_unbind_cursor);
    let hk_ua = settings::build_hotkey(&settings.hotkey_unbind_all);

    let mut hk_bind_id = None;
    let mut hk_uc_id = None;
    let mut hk_ua_id = None;

    if let Some(hk) = &hk_bind { hk_mgr.register(*hk)?; hk_bind_id = Some(hk.id()); }
    if let Some(hk) = &hk_uc { hk_mgr.register(*hk)?; hk_uc_id = Some(hk.id()); }
    if let Some(hk) = &hk_ua { hk_mgr.register(*hk)?; hk_ua_id = Some(hk.id()); }

    println!("{}", t("app.ready"));
    info!("Hotkeys registered");

    // Apply auto-start setting
    if settings.auto_start {
        let _ = settings::set_auto_start(true);
    }

    let mut app = App {
        gm,
        settings,
        _tray: Some(tray),
        _hk_mgr: Some(hk_mgr),
        hk_bind_id,
        hk_unbind_cursor_id: hk_uc_id,
        hk_unbind_all_id: hk_ua_id,
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}
