//! goldendict-auto-search: bind windows (hotkey on the window under the
//! cursor) so double-clicking a word copies+queries GoldenDict, and/or so
//! clipboard changes inside bound windows query it.
//! Ported from AHK-GoldenDictAutoSearch.
//!
//! Parity notes:
//! - The AHK original clears the clipboard, sends ^c, ClipWaits 500 ms, then
//!   restores ClipboardAll. We never write the clipboard: we snapshot the
//!   text, send ^c, and wait (<=500 ms) for the text to change — same
//!   detection, zero clipboard pollution.
//! - The 500 ms clipboard poll timer becomes a thread posting wakeups.
//! - Bindings are in-memory only, as in the AHK original.

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::message_box("goldendict-auto-search", &format!("Error:\n{e:#}"));
            return Err(e);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("goldendict-auto-search")
    }
}

#[cfg(windows)]
fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct GdSection {
    executable: Option<String>,
    double_click_group: Option<String>,
    clipboard_group: Option<String>,
    clipboard_hotkey_enabled: Option<bool>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct SettingsSection {
    double_click_enabled: Option<bool>,
    clipboard_enabled: Option<bool>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct HotkeysSection {
    toggle_key: Option<String>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct FileConfig {
    goldendict: Option<GdSection>,
    settings: Option<SettingsSection>,
    hotkeys: Option<HotkeysSection>,
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::keys::Combo;
    use kit::tray::{Event, Tray};
    use kit::{config, hotkey::Hotkeys, win};
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, RwLock};
    use std::time::Duration;

    const APP: &str = "goldendict-auto-search";

    let _single = win::ensure_single_instance(APP)?;

    let config_path = config::exe_dir_file("goldendict-auto-search.toml")?;
    let text =
        config::load_or_create(&config_path, include_str!("../goldendict-auto-search.toml"))?;
    let file: FileConfig = toml::from_str(&text)?;
    let gd = file.goldendict.unwrap_or(GdSection {
        executable: None,
        double_click_group: None,
        clipboard_group: None,
        clipboard_hotkey_enabled: None,
    });
    let st = file.settings.unwrap_or(SettingsSection {
        double_click_enabled: None,
        clipboard_enabled: None,
    });
    let hk = file.hotkeys.unwrap_or(HotkeysSection { toggle_key: None });

    let exe = gd.executable.unwrap_or_else(|| "goldendict".into());
    let dc_group = gd.double_click_group.unwrap_or_else(|| "default".into());
    let cb_group = gd.clipboard_group.unwrap_or_else(|| "translate".into());
    let popup = Arc::new(AtomicBool::new(
        gd.clipboard_hotkey_enabled.unwrap_or(false),
    ));
    let mut dc_enabled = st.double_click_enabled.unwrap_or(true);
    let cb_enabled_cfg = st.clipboard_enabled.unwrap_or(false);
    let toggle_combo: Combo = hk.toggle_key.unwrap_or_else(|| "^!+g".into()).parse()?;

    // Shared state between the tray loop and helper threads.
    let bound: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));
    let clip_guard_ms = Arc::new(AtomicU64::new(0)); // AHK ClipGuardTick
    let cb_enabled = Arc::new(AtomicBool::new(cb_enabled_cfg));

    // Passive mouse hook: report every left-button release to the loop
    // (AHK `~LButton Up`). wparam 1 = "button released".
    let _mouse_hook = mouse::install()?;

    // Clipboard monitor thread (AHK ClipMonitorTimer, 500 ms).
    {
        let bound = bound.clone();
        let clip_guard_ms = clip_guard_ms.clone();
        let cb_enabled = cb_enabled.clone();
        let popup = popup.clone();
        let exe = exe.clone();
        let cb_group = cb_group.clone();
        std::thread::spawn(move || {
            let mut cb_last = String::new(); // AHK CB_LastText
            loop {
                std::thread::sleep(Duration::from_millis(500));
                if !cb_enabled.load(Ordering::SeqCst) {
                    continue;
                }
                if epoch_ms().saturating_sub(clip_guard_ms.load(Ordering::SeqCst)) < 1500 {
                    continue;
                }
                let clip = arboard::Clipboard::new()
                    .ok()
                    .and_then(|mut cb| cb.get_text().ok())
                    .unwrap_or_default();
                let clip_text = clip.trim().to_string();
                let fg_bound = win::foreground_hwnd()
                    .and_then(win::process_name)
                    .is_some_and(|name| bound.read().map(|b| b.contains(&name)).unwrap_or(false));
                if !fg_bound {
                    if !clip_text.is_empty() && clip_text.chars().count() <= 1000 {
                        cb_last = clip_text;
                    }
                    continue;
                }
                if clip_text.is_empty() || clip_text.chars().count() > 1000 {
                    continue;
                }
                if clip_text == cb_last {
                    continue;
                }
                cb_last = clip_text.clone();
                search(&exe, &cb_group, popup.load(Ordering::SeqCst), &clip_text);
            }
        });
    }

    // Tooltip refresh thread (AHK UpdateTrayTip, 1000 ms). wparam 3.
    {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(1000));
            unsafe {
                use windows::Win32::Foundation::{LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
                let _ = PostMessageW(None, WM_APP, WPARAM(3), LPARAM(0));
            }
        });
    }

    let hotkeys = Hotkeys::register(&[("toggle", toggle_combo)])?;

    let mut tray = Tray::new(
        APP,
        kit::icon::letter_rgba('g', [0xb8, 0x86, 0x0b, 0xff], [0x7c, 0x59, 0x06, 0xff]),
    )?;
    refresh_menu(
        &mut tray,
        dc_enabled,
        cb_enabled.load(Ordering::SeqCst),
        popup.load(Ordering::SeqCst),
    );

    let mut last_click_ms: u64 = 0; // AHK LastClickTime

    tray.run(Some(&hotkeys), |tray, event| {
        match event {
            Event::Hotkey(label) if label == "toggle" => {
                let Some(hwnd) = win::hwnd_under_cursor() else {
                    win::center_tip("No window under cursor", Duration::from_millis(1500));
                    return;
                };
                let Some(name) = win::process_name(hwnd) else {
                    win::center_tip("Cannot get window info", Duration::from_millis(1500));
                    return;
                };
                let mut b = bound.write().unwrap();
                let msg = if b.contains(&name) {
                    b.remove(&name);
                    format!("Unbound: {name}")
                } else {
                    b.insert(name.clone());
                    format!("Bound: {name}")
                };
                drop(b);
                win::center_tip(&msg, Duration::from_millis(1500));
                refresh_menu(
                    tray,
                    dc_enabled,
                    cb_enabled.load(Ordering::SeqCst),
                    popup.load(Ordering::SeqCst),
                );
            }
            Event::Wakeup(1) => {
                // Left button released (AHK OnLButtonUp).
                if !dc_enabled {
                    return;
                }
                let fg_bound = win::foreground_hwnd()
                    .and_then(win::process_name)
                    .is_some_and(|name| bound.read().map(|b| b.contains(&name)).unwrap_or(false));
                if !fg_bound {
                    return;
                }
                let now = epoch_ms();
                let is_double = now.saturating_sub(last_click_ms) < 300;
                last_click_ms = now;
                if !is_double {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50)); // AHK Sleep 50
                clip_guard_ms.store(epoch_ms(), Ordering::SeqCst);
                let old = arboard::Clipboard::new()
                    .ok()
                    .and_then(|mut cb| cb.get_text().ok())
                    .unwrap_or_default();
                win::send_ctrl_c();
                // ClipWait 0.5: first text different from the pre-copy text.
                let mut text = String::new();
                for _ in 0..20 {
                    std::thread::sleep(Duration::from_millis(25));
                    text = arboard::Clipboard::new()
                        .ok()
                        .and_then(|mut cb| cb.get_text().ok())
                        .unwrap_or_default();
                    if !text.is_empty() && text != old {
                        break;
                    }
                }
                if text.is_empty() || text == old {
                    return; // AHK: ClipWait timed out, restore and return
                }
                let text = text.trim().to_string();
                if text.is_empty() || text.chars().count() > 1000 {
                    return;
                }
                // AHK also stores DC_LastText here; nothing reads it, so we skip it.
                search(&exe, &dc_group, popup.load(Ordering::SeqCst), &text);
            }
            Event::Wakeup(3) => {
                update_tooltip(tray, &toggle_combo, &bound);
            }
            Event::Menu(label) => {
                match label.as_str() {
                    "DoubleClick Mode" => {
                        dc_enabled = !dc_enabled;
                        persist(
                            &text,
                            &config_path,
                            "double_click_enabled",
                            &dc_enabled.to_string(),
                        );
                    }
                    "Clipboard Mode" => {
                        let now = !cb_enabled.load(Ordering::SeqCst);
                        cb_enabled.store(now, Ordering::SeqCst);
                        persist(&text, &config_path, "clipboard_enabled", &now.to_string());
                    }
                    "Clear All Bound Windows" => {
                        bound.write().unwrap().clear();
                        win::center_tip("All bindings cleared", Duration::from_millis(1500));
                    }
                    "Clipboard Hotkey (Popup Search)" => {
                        let now = !popup.load(Ordering::SeqCst);
                        popup.store(now, Ordering::SeqCst);
                        persist(
                            &text,
                            &config_path,
                            "clipboard_hotkey_enabled",
                            &now.to_string(),
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
                    dc_enabled,
                    cb_enabled.load(Ordering::SeqCst),
                    popup.load(Ordering::SeqCst),
                );
            }
            _ => {}
        }
    });
    Ok(())
}

#[cfg(windows)]
fn search(exe: &str, group: &str, popup: bool, query: &str) {
    let param = if popup {
        format!("--popup-group-name={group}")
    } else {
        format!("--group-name={group}")
    };
    let _ = std::process::Command::new(exe)
        .arg(param)
        .arg(query)
        .spawn();
}

#[cfg(windows)]
fn persist(original: &str, path: &std::path::Path, key: &str, value: &str) {
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
fn update_tooltip(
    tray: &mut kit::tray::Tray,
    toggle: &kit::keys::Combo,
    bound: &std::sync::RwLock<std::collections::HashSet<String>>,
) {
    let mut tip = format!("goldendict-auto-search\nToggle Key: {toggle}");
    let list: Vec<String> = bound
        .read()
        .map(|b| b.iter().cloned().collect())
        .unwrap_or_default();
    if !list.is_empty() {
        let mut sorted = list;
        sorted.sort();
        tip.push_str(&format!("\nBind Windows [{}]", sorted.len()));
        for exe in sorted {
            tip.push_str(&format!("\n  {exe}"));
        }
    }
    tray.set_tooltip(&tip);
}

#[cfg(windows)]
fn refresh_menu(tray: &mut kit::tray::Tray, dc: bool, cb: bool, popup: bool) {
    use kit::tray::Entry;
    tray.set_menu(&[
        Entry::Toggle("DoubleClick Mode", dc),
        Entry::Toggle("Clipboard Mode", cb),
        Entry::Sep,
        Entry::Item("Clear All Bound Windows"),
        Entry::Toggle("Clipboard Hotkey (Popup Search)", popup),
        Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("goldendict-auto-search"),
        ),
        Entry::Sep,
        Entry::Item("Edit Config"),
        Entry::Item("Reload"),
        Entry::Item("Exit"),
    ]);
}

#[cfg(windows)]
mod mouse {
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, WH_MOUSE_LL, WM_LBUTTONUP,
    };

    pub struct MouseHook(HHOOK);

    impl Drop for MouseHook {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.0);
            }
        }
    }

    /// Passive `WH_MOUSE_LL` hook: forwards every left-button release to the
    /// tray loop as `WM_APP` wparam 1 (never swallows).
    pub fn install() -> anyhow::Result<MouseHook> {
        let hmod = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into()
        };
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(proc), Some(hmod), 0) }?;
        Ok(MouseHook(hook))
    }

    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && wparam.0 as u32 == WM_LBUTTONUP {
            unsafe {
                use windows::Win32::Foundation::{LPARAM as LP, WPARAM as WP};
                use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
                let _ = PostMessageW(None, WM_APP, WP(1), LP(0));
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}
