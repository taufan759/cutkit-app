//! Penempatan klip visual di kanvas. Rumus yang sama dipakai `ui/geometry.js`
//! untuk preview, jadi perubahan di sini harus diikuti di sana.

use crate::model::{Fit, Transform};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Ukuran klip setelah diskalakan (sebelum rotasi).
    pub w: u32,
    pub h: u32,
    /// Kotak pembatas setelah rotasi; ffmpeg menghasilkan frame sebesar ini.
    pub ow: u32,
    pub oh: u32,
    /// Pojok kiri atas kotak pembatas di kanvas (boleh negatif).
    pub x: i64,
    pub y: i64,
    pub radians: f64,
}

pub fn place(canvas_w: u32, canvas_h: u32, asset_w: u32, asset_h: u32, fit: Fit, t: &Transform) -> Placement {
    let (aw, ah) = (asset_w.max(1) as f64, asset_h.max(1) as f64);
    let (cw, ch) = (canvas_w as f64, canvas_h as f64);
    let (sx, sy) = (cw / aw, ch / ah);
    let base = match fit {
        Fit::Contain => sx.min(sy),
        Fit::Cover => sx.max(sy),
    };
    let s = base * t.scale.max(0.001);
    let w = (aw * s).round().max(1.0);
    let h = (ah * s).round().max(1.0);
    let radians = t.rotation.to_radians();
    let (sin, cos) = (radians.sin().abs(), radians.cos().abs());
    // Toleransi kecil: cos(90°) tidak persis nol di floating point.
    let ow = (w * cos + h * sin - 1e-6).ceil().max(1.0);
    let oh = (w * sin + h * cos - 1e-6).ceil().max(1.0);
    let cx = cw / 2.0 + t.x;
    let cy = ch / 2.0 + t.y;
    Placement {
        w: w as u32,
        h: h as u32,
        ow: ow as u32,
        oh: oh as u32,
        x: (cx - ow / 2.0).round() as i64,
        y: (cy - oh / 2.0).round() as i64,
        radians,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contain_landscape_in_portrait() {
        let p = place(1080, 1920, 1920, 1080, Fit::Contain, &Transform::default());
        assert_eq!((p.w, p.h), (1080, 608));
        assert_eq!((p.ow, p.oh), (1080, 608));
        assert_eq!((p.x, p.y), (0, 656));
    }

    #[test]
    fn cover_scale_offset_and_rotation() {
        let t = Transform { x: 100.0, y: -200.0, scale: 0.5, rotation: 90.0, opacity: 1.0 };
        let p = place(1080, 1920, 1920, 1080, Fit::Cover, &t);
        // cover: skala 1920/1080 = 1.777; dikali 0.5 -> 1707x960, diputar 90° -> 960x1707
        assert_eq!((p.w, p.h), (1707, 960));
        assert_eq!((p.ow, p.oh), (960, 1707));
        assert_eq!((p.x, p.y), (540 + 100 - 480, 960 - 200 - 854));
    }
}
