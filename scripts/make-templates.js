"use strict";
// Membuat template bawaan di ui/templates dengan API timeline yang sama dengan aplikasi,
// sehingga formatnya selalu valid. Jalankan: node scripts/make-templates.js
const fs = require("node:fs");
const path = require("node:path");
const T = require("../ui/core/timeline.js");
const TP = require("../ui/core/templates.js");

const out = path.join(__dirname, "..", "ui", "templates");
fs.mkdirSync(out, { recursive: true });
const index = [];

function save(file, project, meta) {
  const t = TP.makeTemplate(project, meta);
  fs.writeFileSync(path.join(out, file), JSON.stringify(t, null, 2) + "\n");
  index.push({ file, name: t.template.name, description: t.template.description, ratio: t.template.ratio, duration: t.template.duration });
}

const yellow = { font: "Impact", color: "#ffffff", highlight_color: "#ffe600", outline_color: "#000000", outline: 5, shadow: 2, background: "", uppercase: true, highlight_scale: 1.15, animation: "pop" };

// 1. Fakta Shorts: tiga gambar Ken Burns dengan crossfade, judul di atas, kilat di awal.
{
  const p = T.newProject("Fakta Shorts", "9:16");
  const imgs = [1, 2, 3].map((n) => ({ id: `g${n}`, kind: "image", width: 1080, height: 1920, has_video: true }));
  p.assets.push(...imgs);
  imgs.forEach((a, i) => {
    const c = T.clipFromAsset(a, { fit: "cover", duration: 4 });
    c.motion = { kind: i % 2 ? "zoom-out" : "zoom-in", amount: 0.12 };
    T.insertClip(p, "v1", c);
    if (i > 0) T.setTransition(p, c.id, "fade", 0.4);
  });
  T.insertClip(p, "v2", T.textClip("FAKTA MENARIK", { y: -640, duration: 3, style: { size: 110, outline: 7 } }), 0);
  T.insertClip(p, T.effectTrack(p).id, T.effectClip("flash", { duration: 0.4 }), 0);
  Object.assign(p.captions.style, yellow);
  p.captions.enabled = true;
  save("fakta-shorts.json", p, { name: "Fakta Shorts", description: "3 gambar dengan zoom pelan dan crossfade, judul di atas, kilat di awal, caption kuning.", labels: { g1: "Gambar 1", g2: "Gambar 2", g3: "Gambar 3" } });
}

// 2. Reaksi meme: video utama, meme muncul dengan zoom kejut, SFX, dan teks reaksi.
{
  const p = T.newProject("Reaksi meme", "9:16");
  const vid = { id: "utama", kind: "video", width: 1080, height: 1920, duration: 8, has_audio: true, has_video: true };
  const meme = { id: "meme", kind: "image", width: 600, height: 600, has_video: true };
  const sfx = { id: "sfx", kind: "audio", duration: 1.5, has_audio: true };
  p.assets.push(vid, meme, sfx);
  const v = T.clipFromAsset(vid, { fit: "cover" });
  v.duration = 8;
  v.background = "blur";
  T.insertClip(p, "v1", v);
  const m = T.clipFromAsset(meme, { duration: 2, transform: { y: -420, scale: 0.6 } });
  T.insertClip(p, "v2", m, 2.5);
  T.insertClip(p, "a1", T.clipFromAsset(sfx), 2.5);
  T.insertClip(p, T.effectTrack(p).id, T.effectClip("punch", { duration: 0.6 }), 2.5);
  const txt = T.textClip("WKWKWK", { y: 420, duration: 2, style: { font: "Impact", size: 120, color: "#ffe600", outline: 7 } });
  T.insertClip(p, T.trackFor(p, txt, 2.5).id, txt, 2.5);
  save("reaksi-meme.json", p, { name: "Reaksi meme", description: "Video utama dengan latar blur, meme + SFX + zoom kejut di detik 2,5, teks reaksi.", labels: { utama: "Video utama", meme: "Gambar meme", sfx: "Sound effect" } });
}

// 3. Intro YouTube 16:9: video dengan letterbox, judul besar, subtitle gaya YouTube.
{
  const p = T.newProject("Intro YouTube", "16:9");
  const vid = { id: "klip", kind: "video", width: 1920, height: 1080, duration: 8, has_audio: true, has_video: true };
  const music = { id: "musik", kind: "audio", duration: 8, has_audio: true };
  p.assets.push(vid, music);
  const v = T.clipFromAsset(vid, { fit: "cover" });
  v.duration = 8;
  v.motion = { kind: "zoom-in", amount: 0.08 };
  T.insertClip(p, "v1", v);
  const mc = T.clipFromAsset(music);
  mc.duration = 8; mc.volume = 0.3; mc.fade_in = 1; mc.fade_out = 1.5;
  T.insertClip(p, "a1", mc, 0);
  T.insertClip(p, "v2", T.textClip("JUDUL VIDEO", { y: 0, duration: 3.5, style: { font: "Arial Black", size: 130, outline: 6 } }), 0.5);
  T.insertClip(p, T.effectTrack(p).id, T.effectClip("letterbox", { duration: 8, intensity: 0.3 }), 0);
  Object.assign(p.captions.style, { font: "Segoe UI", bold: true, size: 54, color: "#ffffff", highlight_color: "#ffffff", outline_color: "#000000", outline: 10, shadow: 0, background: "#000000a8", uppercase: false, highlight_scale: 1, animation: "none", words_per_page: 8, max_line_chars: 42, y: 0.88 });
  p.captions.enabled = true;
  save("intro-youtube.json", p, { name: "Intro YouTube 16:9", description: "Klip pembuka dengan zoom pelan, letterbox sinematik, judul besar, musik pelan, subtitle gaya YouTube.", labels: { klip: "Video pembuka", musik: "Musik" } });
}

fs.writeFileSync(path.join(out, "index.json"), JSON.stringify(index, null, 2) + "\n");
console.log(`${index.length} template ditulis ke ${out}`);
