# keytools-rs

Windows-first desktop utility suite: a Rust engine plus optional AHK v2 /
keysharp glue for orchestration tools. Windows is the full-parity target;
Linux means X11 only (see `DECISIONS.md`).

## Map

- `crates/kit/` — shared Rust library: config, hotkeys, tray, Win32, Linux stubs
- `apps/` — one binary per tool (monitor toggle, keyboard locker, GoldenDict, OCR)
- `scripts/` — future AHK v2 / keysharp glue, outside the Cargo workspace
- `DECISIONS.md` — scope, layering, tool buckets, keysharp spike gates
- `PLAN.md` — implementation phases and per-tool design
- `../rectangle-rs/` — separate repo for window snapping (P1 focus)

Authors: Kimi-K2.7-Code🧙‍♂️, scillidan🤡
