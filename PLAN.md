# AHK → Rust cross-platform refactoring plan

> **Superseded (2026-11-01):** the five keyboard tools move to
> [`../keyboard-tools-rs/PLAN.md`](../keyboard-tools-rs/PLAN.md). This file
> stays as the baseline record until that workspace imports the code.
> `togglescreen` now lives in `../desktop-tools-rs` (`toggleScreen`);
> `screen-ocr` now lives in `../desktop-tools-rs-ex` (`dt-ocr`).

> **Direction and scope:** see [`DECISIONS.md`](DECISIONS.md) (2026-10-25).
> PLAN.md keeps implementation phases; DECISIONS.md wins on scope conflicts.

Reference: `keymap-rs` (code style, Windows backend, tray, conventions).
Goal: a Windows-first utility suite. Rust is the engine for the low-level
tools; the existing Rust ports of orchestration tools remain the baseline
until a keysharp spike proves a better glue layer (DECISIONS.md D3–D4). Full
parity on Windows; Linux targets X11 only, and Wayland is out of scope.

Date: 2026-10-18 (scope revised 2026-10-25 and 2026-11-01)

---

## 1. Decisions

| Question | Decision |
|---|---|
| Structure | Standalone cargo workspace + shared crate, one bin per tool |
| Linux | Windows parity first; Linux targets X11 only (pragmatic xdotool/xclip/xset-based backends). Wayland is out of scope (DECISIONS.md D2). |
| Config | TOML (serde/toml, same as keymap-rs), default config written on first run; no INI migration |
| Removed tools | togglescreen moved to `../desktop-tools-rs`; screen-ocr moved to `../desktop-tools-rs-ex` |

## 2. Workspace layout

```
keytools-rs/
├── Cargo.toml                 # workspace
├── DECISIONS.md               # suite scope, tool buckets, keysharp gate
├── PLAN.md                    # this file
├── crates/
│   └── kit/                   # shared library (extracted/portable from keymap-rs)
│       ├── src/config.rs      # TOML first-run creation + merge (like keymap-rs config.rs)
│       ├── src/tray.rs        # ported keymap-rs tray.rs (tray-icon crate)
│       ├── src/autostart.rs   # HKCU ...\CurrentVersion\Run (like keymap-rs windows.rs::set_autostart)
│       ├── src/single.rs      # Windows: CreateMutex single instance; Linux: flock
│       └── src/hotkey.rs      # global hotkey abstraction: Win=RegisterHotKey, with double-press (400ms, like SearchInGroup)
└── apps/
    ├── keyboard-locker/       # WH_KEYBOARD_LL swallowing (ported from keymap-rs platform/windows.rs hook)
    ├── goldendict-search-in-group/
    ├── goldendict-auto-search/
    ├── keyboard-auto-switch-back/
    └── keyboard-switch/
```

App names follow the AHK projects (kebab-case). Each app keeps its own
`assets/icon.ico`, `.justfile dist` (cargo build + 7z, like keymap-rs), MIT LICENSE.

Glue scripts (AHK v2 / keysharp), if any, live outside this Cargo workspace
(see DECISIONS.md D1).

### What kit reuses from keymap-rs

| keymap-rs piece | Reused by |
|---|---|
| `tray.rs` (tray-icon menu, 32x32 RGBA icons) | all apps |
| `windows.rs` WH_KEYBOARD_LL hook + LLKHF_INJECTED skip | keyboard-locker, goldendict-auto-search (double-click) |
| `GetKeyboardLayout` + `GetWindowThreadProcessId` + `ToUnicodeEx` | keyboard-switch, keyboard-auto-switch-back |
| `keys.rs` combo parser (`ctrl+shift+c`) | every configured hotkey |
| `config.rs` TOML merge pattern | all apps |
| SendInput helpers | keyboard-locker (pass-through on unlock) |

Note: where RegisterHotKey is enough, no hook is used (simpler):
search-in-group hotkeys.

## 3. Per-tool design

The designs below describe the current Rust baseline. Glue tools may move to
AHK v2 / keysharp if the spike in DECISIONS.md D4 passes; the Rust engine
remains.

### 3.1 keyboard-locker (replaces sophice/ahk-keyboard-locker, GPL-3.0 -> clean-room MIT rewrite)
- Global hotkey (default `^!+Insert`) toggles lock; while locked WH_KEYBOARD_LL swallows all keyboard events
- tray icon switches locked/unlocked (kit tray supports icon swap)
- config settings.toml: `shortcut`, `shortcut-hint`
- Linux: rdev grab mode (X11 only; see keymap-rs TODO.md)

### 3.2 goldendict-search-in-group (simple, first batch)
- Config `Group_1..10` + hotkeys (kit combo parser)
- Double-press hotkeys (`xx` syntax, 400 ms, direct port of the A_TickCount logic)
- Read clipboard (arboard, like keymap-rs clipboard.rs) -> `Run goldendict --group-name="<g>" "<query>"`
- Message box on empty clipboard
- Pure RegisterHotKey, no cross-platform obstacles (on Linux X11 only the goldendict path differs)

### 3.3 goldendict-auto-search
- `Ctrl+Alt+Shift+G` binds/unbinds the window under the cursor: `GetCursorPos` -> `WindowFromPoint` ->
  `GetWindowThreadProcessId` + `QueryFullProcessImageNameW` for the process name
- `~LButton Up` double-click detection: WH_MOUSE_LL (or GetAsyncKeyState polling, equivalent to AHK) +
  300 ms interval check -> save/restore ClipboardAll + ClipGuard 1500 ms self-trigger suppression + `SendInput ^c`
  (keymap-rs already has inject_ctrl_c) -> `Run goldendict --group-name/--popup-group-name`
- Clipboard monitor: `AddClipboardFormatListener` (better than AHK's 500 ms polling), fires only while a bound window is foreground
- Self-trigger guard: record the timestamp of our own clipboard writes (direct port of AHK ClipGuardTick)
- Linux X11: `xdotool` for the window-under-cursor process + `xclip -selection clipboard -o` polling monitor

### 3.4 keyboard-auto-switch-back
- `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` replaces the 200 ms SetTimer (timelier, cheaper CPU)
- 600 ms (configurable SwitchDelay) after a foreground change, check `GetKeyboardLayout(GetWindowThreadProcessId(hwnd))`;
  if not the target layout, `PostMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST=0x50, 0, targetLayout)`
- Layout display name: read `HKLM\...\Keyboard Layouts\<KLID>` value "Layout Text" (windows-registry crate)
- Linux: not planned (Windows-only layout API); revisit only with a per-DE
  backend decision

### 3.5 keyboard-switch
- Default `CapsLock` (`$*` suppresses native behavior -> RegisterHotKey satisfies this naturally)
- InAll mode: enumerate HKLs with `GetKeyboardLayoutList`, cycle to the next, `PostMessageW(WM_INPUTLANGCHANGEREQUEST)`
- InDefine mode: `SendInput Alt+Shift+N` sequence (kit SendInput helpers)
- Centered tooltip with the layout name after switching: direct port of the GDI `GetTextExtentPoint32W` centering
  (or fall back to a tray notification if parity issues surface)
- Optional ESC<->CapsLock swap: register one more hotkey
- `tests/CheckLayout.ahk` -> Rust integration test or check script
- Linux: not planned (Windows-only layout API); revisit only with a per-DE
  backend decision

## 4. Phases (each with verifiable criteria)

1. **Scaffold + kit** -> verify: `cargo build` passes; a demo bin shows the tray
   menu, autostart toggle, single-instance mutex, TOML default config — **DONE
   (the first app doubled as the demo)**
2. **Easy pair**: keyboard-locker, goldendict-search-in-group -> verify:
   per-item manual comparison against the AHK versions (hotkey fires,
   double-press, lock swallowing); parsing logic (hotkey strings, double-press
   interval) has unit tests — **DONE (build + unit tests + smoke tests pass;
   item-by-item comparison pending hands-on verification)**
3. **GoldenDict auto search + the two keyboard-layout tools** -> verify: bind
   window -> double-click word -> GoldenDict pops up; foreground switch -> layout
   falls back after 600 ms; CapsLock cycles layouts with a centered tooltip;
   side-by-side run against the AHK versions — **DONE (build + unit tests +
   smoke tests pass; item-by-item comparison pending hands-on verification)**
4. **Linux X11 subset** (GoldenDict tools, keyboard-locker; xdotool/xclip/rdev)
   -> verify: the X11 subset compiles for Linux
   (`cargo check --target x86_64-unknown-linux-gnu`) + real X11
   implementations. Wayland is out of scope; keyboard-layout tools stay
   Windows-only.
5. **Release**: add/replace scoop-pile manifests for the product tools first
   (keyboard-locker, keyboard-switch, keyboard-auto-switch-back; persist
   configs, add/remove-startup); versions, checkver/autoupdate included ->
   verify: `scoop install pile/<app>` works on a clean machine. Windows suite
   ships first; Linux packages cover the X11 subset only. The two GoldenDict
   tools keep their existing AHK manifests until promoted.

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
- **Wayland**: out of scope (DECISIONS.md D2); xdotool/xset are ineffective
  there and no stub is promised
- **Moved tools**: togglescreen and screen-ocr are maintained in
  `../desktop-tools-rs` and `../desktop-tools-rs-ex`; do not reintroduce them
  here.
