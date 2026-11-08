//! Menemukan dan menjalankan ffmpeg/ffprobe.

use crate::export::{ExportJob, ExportPlan, ExportStep, ASS_FILE, FILTER_FILE};
use crate::probe::{parse_ffprobe, probe_args, MediaInfo};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Tools {
    pub ffmpeg: String,
    pub ffprobe: String,
    pub version: String,
    pub has_qsv: bool,
}

#[cfg(windows)]
fn quiet(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
fn quiet(cmd: &mut Command) -> &mut Command {
    cmd
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Mencari ffmpeg: path yang diberikan, PATH, lalu folder paket winget (Gyan.FFmpeg).
pub fn find_tools(override_dir: Option<&str>) -> Option<Tools> {
    let mut candidates: Vec<PathBuf> = vec![];
    if let Some(dir) = override_dir.filter(|d| !d.trim().is_empty()) {
        candidates.push(PathBuf::from(dir));
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir);
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let packages = Path::new(&local).join("Microsoft").join("WinGet").join("Packages");
        if let Ok(entries) = std::fs::read_dir(&packages) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.to_ascii_lowercase().contains("ffmpeg") {
                    if let Ok(inner) = std::fs::read_dir(e.path()) {
                        for i in inner.flatten() {
                            candidates.push(i.path().join("bin"));
                            candidates.push(i.path());
                        }
                    }
                }
            }
        }
        candidates.push(Path::new(&local).join("Microsoft").join("WinGet").join("Links"));
    }
    let own = local_ffmpeg_dir();
    candidates.push(own.join("bin"));
    if let Ok(entries) = std::fs::read_dir(&own) {
        for e in entries.flatten() {
            candidates.push(e.path().join("bin"));
        }
    }
    if !cfg!(windows) {
        // ffmpeg-full Homebrew (keg-only) didahulukan: ffmpeg biasa Homebrew tidak menyertakan libass.
        for d in ["/opt/homebrew/opt/ffmpeg-full/bin", "/usr/local/opt/ffmpeg-full/bin", "/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/snap/bin", "/opt/local/bin"] {
            candidates.push(PathBuf::from(d));
        }
    }
    for dir in candidates {
        let ffmpeg = dir.join(exe("ffmpeg"));
        let ffprobe = dir.join(exe("ffprobe"));
        if ffmpeg.is_file() && ffprobe.is_file() {
            let version = version_of(&ffmpeg).unwrap_or_default();
            return Some(Tools { ffmpeg: ffmpeg.to_string_lossy().into(), ffprobe: ffprobe.to_string_lossy().into(), version, has_qsv: false });
        }
    }
    None
}

fn version_of(ffmpeg: &Path) -> Option<String> {
    let out = quiet(Command::new(ffmpeg).arg("-version")).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next()?;
    Some(first.split_whitespace().nth(2).unwrap_or(first).to_string())
}

/// Mengecek apakah encoder Intel Quick Sync benar-benar bisa dipakai (bukan hanya terdaftar).
/// ffmpeg yang diunduh CutKit sendiri (Linux): <data>/ffmpeg.
pub fn local_ffmpeg_dir() -> PathBuf {
    crate::paths::data_dir().join("ffmpeg")
}

/// Versi mayor dari string versi ffmpeg ("8.0.1", "n7.1-...", "9.0.2-full_build"); build git master = 99.
pub fn major_version(version: &str) -> u32 {
    let v = version.trim_start_matches('n');
    if v.starts_with('N') || v.starts_with("git") {
        return 99;
    }
    v.split(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse().ok()).unwrap_or(99)
}

/// Apakah ffmpeg ini punya filter `name` (whisper, flite, ass, ... tergantung cara ffmpeg dibangun).
pub fn has_filter(ffmpeg: &str, name: &str) -> bool {
    quiet(Command::new(ffmpeg).args(["-hide_banner", "-filters"]))
        .stdin(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.split_whitespace().nth(1) == Some(name)))
        .unwrap_or(false)
}

/// Filter whisper (auto caption) hanya ada di ffmpeg 8+ yang dibangun dengan whisper.cpp.
pub fn has_whisper(ffmpeg: &str) -> bool {
    has_filter(ffmpeg, "whisper")
}

/// ffmpeg di bawah 7 belum mengenal `-/filter_complex <file>`; pakai `-filter_complex_script` yang setara.
fn compat_args(ffmpeg: &str, args: &[String]) -> Vec<String> {
    if !args.iter().any(|a| a == "-/filter_complex") {
        return args.to_vec();
    }
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, u32>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let major = {
        let mut c = cache.lock().unwrap();
        *c.entry(ffmpeg.to_string()).or_insert_with(|| version_of(Path::new(ffmpeg)).map(|v| major_version(&v)).unwrap_or(99))
    };
    if major >= 7 {
        return args.to_vec();
    }
    args.iter().map(|a| if a == "-/filter_complex" { "-filter_complex_script".to_string() } else { a.clone() }).collect()
}

pub fn check_qsv(ffmpeg: &str) -> bool {
    quiet(Command::new(ffmpeg).args([
        "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=black:s=256x256:r=30:d=0.2", "-pix_fmt", "nv12", "-c:v", "h264_qsv", "-f", "null", "-",
    ]))
    .stdin(Stdio::null())
    .output()
    .map(|o| o.status.success())
    .unwrap_or(false)
}

pub fn probe(ffprobe: &str, path: &str) -> Result<MediaInfo, String> {
    let out = quiet(Command::new(ffprobe).args(probe_args(path))).stdin(Stdio::null()).output().map_err(|e| format!("ffprobe gagal dijalankan: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "ffprobe tidak bisa membaca file ini.".into() } else { err });
    }
    parse_ffprobe(path, &String::from_utf8_lossy(&out.stdout))
}

/// Menjalankan ffmpeg sampai selesai; error ffmpeg dikembalikan sebagai pesan.
pub fn run(ffmpeg: &str, args: &[String]) -> Result<(), String> {
    let out = quiet(Command::new(ffmpeg).args(compat_args(ffmpeg, args))).stdin(Stdio::null()).output().map_err(|e| format!("ffmpeg gagal dijalankan: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(tail(&String::from_utf8_lossy(&out.stderr)))
    }
}

fn tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    let text = lines.iter().rev().take(6).rev().cloned().collect::<Vec<_>>().join("\n");
    if text.is_empty() {
        "ffmpeg berhenti dengan kesalahan.".into()
    } else {
        text
    }
}

/// Argumen winget untuk memasang ffmpeg (build Gyan, punya libx264, libass, flite) untuk user ini saja.
pub fn winget_install_args() -> Vec<String> {
    ["install", "--id", "Gyan.FFmpeg", "--exact", "--source", "winget", "--scope", "user", "--silent", "--disable-interactivity", "--accept-package-agreements", "--accept-source-agreements"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

pub fn has_winget() -> bool {
    quiet(Command::new("winget").arg("--version")).stdin(Stdio::null()).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Cara CutKit memasang ffmpeg di OS ini, untuk ditampilkan ke pengguna (None = tidak bisa otomatis).
pub fn installer_name() -> Option<&'static str> {
    if cfg!(windows) {
        has_winget().then_some("winget")
    } else if cfg!(target_os = "macos") {
        brew().map(|_| "Homebrew")
    } else if cfg!(target_os = "linux") && (cfg!(target_arch = "x86_64") || cfg!(target_arch = "aarch64")) {
        Some("unduhan ffmpeg statis (BtbN)")
    } else {
        None
    }
}

/// Petunjuk pasang manual per OS.
pub fn install_hint() -> &'static str {
    if cfg!(windows) {
        "winget install Gyan.FFmpeg"
    } else if cfg!(target_os = "macos") {
        "brew install ffmpeg-full"
    } else {
        "sudo apt install ffmpeg (butuh ffmpeg 7 atau lebih baru), atau klik Pasang otomatis"
    }
}

fn brew() -> Option<PathBuf> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"].iter().map(PathBuf::from).find(|p| p.is_file())
}

/// Menjalankan program dan meneruskan tiap baris keluaran (stdout, \r juga dianggap baris baru).
fn stream_lines(mut cmd: Command, mut on_line: impl FnMut(String)) -> Result<std::process::ExitStatus, String> {
    let mut child = quiet(&mut cmd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let mut buf = Vec::new();
    for byte in BufReader::new(stdout).bytes() {
        let Ok(b) = byte else { break };
        if b == b'\n' || b == b'\r' {
            let line = String::from_utf8_lossy(&buf).trim().to_string();
            buf.clear();
            if line.chars().any(|c| c.is_alphanumeric()) {
                on_line(line);
            }
        } else {
            buf.push(b);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_reader.join().unwrap_or_default();
    for l in err.lines().filter(|l| !l.trim().is_empty()).take(20) {
        on_line(l.trim().to_string());
    }
    Ok(status)
}

/// Memasang ffmpeg: winget (Windows), Homebrew (macOS), atau build statis BtbN ke folder data CutKit
/// (Linux, tanpa sudo, dicek SHA-256). Tiap baris proses diteruskan ke `on_line`.
pub fn install_ffmpeg(mut on_line: impl FnMut(String)) -> Result<(), String> {
    if cfg!(windows) {
        if !has_winget() {
            return Err("winget tidak tersedia. Pasang \"App Installer\" dari Microsoft Store, atau unduh ffmpeg manual dari gyan.dev.".into());
        }
        let mut cmd = Command::new("winget");
        cmd.args(winget_install_args());
        let status = stream_lines(cmd, &mut on_line).map_err(|e| format!("winget gagal dijalankan: {e}"))?;
        // Kode -1978335189 (0x8A15002B) = sudah terpasang versi terbaru; anggap berhasil.
        if status.success() || status.code() == Some(-1978335189) {
            return Ok(());
        }
        return Err(format!("winget berhenti dengan kode {:?}.", status.code()));
    }
    if cfg!(target_os = "macos") {
        let Some(brew) = brew() else {
            return Err("Homebrew belum terpasang. Pasang dari https://brew.sh, lalu jalankan: brew install ffmpeg-full".into());
        };
        // ffmpeg-full: termasuk libass (teks/caption) yang tidak ada di formula ffmpeg biasa.
        on_line("Menjalankan: brew install ffmpeg-full".into());
        let mut cmd = Command::new(brew);
        cmd.args(["install", "ffmpeg-full"]).env("HOMEBREW_NO_AUTO_UPDATE", "1");
        let status = stream_lines(cmd, &mut on_line).map_err(|e| format!("brew gagal dijalankan: {e}"))?;
        return if status.success() { Ok(()) } else { Err(format!("brew berhenti dengan kode {:?}.", status.code())) };
    }
    install_btbn(&mut on_line)
}

/// Build rilis stabil terbaru (bukan master) untuk arsitektur ini dari checksums.sha256 BtbN:
/// "ffmpeg-n<versi>-latest-<arch>-gpl-<versi>.tar.xz" dengan versi tertinggi.
pub fn btbn_pick(sums: &str, arch: &str) -> Option<String> {
    sums.lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .map(|n| n.trim_start_matches('*'))
        .filter(|n| n.starts_with("ffmpeg-n") && n.ends_with(".tar.xz") && n.contains(&format!("-latest-{arch}-gpl-")) && !n.contains("shared"))
        .max_by_key(|n| {
            let v = n.trim_start_matches("ffmpeg-n").split('-').next().unwrap_or("0");
            v.split('.').map(|x| x.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>()
        })
        .map(String::from)
}

/// Linux: ffmpeg statis terbaru dari BtbN/FFmpeg-Builds ke <data>/ffmpeg.
fn install_btbn(on_line: &mut dyn FnMut(String)) -> Result<(), String> {
    let arch = if cfg!(target_arch = "aarch64") { "linuxarm64" } else { "linux64" };
    let base = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/";
    on_line(format!("Mengambil daftar checksum {base}checksums.sha256"));
    let sums = ureq::get(&format!("{base}checksums.sha256")).call().map_err(|e| e.to_string())?.into_body().read_to_string().map_err(|e| e.to_string())?;
    let file = btbn_pick(&sums, arch).ok_or("Build ffmpeg statis untuk Linux tidak ditemukan di BtbN/FFmpeg-Builds.")?;
    let name = file.trim_end_matches(".tar.xz").to_string();
    let want = crate::setup::checksum_for(&sums, &file).ok_or_else(|| format!("{file} tidak ada di checksums.sha256"))?;
    let dir = local_ffmpeg_dir();
    let archive = dir.join(&file);
    on_line(format!("Mengunduh {file}..."));
    let mut last = 0u64;
    crate::setup::download(&format!("{base}{file}"), &archive, Some(&want), |got, total| {
        if got >= last + 8 * 1_048_576 || got == total {
            last = got;
            on_line(format!("{} / {} MB", got / 1_048_576, total.max(got) / 1_048_576));
        }
    })?;
    on_line("Checksum SHA-256 cocok. Mengekstrak...".into());
    let out = Command::new("tar").arg("-xJf").arg(&archive).arg("-C").arg(&dir).output().map_err(|e| format!("tar gagal dijalankan: {e}"))?;
    let _ = std::fs::remove_file(&archive);
    if !out.status.success() || !dir.join(&name).join("bin").join("ffmpeg").is_file() {
        return Err(format!("Gagal mengekstrak ffmpeg: {}", String::from_utf8_lossy(&out.stderr)));
    }
    on_line(format!("ffmpeg terpasang di {}", dir.join(&name).display()));
    Ok(())
}

#[cfg(test)]
mod platform_tests {
    use super::*;

    #[test]
    fn picks_newest_stable_btbn_build() {
        let sums = "aa  ffmpeg-master-latest-linux64-gpl.tar.xz\nbb  ffmpeg-n8.1-latest-linux64-gpl-8.1.tar.xz\ncc  ffmpeg-n9.0-latest-linux64-gpl-9.0.tar.xz\ndd  ffmpeg-n9.0-latest-linuxarm64-gpl-9.0.tar.xz\nee  ffmpeg-n9.0-latest-linux64-gpl-shared-9.0.tar.xz\nff  ffmpeg-n10.0-latest-win64-gpl-10.0.zip";
        assert_eq!(btbn_pick(sums, "linux64").as_deref(), Some("ffmpeg-n9.0-latest-linux64-gpl-9.0.tar.xz"));
        assert_eq!(btbn_pick(sums, "linuxarm64").as_deref(), Some("ffmpeg-n9.0-latest-linuxarm64-gpl-9.0.tar.xz"));
        assert_eq!(btbn_pick("", "linux64"), None);
    }

    #[test]
    fn parses_major_versions() {
        assert_eq!(major_version("9.0.2-full_build-www.gyan.dev"), 9);
        assert_eq!(major_version("n7.1-153-gaeb8631048"), 7);
        assert_eq!(major_version("4.4.2-0ubuntu0.22.04.1"), 4);
        assert_eq!(major_version("N-118960-g1ba5a5d4c8-20250312"), 99);
        assert_eq!(compat_args("ffmpeg-tidak-ada", &["-i".into(), "x".into()]), vec!["-i".to_string(), "x".to_string()]);
    }
}

/// Total ukuran file dalam folder (rekursif), untuk info cache.
pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Progress {
    pub seconds: f64,
    pub percent: f64,
    pub speed: String,
    pub done: bool,
    /// Langkah yang sedang berjalan (mis. "Video bagian 2/5").
    pub step: String,
}

/// Menjalankan rencana sekali jalan (dipakai tes dan contoh render).
pub fn run_export(ffmpeg: &str, work_dir: &Path, plan: &ExportPlan, cancel: Arc<AtomicBool>, on_progress: impl FnMut(Progress)) -> Result<(), String> {
    let mut files = vec![(FILTER_FILE.to_string(), plan.filter_script.clone())];
    if let Some(a) = &plan.ass {
        files.push((ASS_FILE.to_string(), a.clone()));
    }
    let output = plan.args.last().cloned().unwrap_or_default();
    let job = ExportJob {
        steps: vec![ExportStep { label: "Ekspor".into(), args: plan.args.clone(), files, copies: plan.copies.clone(), duration: plan.duration, weight: 1.0 }],
        duration: plan.duration,
        output,
        chunked: false,
    };
    run_job(ffmpeg, work_dir, &job, cancel, on_progress)
}

/// Menjalankan semua langkah ekspor berurutan di `work_dir` dengan satu progres gabungan.
/// `cancel` yang di-set true mematikan ffmpeg dan menghapus file hasil yang belum jadi.
pub fn run_job(ffmpeg: &str, work_dir: &Path, job: &ExportJob, cancel: Arc<AtomicBool>, mut on_progress: impl FnMut(Progress)) -> Result<(), String> {
    std::fs::create_dir_all(work_dir).map_err(|e| format!("Tidak bisa membuat folder kerja: {e}"))?;
    if let Some(parent) = Path::new(&job.output).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Tidak bisa membuat folder tujuan: {e}"))?;
        }
    }
    let mut done = 0.0;
    let mut speed = String::new();
    for step in &job.steps {
        if cancel.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&job.output);
            return Err("Ekspor dibatalkan.".into());
        }
        let weight = step.weight;
        let label = step.label.clone();
        let result = run_step(ffmpeg, work_dir, step, cancel.clone(), |sec, sp| {
            speed = sp.to_string();
            let frac = if step.duration > 0.0 { (sec / step.duration).clamp(0.0, 1.0) } else { 0.0 };
            let percent = ((done + weight * frac) * 100.0).clamp(0.0, 100.0);
            on_progress(Progress { seconds: percent / 100.0 * job.duration, percent, speed: sp.to_string(), done: false, step: label.clone() });
        });
        if let Err(e) = result {
            let _ = std::fs::remove_file(&job.output);
            return Err(if job.steps.len() > 1 && !e.contains("dibatalkan") { format!("{}: {e}", step.label) } else { e });
        }
        done += weight;
    }
    on_progress(Progress { seconds: job.duration, percent: 100.0, speed, done: true, step: "Selesai".into() });
    Ok(())
}

/// Satu proses ffmpeg dengan laporan (detik keluaran, kecepatan).
fn run_step(ffmpeg: &str, work_dir: &Path, step: &ExportStep, cancel: Arc<AtomicBool>, mut on_progress: impl FnMut(f64, &str)) -> Result<(), String> {
    for (name, content) in &step.files {
        std::fs::write(work_dir.join(name), content).map_err(|e| e.to_string())?;
    }
    for (name, src) in &step.copies {
        std::fs::copy(src, work_dir.join(name)).map_err(|e| format!("Tidak bisa menyalin {src}: {e}"))?;
    }
    run_ffmpeg_progress(ffmpeg, work_dir, &step.args, cancel, |sec, sp| on_progress(sec, sp))
}

pub fn run_ffmpeg_progress(ffmpeg: &str, work_dir: &Path, args: &[String], cancel: Arc<AtomicBool>, mut on_progress: impl FnMut(f64, &str)) -> Result<(), String> {
    let mut child = quiet(Command::new(ffmpeg).args(compat_args(ffmpeg, args)).current_dir(work_dir))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg gagal dijalankan: {e}"))?;
    let mut stderr = child.stderr.take().expect("stderr");
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let stdout = child.stdout.take().expect("stdout");
    let reader = BufReader::new(stdout);
    let mut seconds = 0.0;
    let mut speed = String::new();
    let mut cancelled = cancel.load(Ordering::Relaxed);
    if cancelled {
        let _ = child.kill();
    }
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            let _ = child.kill();
            break;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "out_time_us" | "out_time_ms" => {
                    if let Ok(us) = value.trim().parse::<i64>() {
                        seconds = us.max(0) as f64 / 1_000_000.0;
                    }
                }
                "speed" => speed = value.trim().to_string(),
                "progress" => on_progress(seconds, &speed),
                _ => {}
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let stderr_text = err_reader.join().unwrap_or_default();
    if cancelled {
        return Err("Ekspor dibatalkan.".into());
    }
    if !status.success() {
        return Err(tail(&stderr_text));
    }
    Ok(())
}
