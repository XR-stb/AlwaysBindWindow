/// Internationalization — Chinese + English
use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(0); // 0 = English, 1 = Chinese

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang { En, Zh }

pub fn set_lang(lang: Lang) {
    LANG.store(match lang { Lang::En => 0, Lang::Zh => 1 }, Ordering::SeqCst);
}

pub fn get_lang() -> Lang {
    match LANG.load(Ordering::SeqCst) { 1 => Lang::Zh, _ => Lang::En }
}

pub fn detect_system_lang() -> Lang {
    #[cfg(target_os = "windows")]
    {
        let lang = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        if (lang & 0xFF) == 0x04 { return Lang::Zh; }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Check LANG / LC_ALL environment variable
        for var in &["LANG", "LC_ALL", "LC_MESSAGES"] {
            if let Ok(val) = std::env::var(var) {
                let lower = val.to_lowercase();
                if lower.starts_with("zh") { return Lang::Zh; }
            }
        }
    }
    Lang::En
}

pub fn t(key: &str) -> &'static str {
    let zh = get_lang() == Lang::Zh;
    match key {
        // App
        "app.name" => if zh { "窗口绑定助手" } else { "AlwaysBindWindow" },
        "app.ready" => if zh { "  就绪！快捷键已激活。" } else { "  Ready! Hotkeys active." },

        // Hotkeys
        "hk.unavailable" => if zh { "快捷键不可用" } else { "Shortcut unavailable" },
        "menu.hotkey_settings_conflict" => if zh { "快捷键设置…（部分快捷键不可用）" } else { "Hotkey Settings… (some shortcuts unavailable)" },
        "hk.bind" => if zh { "框选绑定窗口" } else { "Bind windows (lasso)" },
        "hk.unbind_cursor" => if zh { "解绑光标所在组" } else { "Unbind group under cursor" },
        "hk.unbind_all" => if zh { "解绑全部" } else { "Unbind all" },

        // Tray menu
        "menu.pick_windows" => if zh { "从列表选择窗口…" } else { "Choose Windows from List…" },
        "menu.sync_move" => if zh { "同步移动（全局）" } else { "Move Together (Global)" },
        "menu.recover" => if zh { "找回屏幕外窗口" } else { "Recover Off-screen Windows" },
        "menu.bind" => if zh { "框选绑定" } else { "Bind Windows" },
        "menu.unbind_cursor" => if zh { "解绑此组" } else { "Unbind This Group" },
        "menu.unbind_all" => if zh { "解绑全部" } else { "Unbind All" },
        "menu.lang" => if zh { "Switch to English" } else { "\u{5207}\u{6362}\u{5230}\u{4E2D}\u{6587} (Chinese)" },
        "menu.quit" => if zh { "退出" } else { "Quit" },

        "menu.groups" => if zh { "各组同步移动" } else { "Movement by Group" },
        "menu.no_groups" => if zh { "尚未绑定窗口" } else { "No bound groups" },
        "movement.label" => if zh { "此组同步移动(&M)" } else { "Group &movement" },
        "movement.inherit" => if zh { "跟随全局" } else { "Follow global" },
        "movement.on" => if zh { "开启" } else { "On" },
        "movement.off" => if zh { "关闭" } else { "Off" },

        // Overlay
        "overlay.hint" => if zh { "点选或框选窗口，可跨屏追加选择 · Esc 取消" } else { "Click or drag to select windows across screens · Esc to cancel" },
        "overlay.one_selected" => if zh { "已选 1 个窗口 · 请继续点选或框选 · Esc 取消" } else { "1 window selected · Select another · Esc to cancel" },
        "overlay.ready" => if zh { "可继续点选或框选 · Enter 绑定 · Esc 取消" } else { "Keep selecting · Enter to bind · Esc to cancel" },
        "overlay.movement" => if zh { "同步移动" } else { "Movement" },
        "overlay.bind" => if zh { "绑定" } else { "Bind" },
        "overlay.title" => if zh { "选择窗口并绑定" } else { "Select and Bind Windows" },
        "overlay.clear" => if zh { "重选" } else { "Clear" },
        "overlay.stale" => if zh { "部分所选窗口已关闭，已移除。请检查选择后再次绑定。" } else { "Closed windows were removed. Review your selection and bind again." },
        "overlay.selecting" => if zh { "选择中..." } else { "Selecting..." },

        // Hotkey settings dialog
        "menu.hotkey_settings" => if zh { "快捷键设置" } else { "Hotkey Settings" },
        "hk_dlg.title" => if zh { "快捷键设置" } else { "Hotkey Settings" },
        "hk_dlg.bind_label" => if zh { "框选绑定：" } else { "Bind (Lasso):" },
        "hk_dlg.unbind_cursor_label" => if zh { "解绑此组：" } else { "Unbind Group:" },
        "hk_dlg.unbind_all_label" => if zh { "解绑全部：" } else { "Unbind All:" },
        "hk_dlg.sync_move_label" => if zh { "切换同步移动：" } else { "Toggle Movement:" },
        "hk_dlg.hint" => if zh { "点击输入框后按下新的快捷键组合" } else { "Click a field then press new key combo" },
        "hk_dlg.save" => if zh { "保存" } else { "Save" },
        "hk_dlg.cancel" => if zh { "取消" } else { "Cancel" },
        "hk_dlg.reset" => if zh { "恢复默认" } else { "Reset Defaults" },
        "hk_dlg.saved" => if zh { "快捷键已更新" } else { "Hotkeys updated" },
        "hk_dlg.invalid" => if zh { "快捷键无效或重复，请使用不同的字母、数字或 F1–F12 组合" } else { "Invalid or duplicate hotkeys. Use distinct letter, digit or F1–F12 combinations." },
        "hk_dlg.recording" => if zh { "按下快捷键..." } else { "Press keys..." },

        // Messages
        "msg.startup_error" => if zh { "无法启动托盘或注册快捷键。请退出其他实例，或检查快捷键是否被占用后重试。" } else { "Could not start the tray or register hotkeys. Close other instances or check for shortcut conflicts and try again." },
        "msg.settings_error" => if zh { "无法保存设置。请检查配置目录是否可写，然后重试。" } else { "Could not save settings. Check that the settings folder is writable and try again." },
        "picker.title" => if zh { "选择要绑定的窗口" } else { "Choose Windows to Bind" },
        "picker.hint" => if zh { "单击或按空格切换选择；至少选择 2 个窗口，可跨显示器。" } else { "Click or press Space to toggle selection. Choose at least 2 windows, on any monitor." },
        "picker.list" => if zh { "窗口列表(&W)" } else { "&Windows" },
        "picker.bind" => if zh { "绑定所选窗口(&B)" } else { "&Bind Selected" },
        "picker.refresh" => if zh { "刷新(&R)" } else { "&Refresh" },
        "picker.cancel" => if zh { "取消" } else { "Cancel" },
        "picker.selected" => if zh { "已选择窗口：" } else { "Selected windows: " },
        "picker.empty" => if zh { "没有可选窗口。打开至少 2 个应用窗口，然后刷新。" } else { "No windows available. Open at least 2 application windows, then refresh." },
        "picker.stale" => if zh { "部分窗口已关闭或不可用。列表已刷新，请确认后重新绑定。" } else { "Some windows are no longer available. Review the refreshed list and bind again." },
        "picker.error" => if zh { "无法打开窗口列表，请重试。" } else { "Could not open the window list. Please try again." },
        "msg.bound" => if zh { "已绑定" } else { "Bound" },
        "msg.windows" => if zh { "个窗口" } else { "windows" },
        "msg.unbound_group" => if zh { "已解绑组" } else { "Unbound group" },
        "msg.unbound_all" => if zh { "已解绑全部" } else { "Unbound all" },
        "msg.no_group" => if zh { "光标下的窗口不在任何组中" } else { "Window under cursor is not in any group" },
        "msg.cancelled" => if zh { "已取消" } else { "Cancelled" },
        "msg.need2" => if zh { "至少需要选择2个窗口" } else { "Need at least 2 windows" },

        _ => "???",
    }
}
