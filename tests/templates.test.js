"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const T = require("../ui/core/timeline.js");
const TP = require("../ui/core/templates.js");
const fs = require("node:fs");
const path = require("node:path");

function sample() {
  const p = T.newProject("Asli");
  p.assets.push({ id: "v", kind: "video", path: "D:/a.mp4", name: "a.mp4", duration: 10, width: 1920, height: 1080, has_audio: true, has_video: true, proxy: "x", beats: { bpm: 120 } });
  p.assets.push({ id: "i", kind: "image", path: "D:/b.png", name: "b.png", width: 500, height: 500, has_video: true });
  p.assets.push({ id: "unused", kind: "audio", path: "D:/c.mp3", duration: 3, has_audio: true });
  const v = T.insertClip(p, "v1", T.clipFromAsset(p.assets[0]));
  v.in = 2; v.duration = 6;
  T.insertClip(p, "v2", T.clipFromAsset(p.assets[1]), 1);
  T.insertClip(p, "v2", T.textClip("JUDUL"), 5);
  p.captions.enabled = true;
  p.captions.words = [{ text: "halo", start: 0, end: 1 }];
  return p;
}

test("makeTemplate: media jadi slot tanpa path, kata caption dikosongkan", () => {
  const t = TP.makeTemplate(sample(), { name: "Contoh", description: "uji" });
  assert.deepEqual(t.assets.map((a) => [a.id, a.label, a.slot]), [["v", "Video 1", true], ["i", "Gambar 1", true]]);
  assert.ok(t.assets.every((a) => !a.path && !a.proxy && !a.beats), "tidak ada path atau data cache");
  assert.equal(t.captions.words.length, 0);
  assert.ok(t.captions.enabled, "gaya caption tetap");
  assert.deepEqual(t.template, { name: "Contoh", description: "uji", ratio: "1080×1920", duration: 8 });
  assert.deepEqual(TP.slots(t), [{ id: "v", kind: "video", label: "Video 1", need: 8 }, { id: "i", kind: "image", label: "Gambar 1", need: 0 }]);
});

test("applyTemplate: slot diisi media baru, dipangkas bila lebih pendek, slot dilewati dibuang", () => {
  const t = TP.makeTemplate(sample());
  const shortVid = { id: "baru", kind: "video", path: "E:/pendek.mp4", duration: 5, width: 1080, height: 1920, has_audio: true, has_video: true };
  const p = TP.applyTemplate(t, { v: shortVid, i: null }, "Hasil");
  assert.equal(p.name, "Hasil");
  assert.equal(p.template, undefined);
  assert.deepEqual(p.assets.map((a) => a.path), ["E:/pendek.mp4"]);
  const v = p.tracks[0].clips[0];
  assert.deepEqual([v.asset_id, v.in, v.duration], ["baru", 2, 3], "sisa sumber 5 - 2 = 3 detik");
  assert.equal(p.tracks[1].clips.length, 1, "gambar dilewati, teks tetap");
  assert.equal(p.tracks[1].clips[0].kind, "text");
  // Gambar diganti video: jenis klip ikut berubah.
  const q = TP.applyTemplate(t, { v: shortVid, i: shortVid }, "Q");
  assert.equal(q.tracks[1].clips.find((c) => c.asset_id === "baru").kind, "video");
  assert.equal(q.assets.length, 1, "media yang sama tidak diduplikasi");
});

test("template bawaan valid dan bisa dipakai", () => {
  const dir = path.join(__dirname, "..", "ui", "templates");
  const index = JSON.parse(fs.readFileSync(path.join(dir, "index.json"), "utf8"));
  assert.ok(index.length >= 3);
  for (const item of index) {
    const t = JSON.parse(fs.readFileSync(path.join(dir, item.file), "utf8"));
    assert.equal(t.template.name, item.name);
    const sl = TP.slots(t);
    assert.ok(sl.length >= 1, item.name);
    const picks = Object.fromEntries(sl.map((s) => [s.id, { id: `r-${s.id}`, kind: s.kind, path: "x", duration: 60, width: 1080, height: 1920, has_audio: s.kind !== "image", has_video: s.kind !== "audio" }]));
    const p = T.normalize(TP.applyTemplate(t, picks, "uji"));
    assert.ok(T.duration(p) > 0, item.name);
  }
});
