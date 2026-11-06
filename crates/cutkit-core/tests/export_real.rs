//! Tes ekspor sungguhan: membuat klip sintetis dengan ffmpeg, menyusun proyek
//! (video utama, overlay gambar berputar, SFX, teks, caption), mengekspor, lalu
//! memeriksa hasilnya dengan ffprobe. Dilewati bila ffmpeg tidak ditemukan.

use cutkit_core::ffmpeg::{find_tools, probe, run, run_export};
use cutkit_core::*;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("cutkit-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn export_synthetic_project() {
    let Some(tools) = find_tools(None) else {
        eprintln!("ffmpeg tidak ditemukan, tes ekspor dilewati");
        return;
    };
    let dir = tmp_dir("export");
    let s = |p: &PathBuf| p.to_string_lossy().to_string();

    // Klip sintetis: video testsrc 3 detik + nada 440 Hz, gambar PNG, SFX 1 detik.
    let video = dir.join("klip.mp4");
    run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=640x360:r=30:d=3", "-f", "lavfi", "-i", "sine=f=440:d=3", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", &s(&video)].map(String::from)).unwrap();
    let image = dir.join("meme.png");
    run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=red@0.8:s=200x120:d=1", "-frames:v", "1", "-update", "1", &s(&image)].map(String::from)).unwrap();
    let sfx = dir.join("boom.wav");
    run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=f=880:d=1", &s(&sfx)].map(String::from)).unwrap();

    let vi = probe(&tools.ffprobe, &s(&video)).unwrap();
    assert_eq!(vi.kind, AssetKind::Video);
    assert!((vi.duration - 3.0).abs() < 0.2, "durasi {}", vi.duration);
    assert!(vi.has_audio && vi.playable);
    let ii = probe(&tools.ffprobe, &s(&image)).unwrap();
    assert_eq!(ii.kind, AssetKind::Image);
    assert_eq!((ii.width, ii.height), (200, 120));
    let ai = probe(&tools.ffprobe, &s(&sfx)).unwrap();
    assert_eq!(ai.kind, AssetKind::Audio);

    let mut p = Project::new("tes");
    p.canvas = Canvas { width: 540, height: 960, fps: 30.0, background: "#101020".into() };
    p.assets.push(Asset { id: "v".into(), path: s(&video), kind: AssetKind::Video, duration: vi.duration, width: vi.width, height: vi.height, has_audio: true, has_video: true, ..Default::default() });
    p.assets.push(Asset { id: "i".into(), path: s(&image), kind: AssetKind::Image, width: 200, height: 120, has_video: true, ..Default::default() });
    p.assets.push(Asset { id: "a".into(), path: s(&sfx), kind: AssetKind::Audio, duration: 1.0, has_audio: true, ..Default::default() });
    // Video utama dipotong dua: 0-1.5 dari detik 1, lalu 1.5-3 dari detik 0, jeda hitam 0.5 detik, lalu 1 detik lagi.
    p.tracks[0].clips.push(Clip { id: "c1".into(), asset_id: Some("v".into()), start: 0.0, duration: 1.5, source_in: 1.0, fade_in: 0.3, ..Default::default() });
    p.tracks[0].clips.push(Clip { id: "c2".into(), asset_id: Some("v".into()), start: 1.5, duration: 1.5, source_in: 0.0, volume: 0.4, ..Default::default() });
    p.tracks[0].clips.push(Clip { id: "c3".into(), asset_id: Some("v".into()), start: 3.5, duration: 1.0, source_in: 0.5, fit: Fit::Cover, ..Default::default() });
    p.tracks[1].clips.push(Clip {
        id: "o1".into(),
        kind: ClipKind::Image,
        asset_id: Some("i".into()),
        start: 0.5,
        duration: 2.0,
        transform: Transform { x: 60.0, y: -250.0, scale: 0.6, rotation: 20.0, opacity: 0.7 },
        ..Default::default()
    });
    p.tracks[1].clips.push(Clip {
        id: "t1".into(),
        kind: ClipKind::Text,
        start: 0.0,
        duration: 4.5,
        transform: Transform { y: -350.0, ..Default::default() },
        text: Some(TextClip { text: "Judul Tes".into(), style: TextStyle { size: 48.0, background: "#00000080".into(), ..Default::default() } }),
        ..Default::default()
    });
    p.tracks[2].clips.push(Clip { id: "s1".into(), kind: ClipKind::Audio, asset_id: Some("a".into()), start: 2.0, duration: 1.0, fade_out: 0.5, ..Default::default() });
    p.captions.enabled = true;
    p.captions.style.size = 40.0;
    p.captions.words = ["ini", "caption", "kata", "per", "kata", "ya"]
        .iter()
        .enumerate()
        .map(|(i, t)| Word { text: t.to_string(), start: i as f64 * 0.5, end: i as f64 * 0.5 + 0.45 })
        .collect();

    let out = dir.join("hasil").join("tes.mp4");
    let plan = build_plan(&p, &ExportOptions { output: s(&out), encoder: Encoder::X264, quality: Quality::Fast }).unwrap();
    assert!(plan.ass.is_some());
    let mut reports = vec![];
    run_export(&tools.ffmpeg, &dir.join("kerja"), &plan, Arc::new(AtomicBool::new(false)), |p| reports.push(p)).expect("ekspor jalan");
    let last = reports.last().expect("ada laporan progres");
    assert!(last.done && (last.percent - 100.0).abs() < 1e-9);

    let info = probe(&tools.ffprobe, &s(&out)).unwrap();
    assert_eq!((info.width, info.height), (540, 960));
    assert_eq!(info.video_codec, "h264");
    assert_eq!(info.audio_codec, "aac");
    assert!((info.duration - 4.5).abs() < 0.15, "durasi hasil {}", info.duration);
    assert!(info.playable);

    // Pembatalan: flag sudah true sebelum mulai, file hasil tidak boleh tertinggal.
    let out2 = dir.join("batal.mp4");
    let plan2 = build_plan(&p, &ExportOptions { output: s(&out2), ..Default::default() }).unwrap();
    let err = run_export(&tools.ffmpeg, &dir.join("kerja"), &plan2, Arc::new(AtomicBool::new(true)), |_| {}).unwrap_err();
    assert!(err.contains("dibatalkan"));
    assert!(!out2.exists());
}
