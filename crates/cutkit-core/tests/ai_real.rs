//! Tes model AI sungguhan (Piper + Whisper). Butuh model di folder CUTKIT_MODELS
//! (bawaan D:\DevTools\cutkit-models), jadi ditandai ignore:
//!   cargo test -p cutkit-core --test ai_real -- --ignored --nocapture

use cutkit_core::ai::{self, model};
use cutkit_core::ffmpeg::{find_tools, run_ffmpeg_progress};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn models_dir() -> PathBuf {
    PathBuf::from(std::env::var("CUTKIT_MODELS").unwrap_or_else(|_| "D:/DevTools/cutkit-models".into()))
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

#[test]
#[ignore]
fn piper_then_whisper_roundtrip() {
    let tools = find_tools(None).expect("ffmpeg");
    let dir = models_dir();
    for id in ["piper-engine", "piper-id", "piper-id-config", "whisper-small"] {
        assert!(ai::is_installed(&dir, model(id).unwrap()), "{id} belum ada di {}", dir.display());
    }
    let work = std::env::temp_dir().join("cutkit-test").join("ai");
    let _ = std::fs::remove_dir_all(&work);
    let script = "Sebab sejarah selalu berulang. Pelabuhan menjadi kunci kekuatan kerajaan di Nusantara.";
    let wav = work.join("vo.wav");
    let res = cutkit_core::tts::synthesize(&ai::piper_exe(&dir), &dir.join(model("piper-id").unwrap().file), &tools.ffmpeg, &tools.ffprobe, script, 1.0, 0.3, &work.join("piper"), &wav).expect("tts");
    assert_eq!(res.sentences.len(), 2);
    assert!(res.duration > 3.0 && res.sentences[1].start > res.sentences[0].end, "{res:?}");

    let out = "cutkit-test-transcribe.json";
    let args = ai::transcribe_args(&wav.to_string_lossy(), 0.0, 0.0, model("whisper-small").unwrap().file, None, "id", out);
    let t0 = std::time::Instant::now();
    run_ffmpeg_progress(&tools.ffmpeg, &dir, &args, Arc::new(AtomicBool::new(false)), |_, _| {}).expect("whisper");
    eprintln!("whisper small: {:.1} detik untuk audio {:.1} detik", t0.elapsed().as_secs_f64(), res.duration);
    let text = std::fs::read_to_string(dir.join(out)).unwrap();
    let _ = std::fs::remove_file(dir.join(out));
    let words = ai::segments_to_words(&ai::parse_whisper_json(&text));
    eprintln!("{:?}", words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>());
    let expected: Vec<String> = script.split_whitespace().map(norm).collect();
    let heard: Vec<String> = words.iter().map(|w| norm(&w.text)).collect();
    let hits = expected.iter().filter(|w| heard.contains(w)).count();
    assert!(hits * 10 >= expected.len() * 7, "hanya {hits}/{} kata dikenali", expected.len());
    assert!(words.windows(2).all(|p| p[1].start >= p[0].start), "waktu kata harus urut");
    assert!(words.last().unwrap().end <= res.duration + 0.5);
}
