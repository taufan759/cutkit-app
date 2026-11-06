//! Penyusun perintah ffmpeg untuk ekspor. Fungsi di sini murni (tidak menjalankan
//! apa pun) supaya bisa diuji; `ffmpeg::run_export` yang mengeksekusinya.
//!
//! Strategi: satu filter graph. Latar `color` sebesar kanvas, lalu tiap klip
//! visual (urut dari track bawah ke atas) di-overlay pada rentang waktunya;
//! caption dan teks dirender terakhir lewat libass; audio tiap klip ditunda
//! (`adelay`) lalu dicampur `amix`.

use crate::ass;
use crate::layout::{place, Placement};
use crate::model::{AssetKind, ClipKind, Project, TrackKind};
use serde::{Deserialize, Serialize};

pub const FILTER_FILE: &str = "cutkit-filter.txt";
pub const ASS_FILE: &str = "cutkit-captions.ass";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Encoder {
    #[default]
    X264,
    Qsv,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Fast,
    #[default]
    Balanced,
    Best,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ExportOptions {
    pub output: String,
    pub encoder: Encoder,
    pub quality: Quality,
}

#[derive(Debug, Clone)]
pub struct ExportPlan {
    /// Argumen ffmpeg (tanpa nama program). Dijalankan dengan direktori kerja
    /// berisi `FILTER_FILE` dan `ASS_FILE`.
    pub args: Vec<String>,
    pub filter_script: String,
    pub ass: Option<String>,
    pub duration: f64,
}

struct Visual {
    input: usize,
    speed: f64,
    start: f64,
    end: f64,
    place: Placement,
    opacity: f64,
}

struct AudioSrc {
    input: usize,
    speed: f64,
    duration: f64,
    start: f64,
    volume: f64,
    fade_in: f64,
    fade_out: f64,
}

/// atempo menerima 0.5..100; kecepatan di bawah 0.5 dirantai (0.25 = 0.5 x 0.5).
fn atempo_chain(speed: f64) -> String {
    let mut parts = vec![];
    let mut s = speed;
    while s < 0.5 - 1e-9 {
        parts.push("atempo=0.5".to_string());
        s /= 0.5;
    }
    while s > 100.0 + 1e-9 {
        parts.push("atempo=100".to_string());
        s /= 100.0;
    }
    if (s - 1.0).abs() > 1e-6 {
        parts.push(format!("atempo={}", f(s)));
    }
    parts.join(",")
}

fn f(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s
    }
}

pub fn build_plan(project: &Project, opts: &ExportOptions) -> Result<ExportPlan, String> {
    if opts.output.trim().is_empty() {
        return Err("Tentukan file hasil ekspor.".into());
    }
    let duration = project.duration();
    if duration <= 0.0 {
        return Err("Timeline masih kosong, tidak ada yang bisa diekspor.".into());
    }
    let (w, h, fps) = (project.canvas.width, project.canvas.height, project.canvas.fps);
    let bg = project.canvas.background.trim_start_matches('#');
    let bg = if bg.len() == 6 { format!("0x{bg}") } else { "black".to_string() };

    let mut args: Vec<String> = vec![
        "-hide_banner", "-y", "-nostats", "-loglevel", "error", "-progress", "pipe:1", "-stats_period", "0.25",
        "-f", "lavfi", "-i", &format!("color=c={bg}:s={w}x{h}:r={}:d={}", f(fps), f(duration)),
        "-f", "lavfi", "-t", &f(duration), "-i", "anullsrc=r=48000:cl=stereo",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    let mut visuals: Vec<Visual> = vec![];
    let mut audios: Vec<AudioSrc> = vec![];
    let mut next_input = 2usize;

    for track in &project.tracks {
        let mut clips: Vec<_> = track.clips.iter().filter(|c| c.duration > 0.0).collect();
        clips.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        for clip in clips {
            if clip.kind == ClipKind::Text {
                continue;
            }
            let Some(asset) = clip.asset_id.as_deref().and_then(|id| project.asset(id)) else {
                return Err(format!("Klip {} merujuk ke media yang tidak ada di proyek.", clip.id));
            };
            let want_video = track.kind == TrackKind::Video && !track.hidden && asset.kind != AssetKind::Audio && clip.kind != ClipKind::Audio;
            let want_audio = !track.muted && !clip.muted && clip.volume > 0.0 && asset.has_audio && clip.kind != ClipKind::Image;
            if !want_video && !want_audio {
                continue;
            }
            let input = next_input;
            next_input += 1;
            if asset.kind == AssetKind::Image && asset.path.to_ascii_lowercase().ends_with(".gif") {
                // GIF diam tidak bisa dibaca image2; ulangi frame-nya dengan stream_loop.
                args.extend(["-stream_loop", "-1", "-t", &f(clip.duration), "-i", &asset.path].map(String::from));
            } else if asset.kind == AssetKind::Image {
                // image2 + pattern_type none: nama file dengan '%' (umum di nama meme) tidak dianggap pola urutan.
                args.extend(["-f", "image2", "-pattern_type", "none", "-loop", "1", "-framerate", &f(fps), "-t", &f(clip.duration), "-i", &asset.path].map(String::from));
            } else {
                if clip.source_in > 0.0 {
                    args.extend(["-ss", &f(clip.source_in)].map(String::from));
                }
                args.extend(["-t", &f(clip.source_duration())].map(String::from));
                if !want_video {
                    args.push("-vn".into());
                }
                if !want_audio {
                    args.push("-an".into());
                }
                args.extend(["-i", &asset.path].map(String::from));
            }
            if want_video {
                let pl = place(w, h, asset.width, asset.height, clip.fit, &clip.transform);
                visuals.push(Visual { input, speed: clip.effective_speed(), start: clip.start, end: clip.end(), place: pl, opacity: clip.transform.opacity.clamp(0.0, 1.0) });
            }
            if want_audio {
                audios.push(AudioSrc {
                    input,
                    speed: clip.effective_speed(),
                    duration: clip.duration,
                    start: clip.start,
                    volume: clip.volume,
                    fade_in: clip.fade_in.min(clip.duration),
                    fade_out: clip.fade_out.min(clip.duration),
                });
            }
        }
    }

    let mut g: Vec<String> = vec![];
    g.push("[0:v]format=rgba[b0]".into());
    for (i, v) in visuals.iter().enumerate() {
        let p = &v.place;
        let mut chain = format!("[{}:v]format=rgba,scale={}:{}", v.input, p.w, p.h);
        if p.radians.abs() > 1e-9 {
            chain.push_str(&format!(",rotate={}:ow={}:oh={}:c=0x00000000", f(p.radians), p.ow, p.oh));
        }
        if v.opacity < 0.999 {
            chain.push_str(&format!(",colorchannelmixer=aa={}", f(v.opacity)));
        }
        if (v.speed - 1.0).abs() > 1e-6 {
            chain.push_str(&format!(",setpts=(PTS-STARTPTS)/{}+{}/TB[v{i}]", f(v.speed), f(v.start)));
        } else {
            chain.push_str(&format!(",setpts=PTS-STARTPTS+{}/TB[v{i}]", f(v.start)));
        }
        g.push(chain);
        g.push(format!(
            "[b{i}][v{i}]overlay=x={}:y={}:format=rgb:eof_action=pass:enable='between(t,{},{})'[b{}]",
            p.x,
            p.y,
            f(v.start),
            f(v.end),
            i + 1
        ));
    }
    let last = format!("[b{}]", visuals.len());
    let ass_text = ass::build_ass(project);
    let final_fmt = if opts.encoder == Encoder::Qsv { "nv12" } else { "yuv420p" };
    match &ass_text {
        Some(_) => g.push(format!("{last}ass={ASS_FILE},format={final_fmt}[vout]")),
        None => g.push(format!("{last}format={final_fmt}[vout]")),
    }

    if audios.is_empty() {
        g.push("[1:a]anull[aout]".into());
    } else {
        let mut labels = String::from("[1:a]");
        for (i, a) in audios.iter().enumerate() {
            let mut chain = format!(
                "[{}:a]atrim=0:{},asetpts=PTS-STARTPTS,aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo",
                a.input,
                f(a.duration * a.speed)
            );
            let tempo = atempo_chain(a.speed);
            if !tempo.is_empty() {
                chain.push(',');
                chain.push_str(&tempo);
            }
            if (a.volume - 1.0).abs() > 1e-6 {
                chain.push_str(&format!(",volume={}", f(a.volume)));
            }
            if a.fade_in > 0.0 {
                chain.push_str(&format!(",afade=t=in:st=0:d={}", f(a.fade_in)));
            }
            if a.fade_out > 0.0 {
                chain.push_str(&format!(",afade=t=out:st={}:d={}", f(a.duration - a.fade_out), f(a.fade_out)));
            }
            let ms = (a.start * 1000.0).round() as i64;
            chain.push_str(&format!(",adelay={ms}:all=1[a{i}]"));
            g.push(chain);
            labels.push_str(&format!("[a{i}]"));
        }
        g.push(format!("{labels}amix=inputs={}:normalize=0:duration=first:dropout_transition=0[aout]", audios.len() + 1));
    }
    let filter_script = g.join(";\n") + "\n";

    args.extend(["-/filter_complex", FILTER_FILE, "-map", "[vout]", "-map", "[aout]"].map(String::from));
    match opts.encoder {
        Encoder::X264 => {
            let (preset, crf) = match opts.quality {
                Quality::Fast => ("veryfast", "23"),
                Quality::Balanced => ("medium", "20"),
                Quality::Best => ("slow", "18"),
            };
            args.extend(["-c:v", "libx264", "-preset", preset, "-crf", crf, "-profile:v", "high"].map(String::from));
        }
        Encoder::Qsv => {
            let (preset, q) = match opts.quality {
                Quality::Fast => ("veryfast", "26"),
                Quality::Balanced => ("medium", "23"),
                Quality::Best => ("slow", "20"),
            };
            args.extend(["-c:v", "h264_qsv", "-preset", preset, "-global_quality", q].map(String::from));
        }
    }
    let gop = (fps * 2.0).round().max(1.0) as i64;
    args.extend(["-g", &gop.to_string(), "-r", &f(fps), "-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-movflags", "+faststart", "-t", &f(duration), &opts.output].map(String::from));

    Ok(ExportPlan { args, filter_script, ass: ass_text, duration })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn project() -> Project {
        let mut p = Project::new("t");
        p.assets.push(Asset { id: "vid".into(), path: "C:/media/a.mp4".into(), kind: AssetKind::Video, duration: 10.0, width: 1920, height: 1080, has_audio: true, has_video: true, ..Default::default() });
        p.assets.push(Asset { id: "img".into(), path: "C:/meme/x.png".into(), kind: AssetKind::Image, width: 500, height: 400, has_video: true, ..Default::default() });
        p.assets.push(Asset { id: "sfx".into(), path: "C:/sfx/boom.mp3".into(), kind: AssetKind::Audio, duration: 2.0, has_audio: true, ..Default::default() });
        p.tracks[0].clips.push(Clip { id: "c1".into(), asset_id: Some("vid".into()), start: 0.0, duration: 4.0, source_in: 2.5, fit: Fit::Cover, volume: 0.5, fade_out: 1.0, ..Default::default() });
        p.tracks[1].clips.push(Clip {
            id: "c2".into(),
            kind: ClipKind::Image,
            asset_id: Some("img".into()),
            start: 1.0,
            duration: 2.0,
            transform: Transform { x: 0.0, y: -400.0, scale: 0.4, rotation: 15.0, opacity: 0.8 },
            ..Default::default()
        });
        p.tracks[2].clips.push(Clip { id: "c3".into(), kind: ClipKind::Audio, asset_id: Some("sfx".into()), start: 3.0, duration: 1.5, fade_in: 0.2, ..Default::default() });
        p
    }

    fn opts() -> ExportOptions {
        ExportOptions { output: "D:/out/hasil.mp4".into(), ..Default::default() }
    }

    #[test]
    fn empty_timeline_is_an_error() {
        assert!(build_plan(&Project::new("x"), &opts()).is_err());
    }

    #[test]
    fn inputs_are_trimmed_and_mapped() {
        let plan = build_plan(&project(), &opts()).unwrap();
        let a = plan.args.join(" ");
        assert_eq!(plan.duration, 4.5);
        assert!(a.contains("-f lavfi -i color=c=0x000000:s=1080x1920:r=30:d=4.5"));
        assert!(a.contains("-f lavfi -t 4.5 -i anullsrc=r=48000:cl=stereo"));
        assert!(a.contains("-ss 2.5 -t 4 -i C:/media/a.mp4"), "video dipangkas di sisi input");
        assert!(a.contains("-f image2 -pattern_type none -loop 1 -framerate 30 -t 2 -i C:/meme/x.png"));
        assert!(a.contains("-t 1.5 -vn -i C:/sfx/boom.mp3"));
        assert!(a.ends_with("-c:v libx264 -preset medium -crf 20 -profile:v high -g 60 -r 30 -c:a aac -b:a 192k -ar 48000 -movflags +faststart -t 4.5 D:/out/hasil.mp4"));
        assert!(a.contains("-/filter_complex cutkit-filter.txt -map [vout] -map [aout]"));
        assert!(plan.ass.is_none());
    }

    #[test]
    fn filter_graph_overlays_in_track_order() {
        let plan = build_plan(&project(), &opts()).unwrap();
        let g = plan.filter_script;
        let lines: Vec<&str> = g.lines().collect();
        assert_eq!(lines[0], "[0:v]format=rgba[b0];");
        // Video utama: cover 1920x1080 -> 3413x1920, dipusatkan -> x = -1166.5, dibulatkan menjauhi nol
        assert_eq!(lines[1], "[2:v]format=rgba,scale=3413:1920,setpts=PTS-STARTPTS+0/TB[v0];");
        assert_eq!(lines[2], "[b0][v0]overlay=x=-1167:y=0:format=rgb:eof_action=pass:enable='between(t,0,4)'[b1];");
        assert!(lines[3].starts_with("[3:v]format=rgba,scale=432:346,rotate=0.2618:ow=507:oh=447:c=0x00000000,colorchannelmixer=aa=0.8,setpts=PTS-STARTPTS+1/TB[v1];"));
        assert_eq!(lines[4], "[b1][v1]overlay=x=287:y=337:format=rgb:eof_action=pass:enable='between(t,1,3)'[b2];");
        assert_eq!(lines[5], "[b2]format=yuv420p[vout];");
        assert_eq!(lines[6], "[2:a]atrim=0:4,asetpts=PTS-STARTPTS,aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,volume=0.5,afade=t=out:st=3:d=1,adelay=0:all=1[a0];");
        assert_eq!(lines[7], "[4:a]atrim=0:1.5,asetpts=PTS-STARTPTS,aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,afade=t=in:st=0:d=0.2,adelay=3000:all=1[a1];");
        assert_eq!(lines[8], "[1:a][a0][a1]amix=inputs=3:normalize=0:duration=first:dropout_transition=0[aout]");
    }

    #[test]
    fn muted_hidden_and_text_clips() {
        let mut p = project();
        p.tracks[0].muted = true;
        p.tracks[1].hidden = true;
        p.tracks[2].clips.clear();
        p.tracks[1].clips.push(Clip { id: "t".into(), kind: ClipKind::Text, start: 0.0, duration: 1.0, text: Some(TextClip { text: "Hai".into(), ..Default::default() }), ..Default::default() });
        p.captions.enabled = true;
        p.captions.words = vec![Word { text: "yo".into(), start: 0.0, end: 0.5 }];
        let plan = build_plan(&p, &ExportOptions { encoder: Encoder::Qsv, quality: Quality::Fast, ..opts() }).unwrap();
        let a = plan.args.join(" ");
        assert!(a.contains("-ss 2.5 -t 4 -an -i C:/media/a.mp4"), "track dibisukan: audio tidak dibaca");
        assert!(!a.contains("x.png"), "track tersembunyi dilewati");
        assert!(a.contains("-c:v h264_qsv -preset veryfast -global_quality 26"));
        let g = plan.filter_script;
        assert!(g.contains("[b1]ass=cutkit-captions.ass,format=nv12[vout]"));
        assert!(g.contains("[1:a]anull[aout]"));
        let ass = plan.ass.unwrap();
        assert!(!ass.contains("Hai"), "teks di track tersembunyi tidak dirender");
        assert!(ass.contains("YO"));
    }

    #[test]
    fn speed_changes_source_length_video_pts_and_audio_tempo() {
        let mut p = project();
        p.tracks[0].clips[0].speed = 2.0; // 4 detik di timeline = 8 detik sumber
        p.tracks[2].clips[0].speed = 0.25;
        let plan = build_plan(&p, &opts()).unwrap();
        let a = plan.args.join(" ");
        assert!(a.contains("-ss 2.5 -t 8 -i C:/media/a.mp4"));
        assert!(a.contains("-t 0.375 -vn -i C:/sfx/boom.mp3"));
        let g = plan.filter_script;
        assert!(g.contains(",setpts=(PTS-STARTPTS)/2+0/TB[v0]"));
        assert!(g.contains("[2:a]atrim=0:8,asetpts=PTS-STARTPTS,aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,atempo=2,volume=0.5"));
        assert!(g.contains("atrim=0:0.375,asetpts=PTS-STARTPTS,aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,atempo=0.5,atempo=0.5,afade"));
        // gambar tidak terpengaruh speed
        p.tracks[1].clips[0].speed = 3.0;
        assert!(build_plan(&p, &opts()).unwrap().args.join(" ").contains("-loop 1 -framerate 30 -t 2 -i C:/meme/x.png"));
    }

    #[test]
    fn atempo_chain_ranges() {
        assert_eq!(atempo_chain(1.0), "");
        assert_eq!(atempo_chain(1.5), "atempo=1.5");
        assert_eq!(atempo_chain(0.3), "atempo=0.5,atempo=0.6");
        assert_eq!(atempo_chain(0.1), "atempo=0.5,atempo=0.5,atempo=0.5,atempo=0.8");
    }

    #[test]
    fn number_formatting() {
        assert_eq!(f(0.0), "0");
        assert_eq!(f(1.5), "1.5");
        assert_eq!(f(29.97), "29.97");
        assert_eq!(f(0.12345), "0.1235");
        assert_eq!(f(-0.00001), "0");
    }
}
