// Uncomment for release:
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod group;
mod platform;
mod tray;
mod overlay;
mod picker;
mod i18n;
mod settings;

use log::{info, error};
use std::sync::{Arc, Mutex};
use group::GroupManager;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Load settings
    let s = settings::load();

    // Set language
    match s.lang.as_str() {
        "zh" => i18n::set_lang(i18n::Lang::Zh),
        "en" => i18n::set_lang(i18n::Lang::En),
        _ => i18n::set_lang(i18n::detect_system_lang()),
    }

    let bind_hk = settings::format_hotkey(&s.hotkey_bind);
    let unbind_c = settings::format_hotkey(&s.hotkey_unbind_cursor);
    let unbind_a = settings::format_hotkey(&s.hotkey_unbind_all);

    println!("{}", "=".repeat(45));
    println!("  {} v{}", i18n::t("app.name"), env!("CARGO_PKG_VERSION"));
    println!("{}", "-".repeat(45));
    println!("  {}  ->  {}", bind_hk, i18n::t("hk.bind"));
    println!("  {}  ->  {}", unbind_c, i18n::t("hk.unbind_cursor"));
    println!("  {}  ->  {}", unbind_a, i18n::t("hk.unbind_all"));
    println!("{}", "=".repeat(45));
    println!();

    info!("AlwaysBindWindow starting...");

    let group_manager = Arc::new(Mutex::new(GroupManager::new()));

    let gm_clone = Arc::clone(&group_manager);
    let monitor_handle = std::thread::spawn(move || {
        if let Err(e) = platform::start_monitor(gm_clone) {
            error!("Monitor error: {}", e);
        }
    });

    if let Err(e) = tray::run_tray(group_manager, s) {
        error!("Tray error: {}", e);
    }

    let _ = monitor_handle.join();
}
