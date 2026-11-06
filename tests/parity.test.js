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
