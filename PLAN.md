# AHK → Rust cross-platform refactoring plan

Reference: `keymap-rs` (code style, Windows backend, tray, conventions).
Goal: reimplement 5 AHK v1 tools + keyboard-locker (third-party AHK) + the new
togglescreen tool as one Rust workspace. Full parity on Windows; real Linux
backends land incrementally.

Date: 2026-10-18

---

## 1. Decisions

| Question | Decision |
|---|---|
| Structure | Standalone cargo workspace + shared crate, one bin per tool |
| Linux | Full Windows parity first; Linux uses pragmatic xdotool/xclip/xset-based backends, Wayland gets a stub + adaptation checklist (same pattern as keymap-rs) |
| Config | TOML (serde/toml, same as keymap-rs), default config written on first run; no INI migration |
| togglescreen | New tool: hotkey toggles monitor power (e.g. Win+F2), published to scoop-pile |

## 2. Workspace layout

```
keytools-rs/
├── Cargo.toml                 # workspace
├── PLAN.md                    # this file
├── crates/
│   └── kit/                   # shared library (extracted/portable from keymap-rs)
│       ├── src/config.rs      # TOML first-run creation + merge (like keymap-rs config.rs)
│       ├── src/tray.rs        # ported keymap-rs tray.rs (tray-icon crate)
│       ├── src/autostart.rs   # HKCU ...\CurrentVersion\Run (like keymap-rs windows.rs::set_autostart)
│       ├── src/single.rs      # Windows: CreateMutex single instance; Linux: flock
│       └── src/hotkey.rs      # global hotkey abstraction: Win=RegisterHotKey, with double-press (400ms, like SearchInGroup)
└── src/bin/ or apps/
    ├── togglescreen/          # smallest, first
    ├── keyboard-locker/       # WH_KEYBOARD_LL swallowing (ported from keymap-rs platform/windows.rs hook)
    ├── goldendict-search-in-group/
    ├── goldendict-auto-search/
    ├── keyboard-auto-switch-back/
    ├── keyboard-switch/
    └── screen-ocr/
```

App names follow the AHK projects (kebab-case). Each app keeps its own
`assets/icon.ico`, `.justfile dist` (cargo build + 7z, like keymap-rs), MIT LICENSE.

### What kit reuses from keymap-rs

| keymap-rs piece | Reused by |
|---|---|
| `tray.rs` (tray-icon menu, 32x32 RGBA icons) | all apps |
| `windows.rs` WH_KEYBOARD_LL hook + LLKHF_INJECTED skip | keyboard-locker, goldendict-auto-search (double-click), togglescreen (wake hotkey) |
| `GetKeyboardLayout` + `GetWindowThreadProcessId` + `ToUnicodeEx` | keyboard-switch, keyboard-auto-switch-back |
| `keys.rs` combo parser (`ctrl+shift+c`) | every configured hotkey |
| `config.rs` TOML merge pattern | all apps |
| SendInput helpers | keyboard-locker (pass-through on unlock) |

Note: where RegisterHotKey is enough, no hook is used (simpler):
search-in-group and togglescreen hotkeys.

## 3. Per-tool design

### 3.1 togglescreen (new, ~100 lines)
- Hotkey (default `#f2`, TOML-configurable) -> monitor off:
  - Windows: `PostMessageW(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER, 2)`
  - Linux: `xset dpms force off` (X11); Wayland stub
- Any input wakes the screen; no "on" logic needed
- tray + autostart (provided by kit)

### 3.2 keyboard-locker (replaces sophice/ahk-keyboard-locker, GPL-3.0 -> clean-room MIT rewrite)
- Global hotkey (default `^!+Insert`) toggles lock; while locked WH_KEYBOARD_LL swallows all keyboard events
- tray icon switches locked/unlocked (kit tray supports icon swap)
- config settings.toml: `shortcut`, `shortcut-hint`
- Linux: rdev grab mode (see keymap-rs TODO.md)

### 3.3 goldendict-search-in-group (simple, first batch)
- Config `Group_1..10` + hotkeys (kit combo parser)
- Double-press hotkeys (`xx` syntax, 400 ms, direct port of the A_TickCount logic)
- Read clipboard (arboard, like keymap-rs clipboard.rs) -> `Run goldendict --group-name="<g>" "<query>"`
- Message box on empty clipboard
- Pure RegisterHotKey, no cross-platform obstacles (on Linux only the goldendict path differs)

### 3.4 goldendict-auto-search
- `Ctrl+Alt+Shift+G` binds/unbinds the window under the cursor: `GetCursorPos` -> `WindowFromPoint` ->
  `GetWindowThreadProcessId` + `QueryFullProcessImageNameW` for the process name
- `~LButton Up` double-click detection: WH_MOUSE_LL (or GetAsyncKeyState polling, equivalent to AHK) +
  300 ms interval check -> save/restore ClipboardAll + ClipGuard 1500 ms self-trigger suppression + `SendInput ^c`
  (keymap-rs already has inject_ctrl_c) -> `Run goldendict --group-name/--popup-group-name`
- Clipboard monitor: `AddClipboardFormatListener` (better than AHK's 500 ms polling), fires only while a bound window is foreground
- Self-trigger guard: record the timestamp of our own clipboard writes (direct port of AHK ClipGuardTick)
- Linux: `xdotool` for the window-under-cursor process + `xclip -selection clipboard -o` polling monitor

### 3.5 keyboard-auto-switch-back
- `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` replaces the 200 ms SetTimer (timelier, cheaper CPU)
- 600 ms (configurable SwitchDelay) after a foreground change, check `GetKeyboardLayout(GetWindowThreadProcessId(hwnd))`;
  if not the target layout, `PostMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST=0x50, 0, targetLayout)`
- Layout display name: read `HKLM\...\Keyboard Layouts\<KLID>` value "Layout Text" (windows-registry crate)
- Linux: fcitx5/ibus dbus (stub + checklist first)

### 3.6 keyboard-switch
- Default `CapsLock` (`$*` suppresses native behavior -> RegisterHotKey satisfies this naturally)
- InAll mode: enumerate HKLs with `GetKeyboardLayoutList`, cycle to the next, `PostMessageW(WM_INPUTLANGCHANGEREQUEST)`
- InDefine mode: `SendInput Alt+Shift+N` sequence (kit SendInput helpers)
- Centered tooltip with the layout name after switching: direct port of the GDI `GetTextExtentPoint32W` centering
  (or fall back to a tray notification if parity issues surface)
- Optional ESC<->CapsLock swap: register one more hotkey
- `tests/CheckLayout.ahk` -> Rust integration test or check script
- Linux: `setxkbmap -query` / layout cycling (stub + checklist)

### 3.7 screen-ocr (heaviest, last)
- Keep the Python RapidOCR path (`uv run lib/rapidocr_cli.py`, PEP-723, already cross-platform)
- Region selection: default to Flameshot (available on Windows/Linux); builtin GDI+ selector as fallback
- Multi-language hotkey table `[RapidOCRLangHotkey]` (chinese_cht etc.) -> TOML table
- Language list cache `cache/rapidocr_lang.txt`, direct port
- snoretoast notification optional

## 4. Phases (each with verifiable criteria)

1. **Scaffold + kit** -> verify: `cargo build` passes; a demo bin shows the tray
   menu, autostart toggle, single-instance mutex, TOML default config — **DONE
   (togglescreen doubled as the demo)**
2. **Easy trio**: togglescreen, keyboard-locker, goldendict-search-in-group ->
   verify: per-item manual comparison against the AHK versions (hotkey fires,
   double-press, lock swallowing, monitor off/wake); parsing logic (hotkey
   strings, double-press interval) has unit tests — **DONE (build + unit tests +
   smoke tests pass; item-by-item comparison pending hands-on verification)**
3. **GoldenDict auto search + the two keyboard-layout tools** -> verify: bind
   window -> double-click word -> GoldenDict pops up; foreground switch -> layout
   falls back after 600 ms; CapsLock cycles layouts with a centered tooltip;
   side-by-side run against the AHK versions — **DONE (build + unit tests +
   smoke tests pass; item-by-item comparison pending hands-on verification)**
4. **screen-ocr** -> verify: same region gives the same OCR result as the AHK
   version — **DONE (build + smoke tests pass; the builtin selector is a new
   Win32 implementation, OCR comparison pending hands-on verification)**
5. **Linux backends** (xdotool/xclip/xset/rdev) -> verify: every tool compiles
   for Linux (`cargo check --target x86_64-unknown-linux-gnu`) + real X11
   implementation + Wayland stub
6. **Release**: add 7 manifests to scoop-pile (persist configs,
   add/remove-startup), replacing the existing AHK manifests; versions,
   checkver/autoupdate included -> verify: `scoop install pile/<app>` works on a
   clean machine

## 5. Conventions (inherited from keymap-rs)

- anyhow + context; unwrap only for locks/tests
- inline `#[cfg(test)]` tests mirroring AHK behavior
- TOML config, default written on first run (`include_str!` embedded default)
- Windows-only deps in target-specific dependencies (Cargo.toml like keymap-rs)
- comments note AHK-parity decisions (same style as keymap-rs)
- one AGENTS.md at the workspace root for shared conventions, not per app

## 6. Risks and open questions

- **keyboard-switch centered tooltip**: direct GDI measuring port vs tray
  notification — confirm during implementation
- **keyboard-locker GPL**: the sophice original is GPL-3.0; we rewrite from
  scratch (MIT) without copying code, and the manifest no longer links the
  original repo
- **GoldenDict popup-group vs group**: depends on GoldenDict behavior; confirm
  CLI flags during parity testing
- **Wayland**: xdotool/xset are both ineffective there; uniform stub +
  adaptation checklist (keymap-rs pattern), no Wayland promises
- **ScreenOCR builtin selection**: the GDI+ overlay is a lot of work; phase 1
  only guarantees the Flameshot path
