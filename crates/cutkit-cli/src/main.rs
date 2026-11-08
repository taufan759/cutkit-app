//! cutkit-cli: mesin CutKit tanpa UI. Semua hasil berupa JSON di stdout; progres
//! (bila ada) berupa baris `PROGRESS <persen> <keterangan>` di stderr. Dipakai oleh
//! server MCP (mcp/server.js) dan bisa dipakai langsung dari terminal.

use cutkit_core::ai;
use cutkit_core::ffmpeg::{self, find_tools};
use cutkit_core::{build_frame, build_job, Encoder, ExportOptions, Project, Quality};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const HELP: &str = "cutkit-cli <perintah> [argumen]

  info                                   status ffmpeg dan model AI
  probe <file>...                        info media
  export <proyek.json> <hasil.mp4> [--quality fast|balanced|best] [--qsv] [--loudnorm]
  frame <proyek.json> <detik> <hasil.png> [--height 720]   satu frame hasil ekspor
  transcribe <file> [--start s] [--duration d] [--model whisper-small] [--lang id] [--vad]
  tts <hasil.wav> (--text \"...\" | --text-file f) [--speed 1] [--gap 0.3]
  beats <file>                           tempo dan ketukan musik
  remove-bg <masuk> <keluar> [--kind image|video] [--start s] [--duration d] [--fps 30] [--model matting-fast] [--audio]
  download <id-model>                    unduh model (lihat info)
  setup [--ffmpeg] [--download whisper-small,piper-id,...] [--claude] [--installer]
                                         persiapan untuk pemula (dijalankan installer)
  uninstall [--delete-models]            lepas MCP dari Claude dan hapus cache (dijalankan uninstaller)

Opsi umum: --models <folder> (bawaan: folder model aplikasi CutKit, sama di Windows, macOS, dan Linux)";

struct Args {
    pos: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Args {
    fn parse(raw: Vec<String>) -> Args {
        let mut pos = vec![];
        let mut flags = vec![];
        let mut i = 0;
        while i < raw.len() {
            let a = &raw[i];
            if let Some(name) = a.strip_prefix("--") {
                let takes = !matches!(name, "qsv" | "loudnorm" | "vad" | "audio" | "installer" | "ffmpeg" | "claude" | "yes" | "delete-models");
                if takes && i + 1 < raw.len() {
                    flags.push((name.to_string(), Some(raw[i + 1].clone())));
                    i += 2;
                    continue;
                }
                flags.push((name.to_string(), None));
            } else {
                pos.push(a.clone());
            }
            i += 1;
        }
        Args { pos, flags }
    }
    fn get(&self, name: &str) -> Option<&str> {
        self.flags.iter().find(|(n, _)| n == name).and_then(|(_, v)| v.as_deref())
    }
    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(n, _)| n == name)
    }
    fn num(&self, name: &str, default: f64) -> f64 {
        self.get(name).and_then(|v| v.parse().ok()).unwrap_or(default)
    }
}

/// Folder model: --models, CUTKIT_MODELS, pengaturan aplikasi, lalu folder data aplikasi.
fn models_dir(args: &Args) -> PathBuf {
    if let Some(d) = args.get("models") {
        return PathBuf::from(d);
    }
    if let Ok(d) = std::env::var("CUTKIT_MODELS") {
        return PathBuf::from(d);
    }
    cutkit_core::paths::models_dir()
}

/// Font caption bawaan: <app>/fonts (terpasang) atau ui/fonts di repo (target/release/../../ui/fonts).
fn fonts_dir(args: &Args) -> String {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_default();
    let c = [app_dir(args).join("fonts"), exe_dir.join("..").join("..").join("ui").join("fonts")];
    cutkit_core::paths::find_fonts_dir(&c).map(|d| d.to_string_lossy().to_string()).unwrap_or_default()
}

fn progress(pct: f64, what: &str) {
    eprintln!("PROGRESS {pct:.1} {what}");
}

fn need_model<'a>(dir: &Path, id: &str) -> Result<&'a ai::ModelInfo, String> {
    let m = ai::model(id).ok_or_else(|| format!("Model tidak dikenal: {id}"))?;
    if !ai::is_installed(dir, m) {
        return Err(format!("Model {} belum diunduh. Jalankan: cutkit-cli download {}", m.label, m.id));
    }
    Ok(m)
}

fn run(args: Args) -> Result<Value, String> {
    let cmd = args.pos.first().cloned().unwrap_or_default();
    let tools = || find_tools(None).ok_or_else(|| format!("ffmpeg tidak ditemukan. Pasang: {}", ffmpeg::install_hint()));
    let dir = models_dir(&args);
    match cmd.as_str() {
        "info" => {
            let t = find_tools(None);
            Ok(json!({
                "ffmpeg": t.as_ref().map(|t| t.ffmpeg.clone()),
                "version": t.as_ref().map(|t| t.version.clone()),
                "models_dir": dir.to_string_lossy(),
                "python_ready": cutkit_core::matting::python_ready(&dir),
                "models": ai::MODELS.iter().map(|m| json!({ "id": m.id, "kind": m.kind, "label": m.label, "size": m.size, "installed": ai::is_installed(&dir, m) })).collect::<Vec<_>>(),
            }))
        }
        "probe" => {
            let t = tools()?;
            Ok(Value::Array(
                args.pos[1..]
                    .iter()
                    .map(|p| match ffmpeg::probe(&t.ffprobe, p) {
                        Ok(info) => json!({ "path": p, "info": info }),
                        Err(e) => json!({ "path": p, "error": e }),
                    })
                    .collect(),
            ))
        }
        "export" => {
            let (proj, out) = (args.pos.get(1).ok_or("Butuh file proyek")?, args.pos.get(2).ok_or("Butuh file hasil")?);
            let t = tools()?;
            let project = Project::from_json(&std::fs::read_to_string(proj).map_err(|e| e.to_string())?)?;
            let quality = match args.get("quality") {
                Some("fast") => Quality::Fast,
                Some("best") => Quality::Best,
                _ => Quality::Balanced,
            };
            let opts = ExportOptions { output: out.clone(), encoder: if args.has("qsv") { Encoder::Qsv } else { Encoder::X264 }, quality, normalize_loudness: args.has("loudnorm"), fonts_dir: fonts_dir(&args) };
            let job = build_job(&project, &opts)?;
            let work = std::env::temp_dir().join("cutkit-cli").join(format!("{}", std::process::id()));
            let start = std::time::Instant::now();
            let r = ffmpeg::run_job(&t.ffmpeg, &work, &job, Arc::new(AtomicBool::new(false)), |p| progress(p.percent, &p.step));
            let _ = std::fs::remove_dir_all(&work);
            r?;
            Ok(json!({ "output": out, "duration": job.duration, "parts": job.steps.len(), "seconds": start.elapsed().as_secs_f64() }))
        }
        "frame" => {
            let (proj, t, out) = (args.pos.get(1).ok_or("Butuh file proyek")?, args.pos.get(2).ok_or("Butuh detik")?, args.pos.get(3).ok_or("Butuh file hasil .png")?);
            let t: f64 = t.parse().map_err(|_| format!("Detik tidak valid: {t}"))?;
            let tools = tools()?;
            let project = Project::from_json(&std::fs::read_to_string(proj).map_err(|e| e.to_string())?)?;
            let job = build_frame(&project, t, out, args.num("height", 720.0) as u32, &fonts_dir(&args))?;
            let work = std::env::temp_dir().join("cutkit-cli-frame").join(format!("{}", std::process::id()));
            let r = ffmpeg::run_job(&tools.ffmpeg, &work, &job, Arc::new(AtomicBool::new(false)), |_| {});
            let _ = std::fs::remove_dir_all(&work);
            r?;
            Ok(json!({ "output": out, "time": t, "duration": project.duration() }))
        }
        "transcribe" => {
            let file = args.pos.get(1).ok_or("Butuh file audio/video")?;
            let t = tools()?;
            let m = need_model(&dir, args.get("model").unwrap_or("whisper-small"))?;
            let vad = if args.has("vad") { Some(need_model(&dir, "vad-silero")?.file) } else { None };
            let out = format!("cutkit-cli-transcribe-{}.json", std::process::id());
            let a = ai::transcribe_args(file, args.num("start", 0.0), args.num("duration", 0.0), m.file, vad, args.get("lang").unwrap_or("id"), &out);
            let r = ffmpeg::run_ffmpeg_progress(&t.ffmpeg, &dir, &a, Arc::new(AtomicBool::new(false)), |sec, _| progress(sec, "detik"));
            let text = std::fs::read_to_string(dir.join(&out)).unwrap_or_default();
            let _ = std::fs::remove_file(dir.join(&out));
            r?;
            Ok(json!({ "words": ai::segments_to_words(&ai::parse_whisper_json(&text)) }))
        }
        "tts" => {
            let out = args.pos.get(1).ok_or("Butuh file hasil .wav")?;
            let text = match (args.get("text"), args.get("text-file")) {
                (Some(t), _) => t.to_string(),
                (None, Some(f)) => std::fs::read_to_string(f).map_err(|e| e.to_string())?,
                _ => return Err("Butuh --text atau --text-file".into()),
            };
            let t = tools()?;
            for id in ["piper-engine", "piper-id", "piper-id-config"] {
                need_model(&dir, id)?;
            }
            let work = std::env::temp_dir().join("cutkit-cli-tts").join(format!("{}", std::process::id()));
            let r = cutkit_core::tts::synthesize(&ai::piper_exe(&dir), &dir.join(ai::model("piper-id").unwrap().file), &t.ffmpeg, &t.ffprobe, &text, 1.0 / args.num("speed", 1.0).clamp(0.5, 2.0), args.num("gap", 0.3), &work, Path::new(out))?;
            Ok(serde_json::to_value(r).unwrap())
        }
        "beats" => {
            let file = args.pos.get(1).ok_or("Butuh file musik")?;
            let t = tools()?;
            Ok(serde_json::to_value(cutkit_core::beats::detect(&t.ffmpeg, file, 0.0, 0.0)?).unwrap())
        }
        "remove-bg" => {
            let (input, output) = (args.pos.get(1).ok_or("Butuh file masuk")?, args.pos.get(2).ok_or("Butuh file keluar")?);
            let t = tools()?;
            if !cutkit_core::matting::python_ready(&dir) {
                eprintln!("Menyiapkan Python (sekali saja)…");
                cutkit_core::matting::setup_python(&dir, Arc::new(AtomicBool::new(false)), |l| eprintln!("{l}"))?;
            }
            let m = need_model(&dir, args.get("model").unwrap_or("matting-fast"))?;
            let model = dir.join(m.file);
            if args.get("kind").unwrap_or("image") == "image" {
                cutkit_core::matting::remove_bg_image(&dir, &model, input, Path::new(output))?;
            } else {
                let work = std::env::temp_dir().join("cutkit-cli-matting").join(format!("{}", std::process::id()));
                cutkit_core::matting::remove_bg_video(&t.ffmpeg, &dir, &model, input, args.num("start", 0.0), args.num("duration", 5.0), args.num("fps", 30.0), args.has("audio"), &work, Path::new(output), Arc::new(AtomicBool::new(false)), |d, n| progress(d as f64 * 100.0 / n.max(1) as f64, "frame"))?;
            }
            Ok(json!({ "output": output }))
        }
        "download" => {
            let id = args.pos.get(1).ok_or("Butuh id model")?;
            let m = ai::model(id).ok_or_else(|| format!("Model tidak dikenal: {id}"))?;
            if !ai::is_installed(&dir, m) {
                ai::download(m, &dir, Arc::new(AtomicBool::new(false)), |got, total| progress(got as f64 * 100.0 / total.max(1) as f64, m.file))?;
            }
            Ok(json!({ "id": m.id, "installed": true, "models_dir": dir.to_string_lossy() }))
        }
        "bank" => {
            // Bank aset buatan CutKit: `bank` = daftar, `bank make [--dir D]` = buat yang belum ada.
            let bank = args.get("dir").map(PathBuf::from).unwrap_or_else(cutkit_core::bank::bank_dir);
            if args.pos.get(1).map(|s| s.as_str()) == Some("make") {
                let t = tools()?;
                let mut times = vec![];
                let mut last = std::time::Instant::now();
                let mut prev = String::new();
                let made = cutkit_core::bank::prepare(&t.ffmpeg, &bank, |done, total, label| {
                    if !prev.is_empty() {
                        times.push(json!({ "item": prev, "seconds": (last.elapsed().as_secs_f64() * 10.0).round() / 10.0 }));
                    }
                    prev = label.to_string();
                    last = std::time::Instant::now();
                    progress(done as f64 * 100.0 / total.max(1) as f64, label);
                })?;
                return Ok(json!({ "dir": bank.to_string_lossy(), "made": made, "times": times }));
            }
            Ok(json!({
                "dir": bank.to_string_lossy(),
                "items": cutkit_core::bank::ITEMS.iter().map(|i| json!({ "category": i.category, "label": i.label, "path": cutkit_core::bank::path_of(&bank, i).to_string_lossy(), "ready": cutkit_core::bank::path_of(&bank, i).is_file() })).collect::<Vec<_>>(),
            }))
        }
        "setup" => setup(&args, &dir),
        "uninstall" => uninstall(&args, &dir),
        _ => Err(HELP.to_string()),
    }
}

/* ---------- persiapan untuk pemula (dijalankan installer) ---------- */

/// Folder program: `<app>\mcp\cutkit-cli.exe` → `<app>`.
fn app_dir(args: &Args) -> PathBuf {
    if let Some(d) = args.get("app") {
        return PathBuf::from(d);
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    if dir.file_name().map(|n| n.eq_ignore_ascii_case("mcp")).unwrap_or(false) {
        dir.parent().map(Path::to_path_buf).unwrap_or(dir)
    } else {
        dir
    }
}

fn bar(got: u64, total: u64) {
    use std::io::Write;
    let mb = |b: u64| b as f64 / 1_048_576.0;
    if total > 0 {
        let pct = (got as f64 / total as f64).min(1.0);
        let fill = (pct * 30.0).round() as usize;
        print!("\r     [{}{}] {:>3.0}%  {:.0}/{:.0} MB   ", "#".repeat(fill), ".".repeat(30 - fill), pct * 100.0, mb(got), mb(total));
    } else {
        print!("\r     {:.0} MB   ", mb(got));
    }
    let _ = std::io::stdout().flush();
}

fn step(n: usize, total: usize, title: &str) {
    println!("\n[{n}/{total}] {title}");
}

/// Menyiapkan semua kebutuhan CutKit dan mencetak progres yang mudah dibaca. Langkah yang
/// gagal dilaporkan di akhir tanpa menghentikan langkah lain; aplikasi tetap bisa dipakai.
fn setup(args: &Args, models: &Path) -> Result<Value, String> {
    let app = app_dir(args);
    let want_models: Vec<String> = args.get("download").map(|m| m.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    let mut plan: Vec<&str> = vec!["webview2"];
    if args.has("ffmpeg") {
        plan.push("ffmpeg");
    }
    if !want_models.is_empty() {
        plan.push("models");
    }
    if args.has("claude") {
        plan.push("claude");
    }
    let total = plan.len();
    let mut done: Vec<String> = vec![];
    let mut failed: Vec<String> = vec![];
    println!("Menyiapkan CutKit di {}", app.display());
    println!("Jendela ini menampilkan proses persiapan. Biarkan terbuka sampai selesai.");

    for (i, what) in plan.iter().enumerate() {
        let n = i + 1;
        match *what {
            "webview2" => {
                step(n, total, "Mesin tampilan (Microsoft WebView2)");
                if cutkit_core::setup::webview2_installed() {
                    println!("     Sudah ada.");
                } else {
                    println!("     Memasang WebView2 dari Microsoft...");
                    match cutkit_core::setup::install_webview2(bar) {
                        Ok(()) => {
                            println!("\n     Selesai.");
                            done.push("WebView2".into());
                        }
                        Err(e) => {
                            println!("\n     Gagal: {e}");
                            failed.push(format!("WebView2: {e}"));
                        }
                    }
                }
            }
            "ffmpeg" => {
                step(n, total, "ffmpeg (untuk impor, preview, dan ekspor video)");
                if let Some(t) = find_tools(None) {
                    println!("     Sudah ada: {} ({})", t.ffmpeg, t.version);
                } else {
                    println!("     Memasang ffmpeg lewat {}. Bisa beberapa menit tergantung internet...", ffmpeg::installer_name().unwrap_or("pemasang sistem"));
                    match ffmpeg::install_ffmpeg(|line| println!("     {line}")) {
                        Ok(()) => {
                            println!("     ffmpeg terpasang.");
                            done.push("ffmpeg".into());
                        }
                        Err(e) => {
                            println!("     Gagal: {e}");
                            failed.push(format!("ffmpeg: {e} (bisa dipasang nanti dari menu Proyek → Cek sistem & ffmpeg)"));
                        }
                    }
                }
            }
            "models" => {
                step(n, total, &format!("Model AI (disimpan di {})", models.display()));
                for id in &want_models {
                    let Some(m) = ai::model(id) else {
                        failed.push(format!("Model tidak dikenal: {id}"));
                        continue;
                    };
                    if ai::is_installed(models, m) {
                        println!("     {}: sudah ada.", m.label);
                        continue;
                    }
                    println!("     Mengunduh {} ({:.0} MB)...", m.label, m.size as f64 / 1_048_576.0);
                    match ai::download(m, models, Arc::new(AtomicBool::new(false)), bar) {
                        Ok(()) => {
                            println!("\n     Selesai, checksum SHA-256 cocok.");
                            done.push(m.label.to_string());
                        }
                        Err(e) => {
                            println!("\n     Gagal: {e}");
                            failed.push(format!("{}: {e} (bisa diunduh nanti dari aplikasi)", m.label));
                        }
                    }
                }
            }
            "claude" => {
                step(n, total, "Hubungkan ke Claude (MCP)");
                match connect_claude(&app) {
                    Ok(list) if list.is_empty() => println!("     Aplikasi Claude belum terdeteksi. Nanti bisa dihubungkan dari menu Proyek → Hubungkan ke Claude."),
                    Ok(list) => {
                        for c in &list {
                            println!("     Terhubung ke {c}.");
                        }
                        println!("     Buka ulang Claude supaya tool CutKit muncul.");
                        done.push("Claude (MCP)".into());
                    }
                    Err(e) => {
                        println!("\n     Gagal: {e}");
                        failed.push(format!("Claude (MCP): {e} (bisa diulang dari menu Proyek → Hubungkan ke Claude)"));
                    }
                }
            }
            _ => {}
        }
    }

    println!();
    if failed.is_empty() {
        println!("Semua siap. CutKit akan terbuka sebentar lagi.");
    } else {
        println!("Selesai dengan catatan. CutKit tetap bisa dipakai; bagian berikut bisa diulang dari aplikasi:");
        for f in &failed {
            println!("  - {f}");
        }
    }
    if args.has("installer") {
        if failed.is_empty() {
            std::thread::sleep(std::time::Duration::from_secs(3));
        } else {
            println!("\nTekan Enter untuk menutup jendela ini.");
            let mut s = String::new();
            let _ = std::io::stdin().read_line(&mut s);
        }
    }
    Ok(json!({ "done": done, "failed": failed }))
}

/// Mendaftarkan server MCP ke semua aplikasi Claude yang terdeteksi; Node.js portabel dipasang bila perlu.
fn connect_claude(app: &Path) -> Result<Vec<String>, String> {
    let server = app.join("mcp").join("server.js");
    if !server.is_file() {
        return Err(format!("{} tidak ada", server.display()));
    }
    let clients: Vec<_> = cutkit_core::mcpcfg::clients().into_iter().filter(|(_, _, p)| p.parent().map(|d| d.is_dir()).unwrap_or(false)).collect();
    if clients.is_empty() {
        return Ok(vec![]);
    }
    let node = match cutkit_core::mcpcfg::find_node(Some(app)) {
        Some(n) => n,
        None => {
            println!("     Memasang Node.js portabel untuk server MCP (±30 MB, dicek SHA-256)...");
            let home = cutkit_core::setup::node_home(Some(app));
            let v = cutkit_core::setup::install_node(&home, bar)?;
            println!("\n     Node.js {v} siap.");
            cutkit_core::setup::bundled_node(&home).to_string_lossy().to_string()
        }
    };
    let server_s = server.to_string_lossy().replace('\\', "/");
    let mut out = vec![];
    for (_, name, path) in clients {
        cutkit_core::mcpcfg::install(&path, "cutkit", &node, &[server_s.clone()])?;
        out.push(name.to_string());
    }
    Ok(out)
}

/// Dipanggil uninstaller: melepas MCP CutKit dari Claude dan menghapus cache. Proyek dan
/// template tidak disentuh; model AI hanya dihapus dengan `--delete-models`.
fn uninstall(args: &Args, models: &Path) -> Result<Value, String> {
    let app = app_dir(args);
    let ours = app.join("mcp").to_string_lossy().replace('\\', "/").to_lowercase();
    let mut removed = vec![];
    for (_, name, path) in cutkit_core::mcpcfg::clients() {
        let entry = cutkit_core::mcpcfg::entry(&path, "cutkit");
        let mine = entry.as_ref().and_then(|v| v["args"].get(0)).and_then(|v| v.as_str()).map(|s| s.replace('\\', "/").to_lowercase().starts_with(&ours)).unwrap_or(false);
        if mine && cutkit_core::mcpcfg::uninstall(&path, "cutkit").unwrap_or(false) {
            removed.push(name);
        }
    }
    let data = Some(cutkit_core::paths::data_dir());
    if let Some(d) = &data {
        for sub in ["proxy", "thumbs", "wave", "tts", "export", "EBWebView"] {
            let p = d.join(sub);
            if p.exists() {
                if let Err(e) = std::fs::remove_dir_all(&p) {
                    eprintln!("Tidak bisa menghapus {}: {e}", p.display());
                }
            }
        }
    }
    let mut models_removed = false;
    if args.has("delete-models") && models.is_dir() {
        // Hanya file model CutKit yang dikenal (folder bisa dipilih sendiri dan berisi file lain).
        for m in ai::MODELS {
            let _ = std::fs::remove_file(models.join(m.file));
        }
        for sub in ["piper", "py"] {
            let _ = std::fs::remove_dir_all(models.join(sub));
        }
        let _ = std::fs::remove_file(models.join("cutkit-matting.py"));
        let _ = std::fs::remove_dir(models);
        models_removed = true;
    }
    if let Some(d) = &data {
        let _ = std::fs::remove_dir(d);
    }
    Ok(json!({ "mcp_removed_from": removed, "models_removed": models_removed }))
}

fn main() {
    let args = Args::parse(std::env::args().skip(1).collect());
    match run(args) {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
        Err(e) => {
            println!("{}", json!({ "error": e }));
            std::process::exit(1);
        }
    }
}
