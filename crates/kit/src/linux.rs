//! Linux stubs. Real backends land in phase 5 (see PLAN.md): xdotool/xclip/xset
//! for the GoldenDict and screen tools, rdev for key capture, flock for single
//! instance. Wayland is out of scope (same decision as keymap-rs).

/// Print the adaptation checklist for `feature` and exit.
pub fn stub(feature: &str) -> ! {
    eprintln!(
        "{feature}: Linux backend not implemented yet.\n\
         Adaptation checklist:\n\
         - single instance: flock on a pidfile in $XDG_RUNTIME_DIR\n\
         - global hotkeys / key capture: rdev (X11 grab; Wayland needs compositor cooperation)\n\
         - clipboard: xclip / wl-clipboard\n\
         - window under cursor / foreground window: xdotool (X11)\n\
         - monitor off: xset dpms force off (X11)\n\
         - tray: tray-icon linux backend\n"
    );
    std::process::exit(1);
}
