//! cutkit-cli: mesin CutKit tanpa UI. Semua hasil berupa JSON di stdout; progres
//! (bila ada) berupa baris `PROGRESS <persen> <keterangan>` di stderr. Dipakai oleh
//! server MCP (mcp/server.js) dan bisa dipakai langsung dari terminal.

use cutkit_core::ai;
use cutkit_core::ffmpeg::{self, find_tools};
use cutkit_core::{build_job, Encoder, ExportOptions, Project, Quality};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const HELP: &str = "cutkit-cli <perintah> [argumen]

  info                                   status ffmpeg dan model AI
  probe <file>...                        info media
  export <proyek.json> <hasil.mp4> [--quality fast|balanced|best] [--qsv] [--loudnorm]
  transcribe <file> [--start s] [--duration d] [--model whisper-small] [--lang id] [--vad]
  tts <hasil.wav> (--text \"...\" | --text-file f) [--speed 1] [--gap 0.3]
  beats <file>                           tempo dan ketukan musik
  remove-bg <masuk> <keluar> [--kind image|video] [--start s] [--duration d] [--fps 30] [--model matting-fast] [--audio]
  download <id-model>                    unduh model (lihat info)

Opsi umum: --models <folder> (bawaan: folder model aplikasi CutKit)";

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
                let takes = !matches!(name, "qsv" | "loudnorm" | "vad" | "audio");
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
    if let Ok(appdata) = std::env::var("APPDATA") {
        let settings = Path::new(&appdata).join("io.github.taufan759.cutkit").join("settings.json");
        if let Ok(text) = std::fs::read_to_string(settings) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                if let Some(d) = v["models_dir"].as_str().filter(|d| !d.trim().is_empty()) {
                    return PathBuf::from(d);
                }
            }
        }
    }
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
    Path::new(&base).join("io.github.taufan759.cutkit").join("models")
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
    let tools = || find_tools(None).ok_or_else(|| "ffmpeg tidak ditemukan. Pasang: winget install Gyan.FFmpeg".to_string());
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
            let opts = ExportOptions { output: out.clone(), encoder: if args.has("qsv") { Encoder::Qsv } else { Encoder::X264 }, quality, normalize_loudness: args.has("loudnorm") };
            let job = build_job(&project, &opts)?;
            let work = std::env::temp_dir().join("cutkit-cli").join(format!("{}", std::process::id()));
            let start = std::time::Instant::now();
            let r = ffmpeg::run_job(&t.ffmpeg, &work, &job, Arc::new(AtomicBool::new(false)), |p| progress(p.percent, &p.step));
            let _ = std::fs::remove_dir_all(&work);
            r?;
            Ok(json!({ "output": out, "duration": job.duration, "parts": job.steps.len(), "seconds": start.elapsed().as_secs_f64() }))
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
        _ => Err(HELP.to_string()),
    }
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
