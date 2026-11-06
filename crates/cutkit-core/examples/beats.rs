//! Deteksi tempo dan ketukan dari baris perintah:
//!   cargo run -p cutkit-core --example beats -- musik.mp3
fn main() {
    let path = std::env::args().nth(1).expect("pakai: beats <file audio>");
    let tools = cutkit_core::find_tools(None).expect("ffmpeg tidak ditemukan");
    match cutkit_core::beats::detect(&tools.ffmpeg, &path, 0.0, 0.0) {
        Ok(Some(info)) => println!("{} BPM, {} ketukan, awal: {:?}", info.bpm, info.beats.len(), &info.beats[..info.beats.len().min(8)]),
        Ok(None) => println!("tidak ada ketukan yang jelas"),
        Err(e) => println!("gagal: {e}"),
    }
}
