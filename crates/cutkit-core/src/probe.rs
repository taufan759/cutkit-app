//! Membaca info media lewat `ffprobe -print_format json`.

use crate::model::AssetKind;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MediaInfo {
    pub kind: AssetKind,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub has_video: bool,
    pub has_audio: bool,
    pub video_codec: String,
    pub audio_codec: String,
    pub fps: f64,
    pub container: String,
    /// Apakah WebView2 bisa memutar file ini langsung; kalau tidak, dibuatkan proxy.
    pub playable: bool,
}

const IMAGE_CODECS: &[&str] = &["png", "mjpeg", "webp", "bmp", "tiff", "psd", "ppm", "pgm", "jpeg2000", "jpegxl", "heif", "hevc_still", "targa"];
const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "avif"];
const PLAYABLE_VIDEO_EXT: &[&str] = &["mp4", "m4v", "webm"];
const PLAYABLE_AUDIO_EXT: &[&str] = &["mp3", "wav", "m4a", "aac", "ogg", "oga", "opus", "flac", "weba"];
const PLAYABLE_VIDEO_CODECS: &[&str] = &["h264", "vp8", "vp9", "av1"];
const PLAYABLE_AUDIO_CODECS: &[&str] = &["aac", "mp3", "opus", "vorbis", "flac", "pcm_s16le", "pcm_s24le", "pcm_f32le", "pcm_u8"];

pub fn extension(path: &str) -> String {
    Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Mengurai keluaran JSON ffprobe menjadi `MediaInfo`. `path` hanya dipakai untuk ekstensi.
pub fn parse_ffprobe(path: &str, json: &str) -> Result<MediaInfo, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Keluaran ffprobe tidak valid: {e}"))?;
    let streams = v.get("streams").and_then(|s| s.as_array()).cloned().unwrap_or_default();
    if streams.is_empty() {
        return Err("File tidak dikenali sebagai media.".into());
    }
    let ext = extension(path);
    let format = v.get("format").cloned().unwrap_or_default();
    let mut info = MediaInfo { container: format.get("format_name").and_then(|s| s.as_str()).unwrap_or("").to_string(), ..Default::default() };
    let num = |val: Option<&serde_json::Value>| -> f64 {
        match val {
            Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0.0),
            Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
            _ => 0.0,
        }
    };
    info.duration = num(format.get("duration"));

    for s in &streams {
        let codec_type = s.get("codec_type").and_then(|c| c.as_str()).unwrap_or("");
        let codec = s.get("codec_name").and_then(|c| c.as_str()).unwrap_or("").to_string();
        let disposition_cover = s.get("disposition").and_then(|d| d.get("attached_pic")).and_then(|a| a.as_i64()).unwrap_or(0) == 1;
        match codec_type {
            "video" if !info.has_video && !disposition_cover => {
                info.has_video = true;
                info.video_codec = codec;
                let mut w = num(s.get("width")) as u32;
                let mut h = num(s.get("height")) as u32;
                let rotation = stream_rotation(s);
                if rotation % 180 != 0 {
                    std::mem::swap(&mut w, &mut h);
                }
                info.width = w;
                info.height = h;
                info.fps = parse_rate(s.get("avg_frame_rate")).or_else(|| parse_rate(s.get("r_frame_rate"))).unwrap_or(0.0);
                if info.duration <= 0.0 {
                    info.duration = num(s.get("duration"));
                }
            }
            "audio" if !info.has_audio => {
                info.has_audio = true;
                info.audio_codec = codec;
                if info.duration <= 0.0 {
                    info.duration = num(s.get("duration"));
                }
            }
            _ => {}
        }
    }

    let is_gif = info.video_codec == "gif" || ext == "gif";
    // GIF satu frame (meme diam) diperlakukan sebagai gambar; GIF animasi sebagai video.
    let still_gif = is_gif && info.duration < 0.15;
    let is_image = info.has_video
        && (still_gif
            || (!is_gif && (IMAGE_CODECS.contains(&info.video_codec.as_str()) || info.container.contains("image2") || info.container.ends_with("_pipe"))));
    info.kind = if is_image {
        AssetKind::Image
    } else if info.has_video {
        AssetKind::Video
    } else if info.has_audio {
        AssetKind::Audio
    } else {
        return Err("File tidak berisi video maupun audio.".into());
    };
    if info.kind == AssetKind::Image {
        info.duration = 0.0;
        info.has_audio = false;
    }
    info.playable = match info.kind {
        AssetKind::Image => IMAGE_EXT.contains(&ext.as_str()),
        AssetKind::Video => {
            PLAYABLE_VIDEO_EXT.contains(&ext.as_str())
                && PLAYABLE_VIDEO_CODECS.contains(&info.video_codec.as_str())
                && (!info.has_audio || PLAYABLE_AUDIO_CODECS.contains(&info.audio_codec.as_str()))
                && info.width <= 1920
                && info.height <= 1920
        }
        AssetKind::Audio => PLAYABLE_AUDIO_EXT.contains(&ext.as_str()) && PLAYABLE_AUDIO_CODECS.contains(&info.audio_codec.as_str()),
    };
    Ok(info)
}

fn stream_rotation(s: &serde_json::Value) -> i64 {
    if let Some(list) = s.get("side_data_list").and_then(|l| l.as_array()) {
        for sd in list {
            if let Some(r) = sd.get("rotation") {
                let r = match r {
                    serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
                    serde_json::Value::String(st) => st.parse().unwrap_or(0.0),
                    _ => 0.0,
                };
                return (r.round() as i64).rem_euclid(360);
            }
        }
    }
    s.get("tags").and_then(|t| t.get("rotate")).and_then(|r| r.as_str()).and_then(|r| r.parse::<i64>().ok()).map(|r| r.rem_euclid(360)).unwrap_or(0)
}

fn parse_rate(v: Option<&serde_json::Value>) -> Option<f64> {
    let s = v?.as_str()?;
    let (n, d) = s.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    if d <= 0.0 || n <= 0.0 {
        return None;
    }
    Some(n / d)
}

/// Argumen ffprobe untuk satu file.
pub fn probe_args(path: &str) -> Vec<String> {
    ["-v", "error", "-print_format", "json", "-show_format", "-show_streams", path].iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rotated_phone_video() {
        let json = r#"{"streams":[
            {"codec_type":"video","codec_name":"hevc","width":1920,"height":1080,"avg_frame_rate":"30000/1001","side_data_list":[{"rotation":-90}]},
            {"codec_type":"audio","codec_name":"aac"}],
            "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"12.5"}}"#;
        let info = parse_ffprobe("C:/x/IMG_1.MOV", json).unwrap();
        assert_eq!(info.kind, AssetKind::Video);
        assert_eq!((info.width, info.height), (1080, 1920));
        assert!((info.fps - 29.97).abs() < 0.01);
        assert_eq!(info.duration, 12.5);
        assert!(!info.playable, "HEVC di MOV butuh proxy");
    }

    #[test]
    fn mp4_h264_is_playable_but_4k_is_not() {
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":1080,"height":1920,"r_frame_rate":"30/1"},{"codec_type":"audio","codec_name":"aac"}],"format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"3"}}"#;
        assert!(parse_ffprobe("a.mp4", json).unwrap().playable);
        let json4k = json.replace("\"width\":1080,\"height\":1920", "\"width\":2160,\"height\":3840");
        assert!(!parse_ffprobe("a.mp4", &json4k).unwrap().playable);
    }

    #[test]
    fn images_audio_and_gif() {
        let png = r#"{"streams":[{"codec_type":"video","codec_name":"png","width":500,"height":400}],"format":{"format_name":"png_pipe"}}"#;
        let i = parse_ffprobe("meme.png", png).unwrap();
        assert_eq!(i.kind, AssetKind::Image);
        assert!(i.playable);
        assert_eq!(i.duration, 0.0);

        let gif = r#"{"streams":[{"codec_type":"video","codec_name":"gif","width":200,"height":200,"avg_frame_rate":"10/1"}],"format":{"format_name":"gif","duration":"2.4"}}"#;
        let g = parse_ffprobe("x.gif", gif).unwrap();
        assert_eq!(g.kind, AssetKind::Video, "GIF diperlakukan sebagai video (lewat proxy)");
        assert!(!g.playable);
        let still = gif.replace("\"duration\":\"2.4\"", "\"duration\":\"0.1\"");
        assert_eq!(parse_ffprobe("x.gif", &still).unwrap().kind, AssetKind::Image, "GIF satu frame = gambar");

        let mp3 = r#"{"streams":[{"codec_type":"video","codec_name":"mjpeg","disposition":{"attached_pic":1}},{"codec_type":"audio","codec_name":"mp3","duration":"1.2"}],"format":{"format_name":"mp3"}}"#;
        let a = parse_ffprobe("boom.mp3", mp3).unwrap();
        assert_eq!(a.kind, AssetKind::Audio, "sampul album diabaikan");
        assert!(a.playable);
        assert_eq!(a.duration, 1.2);

        assert!(parse_ffprobe("x.txt", r#"{"streams":[]}"#).is_err());
    }
}
