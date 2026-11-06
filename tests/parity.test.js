"use strict";
// Sisi JS dari tes kesamaan preview <-> ekspor. Pasangannya: crates/cutkit-core/tests/parity.rs
const test = require("node:test");
const assert = require("node:assert/strict");
const fixture = require("./fixtures/parity.json");
const captions = require("../ui/core/captions.js");
const geometry = require("../ui/core/geometry.js");

test("pembagian halaman caption sama dengan Rust", () => {
  for (const c of fixture.pagination) {
    const words = c.words.map(([text, start, end]) => ({ text, start, end }));
    const pages = captions.paginate(words, c.words_per_page, c.max_line_chars);
    assert.deepEqual(pages.map((p) => p.lines), c.pages);
    const cues = captions.cues(words, pages).map((q) => [q.page, q.active, q.start, q.end]);
    assert.deepEqual(cues, c.cues);
  }
});

test("gerak Ken Burns sama dengan Rust", () => {
  for (const c of fixture.motion) {
    const got = geometry.motionAt({ kind: c.kind, amount: c.amount }, c.u);
    got.forEach((v, i) => assert.ok(Math.abs(v - c.out[i]) < 1e-9, `${JSON.stringify(c)} -> ${got}`));
  }
});

test("penempatan klip sama dengan Rust", () => {
  for (const c of fixture.placement) {
    const [x, y, scale, rotation, opacity] = c.t;
    const p = geometry.place(c.canvas[0], c.canvas[1], c.asset[0], c.asset[1], c.fit, { x, y, scale, rotation, opacity });
    assert.deepEqual([p.w, p.h, p.ow, p.oh, p.x, p.y], c.out, JSON.stringify(c));
  }
});

test("chroma key mengikuti rumus colorkey ffmpeg", () => {
  // hijau murni -> transparan; merah -> utuh; warna di zona peralihan -> setengah
  const px = new Uint8ClampedArray([0, 255, 0, 255, 255, 0, 0, 255, 0, 160, 0, 255, 0, 255, 0, 100]);
  geometry.colorKey(px, [0, 255, 0], 0.2, 0.1);
  assert.equal(px[3], 0);
  assert.equal(px[7], 255);
  // jarak (0,160,0) ke (0,255,0) = 95/sqrt(3*255^2) = 0.2151 -> (0.2151-0.2)/0.1 = 0.151 -> 38
  assert.equal(px[11], 38);
  assert.equal(px[15], 0, "alpha asli tidak pernah dinaikkan");
  assert.deepEqual(geometry.hexToRgb("#0047bb"), [0, 71, 187]);
});

test("transisi preview memakai easing smoothstep yang sama dengan ekspor", () => {
  assert.equal(geometry.ease(0.5), 0.5);
  assert.ok(Math.abs(geometry.ease(0.25) - 0.15625) < 1e-12);
  const mid = geometry.transitionAt(3.75, 3.5, { kind: "push-left", duration: 0.5 }, null, 0, 1080, 1920);
  assert.equal(mid.dx, 540);
  const out = geometry.transitionAt(3.75, 0, null, { kind: "push-left", duration: 0.5 }, 3.5, 1080, 1920);
  assert.equal(out.dx, -540);
  assert.ok(Math.abs(geometry.transitionAt(3.6, 3.5, { kind: "fade", duration: 0.4 }, null, 0, 1, 1).alpha - geometry.ease(0.25)) < 1e-9);
});

test("matriks warna JS sama dengan rumus Rust", () => {
  const color = require("../ui/core/color.js");
  const gray = color.matrix({ saturation: 0 });
  assert.deepEqual(gray.map((r) => r.map((v) => +v.toFixed(6))), [[0.213, 0.715, 0.072], [0.213, 0.715, 0.072], [0.213, 0.715, 0.072]]);
  for (const c of fixture.color) {
    const m = color.matrix(c.adj);
    // Abu-abu tetap netral (tanpa suhu): jumlah baris = kecerahan.
    if (!c.adj.temperature) m.forEach((row) => assert.ok(Math.abs(row[0] + row[1] + row[2] - (c.adj.brightness || 1)) < 3e-3));
  }
  // Nilai acuan dihitung dengan rumus Rust (color.rs) untuk kasus kedua.
  const m = color.matrix(fixture.color[1].adj);
  assert.ok(Math.abs(m[0][0] - 1.387515) < 1e-5, String(m[0][0]));
  assert.ok(color.isIdentity({}));
  assert.ok(!color.isIdentity({ lut: "x.cube" }));
});

test("LUT .cube: identitas tidak mengubah warna, LUT negatif membalik", () => {
  const color = require("../ui/core/color.js");
  const id = color.parseCube("LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n");
  const px = new Uint8ClampedArray([10, 128, 250, 255]);
  color.applyLut(px, id);
  assert.deepEqual([...px], [10, 128, 250, 255]);
  const inv = color.parseCube("# negatif\nLUT_3D_SIZE 2\n1 1 1\n0 1 1\n1 0 1\n0 0 1\n1 1 0\n0 1 0\n1 0 0\n0 0 0\n");
  color.applyLut(px, inv);
  assert.deepEqual([...px], [245, 127, 5, 255]);
  assert.throws(() => color.parseCube("LUT_3D_SIZE 3\n0 0 0\n"), /tidak valid/);
});
