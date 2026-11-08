//! Tes ekspor sungguhan: membuat klip sintetis dengan ffmpeg, menyusun proyek
//! (video utama, overlay gambar berputar, SFX, teks, caption), mengekspor, lalu
//! memeriksa hasilnya dengan ffprobe. Dilewati bila ffmpeg tidak ditemukan.

use cutkit_core::ffmpeg::{find_tools, probe, run, run_export, run_job};
use cutkit_core::*;
use cutkit_core::model::Keyframe;
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
    let plan = build_plan(&p, &ExportOptions { output: s(&out), encoder: Encoder::X264, quality: Quality::Fast, ..Default::default() }).unwrap();
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

#[test]
fn demo_media_is_generated() {
    let Some(tools) = find_tools(None) else {
        eprintln!("ffmpeg tidak ditemukan, tes media contoh dilewati");
        return;
    };
    let dir = tmp_dir("demo");
    let (media, jobs) = cutkit_core::demo::demo_jobs(&dir, cutkit_core::ffmpeg::has_filter(&tools.ffmpeg, "flite"));
    for (_, args) in &jobs {
        run(&tools.ffmpeg, args).expect("media contoh dibuat");
    }
    let voice = probe(&tools.ffprobe, &media.voice).unwrap();
    assert_eq!(voice.kind, AssetKind::Audio);
    assert!(voice.duration > 3.0, "narasi TTS terlalu pendek: {}", voice.duration);
    let kinds: Vec<AssetKind> = media.visuals.iter().map(|v| probe(&tools.ffprobe, v).unwrap().kind).collect();
    assert_eq!(kinds, vec![AssetKind::Image, AssetKind::Video, AssetKind::Image, AssetKind::Video, AssetKind::Image]);
    let img = probe(&tools.ffprobe, &media.visuals[0]).unwrap();
    assert_eq!((img.width, img.height), (1080, 1920));
    assert!((probe(&tools.ffprobe, &media.music).unwrap().duration - 20.0).abs() < 0.1);
}

/// Warna satu piksel (RGB) pada waktu `t` di file video.
fn pixel(ffmpeg: &str, file: &str, t: f64, x: u32, y: u32) -> [u8; 3] {
    let out = std::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-ss", &format!("{t}"), "-i", file, "-frames:v", "1", "-vf", &format!("crop=1:1:{x}:{y},format=rgb24"), "-f", "rawvideo", "-"])
        .output()
        .unwrap();
    assert!(out.status.success() && out.stdout.len() >= 3, "{}", String::from_utf8_lossy(&out.stderr));
    [out.stdout[0], out.stdout[1], out.stdout[2]]
}

/// Loudness terintegrasi (LUFS) menurut filter ebur128.
fn integrated_lufs(ffmpeg: &str, file: &str) -> f64 {
    let out = std::process::Command::new(ffmpeg).args(["-hide_banner", "-nostats", "-i", file, "-af", "ebur128", "-f", "null", "-"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    let summary = &text[text.rfind("Summary:").expect("ringkasan ebur128")..];
    let line = summary.lines().find(|l| l.trim_start().starts_with("I:")).unwrap();
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[test]
fn export_with_effects() {
    let Some(tools) = find_tools(None) else {
        eprintln!("ffmpeg tidak ditemukan, tes efek dilewati");
        return;
    };
    let dir = tmp_dir("effects");
    let s = |p: &PathBuf| p.to_string_lossy().to_string();
    let ff = |args: &[&str]| run(&tools.ffmpeg, &args.iter().map(|a| a.to_string()).collect::<Vec<_>>()).unwrap();
    let land = dir.join("lanskap.mp4");
    ff(&["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=640x360:r=30:d=4", "-f", "lavfi", "-i", "sine=f=440:d=4", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", &s(&land)]);
    let green = dir.join("greenscreen.mp4");
    ff(&["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=0x00ff00:s=320x240:r=30:d=3,drawbox=x=110:y=70:w=100:h=100:color=blue:t=fill", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", &s(&green)]);
    let grad = dir.join("gradasi.png");
    ff(&["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "nullsrc=s=400x400:d=1,geq=lum='X*255/W':cb=128:cr=128", "-frames:v", "1", "-update", "1", &s(&grad)]);

    let mut p = Project::new("efek");
    p.canvas = Canvas { width: 540, height: 960, fps: 30.0, background: "#000000".into() };
    for (id, path, kind) in [("l", &land, AssetKind::Video), ("g", &green, AssetKind::Video), ("i", &grad, AssetKind::Image)] {
        let info = probe(&tools.ffprobe, &s(path)).unwrap();
        p.assets.push(Asset { id: id.into(), path: s(path), kind, duration: info.duration, width: info.width, height: info.height, has_audio: info.has_audio, has_video: true, ..Default::default() });
    }
    // Lanskap 4 detik diputar 2x (2 detik), latar blur.
    p.tracks[0].clips.push(Clip { id: "a".into(), asset_id: Some("l".into()), start: 0.0, duration: 2.0, speed: 2.0, background: "blur".into(), ..Default::default() });
    // Gradasi horizontal dengan zoom masuk kuat, memenuhi kanvas.
    p.tracks[0].clips.push(Clip { id: "b".into(), kind: ClipKind::Image, asset_id: Some("i".into()), start: 2.0, duration: 2.0, fit: Fit::Cover, motion: Motion { kind: "zoom-in".into(), amount: 1.0 }, ..Default::default() });
    // Green screen di atas, latar hijaunya dihapus.
    p.tracks[1].clips.push(Clip {
        id: "c".into(),
        asset_id: Some("g".into()),
        start: 0.0,
        duration: 2.0,
        transform: Transform { y: -250.0, scale: 0.5, ..Default::default() },
        chroma: Chroma { enabled: true, ..Default::default() },
        ..Default::default()
    });
    p.captions.enabled = true;
    p.captions.style.animation = "pop".into();
    p.captions.words = vec![Word { text: "efek".into(), start: 0.0, end: 1.0 }, Word { text: "keren".into(), start: 1.0, end: 4.0 }];

    let out = dir.join("efek.mp4");
    let plan = build_plan(&p, &ExportOptions { output: s(&out), quality: Quality::Fast, normalize_loudness: true, ..Default::default() }).unwrap();
    run_export(&tools.ffmpeg, &dir.join("kerja"), &plan, Arc::new(AtomicBool::new(false)), |_| {}).expect("ekspor efek");
    let o = s(&out);
    let info = probe(&tools.ffprobe, &o).unwrap();
    assert!((info.duration - 4.0).abs() < 0.15, "durasi {}", info.duration);

    // Green screen: placement 270x203 di (135,129). Sudut klip (dulu hijau) kini transparan,
    // kotak biru di tengahnya tetap ada.
    let corner = pixel(&tools.ffmpeg, &o, 1.0, 145, 139);
    assert!(!(corner[1] > 180 && corner[0] < 80 && corner[2] < 80), "latar hijau masih terlihat: {corner:?}");
    let center = pixel(&tools.ffmpeg, &o, 1.0, 270, 230);
    assert!(center[2] > 150 && center[1] < 100, "kotak biru hilang: {center:?}");

    // Latar blur: di atas video lanskap (y=10) ada isi blur, bukan hitam polos.
    let above = pixel(&tools.ffmpeg, &o, 1.0, 270, 820);
    assert!(above.iter().map(|&c| c as u32).sum::<u32>() > 60, "latar blur kosong: {above:?}");

    // Zoom masuk sampai 2x pada gradasi horizontal: di x=100 pandangan bergeser ke arah tengah,
    // jadi nilainya naik dari ~82 (tanpa zoom) ke ~105 (zoom 2x).
    let early = pixel(&tools.ffmpeg, &o, 2.05, 100, 300)[0] as u32;
    let late = pixel(&tools.ffmpeg, &o, 3.9, 100, 300)[0] as u32;
    assert!(late > early + 12, "zoom tidak terlihat: awal {early}, akhir {late}");

    // Loudness diratakan mendekati -14 LUFS.
    let lufs = integrated_lufs(&tools.ffmpeg, &o);
    assert!((lufs + 14.0).abs() < 3.0, "loudness {lufs} LUFS");
}

#[test]
fn long_project_exports_in_parts() {
    let Some(tools) = find_tools(None) else {
        eprintln!("ffmpeg tidak ditemukan, tes video panjang dilewati");
        return;
    };
    let dir = tmp_dir("long");
    let s = |p: &PathBuf| p.to_string_lossy().to_string();
    let ff = |args: &[&str]| run(&tools.ffmpeg, &args.iter().map(|a| a.to_string()).collect::<Vec<_>>()).unwrap();
    let clip = dir.join("klip.mp4");
    ff(&["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=320x180:r=30:d=3", "-f", "lavfi", "-i", "sine=f=330:d=3", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", &s(&clip)]);
    let logo = dir.join("logo.png");
    ff(&["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "nullsrc=s=200x200:d=1,geq=lum='X*255/W':cb=128:cr=128", "-frames:v", "1", "-update", "1", &s(&logo)]);
    let vi = probe(&tools.ffprobe, &s(&clip)).unwrap();

    let mut p = Project::new("panjang");
    p.canvas = Canvas { width: 640, height: 360, fps: 30.0, background: "#000000".into() };
    p.assets.push(Asset { id: "v".into(), path: s(&clip), kind: AssetKind::Video, duration: vi.duration, width: 320, height: 180, has_audio: true, has_video: true, ..Default::default() });
    p.assets.push(Asset { id: "l".into(), path: s(&logo), kind: AssetKind::Image, width: 200, height: 200, has_video: true, ..Default::default() });
    for i in 0..20 {
        p.tracks[0].clips.push(Clip { id: format!("c{i}"), asset_id: Some("v".into()), start: i as f64, duration: 1.0, source_in: (i % 2) as f64, ..Default::default() });
    }
    // Logo kecil di pojok sepanjang video, zoom pelan melintasi batas bagian.
    p.tracks[1].clips.push(Clip {
        id: "logo".into(),
        kind: ClipKind::Image,
        asset_id: Some("l".into()),
        start: 0.0,
        duration: 20.0,
        transform: Transform { x: 220.0, y: -100.0, scale: 0.3, ..Default::default() },
        motion: Motion { kind: "zoom-in".into(), amount: 1.0 },
        ..Default::default()
    });
    p.captions.enabled = true;
    p.captions.style.size = 30.0;
    p.captions.words = (0..10).map(|i| Word { text: format!("kata{i}"), start: i as f64 * 2.0, end: i as f64 * 2.0 + 1.8 }).collect();

    let out = dir.join("panjang.mp4");
    let job = build_job(&p, &ExportOptions { output: s(&out), quality: Quality::Fast, ..Default::default() }).unwrap();
    assert!(job.chunked, "20 klip visual > batas, harus bertahap");
    assert_eq!(job.steps.len(), 4, "audio + 2 bagian + gabung");
    let mut last = 0.0;
    let mut steps_seen = std::collections::BTreeSet::new();
    run_job(&tools.ffmpeg, &dir.join("kerja"), &job, Arc::new(AtomicBool::new(false)), |pr| {
        assert!(pr.percent + 1e-9 >= last, "progres mundur: {} -> {}", last, pr.percent);
        last = pr.percent;
        steps_seen.insert(pr.step.clone());
    })
    .expect("ekspor bertahap");
    assert!(steps_seen.contains("Video bagian 2/2") && steps_seen.contains("Menggabungkan"));
    let info = probe(&tools.ffprobe, &s(&out)).unwrap();
    assert!((info.duration - 20.0).abs() < 0.15, "durasi {}", info.duration);
    assert!(info.has_audio && info.playable);
    // Jumlah frame tepat 20 x 30 (bagian digabung tanpa frame hilang/dobel).
    let frames = std::process::Command::new(&tools.ffprobe)
        .args(["-v", "error", "-count_frames", "-select_streams", "v:0", "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0", &s(&out)])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&frames.stdout).trim(), "600");
    // Zoom logo bersambung di batas bagian (detik 16): tidak melompat kembali ke awal.
    let before = pixel(&tools.ffmpeg, &s(&out), 15.9, 515, 55)[0] as i32;
    let after = pixel(&tools.ffmpeg, &s(&out), 16.1, 515, 55)[0] as i32;
    let start = pixel(&tools.ffmpeg, &s(&out), 0.1, 515, 55)[0] as i32;
    assert!((before - after).abs() < 12, "zoom melompat di batas bagian: {before} -> {after}");
    assert!((after - start).abs() > 12, "zoom tidak berjalan: awal {start}, detik 16 {after}");
}

#[test]
fn transitions_render_between_clips() {
    let Some(tools) = find_tools(None) else {
        eprintln!("ffmpeg tidak ditemukan, tes transisi dilewati");
        return;
    };
    let dir = tmp_dir("transisi");
    let s = |p: &PathBuf| p.to_string_lossy().to_string();
    let mk = |name: &str, color: &str| {
        let out = dir.join(name);
        run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", &format!("color=c={color}:s=160x90:r=30:d=4"), "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", &s(&out)].map(String::from)).unwrap();
        out
    };
    let red = mk("merah.mp4", "red");
    let blue = mk("biru.mp4", "blue");
    let mut p = Project::new("transisi");
    p.canvas = Canvas { width: 320, height: 180, fps: 30.0, background: "#000000".into() };
    for (id, path) in [("r", &red), ("b", &blue)] {
        p.assets.push(Asset { id: id.into(), path: s(path), kind: AssetKind::Video, duration: 4.0, width: 160, height: 90, has_video: true, ..Default::default() });
    }
    // Merah 0-3 detik, biru menumpuk 1 detik (mulai 2) dengan crossfade, lalu biru ke merah dengan dorong ke kiri.
    p.tracks[0].clips.push(Clip { id: "a".into(), asset_id: Some("r".into()), start: 0.0, duration: 3.0, ..Default::default() });
    p.tracks[0].clips.push(Clip { id: "b".into(), asset_id: Some("b".into()), start: 2.0, duration: 3.0, transition: Transition { kind: "fade".into(), duration: 1.0 }, ..Default::default() });
    p.tracks[0].clips.push(Clip { id: "c".into(), asset_id: Some("r".into()), start: 4.0, duration: 2.0, transition: Transition { kind: "push-left".into(), duration: 1.0 }, ..Default::default() });
    let out = dir.join("transisi.mp4");
    let plan = build_plan(&p, &ExportOptions { output: s(&out), quality: Quality::Fast, ..Default::default() }).unwrap();
    run_export(&tools.ffmpeg, &dir.join("kerja"), &plan, Arc::new(AtomicBool::new(false)), |_| {}).expect("ekspor transisi");
    let o = s(&out);
    let before = pixel(&tools.ffmpeg, &o, 1.5, 160, 90);
    let mid = pixel(&tools.ffmpeg, &o, 2.5, 160, 90);
    let after = pixel(&tools.ffmpeg, &o, 3.5, 160, 90);
    assert!(before[0] > 200 && before[2] < 60, "sebelum transisi merah: {before:?}");
    assert!(mid[0] > 70 && mid[2] > 70, "tengah crossfade campuran merah-biru: {mid:?}");
    assert!(after[2] > 200 && after[0] < 60, "sesudah transisi biru: {after:?}");
    // Dorong di detik 4.5 (tengah): biru terdorong ke kiri, merah masuk dari kanan.
    let left = pixel(&tools.ffmpeg, &o, 4.5, 40, 90);
    let right = pixel(&tools.ffmpeg, &o, 4.5, 280, 90);
    assert!(left[2] > 200 && left[0] < 60, "kiri masih biru: {left:?}");
    assert!(right[0] > 200 && right[2] < 60, "kanan sudah merah: {right:?}");
    let info = probe(&tools.ffprobe, &o).unwrap();
    assert!((info.duration - 6.0).abs() < 0.15, "durasi {}", info.duration);
}

#[test]
fn color_adjust_and_lut_render() {
    let Some(tools) = find_tools(None) else {
        eprintln!("ffmpeg tidak ditemukan, tes warna dilewati");
        return;
    };
    let dir = tmp_dir("warna");
    let s = |p: &PathBuf| p.to_string_lossy().to_string();
    let red = dir.join("merah.png");
    run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=0xff0000:s=64x64:d=1", "-frames:v", "1", "-update", "1", &s(&red)].map(String::from)).unwrap();
    // LUT 2x2x2 pembalik warna (merah -> cyan).
    let lut = dir.join("negatif lut.cube");
    std::fs::write(&lut, "LUT_3D_SIZE 2\n1 1 1\n0 1 1\n1 0 1\n0 0 1\n1 1 0\n0 1 0\n1 0 0\n0 0 0\n").unwrap();
    let mut p = Project::new("warna");
    p.canvas = Canvas { width: 128, height: 64, fps: 30.0, background: "#000000".into() };
    p.assets.push(Asset { id: "r".into(), path: s(&red), kind: AssetKind::Image, width: 64, height: 64, has_video: true, ..Default::default() });
    // Kiri: hitam putih. Kanan: LUT negatif.
    p.tracks[0].clips.push(Clip { id: "a".into(), kind: ClipKind::Image, asset_id: Some("r".into()), start: 0.0, duration: 1.0, transform: Transform { x: -32.0, ..Default::default() }, color: cutkit_core::color::ColorAdjust { saturation: 0.0, ..Default::default() }, ..Default::default() });
    p.tracks[1].clips.push(Clip { id: "b".into(), kind: ClipKind::Image, asset_id: Some("r".into()), start: 0.0, duration: 1.0, transform: Transform { x: 32.0, ..Default::default() }, color: cutkit_core::color::ColorAdjust { lut: s(&lut), ..Default::default() }, ..Default::default() });
    let out = dir.join("warna.mp4");
    let plan = build_plan(&p, &ExportOptions { output: s(&out), quality: Quality::Fast, ..Default::default() }).unwrap();
    run_export(&tools.ffmpeg, &dir.join("kerja"), &plan, Arc::new(AtomicBool::new(false)), |_| {}).expect("ekspor warna");
    let gray = pixel(&tools.ffmpeg, &s(&out), 0.5, 32, 32);
    let cyan = pixel(&tools.ffmpeg, &s(&out), 0.5, 96, 32);
    // 0.213 x 255 = 54: merah tanpa saturasi jadi abu-abu gelap.
    assert!((gray[0] as i32 - gray[1] as i32).abs() < 12 && (gray[0] as i32 - 54).abs() < 16, "abu-abu: {gray:?}");
    assert!(cyan[0] < 50 && cyan[1] > 200 && cyan[2] > 200, "cyan: {cyan:?}");
}

#[test]
fn every_effect_renders() {
    let Some(tools) = find_tools(None) else {
        eprintln!("ffmpeg tidak ditemukan, tes efek video dilewati");
        return;
    };
    let dir = tmp_dir("efek-video");
    let s = |p: &PathBuf| p.to_string_lossy().to_string();
    let img = dir.join("merah.png");
    run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=0xff2000:s=160x90:d=1", "-frames:v", "1", "-update", "1", &s(&img)].map(String::from)).unwrap();
    let mut p = Project::new("efek video");
    p.canvas = Canvas { width: 160, height: 90, fps: 30.0, background: "#000000".into() };
    p.assets.push(Asset { id: "i".into(), path: s(&img), kind: AssetKind::Image, width: 160, height: 90, has_video: true, ..Default::default() });
    let n = cutkit_core::model::EFFECTS.len();
    // Satu detik terakhir tanpa efek untuk memastikan efek tidak bocor.
    p.tracks[0].clips.push(Clip { id: "bg".into(), kind: ClipKind::Image, asset_id: Some("i".into()), start: 0.0, duration: n as f64 + 1.0, ..Default::default() });
    for (i, k) in cutkit_core::model::EFFECTS.iter().enumerate() {
        p.tracks[1].clips.push(Clip { id: format!("e{i}"), kind: ClipKind::Effect, start: i as f64, duration: 1.0, effect: Some(Effect { kind: k.to_string(), intensity: 0.6 }), ..Default::default() });
    }
    let out = dir.join("efek.mp4");
    let plan = build_plan(&p, &ExportOptions { output: s(&out), quality: Quality::Fast, ..Default::default() }).unwrap();
    run_export(&tools.ffmpeg, &dir.join("kerja"), &plan, Arc::new(AtomicBool::new(false)), |_| {}).expect("semua efek valid di ffmpeg");
    let o = s(&out);
    let at = |name: &str| cutkit_core::model::EFFECTS.iter().position(|k| *k == name).unwrap() as f64 + 0.5;
    let bw = pixel(&tools.ffmpeg, &o, at("bw"), 80, 45);
    assert!((bw[0] as i32 - bw[2] as i32).abs() < 15, "hitam putih: {bw:?}");
    let neg = pixel(&tools.ffmpeg, &o, at("negate"), 80, 45);
    assert!(neg[0] < 60 && neg[2] > 180, "negatif dari merah = cyan: {neg:?}");
    let bar = pixel(&tools.ffmpeg, &o, at("letterbox"), 80, 2);
    assert!(bar.iter().all(|&c| c < 30), "letterbox hitam di atas: {bar:?}");
    let plain = pixel(&tools.ffmpeg, &o, n as f64 + 0.5, 80, 45);
    assert!(plain[0] > 200, "efek tidak bocor di luar waktunya: {plain:?}");
}

#[test]
fn keyframes_render_motion_and_fade() {
    let Some(tools) = find_tools(None) else {
        eprintln!("ffmpeg tidak ditemukan, tes keyframe dilewati");
        return;
    };
    let dir = tmp_dir("keyframe");
    let s = |p: &PathBuf| p.to_string_lossy().to_string();
    let sq = dir.join("kotak.png");
    run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=0xff0000:s=40x40:d=1", "-frames:v", "1", "-update", "1", &s(&sq)].map(String::from)).unwrap();
    let mut p = Project::new("keyframe");
    p.canvas = Canvas { width: 320, height: 180, fps: 30.0, background: "#000000".into() };
    p.assets.push(Asset { id: "k".into(), path: s(&sq), kind: AssetKind::Image, width: 40, height: 40, has_video: true, ..Default::default() });
    // Kotak 40 px (skala 0.25 dari tinggi kanvas penuh 180 = 45 px) bergerak -100 -> +100 sambil memudar.
    p.tracks[0].clips.push(Clip {
        id: "a".into(),
        kind: ClipKind::Image,
        asset_id: Some("k".into()),
        start: 0.0,
        duration: 2.0,
        keyframes: vec![
            Keyframe { t: 0.0, x: -100.0, scale: 0.25, opacity: 1.0, ..Default::default() },
            Keyframe { t: 2.0, x: 100.0, scale: 0.25, opacity: 0.2, ..Default::default() },
        ],
        ..Default::default()
    });
    let out = dir.join("keyframe.mp4");
    let plan = build_plan(&p, &ExportOptions { output: s(&out), quality: Quality::Fast, ..Default::default() }).unwrap();
    run_export(&tools.ffmpeg, &dir.join("kerja"), &plan, Arc::new(AtomicBool::new(false)), |_| {}).expect("ekspor keyframe");
    let o = s(&out);
    let start_left = pixel(&tools.ffmpeg, &o, 0.05, 160 - 98, 90);
    let start_right = pixel(&tools.ffmpeg, &o, 0.05, 160 + 98, 90);
    let end_right = pixel(&tools.ffmpeg, &o, 1.95, 160 + 98, 90);
    assert!(start_left[0] > 200, "awal: kotak di kiri {start_left:?}");
    assert!(start_right[0] < 40, "awal: kanan kosong {start_right:?}");
    assert!(end_right[0] > 30 && end_right[0] < 110, "akhir: kotak di kanan dan memudar ke ~20%: {end_right:?}");
}

#[test]
fn every_transition_renders() {
    let Some(tools) = find_tools(None) else {
        eprintln!("ffmpeg tidak ditemukan, tes semua transisi dilewati");
        return;
    };
    let dir = tmp_dir("semua-transisi");
    let s = |p: &PathBuf| p.to_string_lossy().to_string();
    let mk = |name: &str, color: &str| {
        let out = dir.join(name);
        run(&tools.ffmpeg, &["-hide_banner", "-y", "-loglevel", "error", "-f", "lavfi", "-i", &format!("color=c={color}:s=320x192:r=30:d=3"), "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", &s(&out)].map(String::from)).unwrap();
        out
    };
    let red = mk("merah.mp4", "red");
    let blue = mk("biru.mp4", "blue");
    let is_red = |c: [u8; 3]| c[0] > 190 && c[2] < 70;
    let is_blue = |c: [u8; 3]| c[2] > 190 && c[0] < 70;
    let mixed = |c: [u8; 3]| c[0] > 50 && c[2] > 50;
    for kind in cutkit_core::transition::KINDS {
        let mut p = Project::new("transisi");
        p.canvas = Canvas { width: 320, height: 192, fps: 30.0, background: "#000000".into() };
        for (id, path) in [("r", &red), ("b", &blue)] {
            p.assets.push(Asset { id: id.into(), path: s(path), kind: AssetKind::Video, duration: 3.0, width: 320, height: 192, has_video: true, ..Default::default() });
        }
        // Merah 0-2 detik; biru menumpuk mulai detik 1 dengan transisi 1 detik, jadi tengahnya di 1.5.
        p.tracks[0].clips.push(Clip { id: "a".into(), asset_id: Some("r".into()), start: 0.0, duration: 2.0, ..Default::default() });
        p.tracks[0].clips.push(Clip { id: "b".into(), asset_id: Some("b".into()), start: 1.0, duration: 2.0, transition: Transition { kind: kind.to_string(), duration: 1.0 }, ..Default::default() });
        let out = dir.join(format!("{kind}.mp4"));
        let plan = build_plan(&p, &ExportOptions { output: s(&out), quality: Quality::Fast, ..Default::default() }).unwrap();
        run_export(&tools.ffmpeg, &dir.join(format!("kerja-{kind}")), &plan, Arc::new(AtomicBool::new(false)), |_| {}).unwrap_or_else(|e| panic!("{kind}: {e}"));
        let o = s(&out);
        let px = |t: f64, x: u32, y: u32| pixel(&tools.ffmpeg, &o, t, x, y);
        assert!(is_red(px(0.5, 160, 96)), "{kind}: sebelum transisi merah");
        assert!(is_blue(px(2.5, 160, 96)), "{kind}: sesudah transisi biru {:?}", px(2.5, 160, 96));
        let (l, r, top, bot, c) = (px(1.5, 30, 96), px(1.5, 290, 96), px(1.5, 160, 20), px(1.5, 160, 172), px(1.5, 160, 96));
        match *kind {
            "fade" | "blur" | "zoom" | "spin" => assert!(mixed(c), "{kind}: tengah campuran {c:?}"),
            "dip-black" => assert!(c.iter().all(|v| *v < 45), "{kind}: hitam di tengah {c:?}"),
            "dip-white" => assert!(c.iter().all(|v| *v > 210), "{kind}: putih di tengah {c:?}"),
            "slide-left" | "push-left" | "whip-left" | "wipe-left" | "clock" => assert!(is_red(l) && is_blue(r), "{kind}: kiri merah {l:?}, kanan biru {r:?}"),
            "slide-right" | "push-right" | "whip-right" | "wipe-right" => assert!(is_blue(l) && is_red(r), "{kind}: kiri biru {l:?}, kanan merah {r:?}"),
            "slide-up" | "wipe-up" => assert!(is_red(top) && is_blue(bot), "{kind}: atas merah {top:?}, bawah biru {bot:?}"),
            "slide-down" | "wipe-down" => assert!(is_blue(top) && is_red(bot), "{kind}: atas biru {top:?}, bawah merah {bot:?}"),
            "iris" => assert!(is_blue(c) && is_red(px(1.5, 4, 4)), "{kind}: tengah biru, sudut merah"),
            "dissolve" => {
                let blues = (0..24).filter(|i| { let c = px(1.5, 13 + (i % 6) * 53, 20 + (i / 6) * 45); c[2] > c[0] }).count();
                assert!((5..=19).contains(&blues), "{kind}: sebagian biru, sebagian merah ({blues}/24)");
            }
            "pixelate" | "glitch" => {
                assert!(is_red(px(1.3, 160, 96)), "{kind}: sebelum tengah masih merah");
                assert!(is_blue(px(1.7, 160, 96)), "{kind}: sesudah tengah biru {:?}", px(1.7, 160, 96));
            }
            other => panic!("transisi {other} belum dites"),
        }
    }
}
