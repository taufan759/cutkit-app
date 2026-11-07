//! Jenis transisi antar klip dan potongan filter ffmpeg-nya. Rumus yang sama ada di
//! ui/core/geometry.js (transitionAt, maskReveal) supaya preview sama dengan hasil ekspor.
//!
//! Klip yang masuk menumpuk klip sebelumnya selama `d` detik mulai `s`. Kemajuan p = (t-s)/d,
//! e = smoothstep(p). Kelompok:
//! - alpha linear p: fade, blur, zoom, spin (filter fade ffmpeg)
//! - geser: slide-*, push-* (klip lama ikut terdorong), whip-* (dorong cepat + blur horizontal)
//! - mask (maskedmerge, mask kecil dari geq lalu diperbesar): wipe-*, clock, iris, dissolve
//! - tukar di tengah + efek komposisi: dip-black, dip-white, pixelate, glitch

/// Semua jenis yang dikenal (urutan = urutan di UI).
pub const KINDS: &[&str] = &[
    "fade", "dip-black", "dip-white", "slide-left", "slide-right", "slide-up", "slide-down", "push-left", "push-right",
    "whip-left", "whip-right", "wipe-left", "wipe-right", "wipe-up", "wipe-down", "clock", "iris", "dissolve", "blur", "zoom",
    "spin", "pixelate", "glitch",
];

pub const ZOOM_FROM: f64 = 1.25;
pub const SPIN_SCALE_FROM: f64 = 0.6;
/// Puncak efek komposisi, relatif lebar kanvas, dikali sin(pi * p).
pub const BLUR_MAX: f64 = 0.02;
pub const WHIP_BLUR_MAX: f64 = 0.03;
pub const PIXEL_MAX: f64 = 0.05;
pub const GLITCH_MAX: f64 = 0.025;
/// Mask dibuat 1/MASK_DIV ukuran kanvas lalu diperbesar bilinear.
pub const MASK_DIV: u32 = 8;

pub fn known(kind: &str) -> bool {
    KINDS.contains(&kind)
}
pub fn is_push(kind: &str) -> bool {
    kind.starts_with("push-") || kind.starts_with("whip-")
}
pub fn is_mask(kind: &str) -> bool {
    kind.starts_with("wipe-") || matches!(kind, "clock" | "iris" | "dissolve")
}
/// Klip baru baru terlihat di tengah transisi (tertutup warna/efek).
pub fn swaps_at_half(kind: &str) -> bool {
    matches!(kind, "dip-black" | "dip-white" | "pixelate" | "glitch")
}
/// Alpha klip baru naik linear dari 0 ke 1.
pub fn fades_alpha(kind: &str) -> bool {
    matches!(kind, "fade" | "blur" | "zoom" | "spin")
}

fn f(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s.is_empty() || s == "-0" { "0".into() } else { s }
}

/// Ekspresi smoothstep kemajuan transisi di waktu `t`; whip memakai smoothstep dua kali (lebih tajam).
pub fn ease_expr(kind: &str, s: f64, d: f64) -> String {
    let base = format!("st(0,clip((t-{})/{},0,1));ld(0)*ld(0)*(3-2*ld(0))", f(s), f(d.max(0.001)));
    if kind.starts_with("whip-") {
        format!("st(0,{base});ld(0)*ld(0)*(3-2*ld(0))")
    } else {
        base
    }
}

/// Ekspresi geq (lum) untuk mask maskedmerge: 0 = klip baru terlihat, 255 = komposisi lama.
/// Waktu T global; W/H ukuran mask.
pub fn mask_geq(kind: &str, s: f64, d: f64) -> String {
    let e = format!("st(0,clip((T-{})/{},0,1));st(0,ld(0)*ld(0)*(3-2*ld(0)))", f(s), f(d.max(0.001)));
    let reveal = match kind {
        "wipe-left" => "gte(X+0.5,W*(1-ld(0)))".to_string(),
        "wipe-right" => "lte(X+0.5,W*ld(0))".to_string(),
        "wipe-up" => "gte(Y+0.5,H*(1-ld(0)))".to_string(),
        "wipe-down" => "lte(Y+0.5,H*ld(0))".to_string(),
        "clock" => "st(1,atan2(X+0.5-W/2,H/2-Y-0.5));st(1,if(lt(ld(1),0),ld(1)+2*PI,ld(1)));lt(ld(1),2*PI*ld(0))".to_string(),
        "iris" => "lte(hypot(X+0.5-W/2,Y+0.5-H/2),ld(0)*hypot(W/2,H/2))".to_string(),
        // derau tetap per sel mask (hash sin klasik), muncul saat ambangnya dilewati
        _ => "st(1,sin(X*12.9898+Y*78.233)*43758.5453);lt(ld(1)-floor(ld(1)),ld(0))".to_string(),
    };
    format!("{e};255*(1-({reveal}))")
}

/// Ukuran mask untuk kanvas `w`x`h`.
pub fn mask_size(w: u32, h: u32) -> (u32, u32) {
    ((w / MASK_DIV).max(2), (h / MASK_DIV).max(2))
}

/// Nilai efek komposisi (blur sigma, ukuran piksel, geseran RGB) di kemajuan `p`, untuk kanvas lebar `w`.
pub fn post_amount(kind: &str, p: f64, w: u32) -> f64 {
    let bump = (std::f64::consts::PI * p.clamp(0.0, 1.0)).sin();
    let k = match kind {
        "blur" => BLUR_MAX,
        k if k.starts_with("whip-") => WHIP_BLUR_MAX,
        "pixelate" => PIXEL_MAX,
        "glitch" => GLITCH_MAX,
        _ => 0.0,
    };
    k * w as f64 * bump
}

/// Alpha klip naik linear dari 0 di detik `s` sampai 1 di `s + d`. Filter fade ffmpeg tidak menerima
/// awal negatif (bagian/frame yang dimulai di tengah transisi), jadi saat itu alpha diatur per frame.
pub fn fade_in(s: f64, d: f64, fps: f64, tag: &str) -> String {
    if s >= 0.0 {
        return format!(",fade=t=in:st={}:d={}:alpha=1", f(s), f(d));
    }
    alpha_ramp(&format!("colorchannelmixer@{tag}"), s, d, fps, |p| p)
}

/// Lapisan warna dip: alpha 0 -> 1 di paruh pertama, 1 -> 0 di paruh kedua.
pub fn dip_alpha(s: f64, d: f64, fps: f64, tag: &str) -> String {
    if s >= 0.0 {
        return format!(",fade=t=in:st={}:d={}:alpha=1,fade=t=out:st={}:d={}:alpha=1", f(s), f(d / 2.0), f(s + d / 2.0), f(d / 2.0));
    }
    alpha_ramp(&format!("colorchannelmixer@{tag}"), s, d, fps, |p| if p < 0.5 { 2.0 * p } else { 2.0 - 2.0 * p })
}

/// Alpha per frame lewat sendcmd + colorchannelmixer, untuk t dari max(0, s) sampai s + d.
fn alpha_ramp(name: &str, s: f64, d: f64, fps: f64, a: impl Fn(f64) -> f64) -> String {
    let step = 1.0 / fps.max(1.0);
    let t0 = s.max(0.0);
    let mut cmds = vec![];
    let mut t = t0;
    while t <= s + d + step / 2.0 {
        let p = ((t - s) / d.max(0.001)).clamp(0.0, 1.0);
        cmds.push(format!("{} {name} aa {}", f(t), f(a(p))));
        t += step;
    }
    let first = a(((t0 - s) / d.max(0.001)).clamp(0.0, 1.0));
    format!(",sendcmd=c='{}',{name}=aa={}", cmds.join(";"), f(first))
}

/// Filter efek komposisi selama transisi (diterapkan ke komposisi sesudah klip baru ditumpuk),
/// nilainya diperbarui tiap frame lewat sendcmd. None bila jenis ini tanpa efek komposisi.
pub fn post_filter(kind: &str, s: f64, d: f64, w: u32, fps: f64, tag: usize) -> Option<String> {
    let (filter, init, cmd): (String, String, Box<dyn Fn(f64) -> String>) = match kind {
        "blur" => {
            let n = format!("gblur@tr{tag}");
            (n.clone(), format!("{n}=sigma=0:steps=2"), Box::new(move |v| format!("{n} sigma {}", f(v))))
        }
        k if k.starts_with("whip-") => {
            let n = format!("gblur@tr{tag}");
            (n.clone(), format!("{n}=sigma=0:sigmaV=0:steps=2"), Box::new(move |v| format!("{n} sigma {}", f(v))))
        }
        "pixelate" => {
            let n = format!("pixelize@tr{tag}");
            (n.clone(), format!("{n}=w=1:h=1"), Box::new(move |v: f64| { let b = v.round().max(1.0); format!("{n} w {b};{} {n} h {b}", "{T}") }))
        }
        "glitch" => {
            let n = format!("rgbashift@tr{tag}");
            (n.clone(), format!("{n}=rh=0:bh=0:edge=wrap"), Box::new(move |v: f64| { let r = v.round().clamp(0.0, 255.0); format!("{n} rh {r};{} {n} bh {}", "{T}", -r) }))
        }
        _ => return None,
    };
    let _ = filter;
    let step = 1.0 / fps.max(1.0);
    let frames = (d / step).ceil() as i64;
    let mut cmds = vec![];
    for i in 0..=frames {
        let lt = (i as f64 * step).min(d);
        let tt = f(s + lt);
        cmds.push(format!("{tt} {}", cmd(post_amount(kind, lt / d.max(0.001), w)).replace("{T}", &tt)));
    }
    cmds.push(format!("{} {}", f(s + d + step), cmd(0.0).replace("{T}", &f(s + d + step))));
    Some(format!("sendcmd=c='{}',{init}:enable='between(t,{},{})'", cmds.join(";"), f(s), f(s + d)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_grouped() {
        for k in KINDS {
            let groups = [is_push(k), is_mask(k), swaps_at_half(k), fades_alpha(k), k.starts_with("slide-")];
            assert!(groups.iter().filter(|g| **g).count() <= 1, "{k} punya satu kelompok");
        }
        assert!(is_push("whip-left") && is_mask("clock") && swaps_at_half("glitch") && fades_alpha("zoom"));
        assert!(!known("bounce"));
    }

    #[test]
    fn post_peaks_in_the_middle() {
        assert_eq!(post_amount("blur", 0.0, 1080), 0.0);
        assert!((post_amount("blur", 0.5, 1080) - 21.6).abs() < 1e-9);
        assert!(post_amount("pixelate", 1.0, 1080).abs() < 1e-9);
        let pf = post_filter("pixelate", 2.0, 0.5, 1080, 30.0, 3).unwrap();
        assert!(pf.starts_with("sendcmd=c='2 pixelize@tr3 w 1;2 pixelize@tr3 h 1;"), "{pf}");
        assert!(pf.ends_with("pixelize@tr3=w=1:h=1:enable='between(t,2,2.5)'"), "{pf}");
        assert!(pf.contains("2.2667 pixelize@tr3 w 54;2.2667 pixelize@tr3 h 54"), "puncak dekat tengah: {pf}");
        assert!(post_filter("wipe-left", 0.0, 1.0, 1080, 30.0, 0).is_none());
    }

    #[test]
    fn mask_expressions_use_shared_formulas() {
        let m = mask_geq("iris", 1.0, 0.5);
        assert!(m.starts_with("st(0,clip((T-1)/0.5,0,1));st(0,ld(0)*ld(0)*(3-2*ld(0)))"), "{m}");
        assert!(m.ends_with("255*(1-(lte(hypot(X+0.5-W/2,Y+0.5-H/2),ld(0)*hypot(W/2,H/2))))"), "{m}");
        assert_eq!(mask_size(1080, 1920), (135, 240));
    }

    #[test]
    fn fades_starting_before_the_window_use_per_frame_alpha() {
        assert_eq!(fade_in(2.0, 0.5, 30.0, "fi1"), ",fade=t=in:st=2:d=0.5:alpha=1");
        let a = fade_in(-0.5, 1.0, 10.0, "fi1");
        assert!(a.starts_with(",sendcmd=c='0 colorchannelmixer@fi1 aa 0.5;0.1 colorchannelmixer@fi1 aa 0.6;"), "{a}");
        assert!(a.ends_with(",colorchannelmixer@fi1=aa=0.5"), "{a}");
        let d = dip_alpha(-0.25, 1.0, 4.0, "dp0");
        assert!(d.starts_with(",sendcmd=c='0 colorchannelmixer@dp0 aa 0.5;0.25 colorchannelmixer@dp0 aa 1;0.5 colorchannelmixer@dp0 aa 0.5;"), "{d}");
    }
}
