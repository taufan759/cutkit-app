//! Mengekspor file proyek dari baris perintah (tanpa UI), berguna untuk debugging:
//!   cargo run -p cutkit-core --example render -- proyek.cutkit.json hasil.mp4 [fast|balanced|best] [--loudnorm]
use cutkit_core::ffmpeg::{find_tools, run_export};
use cutkit_core::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("pakai: render <proyek.json> <hasil.mp4> [fast|balanced|best]");
        std::process::exit(2);
    }
    let project = Project::from_json(&std::fs::read_to_string(&args[1]).expect("baca proyek")).expect("proyek valid");
    let quality = match args.get(3).map(String::as_str) {
        Some("fast") => Quality::Fast,
        Some("best") => Quality::Best,
        _ => Quality::Balanced,
    };
    let tools = find_tools(None).expect("ffmpeg tidak ditemukan");
    let plan = build_plan(&project, &ExportOptions { output: args[2].clone(), encoder: Encoder::X264, quality, normalize_loudness: args.iter().any(|a| a == "--loudnorm"), fonts_dir: String::new() }).expect("rencana ekspor");
    let work = std::env::temp_dir().join("cutkit-render");
    let start = std::time::Instant::now();
    run_export(&tools.ffmpeg, &work, &plan, Arc::new(AtomicBool::new(false)), |p| eprint!("\r{:5.1}%  {}   ", p.percent, p.speed)).unwrap_or_else(|e| {
        eprintln!("\ngagal: {e}");
        std::process::exit(1);
    });
    eprintln!("\nselesai dalam {:.1} detik: {}", start.elapsed().as_secs_f64(), args[2]);
}
