//! Tes model AI sungguhan (Piper + Whisper). Butuh model di folder CUTKIT_MODELS
//! (bawaan D:\DevTools\cutkit-models), jadi ditandai ignore:
//!   cargo test -p cutkit-core --test ai_real -- --ignored --nocapture

use cutkit_core::ai::{self, model};
use cutkit_core::*;
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

#[test]
#[ignore]
fn remove_background_image_and_video() {
    let tools = find_tools(None).expect("ffmpeg");
    let dir = models_dir();
    assert!(cutkit_core::matting::python_ready(&dir), "venv Python belum siap di {}", dir.display());
    let m = model("matting-fast").unwrap();
    assert!(ai::is_installed(&dir, m));
    let work = std::env::temp_dir().join("cutkit-test").join("matting");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let s = |p: &std::path::Path| p.to_string_lossy().to_string();
    // Lingkaran oranye di latar abu-abu polos.
    let img = work.join("bola.png");
    cutkit_core::ffmpeg::run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=0x9a9a9a:s=320x240:d=1,geq=r='if(lt(hypot(X-160,Y-120),70),250,154)':g='if(lt(hypot(X-160,Y-120),70),120,154)':b='if(lt(hypot(X-160,Y-120),70),30,154)'", "-frames:v", "1", "-update", "1", &s(&img)].map(String::from)).unwrap();
    let out = work.join("bola-transparan.png");
    let t0 = std::time::Instant::now();
    cutkit_core::matting::remove_bg_image(&dir, &dir.join(m.file), &s(&img), &out).expect("hapus background gambar");
    eprintln!("gambar: {:.1} detik", t0.elapsed().as_secs_f64());
    let alpha = |x: u32, y: u32| -> u8 {
        let o = std::process::Command::new(&tools.ffmpeg).args(["-hide_banner", "-loglevel", "error", "-i", &s(&out), "-vf", &format!("crop=1:1:{x}:{y},format=rgba"), "-f", "rawvideo", "-"]).output().unwrap();
        o.stdout[3]
    };
    assert!(alpha(160, 120) > 200, "tengah bola harus tetap terlihat: {}", alpha(160, 120));
    assert!(alpha(5, 5) < 60, "sudut latar harus transparan: {}", alpha(5, 5));

    // Video 1 detik: bola bergerak, hasilnya WebM beralpha.
    let vid = work.join("bola.mp4");
    cutkit_core::ffmpeg::run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-loop", "1", "-i", &s(&img), "-t", "1", "-r", "10", "-vf", "scroll=h=0.01", "-c:v", "libx264", "-pix_fmt", "yuv420p", &s(&vid)].map(String::from)).unwrap();
    let webm = work.join("bola.webm");
    let t0 = std::time::Instant::now();
    cutkit_core::matting::remove_bg_video(&tools.ffmpeg, &dir, &dir.join(m.file), &s(&vid), 0.0, 1.0, 10.0, false, &work.join("kerja"), &webm, Arc::new(AtomicBool::new(false)), |_, _| {}).expect("hapus background video");
    eprintln!("video 10 frame: {:.1} detik", t0.elapsed().as_secs_f64());
    let info = cutkit_core::ffmpeg::probe(&tools.ffprobe, &s(&webm)).unwrap();
    assert!(info.alpha, "WebM harus punya kanal alpha");
    assert_eq!(info.video_codec, "vp9");

    // Diekspor di atas latar biru: sudut biru (transparan), tengah oranye (bola).
    let mut p = Project::new("alpha");
    p.canvas = Canvas { width: 320, height: 240, fps: 10.0, background: "#0000ff".into() };
    p.assets.push(Asset { id: "w".into(), path: s(&webm), kind: AssetKind::Video, duration: info.duration, width: 320, height: 240, has_video: true, alpha: true, ..Default::default() });
    p.tracks[0].clips.push(Clip { id: "c".into(), asset_id: Some("w".into()), start: 0.0, duration: 0.9, ..Default::default() });
    let mp4 = work.join("alpha.mp4");
    let plan = build_plan(&p, &ExportOptions { output: s(&mp4), quality: Quality::Fast, ..Default::default() }).unwrap();
    cutkit_core::ffmpeg::run_export(&tools.ffmpeg, &work.join("ekspor"), &plan, Arc::new(AtomicBool::new(false)), |_| {}).expect("ekspor alpha");
    let px = |x: u32, y: u32| -> Vec<u8> {
        std::process::Command::new(&tools.ffmpeg).args(["-hide_banner", "-loglevel", "error", "-ss", "0.5", "-i", &s(&mp4), "-frames:v", "1", "-vf", &format!("crop=1:1:{x}:{y},format=rgb24"), "-f", "rawvideo", "-"]).output().unwrap().stdout
    };
    let corner = px(5, 5);
    let center = px(160, 120);
    assert!(corner[2] > 180 && corner[0] < 80, "sudut harus biru latar: {corner:?}");
    assert!(center[0] > 180 && center[2] < 120, "tengah harus oranye bola: {center:?}");
}
