#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod group;
mod platform;
mod tray;
mod overlay;
mod picker;
mod i18n;
mod settings;
mod hotkey_dialog;
mod window_geometry;
mod window_selection;
#[cfg(target_os = "windows")]
mod native_ui;

use log::{info, error};
use std::sync::{Arc, Mutex};
use group::GroupManager;

#[cfg(target_os = "windows")]
struct InstanceGuard(windows::Win32::Foundation::HANDLE);

#[cfg(target_os = "windows")]
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe { let _ = windows::Win32::Foundation::CloseHandle(self.0); }
    }
}

#[cfg(target_os = "windows")]
fn acquire_instance() -> windows::core::Result<Option<InstanceGuard>> {
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    unsafe {
        let handle = CreateMutexW(None, false, windows::core::w!("Local\\AlwaysBindWindow.Instance"))?;
        let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
        let guard = InstanceGuard(handle);
        if already_running { Ok(None) } else { Ok(Some(guard)) }
    }
}

fn show_startup_error(message: &str) {
    error!("{}", message);
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::*;
        let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let _ = MessageBoxW(None, windows::core::PCWSTR(message.as_ptr()),
            windows::core::w!("AlwaysBindWindow"), MB_OK | MB_ICONERROR);
    }
}

fn main() {
    let mut logger = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    // GUI builds have no stderr console; opt-in file logging supports diagnosis.
    if let Some(path) = std::env::var_os("ALWAYSBINDWINDOW_LOG") {
        if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            logger.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }
    logger.init();

    #[cfg(target_os = "windows")]
    let _instance = match acquire_instance() {
        Ok(Some(guard)) => guard,
        Ok(None) => return,
        Err(error) => {
            show_startup_error(&format!("Could not initialize AlwaysBindWindow: {}", error));
            return;
        }
    };

    // Load settings
    let s = settings::load();

    // Set language
    match s.lang.as_str() {
        "zh" => i18n::set_lang(i18n::Lang::Zh),
        "en" => i18n::set_lang(i18n::Lang::En),
        _ => i18n::set_lang(i18n::detect_system_lang()),
    }

    info!("AlwaysBindWindow v{} starting...", env!("CARGO_PKG_VERSION"));
    info!("Hotkeys: {} = Bind, {} = Unbind Group, {} = Unbind All",
        settings::format_hotkey(&s.hotkey_bind),
        settings::format_hotkey(&s.hotkey_unbind_cursor),
        settings::format_hotkey(&s.hotkey_unbind_all));

    let group_manager = Arc::new(Mutex::new(GroupManager::new()));

    let gm_clone = Arc::clone(&group_manager);
    std::thread::spawn(move || {
        if let Err(e) = platform::start_monitor(gm_clone) {
            error!("Monitor error: {}", e);
        }
    });

    if let Err(e) = tray::run_tray(group_manager, s) {
        show_startup_error(&format!("{}\n{}", i18n::t("msg.startup_error"), e));
    }
    // The monitor runs indefinitely. Joining it after a tray startup failure
    // leaves an invisible process alive, potentially still holding hotkeys.
}
