//! Notification helper (AHK ShowNotification).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub struct Config {
    /// "traytip" (shown via `center_tip`, see crate docs) or "snoretoast".
    pub method: String,
    pub enabled: AtomicBool,
    pub recog_msg: String,
    pub no_text_msg: String,
    pub icon_png: PathBuf,
}

impl Config {
    pub fn show(&self, title: &str, msg: &str) {
        if !self.enabled.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if self.method == "snoretoast" {
            let _ = std::process::Command::new("snoretoast")
                .args(["-t", title, "-m", msg, "-silent", "-p"])
                .arg(&self.icon_png)
                .spawn();
        } else {
            kit::win::center_tip(msg, Duration::from_secs(3));
        }
    }
}
