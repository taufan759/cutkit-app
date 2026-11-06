//! Penyusun perintah ffmpeg untuk ekspor. Fungsi di sini murni (tidak menjalankan
//! apa pun) supaya bisa diuji; `ffmpeg::run_job` yang mengeksekusinya.
//!
//! Strategi satu pass: latar `color` sebesar kanvas, lalu tiap klip visual (urut dari
//! track bawah ke atas) di-overlay pada rentang waktunya; caption dan teks dirender
//! terakhir lewat libass; audio tiap klip ditunda (`adelay`) lalu dicampur `amix`.
//!
//! Video panjang (banyak klip atau durasi lama) diekspor bertahap supaya RAM tetap
//! kecil: satu pass audio untuk seluruh video, video per bagian (tiap bagian hanya
//! membuka klip yang tampil di dalamnya), lalu bagian-bagian digabung tanpa encode ulang.

use crate::ass;
use crate::layout::{place, Placement};
use crate::model::{keyframe_expr, AssetKind, ClipKind, Keyframe, Motion, Project, TrackKind, Transition};
use serde::{Deserialize, Serialize};

pub const FILTER_FILE: &str = "cutkit-filter.txt";
pub const ASS_FILE: &str = "cutkit-captions.ass";

/// Batas klip visual per pass dan panjang tiap bagian pada ekspor bertahap.
pub const MAX_VISUAL_INPUTS: usize = 16;
pub const MAX_CHUNK_SECONDS: f64 = 120.0;
/// Proyek sampai sepanjang ini dengan klip sedikit diekspor sekali jalan.
pub const MAX_SINGLE_PASS_SECONDS: f64 = 300.0;

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
    /// Ratakan kerasnya suara ke -14 LUFS (patokan YouTube) dengan loudnorm.
    pub normalize_loudness: bool,
}

/// Target loudness YouTube: -14 LUFS, puncak -1.5 dBTP.
pub const LOUDNORM: &str = "loudnorm=I=-14:TP=-1.5:LRA=11,aresample=48000";

#[derive(Debug, Clone)]
pub struct ExportPlan {
    /// Argumen ffmpeg (tanpa nama program). Dijalankan dengan direktori kerja
    /// berisi `FILTER_FILE` dan `ASS_FILE`.
    pub args: Vec<String>,
    pub filter_script: String,
    pub ass: Option<String>,
    pub duration: f64,
    /// File yang disalin ke folder kerja: (nama di folder kerja, path sumber), mis. LUT .cube.
    pub copies: Vec<(String, String)>,
}

/// Satu proses ffmpeg dalam ekspor.
#[derive(Debug, Clone)]
pub struct ExportStep {
    pub label: String,
    pub args: Vec<String>,
    /// File (nama, isi) yang ditulis ke folder kerja sebelum langkah dijalankan.
    pub files: Vec<(String, String)>,
    /// File (nama di folder kerja, path sumber) yang disalin sebelum langkah dijalankan.
    pub copies: Vec<(String, String)>,
    /// Panjang keluaran langkah ini (detik), untuk menghitung progres.
    pub duration: f64,
    /// Porsi progres total; jumlah semua langkah = 1.
    pub weight: f64,
}

#[derive(Debug, Clone)]
pub struct ExportJob {
    pub steps: Vec<ExportStep>,
    pub duration: f64,
    pub output: String,
    /// true bila ekspor dibagi beberapa bagian (video panjang).
    pub chunked: bool,
}

struct Visual {
    input: usize,
    blur_bg: bool,
    /// Argumen colorkey (warna:similarity:blend) bila chroma key aktif.
    colorkey: Option<String>,
    speed: f64,
    /// Gerak, jumlah frame seluruh klip, dan frame awal bila klip terpotong di batas bagian.
    motion: Option<(Motion, u64, u64)>,
    start: f64,
    end: f64,
    place: Placement,
    opacity: f64,
    /// Transisi masuk: (jenis, durasi, waktu timeline awal transisi di pass ini).
    t_in: Option<(String, f64, f64)>,
    /// Didorong keluar oleh klip berikutnya (push): (jenis, durasi, waktu mulai).
    t_out: Option<(String, f64, f64)>,
    /// Filter warna (diawali koma) dan LUT yang perlu disalin.
    color: String,
    /// Animasi keyframe (minimal dua keyframe).
    kf: Option<KfAnim>,
}

/// Animasi transform per frame dari keyframe.
struct KfAnim {
    /// Rantai sesudah setpts: scale per frame, pad ke ukuran tetap, rotate, opacity.
    chain: String,
    /// Ekspresi posisi overlay (kiri-atas) sebelum offset transisi.
    x: String,
    y: String,
}

/// Menyusun animasi keyframe: ukuran dasar (skala 1) `bw`x`bh`, kanvas `w`x`h`, `t0` = waktu klip lokal 0.
fn keyframe_anim(kfs: &[Keyframe], bw: u32, bh: u32, w: u32, h: u32, t0: f64, tag: usize, fps: f64) -> KfAnim {
    let smax = kfs.iter().map(|k| k.scale.clamp(0.01, 20.0)).fold(0.0, f64::max);
    let rotates = kfs.iter().any(|k| k.rotation.abs() > 1e-6);
    let (mw, mh) = if rotates {
        let d = ((bw as f64).hypot(bh as f64) * smax).ceil() as u32 + 2;
        (d + d % 2, d + d % 2)
    } else {
        let a = (bw as f64 * smax).ceil() as u32 + 2;
        let b = (bh as f64 * smax).ceil() as u32 + 2;
        (a + a % 2, b + b % 2)
    };
    let s = keyframe_expr(kfs, t0, |k| k.scale.clamp(0.01, 20.0));
    let mut chain = format!(
        ",scale=w='max(2,2*trunc({bw}*({s})/2))':h='max(2,2*trunc({bh}*({s})/2))':eval=frame,pad=w={mw}:h={mh}:x='(ow-iw)/2':y='(oh-ih)/2':color=0x00000000:eval=frame"
    );
    if rotates {
        let r = keyframe_expr(kfs, t0, |k| k.rotation.to_radians());
        chain.push_str(&format!(",rotate=a='{r}':ow={mw}:oh={mh}:c=0x00000000"));
    }
    let ops: Vec<f64> = kfs.iter().map(|k| k.opacity.clamp(0.0, 1.0)).collect();
    if ops.iter().any(|o| (o - ops[0]).abs() > 1e-6) {
        // Opacity tidak bisa berupa ekspresi di colorchannelmixer: perintah per frame lewat sendcmd.
        let name = format!("colorchannelmixer@kf{tag}");
        let (first, last) = (kfs[0].t, kfs[kfs.len() - 1].t);
        let step = 1.0 / fps.max(1.0);
        let mut cmds = vec![];
        let mut lt = first;
        let mut prev = f64::NAN;
        while lt <= last + step / 2.0 {
            let v = keyframe_value(kfs, lt, |k| k.opacity.clamp(0.0, 1.0));
            if (v - prev).abs() > 0.002 || prev.is_nan() {
                cmds.push(format!("{} {name} aa {}", fmt_t(t0 + lt), fmt_t(v)));
                prev = v;
            }
            lt += step;
        }
        chain.push_str(&format!(",sendcmd=c='{}',{name}=aa={}", cmds.join(";"), fmt_t(ops[0])));
    } else if ops[0] < 0.999 {
        chain.push_str(&format!(",colorchannelmixer=aa={}", fmt_t(ops[0])));
    }
    let x = format!("{}+({})", w as f64 / 2.0 - mw as f64 / 2.0, keyframe_expr(kfs, t0, |k| k.x));
    let y = format!("{}+({})", h as f64 / 2.0 - mh as f64 / 2.0, keyframe_expr(kfs, t0, |k| k.y));
    KfAnim { chain, x, y }
}

fn fmt_t(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s.is_empty() || s == "-0" { "0".into() } else { s }
}

/// Nilai keyframe pada waktu lokal `lt` (sama dengan transformAt di ui/core/timeline.js).
pub fn keyframe_value(kfs: &[Keyframe], lt: f64, get: impl Fn(&Keyframe) -> f64) -> f64 {
    if kfs.is_empty() {
        return 0.0;
    }
    if lt <= kfs[0].t {
        return get(&kfs[0]);
    }
    for w in kfs.windows(2) {
        if lt < w[1].t {
            let u = (lt - w[0].t) / (w[1].t - w[0].t).max(1e-9);
            return get(&w[0]) + (get(&w[1]) - get(&w[0])) * u;
        }
    }
    get(&kfs[kfs.len() - 1])
}

/// Ekspresi easing halus (smoothstep) dari kemajuan transisi di waktu `t`.
fn ease_expr(start: f64, dur: f64) -> String {
    format!("st(0,clip((t-{})/{},0,1));ld(0)*ld(0)*(3-2*ld(0))", f(start), f(dur.max(0.001)))
}

/// Ekspresi posisi overlay x dan y untuk transisi masuk/keluar (geser/dorong), dari posisi dasar.
fn overlay_xy(base: (String, String), w: u32, h: u32, t_in: &Option<(String, f64, f64)>, t_out: &Option<(String, f64, f64)>) -> (String, String) {
    let (mut x, mut y) = base;
    let p = Placement { x: 0, y: 0, w: 0, h: 0, ow: 0, oh: 0, radians: 0.0 };
    let _ = p;
    if let Some((kind, d, s)) = t_in {
        let e = ease_expr(*s, *d);
        match kind.as_str() {
            "slide-left" | "push-left" => x = format!("{x}+{w}*(1-({e}))"),
            "slide-right" | "push-right" => x = format!("{x}-{w}*(1-({e}))"),
            "slide-up" => y = format!("{y}+{h}*(1-({e}))"),
            "slide-down" => y = format!("{y}-{h}*(1-({e}))"),
            _ => {}
        }
    }
    if let Some((kind, d, s)) = t_out {
        let e = ease_expr(*s, *d);
        match kind.as_str() {
            "push-left" => x = format!("{x}-{w}*({e})"),
            "push-right" => x = format!("{x}+{w}*({e})"),
            _ => {}
        }
    }
    (x, y)
}

/// Efek yang disisipkan ke rantai komposisi pada urutan lapisannya.
struct FxStep {
    /// Indeks visual sesudah efek ini diterapkan (efek berlaku untuk visual 0..after).
    after: usize,
    effect: crate::model::Effect,
    start: f64,
    end: f64,
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

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Apa yang dihasilkan satu pass ffmpeg.
struct Pass {
    video: bool,
    audio: bool,
    loudnorm: bool,
    ass: Option<String>,
    filter_file: String,
    ass_file: String,
    /// Panjang keluaran (detik); bisa lebih panjang dari klip terakhir pada bagian video.
    duration: f64,
}

fn is_visual(track_kind: TrackKind, hidden: bool, clip_kind: ClipKind, asset_kind: AssetKind) -> bool {
    track_kind == TrackKind::Video && !hidden && asset_kind != AssetKind::Audio && clip_kind != ClipKind::Audio && clip_kind != ClipKind::Text
}

/// Menyusun input dan filter graph satu pass. Mengembalikan (argumen input + map, filter graph, file LUT yang disalin).
fn build_pass(project: &Project, pass: &Pass) -> Result<(Vec<String>, String, Vec<(String, String)>), String> {
    let (w, h, fps) = (project.canvas.width, project.canvas.height, project.canvas.fps);
    let duration = pass.duration;
    let bg = project.canvas.background.trim_start_matches('#');
    let bg = if bg.len() == 6 { format!("0x{bg}") } else { "black".to_string() };

    let mut args = strings(&[
        "-hide_banner", "-y", "-nostats", "-loglevel", "error", "-progress", "pipe:1", "-stats_period", "0.25",
        "-f", "lavfi", "-i", &format!("color=c={bg}:s={w}x{h}:r={}:d={}", f(fps), f(duration)),
        "-f", "lavfi", "-t", &f(duration), "-i", "anullsrc=r=48000:cl=stereo",
    ]);

    let mut visuals: Vec<Visual> = vec![];
    let mut audios: Vec<AudioSrc> = vec![];
    let mut next_input = 2usize;
    let mut copies: Vec<(String, String)> = vec![];
    let mut fx: Vec<FxStep> = vec![];

    for track in &project.tracks {
        let mut clips: Vec<_> = track.clips.iter().filter(|c| c.duration > 0.0).collect();
        clips.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        for (ci, clip) in clips.iter().enumerate() {
            let clip = *clip;
            if clip.kind == ClipKind::Text {
                continue;
            }
            if clip.kind == ClipKind::Effect {
                if pass.video && !track.hidden && track.kind == TrackKind::Video {
                    if let Some(e) = &clip.effect {
                        fx.push(FxStep { after: visuals.len(), effect: e.clone(), start: clip.start, end: clip.end() });
                    }
                }
                continue;
            }
            // Transisi masuk aktif bila klip ini menumpuk klip sebelumnya; klip berikutnya bisa mendorong klip ini keluar.
            let incoming: Option<&Transition> = Some(&clip.transition).filter(|t| t.is_active() && ci > 0);
            let next = clips.get(ci + 1).copied().filter(|n| n.transition.is_active() && n.start < clip.end() - 1e-6);
            let Some(asset) = clip.asset_id.as_deref().and_then(|id| project.asset(id)) else {
                return Err(format!("Klip {} merujuk ke media yang tidak ada di proyek.", clip.id));
            };
            let want_video = pass.video && is_visual(track.kind, track.hidden, clip.kind, asset.kind);
            let want_audio = pass.audio && !track.muted && !clip.muted && clip.volume > 0.0 && asset.has_audio && clip.kind != ClipKind::Image;
            if !want_video && !want_audio {
                continue;
            }
            let input = next_input;
            next_input += 1;
            if asset.kind == AssetKind::Image && asset.path.to_ascii_lowercase().ends_with(".gif") {
                // GIF diam tidak bisa dibaca image2; ulangi frame-nya dengan stream_loop.
                args.extend(strings(&["-stream_loop", "-1", "-t", &f(clip.duration), "-i", &asset.path]));
            } else if asset.kind == AssetKind::Image {
                // image2 + pattern_type none: nama file dengan '%' (umum di nama meme) tidak dianggap pola urutan.
                args.extend(strings(&["-f", "image2", "-pattern_type", "none", "-loop", "1", "-framerate", &f(fps), "-t", &f(clip.duration), "-i", &asset.path]));
            } else {
                if clip.source_in > 0.0 {
                    args.extend(strings(&["-ss", &f(clip.source_in)]));
                }
                args.extend(strings(&["-t", &f(clip.source_duration())]));
                // Decoder bawaan VP9 membuang kanal alpha; libvpx membacanya.
                if want_video && asset.alpha {
                    args.extend(strings(&["-c:v", "libvpx-vp9"]));
                }
                if !want_video {
                    args.push("-vn".into());
                }
                if !want_audio {
                    args.push("-an".into());
                }
                args.extend(strings(&["-i", &asset.path]));
            }
            if want_video {
                // Satu keyframe = transform tetap; dua atau lebih = animasi.
                let static_t = if clip.keyframes.len() == 1 { clip.keyframes[0].transform() } else { clip.transform };
                let animated = clip.keyframes.len() >= 2;
                let pl = if animated {
                    place(w, h, asset.width, asset.height, clip.fit, &crate::model::Transform { x: 0.0, y: 0.0, scale: 1.0, rotation: 0.0, opacity: 1.0 })
                } else {
                    place(w, h, asset.width, asset.height, clip.fit, &static_t)
                };
                let kf = animated.then(|| keyframe_anim(&clip.keyframes, pl.w, pl.h, w, h, clip.start - clip.offset, visuals.len(), fps));
                let speed = clip.effective_speed();
                let span = if clip.span > 0.0 { clip.span } else { clip.duration };
                let motion = clip.motion.is_active().then(|| {
                    let total = (span * speed * fps).round().max(1.0) as u64;
                    let first = (clip.offset * speed * fps).round().max(0.0) as u64;
                    (clip.motion.clone(), total, first)
                });
                let colorkey = clip.chroma.enabled.then(|| {
                    let (r, g, b, _) = crate::ass::parse_color(&clip.chroma.color);
                    format!("0x{r:02x}{g:02x}{b:02x}:{}:{}", f(clip.chroma.similarity.clamp(0.01, 1.0)), f(clip.chroma.blend.clamp(0.0, 1.0)))
                });
                // Awal transisi di waktu pass ini (bisa negatif bila klip terpotong di batas bagian).
                let t_in = incoming.map(|t| (t.kind.clone(), t.duration, clip.start - clip.offset));
                let t_out = next.filter(|n| n.transition.kind.starts_with("push")).map(|n| (n.transition.kind.clone(), n.transition.duration, n.start - n.offset));
                // LUT disalin ke folder kerja dengan nama sederhana agar tidak perlu escape path di filter.
                let lut_name = (!clip.color.lut.trim().is_empty()).then(|| {
                    if let Some((name, _)) = copies.iter().find(|(_, src)| src == &clip.color.lut) {
                        name.clone()
                    } else {
                        let name = format!("cutkit-lut-{}.cube", copies.len());
                        copies.push((name.clone(), clip.color.lut.clone()));
                        name
                    }
                });
                let color = clip.color.filters(lut_name.as_deref());
                let opacity = if animated { 1.0 } else { static_t.opacity.clamp(0.0, 1.0) };
                visuals.push(Visual { input, blur_bg: clip.background == "blur", colorkey, speed, motion, start: clip.start, end: clip.end(), place: pl, opacity, t_in, t_out, color, kf });
            }
            if want_audio {
                // Selama transisi suara ikut crossfade: klip baru masuk pelan, klip lama keluar pelan.
                let x_in = incoming.map(|t| t.duration).unwrap_or(0.0);
                let x_out = next.map(|n| clip.end() - n.start).unwrap_or(0.0);
                audios.push(AudioSrc {
                    input,
                    speed: clip.effective_speed(),
                    duration: clip.duration,
                    start: clip.start,
                    volume: clip.volume,
                    fade_in: clip.fade_in.max(x_in).min(clip.duration),
                    fade_out: clip.fade_out.max(x_out).min(clip.duration),
                });
            }
        }
    }

    let mut g: Vec<String> = vec![];
    if pass.video {
        g.push("[0:v]format=rgba[b0]".into());
        // Radius blur latar sebanding ukuran kanvas (40 px untuk 1080).
        let blur_radius = ((w.min(h) as f64) / 27.0).round().max(4.0) as u32;
        let mut fx_iter = fx.iter().peekable();
        let mut fx_n = 0usize;
        // Label komposisi saat ini; tiap efek menghasilkan label baru (label filtergraph harus unik).
        let mut cur = "[b0]".to_string();
        // Efek yang berlaku sebelum visual ke-i (lapisan di bawahnya sudah tergambar di `cur`).
        let mut apply_fx = |g: &mut Vec<String>, cur: &mut String, i: usize, fx_iter: &mut std::iter::Peekable<std::slice::Iter<FxStep>>| {
            while let Some(e) = fx_iter.peek() {
                if e.after != i {
                    break;
                }
                let out = format!("[e{fx_n}]");
                g.extend(e.effect.filter_lines(cur, &out, &format!("x{fx_n}"), e.start, e.end, w, h));
                *cur = out;
                fx_n += 1;
                fx_iter.next();
            }
        };
        for (i, v) in visuals.iter().enumerate() {
            apply_fx(&mut g, &mut cur, i, &mut fx_iter);
            let p = &v.place;
            let pts = if (v.speed - 1.0).abs() > 1e-6 {
                format!("setpts=(PTS-STARTPTS)/{}+{}/TB", f(v.speed), f(v.start))
            } else {
                format!("setpts=PTS-STARTPTS+{}/TB", f(v.start))
            };
            let mut base = cur.clone();
            let mut src = format!("[{}:v]", v.input);
            if v.blur_bg {
                g.push(format!("[{}:v]split=2[s{i}][q{i}]", v.input));
                g.push(format!(
                    "[q{i}]format=rgba,scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}{},boxblur={blur_radius}:2,{pts}[l{i}]",
                    v.color
                ));
                let fade_bg = match &v.t_in {
                    Some((k, d, s)) if k == "fade" => format!(",fade=t=in:st={}:d={}:alpha=1", f(*s), f(*d)),
                    _ => String::new(),
                };
                if !fade_bg.is_empty() {
                    let last = g.pop().unwrap();
                    g.push(last.replace(&format!("[l{i}]"), &format!("{fade_bg}[l{i}]")));
                }
                g.push(format!("{cur}[l{i}]overlay=x=0:y=0:format=rgb:eof_action=pass:enable='between(t,{},{})'[k{i}]", f(v.start), f(v.end)));
                base = format!("[k{i}]");
                src = format!("[s{i}]");
            }
            // Chroma key dikerjakan pada resolusi asli sebelum diskalakan.
            let key = v.colorkey.as_ref().map(|k| format!(",colorkey={k}")).unwrap_or_default();
            let mut chain = match &v.motion {
                // Diperbesar dulu supaya pembulatan posisi zoompan tidak membuat gambar bergetar.
                Some((m, frames, first)) => {
                    let (z, x, y) = m.zoompan_exprs_from(*frames, *first);
                    // Faktor dibatasi agar sisi terpanjang <= 4096 px (klip besar tetap ringan di laptop).
                    let k = (4096.0 / p.w.max(p.h) as f64).clamp(1.0, 2.0);
                    format!(
                        "{src}format=rgba{key},fps={},scale={}:{},zoompan=z='{z}':x='{x}':y='{y}':d=1:s={}x{}:fps={}",
                        f(fps),
                        (p.w as f64 * k).round() as u32,
                        (p.h as f64 * k).round() as u32,
                        p.w,
                        p.h,
                        f(fps)
                    )
                }
                None => format!("{src}format=rgba{key},scale={}:{}", p.w, p.h),
            };
            // Warna setelah skala (lebih ringan untuk sumber besar), sebelum rotasi.
            chain.push_str(&v.color);
            if v.kf.is_none() && p.radians.abs() > 1e-9 {
                chain.push_str(&format!(",rotate={}:ow={}:oh={}:c=0x00000000", f(p.radians), p.ow, p.oh));
            }
            if v.opacity < 0.999 {
                chain.push_str(&format!(",colorchannelmixer=aa={}", f(v.opacity)));
            }
            chain.push_str(&format!(",{pts}"));
            // Animasi keyframe memakai waktu timeline, jadi diterapkan sesudah setpts.
            if let Some(kf) = &v.kf {
                chain.push_str(&kf.chain);
            }
            if let Some((k, d, s)) = &v.t_in {
                if k == "fade" {
                    chain.push_str(&format!(",fade=t=in:st={}:d={}:alpha=1", f(*s), f(*d)));
                }
            }
            chain.push_str(&format!("[v{i}]"));
            g.push(chain);
            let base_xy = match &v.kf {
                Some(kf) => (kf.x.clone(), kf.y.clone()),
                None => (p.x.to_string(), p.y.to_string()),
            };
            let (ox, oy) = overlay_xy(base_xy, w, h, &v.t_in, &v.t_out);
            let quote = |e: String| if e.contains('(') { format!("'{e}'") } else { e };
            g.push(format!(
                "{base}[v{i}]overlay=x={}:y={}:format=rgb:eof_action=pass:enable='between(t,{},{})'[b{}]",
                quote(ox),
                quote(oy),
                f(v.start),
                f(v.end),
                i + 1
            ));
            cur = format!("[b{}]", i + 1);
        }
        apply_fx(&mut g, &mut cur, visuals.len(), &mut fx_iter);
        let last = cur.clone();
        match &pass.ass {
            Some(_) => g.push(format!("{last}ass={},format=yuv420p[vout]", pass.ass_file)),
            None => g.push(format!("{last}format=yuv420p[vout]")),
        }
    }

    if pass.audio {
        let mix_out = if pass.loudnorm { "[amix]" } else { "[aout]" };
        if audios.is_empty() {
            g.push(format!("[1:a]anull{mix_out}"));
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
            g.push(format!("{labels}amix=inputs={}:normalize=0:duration=first:dropout_transition=0{mix_out}", audios.len() + 1));
        }
        if pass.loudnorm {
            g.push(format!("[amix]{LOUDNORM}[aout]"));
        }
    }
    let filter_script = g.join(";\n") + "\n";

    args.extend(strings(&["-/filter_complex", &pass.filter_file]));
    if pass.video {
        args.extend(strings(&["-map", "[vout]"]));
    }
    if pass.audio {
        args.extend(strings(&["-map", "[aout]"]));
    }
    Ok((args, filter_script, copies))
}

/// Argumen encoder video (sama untuk semua bagian supaya bisa digabung tanpa encode ulang).
fn video_codec_args(opts: &ExportOptions, fps: f64) -> Vec<String> {
    let mut a = match opts.encoder {
        Encoder::X264 => {
            let (preset, crf) = match opts.quality {
                Quality::Fast => ("veryfast", "23"),
                Quality::Balanced => ("medium", "20"),
                Quality::Best => ("slow", "18"),
            };
            strings(&["-c:v", "libx264", "-preset", preset, "-crf", crf, "-profile:v", "high", "-pix_fmt", "yuv420p"])
        }
        Encoder::Qsv => {
            let (preset, q) = match opts.quality {
                Quality::Fast => ("veryfast", "26"),
                Quality::Balanced => ("medium", "23"),
                Quality::Best => ("slow", "20"),
            };
            strings(&["-c:v", "h264_qsv", "-preset", preset, "-global_quality", q, "-pix_fmt", "nv12"])
        }
    };
    let gop = (fps * 2.0).round().max(1.0) as i64;
    a.extend(strings(&["-g", &gop.to_string(), "-r", &f(fps)]));
    a
}

fn check(project: &Project, opts: &ExportOptions) -> Result<f64, String> {
    if opts.output.trim().is_empty() {
        return Err("Tentukan file hasil ekspor.".into());
    }
    let duration = project.duration();
    if duration <= 0.0 {
        return Err("Timeline masih kosong, tidak ada yang bisa diekspor.".into());
    }
    Ok(duration)
}

/// Rencana ekspor sekali jalan (video + audio dalam satu proses ffmpeg).
pub fn build_plan(project: &Project, opts: &ExportOptions) -> Result<ExportPlan, String> {
    let duration = check(project, opts)?;
    let ass_text = ass::build_ass(project);
    let pass = Pass {
        video: true,
        audio: true,
        loudnorm: opts.normalize_loudness,
        ass: ass_text.clone(),
        filter_file: FILTER_FILE.into(),
        ass_file: ASS_FILE.into(),
        duration,
    };
    let (mut args, filter_script, copies) = build_pass(project, &pass)?;
    args.extend(video_codec_args(opts, project.canvas.fps));
    args.extend(strings(&["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-movflags", "+faststart", "-t", &f(duration), &opts.output]));
    Ok(ExportPlan { args, filter_script, ass: ass_text, duration, copies })
}

/// Klip visual yang membutuhkan input ffmpeg sendiri: (mulai, selesai).
fn visual_intervals(project: &Project) -> Vec<(f64, f64)> {
    let mut v = vec![];
    for track in &project.tracks {
        for c in track.clips.iter().filter(|c| c.duration > 0.0) {
            let Some(asset) = c.asset_id.as_deref().and_then(|id| project.asset(id)) else { continue };
            if is_visual(track.kind, track.hidden, c.kind, asset.kind) {
                v.push((c.start, c.end()));
            }
        }
    }
    v
}

/// Batas bagian [0, t1, t2, ..., durasi] untuk ekspor bertahap. Tiap bagian sepanjang
/// maksimal `max_len` detik dan sebisa mungkin hanya memuat `max_inputs` klip visual;
/// batas dipilih di tepi klip bila memungkinkan dan dibulatkan ke frame.
pub fn split_points(project: &Project, max_inputs: usize, max_len: f64) -> Vec<f64> {
    let duration = project.duration();
    let fps = project.canvas.fps.max(1.0);
    let iv = visual_intervals(project);
    let count = |a: f64, b: f64| iv.iter().filter(|(s, e)| *s < b - 1e-9 && *e > a + 1e-9).count();
    let mut cands: Vec<f64> = iv.iter().flat_map(|(s, e)| [*s, *e]).filter(|t| *t > 1e-9 && *t < duration - 1e-9).collect();
    cands.push(duration);
    cands.sort_by(|a, b| a.partial_cmp(b).unwrap());
    cands.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let mut bounds = vec![0.0];
    let mut t0 = 0.0;
    while t0 < duration - 1e-6 {
        let limit = (t0 + max_len).min(duration);
        let inside: Vec<f64> = cands.iter().copied().filter(|c| *c > t0 + 1e-6 && *c <= limit + 1e-9).collect();
        let best = inside.iter().copied().filter(|c| count(t0, *c) <= max_inputs).last();
        let mut t1 = match best {
            Some(c) => c,
            // Terlalu banyak klip bertumpuk sekaligus: ambil tepi klip terdekat, atau potong di tengah.
            None => inside.first().copied().unwrap_or(limit),
        };
        t1 = ((t1 * fps).round() / fps).min(duration);
        if t1 <= t0 + 1e-6 {
            t1 = ((t0 * fps).round() + 1.0) / fps;
        }
        let t1 = if duration - t1 < 0.5 / fps { duration } else { t1 };
        bounds.push(t1);
        t0 = t1;
    }
    bounds
}

/// Proyek berisi hanya klip visual yang tampil di [t0, t1), digeser ke 0. Klip yang
/// terpotong menyimpan `offset` (detik sejak awal klip) dan `span` (panjang klip utuh).
pub fn window_project(project: &Project, t0: f64, t1: f64) -> Project {
    let mut p = project.clone();
    p.captions.enabled = false;
    for track in &mut p.tracks {
        if track.kind == TrackKind::Audio {
            track.clips.clear();
            continue;
        }
        let mut clips = vec![];
        for c in track.clips.iter().filter(|c| c.kind != ClipKind::Text && c.kind != ClipKind::Audio) {
            // Efek yang terpotong di batas bagian tetap dimulai dari awal aslinya untuk kilat (flash).

            let s = c.start.max(t0);
            let e = c.end().min(t1);
            if e - s < 1e-6 {
                continue;
            }
            let off = s - c.start;
            let mut n = c.clone();
            n.span = if c.span > 0.0 { c.span } else { c.duration };
            n.offset = c.offset + off;
            n.start = s - t0;
            n.duration = e - s;
            if matches!(c.kind, ClipKind::Video) {
                n.source_in = c.source_in + off * c.effective_speed();
            }
            clips.push(n);
        }
        track.clips = clips;
    }
    p
}

fn header() -> Vec<String> {
    strings(&["-hide_banner", "-y", "-nostats", "-loglevel", "error", "-progress", "pipe:1", "-stats_period", "0.25"])
}

/// Satu frame proyek di waktu `t` sebagai PNG (tinggi maksimal `max_height`), memakai
/// penyusun yang sama dengan ekspor sehingga gambarnya sama dengan hasil MP4.
/// Hanya klip di sekitar `t` yang didekode, jadi cepat juga untuk video panjang.
pub fn build_frame(project: &Project, t: f64, output: &str, max_height: u32) -> Result<ExportJob, String> {
    let duration = check(project, &ExportOptions { output: output.to_string(), ..Default::default() })?;
    let fps = project.canvas.fps.max(1.0);
    let t0 = (t.max(0.0) * fps).floor() / fps;
    let t0 = t0.min((duration - 1.0 / fps).max(0.0));
    let t1 = t0 + 2.0 / fps;
    let wp = window_project(project, t0, t1);
    let ass_text = ass::build_ass_window(project, t0, t1);
    let pass = Pass { video: true, audio: false, loudnorm: false, ass: ass_text.clone(), filter_file: FILTER_FILE.into(), ass_file: ASS_FILE.into(), duration: t1 - t0 };
    let (mut args, filter, copies) = build_pass(&wp, &pass)?;
    args.extend(strings(&["-an", "-frames:v", "1", "-update", "1", "-f", "image2", "cutkit-frame.png"]));
    let mut files = vec![(FILTER_FILE.to_string(), filter)];
    if let Some(a) = ass_text {
        files.push((ASS_FILE.to_string(), a));
    }
    let h = max_height.clamp(64, 4320);
    let mut scale = header();
    scale.extend(strings(&["-i", "cutkit-frame.png", "-vf", &format!("scale=-2:'min(ih,{h})':flags=lanczos"), "-frames:v", "1", "-update", "1", output]));
    let steps = vec![
        ExportStep { label: "Frame".into(), args, files, copies, duration: t1 - t0, weight: 0.8 },
        ExportStep { label: "Perkecil".into(), args: scale, files: vec![], copies: vec![], duration: 0.0, weight: 0.2 },
    ];
    Ok(ExportJob { steps, duration: t1 - t0, output: output.to_string(), chunked: false })
}

/// Rencana ekspor lengkap: sekali jalan untuk proyek pendek, bertahap untuk video panjang.
pub fn build_job(project: &Project, opts: &ExportOptions) -> Result<ExportJob, String> {
    let duration = check(project, opts)?;
    let visual_count = visual_intervals(project).len();
    if visual_count <= MAX_VISUAL_INPUTS && duration <= MAX_SINGLE_PASS_SECONDS {
        let plan = build_plan(project, opts)?;
        let mut files = vec![(FILTER_FILE.to_string(), plan.filter_script.clone())];
        if let Some(a) = &plan.ass {
            files.push((ASS_FILE.to_string(), a.clone()));
        }
        let step = ExportStep { label: "Ekspor".into(), args: plan.args, files, copies: plan.copies, duration, weight: 1.0 };
        return Ok(ExportJob { steps: vec![step], duration, output: opts.output.clone(), chunked: false });
    }

    let fps = project.canvas.fps;
    let bounds = split_points(project, MAX_VISUAL_INPUTS, MAX_CHUNK_SECONDS);
    let parts = bounds.len() - 1;
    let (w_audio, w_concat) = (0.06, if opts.normalize_loudness { 0.1 } else { 0.04 });
    let w_video = 1.0 - w_audio - w_concat;
    let mut steps = vec![];

    // 1. Seluruh audio sekali jalan ke WAV (decoder audio ringan, jadi semua klip boleh dibuka).
    let audio_pass = Pass { video: false, audio: true, loudnorm: false, ass: None, filter_file: "cutkit-audio-filter.txt".into(), ass_file: String::new(), duration };
    let (mut args, filter, _) = build_pass(project, &audio_pass)?;
    args.extend(strings(&["-c:a", "pcm_s16le", "-ar", "48000", "-t", &f(duration), "-f", "wav", "cutkit-audio.wav"]));
    steps.push(ExportStep { label: "Audio".into(), args, files: vec![(audio_pass.filter_file.clone(), filter)], copies: vec![], duration, weight: w_audio });

    // 2. Video per bagian.
    let mut list = String::new();
    for i in 0..parts {
        let (t0, t1) = (bounds[i], bounds[i + 1]);
        let len = t1 - t0;
        let wp = window_project(project, t0, t1);
        let ass_text = ass::build_ass_window(project, t0, t1);
        let pass = Pass {
            video: true,
            audio: false,
            loudnorm: false,
            ass: ass_text.clone(),
            filter_file: format!("cutkit-filter-{i}.txt"),
            ass_file: format!("cutkit-captions-{i}.ass"),
            duration: len,
        };
        let (mut args, filter, copies) = build_pass(&wp, &pass)?;
        args.extend(video_codec_args(opts, fps));
        let out = format!("cutkit-part-{i}.mkv");
        args.extend(strings(&["-an", "-t", &f(len), "-f", "matroska", &out]));
        let mut files = vec![(pass.filter_file.clone(), filter)];
        if let Some(a) = ass_text {
            files.push((pass.ass_file.clone(), a));
        }
        steps.push(ExportStep { label: format!("Video bagian {}/{}", i + 1, parts), args, files, copies, duration: len, weight: w_video * len / duration });
        list.push_str(&format!("file '{out}'\n"));
    }

    // 3. Gabungkan bagian video (tanpa encode ulang) dengan audio.
    let mut args = header();
    args.extend(strings(&["-f", "concat", "-safe", "0", "-i", "cutkit-parts.txt", "-i", "cutkit-audio.wav", "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy"]));
    if opts.normalize_loudness {
        args.extend(strings(&["-af", LOUDNORM]));
    }
    args.extend(strings(&["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-movflags", "+faststart", "-t", &f(duration), &opts.output]));
    steps.push(ExportStep { label: "Menggabungkan".into(), args, files: vec![("cutkit-parts.txt".into(), list)], copies: vec![], duration, weight: w_concat });

    Ok(ExportJob { steps, duration, output: opts.output.clone(), chunked: true })
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
    fn frame_decodes_only_clips_around_t() {
        let job = build_frame(&project(), 3.2, "D:/out/f.png", 720).unwrap();
        let a = job.steps[0].args.join(" ");
        assert!(a.contains("-ss 5.7"), "video maju ke detik 3.2 sumber: {a}");
        assert!(!a.contains("x.png"), "meme sudah lewat di detik 3.2");
        assert!(a.ends_with("-an -frames:v 1 -update 1 -f image2 cutkit-frame.png"));
        assert!(job.steps[1].args.join(" ").contains("min(ih,720)"));
        assert!(build_frame(&Project::new("x"), 0.0, "f.png", 720).is_err());
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
        assert!(a.ends_with("-c:v libx264 -preset medium -crf 20 -profile:v high -pix_fmt yuv420p -g 60 -r 30 -c:a aac -b:a 192k -ar 48000 -movflags +faststart -t 4.5 D:/out/hasil.mp4"));
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
        assert!(g.contains("[b1]ass=cutkit-captions.ass,format=yuv420p[vout]"));
        assert!(a.contains("-c:v h264_qsv -preset veryfast -global_quality 26 -pix_fmt nv12"));
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
    fn motion_uses_upscaled_zoompan() {
        let mut p = project();
        p.tracks[1].clips[0].motion = Motion { kind: "zoom-in".into(), amount: 0.2 };
        p.tracks[0].clips[0].motion = Motion { kind: "pan-left".into(), amount: 0.1 };
        let g = build_plan(&p, &opts()).unwrap().filter_script;
        // gambar 2 detik @30 fps = 60 frame
        assert!(g.contains("[3:v]format=rgba,fps=30,scale=864:692,zoompan=z='1+0.2*min(on/59,1)':x='(iw-iw/zoom)*0.5':y='(ih-ih/zoom)*0.5':d=1:s=432x346:fps=30,rotate="), "{g}");
        assert!(g.contains("[2:v]format=rgba,fps=30,scale=4096:2304,zoompan=z='1+0.1':x='(iw-iw/zoom)*(1-min(on/119,1))':y='(ih-ih/zoom)*0.5':d=1:s=3413x1920:fps=30,setpts"), "{g}");
        p.tracks[0].clips[0].motion.kind = "none".into();
        assert!(!build_plan(&p, &opts()).unwrap().filter_script.contains("[2:v]format=rgba,fps"));
    }

    #[test]
    fn blur_background_splits_input_and_overlays_behind() {
        let mut p = project();
        p.tracks[0].clips[0].fit = Fit::Contain;
        p.tracks[0].clips[0].background = "blur".into();
        let g = build_plan(&p, &opts()).unwrap().filter_script;
        let lines: Vec<&str> = g.lines().collect();
        assert_eq!(lines[1], "[2:v]split=2[s0][q0];");
        assert_eq!(lines[2], "[q0]format=rgba,scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,boxblur=40:2,setpts=PTS-STARTPTS+0/TB[l0];");
        assert_eq!(lines[3], "[b0][l0]overlay=x=0:y=0:format=rgb:eof_action=pass:enable='between(t,0,4)'[k0];");
        assert!(lines[4].starts_with("[s0]format=rgba,scale=1080:608,"));
        assert!(lines[5].starts_with("[k0][v0]overlay=x=0:y=656:"));
    }

    #[test]
    fn loudness_normalization_is_last_audio_step() {
        let plan = build_plan(&project(), &ExportOptions { normalize_loudness: true, ..opts() }).unwrap();
        let lines: Vec<&str> = plan.filter_script.lines().collect();
        assert!(lines[lines.len() - 2].ends_with("dropout_transition=0[amix];"));
        assert_eq!(lines[lines.len() - 1], "[amix]loudnorm=I=-14:TP=-1.5:LRA=11,aresample=48000[aout]");
        assert!(!build_plan(&project(), &opts()).unwrap().filter_script.contains("loudnorm"));
    }

    #[test]
    fn chroma_key_runs_before_scaling() {
        let mut p = project();
        p.tracks[1].clips[0].chroma = Chroma { enabled: true, color: "#00FF00".into(), similarity: 0.25, blend: 0.05 };
        let g = build_plan(&p, &opts()).unwrap().filter_script;
        assert!(g.contains("[3:v]format=rgba,colorkey=0x00ff00:0.25:0.05,scale=432:346,"), "{g}");
    }

    #[test]
    fn atempo_chain_ranges() {
        assert_eq!(atempo_chain(1.0), "");
        assert_eq!(atempo_chain(1.5), "atempo=1.5");
        assert_eq!(atempo_chain(0.3), "atempo=0.5,atempo=0.6");
        assert_eq!(atempo_chain(0.1), "atempo=0.5,atempo=0.5,atempo=0.5,atempo=0.8");
    }

    /// Proyek panjang: 40 klip video 5 detik berurutan (200 detik) + musik penuh + logo di atas semuanya.
    fn long_project() -> Project {
        let mut p = project();
        p.tracks[0].clips.clear();
        p.tracks[1].clips.clear();
        p.tracks[2].clips.clear();
        for i in 0..40 {
            p.tracks[0].clips.push(Clip { id: format!("c{i}"), asset_id: Some("vid".into()), start: i as f64 * 5.0, duration: 5.0, source_in: 1.0, ..Default::default() });
        }
        p.tracks[1].clips.push(Clip { id: "logo".into(), kind: ClipKind::Image, asset_id: Some("img".into()), start: 0.0, duration: 200.0, motion: Motion { kind: "zoom-in".into(), amount: 0.2 }, ..Default::default() });
        p.tracks[2].clips.push(Clip { id: "music".into(), kind: ClipKind::Audio, asset_id: Some("sfx".into()), start: 0.0, duration: 2.0, ..Default::default() });
        p.captions.enabled = true;
        p.captions.words = (0..100).map(|i| Word { text: format!("k{i}"), start: i as f64 * 2.0, end: i as f64 * 2.0 + 1.5 }).collect();
        p
    }

    #[test]
    fn short_projects_export_in_one_step() {
        let job = build_job(&project(), &opts()).unwrap();
        assert!(!job.chunked);
        assert_eq!(job.steps.len(), 1);
        assert_eq!(job.steps[0].files[0].0, FILTER_FILE);
    }

    #[test]
    fn split_points_respect_input_limit_and_clip_edges() {
        let p = long_project();
        let b = split_points(&p, 16, 120.0);
        assert_eq!(b.first(), Some(&0.0));
        assert_eq!(b.last(), Some(&200.0));
        for w in b.windows(2) {
            assert!(w[1] - w[0] <= 120.0 + 1e-9);
            // logo selalu ada + paling banyak 15 klip utama per bagian, dan batas jatuh di tepi klip (kelipatan 5)
            let n = p.tracks[0].clips.iter().filter(|c| c.start < w[1] - 1e-9 && c.end() > w[0] + 1e-9).count();
            assert!(n + 1 <= 16, "{w:?} memuat {n} klip");
            assert!((w[1] % 5.0).abs() < 1e-9 || (w[1] - 200.0).abs() < 1e-9);
        }
        assert_eq!(b, vec![0.0, 75.0, 150.0, 200.0]);
    }

    #[test]
    fn window_project_offsets_cut_clips() {
        let mut p = long_project();
        p.tracks[0].clips[0].speed = 2.0;
        p.tracks[0].clips[0].duration = 7.0;
        let w = window_project(&p, 3.0, 10.0);
        let c0 = &w.tracks[0].clips[0];
        assert_eq!((c0.start, c0.duration, c0.offset, c0.span), (0.0, 4.0, 3.0, 7.0));
        assert_eq!(c0.source_in, 1.0 + 3.0 * 2.0, "masuk sumber maju sesuai kecepatan");
        let logo = &w.tracks[1].clips[0];
        assert_eq!((logo.start, logo.duration, logo.offset, logo.span), (0.0, 7.0, 3.0, 200.0));
        assert!(w.tracks[2].clips.is_empty(), "audio dirender terpisah");
        assert!(!w.captions.enabled);
        // Gerak logo melanjutkan dari frame ke-90 dari 6000, bukan mulai dari awal.
        let (_, filter, _) = build_pass(&w, &Pass { video: true, audio: false, loudnorm: false, ass: None, filter_file: "x".into(), ass_file: "y".into(), duration: 7.0 }).unwrap();
        assert!(filter.contains("zoompan=z='1+0.2*min((on+90)/5999,1)'"), "{filter}");
    }

    #[test]
    fn long_projects_export_in_parts_then_concat() {
        let job = build_job(&long_project(), &ExportOptions { normalize_loudness: true, ..opts() }).unwrap();
        assert!(job.chunked);
        let labels: Vec<&str> = job.steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["Audio", "Video bagian 1/3", "Video bagian 2/3", "Video bagian 3/3", "Menggabungkan"]);
        let total: f64 = job.steps.iter().map(|s| s.weight).sum();
        assert!((total - 1.0).abs() < 1e-9);
        let audio = job.steps[0].args.join(" ");
        assert!(audio.contains("-vn -i C:/media/a.mp4") && audio.ends_with("-c:a pcm_s16le -ar 48000 -t 200 -f wav cutkit-audio.wav"));
        assert!(!audio.contains("x.png"), "gambar tidak dibuka di pass audio");
        let part = job.steps[2].args.join(" ");
        assert!(part.contains("-ss 1 -t 5 -an -i C:/media/a.mp4"));
        assert_eq!(part.matches("-i C:/media/a.mp4").count(), 15, "bagian 2 hanya membuka klip 75..150 detik");
        assert!(part.ends_with("-an -t 75 -f matroska cutkit-part-1.mkv"));
        let ass_file = &job.steps[2].files[1];
        assert_eq!(ass_file.0, "cutkit-captions-1.ass");
        assert!(ass_file.1.contains("Dialogue: 0,0:00:01.00,"), "kata pada detik 76 jadi detik 1 di bagian 2");
        let concat = job.steps[4].args.join(" ");
        assert!(concat.contains("-f concat -safe 0 -i cutkit-parts.txt -i cutkit-audio.wav -map 0:v:0 -map 1:a:0 -c:v copy -af loudnorm"));
        assert!(concat.ends_with("-movflags +faststart -t 200 D:/out/hasil.mp4"));
        assert_eq!(job.steps[4].files[0].1, "file 'cutkit-part-0.mkv'\nfile 'cutkit-part-1.mkv'\nfile 'cutkit-part-2.mkv'\n");
    }

    #[test]
    fn transitions_overlap_and_animate() {
        let mut p = project();
        // Klip video kedua menumpuk 0,5 detik dengan crossfade, gambar di track lain tidak terpengaruh.
        p.tracks[0].clips.push(Clip { id: "c2".into(), asset_id: Some("vid".into()), start: 3.5, duration: 3.0, transition: Transition { kind: "fade".into(), duration: 0.5 }, ..Default::default() });
        let g = build_plan(&p, &opts()).unwrap().filter_script;
        assert!(g.contains("setpts=PTS-STARTPTS+3.5/TB,fade=t=in:st=3.5:d=0.5:alpha=1[v1]"), "{g}");
        // Audio crossfade: klip baru masuk 0,5 detik; klip lama memakai fade out terpanjang (bawaannya 1 detik).
        assert!(g.contains("afade=t=out:st=3:d=1,adelay=0:all=1[a0]"), "{g}");
        assert!(g.contains("afade=t=in:st=0:d=0.5,adelay=3500:all=1[a1]"), "{g}");

        p.tracks[0].clips[1].transition.kind = "push-left".into();
        let g = build_plan(&p, &opts()).unwrap().filter_script;
        let e = "st(0,clip((t-3.5)/0.5,0,1));ld(0)*ld(0)*(3-2*ld(0))";
        assert!(g.contains(&format!("overlay=x='-1167-1080*({e})':y=0:")), "klip lama terdorong ke kiri: {g}");
        assert!(g.contains(&format!("overlay=x='0+1080*(1-({e}))':y=656:")), "klip baru (pas, y=656) masuk dari kanan: {g}");
        assert!(!g.contains("fade=t=in:st=3.5"));

        // Klip pertama di track tidak punya pendahulu: transisinya diabaikan.
        p.tracks[0].clips[0].transition = Transition { kind: "fade".into(), duration: 1.0 };
        assert!(!build_plan(&p, &opts()).unwrap().filter_script.contains("fade=t=in:st=0:d=1:alpha=1"));
    }

    #[test]
    fn color_filters_and_lut_copies() {
        let mut p = project();
        p.tracks[0].clips[0].color = crate::color::ColorAdjust { saturation: 0.0, lut: "D:/LUT/film.cube".into(), ..Default::default() };
        p.tracks[1].clips[0].color = crate::color::ColorAdjust { lut: "D:/LUT/film.cube".into(), ..Default::default() };
        let plan = build_plan(&p, &opts()).unwrap();
        assert!(plan.filter_script.contains("scale=3413:1920,colorchannelmixer=rr=0.213:rg=0.715:rb=0.072:gr=0.213:gg=0.715:gb=0.072:br=0.213:bg=0.715:bb=0.072,lut3d=file=cutkit-lut-0.cube,setpts"), "{}", plan.filter_script);
        assert!(plan.filter_script.contains("scale=432:346,lut3d=file=cutkit-lut-0.cube,rotate="), "LUT yang sama disalin sekali");
        assert_eq!(plan.copies, vec![("cutkit-lut-0.cube".to_string(), "D:/LUT/film.cube".to_string())]);
    }

    #[test]
    fn effects_apply_to_layers_below() {
        let mut p = project();
        let mut fx_track = Track { id: "fx".into(), kind: TrackKind::Video, name: "Efek".into(), ..Default::default() };
        fx_track.clips.push(Clip { id: "e1".into(), kind: ClipKind::Effect, start: 1.0, duration: 1.0, effect: Some(Effect { kind: "bw".into(), intensity: 0.5 }), ..Default::default() });
        fx_track.clips.push(Clip { id: "e2".into(), kind: ClipKind::Effect, start: 2.0, duration: 1.0, effect: Some(Effect { kind: "shake".into(), intensity: 0.5 }), ..Default::default() });
        p.tracks.insert(2, fx_track); // di atas video dan overlay, di bawah audio
        let g = build_plan(&p, &opts()).unwrap().filter_script;
        // Efek diterapkan setelah dua lapisan visual ([b2]) dan sebelum format akhir.
        assert!(g.contains("[b2]hue=s=0:enable='between(t,1,2)'[e0];"), "{g}");
        // Getar: salinan 1,06x (1144x2036) digeser maks 80% dari selisih tepinya.
        assert!(g.contains("[e0]split=2[x1a][x1b];\n[x1b]scale=1144:2036[x1c];\n[x1a][x1c]overlay=x='-32+26*sin(t*53)':y='-58+46*cos(t*41)':enable='between(t,2,3)'[e1];"), "{g}");
        assert!(g.contains("[e1]format=yuv420p[vout]"), "{g}");
        // Efek tidak membuka input apa pun.
        assert_eq!(build_plan(&p, &opts()).unwrap().args.iter().filter(|a| *a == "-i").count(), 5);
    }

    #[test]
    fn every_effect_has_a_filter() {
        for k in crate::model::EFFECTS {
            let lines = Effect { kind: k.to_string(), intensity: 0.5 }.filter_lines("[in]", "[out]", "t0", 1.0, 2.0, 1080, 1920);
            assert!(lines.last().unwrap().ends_with("[out]"), "{k}: {lines:?}");
            assert!(lines.iter().any(|l| l.contains("between(t,1,2)")), "{k} harus terbatas waktu");
        }
    }

    #[test]
    fn keyframes_animate_scale_rotation_position_and_opacity() {
        let mut p = project();
        let c = &mut p.tracks[1].clips[0]; // gambar 500x400, mulai 1, durasi 2
        c.keyframes = vec![
            Keyframe { t: 0.0, x: 0.0, y: 0.0, scale: 0.5, rotation: 0.0, opacity: 1.0 },
            Keyframe { t: 1.0, x: 200.0, y: -100.0, scale: 1.0, rotation: 90.0, opacity: 0.5 },
        ];
        let g = build_plan(&p, &opts()).unwrap().filter_script;
        // Ukuran dasar skala 1 = 1080x864 (pas di kanvas); kotak tetap = diagonal x skala maks.
        assert!(g.contains("[3:v]format=rgba,scale=1080:864,setpts=PTS-STARTPTS+1/TB,scale=w='max(2,2*trunc(1080*(if(lt(t,1),0.5,if(lt(t,2),0.5+(0.5)*(t-1)/1,1)))/2))'"), "{g}");
        assert!(g.contains(":eval=frame,pad=w=1386:h=1386:x='(ow-iw)/2':y='(oh-ih)/2':color=0x00000000:eval=frame,rotate=a='if(lt(t,1),0,if(lt(t,2),0+(1.5708)*(t-1)/1,1.5708))':ow=1386:oh=1386"), "{g}");
        assert!(g.contains("sendcmd=c='1 colorchannelmixer@kf1 aa 1;"), "{g}");
        assert!(g.contains("2 colorchannelmixer@kf1 aa 0.5',colorchannelmixer@kf1=aa=1[v1]"), "{g}");
        assert!(g.contains("overlay=x='-153+(if(lt(t,1),0,if(lt(t,2),0+(200)*(t-1)/1,200)))':y='267+(if(lt(t,1),0,if(lt(t,2),0+(-100)*(t-1)/1,-100)))'"), "{g}");
        assert!(!g.contains("rotate=0.2618"), "transform statis diabaikan saat ada animasi");
        // Satu keyframe = transform tetap dari keyframe itu.
        p.tracks[1].clips[0].keyframes.truncate(1);
        let g = build_plan(&p, &opts()).unwrap().filter_script;
        assert!(g.contains("[3:v]format=rgba,scale=540:432,setpts"), "{g}");
    }

    #[test]
    fn keyframe_value_matches_js_rules() {
        let k = [Keyframe { t: 0.0, x: -100.0, ..Default::default() }, Keyframe { t: 2.0, x: 100.0, ..Default::default() }];
        assert_eq!(keyframe_value(&k, 1.0, |k| k.x), 0.0);
        assert_eq!(keyframe_value(&k, -1.0, |k| k.x), -100.0);
        assert_eq!(keyframe_value(&k, 5.0, |k| k.x), 100.0);
    }

    #[test]
    fn alpha_webm_uses_libvpx_decoder() {
        let mut p = project();
        p.assets[0].alpha = true;
        let a = build_plan(&p, &opts()).unwrap().args.join(" ");
        assert!(a.contains("-ss 2.5 -t 4 -c:v libvpx-vp9 -i C:/media/a.mp4"), "{a}");
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
