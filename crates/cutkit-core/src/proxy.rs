//! Argumen ffmpeg untuk proxy preview dan gambar bentuk gelombang.

use crate::model::AssetKind;

/// Tinggi/lebar maksimum proxy; cukup untuk preview dan ringan untuk WebView2.
pub const PROXY_SIZE: u32 = 720;

/// Ekstensi file proxy untuk jenis media tertentu.
pub fn proxy_extension(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Video => "mp4",
        AssetKind::Audio => "m4a",
        AssetKind::Image => "png",
    }
}

pub fn proxy_args(src: &str, dst: &str, kind: AssetKind) -> Vec<String> {
    let mut a: Vec<String> = vec!["-hide_banner", "-y", "-nostats", "-loglevel", "error"].into_iter().map(String::from).collect();
    match kind {
        AssetKind::Video => {
            let scale = format!("scale=w='if(gt(a,1),-2,min({PROXY_SIZE},iw))':h='if(gt(a,1),min({PROXY_SIZE},ih),-2)':flags=bicubic,format=yuv420p");
            a.extend(["-i", src, "-vf", &scale, "-c:v", "libx264", "-preset", "veryfast", "-crf", "26", "-g", "30", "-c:a", "aac", "-b:a", "128k", "-ac", "2", "-movflags", "+faststart", "-sn", "-dn", "-map_metadata", "-1", dst].map(String::from));
        }
        AssetKind::Audio => {
            a.extend(["-i", src, "-vn", "-c:a", "aac", "-b:a", "160k", "-ac", "2", "-map_metadata", "-1", dst].map(String::from));
        }
        AssetKind::Image => {
            a.extend(["-i", src, "-frames:v", "1", "-update", "1", dst].map(String::from));
        }
    }
    a
}

/// Gambar gelombang suara (PNG transparan, warna tunggal) untuk ditampilkan di klip.
pub fn waveform_args(src: &str, dst: &str, width: u32, height: u32) -> Vec<String> {
    let filter = format!("aformat=channel_layouts=mono,showwavespic=s={width}x{height}:colors=#7cc4ff:scale=sqrt:draw=full");
    ["-hide_banner", "-y", "-nostats", "-loglevel", "error", "-i", src, "-filter_complex", &filter, "-frames:v", "1", "-update", "1", dst]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Jumlah gambar mini dalam satu filmstrip untuk durasi tertentu.
pub fn filmstrip_count(duration: f64) -> u32 {
    ((duration / 2.0).ceil() as u32).clamp(4, 60)
}

/// Strip gambar mini (satu JPEG berisi `count` frame berjajar) yang mewakili seluruh durasi video.
pub fn filmstrip_args(src: &str, dst: &str, duration: f64, height: u32) -> Vec<String> {
    let count = filmstrip_count(duration);
    let rate = count as f64 / duration.max(0.1);
    let filter = format!("fps={rate:.5},scale=-2:{height}:flags=fast_bilinear,tile={count}x1,format=yuvj420p");
    ["-hide_banner", "-y", "-nostats", "-loglevel", "error", "-i", src, "-an", "-vf", &filter, "-frames:v", "1", "-update", "1", dst]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_proxy_scales_down_only() {
        let a = proxy_args("C:/in/a.mov", "C:/cache/a.mp4", AssetKind::Video).join(" ");
        assert!(a.contains("-vf scale=w='if(gt(a,1),-2,min(720,iw))':h='if(gt(a,1),min(720,ih),-2)':flags=bicubic,format=yuv420p -c:v libx264 -preset veryfast"));
        assert!(a.ends_with("C:/cache/a.mp4"));
    }

    #[test]
    fn audio_and_image_proxy() {
        assert!(proxy_args("a.wma", "a.m4a", AssetKind::Audio).join(" ").contains("-vn -c:a aac"));
        assert!(proxy_args("a.tif", "a.png", AssetKind::Image).join(" ").ends_with("-frames:v 1 -update 1 a.png"));
        assert_eq!(proxy_extension(AssetKind::Audio), "m4a");
    }

    #[test]
    fn filmstrip() {
        assert_eq!(filmstrip_count(1.0), 4);
        assert_eq!(filmstrip_count(30.0), 15);
        assert_eq!(filmstrip_count(3600.0), 60);
        let a = filmstrip_args("a.mp4", "a.png", 30.0, 48).join(" ");
        assert!(a.contains("-i a.mp4 -an -vf fps=0.50000,scale=-2:48:flags=fast_bilinear,tile=15x1,format=yuvj420p -frames:v 1"));
    }

    #[test]
    fn waveform() {
        let w = waveform_args("a.mp3", "a.png", 2000, 64).join(" ");
        assert!(w.contains("showwavespic=s=2000x64"));
    }
}
