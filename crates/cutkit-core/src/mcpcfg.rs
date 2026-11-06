//! Mendaftarkan server MCP CutKit ke file konfigurasi klien MCP (Claude Code:
//! `~/.claude.json`, Claude Desktop: `claude_desktop_config.json`). Hanya kunci
//! `mcpServers.<nama>` yang diubah; isi lain dan urutannya dibiarkan apa adanya.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

fn read(path: &Path) -> Result<Map<String, Value>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(Map::new()),
        Ok(text) => match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
            Ok(Value::Object(m)) => Ok(m),
            Ok(_) => Err(format!("{} bukan objek JSON.", path.display())),
            Err(e) => Err(format!("{} tidak bisa dibaca sebagai JSON ({e}); tidak diubah.", path.display())),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => Err(format!("Tidak bisa membaca {}: {e}", path.display())),
    }
}

fn write(path: &Path, root: &Map<String, Value>) -> Result<Option<PathBuf>, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Tidak bisa membuat {}: {e}", dir.display()))?;
    }
    let backup = if path.exists() {
        let b = PathBuf::from(format!("{}.cutkit-backup", path.display()));
        std::fs::copy(path, &b).map_err(|e| format!("Tidak bisa membuat cadangan {}: {e}", b.display()))?;
        Some(b)
    } else {
        None
    };
    let tmp = PathBuf::from(format!("{}.cutkit-tmp", path.display()));
    let text = serde_json::to_string_pretty(&Value::Object(root.clone())).unwrap() + "\n";
    std::fs::write(&tmp, text).map_err(|e| format!("Tidak bisa menulis {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Tidak bisa mengganti {}: {e}", path.display()))?;
    Ok(backup)
}

/// Entri server yang terdaftar (bila ada).
pub fn entry(path: &Path, name: &str) -> Option<Value> {
    read(path).ok()?.get("mcpServers")?.get(name).cloned()
}

/// Mendaftarkan (atau memperbarui) server stdio. Mengembalikan path cadangan file lama.
pub fn install(path: &Path, name: &str, command: &str, args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut root = read(path)?;
    let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
    let Value::Object(servers) = servers else {
        return Err(format!("mcpServers di {} bukan objek; tidak diubah.", path.display()));
    };
    servers.insert(name.to_string(), json!({ "type": "stdio", "command": command, "args": args, "env": {} }));
    write(path, &root)
}

/// Menghapus pendaftaran. Mengembalikan false bila memang belum terdaftar.
pub fn uninstall(path: &Path, name: &str) -> Result<bool, String> {
    let mut root = read(path)?;
    let removed = root.get_mut("mcpServers").and_then(|s| s.as_object_mut()).and_then(|s| s.remove(name)).is_some();
    if removed {
        write(path, &root)?;
    }
    Ok(removed)
}

/// Mencari node.exe: Node.js portabel CutKit (`<app_dir>\node`), lalu PATH dan lokasi pemasangan umum.
pub fn find_node(app_dir: Option<&Path>) -> Option<String> {
    let exe = if cfg!(windows) { "node.exe" } else { "node" };
    let mut dirs: Vec<PathBuf> = app_dir.map(|d| vec![d.join("node")]).unwrap_or_default();
    dirs.extend(std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default());
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(d) = std::env::var(var) {
                dirs.push(Path::new(&d).join("nodejs"));
            }
        }
        if let Ok(d) = std::env::var("LOCALAPPDATA") {
            dirs.push(Path::new(&d).join("Programs").join("nodejs"));
        }
    }
    dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file()).map(|p| p.to_string_lossy().to_string())
}

/// Lokasi file konfigurasi klien: (id, nama, path).
pub fn clients() -> Vec<(&'static str, &'static str, PathBuf)> {
    let mut v = vec![];
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    if let Some(h) = &home {
        v.push(("claude-code", "Claude Code (juga tab Code di aplikasi Claude)", h.join(".claude.json")));
    }
    // Claude Desktop versi Microsoft Store menyimpan AppData di folder paketnya.
    let mut desktop = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Claude").join("claude_desktop_config.json"));
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        if let Ok(entries) = std::fs::read_dir(PathBuf::from(local).join("Packages")) {
            for e in entries.flatten() {
                let roaming = e.path().join("LocalCache").join("Roaming").join("Claude");
                if e.file_name().to_string_lossy().starts_with("Claude_") && roaming.is_dir() {
                    desktop = Some(roaming.join("claude_desktop_config.json"));
                }
            }
        }
    }
    if let Some(d) = desktop {
        v.push(("claude-desktop", "Claude Desktop (chat)", d));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cutkit-mcpcfg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("config.json")
    }

    #[test]
    fn install_keeps_other_keys_in_order() {
        let f = tmp("keep");
        std::fs::write(&f, "{\n  \"zeta\": 1,\n  \"mcpServers\": { \"rea\": { \"command\": \"rea\" } },\n  \"alpha\": [true]\n}").unwrap();
        let backup = install(&f, "cutkit", "node", &["D:/x/server.js".into()]).unwrap().unwrap();
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(text.find("zeta").unwrap() < text.find("alpha").unwrap(), "urutan kunci tetap: {text}");
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["mcpServers"]["rea"]["command"], "rea");
        assert_eq!(v["mcpServers"]["cutkit"]["args"][0], "D:/x/server.js");
        assert_eq!(v["alpha"][0], true);
        assert!(std::fs::read_to_string(backup).unwrap().contains("\"zeta\": 1"));
        assert!(entry(&f, "cutkit").is_some());
        assert!(uninstall(&f, "cutkit").unwrap());
        assert!(!uninstall(&f, "cutkit").unwrap());
        assert!(entry(&f, "cutkit").is_none());
        assert!(entry(&f, "rea").is_some());
    }

    #[test]
    fn creates_missing_file_and_refuses_broken_json() {
        let f = tmp("new");
        std::fs::remove_dir_all(f.parent().unwrap()).unwrap();
        assert!(install(&f, "cutkit", "node", &[]).unwrap().is_none());
        assert!(entry(&f, "cutkit").is_some());
        let g = tmp("broken");
        std::fs::write(&g, "{ rusak").unwrap();
        assert!(install(&g, "cutkit", "node", &[]).is_err());
        assert_eq!(std::fs::read_to_string(&g).unwrap(), "{ rusak", "file rusak tidak disentuh");
    }
}
