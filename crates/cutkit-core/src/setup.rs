//! Persiapan untuk pemula (dipakai installer dan aplikasi): Node.js portabel untuk
//! server MCP, pemeriksaan WebView2, dan unduhan umum dengan cek SHA-256.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const NODE_DIST: &str = "https://nodejs.org/dist/";
const WEBVIEW2_BOOTSTRAPPER: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
const WEBVIEW2_CLIENT: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

fn sys(name: &str) -> PathBuf {
    if !cfg!(windows) {
        return PathBuf::from(name.trim_end_matches(".exe"));
    }
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    PathBuf::from(root).join("System32").join(name)
}

fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn get_text(url: &str) -> Result<String, String> {
    ureq::get(url).call().map_err(|e| format!("Gagal membuka {url}: {e}"))?.into_body().read_to_string().map_err(|e| e.to_string())
}

/// Mengunduh `url` ke `dest` (lewat file .part). Bila `sha256` diisi, isi file harus cocok.
pub fn download(url: &str, dest: &Path, sha256: Option<&str>, mut on_progress: impl FnMut(u64, u64)) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err(format!("Hanya unduhan HTTPS yang diizinkan: {url}"));
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let resp = ureq::get(url).call().map_err(|e| format!("Gagal mengunduh {url}: {e}"))?;
    let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_body().into_reader();
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut got = 0u64;
    let mut last = std::time::Instant::now();
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Unduhan terputus: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        got += n as u64;
        if last.elapsed().as_millis() > 200 {
            on_progress(got, total);
            last = std::time::Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    let hash: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if let Some(want) = sha256 {
        if !hash.eq_ignore_ascii_case(want) {
            let _ = std::fs::remove_file(&part);
            return Err(format!("Checksum SHA-256 tidak cocok untuk {url} (diharapkan {want}, didapat {hash})."));
        }
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    on_progress(got, got);
    Ok(())
}

/// Folder induk Node.js portabel: folder program di Windows (dihapus uninstaller), folder data di
/// Linux/macOS (folder program di sana tidak bisa ditulis).
pub fn node_home(app_dir: Option<&Path>) -> PathBuf {
    match app_dir {
        Some(d) if cfg!(windows) => d.to_path_buf(),
        _ => crate::paths::data_dir(),
    }
}

/// Node.js portabel milik CutKit: `<home>\node\node.exe` (Windows) atau `<home>/node/bin/node`.
pub fn bundled_node(home: &Path) -> PathBuf {
    if cfg!(windows) { home.join("node").join("node.exe") } else { home.join("node").join("bin").join("node") }
}

/// Nama paket Node.js untuk OS/arsitektur ini: (kunci di index.json, akhiran nama arsip).
fn node_platform() -> (&'static str, &'static str) {
    let arm = cfg!(target_arch = "aarch64");
    if cfg!(windows) {
        ("win-x64-zip", "win-x64.zip")
    } else if cfg!(target_os = "macos") {
        if arm { ("osx-arm64-tar", "darwin-arm64.tar.gz") } else { ("osx-x64-tar", "darwin-x64.tar.gz") }
    } else if arm {
        ("linux-arm64", "linux-arm64.tar.xz")
    } else {
        ("linux-x64", "linux-x64.tar.xz")
    }
}

/// Versi LTS terbaru untuk OS ini dari nodejs.org/dist/index.json.
pub fn latest_node_lts(index_json: &str) -> Option<String> {
    latest_node_lts_for(index_json, node_platform().0)
}

fn latest_node_lts_for(index_json: &str, key: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(index_json).ok()?;
    v.as_array()?
        .iter()
        .filter(|r| r["lts"].as_str().is_some() && r["files"].as_array().map(|f| f.iter().any(|x| x == key)).unwrap_or(false))
        .filter_map(|r| r["version"].as_str())
        .map(|s| s.trim_start_matches('v').to_string())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .max_by_key(|s| s.split('.').map(|x| x.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>())
}

/// Checksum `name` dari isi SHASUMS256.txt.
pub fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, n) = (it.next()?, it.next()?);
        (n == name).then(|| h.to_string())
    })
}

/// Memasang Node.js LTS portabel (arsip resmi, dicek SHA-256) ke `<home>/node`.
/// Tidak menyentuh PATH maupun Node.js lain di komputer. Mengembalikan versi.
pub fn install_node(app_dir: &Path, mut on_progress: impl FnMut(u64, u64)) -> Result<String, String> {
    let ver = latest_node_lts(&get_text(&format!("{NODE_DIST}index.json"))?).ok_or("Versi LTS Node.js tidak ditemukan di nodejs.org")?;
    let suffix = node_platform().1;
    let zip_name = format!("node-v{ver}-{suffix}");
    let base = format!("{NODE_DIST}v{ver}/");
    let want = checksum_for(&get_text(&format!("{base}SHASUMS256.txt"))?, &zip_name).ok_or("Checksum Node.js tidak ditemukan di SHASUMS256.txt")?;
    let work = app_dir.join("downloads");
    let zip = work.join(&zip_name);
    download(&format!("{base}{zip_name}"), &zip, Some(&want), &mut on_progress)?;
    let tmp = work.join(format!("node-{ver}"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let st = hidden(Command::new(sys("tar.exe")).arg("-xf").arg(&zip).arg("-C").arg(&tmp)).status().map_err(|e| format!("tar.exe gagal: {e}"))?;
    let inner = tmp.join(zip_name.trim_end_matches(".zip").trim_end_matches(".tar.gz").trim_end_matches(".tar.xz"));
    let probe = if cfg!(windows) { inner.join("node.exe") } else { inner.join("bin").join("node") };
    if !st.success() || !probe.is_file() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err("Gagal mengekstrak zip Node.js.".into());
    }
    let dest = app_dir.join("node");
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&inner, &dest).map_err(|e| format!("Tidak bisa memindahkan Node.js ke {}: {e}", dest.display()))?;
    let _ = std::fs::remove_dir_all(&work);
    Ok(ver)
}

/// WebView2 Runtime (mesin tampilan CutKit). Sudah ada di Windows 11 dan sebagian besar Windows 10.
pub fn webview2_installed() -> bool {
    if !cfg!(windows) {
        return true;
    }
    let keys = [
        format!(r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}"),
        format!(r"HKLM\SOFTWARE\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}"),
        format!(r"HKCU\SOFTWARE\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}"),
    ];
    keys.iter().any(|k| {
        hidden(Command::new(sys("reg.exe")).args(["query", k, "/v", "pv"]))
            .output()
            .map(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).contains("0.0.0.0"))
            .unwrap_or(false)
    })
}

/// Memasang WebView2 Runtime dengan bootstrapper resmi Microsoft (ditandatangani Microsoft).
pub fn install_webview2(on_progress: impl FnMut(u64, u64)) -> Result<(), String> {
    let exe = std::env::temp_dir().join("cutkit-webview2").join("MicrosoftEdgeWebview2Setup.exe");
    download(WEBVIEW2_BOOTSTRAPPER, &exe, None, on_progress)?;
    let st = Command::new(&exe).args(["/silent", "/install"]).status().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(exe.parent().unwrap());
    if !st.success() {
        return Err(format!("Pemasang WebView2 keluar dengan kode {:?}", st.code()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_newest_lts_with_windows_zip() {
        let idx = r#"[
            {"version":"v27.1.0","lts":false,"files":["win-x64-zip"]},
            {"version":"v24.11.1","lts":"Krypton","files":["win-x64-zip","win-x64-msi"]},
            {"version":"v24.9.0","lts":"Krypton","files":["win-x64-zip"]},
            {"version":"v22.21.0","lts":"Jod","files":["win-x64-zip"]},
            {"version":"v24.12.0","lts":"Krypton","files":["linux-x64"]},
            {"version":"v1.0.0;rm","lts":"X","files":["win-x64-zip"]}
        ]"#;
        assert_eq!(latest_node_lts_for(idx, "win-x64-zip").as_deref(), Some("24.11.1"));
        assert_eq!(latest_node_lts_for(idx, "linux-x64"), Some("24.12.0".into()), "kunci per OS");
        assert_eq!(latest_node_lts("[]"), None);
    }

    #[test]
    fn finds_checksum_line() {
        let sums = "aaa  node-v24.11.1-win-x86.zip\nbbb  node-v24.11.1-win-x64.zip\n";
        assert_eq!(checksum_for(sums, "node-v24.11.1-win-x64.zip").as_deref(), Some("bbb"));
        assert_eq!(checksum_for(sums, "nope.zip"), None);
    }

    #[test]
    fn refuses_plain_http() {
        assert!(download("http://example.com/x", &std::env::temp_dir().join("x"), None, |_, _| {}).is_err());
    }
}
