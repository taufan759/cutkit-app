"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const T = require("../ui/core/timeline.js");

function setup() {
  const p = T.newProject("Uji");
  p.assets.push({ id: "vid", kind: "video", path: "a.mp4", duration: 10, width: 1920, height: 1080, has_audio: true, has_video: true });
  p.assets.push({ id: "img", kind: "image", path: "m.png", duration: 0, width: 500, height: 500, has_video: true });
  p.assets.push({ id: "sfx", kind: "audio", path: "s.mp3", duration: 2, has_audio: true });
  const c1 = T.insertClip(p, "v1", T.clipFromAsset(p.assets[0]));
  const c2 = T.insertClip(p, "v1", T.clipFromAsset(p.assets[0]));
  return { p, c1, c2 };
}

const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-6, `${a} != ${b}`);

test("proyek baru: kanvas 9:16 dan rasio lain", () => {
  const p = T.newProject();
  assert.equal(p.canvas.width, 1080);
  assert.equal(p.canvas.height, 1920);
  assert.deepEqual([T.newProject("x", "16:9").canvas.width, T.newProject("x", "1:1").canvas.height], [1920, 1080]);
  assert.equal(p.tracks.length, 3);
});

test("insert menempel di akhir track dan menghindari tabrakan", () => {
  const { p, c1, c2 } = setup();
  assert.equal(c1.start, 0);
  assert.equal(c2.start, 10);
  assert.equal(T.duration(p), 20);
  const c3 = T.insertClip(p, "v1", T.clipFromAsset(p.assets[1]), 5);
  assert.ok(c3.start === 20 || c3.start === 0 || c3.start >= 10, "tidak boleh menimpa klip lain");
  assert.throws(() => T.insertClip(p, "a1", T.clipFromAsset(p.assets[0])), /audio/);
});

test("split membagi klip dan menggeser offset sumber", () => {
  const { p, c1 } = setup();
  c1.fade_in = 1;
  c1.fade_out = 2;
  const right = T.splitClip(p, c1.id, 4);
  near(c1.duration, 4);
  near(right.start, 4);
  near(right.duration, 6);
  near(right.in, 4);
  assert.equal(c1.fade_out, 0, "fade out pindah ke bagian kanan");
  assert.equal(right.fade_in, 0);
  assert.equal(right.fade_out, 2);
  assert.equal(p.tracks[0].clips.length, 3);
  assert.deepEqual(p.tracks[0].clips.map((c) => c.start), [0, 4, 10]);
  // split lagi di bagian kanan: in bertambah
  const r2 = T.splitClip(p, right.id, 7.5);
  near(r2.in, 7.5);
  assert.throws(() => T.splitClip(p, c1.id, 0), /di dalam klip/);
  assert.throws(() => T.splitClip(p, c1.id, 4), /di dalam klip/);
});

test("trim dibatasi tetangga, panjang sumber, dan durasi minimum", () => {
  const { p, c1, c2 } = setup();
  T.trimClip(p, c1.id, "end", 6);
  near(c1.duration, 6);
  // tidak bisa diperpanjang melebihi sumber (10 detik)
  T.trimClip(p, c1.id, "end", 30);
  near(c1.duration, 10);
  // trim awal klip kedua: maju 3 detik, in ikut bertambah
  T.trimClip(p, c2.id, "start", 13);
  near(c2.start, 13);
  near(c2.in, 3);
  near(c2.duration, 7);
  // tarik balik ke kiri: tidak bisa melewati in=0 atau klip sebelumnya
  T.trimClip(p, c2.id, "start", 0);
  near(c2.start, 10);
  near(c2.in, 0);
  // durasi minimum
  T.trimClip(p, c2.id, "end", 0);
  near(c2.duration, T.MIN_DURATION);
  // gambar bisa diperpanjang bebas
  const img = T.insertClip(p, "v2", T.clipFromAsset(p.assets[1]), 0);
  T.trimClip(p, img.id, "end", 25);
  near(img.duration, 25);
});

test("pindah klip: slot bebas terdekat dan pindah track sejenis", () => {
  const { p, c1, c2 } = setup();
  T.trimClip(p, c1.id, "end", 4);
  T.moveClip(p, c2.id, 2);
  near(c2.start, 4, "menabrak c1, ditaruh tepat setelahnya");
  T.moveClip(p, c2.id, 1, "v2");
  assert.equal(T.findClip(p, c2.id).track.id, "v2");
  near(c2.start, 1);
  T.moveClip(p, c2.id, 0, "a1");
  assert.equal(T.findClip(p, c2.id).track.id, "v2", "video tidak boleh pindah ke track audio");
});

test("hapus biasa dan ripple, tukar urutan, tutup celah", () => {
  const { p, c1, c2 } = setup();
  const c3 = T.insertClip(p, "v1", T.clipFromAsset(p.assets[1]));
  near(c3.start, 20);
  T.deleteClip(p, c2.id, true);
  near(c3.start, 10, "ripple menggeser klip setelahnya");
  T.deleteClip(p, c1.id);
  near(c3.start, 10, "hapus biasa meninggalkan celah");
  T.closeGaps(p, "v1");
  near(c3.start, 0);

  const s = setup();
  T.trimClip(s.p, s.c1.id, "end", 3);
  T.moveClip(s.p, s.c2.id, 5);
  // [c1 0-3] celah 2 [c2 5-15] -> tukar -> [c2 0-10] celah 2 [c1 12-15]
  assert.ok(T.swapClip(s.p, s.c1.id, 1));
  near(s.c2.start, 0);
  near(s.c1.start, 12);
  assert.deepEqual(s.p.tracks[0].clips.map((c) => c.id), [s.c2.id, s.c1.id]);
  assert.equal(T.swapClip(s.p, s.c1.id, 1), false);
});

test("volume dan fade dijaga dalam batas; gain fade", () => {
  const { p, c1 } = setup();
  T.updateClip(p, c1.id, { volume: 9, fade_in: 20, fade_out: -1, transform: { opacity: 3, scale: 0 } });
  assert.equal(c1.volume, 4);
  assert.equal(c1.fade_in, 10);
  assert.equal(c1.fade_out, 0);
  assert.equal(c1.transform.opacity, 1);
  assert.equal(c1.transform.scale, 0.01);
  const c = { duration: 4, fade_in: 1, fade_out: 2 };
  assert.equal(T.fadeGain(c, 0), 0);
  assert.equal(T.fadeGain(c, 0.5), 0.5);
  assert.equal(T.fadeGain(c, 1.5), 1);
  assert.equal(T.fadeGain(c, 3), 0.5);
});

test("snap ke tepi klip dan playhead", () => {
  const { p, c1 } = setup();
  assert.equal(T.snap(p, 9.93, 0.1), 10);
  assert.equal(T.snap(p, 9.8, 0.1), 9.8);
  assert.equal(T.snap(p, 3.04, 0.1, { playhead: 3 }), 3);
  assert.equal(T.snap(p, 0.05, 0.1, { exclude: [c1.id] }), 0);
});

test("undo/redo memulihkan keadaan", () => {
  const { p, c1 } = setup();
  const h = new T.History();
  h.record(p);
  T.splitClip(p, c1.id, 5);
  assert.equal(p.tracks[0].clips.length, 3);
  const back = h.undo(p);
  assert.equal(back.tracks[0].clips.length, 2);
  assert.ok(h.canRedo);
  const again = h.redo(back);
  assert.equal(again.tracks[0].clips.length, 3);
  assert.ok(h.canUndo);
  h.record(again);
  assert.equal(h.canRedo, false, "perubahan baru menghapus redo");
});

test("track: tambah, urutkan lapisan, hapus", () => {
  const p = T.newProject();
  const v = T.addTrack(p, "video");
  assert.deepEqual(p.tracks.map((t) => t.kind), ["video", "video", "video", "audio"]);
  assert.equal(p.tracks[2].id, v.id);
  assert.ok(T.moveTrack(p, v.id, -1));
  assert.equal(p.tracks[1].id, v.id);
  assert.equal(T.moveTrack(p, v.id, -1) && T.moveTrack(p, v.id, -1), false, "tidak bisa melewati batas");
  assert.ok(T.removeTrack(p, v.id));
  assert.throws(() => T.removeTrack(p, "a1"), /Minimal/);
  T.trackFor(p, T.clipFromAsset({ id: "s", kind: "audio", duration: 1 }), 0);
});

test("trackFor memilih track kosong atau membuat track baru", () => {
  const { p } = setup();
  const sfx = T.clipFromAsset(p.assets[2]);
  assert.equal(T.trackFor(p, sfx, 0).id, "a1");
  T.insertClip(p, "a1", sfx, 0);
  const sfx2 = T.clipFromAsset(p.assets[2]);
  const t = T.trackFor(p, sfx2, 1);
  assert.notEqual(t.id, "a1", "a1 terisi di detik 1, jadi dibuat track audio baru");
  assert.equal(t.kind, "audio");
  const meme = T.clipFromAsset(p.assets[1]);
  assert.equal(T.trackFor(p, meme, 2).id, "v2", "gambar meme masuk track overlay");
});

test("normalize melengkapi proyek lama", () => {
  const p = T.normalize({ name: "lama", tracks: [{ id: "v1", clips: [{ id: "x", start: 1, duration: 2, kind: "text", text: { text: "hai" } }] }] });
  assert.equal(p.canvas.width, 1080);
  assert.equal(p.tracks[0].clips[0].text.style.font, "Impact");
  assert.equal(p.tracks[0].clips[0].volume, 1);
  assert.equal(p.captions.style.words_per_page, 3);
});
