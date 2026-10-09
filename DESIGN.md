---
version: alpha
name: AlwaysBindWindow
description: A Windows tray utility for choosing and linking desktop windows.
colors:
  primary: "#006D78"
  background: "#F5F7F8"
  surface: "#FFFFFF"
  text: "#202B38"
  muted: "#5B6979"
  border: "#D8DEE5"
  selected: "#E5F3F5"
typography:
  controls:
    fontFamily: "Segoe UI, sans-serif"
omitted:
  - section: rounded
    reason: Dialog controls use shared 7 logical pixel button corners; Windows owns tray geometry.
  - section: spacing
    reason: Native dialog dimensions are DPI-scaled in their implementation.
  - section: components
    reason: Native control ownership and behavior are documented below.
---

# AlwaysBindWindow design context

## Overview

Product register: a compact desktop utility for people arranging several application windows, including across monitors. README.md and issues #1/#2 establish the workflows. English and Chinese are supported through `src/i18n.rs`; there is no Japan-specific product scope.

Keep the teal linked-ring tray icon and native Windows conventions. Choosing exact windows is the central task; avoid dashboard layouts, decorative animation, or a visual rebrand.

The user requested a more contemporary picker and reported clipped hotkey settings at high DPI. `src/native_ui.rs` is now the shared runtime owner for dialog colors, rounded button rendering, typography and DPI context. This file mirrors those tokens; it does not generate code. The teal linked-ring tray icon remains unchanged.

## Colors

Dialog backgrounds use #F5F7F8 with white fields and list rows. Text is #202B38; secondary instructions and application names use #5B6979. The primary action and checked indicators use #006D78, with white text. Selection uses #E5F3F5 and a visible check mark, not color alone. Borders use #D8DEE5. COLORREF constants in native_ui.rs encode these values in BGR.

## Typography

Segoe UI with Windows fallback for Chinese. The picker uses a 24 logical pixel semibold heading and 15 pixel body; the compact shortcut dialog uses a 20 pixel heading, 14 pixel labels and 15 pixel fields. Window titles occupy the first row and application names the second. Long titles are ellipsized visually; full text remains stored in the native listbox for accessibility. Do not show raw window handles as user-facing labels.

## Layout

Both dialogs enter the same system-aware DPI scope. Their client size, fonts, spacing and control bounds scale together; AdjustWindowRectExForDpi adds the title bar and frame. Windows scales the complete dialog when it moves to a monitor with a different scale. This deliberately favors consistent geometry over per-monitor font rerasterization.

Dialogs open on the pointer's monitor within its work area. The shortcut page has a 520×412 logical pixel client area. The resizable picker starts at 760×560, with 24 pixel margins and 64 pixel two-line rows. The list scrolls vertically; actions and selection status stay outside that scroll region. These are desktop tools, not web or phone surfaces.

## Elevation & Depth

Windows owns the picker frame and shadow. Avoid persistent topmost behavior so users can inspect other windows before committing their selection.

## Shapes

Keep native BUTTON and LISTBOX semantics, with shared owner-drawn rounded buttons, checkbox indicators and softly rounded selection backgrounds. The selection overlay dims the desktop and outlines selected windows without overlaid titles or watermarks. Its opaque, draggable native tip bar uses the same palette, buttons and movement dropdown as the list picker.

## Components

| Capability | Canonical owner | Source of truth | Allowed variants | Verification |
|---|---|---|---|---|
| Select/Listbox | `src/picker_dialog.rs`, Win32 LISTBOX | README.md, issues #1/#2 | Owner-drawn native multi-select list; lasso remains in `src/overlay.rs` | TESTING.md: list, keyboard, stale windows, scrolling |
| Movement Select | `src/native_ui.rs::init_movement_choices` | User-approved group-over-global priority | Native COMBOBOX in list and overlay panel; Windows owns popup geometry | TESTING.md: priority matrix |
| Form | `src/tray.rs::bind_picked_windows` | README.md | Both pickers share group creation and current sync settings | TESTING.md: list and lasso |
| Scrollbar | Windows native listbox | Windows theme | Vertical scrolling with full native accessibility text | TESTING.md: long titles, DPI |

Selection is temporary until Bind Selected. At least two windows are required. Refresh preserves available selected handles; stale selection is shown inline and must be reviewed before retrying. Cancel discards the pending selection. Buttons in both dialogs are real native controls with tab focus, hover, pressed and disabled states. The picker is a desktop selection window, not an app-wide modal: unrelated application windows remain accessible. Hotkeys are suspended while it is open to avoid starting another bind flow.

Recover Off-screen Windows moves only normal windows whose title-bar grip is outside a monitor work area. Minimized and maximized windows are skipped. Unbinding also recovers the affected group; synchronized follower movement preserves a reachable title bar.

Move Together is a global persisted preference, on by default. Groups default to Follow global; explicit On/Off choices override the global value. All platforms resolve this via GroupManager::movement_enabled. Group membership and overrides are session-only, consistent with existing group lifecycle. Activation and minimize/restore remain unchanged. The current create operation uses the stored `sync_minimize` preference too. All new owned labels are in `src/i18n.rs`.

## Do's and Don'ts

- Keep individual window identity even when several windows share a process or title.
- Preserve native keyboard navigation, selection and disabled-state rendering.
- Keep exact-window selection separate from process-name matching.
- Do not claim runtime or visual verification from static checks. See TESTING.md for pending acceptance tests.

The shortcut page has four rows, including Toggle Movement (Ctrl+Alt+M by default). Its shortcut and tray checkbox use the same persisted global action; explicit group overrides are preserved. Restoring defaults resets all four fields; Cancel discards edits. Unsupported or duplicate combinations stay in the dialog with an explanatory message. Existing settings gain the new default without losing preferences.


The overlay supports click-to-toggle and additive rectangles without modifier keys, preserving selection across monitors. Release does not commit. Enter or Bind confirms at least two live windows; Escape or right-click cancels. Initially a captionless 560×56 logical pixel tip bar sits above the initiating monitor’s taskbar with brief click/drag instructions. After a committed selection reaches two windows, it expands upward to 560×108, exposing movement choice and Bind / Reselect / Cancel. Dropping below two collapses it; the mode choice is retained. Drag its hint area to move it aside. No window-title text is drawn on the desktop or repeated in the tip bar. Dropdown Enter/Escape keep native behavior while expanded. Stale selected windows are removed with a message and require a second confirmation. The list remains the keyboard-first alternative for covered/minimized windows. The picker reserves a footer row for movement choice; its minimum height is 440 logical pixels including its frame.

Tray group submenus identify groups by window titles and show effective movement. Each submenu offers mutually exclusive Follow global / On / Off choices. Switching the global shortcut or menu never replaces a group override.

Hotkey registration errors never open a modal dialog or stop startup. Each shortcut registers independently; unavailable combinations are marked beside the relevant tray action and in the Hotkey Settings entry. Menu actions remain available. Closing settings or a picker refreshes availability after registration.

The desktop overlay uses a persistent compatible GDI bitmap and one BitBlt per paint to avoid exposing intermediate clears. Idle pointer motion never invalidates the overlay. Drag motion redraws the overlay only; tip text and disclosure layout change only with committed selection changes. Buffer resources are released on exit. Tip corners use an 8 logical pixel radius, consistent with the shared compact-control style.
