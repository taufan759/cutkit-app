//! Bank aset CutKit: SFX, musik latar, latar diam/bergerak, dan overlay yang dibuat sendiri dari
//! rumus ffmpeg di komputer pengguna. Tidak ada file pihak ketiga, jadi bebas dipakai (CC0) dan
//! installer tetap kecil. File dibuat sekali ke `<data>/bank/<kategori>/` saat pertama dipakai.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct BankItem {
    pub category: &'static str,
    pub file: &'static str,
    pub label: &'static str,
}

pub const CATEGORIES: &[&str] = &["SFX", "Musik", "Latar diam", "Latar bergerak", "Overlay"];

const fn it(category: &'static str, file: &'static str, label: &'static str) -> BankItem {
    BankItem { category, file, label }
}

pub const ITEMS: &[BankItem] = &[
    it("SFX", "whoosh-cepat.wav", "Whoosh cepat"),
    it("SFX", "whoosh-panjang.wav", "Whoosh panjang"),
    it("SFX", "pop.wav", "Pop"),
    it("SFX", "klik.wav", "Klik"),
    it("SFX", "ding.wav", "Ding"),
    it("SFX", "boom.wav", "Boom"),
    it("SFX", "riser.wav", "Riser"),
    it("SFX", "notifikasi.wav", "Notifikasi"),
    it("SFX", "sukses.wav", "Sukses"),
    it("SFX", "salah.wav", "Salah"),
    it("SFX", "ketik.wav", "Mengetik"),
    it("SFX", "kamera.wav", "Jepret kamera"),
    it("SFX", "detak-jantung.wav", "Detak jantung"),
    it("SFX", "swish-naik.wav", "Swish naik"),
    it("SFX", "swish-turun.wav", "Swish turun"),
    it("SFX", "koin.wav", "Koin"),
    it("Musik", "santai.wav", "Santai (lofi, 80 bpm)"),
    it("Musik", "semangat.wav", "Semangat (124 bpm)"),
    it("Musik", "tegang.wav", "Tegang"),
    it("Musik", "ceria.wav", "Ceria (120 bpm)"),
    it("Latar diam", "gradasi-senja.png", "Gradasi senja"),
    it("Latar diam", "gradasi-laut.png", "Gradasi laut"),
    it("Latar diam", "gradasi-ungu.png", "Gradasi ungu"),
    it("Latar diam", "gradasi-hijau.png", "Gradasi hijau"),
    it("Latar diam", "gelap-halus.png", "Gelap halus"),
    it("Latar diam", "kotak-kotak.png", "Garis kotak"),
    it("Latar diam", "titik-titik.png", "Titik-titik"),
    it("Latar bergerak", "gradasi-bergerak.mp4", "Gradasi bergerak"),
    it("Latar bergerak", "aurora.mp4", "Aurora"),
    it("Latar bergerak", "partikel.mp4", "Partikel"),
    it("Latar bergerak", "fraktal.mp4", "Fraktal"),
    it("Latar bergerak", "garis-neon.mp4", "Garis neon"),
    it("Overlay", "light-leak.webm", "Light leak (transparan)"),
    it("Overlay", "film-grain.webm", "Film grain (transparan)"),
    it("Overlay", "vignette.png", "Vignette"),
    it("Overlay", "bingkai-putih.png", "Bingkai putih"),
    it("Overlay", "letterbox.png", "Letterbox sinematik"),
];

pub fn bank_dir() -> PathBuf {
    crate::paths::data_dir().join("bank")
}

pub fn path_of(dir: &Path, item: &BankItem) -> PathBuf {
    dir.join(item.category).join(item.file)
}

/// Nada yang dimainkan mulai detik `at`: sinus `f` Hz dengan peluruhan eksponensial.
fn note(f: f64, at: f64, amp: f64, decay: f64) -> String {
    format!("{amp}*sin(2*PI*{f}*(t-{at}))*gte(t,{at})*exp(-{decay}*(t-{at}))")
}

fn sfx(expr: &str, d: f64, af: Option<&str>) -> Vec<String> {
    let mut a = vec!["-f".to_string(), "lavfi".into(), "-i".into(), format!("aevalsrc='{expr}':s=48000:d={d}")];
    let fade = format!("afade=t=out:st={}:d=0.03", (d - 0.03).max(0.0));
    a.extend(["-af".into(), af.map(|f| format!("{f},{fade}")).unwrap_or(fade), "-ac".into(), "2".into()]);
    a
}

fn music(expr: &str, af: &str) -> Vec<String> {
    vec![
        "-f".into(), "lavfi".into(), "-i".into(), format!("aevalsrc='{expr}':s=44100:d=30"),
        "-af".into(), format!("{af},afade=t=in:d=0.5,afade=t=out:st=28.5:d=1.5,loudnorm=I=-18:TP=-2"),
        "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
    ]
}

fn image(src: &str) -> Vec<String> {
    vec!["-f".into(), "lavfi".into(), "-i".into(), src.into(), "-frames:v".into(), "1".into(), "-update".into(), "1".into()]
}

fn loop_mp4(src: &str) -> Vec<String> {
    vec![
        "-f".into(), "lavfi".into(), "-i".into(), src.into(), "-t".into(), "6".into(),
        "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-crf".into(), "20".into(), "-pix_fmt".into(), "yuv420p".into(), "-movflags".into(), "+faststart".into(),
    ]
}

fn alpha_webm(src: &str) -> Vec<String> {
    vec![
        "-f".into(), "lavfi".into(), "-i".into(), src.into(), "-t".into(), "5".into(),
        "-c:v".into(), "libvpx-vp9".into(), "-pix_fmt".into(), "yuva420p".into(), "-crf".into(), "34".into(), "-b:v".into(), "0".into(),
        "-auto-alt-ref".into(), "0".into(), "-row-mt".into(), "1".into(), "-deadline".into(), "realtime".into(), "-cpu-used".into(), "8".into(),
    ]
}

/// Argumen ffmpeg (tanpa file keluaran) untuk satu item bank.
pub fn recipe(file: &str) -> Vec<String> {
    let kick = |beat: f64| format!("0.8*sin(2*PI*(45+90*exp(-30*mod(t,{beat})))*mod(t,{beat}))*exp(-9*mod(t,{beat}))");
    match file {
        "whoosh-cepat.wav" => sfx("(random(0)*2-1)*0.8*pow(sin(PI*t/0.45),2)", 0.45, Some("highpass=f=300,lowpass=f=2600,volume=1.8")),
        "whoosh-panjang.wav" => sfx("(random(0)*2-1)*0.8*pow(sin(PI*t),2)", 1.0, Some("highpass=f=200,lowpass=f=1800,volume=1.8")),
        "pop.wav" => sfx("0.7*sin(2*PI*(520*t-900*t*t))*exp(-16*t)", 0.3, None),
        "klik.wav" => sfx("0.8*sin(2*PI*2200*t)*exp(-90*t)", 0.08, None),
        "ding.wav" => sfx(&format!("{}+{}+{}", note(1318.5, 0.0, 0.4, 3.5), note(2637.0, 0.0, 0.18, 5.0), note(3955.5, 0.0, 0.08, 7.0)), 1.6, None),
        "boom.wav" => sfx("0.9*sin(2*PI*(70*t-25*t*t))*exp(-4*t)+0.35*(random(0)*2-1)*exp(-25*t)", 1.4, Some("lowpass=f=900")),
        "riser.wav" => sfx("(0.25*(random(0)*2-1)+0.25*sin(2*PI*(150*t+220*t*t)))*pow(t/2.5,1.5)", 2.5, Some("highpass=f=150")),
        "notifikasi.wav" => sfx(&format!("{}+{}", note(880.0, 0.0, 0.45, 12.0), note(1318.5, 0.13, 0.45, 10.0)), 0.6, None),
        "sukses.wav" => sfx(&format!("{}+{}+{}", note(523.25, 0.0, 0.35, 6.0), note(659.25, 0.12, 0.35, 6.0), note(783.99, 0.24, 0.4, 4.0)), 1.3, None),
        "salah.wav" => sfx("0.28*(2*gt(sin(2*PI*110*t),0)-1)*(lt(t,0.18)+gte(t,0.24)*lt(t,0.42))", 0.5, Some("lowpass=f=1500")),
        "ketik.wav" => sfx("0.5*sin(2*PI*1900*t)*exp(-180*mod(t,0.09))*gt(sin(2*PI*1.7*t)+sin(2*PI*2.9*t),-0.6)", 1.5, Some("highpass=f=800")),
        "kamera.wav" => sfx("0.7*(random(0)*2-1)*(exp(-60*t)+0.8*gte(t,0.09)*exp(-50*(t-0.09)))", 0.3, Some("highpass=f=1500")),
        "detak-jantung.wav" => sfx(
            "0.9*sin(2*PI*55*mod(t,0.9))*exp(-22*mod(t,0.9))+0.7*sin(2*PI*50*mod(t-0.22,0.9))*exp(-22*mod(t-0.22,0.9))*gte(t,0.22)",
            1.8,
            Some("lowpass=f=300,volume=1.5"),
        ),
        "swish-naik.wav" => sfx("0.4*sin(2*PI*(300*t+900*t*t))*sin(PI*t/0.5)", 0.5, None),
        "swish-turun.wav" => sfx("0.4*sin(2*PI*(1200*t-900*t*t))*sin(PI*t/0.5)", 0.5, None),
        "koin.wav" => sfx(&format!("{}+{}+{}+{}", note(1975.5, 0.0, 0.3, 9.0), note(2637.0, 0.0, 0.25, 9.0), note(1975.5, 0.08, 0.3, 7.0), note(2637.0, 0.08, 0.25, 7.0)), 0.9, None),
        // Musik: akor, bas, dan drum dari rumus; 30 detik, berulang mulus di ketukan.
        "santai.wav" => music(
            &format!(
                "0.05*(sin(2*PI*if(lt(mod(t,6),3),261.6,220)*t)+sin(2*PI*if(lt(mod(t,6),3),329.6,261.6)*t)+sin(2*PI*if(lt(mod(t,6),3),392,329.6)*t)+sin(2*PI*if(lt(mod(t,6),3),493.9,392)*t))*(0.8+0.2*sin(2*PI*0.5*t))+0.6*{}+0.03*(random(0)*2-1)*exp(-45*mod(t+0.375,0.75))",
                kick(0.75)
            ),
            "lowpass=f=5000,aecho=0.6:0.5:180:0.25",
        ),
        "semangat.wav" => music(
            &format!(
                "{}+0.18*sin(2*PI*if(lt(mod(t,7.742),3.871),55,49)*t)*gte(mod(t,0.4839),0.2419)*exp(-6*mod(t-0.2419,0.4839))+0.04*(random(0)*2-1)*exp(-60*mod(t+0.121,0.242))+0.05*sin(2*PI*if(lt(mod(t,7.742),3.871),440,392)*t)*(0.5+0.5*sin(2*PI*2.067*t))",
                kick(0.4839)
            ),
            "highpass=f=30",
        ),
        "tegang.wav" => music(
            "0.18*sin(2*PI*55*t)*(0.6+0.4*sin(2*PI*0.1*t))+0.1*sin(2*PI*82.4*t)*(0.5+0.5*sin(2*PI*0.07*t))+0.06*sin(2*PI*116.5*t)*(0.5+0.5*sin(2*PI*0.13*t))+0.25*sin(2*PI*900*mod(t,0.5))*exp(-80*mod(t,0.5))",
            "lowpass=f=3000,aecho=0.7:0.6:400:0.3",
        ),
        "ceria.wav" => music(
            &format!(
                "0.22*sin(2*PI*if(lt(mod(floor(t*8),8),1),523.3,if(lt(mod(floor(t*8),8),2),659.3,if(lt(mod(floor(t*8),8),3),784,if(lt(mod(floor(t*8),8),4),1046.5,if(lt(mod(floor(t*8),8),5),784,if(lt(mod(floor(t*8),8),6),659.3,if(lt(mod(floor(t*8),8),7),587.3,659.3)))))))*t)*exp(-10*mod(t,0.125))+0.12*sin(2*PI*if(lt(mod(t,4),2),130.8,98)*t)+0.5*{}",
                kick(0.5)
            ),
            "aecho=0.5:0.4:250:0.2",
        ),
        "gradasi-senja.png" => image("gradients=s=1080x1920:c0=0x1f3b73:c1=0xff6b3d:x0=0:y0=0:x1=1080:y1=1920:nb_colors=2:seed=1:d=1"),
        "gradasi-laut.png" => image("gradients=s=1080x1920:c0=0x032b44:c1=0x2ec4b6:x0=0:y0=0:x1=1080:y1=1920:nb_colors=2:seed=2:d=1"),
        "gradasi-ungu.png" => image("gradients=s=1080x1920:c0=0x2b1055:c1=0xd16ba5:x0=0:y0=0:x1=1080:y1=1920:nb_colors=2:seed=3:d=1"),
        "gradasi-hijau.png" => image("gradients=s=1080x1920:c0=0x0b3d2e:c1=0xc6f432:x0=0:y0=1920:x1=1080:y1=0:nb_colors=2:seed=4:d=1"),
        "gelap-halus.png" => image("color=c=0x15151b:s=1080x1920:d=1,noise=alls=8:allf=u,vignette=PI/4"),
        "kotak-kotak.png" => image("color=c=0x111118:s=1080x1920:d=1,format=rgb24,geq=r='if(lt(mod(X,90),2)+lt(mod(Y,90),2),60,17)':g='if(lt(mod(X,90),2)+lt(mod(Y,90),2),70,17)':b='if(lt(mod(X,90),2)+lt(mod(Y,90),2),90,24)'"),
        "titik-titik.png" => image("color=c=0xf4f1ea:s=1080x1920:d=1,format=rgb24,geq=r='if(lt(hypot(mod(X,48)-24,mod(Y,48)-24),5),210,244)':g='if(lt(hypot(mod(X,48)-24,mod(Y,48)-24),5),204,241)':b='if(lt(hypot(mod(X,48)-24,mod(Y,48)-24),5),196,234)'"),
        "gradasi-bergerak.mp4" => loop_mp4("gradients=s=1080x1920:c0=0x2b1055:c1=0x35c9ff:c2=0xff6b3d:nb_colors=3:speed=0.02:seed=7:r=30"),
        "aurora.mp4" => loop_mp4("color=c=black:s=270x480:r=30,format=rgb24,geq=r='25+35*sin(Y/55+X/160+T*0.9)':g='70+80*sin(Y/40-X/130+T*0.7)':b='110+90*sin(Y/75+X/200-T*0.5)',scale=540:960:flags=bicubic,gblur=sigma=3"),
        "partikel.mp4" => loop_mp4("life=s=360x640:mold=10:r=30:ratio=0.1:death_color=#0b1a2a:life_color=#35c9ff,scale=1080:1920:flags=neighbor"),
        "fraktal.mp4" => loop_mp4("mandelbrot=s=540x960:r=30"),
        "garis-neon.mp4" => loop_mp4("color=c=black:s=270x480:r=30,format=rgb24,geq=r='255*pow(max(0,sin((X+Y)/18-T*4)),24)':g='60*pow(max(0,sin((X+Y)/18-T*4)),24)':b='255*pow(max(0,sin((X-Y)/22+T*3)),24)',scale=540:960:flags=bicubic,gblur=sigma=1"),
        "light-leak.webm" => alpha_webm("color=c=0xff9a3c:s=270x480:r=30,format=rgba,geq=r=255:g='150+60*sin(T)':b=70:a='210*exp(-(pow(X-135-110*sin(T*0.8),2)+pow(Y-200-160*cos(T*0.6),2))/9000)',scale=540:960:flags=bicubic"),
        "film-grain.webm" => alpha_webm("color=c=0x808080:s=540x960:r=24,noise=alls=70:allf=t,format=rgba,colorchannelmixer=aa=0.13"),
        "vignette.png" => image("color=c=black:s=1080x1920:d=1,format=rgba,geq=r=0:g=0:b=0:a='255*min(1,max(0,(hypot(X-W/2,Y-H/2)/hypot(W/2,H/2)-0.42)*1.7))'"),
        "bingkai-putih.png" => image("color=c=white:s=1080x1920:d=1,format=rgba,geq=r=255:g=255:b=255:a='255*(lt(X,36)+gt(X,W-37)+lt(Y,36)+gt(Y,H-37))'"),
        "letterbox.png" => image("color=c=black:s=1080x1920:d=1,format=rgba,geq=r=0:g=0:b=0:a='255*(lt(Y,H*0.12)+gt(Y,H*0.88))'"),
        _ => vec![],
    }
}

/// Membuat item yang belum ada. Progres: (selesai, total, label). Mengembalikan jumlah file baru.
pub fn prepare(ffmpeg: &str, dir: &Path, mut on_progress: impl FnMut(usize, usize, &str)) -> Result<usize, String> {
    let todo: Vec<&BankItem> = ITEMS.iter().filter(|i| !path_of(dir, i).is_file()).collect();
    let total = todo.len();
    for (n, item) in todo.iter().enumerate() {
        on_progress(n, total, item.label);
        let out = path_of(dir, item);
        std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
        let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("bin");
        let part = out.with_extension(format!("part.{ext}"));
        let mut args: Vec<String> = ["-hide_banner", "-y", "-loglevel", "error"].iter().map(|s| s.to_string()).collect();
        args.extend(recipe(item.file));
        args.push(part.to_string_lossy().to_string());
        crate::ffmpeg::run(ffmpeg, &args).map_err(|e| format!("{}: {e}", item.label))?;
        std::fs::rename(&part, &out).map_err(|e| e.to_string())?;
    }
    let _ = std::fs::write(
        dir.join("LISENSI.txt"),
        "Bank aset CutKit dibuat dari rumus ffmpeg di komputermu (tanpa file pihak ketiga).\nBebas dipakai, diubah, dan dibagikan, termasuk untuk konten yang dimonetisasi (CC0 1.0).\n",
    );
    on_progress(total, total, "Selesai");
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_has_a_recipe_and_known_category() {
        for i in ITEMS {
            assert!(!recipe(i.file).is_empty(), "{} tanpa resep", i.file);
            assert!(CATEGORIES.contains(&i.category), "{}", i.category);
        }
        let mut files: Vec<_> = ITEMS.iter().map(|i| i.file).collect();
        files.sort();
        files.dedup();
        assert_eq!(files.len(), ITEMS.len(), "nama file unik");
    }
}
