//! Folder data dan konfigurasi CutKit per OS, sama dengan app_local_data_dir/app_config_dir Tauri
//! untuk identifier io.github.taufan759.cutkit, supaya aplikasi, cutkit-cli, dan server MCP
//! memakai folder model dan pengaturan yang sama.

use std::path::PathBuf;

pub const IDENTIFIER: &str = "io.github.taufan759.cutkit";

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// Data lokal (model AI, Node.js portabel, ffmpeg unduhan):
/// Windows %LOCALAPPDATA%, macOS ~/Library/Application Support, Linux $XDG_DATA_HOME atau ~/.local/share.
pub fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".local").join("share"))
    };
    base.join(IDENTIFIER)
}

/// Pengaturan dan template buatan sendiri:
/// Windows %APPDATA%, macOS ~/Library/Application Support, Linux $XDG_CONFIG_HOME atau ~/.config.
pub fn config_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".config"))
    };
    base.join(IDENTIFIER)
}

/// Folder model AI: pengaturan `models_dir` di settings.json bila diisi, kalau tidak data_dir()/models.
pub fn models_dir() -> PathBuf {
    if let Ok(text) = std::fs::read_to_string(config_dir().join("settings.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(d) = v["models_dir"].as_str().filter(|d| !d.trim().is_empty()) {
                return PathBuf::from(d);
            }
        }
    }
    data_dir().join("models")
}

/// Folder font bawaan (OFL) di antara kandidat: folder resource aplikasi terpasang atau ui/fonts di repo.
pub fn find_fonts_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|d| d.join("Anton-Regular.ttf").is_file()).cloned()
}

/// Nama file program untuk OS ini (".exe" di Windows).
pub fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_end_with_identifier() {
        assert!(data_dir().ends_with(IDENTIFIER));
        assert!(config_dir().ends_with(IDENTIFIER));
        assert!(models_dir().is_absolute());
        assert_eq!(exe("ffmpeg").starts_with("ffmpeg"), true);
    }
}
