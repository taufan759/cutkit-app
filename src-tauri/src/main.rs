// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cutkit_core::ffmpeg;
use cutkit_core::proxy::{filmstrip_args, filmstrip_count, proxy_args, proxy_extension, waveform_args};
use cutkit_core::{build_plan, AssetKind, ExportOptions, MediaInfo, Project, Tools};
use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

struct AppState {
    tools: Mutex<Option<Tools>>,
    cache_dir: PathBuf,
    export_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl AppState {
    fn tools(&self) -> Result<Tools, String> {
        let mut guard = self.tools.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            *guard = ffmpeg::find_tools(None);
        }
        guard.clone().ok_or_else(|| "ffmpeg tidak ditemukan. Pasang dengan: winget install Gyan.FFmpeg, lalu buka ulang CutKit.".to_string())
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// Info ffmpeg; pengecekan Quick Sync dijalankan sekali di sini karena butuh sepersekian detik.
#[tauri::command]
async fn tools_info(state: State<'_, AppState>) -> Result<Tools, String> {
    let mut tools = state.tools()?;
    if !tools.has_qsv {
        let ff = tools.ffmpeg.clone();
        tools.has_qsv = blocking(move || Ok(ffmpeg::check_qsv(&ff))).await?;
        if let Ok(mut guard) = state.tools.lock() {
            *guard = Some(tools.clone());
        }
    }
    Ok(tools)
}

#[derive(Serialize)]
struct Probed {
    path: String,
    info: Option<MediaInfo>,
    error: Option<String>,
}

#[tauri::command]
async fn probe_media(paths: Vec<String>, state: State<'_, AppState>) -> Result<Vec<Probed>, String> {
    let tools = state.tools()?;
    blocking(move || {
        Ok(paths
            .into_iter()
            .map(|path| match ffmpeg::probe(&tools.ffprobe, &path) {
                Ok(info) => Probed { path, info: Some(info), error: None },
                Err(e) => Probed { path, info: None, error: Some(e) },
            })
            .collect())
    })
    .await
}

/// Nama file cache stabil untuk satu file sumber (berubah bila file diubah).
fn cache_name(path: &str, suffix: &str) -> String {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(&mut h);
        if let Ok(m) = meta.modified() {
            m.hash(&mut h);
        }
    }
    let stem = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("media");
    let stem: String = stem.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').take(32).collect();
    format!("{stem}-{:016x}.{suffix}", h.finish())
}

/// Membuat proxy kecil untuk preview; hasil disimpan di cache dan dipakai ulang.
#[tauri::command]
async fn make_proxy(path: String, kind: AssetKind, state: State<'_, AppState>) -> Result<String, String> {
    let tools = state.tools()?;
    let dir = state.cache_dir.join("proxy");
    blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dst = dir.join(cache_name(&path, proxy_extension(kind)));
        if !dst.is_file() {
            let tmp = dst.with_extension(format!("tmp.{}", proxy_extension(kind)));
            ffmpeg::run(&tools.ffmpeg, &proxy_args(&path, &tmp.to_string_lossy(), kind))?;
            std::fs::rename(&tmp, &dst).map_err(|e| e.to_string())?;
        }
        Ok(dst.to_string_lossy().to_string())
    })
    .await
}

#[tauri::command]
async fn make_waveform(path: String, state: State<'_, AppState>) -> Result<String, String> {
    let tools = state.tools()?;
    let dir = state.cache_dir.join("wave");
    blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dst = dir.join(cache_name(&path, "png"));
        if !dst.is_file() {
            ffmpeg::run(&tools.ffmpeg, &waveform_args(&path, &dst.to_string_lossy(), 1600, 60))?;
        }
        Ok(dst.to_string_lossy().to_string())
    })
    .await
}

#[derive(Serialize)]
struct Filmstrip {
    path: String,
    count: u32,
}

/// Strip gambar mini untuk klip video di timeline.
#[tauri::command]
async fn make_filmstrip(path: String, duration: f64, state: State<'_, AppState>) -> Result<Filmstrip, String> {
    let tools = state.tools()?;
    let dir = state.cache_dir.join("thumbs");
    blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dst = dir.join(cache_name(&path, "jpg"));
        if !dst.is_file() {
            ffmpeg::run(&tools.ffmpeg, &filmstrip_args(&path, &dst.to_string_lossy(), duration, 54))?;
        }
        Ok(Filmstrip { path: dst.to_string_lossy().to_string(), count: filmstrip_count(duration) })
    })
    .await
}

#[tauri::command]
fn load_project(path: String) -> Result<Project, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Tidak bisa membuka proyek: {e}"))?;
    Project::from_json(&text)
}

/// Ditulis ke file sementara dulu supaya proyek tidak rusak bila listrik mati di tengah jalan.
/// Disimpan apa adanya (bukan lewat struct) agar data tambahan UI seperti filmstrip ikut tersimpan.
#[tauri::command]
fn save_project(path: String, project: serde_json::Value) -> Result<(), String> {
    serde_json::from_value::<Project>(project.clone()).map_err(|e| format!("Data proyek tidak valid: {e}"))?;
    let text = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    write_atomic(Path::new(&path), &text)
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("cutkit-tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("Gagal menyimpan: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Gagal menyimpan: {e}"))
}

/// Untuk ekspor caption (.srt / JSON kata).
#[tauri::command]
fn write_text(path: String, text: String) -> Result<(), String> {
    write_atomic(Path::new(&path), &text)
}

#[tauri::command]
fn read_text(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Tidak bisa membaca file: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.trim_start_matches('\u{feff}').to_string())
}

#[derive(Serialize)]
struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
}

/// Isi folder untuk panel pustaka (hanya media yang dikenal + subfolder).
#[tauri::command]
fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    const EXT: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "m4v", "gif", "png", "jpg", "jpeg", "webp", "bmp", "mp3", "wav", "m4a", "aac", "ogg", "opus", "flac", "wma"];
    let mut out = vec![];
    for e in std::fs::read_dir(&path).map_err(|e| format!("Folder tidak bisa dibuka: {e}"))?.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = p.is_dir();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if is_dir || EXT.contains(&ext.as_str()) {
            out.push(DirEntry { name, path: p.to_string_lossy().to_string(), is_dir });
        }
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

#[tauri::command]
fn path_exists(path: String) -> bool {
    Path::new(&path).exists()
}

/// Memulai ekspor di thread latar; progres dikirim lewat event `export-progress`,
/// hasil akhir lewat `export-done` ({ ok, output, error }).
#[tauri::command]
fn export_start(app: AppHandle, project: Project, options: ExportOptions, state: State<AppState>) -> Result<(), String> {
    let tools = state.tools()?;
    let plan = build_plan(&project, &options)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = state.export_cancel.lock().map_err(|e| e.to_string())?;
        if slot.is_some() {
            return Err("Ekspor lain masih berjalan.".into());
        }
        *slot = Some(cancel.clone());
    }
    let work = state.cache_dir.join("export").join(format!("{}", std::process::id()));
    std::thread::spawn(move || {
        let app2 = app.clone();
        let result = ffmpeg::run_export(&tools.ffmpeg, &work, &plan, cancel, move |p| {
            let _ = app2.emit("export-progress", p);
        });
        let _ = std::fs::remove_dir_all(&work);
        if let Some(st) = app.try_state::<AppState>() {
            if let Ok(mut slot) = st.export_cancel.lock() {
                *slot = None;
            }
        }
        let payload = match result {
            Ok(()) => serde_json::json!({ "ok": true, "output": options.output }),
            Err(e) => serde_json::json!({ "ok": false, "error": e }),
        };
        let _ = app.emit("export-done", payload);
    });
    Ok(())
}

#[tauri::command]
fn export_cancel(state: State<AppState>) -> Result<(), String> {
    if let Some(flag) = state.export_cancel.lock().map_err(|e| e.to_string())?.as_ref() {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Perintah ffmpeg lengkap untuk disalin pengguna (berguna saat debugging).
#[tauri::command]
fn export_preview_command(project: Project, options: ExportOptions) -> Result<serde_json::Value, String> {
    let plan = build_plan(&project, &options)?;
    Ok(serde_json::json!({ "args": plan.args, "filter": plan.filter_script, "ass": plan.ass }))
}

#[tauri::command]
fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_path(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let cache_dir = app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir().join("cutkit"));
            app.manage(AppState { tools: Mutex::new(None), cache_dir, export_cancel: Mutex::new(None) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tools_info,
            probe_media,
            make_proxy,
            make_waveform,
            make_filmstrip,
            load_project,
            save_project,
            read_text,
            write_text,
            list_dir,
            path_exists,
            export_start,
            export_cancel,
            export_preview_command,
            open_path,
            reveal_path
        ])
        .run(tauri::generate_context!())
        .expect("CutKit gagal dijalankan");
}
