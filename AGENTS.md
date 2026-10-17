# keytools-rs

Rust reimplementation of the AHK- tool suite, with cross-platform structure.
Rust successors of the AHK-* tools; no AutoHotkey required. Full parity on
Windows; Linux backends land per PLAN.md phase 5.

Reimplements (see PLAN.md for the full mapping):

- `togglescreen` — turn the monitor off with a hotkey (new; replaces the idea of
  `scoop install pile/togglescreen`)
- the five `AHK-*` projects under `C:\Users\User\Share\projs` (GoldenDict auto
  search / search-in-group, keyboard auto-switch-back / switch, screen OCR)
- `keyboard-locker` (clean-room MIT rewrite of sophice/ahk-keyboard-locker)

Conventions inherited from keymap-rs: anyhow error handling, TOML config created
next to the exe on first run (`include_str!` default), `>` tray-menu prefix for
active toggles, inline `#[cfg(test)]` tests, `just dist` packaging.

## Status

- [x] Phase 1: workspace + `kit` crate (config, keys, tray, hotkeys, win32 helpers)
- [x] Phase 2: `togglescreen`, `keyboard-locker`, `goldendict-search-in-group` (built + smoke-tested)
- [x] Phase 3: `goldendict-auto-search`, `keyboard-auto-switch-back`, `keyboard-switch` (built + smoke-tested)
- [x] Phase 4: `screen-ocr` (built + smoke-tested; OCR comparison pending hands-on verification)
- [ ] Phase 5: Linux backends (kit::linux has stubs + checklist)
- [ ] Phase 6: scoop-pile manifests

## Commands

```sh
cargo build --release    # build all apps
cargo test               # unit tests
cargo clippy --all-targets
```

## Structure

```
crates/kit/     shared library (ported from keymap-rs)
  src/config.rs   TOML config: first-run create next to exe, then load
  src/keys.rs     Key/Combo + `ctrl+shift+c`-style parser (unit-tested)
  src/icon.rs     generated 32x32 RGBA keycap icons
  src/hotkey.rs   [windows] RegisterHotKey wrapper (label -> combo)
  src/tray.rs     [windows] generic tray menu + message loop dispatching
                  menu clicks AND hotkeys to one app closure
  src/win.rs      [windows] single instance, HKCU Run autostart, message boxes
  src/linux.rs    [linux] stubs with adaptation checklist
apps/<name>/    one bin per tool; <name>.toml is the embedded default config
```
