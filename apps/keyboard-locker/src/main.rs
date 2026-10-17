//! keyboard-locker: press the shortcut (default Ctrl+Alt+Shift+Insert) to lock
//! the keyboard — a `WH_KEYBOARD_LL` hook swallows every key event. The same
//! combo (tracked inside the hook) unlocks. The mouse stays active, so the tray
//! menu always works. Clean-room MIT rewrite of sophice/ahk-keyboard-locker.

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::message_box("keyboard-locker", &format!("Error:\n{e:#}"));
            return Err(e);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("keyboard-locker")
    }
}

#[cfg(windows)]
mod lock {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    pub static LOCKED: AtomicBool = AtomicBool::new(false);

    /// Modifier/key state tracked from the events we see (not
    /// GetAsyncKeyState: swallowed events never reach the queue, so the
    /// async state would go stale while locked).
    struct State {
        ctrl: bool,
        shift: bool,
        alt: bool,
    }

    pub struct HookGuard {
        hook: HHOOK,
    }

    impl Drop for HookGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.hook);
            }
        }
    }

    /// Install the low-level keyboard hook. `combo` is the toggle; while
    /// unlocked it just watches, while locked it swallows everything except
    /// the toggle combo.
    pub fn install(combo: kit::keys::Combo) -> anyhow::Result<HookGuard> {
        COMBO.get_or_init(|| combo);
        let hmod = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into()
        };
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), Some(hmod), 0) }?;
        Ok(HookGuard { hook })
    }

    fn vk_of(key: kit::keys::Key) -> Option<VIRTUAL_KEY> {
        kit::hotkey::vk_of_key(key)
    }

    static COMBO: std::sync::OnceLock<kit::keys::Combo> = std::sync::OnceLock::new();

    thread_local! {
        static STATE: std::cell::RefCell<State> = const {
            std::cell::RefCell::new(State { ctrl: false, shift: false, alt: false })
        };
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let down = matches!(wparam.0 as u32, 0x0100 | 0x0104); // WM_KEYDOWN | WM_SYSKEYDOWN
            let vk = VIRTUAL_KEY(kb.vkCode as u16);

            let is_toggle = STATE.with(|s| {
                let mut s = s.borrow_mut();
                match vk {
                    VK_LCONTROL | VK_RCONTROL | VK_CONTROL => s.ctrl = down,
                    VK_LSHIFT | VK_RSHIFT | VK_SHIFT => s.shift = down,
                    VK_LMENU | VK_RMENU | VK_MENU => s.alt = down,
                    _ => {}
                }
                if !down {
                    return false;
                }
                let combo = *COMBO.get().unwrap_or(&kit::keys::Combo::default());
                let (want_vk, mods_ok) = match vk_of(combo.key) {
                    Some(want) => (
                        want,
                        s.ctrl >= combo.ctrl && s.shift >= combo.shift && s.alt >= combo.alt,
                    ),
                    None => (VIRTUAL_KEY(0), false),
                };
                vk == want_vk && mods_ok
            });

            if is_toggle {
                let now = !LOCKED.load(Ordering::SeqCst);
                LOCKED.store(now, Ordering::SeqCst);
                // Wake the tray loop so it can refresh icon/tooltip/menu.
                unsafe {
                    let _ = PostMessageW(None, WM_APP, WPARAM(0), LPARAM(0));
                }
                return LRESULT(1); // swallow the combo either way
            }

            if LOCKED.load(Ordering::SeqCst) {
                return LRESULT(1); // swallow everything
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::keys::Combo;
    use kit::tray::{Event, Tray};
    use kit::{config, win};
    use serde::Deserialize;

    const APP: &str = "keyboard-locker";

    let _single = win::ensure_single_instance(APP)?;

    #[derive(Deserialize)]
    struct General {
        shortcut: Option<String>,
    }
    #[derive(Deserialize, Default)]
    struct FileConfig {
        general: Option<General>,
    }

    let config_path = config::exe_dir_file("keyboard-locker.toml")?;
    let text = config::load_or_create(&config_path, include_str!("../keyboard-locker.toml"))?;
    let file: FileConfig = toml::from_str(&text)?;
    let combo: Combo = file
        .general
        .and_then(|g| g.shortcut)
        .unwrap_or_else(|| "ctrl+alt+shift+insert".into())
        .parse()?;

    let _hook = lock::install(combo)?;

    let mut tray = Tray::new(APP, icon(false))?;
    refresh_menu(&mut tray, false);

    tray.run(None, |tray, event| {
        let locked = lock::LOCKED.load(std::sync::atomic::Ordering::SeqCst);
        match event {
            Event::Wakeup(_) => {
                let locked = lock::LOCKED.load(std::sync::atomic::Ordering::SeqCst);
                let _ = tray.set_icon(icon(locked));
                tray.set_tooltip(if locked {
                    "keyboard-locker (locked)"
                } else {
                    APP
                });
                refresh_menu(tray, locked);
            }
            Event::Menu(label) if label == "Keyboard Locked" => {
                let now = !lock::LOCKED.load(std::sync::atomic::Ordering::SeqCst);
                lock::LOCKED.store(now, std::sync::atomic::Ordering::SeqCst);
                let _ = tray.set_icon(icon(now));
                tray.set_tooltip(if now { "keyboard-locker (locked)" } else { APP });
                refresh_menu(tray, now);
            }
            Event::Menu(label) if label == "Start with Windows" => {
                let now = !win::autostart_enabled(APP);
                if let Err(e) = win::set_autostart(APP, now) {
                    win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
                }
                refresh_menu(tray, locked);
            }
            Event::Menu(label) if label == "Exit" => tray.quit(),
            _ => {}
        }
    });
    Ok(())
}

#[cfg(windows)]
fn icon(locked: bool) -> Vec<u8> {
    if locked {
        kit::icon::letter_rgba('l', [0xc0, 0x39, 0x2b, 0xff], [0x7a, 0x1f, 0x16, 0xff])
    } else {
        kit::icon::letter_rgba('l', [0x2e, 0x7d, 0x32, 0xff], [0x1b, 0x4d, 0x1d, 0xff])
    }
}

#[cfg(windows)]
fn refresh_menu(tray: &mut kit::tray::Tray, locked: bool) {
    use kit::tray::Entry;
    tray.set_menu(&[
        Entry::Toggle("Keyboard Locked", locked),
        Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("keyboard-locker"),
        ),
        Entry::Sep,
        Entry::Item("Exit"),
    ]);
}
