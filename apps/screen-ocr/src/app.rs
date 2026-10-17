//! App wiring: config, hotkeys, tray.

use crate::{notify, ocr, FileConfig};
use anyhow::Result;
use kit::keys::Combo;
use kit::tray::{Event, Tray};
use kit::{config, hotkey::Hotkeys, win};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn run() -> Result<()> {
    const APP: &str = "screen-ocr";
    let _single = win::ensure_single_instance(APP)?;

    let exe_dir = std::env::current_exe()?
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("no exe dir"))?;

    // Ship the OCR CLI script next to the exe (AHK: lib\rapidocr_cli.py).
    let script_dir = exe_dir.join("lib");
    std::fs::create_dir_all(&script_dir)?;
    let script_path = script_dir.join("rapidocr_cli.py");
    if !script_path.exists() {
        std::fs::write(&script_path, include_str!("../lib/rapidocr_cli.py"))?;
    }

    let config_path = config::exe_dir_file("screen-ocr.toml")?;
    let text = config::load_or_create(&config_path, include_str!("../screen-ocr.toml"))?;
    let file: FileConfig = toml::from_str(&text)?;
    let ocr_cfg = file.ocr.unwrap();
    let rapid = file.rapidocr.unwrap();
    let msgs = file.notify_messages.unwrap();

    let langs: Vec<String> = rapid
        .languages
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if langs.is_empty() {
        anyhow::bail!("no languages configured; set [rapidocr] languages");
    }
    let default_lang = langs[0].clone();
    // app_run continues below
    app_run(
        APP,
        &exe_dir,
        &config_path,
        ocr_cfg,
        msgs,
        default_lang,
        rapid.hotkey,
        file.rapidocr_lang_hotkey,
    )
}

#[allow(clippy::too_many_arguments)]
fn app_run(
    app: &str,
    exe_dir: &std::path::Path,
    config_path: &std::path::Path,
    ocr_cfg: crate::OcrSection,
    msgs: crate::NotifyMessages,
    default_lang: String,
    rapid_hotkey: Option<String>,
    lang_hotkeys: Option<std::collections::HashMap<String, String>>,
) -> Result<()> {
    let flameshot = ocr_cfg.screenshot_tool.as_deref() == Some("flameshot");
    let notify_cfg = Arc::new(notify::Config {
        method: ocr_cfg
            .notify_method
            .clone()
            .unwrap_or_else(|| "traytip".into()),
        enabled: AtomicBool::new(ocr_cfg.notify.unwrap_or(true)),
        recog_msg: msgs.recog_msg.clone().unwrap_or_default(),
        no_text_msg: msgs.no_text_msg.clone().unwrap_or_default(),
        icon_png: exe_dir.join("assets").join("icon_128.png"),
    });
    let checking = Arc::new(AtomicBool::new(false));

    // Hotkeys: global, rapid, then per-language ("lang:<code>").
    let mut defs: Vec<(String, Combo)> = Vec::new();
    let global_hk = ocr_cfg.global_hotkey.clone().unwrap_or_default();
    if !global_hk.trim().is_empty() {
        defs.push(("global".into(), global_hk.parse()?));
    }
    if let Some(hk) = rapid_hotkey {
        if !hk.trim().is_empty() {
            defs.push(("rapid".into(), hk.parse()?));
        }
    }
    for (lang, hk) in lang_hotkeys.unwrap_or_default() {
        if !lang.trim().is_empty() && !hk.trim().is_empty() {
            defs.push((format!("lang:{lang}"), hk.parse()?));
        }
    }
    if defs.is_empty() {
        anyhow::bail!("no hotkeys configured");
    }
    let hotkeys = Hotkeys::register(
        &defs
            .iter()
            .map(|(l, c)| (l.as_str(), *c))
            .collect::<Vec<_>>(),
    )?;

    let script_path = exe_dir.join("lib").join("rapidocr_cli.py");
    let cache_dir = exe_dir.join("cache");
    std::fs::create_dir_all(&cache_dir)?;

    let mut tray = Tray::new(
        "screen-ocr",
        kit::icon::letter_rgba('o', [0x3b, 0x6e, 0xa5, 0xff], [0x23, 0x46, 0x6e, 0xff]),
    )?;
    refresh_menu(
        &mut tray,
        notify_cfg.enabled.load(Ordering::SeqCst),
        checking.load(Ordering::SeqCst),
    );

    tray_loop(
        tray,
        hotkeys,
        notify_cfg,
        checking,
        app,
        config_path,
        script_path,
        cache_dir,
        default_lang,
        flameshot,
    )
}

#[allow(clippy::too_many_arguments)]
fn tray_loop(
    mut tray: Tray,
    hotkeys: Hotkeys,
    notify_cfg: Arc<notify::Config>,
    checking: Arc<AtomicBool>,
    app: &str,
    config_path: &std::path::Path,
    script_path: std::path::PathBuf,
    cache_dir: std::path::PathBuf,
    default_lang: String,
    flameshot: bool,
) -> Result<()> {
    tray.run(Some(&hotkeys), |tray, event| match event {
        Event::Hotkey(label) => {
            let lang = if let Some(code) = label.strip_prefix("lang:") {
                code.to_string()
            } else {
                default_lang.clone()
            };
            ocr::run_ocr(lang, flameshot, &script_path, &notify_cfg);
        }
        Event::Menu(label) => {
            match label.as_str() {
                "Notify" => {
                    let now = !notify_cfg.enabled.load(Ordering::SeqCst);
                    notify_cfg.enabled.store(now, Ordering::SeqCst);
                }
                "Start with Windows" => {
                    let now = !win::autostart_enabled(app);
                    if let Err(e) = win::set_autostart(app, now) {
                        win::message_box(app, &format!("Autostart failed:\n{e:#}"));
                    }
                }
                "Check Languages" => {
                    if checking
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        let (script, cache, cfg, chk) = (
                            script_path.clone(),
                            cache_dir.clone(),
                            notify_cfg.clone(),
                            checking.clone(),
                        );
                        std::thread::spawn(move || {
                            ocr::check_langs(&script, &cache, &cfg);
                            chk.store(false, Ordering::SeqCst);
                        });
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
                notify_cfg.enabled.load(Ordering::SeqCst),
                checking.load(Ordering::SeqCst),
            );
        }
        _ => {}
    });
    Ok(())
}

fn refresh_menu(tray: &mut Tray, notify_on: bool, checking: bool) {
    use kit::tray::Entry;
    tray.set_menu(&[
        Entry::Toggle("Notify", notify_on),
        Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("screen-ocr"),
        ),
        Entry::Toggle("Check Languages", checking),
        Entry::Sep,
        Entry::Item("Edit Config"),
        Entry::Item("Reload"),
        Entry::Item("Exit"),
    ]);
}
