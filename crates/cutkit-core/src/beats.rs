//! Deteksi ketukan musik tanpa library tambahan: audio didekode ffmpeg ke PCM mono,
//! lalu kurva onset (spectral flux sederhana per pita energi) dicari tempo-nya dengan
//! autokorelasi, dan ketukan dipilih dengan pemrograman dinamis (gaya Ellis 2007).

use serde::Serialize;

pub const SAMPLE_RATE: u32 = 11025;
const HOP: usize = 256; // ~23 ms per frame onset
const WIN: usize = 1024;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct BeatInfo {
    pub bpm: f64,
    /// Waktu ketukan (detik) dari awal audio.
    pub beats: Vec<f64>,
    /// Ketukan kuat (awal birama, asumsi 4/4).
    pub downbeats: Vec<f64>,
}

/// Argumen ffmpeg: dekode `path` (opsional potongan) ke PCM f32 mono 11025 Hz di stdout.
pub fn decode_args(path: &str, start: f64, duration: f64) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-nostats", "-loglevel", "error"].iter().map(|s| s.to_string()).collect();
    if start > 0.0 {
        a.extend(["-ss".into(), format!("{start:.3}")]);
    }
    if duration > 0.0 {
        a.extend(["-t".into(), format!("{duration:.3}")]);
    }
    a.extend(["-i", path, "-vn", "-ac", "1", "-ar", &SAMPLE_RATE.to_string(), "-f", "f32le", "-"].iter().map(|s| s.to_string()));
    a
}

/// Kurva onset: kenaikan energi log per pita frekuensi kasar (4 pita lewat filter sederhana).
pub fn onset_envelope(samples: &[f32]) -> Vec<f32> {
    if samples.len() < WIN {
        return vec![];
    }
    let frames = (samples.len() - WIN) / HOP + 1;
    // Pita: rendah (kick), rendah-menengah, menengah, tinggi (hi-hat) lewat selisih filter low-pass bertingkat.
    let mut bands: Vec<Vec<f32>> = vec![Vec::with_capacity(samples.len()); 4];
    let (mut lp1, mut lp2, mut lp3) = (0f32, 0f32, 0f32);
    for &x in samples {
        lp1 += 0.05 * (x - lp1); // ~90 Hz
        lp2 += 0.15 * (x - lp2); // ~290 Hz
        lp3 += 0.45 * (x - lp3); // ~1.2 kHz
        bands[0].push(lp1);
        bands[1].push(lp2 - lp1);
        bands[2].push(lp3 - lp2);
        bands[3].push(x - lp3);
    }
    let mut prev = [0f32; 4];
    let mut env = Vec::with_capacity(frames);
    for f in 0..frames {
        let s = f * HOP;
        let mut flux = 0f32;
        for (b, band) in bands.iter().enumerate() {
            let e: f32 = band[s..s + WIN].iter().map(|v| v * v).sum::<f32>() / WIN as f32;
            let le = (1.0 + 1000.0 * e).ln();
            if f > 0 {
                flux += (le - prev[b]).max(0.0);
            }
            prev[b] = le;
        }
        env.push(flux);
    }
    // Buang tren lambat (rata-rata bergerak ~0.5 detik) lalu normalisasi.
    let w = 22usize;
    let mut out = vec![0f32; env.len()];
    for i in 0..env.len() {
        let a = i.saturating_sub(w);
        let b = (i + w + 1).min(env.len());
        let mean = env[a..b].iter().sum::<f32>() / (b - a) as f32;
        out[i] = (env[i] - mean).max(0.0);
    }
    let sd = (out.iter().map(|v| v * v).sum::<f32>() / out.len().max(1) as f32).sqrt();
    if sd > 0.0 {
        for v in &mut out {
            *v /= sd;
        }
    }
    out
}

fn frame_rate() -> f64 {
    SAMPLE_RATE as f64 / HOP as f64
}

/// Periode ketukan (dalam frame onset) dari autokorelasi dengan bobot di sekitar 120 BPM.
pub fn estimate_period(env: &[f32], min_bpm: f64, max_bpm: f64) -> Option<f64> {
    let fr = frame_rate();
    let min_lag = (fr * 60.0 / max_bpm).floor() as usize;
    let max_lag = (fr * 60.0 / min_bpm).ceil() as usize;
    if env.len() < max_lag * 3 {
        return None;
    }
    let mut best = (0.0f64, 0usize);
    for lag in min_lag.max(1)..=max_lag {
        let mut ac = 0f64;
        for i in lag..env.len() {
            ac += (env[i] * env[i - lag]) as f64;
        }
        ac /= (env.len() - lag) as f64;
        // Prior log-normal di 120 BPM supaya tidak memilih setengah/dua kali tempo.
        let bpm = fr * 60.0 / lag as f64;
        let prior = (-0.5 * ((bpm / 120.0).log2() / 0.9).powi(2)).exp();
        let score = ac * prior;
        if score > best.0 {
            best = (score, lag);
        }
    }
    if best.1 == 0 {
        return None;
    }
    // Penyempurnaan parabolik di sekitar lag terbaik.
    let lag = best.1;
    let ac = |l: usize| -> f64 { (l..env.len()).map(|i| (env[i] * env[i - l]) as f64).sum::<f64>() / (env.len() - l) as f64 };
    let (a, b, c) = (ac(lag - 1), ac(lag), ac(lag + 1));
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > 1e-12 { (0.5 * (a - c) / denom).clamp(-0.5, 0.5) } else { 0.0 };
    Some(lag as f64 + shift)
}

/// Pemilihan ketukan dengan DP: tiap ketukan dekat puncak onset dan berjarak ~periode dari sebelumnya.
pub fn track_beats(env: &[f32], period: f64, tightness: f64) -> Vec<usize> {
    let n = env.len();
    if n == 0 || period < 2.0 {
        return vec![];
    }
    let mut score = vec![0f64; n];
    let mut back = vec![usize::MAX; n];
    let lo = (period * 0.5).round() as usize;
    let hi = (period * 2.0).round() as usize;
    for i in 0..n {
        let mut best = 0f64;
        let mut arg = usize::MAX;
        if i >= lo {
            for j in i.saturating_sub(hi)..=i - lo {
                let d = (i - j) as f64;
                let pen = -tightness * (d / period).ln().powi(2);
                let v = score[j] + pen;
                if arg == usize::MAX || v > best {
                    best = v;
                    arg = j;
                }
            }
        }
        score[i] = env[i] as f64 + if arg == usize::MAX { 0.0 } else { best.max(0.0) };
        back[i] = if arg != usize::MAX && best > 0.0 { arg } else { usize::MAX };
    }
    // Mulai dari skor tertinggi di periode terakhir.
    let tail = n.saturating_sub(period.ceil() as usize);
    let mut i = (tail..n).max_by(|a, b| score[*a].partial_cmp(&score[*b]).unwrap()).unwrap_or(n - 1);
    let mut beats = vec![i];
    while back[i] != usize::MAX {
        i = back[i];
        beats.push(i);
    }
    beats.reverse();
    beats
}

/// Analisis lengkap dari sampel PCM mono 11025 Hz.
pub fn analyze(samples: &[f32]) -> Option<BeatInfo> {
    let env = onset_envelope(samples);
    let period = estimate_period(&env, 60.0, 200.0)?;
    let frames = track_beats(&env, period, 100.0);
    if frames.len() < 2 {
        return None;
    }
    let fr = frame_rate();
    // Lonjakan energi terjadi saat ketukan baru masuk ke ujung jendela onset, jadi waktu
    // ketukan = awal frame + panjang jendela.
    let offset = WIN as f64 / SAMPLE_RATE as f64;
    let beats: Vec<f64> = frames.iter().map(|&f| ((f as f64 / fr + offset) * 1000.0).round() / 1000.0).collect();
    // Ketukan kuat: fase kelipatan 4 dengan energi onset terbesar.
    let mut phase_energy = [0f64; 4];
    for (k, &f) in frames.iter().enumerate() {
        phase_energy[k % 4] += env[f] as f64;
    }
    let phase = (0..4).max_by(|a, b| phase_energy[*a].partial_cmp(&phase_energy[*b]).unwrap()).unwrap_or(0);
    let downbeats = beats.iter().enumerate().filter(|(k, _)| k % 4 == phase).map(|(_, t)| *t).collect();
    Some(BeatInfo { bpm: ((fr * 60.0 / period) * 10.0).round() / 10.0, beats, downbeats })
}

/// Mendekode audio dengan ffmpeg lalu menganalisis ketukannya.
pub fn detect(ffmpeg: &str, path: &str, start: f64, duration: f64) -> Result<Option<BeatInfo>, String> {
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(decode_args(path, start, duration));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.stdin(std::process::Stdio::null()).output().map_err(|e| format!("ffmpeg gagal dijalankan: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("Audio tidak bisa dibaca.").to_string());
    }
    Ok(analyze(&samples_from_f32le(&out.stdout)))
}

pub fn samples_from_f32le(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Klik sintetis pada `bpm` selama `secs` detik, dengan derau kecil.
    fn clicks(bpm: f64, secs: f64, offset: f64) -> Vec<f32> {
        let n = (secs * SAMPLE_RATE as f64) as usize;
        let mut v = vec![0f32; n];
        let mut seed = 12345u32;
        for (i, x) in v.iter_mut().enumerate() {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            *x = ((seed >> 16) as f32 / 65536.0 - 0.5) * 0.01;
            let _ = i;
        }
        let period = 60.0 / bpm;
        let mut t = offset;
        while t < secs {
            let s = (t * SAMPLE_RATE as f64) as usize;
            for k in 0..400.min(n.saturating_sub(s)) {
                let env = (-(k as f32) / 60.0).exp();
                v[s + k] += env * (2.0 * std::f32::consts::PI * 80.0 * k as f32 / SAMPLE_RATE as f32).sin();
            }
            t += period;
        }
        v
    }

    #[test]
    fn finds_tempo_and_beat_times() {
        for bpm in [90.0, 120.0, 128.0, 150.0] {
            let info = analyze(&clicks(bpm, 20.0, 0.25)).expect("ada ketukan");
            assert!((info.bpm - bpm).abs() < 2.5, "tempo {bpm} terbaca {}", info.bpm);
            let period = 60.0 / bpm;
            // Tiap ketukan dekat kelipatan periode dari 0.25 detik (toleransi 40 ms).
            let close = info.beats.iter().filter(|t| {
                let k = ((*t - 0.25) / period).round();
                ((*t - 0.25) - k * period).abs() < 0.04
            });
            assert!(close.count() * 10 >= info.beats.len() * 9, "ketukan {bpm} tidak pas: {:?}", &info.beats[..6]);
            assert!(info.beats.len() as f64 >= 20.0 / period * 0.85);
            assert!(!info.downbeats.is_empty() && info.downbeats.len() <= info.beats.len() / 4 + 1);
        }
    }

    #[test]
    fn silence_has_no_beats() {
        assert!(analyze(&vec![0f32; SAMPLE_RATE as usize * 5]).is_none());
        assert!(analyze(&[]).is_none());
    }

    #[test]
    fn decode_command() {
        let a = decode_args("m.mp3", 2.0, 10.0).join(" ");
        assert!(a.ends_with("-ss 2.000 -t 10.000 -i m.mp3 -vn -ac 1 -ar 11025 -f f32le -"));
    }
}
