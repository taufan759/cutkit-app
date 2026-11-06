// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cutkit_core::ffmpeg;
use cutkit_core::proxy::{filmstrip_args, filmstrip_count, proxy_args, proxy_extension, waveform_args};
use cutkit_core::{build_job, build_plan, AssetKind, ExportOptions, MediaInfo, Project, Tools};
use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

struct AppState {
    tools: Mutex<Option<Tools>>,
    cache_dir: PathBuf,
    export_cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// Pembatal unduhan model / transkripsi yang sedang berjalan.
    ai_cancel: Mutex<Option<Arc<AtomicBool>>>,
    settings_path: PathBuf,
    default_models_dir: PathBuf,
}

#[derive(Serialize, serde::Deserialize, Clone, Default)]
#[serde(default)]
struct Settings {
    /// Folder model AI (Whisper, Piper, dll.). Kosong = folder data aplikasi.
    models_dir: String,
}

impl AppState {
    fn settings(&self) -> Settings {
        std::fs::read_to_string(&self.settings_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn models_dir(&self) -> PathBuf {
        let s = self.settings();
        if s.models_dir.trim().is_empty() {
            self.default_models_dir.clone()
        } else {
            PathBuf::from(s.models_dir)
        }
    }

    /// Satu tugas AI pada satu waktu; mengembalikan flag pembatalnya.
    fn begin_ai(&self) -> Result<Arc<AtomicBool>, String> {
        let mut slot = self.ai_cancel.lock().map_err(|e| e.to_string())?;
        if slot.is_some() {
            return Err("Masih ada unduhan atau transkripsi yang berjalan.".into());
        }
        let flag = Arc::new(AtomicBool::new(false));
        *slot = Some(flag.clone());
        Ok(flag)
    }

    fn end_ai(&self) {
        if let Ok(mut slot) = self.ai_cancel.lock() {
            *slot = None;
        }
    }
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

/// Menulis data biner (base64, boleh berupa data URL) ke file, mis. stiker PNG hasil render.
#[tauri::command]
fn write_binary(path: String, data: String) -> Result<(), String> {
    use base64::Engine;
    let b64 = data.split_once(',').map(|(_, d)| d).unwrap_or(&data);
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|e| format!("Data tidak valid: {e}"))?;
    let p = Path::new(&path);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(p, bytes).map_err(|e| format!("Gagal menyimpan: {e}"))
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

/// Isi folder untuk panel pustaka dan wizard (media yang dikenal, caption .srt/.json, dan subfolder).
#[tauri::command]
fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    const EXT: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "m4v", "gif", "png", "jpg", "jpeg", "webp", "bmp", "mp3", "wav", "m4a", "aac", "ogg", "opus", "flac", "wma", "srt", "json"];
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
    let job = build_job(&project, &options)?;
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
        let result = ffmpeg::run_job(&tools.ffmpeg, &work, &job, cancel, move |p| {
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

#[derive(Serialize)]
struct SystemInfo {
    tools: Option<Tools>,
    winget: bool,
    cache_dir: String,
    cache_bytes: u64,
}

/// Cek sistem untuk pemula: ffmpeg, Quick Sync, winget, dan ukuran cache.
#[tauri::command]
async fn system_info(state: State<'_, AppState>) -> Result<SystemInfo, String> {
    let tools = state.tools().ok();
    let cache = state.cache_dir.clone();
    blocking(move || {
        let tools = tools.map(|mut t| {
            t.has_qsv = ffmpeg::check_qsv(&t.ffmpeg);
            t
        });
        Ok(SystemInfo { tools, winget: ffmpeg::has_winget(), cache_bytes: ffmpeg::dir_size(&cache), cache_dir: cache.to_string_lossy().to_string() })
    })
    .await
}

/// Menghapus proxy, gambar mini, dan gelombang. Semuanya dibuat ulang saat dibutuhkan.
#[tauri::command]
fn clear_cache(state: State<AppState>) -> Result<u64, String> {
    let before = ffmpeg::dir_size(&state.cache_dir);
    for sub in ["proxy", "thumbs", "wave", "export"] {
        let _ = std::fs::remove_dir_all(state.cache_dir.join(sub));
    }
    Ok(before.saturating_sub(ffmpeg::dir_size(&state.cache_dir)))
}

/// Memasang ffmpeg lewat winget; log dikirim lewat event `setup-log`.
#[tauri::command]
async fn install_ffmpeg(app: AppHandle, state: State<'_, AppState>) -> Result<Tools, String> {
    let app2 = app.clone();
    blocking(move || ffmpeg::install_ffmpeg(|line| { let _ = app2.emit("setup-log", line); })).await?;
    let mut guard = state.tools.lock().map_err(|e| e.to_string())?;
    *guard = ffmpeg::find_tools(None);
    guard.clone().ok_or_else(|| "ffmpeg sudah dipasang tapi belum ditemukan. Tutup lalu buka ulang CutKit.".to_string())
}

/// Membuat media untuk proyek contoh di folder `dir`.
#[tauri::command]
async fn make_demo(dir: String, state: State<'_, AppState>) -> Result<cutkit_core::demo::DemoMedia, String> {
    let tools = state.tools()?;
    blocking(move || {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let (media, jobs) = cutkit_core::demo::demo_jobs(&dir);
        for (out, args) in jobs {
            if !Path::new(&out).is_file() {
                ffmpeg::run(&tools.ffmpeg, &args)?;
            }
        }
        Ok(media)
    })
    .await
}

#[derive(Serialize)]
struct ModelStatus {
    #[serde(flatten)]
    info: cutkit_core::ai::ModelInfo,
    installed: bool,
}

#[derive(Serialize)]
struct AiInfo {
    models_dir: String,
    models: Vec<ModelStatus>,
}

#[tauri::command]
fn ai_models(state: State<AppState>) -> AiInfo {
    let dir = state.models_dir();
    AiInfo {
        models_dir: dir.to_string_lossy().to_string(),
        models: cutkit_core::ai::MODELS.iter().map(|m| ModelStatus { info: *m, installed: cutkit_core::ai::is_installed(&dir, m) }).collect(),
    }
}

#[tauri::command]
fn set_models_dir(path: String, state: State<AppState>) -> Result<(), String> {
    let mut s = state.settings();
    s.models_dir = path;
    if let Some(dir) = state.settings_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&state.settings_path, serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Mengunduh model; progres lewat event `ai-download` ({ id, got, total }).
#[tauri::command]
async fn ai_download(app: AppHandle, id: String, state: State<'_, AppState>) -> Result<(), String> {
    let m = cutkit_core::ai::model(&id).ok_or("Model tidak dikenal.")?;
    let dir = state.models_dir();
    if cutkit_core::ai::is_installed(&dir, m) {
        return Ok(());
    }
    let cancel = state.begin_ai()?;
    let app2 = app.clone();
    let result = blocking(move || {
        cutkit_core::ai::download(m, &dir, cancel, |got, total| {
            let _ = app2.emit("ai-download", serde_json::json!({ "id": m.id, "got": got, "total": total }));
        })
    })
    .await;
    state.end_ai();
    result
}

#[tauri::command]
fn ai_cancel(state: State<AppState>) -> Result<(), String> {
    if let Some(flag) = state.ai_cancel.lock().map_err(|e| e.to_string())?.as_ref() {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Auto caption satu file (bagian start..start+duration dari sumber) dengan Whisper.
/// Waktu kata relatif terhadap `start`; progres lewat event `transcribe-progress` ({ seconds }).
#[tauri::command]
async fn transcribe(app: AppHandle, path: String, start: f64, duration: f64, model: String, language: String, use_vad: bool, state: State<'_, AppState>) -> Result<Vec<cutkit_core::Word>, String> {
    let tools = state.tools()?;
    let dir = state.models_dir();
    let m = cutkit_core::ai::model(&model).filter(|m| m.kind == "whisper").ok_or("Model Whisper tidak dikenal.")?;
    if !cutkit_core::ai::is_installed(&dir, m) {
        return Err(format!("Model {} belum diunduh.", m.label));
    }
    let vad = cutkit_core::ai::model("vad-silero").filter(|v| use_vad && cutkit_core::ai::is_installed(&dir, v)).map(|v| v.file);
    let cancel = state.begin_ai()?;
    let out_name = format!("cutkit-transcribe-{}.json", std::process::id());
    let args = cutkit_core::ai::transcribe_args(&path, start, duration, m.file, vad, &language, &out_name);
    let app2 = app.clone();
    let result = blocking(move || {
        let out = dir.join(&out_name);
        let _ = std::fs::remove_file(&out);
        let run = ffmpeg::run_ffmpeg_progress(&tools.ffmpeg, &dir, &args, cancel, |sec, _| {
            let _ = app2.emit("transcribe-progress", serde_json::json!({ "seconds": sec }));
        });
        let text = std::fs::read_to_string(&out).unwrap_or_default();
        let _ = std::fs::remove_file(&out);
        run.map_err(|e| if e.contains("dibatalkan") { "Transkripsi dibatalkan.".to_string() } else { e })?;
        Ok(cutkit_core::ai::segments_to_words(&cutkit_core::ai::parse_whisper_json(&text)))
    })
    .await;
    state.end_ai();
    result
}

/// TTS offline (Piper) ke `out_path` (WAV). Mengembalikan durasi dan waktu tiap kalimat.
#[tauri::command]
async fn tts(text: String, length_scale: f64, gap: f64, out_path: String, state: State<'_, AppState>) -> Result<cutkit_core::tts::TtsResult, String> {
    let tools = state.tools()?;
    let dir = state.models_dir();
    for id in ["piper-engine", "piper-id", "piper-id-config"] {
        let m = cutkit_core::ai::model(id).ok_or("Katalog TTS tidak lengkap.")?;
        if !cutkit_core::ai::is_installed(&dir, m) {
            return Err(format!("{} belum dipasang.", m.label));
        }
    }
    let work = state.cache_dir.join("tts").join(format!("{}", std::process::id()));
    blocking(move || {
        let voice = dir.join(cutkit_core::ai::model("piper-id").unwrap().file);
        cutkit_core::tts::synthesize(&cutkit_core::ai::piper_exe(&dir), &voice, &tools.ffmpeg, &tools.ffprobe, &text, length_scale, gap, &work, Path::new(&out_path))
    })
    .await
}

/// Deteksi ketukan musik pada file audio/video (waktu relatif terhadap awal file).
#[tauri::command]
async fn detect_beats(path: String, state: State<'_, AppState>) -> Result<Option<cutkit_core::beats::BeatInfo>, String> {
    let tools = state.tools()?;
    blocking(move || cutkit_core::beats::detect(&tools.ffmpeg, &path, 0.0, 0.0)).await
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
            let config_dir = app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir().join("cutkit-config"));
            let data_dir = app.path().app_local_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("cutkit-data"));
            app.manage(AppState {
                tools: Mutex::new(None),
                cache_dir,
                export_cancel: Mutex::new(None),
                ai_cancel: Mutex::new(None),
                settings_path: config_dir.join("settings.json"),
                default_models_dir: data_dir.join("models"),
            });
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
            write_binary,
            list_dir,
            path_exists,
            export_start,
            export_cancel,
            export_preview_command,
            system_info,
            clear_cache,
            install_ffmpeg,
            make_demo,
            ai_models,
            set_models_dir,
            ai_download,
            ai_cancel,
            transcribe,
            tts,
            detect_beats,
            open_path,
            reveal_path
        ])
        .run(tauri::generate_context!())
        .expect("CutKit gagal dijalankan");
}
