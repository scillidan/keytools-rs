//! keyboard-auto-switch-back: when the foreground window changes, wait
//! `switch_delay` ms and then force the target keyboard layout via
//! `WM_INPUTLANGCHANGEREQUEST`.
//! Ported from AHK-KeyboardAutoSwitchBack.
//!
//! Parity notes:
//! - The AHK original polls every 200 ms (`CheckInterval`); we use
//!   `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`, which is event-driven and
//!   needs no polling. `CheckInterval` is therefore dropped.
//! - The one-shot delay timer becomes a generation counter + worker thread:
//!   each new foreground event bumps the generation, so only the latest
//!   window gets the switch after the delay.

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::message_box("keyboard-auto-switch-back", &format!("Error:\n{e:#}"));
            return Err(e);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("keyboard-auto-switch-back")
    }
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use anyhow::Context;
    use kit::tray::{Event, Tray};
    use kit::{config, win};
    use serde::Deserialize;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT};

    const APP: &str = "keyboard-auto-switch-back";

    let _single = win::ensure_single_instance(APP)?;

    #[derive(Deserialize, Default)]
    struct General {
        switch_delay: Option<u64>,
        target_layout: Option<String>,
    }
    #[derive(Deserialize, Default)]
    struct FileConfig {
        general: Option<General>,
    }

    let config_path = config::exe_dir_file("keyboard-auto-switch-back.toml")?;
    let text = config::load_or_create(
        &config_path,
        include_str!("../keyboard-auto-switch-back.toml"),
    )?;
    let file: FileConfig = toml::from_str(&text)?;
    let general = file.general.unwrap_or_default();
    let switch_delay = general.switch_delay.unwrap_or(600);
    let target_layout: isize = general
        .target_layout
        .as_deref()
        .unwrap_or("0x04090409")
        .trim_start_matches("0x")
        .parse()
        .context("target_layout must be a hex HKL like 0x04090409")?;
    let target_lang_id = (target_layout as u32) & 0xFFFF;

    let layout_name =
        win::layout_text(target_lang_id).unwrap_or_else(|| format!("0x{target_lang_id:08X}"));
    let tooltip = format!("{APP}\nTarget layout: {layout_name}");

    // Foreground events -> worker that applies the layout after the delay.
    // HWND is not Send, so shuttle it as isize.
    let (tx, rx) = mpsc::channel::<isize>();
    let generation = std::sync::Arc::new(AtomicU64::new(0));
    {
        let generation = generation.clone();
        thread::spawn(move || {
            while let Ok(hwnd) = rx.recv() {
                let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut core::ffi::c_void);
                let gen = generation.fetch_add(1, Ordering::SeqCst) + 1;
                thread::sleep(Duration::from_millis(switch_delay));
                if generation.load(Ordering::SeqCst) != gen {
                    continue; // a newer foreground event superseded this one
                }
                apply_layout(hwnd, target_layout, target_lang_id);
            }
        });
    }

    // The hook callback runs on this thread while the tray loop pumps messages.
    thread_local! {
        static TX: std::cell::RefCell<Option<mpsc::Sender<isize>>> = const { std::cell::RefCell::new(None) };
    }
    TX.with(|cell| *cell.borrow_mut() = Some(tx));
    unsafe extern "system" fn foreground_hook(
        _hook: HWINEVENTHOOK,
        _event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _event_thread: u32,
        _event_time: u32,
    ) {
        if !hwnd.0.is_null() {
            TX.with(|cell| {
                if let Some(tx) = cell.borrow().as_ref() {
                    let _ = tx.send(hwnd.0 as isize);
                }
            });
        }
    }
    let hook: HWINEVENTHOOK = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_hook),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook.0.is_null() {
        anyhow::bail!("SetWinEventHook(EVENT_SYSTEM_FOREGROUND) failed");
    }
    let _unhook = UnhookGuard(hook);

    let mut tray = Tray::new(
        &tooltip,
        kit::icon::letter_rgba('k', [0x3b, 0x6e, 0xa5, 0xff], [0x23, 0x46, 0x6e, 0xff]),
    )?;
    refresh_menu(&mut tray);

    tray.run(None, |tray, event| {
        if let Event::Menu(label) = event {
            if label == "Start with Windows" {
                let now = !win::autostart_enabled(APP);
                if let Err(e) = win::set_autostart(APP, now) {
                    win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
                }
                refresh_menu(tray);
            } else if label == "Edit Config" {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".into());
                win::run_command(&format!("{editor} \"{}\"", config_path.display()));
            } else if label == "Reload" {
                if let Ok(me) = std::env::current_exe() {
                    let _ = std::process::Command::new(me).spawn();
                }
                tray.quit();
            } else if label == "Exit" {
                tray.quit();
            }
        }
    });
    Ok(())
}

#[cfg(windows)]
fn apply_layout(hwnd: windows::Win32::Foundation::HWND, target_layout: isize, target_lang_id: u32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, PostMessageW};
    const WM_INPUTLANGCHANGEREQUEST: u32 = 0x50;
    unsafe {
        if GetForegroundWindow() != hwnd {
            return; // window is not foreground anymore (AHK: curID != lastWinID)
        }
        let Some(cur) = kit::win::layout_of_window(hwnd) else {
            return;
        };
        if (cur as u32) & 0xFFFF != target_lang_id {
            let _ = PostMessageW(
                Some(hwnd),
                WM_INPUTLANGCHANGEREQUEST,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(target_layout),
            );
        }
    }
}

#[cfg(windows)]
struct UnhookGuard(windows::Win32::UI::Accessibility::HWINEVENTHOOK);

#[cfg(windows)]
impl Drop for UnhookGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::UI::Accessibility::UnhookWinEvent(self.0);
        }
    }
}

#[cfg(windows)]
fn refresh_menu(tray: &mut kit::tray::Tray) {
    tray.set_menu(&[
        kit::tray::Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("keyboard-auto-switch-back"),
        ),
        kit::tray::Entry::Sep,
        kit::tray::Entry::Item("Edit Config"),
        kit::tray::Entry::Item("Reload"),
        kit::tray::Entry::Item("Exit"),
    ]);
}
