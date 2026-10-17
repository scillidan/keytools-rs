//! togglescreen: press the hotkey (default Win+F2) to turn the monitor off.
//! Any input wakes it — there is no "on" action, matching the AHK original.

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        // GUI app: errors must be visible, not just stderr.
        if let Err(e) = run() {
            kit::win::message_box("togglescreen", &format!("Error:\n{e:#}"));
            return Err(e);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("togglescreen")
    }
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::keys::Combo;
    use kit::tray::{Event, Tray};
    use kit::{config, hotkey::Hotkeys, win};
    use serde::Deserialize;

    const APP: &str = "togglescreen";

    let _single = win::ensure_single_instance(APP)?;

    #[derive(Deserialize)]
    struct General {
        hotkey: Option<String>,
    }
    #[derive(Deserialize, Default)]
    struct FileConfig {
        general: Option<General>,
    }

    let config_path = config::exe_dir_file("togglescreen.toml")?;
    let text = config::load_or_create(&config_path, include_str!("../togglescreen.toml"))?;
    let file: FileConfig = toml::from_str(&text)?;
    let combo: Combo = file
        .general
        .and_then(|g| g.hotkey)
        .unwrap_or_else(|| "win+f2".into())
        .parse()?;

    let hotkeys = Hotkeys::register(&[("toggle", combo)])?;

    let mut tray = Tray::new(
        APP,
        kit::icon::letter_rgba('t', [0x3b, 0x6e, 0xa5, 0xff], [0x23, 0x46, 0x6e, 0xff]),
    )?;
    refresh_menu(&mut tray);

    tray.run(Some(&hotkeys), |tray, event| match event {
        Event::Hotkey(label) if label == "toggle" => monitor_off(),
        Event::Menu(label) if label == "Start with Windows" => {
            let now = !win::autostart_enabled(APP);
            if let Err(e) = win::set_autostart(APP, now) {
                win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
            }
            refresh_menu(tray);
        }
        Event::Menu(label) if label == "Exit" => tray.quit(),
        _ => {}
    });
    Ok(())
}

#[cfg(windows)]
fn refresh_menu(tray: &mut kit::tray::Tray) {
    use kit::tray::Entry;
    tray.set_menu(&[
        Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("togglescreen"),
        ),
        Entry::Sep,
        Entry::Item("Exit"),
    ]);
}

/// Post `SC_MONITORPOWER` to all top-level windows (like the AHK script).
#[cfg(windows)]
fn monitor_off() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, HWND_BROADCAST, WM_SYSCOMMAND};
    const SC_MONITORPOWER: usize = 0xF170;
    unsafe {
        let _ = PostMessageW(
            Some(HWND_BROADCAST),
            WM_SYSCOMMAND,
            WPARAM(SC_MONITORPOWER),
            LPARAM(2),
        );
    }
}
