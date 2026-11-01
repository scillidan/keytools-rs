# keytools-rs (baseline)

Windows-first keyboard utility suite: Rust engine for low-level tools, optional
AHK v2 / keysharp glue for orchestration tools. No whole-suite keysharp
rewrite. Windows is the full-parity target; Linux means X11 only (see
`DECISIONS.md`).

> **Superseded (2026-11-01):** the five remaining tools moved to
> [`../keyboard-tools-rs/`](../keyboard-tools-rs/); P0 imported them. This repo
> is the historical baseline.
>
> **Do not build here.** Both repos share
> `CARGO_TARGET_DIR=C:\Users\User\.cache\cargo`, so an old build overwrites
> `keyboard-locker.exe`, `keyboard-switch.exe`, etc. with pre-fix code. Build
> and run from `../keyboard-tools-rs`.

Reimplements (see PLAN.md for the full mapping):

- the four `AHK-*` projects under `C:\Users\User\Share\projs` (GoldenDict auto
  search / search-in-group, keyboard auto-switch-back / switch)
- `keyboard-locker` (clean-room MIT rewrite of sophice/ahk-keyboard-locker)

Monitor power and screen OCR were moved out to `../desktop-tools-rs` and
`../desktop-tools-rs-ex`; do not add them back here.

## Scope

[`DECISIONS.md`](DECISIONS.md) is the source of truth for direction and scope.

- Rust is the engine: `kit` + one bin per tool. Do not port the suite to
  keysharp.
- Windows first. Linux means X11 only; Wayland and macOS are out of scope.
- keysharp is only a glue experiment for orchestration tools (GoldenDict),
  never for low-level window/input work. Native AHK v2 is the default for
  Windows-only glue.
- A new tool starts with a decision entry in `DECISIONS.md`, not with code.
- Glue scripts, if any, live outside this Cargo workspace.

Conventions inherited from keymap-rs: anyhow error handling, TOML config created
next to the exe on first run (`include_str!` default), `>` tray-menu prefix for
active toggles, inline `#[cfg(test)]` tests, `just dist` packaging.

## Status

- [x] Phase 1: workspace + `kit` crate (config, keys, tray, hotkeys, win32 helpers)
- [x] Phase 2: `keyboard-locker`, `goldendict-search-in-group` (built + smoke-tested)
- [x] Phase 3: `goldendict-auto-search`, `keyboard-auto-switch-back`, `keyboard-switch` (built + smoke-tested)
- [ ] Phase 4: Linux X11 backends (kit::linux has stubs + checklist); Wayland out of scope
- [ ] Phase 5: scoop-pile manifests (product tools first)
- [x] Moved out: `togglescreen` -> `../desktop-tools-rs`, `screen-ocr` -> `../desktop-tools-rs-ex`

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
