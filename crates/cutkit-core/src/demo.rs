//! Media contoh untuk proyek latihan: narasi TTS (flite bawaan ffmpeg; nada berirama bila ffmpeg
//! tanpa flite, mis. build statis Linux dan Homebrew), latar
//! gradasi, klip fraktal, dan musik sederhana. Semua dibuat lokal oleh ffmpeg.

use serde::Serialize;
use std::path::Path;

pub const DEMO_SCRIPT: &str = "Halo semua! Ini contoh proyek CutKit. Coba potong klip, tambah meme dan caption, lalu ekspor video kamu.";

#[derive(Serialize, Clone, Debug)]
pub struct DemoMedia {
    pub voice: String,
    pub music: String,
    pub visuals: Vec<String>,
    pub script: String,
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Daftar (path tujuan, argumen ffmpeg) untuk semua media contoh di folder `dir`.
pub fn demo_jobs(dir: &Path, flite: bool) -> (DemoMedia, Vec<(String, Vec<String>)>) {
    let p = |name: &str| dir.join(name).to_string_lossy().to_string();
    let base = ["-hide_banner", "-y", "-nostats", "-loglevel", "error"];
    let mut jobs = vec![];
    // flite membaca teks dari argumen filter; tanda kutip tunggal dan titik dua harus dihindari.
    let text = DEMO_SCRIPT.replace(['\'', ':'], "");
    let voice = p("01-narasi.wav");
    let src = if flite {
        format!("flite=text='{text}':voice=slt")
    } else {
        // Pengganti narasi: nada berirama sepanjang kira-kira waktu baca naskah.
        "aevalsrc='0.25*sin(2*PI*180*t)*gt(sin(2*PI*2.6*t),0)*(0.6+0.4*sin(2*PI*0.7*t))':s=22050:d=6.5".to_string()
    };
    jobs.push((voice.clone(), [s(&base), s(&["-f", "lavfi", "-i", &src, "-af", "apad=pad_dur=0.6", "-ar", "48000", &voice])].concat()));
    let colors = [("0x1f3b73", "0xff6b3d"), ("0x0b6e4f", "0xf5d547"), ("0x5b2a86", "0x35c9ff")];
    let mut visuals = vec![];
    for (i, (c0, c1)) in colors.iter().enumerate() {
        let out = p(&format!("{:02}-latar.png", i * 2 + 1));
        let src = format!("gradients=s=1080x1920:c0={c0}:c1={c1}:x0=0:y0=0:x1=1080:y1=1920:nb_colors=2:seed={i}:d=1");
        jobs.push((out.clone(), [s(&base), s(&["-f", "lavfi", "-i", &src, "-frames:v", "1", "-update", "1", &out])].concat()));
        visuals.push(out);
        if i < 2 {
            let clip = p(&format!("{:02}-klip.mp4", i * 2 + 2));
            let src = if i == 0 { "mandelbrot=s=720x1280:r=30" } else { "life=s=360x640:mold=10:r=30:ratio=0.1:death_color=#0b1a2a:life_color=#35c9ff,scale=720:1280:flags=neighbor" };
            jobs.push((clip.clone(), [s(&base), s(&["-f", "lavfi", "-i", src, "-t", "4", "-c:v", "libx264", "-preset", "veryfast", "-pix_fmt", "yuv420p", &clip])].concat()));
            visuals.push(clip);
        }
    }
    let music = p("musik.wav");
    let melody = "aevalsrc='0.08*sin(2*PI*220*t)*(0.6+0.4*sin(2*PI*0.5*t))+0.06*sin(2*PI*277.18*t)+0.05*sin(2*PI*329.63*t)*(0.5+0.5*sin(2*PI*0.25*t)):s=48000:c=stereo'";
    jobs.push((music.clone(), [s(&base), s(&["-f", "lavfi", "-i", melody, "-t", "20", &music])].concat()));
    (DemoMedia { voice, music, visuals, script: DEMO_SCRIPT.to_string() }, jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_cover_every_file() {
        let (media, jobs) = demo_jobs(Path::new("C:/demo"), true);
        assert_eq!(media.visuals.len(), 5);
        assert!(media.visuals[0].ends_with("01-latar.png") && media.visuals[1].ends_with("02-klip.mp4"));
        assert_eq!(jobs.len(), 7);
        for (out, args) in &jobs {
            assert_eq!(args.last().unwrap(), out, "file tujuan di akhir argumen");
        }
        let voice = jobs[0].1.join(" ");
        assert!(voice.contains("flite=text='Halo semua! Ini contoh proyek CutKit."));
        assert_eq!(jobs[0].1[8].matches(':').count(), 1, "titik dua hanya pemisah opsi voice");
        let (_, without) = demo_jobs(Path::new("C:/demo"), false);
        assert!(without[0].1.join(" ").contains("aevalsrc="), "tanpa flite: nada pengganti");
    }
}
