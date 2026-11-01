# keytools-rs (baseline)

Windows-first keyboard utility suite: a Rust engine plus optional AHK v2 /
keysharp glue for the GoldenDict tools. Windows is the full-parity target;
Linux means X11 only (see `DECISIONS.md`).

> **Superseded (2026-11-01):** the five remaining tools move to
> [`../keyboard-tools-rs/`](../keyboard-tools-rs/). This repo stays buildable
> as the reference baseline until that workspace imports the code.

## Map

- `crates/kit/` — shared Rust library: config, hotkeys, tray, Win32, Linux stubs
- `apps/` — one binary per tool: keyboard locker, keyboard layout switch,
  layout auto switch-back, GoldenDict search-in-group and auto-search
- `scripts/` — future AHK v2 / keysharp glue, outside the Cargo workspace
- `DECISIONS.md` — scope, layering, tool buckets, keysharp spike gates
- `PLAN.md` — baseline implementation phases and per-tool design

## Moved out

- monitor power (`toggleScreen`) — [`../desktop-tools-rs/`](../desktop-tools-rs/)
- screen OCR (`dt-ocr`) — [`../desktop-tools-rs-ex/`](../desktop-tools-rs-ex/)

Authors: Kimi-K2.7-Code🧙‍♂️, scillidan🤡
