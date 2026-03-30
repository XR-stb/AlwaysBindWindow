/// Persistent settings: custom hotkeys + language
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub modifiers: String,   // e.g. "Ctrl+Alt"
    pub key: String,         // e.g. "G"
}

impl HotkeyConfig {
    pub fn new(modifiers: &str, key: &str) -> Self {
        Self { modifiers: modifiers.to_string(), key: key.to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub lang: String,  // "en" or "zh" or "auto"
    pub hotkey_bind: HotkeyConfig,
    pub hotkey_unbind_cursor: HotkeyConfig,
    pub hotkey_unbind_all: HotkeyConfig,
    pub sync_move: bool,
    pub sync_minimize: bool,
    pub auto_start: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: "auto".to_string(),
            hotkey_bind: HotkeyConfig::new("Ctrl+Alt", "G"),
            hotkey_unbind_cursor: HotkeyConfig::new("Ctrl+Alt", "D"),
            hotkey_unbind_all: HotkeyConfig::new("Ctrl+Alt", "U"),
            sync_move: true,
            sync_minimize: true,
            auto_start: false,
        }
    }
}

fn settings_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AlwaysBindWindow");
    let _ = fs::create_dir_all(&dir);
    dir.join("settings.json")
}

pub fn load() -> Settings {
    let path = settings_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(s) = serde_json::from_str::<Settings>(&content) {
            return s;
        }
    }
    // Return default and save it
    let s = Settings::default();
    let _ = save(&s);
    s
}

pub fn save(settings: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    let path = settings_path();
    let content = serde_json::to_string_pretty(settings)?;
    fs::write(&path, content)?;
    Ok(())
}

/// Parse modifier string like "Ctrl+Alt" into global_hotkey Modifiers
pub fn parse_modifiers(s: &str) -> global_hotkey::hotkey::Modifiers {
    use global_hotkey::hotkey::Modifiers;
    let mut m = Modifiers::empty();
    let upper = s.to_uppercase();
    if upper.contains("CTRL") || upper.contains("CONTROL") { m |= Modifiers::CONTROL; }
    if upper.contains("ALT") { m |= Modifiers::ALT; }
    if upper.contains("SHIFT") { m |= Modifiers::SHIFT; }
    if upper.contains("SUPER") || upper.contains("WIN") || upper.contains("META") { m |= Modifiers::SUPER; }
    m
}

/// Parse key string like "G" into global_hotkey Code
pub fn parse_key(s: &str) -> Option<global_hotkey::hotkey::Code> {
    use global_hotkey::hotkey::Code;
    match s.to_uppercase().as_str() {
        "A" => Some(Code::KeyA), "B" => Some(Code::KeyB), "C" => Some(Code::KeyC),
        "D" => Some(Code::KeyD), "E" => Some(Code::KeyE), "F" => Some(Code::KeyF),
        "G" => Some(Code::KeyG), "H" => Some(Code::KeyH), "I" => Some(Code::KeyI),
        "J" => Some(Code::KeyJ), "K" => Some(Code::KeyK), "L" => Some(Code::KeyL),
        "M" => Some(Code::KeyM), "N" => Some(Code::KeyN), "O" => Some(Code::KeyO),
        "P" => Some(Code::KeyP), "Q" => Some(Code::KeyQ), "R" => Some(Code::KeyR),
        "S" => Some(Code::KeyS), "T" => Some(Code::KeyT), "U" => Some(Code::KeyU),
        "V" => Some(Code::KeyV), "W" => Some(Code::KeyW), "X" => Some(Code::KeyX),
        "Y" => Some(Code::KeyY), "Z" => Some(Code::KeyZ),
        "0" | ")" => Some(Code::Digit0), "1" | "!" => Some(Code::Digit1),
        "2" | "@" => Some(Code::Digit2), "3" | "#" => Some(Code::Digit3),
        "4" | "$" => Some(Code::Digit4), "5" | "%" => Some(Code::Digit5),
        "6" | "^" => Some(Code::Digit6), "7" | "&" => Some(Code::Digit7),
        "8" | "*" => Some(Code::Digit8), "9" | "(" => Some(Code::Digit9),
        "F1" => Some(Code::F1), "F2" => Some(Code::F2), "F3" => Some(Code::F3),
        "F4" => Some(Code::F4), "F5" => Some(Code::F5), "F6" => Some(Code::F6),
        "F7" => Some(Code::F7), "F8" => Some(Code::F8), "F9" => Some(Code::F9),
        "F10" => Some(Code::F10), "F11" => Some(Code::F11), "F12" => Some(Code::F12),
        _ => None,
    }
}

/// Build a HotKey from config
pub fn build_hotkey(cfg: &HotkeyConfig) -> Option<global_hotkey::hotkey::HotKey> {
    use global_hotkey::hotkey::HotKey;
    let mods = parse_modifiers(&cfg.modifiers);
    let key = parse_key(&cfg.key)?;
    Some(HotKey::new(Some(mods), key))
}

/// Format hotkey for display: "Ctrl+Alt+G"
pub fn format_hotkey(cfg: &HotkeyConfig) -> String {
    format!("{}+{}", cfg.modifiers, cfg.key)
}

// ── Auto-start ────────────────────────────────────────────────

const APP_NAME: &str = "AlwaysBindWindow";

/// Set or remove auto-start on login
pub fn set_auto_start(enable: bool) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        set_auto_start_windows(enable)
    }
    #[cfg(target_os = "macos")]
    {
        set_auto_start_macos(enable)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err("Auto-start not supported on this platform".into())
    }
}

#[cfg(target_os = "windows")]
fn set_auto_start_windows(enable: bool) -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;
    let exe_path = std::env::current_exe()?.to_string_lossy().to_string();

    if enable {
        // Add to HKCU\Software\Microsoft\Windows\CurrentVersion\Run
        Command::new("reg")
            .args(["add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v", APP_NAME,
                "/t", "REG_SZ",
                "/d", &exe_path,
                "/f"])
            .output()?;
    } else {
        // Remove from registry
        let _ = Command::new("reg")
            .args(["delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v", APP_NAME,
                "/f"])
            .output();
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_auto_start_macos(enable: bool) -> Result<(), Box<dyn std::error::Error>> {
    let plist_dir = dirs::home_dir()
        .ok_or("No home dir")?
        .join("Library/LaunchAgents");
    let plist_path = plist_dir.join("com.alwaysbindwindow.plist");

    if enable {
        let exe_path = std::env::current_exe()?.to_string_lossy().to_string();
        let plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.alwaysbindwindow</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#, exe_path);
        let _ = fs::create_dir_all(&plist_dir);
        fs::write(&plist_path, plist)?;
    } else {
        let _ = fs::remove_file(&plist_path);
    }
    Ok(())
}

/// Check if auto-start is currently enabled
pub fn is_auto_start_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("reg")
            .args(["query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v", APP_NAME])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .map(|h| h.join("Library/LaunchAgents/com.alwaysbindwindow.plist").exists())
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    { false }
}
