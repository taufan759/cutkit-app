"use strict";
// Rumus transisi preview (ui/core/geometry.js) harus sama dengan ekspor (transition.rs);
// pemeriksaan di sini memakai harapan yang sama dengan tes ekspor sungguhan every_transition_renders.
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const G = require("../ui/core/geometry.js");
const T = require("../ui/core/timeline.js");

const W = 320, H = 192;
const at = (kind, p) => G.transitionAt(1 + p, 1, { kind, duration: 1 }, null, 0, W, H);
const maskW = W / 8, maskH = H / 8;
const reveal = (kind, p, x, y) => G.maskReveal(kind, G.ease(p), Math.floor(x / 8), Math.floor(y / 8), maskW, maskH);

test("daftar jenis transisi sama di JS dan Rust", () => {
  const rs = fs.readFileSync(path.join(__dirname, "../crates/cutkit-core/src/transition.rs"), "utf8");
  const block = rs.slice(rs.indexOf("pub const KINDS"), rs.indexOf("];", rs.indexOf("pub const KINDS")));
  const kinds = [...block.matchAll(/"([a-z-]+)"/g)].map((m) => m[1]);
  assert.deepEqual(["none", ...kinds], T.TRANSITIONS);
  for (const k of ["ZOOM_FROM", "SPIN_SCALE_FROM", "BLUR_MAX", "WHIP_BLUR_MAX", "PIXEL_MAX", "GLITCH_MAX", "MASK_DIV"]) {
    const m = new RegExp(`pub const ${k}: \\w+ = ([0-9.]+);`).exec(rs);
    assert.equal(Number(m[1]), G.TRANSITION[k], k);
  }
});

test("alpha linear, geser, zoom, dan putar di tengah transisi", () => {
  assert.equal(at("fade", 0.5).alpha, 0.5);
  assert.equal(at("slide-left", 0.5).dx, W / 2);
  assert.equal(at("whip-right", 0.5).dx, -W / 2, "whip: smoothstep dua kali tetap 0.5 di tengah");
  assert.ok(at("whip-left", 0.25).dx > at("push-left", 0.25).dx, "whip lebih lambat di awal, lebih cepat di tengah");
  assert.equal(at("zoom", 0).scale, 1.25);
  assert.equal(at("zoom", 1).scale, 1);
  assert.ok(Math.abs(at("spin", 0).rot + Math.PI / 2) < 1e-12);
  assert.equal(at("spin", 0).scale, 0.6);
  const push = G.transitionAt(1.5, 0, null, { kind: "whip-left", duration: 1 }, 1, W, H);
  assert.equal(push.dx, -W / 2, "klip lama ikut terdorong whip");
});

test("dip, pikselasi, dan glitch menukar klip di tengah", () => {
  for (const k of ["dip-black", "dip-white", "pixelate", "glitch"]) {
    assert.equal(at(k, 0.49).visible, false, k);
    assert.equal(at(k, 0.5).visible, true, k);
  }
  assert.equal(at("dip-black", 0.25).dip.alpha, 0.5);
  assert.equal(at("dip-white", 0.5).dip.alpha, 1);
  assert.equal(at("dip-white", 0.5).dip.color, "#ffffff");
  assert.ok(Math.abs(at("blur", 0.5).post.amount - 0.02 * W) < 1e-9);
  assert.ok(Math.abs(at("pixelate", 0.5).post.amount - 0.05 * W) < 1e-9);
  assert.equal(at("blur", 1).post, null);
});

test("mask di tengah transisi sama dengan harapan tes ekspor", () => {
  assert.ok(!reveal("wipe-left", 0.5, 30, 96) && reveal("wipe-left", 0.5, 290, 96));
  assert.ok(reveal("wipe-right", 0.5, 30, 96) && !reveal("wipe-right", 0.5, 290, 96));
  assert.ok(!reveal("wipe-up", 0.5, 160, 20) && reveal("wipe-up", 0.5, 160, 172));
  assert.ok(reveal("wipe-down", 0.5, 160, 20) && !reveal("wipe-down", 0.5, 160, 172));
  assert.ok(!reveal("clock", 0.5, 30, 96) && reveal("clock", 0.5, 290, 96), "jam: setengah kanan dulu");
  assert.ok(reveal("iris", 0.5, 160, 96) && !reveal("iris", 0.5, 4, 4));
  let n = 0;
  for (let Y = 0; Y < maskH; Y++) for (let X = 0; X < maskW; X++) n += G.maskReveal("dissolve", 0.5, X, Y, maskW, maskH) ? 1 : 0;
  const frac = n / (maskW * maskH);
  assert.ok(frac > 0.4 && frac < 0.6, `larut: sekitar separuh sel terbuka (${frac})`);
  assert.equal(at("wipe-left", 1).mask, null, "mask hilang sesudah transisi");
});

test("setTransition menerima jenis baru dan menolak yang tidak dikenal", () => {
  const p = T.newProject("x");
  const asset = { id: "a", path: "a.png", kind: "image", name: "a", duration: 0, width: 100, height: 100, has_audio: false, has_video: true };
  p.assets.push(asset);
  const a = T.clipFromAsset(asset); const b = T.clipFromAsset(asset);
  T.insertClip(p, p.tracks.find((t) => t.kind === "video").id, a, 0);
  T.insertClip(p, p.tracks.find((t) => t.kind === "video").id, b, null);
  T.setTransition(p, b.id, "clock", 0.6);
  assert.equal(T.findClip(p, b.id).clip.transition.kind, "clock");
  assert.throws(() => T.setTransition(p, b.id, "bounce", 0.5), /tidak dikenal/);
});
