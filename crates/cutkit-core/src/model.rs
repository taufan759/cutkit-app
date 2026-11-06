//! Model proyek CutKit. Disimpan sebagai JSON; semua field punya default agar
//! file dari versi lama tetap bisa dibuka.

use serde::{Deserialize, Serialize};

pub const PROJECT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub canvas: Canvas,
    pub assets: Vec<Asset>,
    pub tracks: Vec<Track>,
    pub captions: Captions,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub background: String,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas { width: 1080, height: 1920, fps: 30.0, background: "#000000".into() }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    #[default]
    Video,
    Image,
    Audio,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Asset {
    pub id: String,
    pub path: String,
    pub kind: AssetKind,
    pub name: String,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    pub has_video: bool,
    /// MP4 kecil yang diputar di preview bila file aslinya tidak bisa diputar WebView2.
    pub proxy: Option<String>,
    /// PNG bentuk gelombang untuk ditampilkan di timeline.
    pub waveform: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    #[default]
    Video,
    Audio,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    pub name: String,
    pub muted: bool,
    pub hidden: bool,
    pub clips: Vec<Clip>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ClipKind {
    #[default]
    Video,
    Image,
    Audio,
    Text,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    #[default]
    Contain,
    Cover,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Clip {
    pub id: String,
    pub kind: ClipKind,
    pub asset_id: Option<String>,
    /// Posisi di timeline, detik.
    pub start: f64,
    pub duration: f64,
    /// Offset di dalam file sumber, detik.
    #[serde(rename = "in")]
    pub source_in: f64,
    /// Kecepatan putar (1 = normal). Klip memakai `duration * speed` detik dari sumber.
    pub speed: f64,
    pub volume: f64,
    pub muted: bool,
    pub fade_in: f64,
    pub fade_out: f64,
    pub fit: Fit,
    pub transform: Transform,
    /// Gerak isi klip (Ken Burns): zoom atau geser pelan di dalam bingkai klip.
    pub motion: Motion,
    /// Latar di belakang klip yang tidak memenuhi kanvas: "none" atau "blur" (klip yang sama, diperbesar dan diburamkan).
    pub background: String,
    /// Hapus warna latar (green screen).
    pub chroma: Chroma,
    /// Transisi masuk dari klip sebelumnya di track yang sama; klip ini menumpuk klip
    /// sebelumnya selama `duration` detik.
    pub transition: Transition,
    /// Penyesuaian warna dan LUT.
    pub color: crate::color::ColorAdjust,
    pub text: Option<TextClip>,
    /// Hanya saat ekspor bertahap: detik sejak awal klip asli (klip terpotong di batas bagian).
    #[serde(skip)]
    pub offset: f64,
    /// Hanya saat ekspor bertahap: panjang klip asli (0 = sama dengan duration).
    #[serde(skip)]
    pub span: f64,
}

impl Default for Clip {
    fn default() -> Self {
        Clip {
            id: String::new(),
            kind: ClipKind::Video,
            asset_id: None,
            start: 0.0,
            duration: 0.0,
            source_in: 0.0,
            speed: 1.0,
            volume: 1.0,
            muted: false,
            fade_in: 0.0,
            fade_out: 0.0,
            fit: Fit::Contain,
            transform: Transform::default(),
            motion: Motion::default(),
            background: "none".into(),
            chroma: Chroma::default(),
            transition: Transition::default(),
            color: crate::color::ColorAdjust::default(),
            text: None,
            offset: 0.0,
            span: 0.0,
        }
    }
}

impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }

    /// Kecepatan yang aman dipakai (gambar dan teks selalu 1).
    pub fn effective_speed(&self) -> f64 {
        if matches!(self.kind, ClipKind::Image | ClipKind::Text) || !self.speed.is_finite() || self.speed <= 0.0 {
            1.0
        } else {
            self.speed.clamp(0.1, 10.0)
        }
    }

    /// Panjang bagian sumber yang dipakai klip ini (detik).
    pub fn source_duration(&self) -> f64 {
        self.duration * self.effective_speed()
    }
}

/// Posisi relatif terhadap tengah kanvas (piksel kanvas), skala dikalikan setelah
/// klip dipaskan ke kanvas, rotasi searah jarum jam dalam derajat.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Transform { x: 0.0, y: 0.0, scale: 1.0, rotation: 0.0, opacity: 1.0 }
    }
}

/// Jenis: none, fade, slide-left, slide-right, slide-up, slide-down, push-left, push-right.
/// "slide-left" = klip baru masuk dari kanan bergerak ke kiri; "push" juga mendorong klip lama keluar.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Transition {
    pub kind: String,
    pub duration: f64,
}

impl Default for Transition {
    fn default() -> Self {
        Transition { kind: "none".into(), duration: 0.5 }
    }
}

impl Transition {
    pub fn is_active(&self) -> bool {
        matches!(self.kind.as_str(), "fade" | "slide-left" | "slide-right" | "slide-up" | "slide-down" | "push-left" | "push-right") && self.duration > 0.0
    }
}

/// Chroma key: piksel yang jaraknya ke `color` di bawah `similarity` jadi transparan,
/// dengan peralihan halus selebar `blend` (sama seperti filter colorkey ffmpeg).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Chroma {
    pub enabled: bool,
    pub color: String,
    pub similarity: f64,
    pub blend: f64,
}

impl Default for Chroma {
    fn default() -> Self {
        Chroma { enabled: false, color: "#00ff00".into(), similarity: 0.3, blend: 0.1 }
    }
}

/// Jenis gerak: none, zoom-in, zoom-out, pan-left, pan-right, pan-up, pan-down.
/// `amount` = tambahan zoom (0.15 = 15% lebih dekat di ujung gerak).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Motion {
    pub kind: String,
    pub amount: f64,
}

impl Default for Motion {
    fn default() -> Self {
        Motion { kind: "none".into(), amount: 0.15 }
    }
}

impl Motion {
    pub fn is_active(&self) -> bool {
        matches!(self.kind.as_str(), "zoom-in" | "zoom-out" | "pan-left" | "pan-right" | "pan-up" | "pan-down") && self.amount > 0.0
    }

    /// Zoom dan titik fokus (0..1, 0.5 = tengah) pada kemajuan `u` (0..1). Sama dengan `motionAt` di ui/core/geometry.js.
    pub fn at(&self, u: f64) -> (f64, f64, f64) {
        let u = u.clamp(0.0, 1.0);
        let a = self.amount.clamp(0.0, 2.0);
        match self.kind.as_str() {
            "zoom-in" => (1.0 + a * u, 0.5, 0.5),
            "zoom-out" => (1.0 + a * (1.0 - u), 0.5, 0.5),
            "pan-left" => (1.0 + a, 1.0 - u, 0.5),
            "pan-right" => (1.0 + a, u, 0.5),
            "pan-up" => (1.0 + a, 0.5, 1.0 - u),
            "pan-down" => (1.0 + a, 0.5, u),
            _ => (1.0, 0.5, 0.5),
        }
    }

    /// Ekspresi zoompan (z, x, y) dengan `u` = kemajuan frame keluaran.
    pub fn zoompan_exprs(&self, frames: u64) -> (String, String, String) {
        self.zoompan_exprs_from(frames, 0)
    }

    /// Seperti `zoompan_exprs`, tetapi frame keluaran pertama adalah frame ke-`first` dari klip utuh.
    pub fn zoompan_exprs_from(&self, frames: u64, first: u64) -> (String, String, String) {
        let a = fmt_num(self.amount.clamp(0.0, 2.0));
        let on = if first > 0 { format!("(on+{first})") } else { "on".to_string() };
        let u = if frames > 1 { format!("min({on}/{},1)", frames - 1) } else { "0".to_string() };
        let (z, px, py) = match self.kind.as_str() {
            "zoom-in" => (format!("1+{a}*{u}"), "0.5".to_string(), "0.5".to_string()),
            "zoom-out" => (format!("1+{a}*(1-{u})"), "0.5".into(), "0.5".into()),
            "pan-left" => (format!("1+{a}"), format!("(1-{u})"), "0.5".into()),
            "pan-right" => (format!("1+{a}"), u.clone(), "0.5".into()),
            "pan-up" => (format!("1+{a}"), "0.5".into(), format!("(1-{u})")),
            "pan-down" => (format!("1+{a}"), "0.5".into(), u.clone()),
            _ => ("1".into(), "0.5".into(), "0.5".into()),
        };
        (z, format!("(iw-iw/zoom)*{px}"), format!("(ih-ih/zoom)*{py}"))
    }
}

fn fmt_num(v: f64) -> String {
    let s = format!("{:.4}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TextClip {
    pub text: String,
    pub style: TextStyle,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct TextStyle {
    pub font: String,
    pub size: f64,
    pub color: String,
    pub outline_color: String,
    pub outline: f64,
    pub shadow: f64,
    pub bold: bool,
    pub italic: bool,
    /// Warna kotak latar (boleh dengan alpha `#rrggbbaa`); kosong = tanpa kotak.
    pub background: String,
    pub align: String,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            font: "Impact".into(),
            size: 72.0,
            color: "#ffffff".into(),
            outline_color: "#000000".into(),
            outline: 4.0,
            shadow: 0.0,
            bold: false,
            italic: false,
            background: String::new(),
            align: "center".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Captions {
    pub enabled: bool,
    pub words: Vec<Word>,
    pub style: CaptionStyle,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct CaptionStyle {
    pub font: String,
    pub size: f64,
    pub color: String,
    pub highlight_color: String,
    pub outline_color: String,
    pub outline: f64,
    pub shadow: f64,
    pub background: String,
    pub bold: bool,
    pub uppercase: bool,
    pub words_per_page: usize,
    pub max_line_chars: usize,
    /// Pusat blok caption, pecahan tinggi kanvas (0 = atas, 1 = bawah).
    pub y: f64,
    /// Geser horizontal dari tengah, piksel kanvas.
    pub x: f64,
    pub highlight_scale: f64,
    /// Animasi kata aktif: "none" atau "pop" (membesar lalu kembali dalam POP_MS).
    pub animation: String,
}

/// Animasi pop: kata aktif mulai dari POP_FACTOR x ukuran sorotan, kembali dalam POP_MS milidetik.
pub const POP_FACTOR: f64 = 1.25;
pub const POP_MS: u32 = 120;

impl Default for CaptionStyle {
    fn default() -> Self {
        CaptionStyle {
            font: "Impact".into(),
            size: 80.0,
            color: "#ffffff".into(),
            highlight_color: "#ffe600".into(),
            outline_color: "#000000".into(),
            outline: 5.0,
            shadow: 2.0,
            background: String::new(),
            bold: false,
            uppercase: true,
            words_per_page: 3,
            max_line_chars: 16,
            y: 0.72,
            x: 0.0,
            highlight_scale: 1.15,
            animation: "none".into(),
        }
    }
}

impl Project {
    pub fn new(name: &str) -> Project {
        Project {
            version: PROJECT_VERSION,
            name: name.to_string(),
            canvas: Canvas::default(),
            assets: vec![],
            tracks: vec![
                Track { id: "v1".into(), kind: TrackKind::Video, name: "Video".into(), ..Default::default() },
                Track { id: "v2".into(), kind: TrackKind::Video, name: "Overlay".into(), ..Default::default() },
                Track { id: "a1".into(), kind: TrackKind::Audio, name: "Audio".into(), ..Default::default() },
            ],
            captions: Captions::default(),
        }
    }

    pub fn asset(&self, id: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.id == id)
    }

    /// Durasi proyek: akhir klip terakhir (caption di luar klip tidak dihitung).
    pub fn duration(&self) -> f64 {
        self.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| c.end()).fold(0.0, f64::max)
    }

    pub fn from_json(text: &str) -> Result<Project, String> {
        let p: Project = serde_json::from_str(text).map_err(|e| format!("File proyek tidak valid: {e}"))?;
        if p.canvas.width == 0 || p.canvas.height == 0 || p.canvas.fps <= 0.0 {
            return Err("Ukuran kanvas atau fps proyek tidak valid.".into());
        }
        Ok(p)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialisasi proyek")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let p = Project::new("Uji");
        let json = p.to_json();
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back.tracks.len(), 3);
        assert_eq!(back.canvas.width, 1080);
        // File minimal dari versi lain tetap bisa dibuka.
        let minimal = Project::from_json(r#"{"name":"x","tracks":[{"id":"v1","clips":[{"id":"c","start":1,"duration":2}]}]}"#).unwrap();
        assert_eq!(minimal.tracks[0].clips[0].volume, 1.0);
        assert_eq!(minimal.tracks[0].clips[0].transform.scale, 1.0);
        assert_eq!(minimal.duration(), 3.0);
        assert!(Project::from_json(r#"{"canvas":{"width":0}}"#).is_err());
    }
}
