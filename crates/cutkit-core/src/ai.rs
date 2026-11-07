//! Model AI lokal (diunduh sekali, lalu offline): katalog, pengunduh terverifikasi
//! SHA-256, dan auto caption dengan filter `whisper` ffmpeg (whisper.cpp).

use crate::model::Word;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Serialize, Clone, Copy, Debug)]
pub struct ModelInfo {
    pub id: &'static str,
    pub kind: &'static str,
    pub label: &'static str,
    pub note: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

/// Ukuran dan hash dari API Hugging Face (ggerganov/whisper.cpp, ggml-org/whisper-vad).
/// Mesin Piper rilis resmi 2023.11.14-2 untuk OS dan arsitektur ini (ukuran dan SHA-256 dari rilisnya).
const fn piper_engine(file: &'static str, url: &'static str, sha256: &'static str, size: u64) -> ModelInfo {
    ModelInfo { id: "piper-engine", kind: "piper-engine", label: "Mesin TTS Piper", note: "20-26 MB. Program TTS offline (rhasspy/piper 2023.11.14-2, MIT).", file, url, sha256, size }
}
#[cfg(windows)]
const PIPER_ENGINE: ModelInfo = piper_engine("piper_windows_amd64.zip", "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip", "f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea", 22_477_236);
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const PIPER_ENGINE: ModelInfo = piper_engine("piper_linux_aarch64.tar.gz", "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_aarch64.tar.gz", "fea0fd2d87c54dbc7078d0f878289f404bd4d6eea6e7444a77835d1537ab88eb", 26_004_717);
#[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
const PIPER_ENGINE: ModelInfo = piper_engine("piper_linux_x86_64.tar.gz", "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz", "a50cb45f355b7af1f6d758c1b360717877ba0a398cc8cbe6d2a7a3a26e225992", 26_460_462);
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const PIPER_ENGINE: ModelInfo = piper_engine("piper_macos_aarch64.tar.gz", "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_macos_aarch64.tar.gz", "6b1eb03b3735946cb35216e063e7eebcc33a6bbf5dd96ec0217959bf1cdcb0cc", 19_146_957);
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
const PIPER_ENGINE: ModelInfo = piper_engine("piper_macos_x64.tar.gz", "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_macos_x64.tar.gz", "ced85c0a3df13945b1e623b878a48fdc2854d5c485b4b67f62857cf551deaf8b", 19_146_927);
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
const PIPER_ENGINE: ModelInfo = piper_engine("piper-unsupported", "https://github.com/rhasspy/piper/releases", "", 0);

pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "whisper-base",
        kind: "whisper",
        label: "Cepat (base)",
        note: "60 MB. Paling cepat, akurasi cukup untuk suara yang jelas.",
        file: "ggml-base-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        size: 59_707_625,
    },
    ModelInfo {
        id: "whisper-small",
        kind: "whisper",
        label: "Seimbang (small) — disarankan",
        note: "190 MB. Akurasi bagus untuk Bahasa Indonesia; di i5-1345U sekitar 8x lebih cepat dari durasi audio.",
        file: "ggml-small-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        size: 190_085_487,
    },
    ModelInfo {
        id: "whisper-medium",
        kind: "whisper",
        label: "Akurat (medium)",
        note: "539 MB. Paling akurat, sekitar 3x lebih lambat dari small.",
        file: "ggml-medium-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin",
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        size: 539_212_467,
    },
    PIPER_ENGINE,
    ModelInfo {
        id: "piper-id",
        kind: "piper-voice",
        label: "Suara Indonesia (news_tts)",
        note: "63 MB. Suara pria Bahasa Indonesia dari rhasspy/piper-voices.",
        file: "id_ID-news_tts-medium.onnx",
        url: "https://huggingface.co/rhasspy/piper-voices/resolve/main/id/id_ID/news_tts/medium/id_ID-news_tts-medium.onnx",
        sha256: "ed8f02aa593f7af6b19acbdb8142e0da0dd72f46194eb33d38e0eb10a52597e8",
        size: 62_950_044,
    },
    ModelInfo {
        id: "piper-id-config",
        kind: "piper-config",
        label: "Pengaturan suara Indonesia",
        note: "5 KB.",
        file: "id_ID-news_tts-medium.onnx.json",
        url: "https://huggingface.co/rhasspy/piper-voices/resolve/main/id/id_ID/news_tts/medium/id_ID-news_tts-medium.onnx.json",
        sha256: "1ef677072668a5e172e0759b1d3871f129009d1167f093325a17607f7add5ad7",
        size: 5_050,
    },
    ModelInfo {
        id: "matting-fast",
        kind: "matting",
        label: "Cepat (U²-Net kecil)",
        note: "4,6 MB. Cepat, tepi kurang halus. Cocok untuk mencoba dan video panjang.",
        file: "u2netp.onnx",
        url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx",
        sha256: "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8",
        size: 4_574_861,
    },
    ModelInfo {
        id: "matting-best",
        kind: "matting",
        label: "Akurat (IS-Net)",
        note: "179 MB. Tepi rapi untuk orang dan benda; lebih lambat.",
        file: "isnet-general-use.onnx",
        url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx",
        sha256: "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
        size: 178_648_008,
    },
    ModelInfo {
        id: "vad-silero",
        kind: "vad",
        label: "Deteksi suara (Silero VAD)",
        note: "0,9 MB. Melewati bagian tanpa ucapan; lambat di filter ffmpeg, jadi hanya dipakai bila diminta.",
        file: "ggml-silero-v6.2.0.bin",
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin",
        sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987",
        size: 885_098,
    },
];

pub fn model(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

/// Model dianggap terpasang bila file ada dan ukurannya cocok (hash dicek saat unduh).
/// Mesin Piper dianggap terpasang bila piper.exe hasil ekstraksinya ada.
pub fn is_installed(dir: &Path, m: &ModelInfo) -> bool {
    if m.kind == "piper-engine" {
        return piper_exe(dir).is_file();
    }
    std::fs::metadata(dir.join(m.file)).map(|md| md.len() == m.size).unwrap_or(false)
}

/// Lokasi piper.exe setelah zip mesin diekstrak ke folder model.
pub fn piper_exe(dir: &Path) -> std::path::PathBuf {
    dir.join("piper").join(if cfg!(windows) { "piper.exe" } else { "piper" })
}

/// Mengekstrak arsip mesin Piper (zip di Windows, tar.gz di Linux/macOS) dengan tar sistem, lalu menghapus arsipnya.
pub fn extract_engine(dir: &Path, m: &ModelInfo) -> Result<(), String> {
    let zip = dir.join(m.file);
    let tar = if cfg!(windows) { std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into())).join("System32").join("tar.exe") } else { "tar".into() };
    let out = std::process::Command::new(tar).arg("-xf").arg(&zip).arg("-C").arg(dir).output().map_err(|e| format!("tar gagal dijalankan: {e}"))?;
    if !out.status.success() || !piper_exe(dir).is_file() {
        return Err(format!("Gagal mengekstrak Piper: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let _ = std::fs::remove_file(zip);
    Ok(())
}

/// Mengunduh model ke `dir` lewat file .part, memverifikasi ukuran dan SHA-256, lalu mengganti nama.
pub fn download(m: &ModelInfo, dir: &Path, cancel: Arc<AtomicBool>, mut on_progress: impl FnMut(u64, u64)) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Tidak bisa membuat folder model: {e}"))?;
    let part = dir.join(format!("{}.part", m.file));
    let dest = dir.join(m.file);
    let resp = ureq::get(m.url).call().map_err(|e| format!("Gagal mengunduh {}: {e}", m.file))?;
    let mut reader = resp.into_body().into_reader();
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut got: u64 = 0;
    let mut last_report = std::time::Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err("Unduhan dibatalkan.".into());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("Unduhan terputus: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        got += n as u64;
        if last_report.elapsed().as_millis() > 250 {
            on_progress(got, m.size);
            last_report = std::time::Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    let hash: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != m.size || hash != m.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("File {} rusak atau tidak cocok (ukuran {got}, hash {hash}). Coba unduh lagi.", m.file));
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    if m.kind == "piper-engine" {
        extract_engine(dir, m)?;
    }
    on_progress(m.size, m.size);
    Ok(())
}

/// Argumen ffmpeg untuk transkripsi `media` mulai `start` detik sepanjang `duration` detik.
/// Dijalankan dengan direktori kerja = folder model, jadi nama file di filter tidak perlu di-escape.
pub fn transcribe_args(media: &str, start: f64, duration: f64, model_file: &str, vad_file: Option<&str>, language: &str, out_file: &str) -> Vec<String> {
    let lang: String = language.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let lang = if lang.is_empty() { "auto".to_string() } else { lang };
    let mut filter = format!("aresample=16000,aformat=sample_fmts=flt:channel_layouts=mono,whisper=model={model_file}:language={lang}:queue=20:destination={out_file}:format=json:max_len=1");
    if let Some(v) = vad_file {
        filter.push_str(&format!(":vad_model={v}"));
    }
    let mut a: Vec<String> = ["-hide_banner", "-nostats", "-loglevel", "error", "-progress", "pipe:1"].iter().map(|s| s.to_string()).collect();
    if start > 0.0 {
        a.extend(["-ss".to_string(), format!("{start:.3}")]);
    }
    if duration > 0.0 {
        a.extend(["-t".to_string(), format!("{duration:.3}")]);
    }
    a.extend(["-i", media, "-vn", "-map", "0:a:0", "-af", &filter, "-f", "null", "-"].iter().map(|s| s.to_string()));
    a
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Mengurai keluaran `format=json` filter whisper: satu objek {"start":ms,"end":ms,"text":"..."} per segmen.
/// Teks tidak di-escape oleh ffmpeg, jadi diurai manual (bukan lewat parser JSON).
pub fn parse_whisper_json(text: &str) -> Vec<Segment> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(i) = rest.find("{\"start\":") {
        rest = &rest[i + 9..];
        let Some(comma) = rest.find(",\"end\":") else { break };
        let start: f64 = rest[..comma].trim().parse().unwrap_or(0.0);
        rest = &rest[comma + 7..];
        let Some(comma) = rest.find(",\"text\":\"") else { break };
        let end: f64 = rest[..comma].trim().parse().unwrap_or(0.0);
        rest = &rest[comma + 9..];
        // Teks berakhir di "} yang diikuti baris baru, koma, atau akhir file.
        let mut close = None;
        let mut search = 0;
        while let Some(j) = rest[search..].find("\"}") {
            let k = search + j;
            let after = rest[k + 2..].trim_start_matches(['\r', ' ']);
            if after.is_empty() || after.starts_with('\n') || after.starts_with(',') || after.starts_with('{') {
                close = Some(k);
                break;
            }
            search = k + 2;
        }
        let Some(k) = close else { break };
        let t = rest[..k].to_string();
        rest = &rest[k + 2..];
        out.push(Segment { start: start / 1000.0, end: end / 1000.0, text: t });
    }
    out
}

fn is_punct_only(s: &str) -> bool {
    !s.trim().is_empty() && s.trim().chars().all(|c| !c.is_alphanumeric())
}

/// Segmen per kata (max_len=1) dirakit jadi daftar kata. ffmpeg memangkas spasi teks segmen, jadi
/// tiap segmen = kata sendiri; tanda baca yang berdiri sendiri menempel ke kata sebelumnya, segmen
/// berisi beberapa kata dibagi rata menurut panjang huruf. Penanda non-ucapan seperti [MUSIK] dibuang.
pub fn segments_to_words(segs: &[Segment]) -> Vec<Word> {
    let mut words: Vec<Word> = vec![];
    for s in segs {
        let raw = s.text.replace('\n', " ");
        let trimmed = raw.trim();
        if trimmed.is_empty() || (trimmed.starts_with('[') && trimmed.ends_with(']')) || (trimmed.starts_with('(') && trimmed.ends_with(')')) {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() == 1 && is_punct_only(parts[0]) && !words.is_empty() {
            let last = words.last_mut().unwrap();
            last.text.push_str(parts[0]);
            last.end = last.end.max(s.end);
            continue;
        }
        let span = (s.end - s.start).max(0.0);
        let weights: Vec<f64> = parts.iter().map(|p| p.chars().filter(|c| c.is_alphanumeric()).count().max(1) as f64 + 1.0).collect();
        let total: f64 = weights.iter().sum();
        let mut t = s.start;
        for (p, wgt) in parts.iter().zip(&weights) {
            let d = span * wgt / total;
            words.push(Word { text: p.to_string(), start: t, end: t + d });
            t += d;
        }
    }
    for w in &mut words {
        if w.end <= w.start {
            w.end = w.start + 0.05;
        }
        w.start = (w.start * 1000.0).round() / 1000.0;
        w.end = (w.end * 1000.0).round() / 1000.0;
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_and_install_check() {
        assert!(model("whisper-small").unwrap().url.ends_with("ggml-small-q5_1.bin"));
        assert!(MODELS.iter().all(|m| m.sha256.len() == 64 && (m.url.starts_with("https://huggingface.co/") || m.url.starts_with("https://github.com/rhasspy/piper/") || m.url.starts_with("https://github.com/danielgatis/rembg/"))));
        let dir = std::env::temp_dir().join("cutkit-ai-test");
        let _ = std::fs::create_dir_all(&dir);
        let vad = model("vad-silero").unwrap();
        std::fs::write(dir.join(vad.file), b"pendek").unwrap();
        assert!(!is_installed(&dir, vad), "ukuran tidak cocok = belum terpasang");
    }

    #[test]
    fn transcribe_command() {
        let a = transcribe_args("D:\\VO\\narasi.mp3", 1.5, 10.0, "ggml-small-q5_1.bin", Some("ggml-silero-v6.2.0.bin"), "id", "out-1.json").join(" ");
        assert!(a.contains("-ss 1.500 -t 10.000 -i D:\\VO\\narasi.mp3 -vn -map 0:a:0 -af aresample=16000,aformat=sample_fmts=flt:channel_layouts=mono,whisper=model=ggml-small-q5_1.bin:language=id:queue=20:destination=out-1.json:format=json:max_len=1:vad_model=ggml-silero-v6.2.0.bin -f null -"), "{a}");
        let b = transcribe_args("x.wav", 0.0, 0.0, "m.bin", None, "id:evil", "o.json").join(" ");
        assert!(b.contains("language=idevil:") && !b.contains("-ss") && !b.contains("vad_model"));
    }

    #[test]
    fn parses_segments_with_unescaped_text() {
        // Format nyata ffmpeg 9 (teks sudah dipangkas), plus kasus tepi: koma sendiri, kutip tanpa escape, penanda musik.
        let json = "{\"start\":0,\"end\":420,\"text\":\"Halo\"}\n{\"start\":420,\"end\":900,\"text\":\"semua\"}\n{\"start\":900,\"end\":950,\"text\":\",\"}\n{\"start\":1000,\"end\":1500,\"text\":\"kata \"kutip\"}\"}\n{\"start\":1500,\"end\":2000,\"text\":\"[MUSIK]\"}\n";
        let segs = parse_whisper_json(json);
        assert_eq!(segs.len(), 5);
        assert_eq!(segs[3].text, "kata \"kutip\"}");
        let words = segments_to_words(&segs);
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["Halo", "semua,", "kata", "\"kutip\"}"]);
        assert_eq!((words[1].start, words[1].end), (0.42, 0.95));
        assert!(words[2].end <= words[3].start + 1e-9);
    }
}
