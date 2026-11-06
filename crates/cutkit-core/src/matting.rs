//! Hapus background dengan model segmentasi U²-Net / IS-Net (ONNX, dari rilis rembg)
//! lewat skrip Python kecil (onnxruntime + numpy + pillow) di venv folder model.
//! Gambar -> PNG transparan; video -> frame per frame -> WebM VP9 dengan alpha.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Skrip inferensi; ditulis ke folder model saat dipakai.
pub const SCRIPT: &str = r#"# CutKit: hapus background (U2-Net / IS-Net, ONNX). Pakai:
#   python matting.py image <model.onnx> <masuk> <keluar.png>
#   python matting.py frames <model.onnx> <folder masuk> <folder keluar>
import os, sys
import numpy as np
from PIL import Image
import onnxruntime as ort

def session(model):
    opts = ort.SessionOptions()
    opts.intra_op_num_threads = max(1, (os.cpu_count() or 4) - 1)
    return ort.InferenceSession(model, sess_options=opts, providers=["CPUExecutionProvider"])

def cutout(sess, img, isnet):
    size = 1024 if isnet else 320
    rgb = img.convert("RGB")
    a = np.asarray(rgb.resize((size, size), Image.LANCZOS)).astype(np.float32)
    a = a / max(float(a.max()), 1e-6)
    if isnet:
        mean, std = (0.5, 0.5, 0.5), (1.0, 1.0, 1.0)
    else:
        mean, std = (0.485, 0.456, 0.406), (0.229, 0.224, 0.225)
    a = (a - np.array(mean, np.float32)) / np.array(std, np.float32)
    x = a.transpose(2, 0, 1)[None].astype(np.float32)
    pred = sess.run(None, {sess.get_inputs()[0].name: x})[0][:, 0, :, :][0]
    pred = (pred - pred.min()) / (pred.max() - pred.min() + 1e-8)
    mask = Image.fromarray((pred * 255).astype(np.uint8)).resize(img.size, Image.LANCZOS)
    out = img.convert("RGBA")
    alpha = np.minimum(np.asarray(out.getchannel("A")), np.asarray(mask))
    out.putalpha(Image.fromarray(alpha))
    return out

def main():
    mode, model = sys.argv[1], sys.argv[2]
    isnet = "isnet" in os.path.basename(model)
    sess = session(model)
    if mode == "image":
        cutout(sess, Image.open(sys.argv[3]), isnet).save(sys.argv[4])
        print("PROGRESS 1 1", flush=True)
    else:
        src, dst = sys.argv[3], sys.argv[4]
        os.makedirs(dst, exist_ok=True)
        names = sorted(n for n in os.listdir(src) if n.endswith(".png"))
        for i, n in enumerate(names):
            cutout(sess, Image.open(os.path.join(src, n)), isnet).save(os.path.join(dst, n))
            print(f"PROGRESS {i + 1} {len(names)}", flush=True)

main()
"#;

pub fn python_exe(models_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        models_dir.join("py").join("Scripts").join("python.exe")
    } else {
        models_dir.join("py").join("bin").join("python")
    }
}

/// venv siap bila python ada dan modulnya bisa diimpor.
pub fn python_ready(models_dir: &Path) -> bool {
    let py = python_exe(models_dir);
    py.is_file() && quiet(Command::new(&py).args(["-c", "import onnxruntime, numpy, PIL"])).stdin(Stdio::null()).output().map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(windows)]
fn quiet(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000)
}

#[cfg(not(windows))]
fn quiet(cmd: &mut Command) -> &mut Command {
    cmd
}

fn run_lines(cmd: &mut Command, cancel: &Arc<AtomicBool>, mut on_line: impl FnMut(&str)) -> Result<(), String> {
    let mut child = quiet(cmd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("Gagal menjalankan: {e}"))?;
    let mut stderr = child.stderr.take().expect("stderr");
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut stderr, &mut s);
        s
    });
    for line in BufReader::new(child.stdout.take().expect("stdout")).lines() {
        let Ok(line) = line else { break };
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            return Err("Dibatalkan.".into());
        }
        on_line(&line);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err.join().unwrap_or_default();
    if cancel.load(Ordering::Relaxed) {
        return Err("Dibatalkan.".into());
    }
    if !status.success() {
        return Err(err.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n"));
    }
    Ok(())
}

/// Membuat venv di folder model dan memasang onnxruntime, numpy, pillow (butuh Python 3 terpasang).
pub fn setup_python(models_dir: &Path, cancel: Arc<AtomicBool>, mut on_line: impl FnMut(String)) -> Result<(), String> {
    std::fs::create_dir_all(models_dir).map_err(|e| e.to_string())?;
    let venv = models_dir.join("py");
    if !python_exe(models_dir).is_file() {
        on_line("Membuat lingkungan Python…".into());
        // Coba launcher py dulu (Windows), lalu python.
        let mut made = false;
        for (exe, pre) in [("py", vec!["-3"]), ("python", vec![]), ("python3", vec![])] {
            let mut c = Command::new(exe);
            c.args(&pre).arg("-m").arg("venv").arg(&venv);
            if run_lines(&mut c, &cancel, |l| on_line(l.to_string())).is_ok() && python_exe(models_dir).is_file() {
                made = true;
                break;
            }
        }
        if !made {
            return Err("Python 3 tidak ditemukan. Pasang dari python.org (centang \"Add to PATH\"), lalu coba lagi.".into());
        }
    }
    on_line("Memasang onnxruntime, numpy, pillow (sekitar 60 MB)…".into());
    let mut c = Command::new(python_exe(models_dir));
    c.args(["-m", "pip", "install", "--disable-pip-version-check", "--progress-bar", "off", "onnxruntime", "numpy", "pillow"]);
    run_lines(&mut c, &cancel, |l| on_line(l.to_string()))?;
    if !python_ready(models_dir) {
        return Err("Modul Python terpasang tetapi tidak bisa diimpor.".into());
    }
    Ok(())
}

fn write_script(models_dir: &Path) -> Result<PathBuf, String> {
    let p = models_dir.join("cutkit-matting.py");
    std::fs::write(&p, SCRIPT).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Gambar -> PNG transparan.
pub fn remove_bg_image(models_dir: &Path, model: &Path, input: &str, output: &Path) -> Result<(), String> {
    let script = write_script(models_dir)?;
    if let Some(d) = output.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let mut c = Command::new(python_exe(models_dir));
    c.arg(&script).arg("image").arg(model).arg(input).arg(output);
    run_lines(&mut c, &Arc::new(AtomicBool::new(false)), |_| {})
}

/// Argumen ffmpeg untuk menyusun frame RGBA jadi WebM VP9 beralpha, dengan audio dari sumber.
pub fn encode_alpha_args(frames_pattern: &str, fps: f64, source: &str, start: f64, duration: f64, has_audio: bool, out: &str) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-y", "-loglevel", "error", "-framerate", &format!("{fps}"), "-i", frames_pattern].iter().map(|s| s.to_string()).collect();
    if has_audio {
        a.extend(["-ss".into(), format!("{start:.3}"), "-t".into(), format!("{duration:.3}"), "-i".into(), source.to_string(), "-map".into(), "0:v".into(), "-map".into(), "1:a?".into(), "-c:a".into(), "libopus".into(), "-b:a".into(), "160k".into()]);
    }
    a.extend(["-c:v", "libvpx-vp9", "-pix_fmt", "yuva420p", "-crf", "30", "-b:v", "0", "-auto-alt-ref", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "4", "-shortest", out].iter().map(|s| s.to_string()));
    a
}

/// Video -> WebM VP9 beralpha untuk bagian [start, start+duration] sumber. Progres = (frame selesai, total).
#[allow(clippy::too_many_arguments)]
pub fn remove_bg_video(ffmpeg: &str, models_dir: &Path, model: &Path, input: &str, start: f64, duration: f64, fps: f64, has_audio: bool, work: &Path, output: &Path, cancel: Arc<AtomicBool>, mut on_progress: impl FnMut(usize, usize)) -> Result<(), String> {
    let script = write_script(models_dir)?;
    let (src_dir, dst_dir) = (work.join("asli"), work.join("hasil"));
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(&src_dir).map_err(|e| e.to_string())?;
    // Frame dikecilkan bila lebih dari 1080 px agar cepat; model bekerja di 320/1024 px.
    crate::ffmpeg::run(
        ffmpeg,
        &[
            "-hide_banner", "-y", "-loglevel", "error", "-ss", &format!("{start:.3}"), "-t", &format!("{duration:.3}"), "-i", input,
            "-vf", &format!("fps={fps},scale='if(gt(iw,ih),min(1920,iw),-2)':'if(gt(iw,ih),-2,min(1920,ih))'"), "-an",
            &src_dir.join("%05d.png").to_string_lossy(),
        ]
        .map(String::from),
    )?;
    let mut c = Command::new(python_exe(models_dir));
    c.arg(&script).arg("frames").arg(model).arg(&src_dir).arg(&dst_dir);
    run_lines(&mut c, &cancel, |l| {
        if let Some(rest) = l.strip_prefix("PROGRESS ") {
            let mut it = rest.split_whitespace().filter_map(|x| x.parse::<usize>().ok());
            if let (Some(i), Some(n)) = (it.next(), it.next()) {
                on_progress(i, n);
            }
        }
    })?;
    if let Some(d) = output.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::ffmpeg::run(ffmpeg, &encode_alpha_args(&dst_dir.join("%05d.png").to_string_lossy(), fps, input, start, duration, has_audio, &output.to_string_lossy()))?;
    let _ = std::fs::remove_dir_all(work);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_args_keep_alpha_and_audio() {
        let a = encode_alpha_args("w/%05d.png", 30.0, "D:/v.mp4", 2.0, 3.0, true, "o.webm").join(" ");
        assert!(a.contains("-framerate 30 -i w/%05d.png -ss 2.000 -t 3.000 -i D:/v.mp4 -map 0:v -map 1:a? -c:a libopus"));
        assert!(a.contains("-c:v libvpx-vp9 -pix_fmt yuva420p"));
        assert!(!encode_alpha_args("w/%05d.png", 30.0, "x", 0.0, 1.0, false, "o.webm").join(" ").contains("libopus"));
    }

    #[test]
    fn script_has_both_modes() {
        assert!(SCRIPT.contains("mode == \"image\"") && SCRIPT.contains("PROGRESS"));
    }
}
