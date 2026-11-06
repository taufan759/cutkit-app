//! TTS offline dengan Piper (rhasspy/piper, MIT) dan suara Bahasa Indonesia dari
//! rhasspy/piper-voices. Naskah dipecah per kalimat supaya waktu tiap kalimat
//! diketahui (dipakai untuk caption), lalu digabung dengan jeda.

use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Sentence {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct TtsResult {
    pub path: String,
    pub duration: f64,
    pub sentences: Vec<Sentence>,
}

/// Memecah naskah per kalimat: setelah . ! ? … (diikuti spasi) atau baris baru.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '\n' || c == '\r' {
            if !cur.trim().is_empty() {
                out.push(cur.trim().to_string());
            }
            cur.clear();
            continue;
        }
        cur.push(c);
        let end_mark = matches!(c, '.' | '!' | '?' | '…');
        let next_is_space = chars.get(i + 1).map(|n| n.is_whitespace()).unwrap_or(true);
        // Angka desimal (3.5) tidak memotong kalimat karena tidak diikuti spasi.
        if end_mark && next_is_space && cur.trim().chars().filter(|c| c.is_alphanumeric()).count() > 1 {
            out.push(cur.trim().to_string());
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn json_escape(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// Baris JSON untuk `piper --json-input`: satu kalimat per baris dengan file keluarannya.
pub fn piper_jsonl(sentences: &[String], files: &[PathBuf]) -> String {
    sentences
        .iter()
        .zip(files)
        .map(|(s, f)| format!("{{\"text\": {}, \"output_file\": {}}}\n", json_escape(s), json_escape(&f.to_string_lossy())))
        .collect()
}

#[cfg(windows)]
fn quiet(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000)
}

#[cfg(not(windows))]
fn quiet(cmd: &mut Command) -> &mut Command {
    cmd
}

/// Membuat suara dari `text` ke `out_wav`. `length_scale` > 1 = lebih lambat; `gap` = jeda antar kalimat (detik).
#[allow(clippy::too_many_arguments)]
pub fn synthesize(piper_exe: &Path, model: &Path, ffmpeg: &str, ffprobe: &str, text: &str, length_scale: f64, gap: f64, work: &Path, out_wav: &Path) -> Result<TtsResult, String> {
    let sentences = split_sentences(text);
    if sentences.is_empty() {
        return Err("Naskah masih kosong.".into());
    }
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let files: Vec<PathBuf> = (0..sentences.len()).map(|i| work.join(format!("s{i}.wav"))).collect();
    for f in &files {
        let _ = std::fs::remove_file(f);
    }
    let mut child = quiet(
        Command::new(piper_exe)
            .args(["--model", &model.to_string_lossy(), "--json-input", "--length_scale", &format!("{:.3}", length_scale.clamp(0.5, 2.0))])
            .current_dir(piper_exe.parent().unwrap_or(Path::new("."))),
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| format!("Piper gagal dijalankan: {e}"))?;
    child.stdin.take().expect("stdin").write_all(piper_jsonl(&sentences, &files).as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() || files.iter().any(|f| !f.is_file()) {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Piper gagal membuat suara: {}", err.lines().rev().take(3).collect::<Vec<_>>().join(" / ")));
    }
    let mut durs = vec![];
    for f in &files {
        durs.push(crate::ffmpeg::probe(ffprobe, &f.to_string_lossy())?.duration);
    }
    let silence = work.join("jeda.wav");
    crate::ffmpeg::run(ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "anullsrc=r=22050:cl=mono", "-t", &format!("{gap:.3}"), "-c:a", "pcm_s16le", &silence.to_string_lossy()].map(String::from))?;
    let mut list = String::new();
    for (i, f) in files.iter().enumerate() {
        if i > 0 && gap > 0.0 {
            list.push_str(&format!("file '{}'\n", silence.to_string_lossy().replace('\\', "/")));
        }
        list.push_str(&format!("file '{}'\n", f.to_string_lossy().replace('\\', "/")));
    }
    let list_path = work.join("daftar.txt");
    std::fs::write(&list_path, list).map_err(|e| e.to_string())?;
    if let Some(dir) = out_wav.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    crate::ffmpeg::run(ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", &list_path.to_string_lossy(), "-ar", "48000", "-ac", "1", "-c:a", "pcm_s16le", &out_wav.to_string_lossy()].map(String::from))?;
    let mut t = 0.0;
    let mut timed = vec![];
    for (i, (s, d)) in sentences.iter().zip(&durs).enumerate() {
        if i > 0 {
            t += gap;
        }
        timed.push(Sentence { text: s.clone(), start: (t * 1000.0_f64).round() / 1000.0, end: ((t + d) * 1000.0_f64).round() / 1000.0 });
        t += d;
    }
    let _ = std::fs::remove_dir_all(work);
    Ok(TtsResult { path: out_wav.to_string_lossy().to_string(), duration: t, sentences: timed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences() {
        let s = split_sentences("Halo semua! Harga naik 3.5 persen. Kenapa?\nBaris baru tanpa titik\n\n  Akhir…  ");
        assert_eq!(s, vec!["Halo semua!", "Harga naik 3.5 persen.", "Kenapa?", "Baris baru tanpa titik", "Akhir…"]);
        assert!(split_sentences("   ").is_empty());
    }

    #[test]
    fn jsonl_escapes_text_and_paths() {
        let l = piper_jsonl(&["Kata \"kutip\" \\ miring".into()], &[PathBuf::from("C:\\x\\s0.wav")]);
        assert_eq!(l, "{\"text\": \"Kata \\\"kutip\\\" \\\\ miring\", \"output_file\": \"C:\\\\x\\\\s0.wav\"}\n");
    }
}
