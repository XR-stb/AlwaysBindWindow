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
    // Check Windows UI language
    #[cfg(target_os = "windows")]
    {
        let lang = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        // Chinese: 0x0804 (zh-CN), 0x0404 (zh-TW), 0x0C04 (zh-HK)
        if (lang & 0xFF) == 0x04 { return Lang::Zh; }
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
        "hk.bind" => if zh { "框选绑定窗口" } else { "Bind windows (lasso)" },
        "hk.unbind_cursor" => if zh { "解绑光标所在组" } else { "Unbind group under cursor" },
        "hk.unbind_all" => if zh { "解绑全部" } else { "Unbind all" },

        // Tray menu
        "menu.bind" => if zh { "框选绑定" } else { "Bind Windows" },
        "menu.unbind_cursor" => if zh { "解绑此组" } else { "Unbind This Group" },
        "menu.unbind_all" => if zh { "解绑全部" } else { "Unbind All" },
        "menu.lang" => if zh { "English" } else { "中文" },
        "menu.quit" => if zh { "退出" } else { "Quit" },

        // Overlay
        "overlay.hint" => if zh { "拖框选择 | ESC = 取消" } else { "Drag to select | ESC = cancel" },
        "overlay.selecting" => if zh { "选择中..." } else { "Selecting..." },

        // Messages
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
