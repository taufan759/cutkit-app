//! Pembuat file ASS (Advanced SubStation Alpha) untuk caption kata-per-kata dan
//! klip teks. Dirender oleh libass lewat filter `ass` ffmpeg saat ekspor.
//! Logika pembagian halaman/baris harus sama dengan `ui/captions.js` agar
//! preview dan hasil ekspor cocok.

use crate::model::{CaptionStyle, Clip, ClipKind, Project, TextStyle, Word, POP_FACTOR, POP_MS};

/// Jeda antar kata lebih dari ini memulai halaman caption baru.
pub const PAGE_GAP: f64 = 1.5;

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// Indeks kata (ke dalam daftar kata yang sudah diurutkan), per baris.
    pub lines: Vec<Vec<usize>>,
}

impl Page {
    pub fn words(&self) -> impl Iterator<Item = usize> + '_ {
        self.lines.iter().flatten().copied()
    }
}

/// Mengurutkan kata berdasarkan waktu mulai dan membuang yang kosong.
pub fn sorted_words(words: &[Word]) -> Vec<Word> {
    let mut v: Vec<Word> = words.iter().filter(|w| !w.text.trim().is_empty() && w.end > w.start).cloned().collect();
    v.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    v
}

/// Membagi kata menjadi halaman (maks `words_per_page` kata, atau jeda > PAGE_GAP)
/// dan tiap halaman menjadi baris (maks `max_line_chars` karakter per baris).
pub fn paginate(words: &[Word], words_per_page: usize, max_line_chars: usize) -> Vec<Page> {
    let per_page = words_per_page.max(1);
    let max_chars = max_line_chars.max(1);
    let mut pages: Vec<Vec<usize>> = vec![];
    let mut current: Vec<usize> = vec![];
    for (i, w) in words.iter().enumerate() {
        let gap_break = current.last().map(|&p| w.start - words[p].end > PAGE_GAP).unwrap_or(false);
        if current.len() >= per_page || gap_break {
            pages.push(std::mem::take(&mut current));
        }
        current.push(i);
    }
    if !current.is_empty() {
        pages.push(current);
    }
    pages
        .into_iter()
        .map(|idx| {
            let mut lines: Vec<Vec<usize>> = vec![];
            let mut line: Vec<usize> = vec![];
            let mut len = 0usize;
            for i in idx {
                let wl = words[i].text.chars().count();
                if !line.is_empty() && len + 1 + wl > max_chars {
                    lines.push(std::mem::take(&mut line));
                    len = 0;
                }
                len += if line.is_empty() { wl } else { wl + 1 };
                line.push(i);
            }
            if !line.is_empty() {
                lines.push(line);
            }
            Page { lines }
        })
        .collect()
}

/// Satu kejadian tampil: halaman `page` dengan kata `active` disorot, pada rentang waktu.
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub page: usize,
    pub active: usize,
    pub start: f64,
    pub end: f64,
}

/// Tiap kata jadi satu cue: tampil sejak kata mulai sampai kata berikutnya di
/// halaman yang sama mulai (atau kata itu berakhir bila terakhir).
pub fn cues(words: &[Word], pages: &[Page]) -> Vec<Cue> {
    let mut out = vec![];
    for (pi, page) in pages.iter().enumerate() {
        let idx: Vec<usize> = page.words().collect();
        for (k, &wi) in idx.iter().enumerate() {
            let start = words[wi].start;
            let mut end = idx.get(k + 1).map(|&n| words[n].start).unwrap_or(words[wi].end);
            if end <= start {
                end = start + 0.05;
            }
            out.push(Cue { page: pi, active: wi, start, end });
        }
    }
    out
}

/// `#rrggbb` / `#rrggbbaa` -> (r, g, b, a) dengan a 0..255 (255 = pekat).
pub fn parse_color(css: &str) -> (u8, u8, u8, u8) {
    let hex = css.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    match hex.len() {
        6 => (byte(0), byte(2), byte(4), 255),
        8 => (byte(0), byte(2), byte(4), byte(6)),
        3 => {
            let n = |i: usize| u8::from_str_radix(&hex[i..i + 1].repeat(2), 16).unwrap_or(0);
            (n(0), n(1), n(2), 255)
        }
        _ => (255, 255, 255, 255),
    }
}

/// Warna gaya ASS: `&HAABBGGRR` dengan AA = 0 berarti pekat.
pub fn ass_color(css: &str) -> String {
    let (r, g, b, a) = parse_color(css);
    format!("&H{:02X}{:02X}{:02X}{:02X}", 255 - a, b, g, r)
}

/// Warna untuk tag sebaris: `&HBBGGRR&`.
pub fn ass_inline_color(css: &str) -> String {
    let (r, g, b, _) = parse_color(css);
    format!("&H{:02X}{:02X}{:02X}&", b, g, r)
}

pub fn ass_time(secs: f64) -> String {
    let cs = (secs.max(0.0) * 100.0).round() as u64;
    let h = cs / 360_000;
    let m = (cs / 6_000) % 60;
    let s = (cs / 100) % 60;
    format!("{h}:{m:02}:{s:02}.{:02}", cs % 100)
}

/// Kurung kurawal adalah tag ASS, jadi diganti; baris baru jadi `\N`.
pub fn escape_text(text: &str) -> String {
    text.trim().replace('{', "(").replace('}', ")").replace("\r\n", "\n").replace('\n', "\\N")
}

fn bold_flag(b: bool) -> i32 {
    if b {
        -1
    } else {
        0
    }
}

fn style_line(name: &str, font: &str, size: f64, color: &str, outline_color: &str, background: &str, bold: bool, italic: bool, outline: f64, shadow: f64) -> String {
    let boxed = !background.trim().is_empty();
    let back = if boxed { ass_color(background) } else { "&H80000000".to_string() };
    format!(
        "Style: {name},{font},{size},{primary},&H000000FF,{outline_color},{back},{bold},{italic},0,0,100,100,0,0,{border},{outline},{shadow},5,0,0,0,1",
        size = fmt(size),
        primary = ass_color(color),
        outline_color = ass_color(outline_color),
        bold = bold_flag(bold),
        italic = bold_flag(italic),
        border = if boxed { 3 } else { 1 },
        outline = fmt(outline),
        shadow = fmt(shadow),
    )
}

fn fmt(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{:.2}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Alpha tag per komponen agar opacity klip dikalikan dengan alpha warna asal.
fn alpha_tags(style: &TextStyle, opacity: f64) -> String {
    if opacity >= 0.999 {
        return String::new();
    }
    let a = |css: &str| {
        let (_, _, _, a) = parse_color(css);
        let eff = (a as f64 / 255.0) * opacity.clamp(0.0, 1.0);
        format!("&H{:02X}&", 255 - (eff * 255.0).round() as u8)
    };
    let back = if style.background.trim().is_empty() { "#00000080".to_string() } else { style.background.clone() };
    format!("\\1a{}\\3a{}\\4a{}", a(&style.color), a(&style.outline_color), a(&back))
}

/// Membuat seluruh file ASS untuk proyek: caption (layer 0) dan klip teks (layer 1+).
/// Mengembalikan None bila tidak ada apa pun yang perlu dirender.
pub fn build_ass(project: &Project) -> Option<String> {
    let w = project.canvas.width;
    let h = project.canvas.height;
    let mut styles = vec![];
    let mut events = vec![];

    if project.captions.enabled {
        let st: &CaptionStyle = &project.captions.style;
        let words = sorted_words(&project.captions.words);
        if !words.is_empty() {
            styles.push(style_line("Cap", &st.font, st.size, &st.color, &st.outline_color, &st.background, st.bold, false, st.outline, st.shadow));
            let pages = paginate(&words, st.words_per_page, st.max_line_chars);
            let cx = w as f64 / 2.0 + st.x;
            let cy = h as f64 * st.y;
            let scale = (st.highlight_scale * 100.0).round() as i64;
            let pop_from = (st.highlight_scale * POP_FACTOR * 100.0).round() as i64;
            let pop = st.animation == "pop";
            let hl = ass_inline_color(&st.highlight_color);
            for cue in cues(&words, &pages) {
                let page = &pages[cue.page];
                let text = page
                    .lines
                    .iter()
                    .map(|line| {
                        line.iter()
                            .map(|&i| {
                                let mut t = escape_text(&words[i].text);
                                if st.uppercase {
                                    t = t.to_uppercase();
                                }
                                if i == cue.active && pop {
                                    format!("{{\\c{hl}\\fscx{pop_from}\\fscy{pop_from}\\t(0,{POP_MS},\\fscx{scale}\\fscy{scale})}}{t}{{\\r}}")
                                } else if i == cue.active {
                                    format!("{{\\c{hl}\\fscx{scale}\\fscy{scale}}}{t}{{\\r}}")
                                } else {
                                    t
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect::<Vec<_>>()
                    .join("\\N");
                events.push(format!(
                    "Dialogue: 0,{},{},Cap,,0,0,0,,{{\\an5\\pos({},{})}}{text}",
                    ass_time(cue.start),
                    ass_time(cue.end),
                    fmt(cx),
                    fmt(cy)
                ));
            }
        }
    }

    let mut n = 0;
    for (ti, track) in project.tracks.iter().enumerate() {
        if track.hidden {
            continue;
        }
        for clip in track.clips.iter().filter(|c| c.kind == ClipKind::Text) {
            let Some(tc) = &clip.text else { continue };
            if tc.text.trim().is_empty() || clip.duration <= 0.0 {
                continue;
            }
            n += 1;
            let name = format!("T{n}");
            let s = &tc.style;
            styles.push(style_line(&name, &s.font, s.size, &s.color, &s.outline_color, &s.background, s.bold, s.italic, s.outline, s.shadow));
            events.push(text_event(&name, ti + 1, clip, &tc.style, &escape_text(&tc.text), w, h));
        }
    }

    if events.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("[Script Info]\nScriptType: v4.00+\n");
    out.push_str(&format!("PlayResX: {w}\nPlayResY: {h}\n"));
    out.push_str("WrapStyle: 2\nScaledBorderAndShadow: yes\nYCbCr Matrix: None\n\n");
    out.push_str("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    for s in styles {
        out.push_str(&s);
        out.push('\n');
    }
    out.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for e in events {
        out.push_str(&e);
        out.push('\n');
    }
    Some(out)
}

fn text_event(style: &str, layer: usize, clip: &Clip, ts: &TextStyle, text: &str, w: u32, h: u32) -> String {
    let t = clip.transform;
    let cx = w as f64 / 2.0 + t.x;
    let cy = h as f64 / 2.0 + t.y;
    let mut tags = format!("\\an5\\pos({},{})", fmt(cx), fmt(cy));
    if t.rotation.abs() > 1e-6 {
        // ASS memutar berlawanan jarum jam; model memakai searah jarum jam.
        tags.push_str(&format!("\\frz{}", fmt(-t.rotation)));
    }
    if (t.scale - 1.0).abs() > 1e-6 {
        let s = fmt(t.scale * 100.0);
        // \fscx tidak memperbesar garis tepi/bayangan di libass; diskalakan manual agar sama dengan preview.
        tags.push_str(&format!("\\fscx{s}\\fscy{s}\\bord{}\\shad{}", fmt(ts.outline * t.scale), fmt(ts.shadow * t.scale)));
    }
    tags.push_str(&alpha_tags(ts, t.opacity));
    format!("Dialogue: {layer},{},{},{style},,0,0,0,,{{{tags}}}{text}", ass_time(clip.start), ass_time(clip.end()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn w(text: &str, start: f64, end: f64) -> Word {
        Word { text: text.into(), start, end }
    }

    #[test]
    fn colors_and_time() {
        assert_eq!(ass_color("#ff8000"), "&H000080FF");
        assert_eq!(ass_color("#ff800080"), "&H7F0080FF");
        assert_eq!(ass_inline_color("#ffe600"), "&H00E6FF&");
        assert_eq!(parse_color("#abc"), (0xaa, 0xbb, 0xcc, 255));
        assert_eq!(ass_time(0.0), "0:00:00.00");
        assert_eq!(ass_time(3661.257), "1:01:01.26");
        assert_eq!(escape_text(" a{b}\nc "), "a(b)\\Nc");
    }

    #[test]
    fn pagination_by_count_gap_and_chars() {
        let words = vec![w("satu", 0.0, 0.3), w("dua", 0.3, 0.6), w("tiga", 0.6, 0.9), w("empat", 0.9, 1.2), w("lima", 5.0, 5.3)];
        let pages = paginate(&words, 3, 8);
        assert_eq!(pages.len(), 3, "3 kata per halaman, lalu jeda panjang memulai halaman baru");
        assert_eq!(pages[0].lines, vec![vec![0, 1], vec![2]], "'satu dua' = 8 huruf pas, 'tiga' pindah baris");
        assert_eq!(pages[1].lines, vec![vec![3]]);
        assert_eq!(pages[2].lines, vec![vec![4]]);

        let cues = cues(&words, &pages);
        assert_eq!(cues.len(), 5);
        assert_eq!((cues[0].start, cues[0].end), (0.0, 0.3));
        assert_eq!((cues[2].start, cues[2].end), (0.6, 0.9), "kata terakhir halaman berakhir di end-nya sendiri");
        assert_eq!(cues[3].page, 1);
    }

    #[test]
    fn builds_caption_and_text_events() {
        let mut p = Project::new("t");
        p.captions.enabled = true;
        p.captions.words = vec![w("halo", 0.0, 0.5), w("dunia", 0.5, 1.0)];
        p.captions.style.words_per_page = 2;
        p.tracks[1].clips.push(Clip {
            id: "t1".into(),
            kind: ClipKind::Text,
            start: 1.0,
            duration: 2.0,
            transform: Transform { x: 0.0, y: -500.0, scale: 1.5, rotation: 10.0, opacity: 0.5 },
            text: Some(TextClip { text: "Judul {x}".into(), style: TextStyle { background: "#00000080".into(), ..Default::default() } }),
            ..Default::default()
        });
        let ass = build_ass(&p).expect("ada event");
        assert!(ass.contains("PlayResX: 1080"));
        assert!(ass.contains("Style: Cap,Impact,80,&H00FFFFFF"));
        assert!(ass.contains("Dialogue: 0,0:00:00.00,0:00:00.50,Cap,,0,0,0,,{\\an5\\pos(540,1382.4)}{\\c&H00E6FF&\\fscx115\\fscy115}HALO{\\r} DUNIA"));
        assert!(ass.contains("Dialogue: 0,0:00:00.50,0:00:01.00,Cap,,0,0,0,,{\\an5\\pos(540,1382.4)}HALO {\\c&H00E6FF&\\fscx115\\fscy115}DUNIA{\\r}"));
        assert!(ass.contains("Style: T1,Impact,72,&H00FFFFFF,&H000000FF,&H00000000,&H7F000000,0,0,0,0,100,100,0,0,3,4,0,5,0,0,0,1"));
        assert!(ass.contains("Dialogue: 2,0:00:01.00,0:00:03.00,T1,,0,0,0,,{\\an5\\pos(540,460)\\frz-10\\fscx150\\fscy150\\bord6\\shad0\\1a&H7F&\\3a&H7F&\\4a&HBF&}Judul (x)"));

        p.captions.style.animation = "pop".into();
        let popped = build_ass(&p).unwrap();
        assert!(popped.contains("{\\c&H00E6FF&\\fscx144\\fscy144\\t(0,120,\\fscx115\\fscy115)}HALO{\\r} DUNIA"), "{popped}");

        p.captions.enabled = false;
        p.tracks[1].clips.clear();
        assert!(build_ass(&p).is_none());
    }
}
