# 🔗 AlwaysBindWindow

**Bind multiple windows together — move, activate, and minimize as one.**

**将多个窗口绑定在一起 — 同步移动、激活、最小化，如同一体。**

[English](#english) | [中文](#中文)

---

## English

### What is this?

AlwaysBindWindow lets you group windows from **different applications** so they behave as one unit:

- **Activate together**: Click any window in a group → all grouped windows come to the foreground
- **Move together**: Drag one window → all others follow, maintaining their relative positions
- **Minimize/Restore together**: Minimize one → all minimize; restore one → all restore
- **Visual lasso selection**: Just like taking a screenshot — drag a rectangle to select windows

### Quick Start

1. **Download** the latest release from [Releases](https://github.com/XR-stb/AlwaysBindWindow/releases)
2. **Run** `always-bind-window.exe` — it starts as a system tray icon
3. **Press `Ctrl+Alt+G`** — a dark overlay appears
4. **Drag a rectangle** over the windows you want to bind
5. **Release mouse** — done! The windows are now linked

### Hotkeys

| Hotkey | Action |
|--------|--------|
| `Ctrl+Alt+G` | Lasso-select windows to bind |
| `Ctrl+Alt+D` | Unbind the group under cursor |
| `Ctrl+Alt+U` | Unbind all groups |

All hotkeys are **customizable** via the settings file.

### Tray Menu

Right-click the tray icon for:
- Bind / Unbind controls
- Language toggle (English ↔ 中文)
- Auto-start on login toggle
- Quit

### Settings

Settings are stored at:
- **Windows**: `%APPDATA%/AlwaysBindWindow/settings.json`
- **macOS**: `~/Library/Application Support/AlwaysBindWindow/settings.json`

Example `settings.json`:
```json
{
  "lang": "auto",
  "hotkey_bind": { "modifiers": "Ctrl+Alt", "key": "G" },
  "hotkey_unbind_cursor": { "modifiers": "Ctrl+Alt", "key": "D" },
  "hotkey_unbind_all": { "modifiers": "Ctrl+Alt", "key": "U" },
  "sync_move": true,
  "sync_minimize": true,
  "auto_start": false
}
```

### How It Works

1. **Window Event Hooks** (`SetWinEventHook`): Monitors foreground changes, minimize/restore events
2. **Polling Thread** (8ms/~120fps): Tracks cursor position to sync window movement with zero drift
3. **Z-order Preservation**: When bringing a group to front, internal window stacking order is maintained
4. **Occlusion-aware Selection**: Only visible (non-occluded) windows can be selected during lasso

### Platform Support

| Platform | Status |
|----------|--------|
| Windows 10/11 | ✅ Fully supported |
| macOS | 🚧 Planned |
| Linux | 📋 Planned |

### Build from Source

```bash
# Prerequisites: Rust 1.75+
git clone https://github.com/XR-stb/AlwaysBindWindow.git
cd AlwaysBindWindow
cargo build --release
# Binary at: target/release/always-bind-window.exe
```

---

## 中文

### 这是什么？

AlwaysBindWindow 可以将**不同应用**的窗口绑定成一组，像同一个软件的窗口一样联动：

- **同步激活**：点击组内任一窗口 → 所有窗口一起浮到前台
- **同步移动**：拖动一个窗口 → 其他窗口跟着动，保持相对位置
- **同步最小化/恢复**：最小化一个 → 全部最小化；恢复一个 → 全部恢复
- **框选绑定**：像截图一样拖一个框，框住的窗口就绑在一起了

### 快速开始

1. 从 [Releases](https://github.com/XR-stb/AlwaysBindWindow/releases) 下载最新版本
2. 运行 `always-bind-window.exe` — 程序以系统托盘图标驻留
3. 按 **`Ctrl+Alt+G`** — 屏幕出现暗色覆盖层
4. **拖框选择**要绑定的窗口
5. **松开鼠标** — 完成！窗口已绑定

### 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+Alt+G` | 框选绑定窗口 |
| `Ctrl+Alt+D` | 解绑光标所在的组 |
| `Ctrl+Alt+U` | 解绑全部 |

所有快捷键均可通过配置文件**自定义**。

### 托盘菜单

右键托盘图标：
- 绑定 / 解绑操作
- 语言切换（中文 ↔ English）
- 开机自启动开关
- 退出

### 配置文件

配置文件位于：
- **Windows**: `%APPDATA%/AlwaysBindWindow/settings.json`
- **macOS**: `~/Library/Application Support/AlwaysBindWindow/settings.json`

首次运行会自动生成默认配置。修改后重启程序生效。

### 技术原理

1. **事件钩子** (`SetWinEventHook`)：监听窗口激活、最小化、恢复事件
2. **轮询线程** (8ms/~120fps)：追踪鼠标位置实现零漂移的窗口跟随移动
3. **Z-order 保持**：前置窗口组时保持组内原有的窗口层级关系
4. **遮挡感知框选**：只有在屏幕上可见的窗口才会被框选到

### 从源码构建

```bash
# 需要 Rust 1.75+
git clone https://github.com/XR-stb/AlwaysBindWindow.git
cd AlwaysBindWindow
cargo build --release
# 产物：target/release/always-bind-window.exe
```

---

## License

MIT License — see [LICENSE](LICENSE) for details.
