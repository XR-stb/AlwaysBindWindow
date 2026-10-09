use crate::group::{GroupManager, MovementMode};
use crate::overlay;
use crate::i18n::{self, t};
use crate::settings::{self, Settings};
use crate::hotkey_dialog;
use log::{info, error};
use std::sync::{mpsc, Arc, Mutex};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, CheckMenuItem, Submenu},
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
const ID_PICK_WINDOWS: &str = "pick_windows";
const ID_SYNC_MOVE: &str = "sync_move";
const ID_RECOVER: &str = "recover_windows";
const ID_UNBIND_CURSOR: &str = "unbind_cursor";
const ID_UNBIND_ALL: &str = "unbind_all";
const ID_HOTKEY_SETTINGS: &str = "hotkey_settings";
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

/// Build the tray menu with current language and settings
fn build_menu(settings: &Settings, gm: &GroupManager, registered: [bool; 4]) -> Menu {
    let display = |config: &settings::HotkeyConfig, active: bool| {
        let label = settings::format_hotkey(config);
        if active { label } else { format!("{} · {}", label, t("hk.unavailable")) }
    };
    let bind_str = display(&settings.hotkey_bind, registered[0]);
    let unbind_c_str = display(&settings.hotkey_unbind_cursor, registered[1]);
    let unbind_a_str = display(&settings.hotkey_unbind_all, registered[2]);

    let menu = Menu::new();
    let _ = menu.append(&MenuItem::with_id("title", t("app.name"), false, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(ID_LASSO,
        &format!("{}  ({})", t("menu.bind"), bind_str), true, None));
    let _ = menu.append(&MenuItem::with_id(ID_PICK_WINDOWS, t("menu.pick_windows"), cfg!(target_os = "windows"), None));
    let _ = menu.append(&MenuItem::with_id(ID_UNBIND_CURSOR,
        &format!("{}  ({})", t("menu.unbind_cursor"), unbind_c_str), true, None));
    let _ = menu.append(&MenuItem::with_id(ID_UNBIND_ALL,
        &format!("{}  ({})", t("menu.unbind_all"), unbind_a_str), true, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&CheckMenuItem::with_id(ID_SYNC_MOVE,
        &format!("{}  ({})", t("menu.sync_move"), display(&settings.hotkey_sync_move, registered[3])),
        true, settings.sync_move, None));
    let groups_menu = Submenu::new(t("menu.groups"), true);
    let mut count = 0;
    for group in &gm.groups {
        let members = gm.active_bindings.values().filter(|id| **id == group.id).count();
        if members < 2 { continue; }
        count += 1;
        let name = group.name.chars().take(64).collect::<String>().replace('&', "&&");
        let status = if gm.movement_enabled(&group.id) { t("movement.on") } else { t("movement.off") };
        let submenu = Submenu::new(format!("{} · {} · {}", name, members, status), true);
        for (index, mode) in MovementMode::ALL.iter().enumerate() {
            let _ = submenu.append(&CheckMenuItem::with_id(
                format!("group_move:{}:{}", group.id, index), mode.label(), true, group.movement == *mode, None));
        }
        let _ = groups_menu.append(&submenu);
    }
    if count == 0 {
        let _ = groups_menu.append(&MenuItem::new(t("menu.no_groups"), false, None));
    }
    let _ = menu.append(&groups_menu);
    let _ = menu.append(&MenuItem::with_id(ID_RECOVER, t("menu.recover"), cfg!(target_os = "windows"), None));
    let settings_label = if registered.iter().all(|active| *active) {
        t("menu.hotkey_settings")
    } else {
        t("menu.hotkey_settings_conflict")
    };
    let _ = menu.append(&MenuItem::with_id(ID_HOTKEY_SETTINGS, settings_label, true, None));
    let _ = menu.append(&MenuItem::with_id(ID_TOGGLE_LANG, t("menu.lang"), true, None));
    let autostart_label = if i18n::get_lang() == i18n::Lang::Zh { "开机自启" } else { "Auto Start" };
    let autostart_check = CheckMenuItem::with_id(ID_TOGGLE_AUTOSTART,
        autostart_label, true, settings.auto_start, None);
    let _ = menu.append(&autostart_check);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(ID_QUIT, t("menu.quit"), true, None));
    menu
}

/// Build tooltip string with current language
fn build_tooltip(settings: &Settings) -> String {
    let bind_str = settings::format_hotkey(&settings.hotkey_bind);
    let unbind_c_str = settings::format_hotkey(&settings.hotkey_unbind_cursor);
    let unbind_a_str = settings::format_hotkey(&settings.hotkey_unbind_all);
    format!("{}\n{} = {} | {} = {} | {} = {}",
        t("app.name"), bind_str, t("hk.bind"),
        unbind_c_str, t("hk.unbind_cursor"),
        unbind_a_str, t("hk.unbind_all"))
}

fn do_lasso_bind(gm: &Arc<Mutex<GroupManager>>, settings: &Settings) {
    info!("Lasso bind triggered");
    let result = overlay::run_picker_overlay(settings.sync_move);
    bind_picked_windows(gm, settings, result);
}

fn bind_picked_windows(gm: &Arc<Mutex<GroupManager>>, settings: &Settings, result: overlay::PickerResult) {
    if result.cancelled || result.selected_windows.len() < 2 {
        info!("Window selection cancelled");
        return;
    }

    let name = result.selected_windows.iter().take(3).map(|window| {
        let title = if window.title.is_empty() { &window.process_name } else { &window.title };
        title.chars().take(24).collect::<String>()
    }).collect::<Vec<_>>().join(" + ");
    let selected: Vec<(isize, String, String)> = result.selected_windows
        .iter().map(|w| (w.hwnd, w.process_name.clone(), w.title.clone())).collect();
    let count = selected.len();

    let mut mgr = gm.lock().unwrap();
    mgr.create_group_from_hwnds(&name, selected, result.movement, settings.sync_minimize);
    info!("Bound {} windows: {}", count, name);
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
            return;
        }
        let hv = hwnd_under_cursor.0 as isize;
        let mut mgr = gm.lock().unwrap();
        if let Some(group_id) = mgr.find_group_for_hwnd(hv).map(|s| s.to_string()) {
            let group_name = mgr.groups.iter()
                .find(|g| g.id == group_id).map(|g| g.name.clone()).unwrap_or_default();
            let mut windows = mgr.get_sibling_hwnds(hv);
            windows.push(hv);
            mgr.remove_group(&group_id);
            drop(mgr);
            crate::platform::recover_windows(&windows);
            info!("Unbound group '{}'", group_name);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        info!("Unbind-at-cursor is only supported on Windows for now.");
    }
}

fn do_unbind_all(gm: &Arc<Mutex<GroupManager>>) {
    let mut mgr = gm.lock().unwrap();
    let count = mgr.groups.len();
    let windows: Vec<isize> = mgr.active_bindings.keys().copied().collect();
    mgr.groups.clear();
    mgr.active_bindings.clear();
    drop(mgr);
    crate::platform::recover_windows(&windows);
    info!("Unbound all ({} groups)", count);
}

struct App {
    hotkey_events: mpsc::Receiver<GlobalHotKeyEvent>,
    menu_events: mpsc::Receiver<MenuEvent>,
    gm: Arc<Mutex<GroupManager>>,
    settings: Settings,
    tray: Option<tray_icon::TrayIcon>,
    hk_mgr: Option<GlobalHotKeyManager>,
    hk_bind_id: Option<u32>,
    hk_unbind_cursor_id: Option<u32>,
    hk_unbind_all_id: Option<u32>,
    hk_sync_move_id: Option<u32>,
    hk_bind: Option<global_hotkey::hotkey::HotKey>,
    hk_unbind_cursor: Option<global_hotkey::hotkey::HotKey>,
    hk_unbind_all: Option<global_hotkey::hotkey::HotKey>,
    hk_sync_move: Option<global_hotkey::hotkey::HotKey>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _: &ActiveEventLoop) { info!("Tray event loop resumed"); }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        while let Ok(ev) = self.hotkey_events.try_recv() {
            if ev.state == global_hotkey::HotKeyState::Pressed && Some(ev.id) == self.hk_bind_id {
                self.lasso_bind();
            } else if ev.state == global_hotkey::HotKeyState::Pressed && Some(ev.id) == self.hk_unbind_cursor_id {
                do_unbind_at_cursor(&self.gm);
                self.rebuild_tray();
            } else if ev.state == global_hotkey::HotKeyState::Pressed && Some(ev.id) == self.hk_unbind_all_id {
                do_unbind_all(&self.gm);
                self.rebuild_tray();
            } else if ev.state == global_hotkey::HotKeyState::Pressed && Some(ev.id) == self.hk_sync_move_id {
                self.toggle_sync_move();
            }
        }
        while let Ok(ev) = self.menu_events.try_recv() {
            info!("Menu action: {}", ev.id().0);
            match ev.id().0.as_str() {
                ID_LASSO => self.lasso_bind(),
                ID_PICK_WINDOWS => {
                    self.unregister_hotkeys();
                    let result = crate::picker::show_window_picker(self.settings.sync_move);
                    bind_picked_windows(&self.gm, &self.settings, result);
                    self.discard_pending_hotkeys();
                    self.register_hotkeys();
                }
                ID_SYNC_MOVE => self.toggle_sync_move(),
                ID_UNBIND_CURSOR => { do_unbind_at_cursor(&self.gm); self.rebuild_tray(); },
                ID_UNBIND_ALL => { do_unbind_all(&self.gm); self.rebuild_tray(); },
                ID_RECOVER => {
                    let windows: Vec<isize> = crate::picker::get_pickable_windows().iter().map(|w| w.hwnd).collect();
                    let count = crate::platform::recover_windows(&windows);
                    info!("Recovered {} off-screen windows", count);
                }
                ID_HOTKEY_SETTINGS => {
                    self.unregister_hotkeys();

                    let result = hotkey_dialog::show_hotkey_dialog(
                        &self.settings.hotkey_bind,
                        &self.settings.hotkey_unbind_cursor,
                        &self.settings.hotkey_unbind_all,
                        &self.settings.hotkey_sync_move,
                    );

                    if let Some(new_hk) = result {
                        self.settings.hotkey_bind = new_hk.bind;
                        self.settings.hotkey_unbind_cursor = new_hk.unbind_cursor;
                        self.settings.hotkey_unbind_all = new_hk.unbind_all;
                        self.settings.hotkey_sync_move = new_hk.sync_move;
                        let _ = settings::save(&self.settings);
                        info!("Hotkeys updated: bind={}, unbind_cursor={}, unbind_all={}, sync_move={}",
                            settings::format_hotkey(&self.settings.hotkey_bind),
                            settings::format_hotkey(&self.settings.hotkey_unbind_cursor),
                            settings::format_hotkey(&self.settings.hotkey_unbind_all),
                            settings::format_hotkey(&self.settings.hotkey_sync_move));
                        // Registration below rebuilds the menu with actual availability.
                    }

                    self.discard_pending_hotkeys();
                    self.register_hotkeys();
                }
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
                    info!("Language: {}", self.settings.lang);
                    // Rebuild tray menu with new language
                    self.rebuild_tray();
                }
                ID_TOGGLE_AUTOSTART => {
                    self.settings.auto_start = !self.settings.auto_start;
                    if let Err(e) = settings::set_auto_start(self.settings.auto_start) {
                        error!("Auto-start error: {}", e);
                    }
                    let _ = settings::save(&self.settings);
                    info!("Auto-start: {}", if self.settings.auto_start { "ON" } else { "OFF" });
                    // Rebuild to reflect checkbox state
                    self.rebuild_tray();
                }
                id if id.starts_with("group_move:") => {
                    let parts: Vec<_> = id.split(':').collect();
                    if let [_, group_id, mode] = parts.as_slice() {
                        if let Some(mode) = mode.parse::<usize>().ok().and_then(|i| MovementMode::ALL.get(i)).copied() {
                            if let Some(group) = self.gm.lock().unwrap().groups.iter_mut().find(|g| g.id == *group_id) {
                                group.movement = mode;
                            }
                        }
                    }
                    self.rebuild_tray();
                }
                ID_QUIT => std::process::exit(0),
                _ => {}
            }
        }
    }
}

impl App {
    fn toggle_sync_move(&mut self) {
        let mut updated = self.settings.clone();
        updated.sync_move = !updated.sync_move;
        match settings::save(&updated) {
            Ok(()) => {
                self.settings = updated;
                self.gm.lock().unwrap().global_sync_move = self.settings.sync_move;
                info!("Sync movement: {}", self.settings.sync_move);
            }
            Err(e) => {
                error!("Failed to save movement setting: {}", e);
                #[cfg(target_os = "windows")]
                unsafe {
                    let message: Vec<u16> = t("msg.settings_error").encode_utf16().chain(Some(0)).collect();
                    let _ = MessageBoxW(None, windows::core::PCWSTR(message.as_ptr()),
                        windows::core::w!("AlwaysBindWindow"), MB_OK | MB_ICONERROR);
                }
            }
        }
        self.rebuild_tray();
    }

    fn discard_pending_hotkeys(&self) {
        while self.hotkey_events.try_recv().is_ok() {}
    }

    fn lasso_bind(&mut self) {
        self.unregister_hotkeys();
        do_lasso_bind(&self.gm, &self.settings);
        self.discard_pending_hotkeys();
        self.register_hotkeys();
    }

    fn unregister_hotkeys(&mut self) {
        if let Some(mgr) = &self.hk_mgr {
            if let Some(hk) = &self.hk_bind {
                let _ = mgr.unregister(*hk);
            }
            if let Some(hk) = &self.hk_unbind_cursor {
                let _ = mgr.unregister(*hk);
            }
            if let Some(hk) = &self.hk_unbind_all {
                let _ = mgr.unregister(*hk);
            }
            if let Some(hk) = &self.hk_sync_move {
                let _ = mgr.unregister(*hk);
            }
        }
        self.hk_bind = None;
        self.hk_unbind_cursor = None;
        self.hk_unbind_all = None;
        self.hk_sync_move = None;
        self.hk_bind_id = None;
        self.hk_unbind_cursor_id = None;
        self.hk_unbind_all_id = None;
        self.hk_sync_move_id = None;
    }

    fn register_hotkeys(&mut self) {
        if let Some(mgr) = &self.hk_mgr {
            let [bind, cursor, all, movement] = register_configured_hotkeys(&self.settings, |hk| mgr.register(hk));
            self.hk_bind = bind;
            self.hk_unbind_cursor = cursor;
            self.hk_unbind_all = all;
            self.hk_sync_move = movement;
            self.hk_bind_id = bind.map(|hk| hk.id());
            self.hk_unbind_cursor_id = cursor.map(|hk| hk.id());
            self.hk_unbind_all_id = all.map(|hk| hk.id());
            self.hk_sync_move_id = movement.map(|hk| hk.id());
            info!("Hotkeys re-registered");
        }
        self.rebuild_tray();
    }

    /// Rebuild the tray icon with updated menu (for language/hotkey/settings changes)
    fn rebuild_tray(&mut self) {
        // Drop old tray first
        self.tray.take();

        let menu = build_menu(&self.settings, &self.gm.lock().unwrap(), [
            self.hk_bind.is_some(), self.hk_unbind_cursor.is_some(),
            self.hk_unbind_all.is_some(), self.hk_sync_move.is_some(),
        ]);
        let tooltip = build_tooltip(&self.settings);

        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(&tooltip)
            .with_icon(create_icon())
            .build()
        {
            Ok(new_tray) => {
                self.tray = Some(new_tray);
            }
            Err(e) => {
                error!("Failed to rebuild tray: {}", e);
            }
        }
    }
}

pub fn run_tray(gm: Arc<Mutex<GroupManager>>, settings: Settings) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    // Native menu/hotkey callbacks do not wake winit's sleeping event loop.
    // Keep events in queues so nested native dialog pumps cannot reenter App,
    // and use the proxy to schedule about_to_wait after every incoming event.
    let (hotkey_tx, hotkey_events) = mpsc::channel();
    let hotkey_proxy = event_loop.create_proxy();
    GlobalHotKeyEvent::set_event_handler(Some(move |event| {
        let _ = hotkey_tx.send(event);
        let _ = hotkey_proxy.send_event(());
    }));
    let (menu_tx, menu_events) = mpsc::channel();
    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_tx.send(event);
        let _ = menu_proxy.send_event(());
    }));

    gm.lock().unwrap().global_sync_move = settings.sync_move;
    let hk_mgr = GlobalHotKeyManager::new()?;
    // An occupied shortcut must not block the event loop or disable other actions.
    let registered = register_configured_hotkeys(&settings, |hk| hk_mgr.register(hk));
    let menu = build_menu(&settings, &gm.lock().unwrap(), registered.map(|hk| hk.is_some()));
    let [hk_bind, hk_uc, hk_ua, hk_sync_move] = registered;
    let hk_bind_id = hk_bind.map(|hk| hk.id());
    let hk_uc_id = hk_uc.map(|hk| hk.id());
    let hk_ua_id = hk_ua.map(|hk| hk.id());
    let hk_sync_move_id = hk_sync_move.map(|hk| hk.id());
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(build_tooltip(&settings))
        .with_icon(create_icon())
        .build()?;

    info!("Tray ready; hotkey availability [bind, unbind-group, unbind-all, movement]: {:?}",
        registered.map(|hk| hk.is_some()));

    // Apply auto-start setting
    if settings.auto_start {
        let _ = settings::set_auto_start(true);
    }

    let mut app = App {
        hotkey_events,
        menu_events,
        gm,
        settings,
        tray: Some(tray),
        hk_mgr: Some(hk_mgr),
        hk_bind_id,
        hk_unbind_cursor_id: hk_uc_id,
        hk_unbind_all_id: hk_ua_id,
        hk_sync_move_id,
        hk_bind,
        hk_unbind_cursor: hk_uc,
        hk_unbind_all: hk_ua,
        hk_sync_move,
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Keep registration failures local to their shortcut. This path never opens a
/// modal dialog: it runs before the event loop and after nested selection loops.
fn register_configured_hotkeys<E: std::fmt::Display>(
    settings: &Settings,
    mut register: impl FnMut(global_hotkey::hotkey::HotKey) -> Result<(), E>,
) -> [Option<global_hotkey::hotkey::HotKey>; 4] {
    [&settings.hotkey_bind, &settings.hotkey_unbind_cursor,
        &settings.hotkey_unbind_all, &settings.hotkey_sync_move].map(|config| {
        let Some(hk) = settings::build_hotkey(config) else {
            error!("Invalid hotkey: {}", settings::format_hotkey(config));
            return None;
        };
        match register(hk) {
            Ok(()) => Some(hk),
            Err(error) => {
                error!("Hotkey unavailable: {}: {}", settings::format_hotkey(config), error);
                None
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occupied_movement_shortcut_does_not_disable_bind_or_unbind() {
        let settings = Settings::default();
        let occupied = settings::build_hotkey(&settings.hotkey_sync_move).unwrap();
        let mut attempts = 0;
        let registered = register_configured_hotkeys(&settings, |hk| {
            attempts += 1;
            if hk == occupied { Err("already registered") } else { Ok(()) }
        });
        assert_eq!(attempts, 4);
        assert_eq!(registered.map(|hk| hk.is_some()), [true, true, true, false]);
        assert_eq!(registered[0], settings::build_hotkey(&settings.hotkey_bind));
    }

    #[test]
    fn registration_failure_does_not_skip_later_shortcuts() {
        let settings = Settings::default();
        let occupied = settings::build_hotkey(&settings.hotkey_bind).unwrap();
        let registered = register_configured_hotkeys(&settings, |hk|
            if hk == occupied { Err("already registered") } else { Ok(()) });
        assert_eq!(registered.map(|hk| hk.is_some()), [false, true, true, true]);
        let retried = register_configured_hotkeys(&settings, |_| Ok::<_, &str>(()));
        assert!(retried.iter().all(Option::is_some));
    }
}
