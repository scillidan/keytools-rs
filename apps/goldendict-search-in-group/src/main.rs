//! goldendict-search-in-group: press a per-group hotkey to search the current
//! clipboard text in GoldenDict restricted to that dictionary group.
//! Ported from AHK-GoldenDictSearchInGroup (INI -> TOML, same behavior).
//!
//! Parity notes:
//! - The AHK original saves/restores `ClipboardAll` but never writes the
//!   clipboard, so the restore is a no-op; we just read the text.
//! - AHK hotkey syntax (`!+z`, `^!+g`) parses via kit's combo parser.
//! - Double-press (`!xx` = press `!x` twice within 400 ms) is detected with
//!   the same `A_TickCount`-style timestamp map.

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::message_box("goldendict-search-in-group", &format!("Error:\n{e:#}"));
            return Err(e);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("goldendict-search-in-group")
    }
}

/// (base hotkey, is_double_press): `!xx` -> (`!x`, true). Same rule as the
/// AHK original: last two characters identical, length >= 2.
#[cfg(windows)]
fn split_double_press(hotkey: &str) -> (&str, bool) {
    let chars: Vec<char> = hotkey.chars().collect();
    if chars.len() >= 2 && chars[chars.len() - 1] == chars[chars.len() - 2] {
        let base_len = hotkey.len() - chars[chars.len() - 1].len_utf8();
        (&hotkey[..base_len], true)
    } else {
        (hotkey, false)
    }
}

#[cfg(windows)]
#[derive(Clone, serde::Deserialize)]
struct Group {
    name: String,
    hotkey: Option<String>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct FileConfig {
    goldendict: Option<GdSection>,
    messages: Option<Messages>,
    group: Option<Vec<Group>>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct GdSection {
    executable: Option<String>,
    clipboard_hotkey_enabled: Option<bool>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct Messages {
    no_selection: Option<String>,
}

#[cfg(windows)]
const DBL_CLICK_MS: u128 = 400; // AHK DblClickTime

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::keys::Combo;
    use kit::tray::{Event, Tray};
    use kit::{config, hotkey::Hotkeys, win};
    use std::collections::HashMap;
    use std::time::Instant;

    const APP: &str = "goldendict-search-in-group";

    let _single = win::ensure_single_instance(APP)?;

    let config_path = config::exe_dir_file("goldendict-search-in-group.toml")?;
    let text = config::load_or_create(
        &config_path,
        include_str!("../goldendict-search-in-group.toml"),
    )?;
    let file: FileConfig = toml::from_str(&text)?;
    let gd = file.goldendict.unwrap_or(GdSection {
        executable: None,
        clipboard_hotkey_enabled: None,
    });
    let no_selection = file
        .messages
        .and_then(|m| m.no_selection)
        .unwrap_or_else(|| "No text copied. Please select the word to query first.".into());

    let mut groups: Vec<Group> = file.group.unwrap_or_default();
    groups.truncate(10); // AHK Loop, 10
    let groups: Vec<Group> = groups.into_iter().filter(|g| !g.name.is_empty()).collect();
    if groups.is_empty() {
        anyhow::bail!("no [[group]] entries configured");
    }

    // Hotkeys: one per group. Double-press groups register their base combo
    // (AHK: `Hotkey, %baseHk%, HandleDoublePress`).
    let mut hotkey_defs: Vec<(String, Combo, bool)> = Vec::new(); // (label, combo, double_press)
    let mut tooltip = String::from("GoldenDict Search\nGroups and hotkeys");
    for (i, g) in groups.iter().enumerate() {
        if let Some(hk) = &g.hotkey {
            let hk = hk.trim();
            if hk.is_empty() {
                continue;
            }
            let (base, dbl) = split_double_press(hk);
            let combo: Combo = base.parse()?;
            tooltip.push_str(&format!("\n  {} = {}", g.name, hk));
            hotkey_defs.push((i.to_string(), combo, dbl));
        }
    }
    if hotkey_defs.is_empty() {
        anyhow::bail!("no group hotkeys configured");
    }

    let hotkeys = Hotkeys::register(
        &hotkey_defs
            .iter()
            .map(|(l, c, _)| (l.as_str(), *c))
            .collect::<Vec<_>>(),
    )?;

    let mut tray = Tray::new(
        &tooltip,
        kit::icon::letter_rgba('g', [0xb8, 0x86, 0x0b, 0xff], [0x7c, 0x59, 0x06, 0xff]),
    )?;

    let mut popup = gd.clipboard_hotkey_enabled.unwrap_or(false);
    let exe = gd.executable.unwrap_or_else(|| "goldendict".into());
    let mut last_press: HashMap<String, Instant> = HashMap::new();

    refresh_menu(&mut tray, popup);

    tray.run(Some(&hotkeys), |tray, event| match event {
        Event::Hotkey(label) => {
            let Ok(i) = label.parse::<usize>() else {
                return;
            };
            let Some((_, _, dbl)) = hotkey_defs.get(i) else {
                return;
            };
            if *dbl {
                let now = Instant::now();
                let hit = last_press
                    .get(&label)
                    .is_some_and(|t| now.duration_since(*t).as_millis() < DBL_CLICK_MS);
                if hit {
                    last_press.remove(&label);
                    search(&exe, &groups[i].name, popup, &no_selection);
                } else {
                    last_press.insert(label, now);
                }
            } else {
                search(&exe, &groups[i].name, popup, &no_selection);
            }
        }
        Event::Menu(label) if label == "Clipboard Hotkey (Popup Search)" => {
            popup = !popup;
            // Persist the toggle by rewriting just its line (keeps comments).
            let new_line = format!("clipboard_hotkey_enabled = {popup}");
            let rewritten = text
                .lines()
                .map(|l| {
                    if l.trim_start().starts_with("clipboard_hotkey_enabled") {
                        new_line.as_str()
                    } else {
                        l
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let _ = std::fs::write(&config_path, rewritten);
            refresh_menu(tray, popup);
        }
        Event::Menu(label) if label == "Start with Windows" => {
            let now = !win::autostart_enabled(APP);
            if let Err(e) = win::set_autostart(APP, now) {
                win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
            }
            refresh_menu(tray, popup);
        }
        Event::Menu(label) if label == "Edit Config" => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".into());
            win::run_command(&format!("{editor} \"{}\"", config_path.display()));
        }
        Event::Menu(label) if label == "Reload" => {
            if let Ok(me) = std::env::current_exe() {
                let _ = std::process::Command::new(me).spawn();
            }
            tray.quit();
        }
        Event::Menu(label) if label == "Exit" => tray.quit(),
        _ => {}
    });
    Ok(())
}

/// Look up the clipboard text in GoldenDict restricted to `group`.
#[cfg(windows)]
fn search(exe: &str, group: &str, popup: bool, no_selection: &str) {
    let query = arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    if query.is_empty() {
        kit::win::message_box("goldendict-search-in-group", no_selection);
        return;
    }
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
fn refresh_menu(tray: &mut kit::tray::Tray, popup: bool) {
    use kit::tray::Entry;
    tray.set_menu(&[
        Entry::Toggle("Clipboard Hotkey (Popup Search)", popup),
        Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("goldendict-search-in-group"),
        ),
        Entry::Sep,
        Entry::Item("Edit Config"),
        Entry::Item("Reload"),
        Entry::Item("Exit"),
    ]);
}

#[cfg(test)]
mod tests {
    use super::split_double_press;

    #[test]
    fn double_press_detection() {
        assert_eq!(split_double_press("!xx"), ("!x", true));
        assert_eq!(split_double_press("xx"), ("x", true));
        assert_eq!(split_double_press("!+z"), ("!+z", false));
        assert_eq!(split_double_press("x"), ("x", false));
    }
}
