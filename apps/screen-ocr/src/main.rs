//! screen-ocr: press a hotkey, select a screen region, recognized text lands
//! in the clipboard. RapidOCR (PaddleOCR ONNX) via `uv run`, ported from
//! AHK-ScreenOCR.
//!
//! Parity notes:
//! - The AHK builtin selector is a full GDI+ port of Vis2; here it is a lean
//!   Win32 overlay (see `capture.rs`), saved as BMP (rapidocr/cv2 reads BMP).
//! - `traytip` notifications become `kit::win::center_tip` (tray-icon has no
//!   balloon API); `snoretoast` still shells out to snoretoast, as AHK does.
//! - The AHK "preprocess" step is a plain file copy; skipped.

#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod notify;
#[cfg(windows)]
mod ocr;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = app::run() {
            kit::win::message_box("screen-ocr", &format!("Error:\n{e:#}"));
            return Err(e);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("screen-ocr")
    }
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct OcrSection {
    global_hotkey: Option<String>,
    notify_method: Option<String>,
    notify: Option<bool>,
    screenshot_tool: Option<String>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct RapidSection {
    hotkey: Option<String>,
    languages: Option<Vec<String>>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct NotifyMessages {
    recog_msg: Option<String>,
    no_text_msg: Option<String>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct FileConfig {
    ocr: Option<OcrSection>,
    rapidocr: Option<RapidSection>,
    rapidocr_lang_hotkey: Option<std::collections::HashMap<String, String>>,
    notify_messages: Option<NotifyMessages>,
}

#[cfg(windows)]
mod app;
