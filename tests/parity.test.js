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

test("penempatan klip sama dengan Rust", () => {
  for (const c of fixture.placement) {
    const [x, y, scale, rotation, opacity] = c.t;
    const p = geometry.place(c.canvas[0], c.canvas[1], c.asset[0], c.asset[1], c.fit, { x, y, scale, rotation, opacity });
    assert.deepEqual([p.w, p.h, p.ow, p.oh, p.x, p.y], c.out, JSON.stringify(c));
  }
});
