//! TOML config created next to the executable on first run (keymap-rs pattern).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// `path` if it exists, else the default TOML written to `path` first.
///
/// `default_toml` is embedded by the app with `include_str!("../appname.toml")`.
pub fn load_or_create(path: &Path, default_toml: &str) -> Result<String> {
    if !path.exists() {
        std::fs::write(path, default_toml)
            .with_context(|| format!("cannot create default {}", path.display()))?;
    }
    std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

/// `<exe_dir>/<name>` — the config lives next to the exe, like the AHK tools' INI.
pub fn exe_dir_file(name: &str) -> Result<PathBuf> {
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    let dir = exe.parent().context("executable has no parent directory")?;
    Ok(dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_default_then_loads() {
        let dir = std::env::temp_dir().join(format!("kit-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.toml");
        let _ = std::fs::remove_file(&path);

        let text = load_or_create(&path, "answer = 42\n").unwrap();
        assert_eq!(text, "answer = 42\n");
        assert!(path.exists());

        // Second call loads the (possibly edited) file as-is.
        std::fs::write(&path, "answer = 43\n").unwrap();
        let text = load_or_create(&path, "answer = 42\n").unwrap();
        assert_eq!(text, "answer = 43\n");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
