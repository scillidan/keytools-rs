//! OCR orchestration: acquire image, run `uv run rapidocr_cli.py`, read text,
//! put it on the clipboard. Ported from AHK-ScreenOCR's Vis2 provider flow.

use crate::{capture, notify};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn temp_path(stem: &str, ext: &str) -> PathBuf {
    std::env::temp_dir().join(format!("screenocr_{}_{}.{ext}", std::process::id(), stem))
}

/// Full OCR flow for one hotkey press, on its own thread (AHK: OCR()).
pub fn run_ocr(lang: String, flameshot: bool, script: &Path, cfg: &Arc<notify::Config>) {
    let script = script.to_path_buf();
    let cfg = cfg.clone();
    std::thread::spawn(move || {
        if !cfg.recog_msg.is_empty() {
            cfg.show("screen-ocr", &cfg.recog_msg.clone());
        }
        let img = temp_path("in", "bmp");
        let out = temp_path("out", "txt");
        let acquired = if flameshot {
            acquire_flameshot(&img)
        } else {
            capture::select_and_capture(&img)
        };
        if !acquired {
            return;
        }
        let text = convert(&script, &img, &out, &lang);
        let _ = std::fs::remove_file(&img);
        let _ = std::fs::remove_file(&out);
        let Some(text) = text else {
            cfg.show("screen-ocr", "OCR failed (see README)");
            return;
        };
        if text.is_empty() {
            if !cfg.no_text_msg.is_empty() {
                cfg.show("screen-ocr", &cfg.no_text_msg.clone());
            }
            return;
        }
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(&text);
        }
        let mut short: String = text.chars().take(200).collect();
        if text.chars().count() > 200 {
            short.push_str("...");
        }
        cfg.show("screen-ocr", &format!("[{lang}] {short}"));
    });
}

/// `flameshot gui` then read the clipboard image and save as BMP.
fn acquire_flameshot(img: &Path) -> bool {
    let ok = std::process::Command::new("flameshot")
        .arg("gui")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return false;
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    let Ok(mut cb) = arboard::Clipboard::new() else {
        return false;
    };
    let Ok(image) = cb.get_image() else {
        return false;
    };
    capture::write_bmp(img, image.width as u32, image.height as u32, &image.bytes)
}

/// Run the CLI (AHK `convert()`): wait until the output file exists or the
/// process dies. Returns the trimmed text (CRLF-normalized), if any.
fn convert(script: &Path, img: &Path, out: &Path, lang: &str) -> Option<String> {
    let cmd = format!(
        "uv run \"{}\" \"{}\" \"{}\" {} --use-angle-cls",
        script.display(),
        img.display(),
        out.display(),
        lang
    );
    let mut child = std::process::Command::new("cmd")
        .args(["/C", &cmd])
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        if out.exists() {
            break;
        }
        match child.try_wait() {
            Ok(Some(_)) => break, // process exited (AHK: Process Exist fails)
            Ok(None) => {}        // still running
            Err(_) => break,
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let text = std::fs::read_to_string(out).ok()?;
    let text = text.trim().replace('\n', "\r\n");
    Some(text)
}

/// "Check Languages" menu item: `uv run rapidocr_cli.py --list-langs`,
/// save to `cache/rapidocr_lang.txt`, notify, offer to open the folder.
pub fn check_langs(script: &Path, cache_dir: &Path, cfg: &Arc<notify::Config>) {
    cfg.show("screen-ocr", "Checking languages... RapidOCR loading.");
    let cmd = format!("uv run \"{}\" --list-langs", script.display());
    let out = std::process::Command::new("cmd")
        .args(["/C", &cmd])
        .output();
    let clean: Option<String> = out.ok().map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("Installed "))
            .collect::<Vec<_>>()
            .join(", ")
    });
    match clean {
        Some(clean) => {
            let cache_path = cache_dir.join("rapidocr_lang.txt");
            let _ = std::fs::write(&cache_path, &clean);
            let msg = if clean.is_empty() {
                "RapidOCR: none".to_string()
            } else {
                let mut m = format!("RapidOCR: {clean}");
                if m.chars().count() > 50 {
                    m = m.chars().take(47).collect::<String>() + "...";
                }
                m
            };
            cfg.show("screen-ocr", &msg);
            if kit::win::message_box_yesno(
                "screen-ocr",
                "Language lists saved. Open folder to view?",
            ) {
                kit::win::open_folder(cache_dir);
            }
        }
        None => cfg.show("screen-ocr", "RapidOCR: failed (see README)"),
    }
}
