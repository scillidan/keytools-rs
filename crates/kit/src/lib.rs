//! Shared scaffolding for the keytools-rs apps, ported from keymap-rs.
//!
//! - [`keys`]: key/combo parsing (`ctrl+shift+c`, `win+f2`) shared by config files
//! - [`config`]: TOML config created next to the exe on first run
//! - [`icon`]: generated 32x32 RGBA tray icons
//! - Windows only (phase 1): [`tray`], [`hotkey`], [`win`]
//! - Linux: stubs with an adaptation checklist (see PLAN.md phase 5)

pub mod config;
pub mod icon;
pub mod keys;

#[cfg(windows)]
pub mod hotkey;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod win;

#[cfg(target_os = "linux")]
pub mod linux;

/// What to do when a platform feature is not implemented yet.
#[cfg(target_os = "linux")]
pub fn unimplemented(feature: &str) -> ! {
    linux::stub(feature)
}
