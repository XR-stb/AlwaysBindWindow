# 🔗 AlwaysBindWindow

**Bind multiple windows together — move, activate, and minimize as one.**

**将多个窗口绑定在一起 — 同步移动、激活、最小化，如同一体。**

![demo](https://github.com/user-attachments/assets/55724435-7678-4bc8-8edf-07930d6c6820)

[English](#english) | [中文](#中文)

---

## English

### What is this?

AlwaysBindWindow lets you group windows from **different applications** so they behave as one unit:

- **Activate together**: Click any window in a group → all grouped windows come to the foreground
- **Move together**: Drag one window → all others follow, maintaining their relative positions
- **Minimize/Restore together**: Minimize one → all minimize; restore one → all restore
- **Visual selection**: Click individual windows or add several with a rectangle, across monitors; confirm when ready
- **Window list selection**: Pick individual windows across monitors, including covered or minimized windows, without rearranging them
- **Independent movement**: Choose **Follow global / On / Off** for each group. Explicit group choices override **Move Together (Global)**; activation and minimize/restore still work together

### Quick Start

1. **Download** the latest release from [Releases](https://github.com/XR-stb/AlwaysBindWindow/releases)
2. **Run** `always-bind-window.exe` — it starts as a system tray icon
3. **Press `Ctrl+Alt+G`** — a dark overlay and a small instruction tip appear
4. **Click windows to toggle selection**, or drag rectangles to add windows. Selection stays active across monitors. A compact action bar appears once two windows are selected. Drag its hint area aside if needed.
5. Choose **Group movement: Follow global / On / Off** in the panel. Press **Enter** or **Bind Selected** to bind; **Esc** cancels.

Alternatively, right-click the tray icon → **Choose Windows from List…**. Click individual rows (or use arrow keys and Space) to select at least two windows, then click **Bind Selected**. **Refresh** updates the list while preserving available selections. Each row represents one window, including File Explorer folders and separate windows of the same application. Group membership and group overrides last for the current app session. Both selection workflows offer the same group movement choices.

### Hotkeys

| Hotkey | Action |
|--------|--------|
| `Ctrl+Alt+G` | Lasso-select windows to bind |
| `Ctrl+Alt+D` | Unbind the group under cursor |
| `Ctrl+Alt+U` | Unbind all groups |
| `Ctrl+Alt+M` | Toggle the global movement default (explicit group overrides are kept) |

All hotkeys are **customizable** — right-click the tray icon → **Hotkey Settings** to change them in-app.

### Tray Menu

Right-click the tray icon for:
- Bind / Unbind controls
- **Choose Windows from List…** — select exact windows without a lasso (Windows)
- **Move Together (Global)** — on by default and remembered after restart; applies to groups using Follow global
- **Movement by Group** — select a named group, then Follow global / On / Off; group choices take precedence
- **Recover Off-screen Windows** — bring unreachable title bars back onto a monitor; unbinding also recovers affected windows
- **Hotkey Settings** — change shortcuts directly in a visual dialog
- Language toggle (Switch to English ↔ 切换到中文)
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
  "hotkey_sync_move": { "modifiers": "Ctrl+Alt", "key": "M" },
  "sync_move": true,
  "sync_minimize": true,
  "auto_start": false
}
```

### How It Works

1. **Window Event Hooks** (`SetWinEventHook`): Monitors foreground changes, minimize/restore events
2. **Polling Thread** (8ms/~120fps): Tracks the dragged window's position to sync movement when **Move Together** is enabled
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
- **点选和框选**：逐个点选或连续拖框追加选择，可跨屏操作，确认后才绑定
- **列表选择**：逐个选择不同显示器上的窗口，包括被遮挡或最小化的窗口，无需先调整布局
- **独立移动**：每组可选择「跟随全局 / 开启 / 关闭」，组内明确设置优先于全局；仍保持同步激活、最小化和恢复

### 快速开始

1. 从 [Releases](https://github.com/XR-stb/AlwaysBindWindow/releases) 下载最新版本
2. 运行 `always-bind-window.exe` — 程序以系统托盘图标驻留
3. 按 **`Ctrl+Alt+G`** — 屏幕出现暗色覆盖层和简短操作提示
4. **点选窗口**，再次点击取消；也可连续拖框追加选择，跨屏选择不会丢失。选满两个窗口后，提示条会展开操作栏；挡住目标时可拖动提示区移到旁边。
5. 在面板中选择**此组同步移动：跟随全局 / 开启 / 关闭**，按 **Enter** 或点击**绑定所选窗口**确认；**Esc** 取消。

也可以右键托盘图标 → **从列表选择窗口…**，单击各行（或用方向键和空格）选择至少两个窗口，再点击 **绑定所选窗口**。**刷新**会更新列表并保留仍可用的选择。每一行代表一个具体窗口，包括文件资源管理器文件夹和同一应用的多个窗口。窗口分组及组级设置仅在本次程序运行期间有效；两种选择方式都能设置此组同步移动。

### 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+Alt+G` | 框选绑定窗口 |
| `Ctrl+Alt+D` | 解绑光标所在的组 |
| `Ctrl+Alt+U` | 解绑全部 |
| `Ctrl+Alt+M` | 切换全局同步移动，不覆盖组的独立设置 |

所有快捷键均可在软件中直接修改 — 右键托盘图标 → **快捷键设置**。

### 托盘菜单

右键托盘图标：
- 绑定 / 解绑操作
- **从列表选择窗口…** — 不用框选，精确选择窗口（Windows）
- **同步移动（全局）** — 默认开启，影响选择「跟随全局」的组，重启后保留开关状态
- **各组同步移动** — 按窗口标题找到组，选择「跟随全局 / 开启 / 关闭」，组的独立设置优先
- **找回屏幕外窗口** — 将无法拖动的标题栏移回屏幕内，解绑时也会自动检查组内窗口
- **快捷键设置** — 直接在界面中修改快捷键
- 语言切换（切换到中文 ↔ Switch to English）
- 开机自启动开关
- 退出

### 配置文件

配置文件位于：
- **Windows**: `%APPDATA%/AlwaysBindWindow/settings.json`
- **macOS**: `~/Library/Application Support/AlwaysBindWindow/settings.json`

首次运行会自动生成默认配置。快捷键建议通过托盘菜单的「快捷键设置」修改，实时生效无需重启。

### 技术原理

1. **事件钩子** (`SetWinEventHook`)：监听窗口激活、最小化、恢复事件
2. **轮询线程** (8ms/~120fps)：开启同步移动时，追踪被拖动窗口的位置，让组内窗口跟随移动
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
