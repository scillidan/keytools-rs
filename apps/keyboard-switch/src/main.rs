//! keyboard-switch: press the switch hotkey (default CapsLock) to cycle the
//! keyboard layout — through all installed layouts (InAll) or a configured
//! list applied via Alt+Shift+N (InDefine). Ported from AHK-KeyboardSwitch.
//!
//! Parity notes:
//! - AHK `$*%switchHotkey%` suppresses the native key; `RegisterHotKey`
//!   consumes the registered combo the same way.
//! - The centered switch tooltip (AHK ToolTip + GDI text measuring) is
//!   `kit::win::center_tip`: same placement, no measuring needed.

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::message_box("keyboard-switch", &format!("Error:\n{e:#}"));
            return Err(e);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("keyboard-switch")
    }
}

#[cfg(windows)]
#[derive(Clone, serde::Deserialize)]
struct DefinedLayout {
    klid: String,
    shortcut: Option<String>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct Settings {
    switch_mode: Option<String>,
    switch_hotkey: Option<String>,
    swap_esc_caps: Option<bool>,
    notify_on_switch: Option<bool>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct FileConfig {
    settings: Option<Settings>,
    layout: Option<Vec<DefinedLayout>>,
}

/// Installed layouts, cached like the AHK `GetInstalledLayouts()` static.
#[cfg(windows)]
fn installed_layouts() -> Vec<isize> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayoutList, HKL};
    let count = unsafe { GetKeyboardLayoutList(None) };
    if count <= 0 {
        return Vec::new();
    }
    let mut buf = vec![HKL(std::ptr::null_mut()); count as usize];
    let got = unsafe { GetKeyboardLayoutList(Some(&mut buf)) };
    buf.truncate(got.max(0) as usize);
    buf.into_iter().map(|h| h.0 as isize).collect()
}

/// Current layout KLID (loword of the foreground thread's HKL, `{:08x}`).
#[cfg(windows)]
fn current_klid() -> Option<String> {
    let hwnd = kit::win::foreground_hwnd()?;
    let hkl = kit::win::layout_of_window(hwnd)?;
    Some(format!("{:08x}", (hkl as u32) & 0xFFFF))
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::keys::{Combo, Key};
    use kit::tray::{Event, Tray};
    use kit::{config, hotkey::Hotkeys, win};
    use std::time::Duration;

    const APP: &str = "keyboard-switch";

    let _single = win::ensure_single_instance(APP)?;

    let config_path = config::exe_dir_file("keyboard-switch.toml")?;
    let text = config::load_or_create(&config_path, include_str!("../keyboard-switch.toml"))?;
    let file: FileConfig = toml::from_str(&text)?;
    let settings = file.settings.unwrap_or(Settings {
        switch_mode: None,
        switch_hotkey: None,
        swap_esc_caps: None,
        notify_on_switch: None,
    });
    let mut in_define = settings.switch_mode.as_deref() == Some("InDefine");
    let switch_combo: Combo = settings
        .switch_hotkey
        .unwrap_or_else(|| "capslock".into())
        .parse()?;
    let mut swap_esc = settings.swap_esc_caps.unwrap_or(false);
    let mut notify = settings.notify_on_switch.unwrap_or(true);

    let is_capslock = switch_combo.key == Key::CapsLock
        && !switch_combo.ctrl
        && !switch_combo.alt
        && !switch_combo.shift
        && !switch_combo.win;

    // Hotkeys: the switch combo, plus Esc when the switch hotkey is CapsLock
    // (AHK: `Hotkey, $*Esc, HotkeyEsc`).
    let esc_combo = Combo {
        key: Key::Escape,
        ..Default::default()
    };
    let mut defs: Vec<(&str, Combo)> = vec![("switch", switch_combo)];
    if is_capslock {
        defs.push(("esc", esc_combo));
    }
    let hotkeys = Hotkeys::register(&defs)?;

    let mut defined: Vec<DefinedLayout> = file.layout.unwrap_or_default();
    if defined.is_empty() {
        // AHK defaults when [Layouts] is empty.
        defined = vec![
            DefinedLayout {
                klid: "00000409".into(),
                shortcut: Some("1".into()),
            },
            DefinedLayout {
                klid: "00000804".into(),
                shortcut: Some("2".into()),
            },
        ];
    }
    let mut installed: Vec<isize> = Vec::new();

    let mut tray = Tray::new(
        APP,
        kit::icon::letter_rgba('k', [0x3b, 0x6e, 0xa5, 0xff], [0x23, 0x46, 0x6e, 0xff]),
    )?;
    refresh_menu(
        &mut tray,
        swap_esc,
        in_define,
        notify,
        is_capslock,
        &switch_combo,
        &defined,
        &mut installed,
    );

    tray.run(Some(&hotkeys), |tray, event| {
        match event {
            Event::Hotkey(label) => {
                let esc_vk = kit::hotkey::vk_of_key(Key::Escape)
                    .unwrap_or(windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0));
                let pressed_esc_slot = label == "esc";
                let do_switch = if label == "switch" {
                    !(swap_esc && is_capslock)
                } else {
                    pressed_esc_slot && swap_esc && is_capslock
                };
                if do_switch {
                    switch_layout(in_define, &defined, &mut installed);
                } else {
                    win::send_key(esc_vk);
                }
                if notify {
                    // AHK: Sleep 100 before reading the new layout.
                    std::thread::spawn(|| {
                        std::thread::sleep(Duration::from_millis(100));
                        if let Some(klid) = current_klid() {
                            let name =
                                win::layout_text(u32::from_str_radix(&klid, 16).unwrap_or(0))
                                    .unwrap_or(klid);
                            win::center_tip(&name, Duration::from_millis(1400));
                        }
                    });
                }
                refresh_menu(
                    tray,
                    swap_esc,
                    in_define,
                    notify,
                    is_capslock,
                    &switch_combo,
                    &defined,
                    &mut installed,
                );
            }
            Event::Menu(label) => {
                match label.as_str() {
                    "Swap ESC/CapsLock" => {
                        swap_esc = !swap_esc;
                        persist_toggle(&text, &config_path, "swap_esc_caps", &swap_esc.to_string());
                    }
                    "Switch Mode: InDefine" | "Switch Mode: InAll" => {
                        in_define = !in_define;
                        let v = if in_define {
                            "\"InDefine\""
                        } else {
                            "\"InAll\""
                        };
                        persist_toggle(&text, &config_path, "switch_mode", v);
                    }
                    "Notify on Switch" => {
                        notify = !notify;
                        persist_toggle(
                            &text,
                            &config_path,
                            "notify_on_switch",
                            &notify.to_string(),
                        );
                    }
                    "Start with Windows" => {
                        let now = !win::autostart_enabled(APP);
                        if let Err(e) = win::set_autostart(APP, now) {
                            win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
                        }
                    }
                    "Edit Config" => {
                        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".into());
                        win::run_command(&format!("{editor} \"{}\"", config_path.display()));
                    }
                    "Reload" => {
                        if let Ok(me) = std::env::current_exe() {
                            let _ = std::process::Command::new(me).spawn();
                        }
                        tray.quit();
                    }
                    "Exit" => tray.quit(),
                    _ => {}
                }
                refresh_menu(
                    tray,
                    swap_esc,
                    in_define,
                    notify,
                    is_capslock,
                    &switch_combo,
                    &defined,
                    &mut installed,
                );
            }
            _ => {}
        }
    });
    Ok(())
}

/// Rewrite one `key = ...` line in the config file, preserving comments.
/// No-op when the key is absent (e.g. user deleted it).
#[cfg(windows)]
fn persist_toggle(original: &str, path: &std::path::Path, key: &str, value: &str) {
    let current = std::fs::read_to_string(path).unwrap_or_else(|_| original.to_string());
    let new_line = format!("{key} = {value}");
    let rewritten = current
        .lines()
        .map(|l| {
            let trimmed = l.trim_start();
            if trimmed.starts_with(&format!("{key} =")) || trimmed == key {
                new_line.as_str()
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(path, rewritten);
}

#[cfg(windows)]
fn switch_layout(in_define: bool, defined: &[DefinedLayout], installed: &mut Vec<isize>) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
    const WM_INPUTLANGCHANGEREQUEST: u32 = 0x50;

    let Some(hwnd) = kit::win::foreground_hwnd() else {
        return;
    };
    let current = kit::win::layout_of_window(hwnd).unwrap_or(0);

    if in_define {
        let cur_klid = format!("{:08x}", (current as u32) & 0xFFFF);
        let idx = defined
            .iter()
            .position(|l| l.klid.trim().eq_ignore_ascii_case(&cur_klid))
            .map(|i| i + 1)
            .unwrap_or(0);
        let next = &defined[idx % defined.len()];
        // AHK SetKeyboardLayoutShortcut: Alt+Shift+<digit>.
        let Some(digit) = next.shortcut.as_deref().and_then(|s| s.chars().next()) else {
            return;
        };
        let Some(vk) = kit::hotkey::vk_of_key(kit::keys::Key::Char(digit)) else {
            return;
        };
        send_alt_shift(vk);
    } else {
        if installed.is_empty() {
            *installed = installed_layouts();
        }
        if installed.is_empty() {
            return;
        }
        let cur = (current as u32) & 0xFFFF;
        let idx = installed
            .iter()
            .position(|h| (*h as u32) & 0xFFFF == cur)
            .map(|i| i + 1)
            .unwrap_or(0);
        let next = installed[idx % installed.len()];
        unsafe {
            let _ = PostMessageW(
                Some(hwnd),
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(0),
                LPARAM(next),
            );
        }
    }
}

#[cfg(windows)]
fn send_alt_shift(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    fn inp(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
    let down = KEYBD_EVENT_FLAGS(0);
    let inputs = [
        inp(VK_LMENU, down),
        inp(VK_LSHIFT, down),
        inp(vk, down),
        inp(vk, KEYEVENTF_KEYUP),
        inp(VK_LSHIFT, KEYEVENTF_KEYUP),
        inp(VK_LMENU, KEYEVENTF_KEYUP),
    ];
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)] // tray + full app state; a struct adds no clarity here
fn refresh_menu(
    tray: &mut kit::tray::Tray,
    swap: bool,
    in_define: bool,
    notify_on: bool,
    is_capslock: bool,
    switch_combo: &kit::keys::Combo,
    defined: &[DefinedLayout],
    installed: &mut Vec<isize>,
) {
    use kit::tray::Entry;

    // Tooltip (AHK UpdateTrayTip).
    let current = current_klid().unwrap_or_default();
    let mut tip = String::from("keyboard-switch\nSwitch Hotkey: ");
    if swap && is_capslock {
        tip.push_str(&format!("ESC ({switch_combo}->ESC)"));
    } else {
        tip.push_str(&switch_combo.to_string());
    }
    tip.push_str(if in_define {
        "\nDefined layouts:"
    } else {
        "\nInstalled layouts:"
    });
    let klids: Vec<String> = if in_define {
        defined
            .iter()
            .map(|l| l.klid.trim().to_ascii_lowercase())
            .collect()
    } else {
        if installed.is_empty() {
            *installed = installed_layouts();
        }
        installed
            .iter()
            .map(|h| format!("{:08x}", (*h as u32) & 0xFFFF))
            .collect()
    };
    for k in &klids {
        let name = u32::from_str_radix(k, 16)
            .ok()
            .and_then(kit::win::layout_text)
            .unwrap_or_else(|| k.clone());
        if *k == current {
            tip.push_str(&format!("\n> {name}"));
        } else {
            tip.push_str(&format!("\n  {name}"));
        }
    }
    tray.set_tooltip(&tip);

    let _ = notify_on;
    tray.set_menu(&[
        Entry::Toggle("Swap ESC/CapsLock", swap),
        Entry::Toggle(
            if in_define {
                "Switch Mode: InDefine"
            } else {
                "Switch Mode: InAll"
            },
            in_define,
        ),
        Entry::Toggle("Notify on Switch", notify_on),
        Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("keyboard-switch"),
        ),
        Entry::Sep,
        Entry::Item("Edit Config"),
        Entry::Item("Reload"),
        Entry::Item("Exit"),
    ]);
}
