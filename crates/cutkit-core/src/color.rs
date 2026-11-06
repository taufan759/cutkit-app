//! Penyesuaian warna per klip. Rumus sama dengan filter CSS (brightness, saturate,
//! hue-rotate) ditambah suhu, dan dicerminkan oleh `ui/core/color.js` untuk preview
//! (filter SVG di ruang sRGB). Di ekspor: kontras lewat `lutrgb`, sisanya satu
//! `colorchannelmixer`, lalu LUT .cube lewat `lut3d`.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ColorAdjust {
    /// 1 = asli; 0..2.
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
    /// Derajat, -180..180.
    pub hue: f64,
    /// -1 (dingin/biru) .. 1 (hangat/oranye).
    pub temperature: f64,
    /// Path file LUT .cube; kosong = tanpa LUT.
    pub lut: String,
}

impl Default for ColorAdjust {
    fn default() -> Self {
        ColorAdjust { brightness: 1.0, contrast: 1.0, saturation: 1.0, hue: 0.0, temperature: 0.0, lut: String::new() }
    }
}

type M3 = [[f64; 3]; 3];

fn mul(a: &M3, b: &M3) -> M3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn saturate(s: f64) -> M3 {
    [
        [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
    ]
}

fn hue_rotate(deg: f64) -> M3 {
    let (s, c) = deg.to_radians().sin_cos();
    [
        [0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928],
        [0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.140, 0.072 - c * 0.072 - s * 0.283],
        [0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072],
    ]
}

impl ColorAdjust {
    /// Matriks warna gabungan: suhu x rona x saturasi x kecerahan.
    pub fn matrix(&self) -> M3 {
        let b = self.brightness.clamp(0.0, 3.0);
        let bright = [[b, 0.0, 0.0], [0.0, b, 0.0], [0.0, 0.0, b]];
        let t = self.temperature.clamp(-1.0, 1.0);
        let temp = [[1.0 + 0.15 * t, 0.0, 0.0], [0.0, 1.0 + 0.03 * t, 0.0], [0.0, 0.0, 1.0 - 0.15 * t]];
        mul(&temp, &mul(&hue_rotate(self.hue), &mul(&saturate(self.saturation.clamp(0.0, 3.0)), &bright)))
    }

    pub fn is_identity(&self) -> bool {
        (self.brightness - 1.0).abs() < 1e-6 && (self.contrast - 1.0).abs() < 1e-6 && (self.saturation - 1.0).abs() < 1e-6 && self.hue.abs() < 1e-6 && self.temperature.abs() < 1e-6 && self.lut.trim().is_empty()
    }

    /// Rantai filter ffmpeg (diawali koma) untuk penyesuaian ini. `lut_name` = nama file LUT di folder kerja.
    pub fn filters(&self, lut_name: Option<&str>) -> String {
        let mut out = String::new();
        let c = self.contrast.clamp(0.0, 3.0);
        if (c - 1.0).abs() > 1e-6 {
            let e = format!("clip((val-128)*{}+128,0,255)", fmt(c));
            out.push_str(&format!(",lutrgb=r='{e}':g='{e}':b='{e}'"));
        }
        let m = self.matrix();
        let identity = (0..3).all(|i| (0..3).all(|j| (m[i][j] - if i == j { 1.0 } else { 0.0 }).abs() < 1e-6));
        if !identity {
            let n = ["r", "g", "b"];
            let parts: Vec<String> = (0..3).flat_map(|i| (0..3).map(move |j| (i, j))).map(|(i, j)| format!("{}{}={}", n[i], n[j], fmt(m[i][j]))).collect();
            out.push_str(&format!(",colorchannelmixer={}", parts.join(":")));
        }
        if let Some(name) = lut_name {
            out.push_str(&format!(",lut3d=file={name}"));
        }
        out
    }
}

fn fmt(v: f64) -> String {
    let s = format!("{:.5}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_and_filters() {
        let c = ColorAdjust::default();
        assert!(c.is_identity());
        assert_eq!(c.filters(None), "");
        let gray = ColorAdjust { saturation: 0.0, ..Default::default() };
        assert_eq!(gray.filters(None), ",colorchannelmixer=rr=0.213:rg=0.715:rb=0.072:gr=0.213:gg=0.715:gb=0.072:br=0.213:bg=0.715:bb=0.072");
        let con = ColorAdjust { contrast: 1.3, brightness: 1.1, ..Default::default() };
        assert_eq!(con.filters(Some("cutkit-lut-0.cube")), ",lutrgb=r='clip((val-128)*1.3+128,0,255)':g='clip((val-128)*1.3+128,0,255)':b='clip((val-128)*1.3+128,0,255)',colorchannelmixer=rr=1.1:rg=0:rb=0:gr=0:gg=1.1:gb=0:br=0:bg=0:bb=1.1,lut3d=file=cutkit-lut-0.cube");
    }

    #[test]
    fn hue_360_is_identity_and_rows_keep_gray() {
        let m = ColorAdjust { hue: 360.0, ..Default::default() }.matrix();
        for i in 0..3 {
            for j in 0..3 {
                assert!((m[i][j] - if i == j { 1.0 } else { 0.0 }).abs() < 1e-3);
            }
        }
        // Saturasi dan rona menjaga abu-abu tetap abu-abu (jumlah baris = 1).
        let m = ColorAdjust { hue: 47.0, saturation: 1.4, ..Default::default() }.matrix();
        for row in m {
            assert!((row.iter().sum::<f64>() - 1.0).abs() < 2e-3, "{row:?}");
        }
    }
}
