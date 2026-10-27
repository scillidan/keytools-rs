# Decisions

Suite direction, scope, and admission rules for the keytools-rs desktop
utility suite. This file is the source of truth for **why** and **what**;
`PLAN.md` keeps the implementation phases, and `AGENTS.md` keeps the working
rules.

- Status: accepted
- Date: 2026-10-25
- Review: 2027-01-31, then quarterly

## Scope

The suite is a Windows-first set of small desktop utilities. The engine is
Rust; optional AHK v2 / keysharp scripts handle orchestration-only tools.
Windows is the full-parity target. Linux means X11 only. Wayland and macOS are
explicitly out of scope for now; revisit at review time.

Out of scope: a whole-suite keysharp rewrite, a single monolithic app, and
cross-platform promises for Windows-only integrations (file dialogs, rbtray
window subclassing, keyboard-layout APIs).

## Decision records

### D1 - Rust engine, optional glue scripts

**Status:** accepted

**Context:** The tools split into low-level Win32 work (window geometry, input
hooks, tray, keyboard layouts) and orchestration of external programs
(GoldenDict, Flameshot, RapidOCR). A single language is a poor fit for both.
keysharp is a cross-platform AHK v2 implementation, but it explicitly does not
support DllCall, COM, or the registry on Linux/macOS.

**Decision:** Rust owns the engine (target architecture: `kt-platform`,
`kt-core`, `kt` CLI). AHK v2 or keysharp may be used only for glue tools. On
Windows-only glue, prefer native AHK v2 over keysharp. Do not rewrite the
engine in keysharp.

**Consequences:** Two runtimes may coexist, connected by the `kt` CLI/socket.
Glue tools stay small and replaceable; the engine keeps a single, tested
low-level implementation. Distribution must handle the script runtime
separately (`.ahk` or `.cks` plus .NET 10).

### D2 - Platform scope

**Status:** accepted

**Context:** Win32 APIs are the source of most behavior. Linux support is
fragmented across X11 and Wayland; Wayland does not allow clients to move or
inspect foreign windows by design. macOS needs a separate accessibility
backend.

**Decision:** Windows is the full-parity target. Linux targets X11 only.
Wayland and macOS are out of scope until a quarterly review reopens them.

**Consequences:** No Wayland stubs or "it might work" promises. Linux docs and
packaging cover the X11 subset only. macOS work, if it ever happens, is a new
decision with its own backends.

### D3 - Tool buckets

**Status:** accepted

**Context:** Not every experiment deserves product-level maintenance. Treating
all 12 tools as one product is why the direction keeps oscillating.

**Decision:** Every tool is `product`, `experiment`, or `drop` (see the table
below). Only `product` tools are eligible for new suite release manifests and
parity guarantees. `experiment` tools get a timeboxed spike and no
cross-platform promise.

Definitions:

- `product`: in the release scope; parity-tested before release, release
  manifest, maintained.
- `experiment`: timeboxed spike or prototype; no release promise.
- `drop`: not doing now; needs a new decision entry to return.

**Consequences:** Capacity goes to the shipped Windows suite first. Experiments
can be abandoned without touching the engine. A tool can be promoted only by
editing this table. Experiments may keep their existing standalone manifests
until they are promoted or dropped.

### D4 - keysharp spike gate

**Status:** accepted (spike not yet run)

**Context:** keysharp could reduce glue-code effort and provide Linux/macOS
backends, but it is alpha, compiles scripts through Roslyn, and lacks
DllCall/COM/registry off Windows. The benefit must be measured, not assumed.

**Decision:** Any keysharp spike is limited to 3 days and to a glue tool.
Order: FlameshotWindow, GoldenDict SearchInGroup, GoldenDict AutoSearch,
ScreenOCR. It must pass all gates:

- cold start under 2 s; warm start (compile daemon) under 300 ms;
- RSS under 150 MB for the script process;
- hotkey-to-action p95 under 50 ms;
- WezTerm and QuteBrowser move within 1 px of the requested rect;
- Linux X11: hotkey, tray, and window move work without root (inputd
  permissions documented separately).

If any gate fails, stop and keep the Rust implementation. No partial adoption.

**Consequences:** The default remains Rust. A failed spike is a valid result
and closes the question for the review period.

### D5 - Config and IPC

**Status:** accepted

**Context:** Per-tool INI/TOML files and direct Win32 calls from scripts make
behavior hard to share and test.

**Decision:** One user-facing TOML config for the suite. The `kt` CLI (and,
later, a local socket) is the stable contract between the engine and glue
scripts. Glue scripts do not call DllCall, COM, or the registry; they call
`kt` or external programs.

**Consequences:** The engine resolves config and passes values to scripts via
arguments/environment. Scripts remain portable and testable against a fake
`kt`. CLI compatibility is versioned with the suite. Existing per-tool configs
migrate only when a tool is promoted; experiments may keep their own format
until then.

### D6 - New-tool admission

**Status:** accepted

**Context:** Adding a repo per idea fragments maintenance and makes the suite
look larger than it is.

**Decision:** A new tool starts with a decision entry here (purpose, layer,
target OS, bucket, owner) before any repository or code. Windows-only complex
integrations are not promised cross-platform. One new tool is evaluated at a
time.

**Consequences:** The suite stays small and intentional. Ideas that do not
justify a decision entry stay out.

## Tool table

| Tool | Source | Layer | Target OS | Status | Next |
|---|---|---|---|---|---|
| GoldenDict SearchInGroup | `../AHK-GoldenDictSearchInGroup/` | glue | Windows + Linux X11 | experiment | keep Rust baseline; keysharp candidate #2 |
| GoldenDict AutoSearch | `../AHK-GoldenDictAutoSearch/` | glue | Windows + Linux X11 | experiment | keep Rust baseline; keysharp candidate #3 |
| Keyboard AutoSwitchBack | `../AHK-KeyboardAutoSwitchBack/` | engine | Windows | product | ship with Windows suite |
| Keyboard Switch | `../AHK-KeyboardSwitch/` | engine | Windows | product | ship with Windows suite |
| ScreenOCR | `../AHK-ScreenOCR/` | glue | Windows + Linux X11 | experiment | keep Rust baseline; keysharp candidate #4 |
| FlameshotWindow | `../AHK-FlameshotWindow/` | glue | Windows + Linux X11 | experiment | keysharp candidate #1 |
| RectangleWin / rectangle-rs | `ahmetb/RectangleWin`, `../rectangle-rs/` | engine | Windows + Linux X11 | product | P1 interaction polish |
| QuickSwitch | `fenchai23/QuickSwitch` (active: `JoyHak/QuickSwitch`) | separate app | Windows | experiment | separate Windows-only product after P2; no cross-platform |
| window-switcher | `sigoden/window-switcher` | separate app | Windows | experiment | separate product after P2 |
| ahk-keyboard-locker | `sophice/ahk-keyboard-locker` | engine | Windows (+ Linux X11 later) | product | Windows release; Linux X11 later |
| rbtray | `benbuck/rbtray` | separate app | Windows | experiment | defer; fragile, Windows-only |
| toggle-screen | `ArturTkaczuk/toggle-screen-autohotkey` | engine | Windows + Linux X11 | product | ship with Windows suite |

`product` tools are the only ones with suite release manifests. `experiment`
tools keep their current Rust baseline and existing standalone manifests until
a spike or a decision promotes them.

Dropped for now: cross-platform QuickSwitch, cross-platform rbtray, and all
macOS ports. These need a new decision entry to return.

## Open questions

- keysharp license: GitHub reports `Other / NOASSERTION`; read the repository
  license before distributing a `.cks` or bundling the runtime.
- Linux inputd permissions and packaging for keysharp, if the spike reaches
  Linux.
- Whether to fold `rectangle-rs` into the same Cargo workspace or keep it a
  sibling repo with a shared `kt-platform` crate.
- Whether the suite needs a single tray hub or per-tool tray icons. Start with
  per-tool; revisit only if tray clutter becomes a real problem.
- AHK v2 versus keysharp for Windows-only glue: native AHK v2 is the default
  until the spike proves keysharp worth its runtime.

## Review

Review this file on 2027-01-31, then quarterly. Changes to scope, buckets, or
platform support require a dated edit here; `PLAN.md` and `AGENTS.md` link to
this file instead of restating it.
