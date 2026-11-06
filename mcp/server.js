#!/usr/bin/env node
"use strict";
/*
 * Server MCP CutKit (stdio, JSON-RPC 2.0, tanpa dependensi).
 * Membuat dan mengedit proyek CutKit (.cutkit.json) dengan modul timeline yang sama
 * dengan aplikasi (ui/core), lalu menjalankan pekerjaan berat lewat cutkit-cli:
 * probe media, ekspor MP4, auto caption Whisper, TTS Piper, ketukan, hapus background.
 *
 * Pakai di Claude Code:   claude mcp add cutkit -- node "<repo>/mcp/server.js"
 * Lokasi cutkit-cli: env CUTKIT_CLI, atau <repo>/target/release/cutkit-cli.exe.
 */
const fs = require("node:fs");
const path = require("node:path");
const { spawn } = require("node:child_process");

const ROOT = path.resolve(__dirname, "..");
const T = require(path.join(ROOT, "ui/core/timeline.js"));
const C = require(path.join(ROOT, "ui/core/captions.js"));
const TP = require(path.join(ROOT, "ui/core/templates.js"));
const VERSION = require(path.join(ROOT, "package.json")).version;

function findCli() {
  const exe = process.platform === "win32" ? "cutkit-cli.exe" : "cutkit-cli";
  const candidates = [process.env.CUTKIT_CLI, path.join(ROOT, "target", "release", exe), path.join(ROOT, "target", "debug", exe), path.join(__dirname, exe)].filter(Boolean);
  return candidates.find((p) => fs.existsSync(p)) || null;
}

/** Menjalankan cutkit-cli; hasil JSON dari stdout. */
function cli(args, onProgress) {
  const exe = findCli();
  if (!exe) return Promise.reject(new Error("cutkit-cli tidak ditemukan. Build dulu: cargo build --release -p cutkit-cli (atau set CUTKIT_CLI)."));
  return new Promise((resolve, reject) => {
    const child = spawn(exe, args, { windowsHide: true });
    let out = "", err = "";
    child.stdout.on("data", (d) => { out += d; });
    child.stderr.on("data", (d) => {
      err += d;
      for (const line of String(d).split(/\r?\n/)) {
        const m = /^PROGRESS ([\d.]+) ?(.*)$/.exec(line);
        if (m && onProgress) onProgress(+m[1], m[2]);
      }
    });
    child.on("error", reject);
    child.on("close", (code) => {
      let v;
      try { v = JSON.parse(out); } catch { return reject(new Error(`cutkit-cli gagal (${code}): ${err.slice(-500) || out.slice(-500)}`)); }
      if (code !== 0 || (v && v.error)) return reject(new Error(v && v.error ? v.error : `cutkit-cli keluar dengan kode ${code}`));
      resolve(v);
    });
  });
}

/* ---------- file proyek ---------- */

function load(file) {
  const p = JSON.parse(fs.readFileSync(file, "utf8"));
  return T.normalize(p);
}

function save(file, p) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = file + ".cutkit-tmp";
  fs.writeFileSync(tmp, JSON.stringify(p, null, 2));
  fs.renameSync(tmp, file);
}

function summary(p, file) {
  return {
    project: file,
    name: p.name,
    canvas: p.canvas,
    duration: Math.round(T.duration(p) * 100) / 100,
    assets: p.assets.map((a) => ({ id: a.id, kind: a.kind, name: a.name, duration: a.duration, width: a.width, height: a.height, path: a.path })),
    tracks: p.tracks.map((t) => ({
      id: t.id, kind: t.kind, name: t.name, role: t.role || undefined, muted: t.muted || undefined, hidden: t.hidden || undefined,
      clips: t.clips.map((c) => ({
        id: c.id, kind: c.kind, start: c.start, duration: c.duration,
        asset: c.asset_id || undefined, text: c.text ? c.text.text : undefined, effect: c.effect ? c.effect.kind : undefined,
        speed: c.speed !== 1 ? c.speed : undefined, motion: c.motion && c.motion.kind !== "none" ? c.motion.kind : undefined,
        transition: c.transition && c.transition.kind !== "none" ? c.transition : undefined, keyframes: c.keyframes && c.keyframes.length ? c.keyframes.length : undefined,
      })),
    })),
    captions: { enabled: p.captions.enabled, words: p.captions.words.length, style: p.captions.style },
  };
}

/** Mengimpor file ke proyek (probe lewat cutkit-cli); file yang sudah ada dipakai ulang. */
async function importFiles(p, files) {
  const fresh = files.filter((f) => !p.assets.some((a) => a.path.toLowerCase() === path.resolve(f).toLowerCase()));
  if (fresh.length) {
    const probed = await cli(["probe", ...fresh.map((f) => path.resolve(f))]);
    for (const r of probed) {
      if (!r.info) throw new Error(`${r.path}: ${r.error}`);
      const i = r.info;
      p.assets.push({ id: T.uid("m"), path: r.path, kind: i.kind, name: path.basename(r.path), duration: i.duration, width: i.width, height: i.height, has_audio: i.has_audio, has_video: i.has_video, proxy: null, waveform: null, playable: i.playable, alpha: !!i.alpha });
    }
  }
  return files.map((f) => p.assets.find((a) => a.path.toLowerCase() === path.resolve(f).toLowerCase()));
}

function trackId(p, want, clip) {
  if (!want || want === "auto") return null;
  if (want === "main") return p.tracks.find((t) => t.kind === "video" && t.role !== "effects").id;
  if (want === "overlay") return T.trackFor(p, clip, clip.start || 0, p.tracks.find((t) => t.kind === "video" && t !== p.tracks[0] && t.role !== "effects")?.id).id;
  if (want === "audio") return T.trackFor(p, clip, clip.start || 0).id;
  if (!T.getTrack(p, want)) throw new Error(`Track ${want} tidak ada`);
  return want;
}

const CAPTION_PRESETS = {
  kuning: { font: "Impact", color: "#ffffff", highlight_color: "#ffe600", outline_color: "#000000", outline: 5, shadow: 2, background: "", uppercase: true, highlight_scale: 1.15, animation: "pop" },
  hijau: { font: "Arial Black", color: "#ffffff", highlight_color: "#38ff6a", outline_color: "#000000", outline: 6, shadow: 0, background: "", uppercase: true, highlight_scale: 1.1, animation: "pop" },
  kotak: { font: "Segoe UI", bold: true, color: "#ffffff", highlight_color: "#35c9ff", outline_color: "#000000", outline: 12, shadow: 0, background: "#000000b0", uppercase: false, highlight_scale: 1, animation: "none" },
  youtube: { font: "Segoe UI", bold: true, size: 54, color: "#ffffff", highlight_color: "#ffffff", outline_color: "#000000", outline: 10, shadow: 0, background: "#000000a8", uppercase: false, highlight_scale: 1, animation: "none", words_per_page: 8, max_line_chars: 42, y: 0.88 },
};

function applyCaptionStyle(p, style) {
  if (!style) return;
  const st = typeof style === "string" ? CAPTION_PRESETS[style] : style;
  if (!st) throw new Error(`Gaya caption tidak dikenal: ${style} (pilihan: ${Object.keys(CAPTION_PRESETS).join(", ")})`);
  Object.assign(p.captions.style, st);
}

function replaceWords(p, words, from, to) {
  p.captions.words = p.captions.words.filter((w) => w.end <= from || w.start >= to).concat(words).sort((a, b) => a.start - b.start);
  p.captions.enabled = true;
}

function modelsDirOf(info) { return info.models_dir; }

/* ---------- daftar tool ---------- */

const str = (description) => ({ type: "string", description });
const num = (description) => ({ type: "number", description });
const bool = (description) => ({ type: "boolean", description });
const PROJECT = str("Path file proyek .cutkit.json");

const TOOLS = [
  {
    name: "cutkit_status",
    description: "Status CutKit: lokasi ffmpeg, folder model AI, model yang sudah terpasang, dan kesiapan Python untuk hapus background.",
    inputSchema: { type: "object", properties: {} },
    run: async () => cli(["info"]),
  },
  {
    name: "cutkit_new_project",
    description: "Membuat proyek CutKit baru (file .cutkit.json). Rasio: 9:16 (Shorts), 16:9 (YouTube), 16:9-4k, 1:1, 4:5, 3:4.",
    inputSchema: { type: "object", required: ["project"], properties: { project: PROJECT, name: str("Nama proyek"), ratio: { type: "string", enum: Object.keys(T.RATIOS) }, fps: num("Frame per detik (24-60), bawaan 30") } },
    run: async (a) => {
      const p = T.newProject(a.name || path.basename(a.project).replace(/\.cutkit\.json$|\.json$/i, ""), a.ratio || "9:16");
      if (a.fps) p.canvas.fps = a.fps;
      save(a.project, p);
      return summary(p, a.project);
    },
  },
  {
    name: "cutkit_project_summary",
    description: "Isi proyek: aset, track, klip (id, waktu, jenis), dan caption. Pakai id klip dari sini untuk edit.",
    inputSchema: { type: "object", required: ["project"], properties: { project: PROJECT } },
    run: async (a) => summary(load(a.project), a.project),
  },
  {
    name: "cutkit_import_media",
    description: "Mengimpor file video/gambar/audio ke proyek (tanpa menaruh di timeline). Mengembalikan id aset.",
    inputSchema: { type: "object", required: ["project", "files"], properties: { project: PROJECT, files: { type: "array", items: { type: "string" } } } },
    run: async (a) => {
      const p = load(a.project);
      const assets = await importFiles(p, a.files);
      save(a.project, p);
      return assets.map((x) => ({ id: x.id, kind: x.kind, name: x.name, duration: x.duration, width: x.width, height: x.height }));
    },
  },
  {
    name: "cutkit_add_clip",
    description: "Menaruh media di timeline. track: main (video utama, ditempel di akhir bila start kosong), overlay, audio, auto, atau id track. Opsi: start, duration, in (detik sumber), fit (contain/cover), x, y, scale, rotation, opacity, speed, volume, fade_in, fade_out, motion (zoom-in/zoom-out/pan-left/pan-right/pan-up/pan-down), background (blur), chroma (true = hapus hijau).",
    inputSchema: {
      type: "object", required: ["project"],
      properties: {
        project: PROJECT, file: str("Path media (diimpor otomatis)"), asset_id: str("Atau id aset yang sudah diimpor"),
        track: str("main | overlay | audio | auto | id track"), start: num("Detik di timeline"), duration: num("Durasi di timeline (detik)"), in: num("Mulai dari detik ke berapa di sumber"),
        fit: { type: "string", enum: ["contain", "cover"] }, x: num("Geser X dari tengah (px kanvas)"), y: num("Geser Y dari tengah (px kanvas)"), scale: num("Skala (1 = pas)"), rotation: num("Derajat"), opacity: num("0..1"),
        speed: num("0.1..10"), volume: num("0..4"), fade_in: num("detik"), fade_out: num("detik"),
        motion: { type: "string", enum: ["none", "zoom-in", "zoom-out", "pan-left", "pan-right", "pan-up", "pan-down"] }, motion_amount: num("0.02..0.6"),
        background: { type: "string", enum: ["none", "blur"] }, chroma: bool("Hapus latar hijau"),
      },
    },
    run: async (a) => {
      const p = load(a.project);
      let asset = a.asset_id ? T.getAsset(p, a.asset_id) : null;
      if (!asset && a.file) [asset] = await importFiles(p, [a.file]);
      if (!asset) throw new Error("Butuh file atau asset_id");
      const clip = T.clipFromAsset(asset, { fit: a.fit, duration: a.duration });
      if (a.in != null) clip.in = a.in;
      if (a.duration != null) clip.duration = a.duration;
      for (const k of ["x", "y", "scale", "rotation", "opacity"]) if (a[k] != null) clip.transform[k] = a[k];
      for (const k of ["volume", "fade_in", "fade_out"]) if (a[k] != null) clip[k] = a[k];
      if (a.motion) clip.motion = { kind: a.motion, amount: a.motion_amount || 0.15 };
      if (a.background) clip.background = a.background;
      if (a.chroma) clip.chroma.enabled = true;
      clip.start = a.start || 0;
      const want = a.track || (asset.kind === "audio" ? "audio" : "main");
      let tid = trackId(p, want, clip);
      if (!tid) tid = T.trackFor(p, clip, clip.start).id;
      T.insertClip(p, tid, clip, a.start == null && want === "main" ? null : clip.start);
      if (a.speed && a.speed !== 1) T.setSpeed(p, clip.id, a.speed);
      save(a.project, p);
      return { clip_id: clip.id, track: tid, start: clip.start, duration: clip.duration };
    },
  },
  {
    name: "cutkit_edit_clip",
    description: "Mengubah klip: start (pindah), duration, speed, volume, fade_in, fade_out, transform {x,y,scale,rotation,opacity}, motion {kind,amount}, background, chroma {enabled,color,similarity,blend}, color {brightness,contrast,saturation,hue,temperature,lut}, transition {kind: fade/slide-left/slide-right/slide-up/slide-down/push-left/push-right/none, duration}, keyframes [{t,x,y,scale,rotation,opacity}], text, style (klip teks).",
    inputSchema: { type: "object", required: ["project", "clip_id"], properties: { project: PROJECT, clip_id: str("Id klip"), set: { type: "object", description: "Properti yang diubah" } } },
    run: async (a) => {
      const p = load(a.project);
      const f = T.findClip(p, a.clip_id);
      if (!f) throw new Error("Klip tidak ditemukan");
      const s = a.set || {};
      if (s.start != null) T.moveClip(p, a.clip_id, s.start, s.track);
      if (s.speed != null) T.setSpeed(p, a.clip_id, s.speed);
      if (s.duration != null) T.setDuration(p, a.clip_id, s.duration);
      if (s.transition) T.setTransition(p, a.clip_id, s.transition.kind, s.transition.duration);
      const patch = {};
      for (const k of ["volume", "fade_in", "fade_out", "muted", "fit", "background", "transform", "motion", "chroma", "color"]) if (s[k] != null) patch[k] = s[k];
      if (s.text != null || s.style) patch.text = { text: s.text, style: s.style };
      if (Object.keys(patch).length) T.updateClip(p, a.clip_id, patch);
      if (Array.isArray(s.keyframes)) {
        f.clip.keyframes = [];
        for (const k of s.keyframes) T.setKeyframe(p, a.clip_id, k.t, k);
      }
      if (s.effect) Object.assign(f.clip.effect || {}, s.effect);
      save(a.project, p);
      return summary(p, a.project).tracks.flatMap((t) => t.clips).find((c) => c.id === a.clip_id);
    },
  },
  {
    name: "cutkit_split_clip",
    description: "Memotong klip di waktu timeline tertentu. Mengembalikan id klip bagian kanan.",
    inputSchema: { type: "object", required: ["project", "clip_id", "time"], properties: { project: PROJECT, clip_id: str("Id klip"), time: num("Detik di timeline") } },
    run: async (a) => { const p = load(a.project); const r = T.splitClip(p, a.clip_id, a.time); save(a.project, p); return { right_clip_id: r.id }; },
  },
  {
    name: "cutkit_delete_clip",
    description: "Menghapus klip; ripple = true menutup celah dengan menggeser klip sesudahnya.",
    inputSchema: { type: "object", required: ["project", "clip_id"], properties: { project: PROJECT, clip_id: str("Id klip"), ripple: bool("Tutup celah") } },
    run: async (a) => { const p = load(a.project); T.deleteClip(p, a.clip_id, !!a.ripple); save(a.project, p); return { deleted: a.clip_id }; },
  },
  {
    name: "cutkit_add_text",
    description: "Menambah teks judul. y negatif = ke atas (px dari tengah). style: font, size, color, outline_color, outline, shadow, background (#rrggbbaa), bold, italic.",
    inputSchema: { type: "object", required: ["project", "text"], properties: { project: PROJECT, text: str("Isi teks"), start: num("Detik"), duration: num("Detik, bawaan 3"), x: num("px"), y: num("px"), style: { type: "object" } } },
    run: async (a) => {
      const p = load(a.project);
      const k = T.styleScale(p.canvas);
      const clip = T.textClip(a.text, { duration: a.duration, y: a.y != null ? a.y : Math.round(-0.32 * p.canvas.height), style: Object.assign({ size: Math.round(96 * k) }, a.style || {}) });
      if (a.x) clip.transform.x = a.x;
      const t = T.trackFor(p, clip, a.start || 0);
      T.insertClip(p, t.id, clip, a.start || 0);
      save(a.project, p);
      return { clip_id: clip.id, track: t.id };
    },
  },
  {
    name: "cutkit_add_effect",
    description: `Menambah efek video di track Efek (berlaku pada lapisan di bawahnya). kind: ${T.EFFECTS.map(([k]) => k).join(", ")}.`,
    inputSchema: { type: "object", required: ["project", "kind"], properties: { project: PROJECT, kind: { type: "string", enum: T.EFFECTS.map(([k]) => k) }, start: num("Detik"), duration: num("Detik"), intensity: num("0..1") } },
    run: async (a) => {
      const p = load(a.project);
      const clip = T.effectClip(a.kind, { duration: a.duration || 1, intensity: a.intensity });
      T.insertClip(p, T.effectTrack(p).id, clip, a.start || 0);
      save(a.project, p);
      return { clip_id: clip.id };
    },
  },
  {
    name: "cutkit_quick_video",
    description: "Menyusun video otomatis: voice over + gambar/klip dibagi sepanjang suara (potongan menempel ke awal kata atau ketukan musik), musik pelan, caption dari file .srt/JSON kata atau naskah, gerak Ken Burns. Isi timeline lama diganti.",
    inputSchema: {
      type: "object", required: ["project", "visuals"],
      properties: {
        project: PROJECT, voice: str("File voice over (opsional)"), visuals: { type: "array", items: { type: "string" }, description: "File gambar/video berurutan" },
        music: str("Musik latar (opsional)"), music_volume: num("0..1, bawaan 0.15"), captions_file: str(".srt atau JSON kata (opsional)"), script: str("Naskah bila tanpa file caption"),
        motion: bool("Zoom pelan otomatis di gambar (bawaan true)"), on_beat: bool("Ganti gambar di ketukan musik"), fit: { type: "string", enum: ["cover", "contain"] },
        caption_style: str("kuning | hijau | kotak | youtube"),
      },
    },
    run: async (a) => {
      const p = load(a.project);
      const voice = a.voice ? (await importFiles(p, [a.voice]))[0] : null;
      const music = a.music ? (await importFiles(p, [a.music]))[0] : null;
      const visuals = await importFiles(p, a.visuals || []);
      let words = null;
      if (a.captions_file) {
        const text = fs.readFileSync(a.captions_file, "utf8");
        words = /\.srt$/i.test(a.captions_file) ? C.srtToWords(C.parseSrt(text)) : C.parseWordsJson(text);
      } else if (a.script) {
        words = C.textToWords(a.script, 0, voice ? voice.duration : visuals.length * T.STILL_DURATION);
      }
      let beats = null;
      if (a.on_beat && music) {
        const info = await cli(["beats", music.path]);
        if (info) { music.beats = info; beats = info.downbeats.length >= visuals.length ? info.downbeats : info.beats; }
      }
      const total = T.autoAssemble(p, { voice, visuals, music, words, musicVolume: a.music_volume ?? 0.15, fit: a.fit || "cover", motion: a.motion !== false, beats });
      applyCaptionStyle(p, a.caption_style || (p.canvas.width > p.canvas.height ? "youtube" : "kuning"));
      save(a.project, p);
      return { duration: total, ...summary(p, a.project) };
    },
  },
  {
    name: "cutkit_captions_from_audio",
    description: "Auto caption: mentranskripsi semua klip bersuara di satu track dengan Whisper (offline), lalu memasang caption kata-per-kata. script (opsional) membetulkan ejaan.",
    inputSchema: { type: "object", required: ["project"], properties: { project: PROJECT, track: str("Id track sumber (bawaan: track audio pertama yang berisi, atau track utama)"), model: { type: "string", enum: ["whisper-base", "whisper-small", "whisper-medium"] }, language: str("id, en, atau auto"), script: str("Naskah untuk koreksi ejaan"), caption_style: str("kuning | hijau | kotak | youtube") } },
    run: async (a, progress) => {
      const p = load(a.project);
      const audible = (t) => t.clips.filter((c) => (c.kind === "audio" || c.kind === "video") && T.getAsset(p, c.asset_id)?.has_audio && !c.muted);
      const track = a.track ? T.getTrack(p, a.track) : p.tracks.find((t) => t.kind === "audio" && audible(t).length) || p.tracks.find((t) => audible(t).length);
      if (!track) throw new Error("Tidak ada klip bersuara di proyek.");
      const clips = audible(track);
      let words = [];
      for (const [i, c] of clips.entries()) {
        const asset = T.getAsset(p, c.asset_id);
        const sp = T.speedOf(c);
        const r = await cli(["transcribe", asset.path, "--start", String(c.in), "--duration", String(c.duration * sp), "--model", a.model || "whisper-small", "--lang", a.language || "id"], (sec) => progress && progress((i + Math.min(1, sec / (c.duration * sp))) / clips.length));
        for (const w of r.words) {
          const s = c.start + w.start / sp;
          if (s < T.clipEnd(c) - 0.01) words.push({ text: w.text, start: +s.toFixed(3), end: +Math.min(c.start + w.end / sp, T.clipEnd(c)).toFixed(3) });
        }
      }
      if (a.script) words = C.alignScript(a.script, words);
      replaceWords(p, words, Math.min(...clips.map((c) => c.start)), Math.max(...clips.map((c) => T.clipEnd(c))));
      applyCaptionStyle(p, a.caption_style);
      save(a.project, p);
      return { words: words.length, text: words.map((w) => w.text).join(" ").slice(0, 2000) };
    },
  },
  {
    name: "cutkit_set_captions",
    description: "Memasang caption dari file .srt, JSON kata ({words:[{text,start,end}]} atau alignment ElevenLabs), atau teks yang dibagi rata ke rentang waktu. Juga mengatur gaya/aktif.",
    inputSchema: { type: "object", required: ["project"], properties: { project: PROJECT, file: str(".srt atau .json"), text: str("Teks manual"), start: num("Detik awal untuk teks manual"), end: num("Detik akhir untuk teks manual"), offset: num("Geser waktu (detik)"), enabled: bool("Tampilkan caption"), style: { description: "Nama preset (kuning/hijau/kotak/youtube) atau objek gaya" } } },
    run: async (a) => {
      const p = load(a.project);
      let words = null;
      if (a.file) {
        const text = fs.readFileSync(a.file, "utf8");
        words = /\.srt$/i.test(a.file) ? C.srtToWords(C.parseSrt(text)) : C.parseWordsJson(text);
      } else if (a.text) {
        words = C.textToWords(a.text, a.start || 0, a.end != null ? a.end : T.duration(p));
      }
      if (words) {
        const off = a.offset || 0;
        words = words.map((w) => ({ text: w.text, start: +(w.start + off).toFixed(3), end: +(w.end + off).toFixed(3) }));
        if (a.text) replaceWords(p, words, words[0].start, words[words.length - 1].end); else { p.captions.words = words; p.captions.enabled = true; }
      }
      if (a.enabled != null) p.captions.enabled = a.enabled;
      applyCaptionStyle(p, a.style);
      save(a.project, p);
      return { words: p.captions.words.length, enabled: p.captions.enabled };
    },
  },
  {
    name: "cutkit_tts",
    description: "TTS offline Bahasa Indonesia (Piper): naskah jadi suara WAV di folder proyek, ditaruh di track audio, plus caption kata-per-kata (waktu diselaraskan Whisper bila tersedia).",
    inputSchema: { type: "object", required: ["project", "text"], properties: { project: PROJECT, text: str("Naskah"), speed: num("0.7..1.4"), gap: num("Jeda antar kalimat (detik)"), at: num("Mulai di detik (bawaan 0)"), captions: bool("Buat caption (bawaan true)"), align: bool("Selaraskan dengan Whisper (bawaan true)") } },
    run: async (a) => {
      const p = load(a.project);
      const out = path.join(path.dirname(path.resolve(a.project)), "suara", `tts-${Date.now()}.wav`);
      const res = await cli(["tts", out, "--text", a.text, "--speed", String(a.speed || 1), "--gap", String(a.gap ?? 0.3)]);
      const [asset] = await importFiles(p, [res.path]);
      const clip = T.clipFromAsset(asset);
      const at = a.at || 0;
      T.insertClip(p, T.trackFor(p, clip, at).id, clip, at);
      if (a.captions !== false) {
        let words = null;
        if (a.align !== false) {
          try { const r = await cli(["transcribe", res.path, "--model", "whisper-small", "--lang", "id"]); if (r.words.length) words = C.alignScript(a.text, r.words); } catch { /* model belum ada: pakai waktu per kalimat */ }
        }
        if (!words) words = res.sentences.flatMap((s) => C.distribute(s.text, s.start, s.end));
        replaceWords(p, words.map((w) => ({ text: w.text, start: +(clip.start + w.start).toFixed(3), end: +(clip.start + w.end).toFixed(3) })), clip.start, T.clipEnd(clip));
      }
      save(a.project, p);
      return { clip_id: clip.id, file: res.path, duration: res.duration, sentences: res.sentences.length };
    },
  },
  {
    name: "cutkit_detect_beats",
    description: "Mendeteksi tempo (BPM), ketukan, dan ketukan kuat dari file musik.",
    inputSchema: { type: "object", required: ["file"], properties: { file: str("File musik") } },
    run: async (a) => cli(["beats", a.file]),
  },
  {
    name: "cutkit_remove_background",
    description: "Hapus background (AI, tanpa green screen) untuk klip gambar/video; klip langsung memakai versi transparan. model: matting-fast (cepat) atau matting-best (akurat).",
    inputSchema: { type: "object", required: ["project", "clip_id"], properties: { project: PROJECT, clip_id: str("Id klip"), model: { type: "string", enum: ["matting-fast", "matting-best"] } } },
    run: async (a, progress) => {
      const p = load(a.project);
      const f = T.findClip(p, a.clip_id);
      if (!f) throw new Error("Klip tidak ditemukan");
      const c = f.clip;
      const asset = T.getAsset(p, c.asset_id);
      const image = c.kind === "image";
      const out = path.join(path.dirname(path.resolve(a.project)), "tanpa-latar", `${path.parse(asset.name).name}-${Date.now()}.${image ? "png" : "webm"}`);
      const args = ["remove-bg", asset.path, out, "--kind", image ? "image" : "video", "--model", a.model || "matting-fast"];
      if (!image) args.push("--start", String(c.in), "--duration", String(c.duration * T.speedOf(c)), "--fps", String(p.canvas.fps), ...(asset.has_audio ? ["--audio"] : []));
      await cli(args, (pct) => progress && progress(pct / 100));
      const [na] = await importFiles(p, [out]);
      c.asset_id = na.id;
      if (!image) c.in = 0;
      save(a.project, p);
      return { clip_id: c.id, file: out };
    },
  },
  {
    name: "cutkit_list_templates",
    description: "Daftar template bawaan dan buatan sendiri beserta slot media yang perlu diisi.",
    inputSchema: { type: "object", properties: {} },
    run: async () => listTemplates().map(({ t, ...rest }) => ({ ...rest, slots: TP.slots(t) })),
  },
  {
    name: "cutkit_apply_template",
    description: "Membuat proyek baru dari template. media = { idSlot: path file }; slot yang tidak diisi dibuang.",
    inputSchema: { type: "object", required: ["template", "project"], properties: { template: str("Nama template atau path file template"), project: PROJECT, name: str("Nama proyek"), media: { type: "object", description: "idSlot -> path file" } } },
    run: async (a) => {
      const found = listTemplates().find((x) => x.name.toLowerCase() === String(a.template).toLowerCase() || x.file === a.template);
      const t = found ? found.t : JSON.parse(fs.readFileSync(a.template, "utf8"));
      const p = T.newProject(a.name || t.template.name);
      const picks = {};
      for (const [slot, file] of Object.entries(a.media || {})) picks[slot] = (await importFiles(p, [file]))[0];
      const out = T.normalize(TP.applyTemplate(t, picks, a.name || t.template.name));
      save(a.project, out);
      return summary(out, a.project);
    },
  },
  {
    name: "cutkit_export",
    description: "Mengekspor proyek ke MP4 (H.264 + AAC). Video panjang otomatis diekspor bertahap. quality: fast/balanced/best; loudnorm = ratakan volume ke -14 LUFS; qsv = Intel Quick Sync.",
    inputSchema: { type: "object", required: ["project", "output"], properties: { project: PROJECT, output: str("File .mp4 hasil"), quality: { type: "string", enum: ["fast", "balanced", "best"] }, loudnorm: bool("Normalisasi loudness"), qsv: bool("Pakai Intel Quick Sync") } },
    run: async (a, progress) => {
      const args = ["export", path.resolve(a.project), path.resolve(a.output), "--quality", a.quality || "balanced"];
      if (a.loudnorm !== false) args.push("--loudnorm");
      if (a.qsv) args.push("--qsv");
      return cli(args, (pct) => progress && progress(pct / 100));
    },
  },
  {
    name: "cutkit_download_model",
    description: "Mengunduh model AI CutKit (cek id lewat cutkit_status): whisper-base/small/medium, piper-engine, piper-id, piper-id-config, matting-fast, matting-best, vad-silero.",
    inputSchema: { type: "object", required: ["id"], properties: { id: str("Id model") } },
    run: async (a, progress) => cli(["download", a.id], (pct) => progress && progress(pct / 100)),
  },
];

function listTemplates() {
  const out = [];
  const dir = path.join(ROOT, "ui", "templates");
  try {
    for (const it of JSON.parse(fs.readFileSync(path.join(dir, "index.json"), "utf8"))) out.push({ name: it.name, file: path.join(dir, it.file), builtin: true, description: it.description, t: JSON.parse(fs.readFileSync(path.join(dir, it.file), "utf8")) });
  } catch { /* */ }
  const user = path.join(process.env.APPDATA || "", "io.github.taufan759.cutkit", "templates");
  try {
    for (const f of fs.readdirSync(user).filter((x) => x.endsWith(".json"))) {
      const t = JSON.parse(fs.readFileSync(path.join(user, f), "utf8"));
      out.push({ name: t.template?.name || f, file: path.join(user, f), builtin: false, description: t.template?.description || "", t });
    }
  } catch { /* */ }
  return out;
}

/* ---------- JSON-RPC stdio ---------- */

function send(msg) { process.stdout.write(JSON.stringify(msg) + "\n"); }

async function handle(msg) {
  const { id, method, params } = msg;
  if (method === "initialize") {
    return send({ jsonrpc: "2.0", id, result: {
      protocolVersion: (params && params.protocolVersion) || "2025-06-18",
      capabilities: { tools: {} },
      serverInfo: { name: "cutkit", version: VERSION },
      instructions: "CutKit: editor video lokal. Alur umum: cutkit_new_project -> cutkit_quick_video (atau cutkit_add_clip/add_text/add_effect) -> cutkit_captions_from_audio -> cutkit_export. Proyek bisa dibuka juga di aplikasi CutKit.",
    } });
  }
  if (method === "ping") return send({ jsonrpc: "2.0", id, result: {} });
  if (method === "tools/list") return send({ jsonrpc: "2.0", id, result: { tools: TOOLS.map(({ name, description, inputSchema }) => ({ name, description, inputSchema })) } });
  if (method === "tools/call") {
    const tool = TOOLS.find((t) => t.name === params.name);
    if (!tool) return send({ jsonrpc: "2.0", id, error: { code: -32602, message: `Tool tidak dikenal: ${params.name}` } });
    const token = params._meta && params._meta.progressToken;
    const progress = token != null ? (frac) => send({ jsonrpc: "2.0", method: "notifications/progress", params: { progressToken: token, progress: Math.round(frac * 1000) / 10, total: 100 } }) : null;
    try {
      const result = await tool.run(params.arguments || {}, progress);
      return send({ jsonrpc: "2.0", id, result: { content: [{ type: "text", text: JSON.stringify(result, null, 2) }] } });
    } catch (e) {
      return send({ jsonrpc: "2.0", id, result: { content: [{ type: "text", text: `Gagal: ${e.message}` }], isError: true } });
    }
  }
  if (id !== undefined && method) return send({ jsonrpc: "2.0", id, error: { code: -32601, message: `Metode tidak didukung: ${method}` } });
  // notifikasi (initialized, cancelled, dll.) tidak dibalas
}

let buf = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buf += chunk;
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i).trim();
    buf = buf.slice(i + 1);
    if (!line) continue;
    let msg;
    try { msg = JSON.parse(line); } catch { send({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "JSON tidak valid" } }); continue; }
    handle(msg).catch((e) => send({ jsonrpc: "2.0", id: msg.id ?? null, error: { code: -32603, message: e.message } }));
  }
});
process.stdin.on("end", () => process.exit(0));

module.exports = { TOOLS };
