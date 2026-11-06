/*
 * Penempatan klip di kanvas. Rumus sama persis dengan
 * crates/cutkit-core/src/layout.rs agar preview sama dengan hasil ekspor.
 */
(function (root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  else (root.CK = root.CK || {}).geometry = api;
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  /** Pembulatan menjauhi nol, sama dengan f64::round di Rust. */
  function rround(v) { return Math.sign(v) * Math.round(Math.abs(v)); }

  function place(cw, ch, aw, ah, fit, t) {
    aw = Math.max(1, aw || 1);
    ah = Math.max(1, ah || 1);
    const sx = cw / aw;
    const sy = ch / ah;
    const base = fit === "cover" ? Math.max(sx, sy) : Math.min(sx, sy);
    const s = base * Math.max(0.001, t.scale);
    const w = Math.max(1, rround(aw * s));
    const h = Math.max(1, rround(ah * s));
    const rad = (t.rotation * Math.PI) / 180;
    const sin = Math.abs(Math.sin(rad));
    const cos = Math.abs(Math.cos(rad));
    const ow = Math.max(1, Math.ceil(w * cos + h * sin - 1e-6));
    const oh = Math.max(1, Math.ceil(w * sin + h * cos - 1e-6));
    const cx = cw / 2 + t.x;
    const cy = ch / 2 + t.y;
    return { w, h, ow, oh, x: rround(cx - ow / 2), y: rround(cy - oh / 2), cx, cy, rad };
  }

  /** Apakah titik kanvas (px, py) berada di dalam klip yang diputar. */
  function hitTest(pl, px, py) {
    const dx = px - pl.cx;
    const dy = py - pl.cy;
    const c = Math.cos(-pl.rad);
    const s = Math.sin(-pl.rad);
    const lx = dx * c - dy * s;
    const ly = dx * s + dy * c;
    return Math.abs(lx) <= pl.w / 2 && Math.abs(ly) <= pl.h / 2;
  }

  /** Titik sudut klip setelah diputar (urutan: kiri-atas, kanan-atas, kanan-bawah, kiri-bawah). */
  function corners(pl) {
    const c = Math.cos(pl.rad);
    const s = Math.sin(pl.rad);
    return [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([sx, sy]) => {
      const x = (sx * pl.w) / 2;
      const y = (sy * pl.h) / 2;
      return [pl.cx + x * c - y * s, pl.cy + x * s + y * c];
    });
  }

  const MOTIONS = ["none", "zoom-in", "zoom-out", "pan-left", "pan-right", "pan-up", "pan-down"];

  /** Zoom dan titik fokus (0..1) pada kemajuan u; sama dengan Motion::at di Rust. */
  function motionAt(m, u) {
    u = Math.min(1, Math.max(0, u));
    const a = Math.min(2, Math.max(0, (m && m.amount) || 0));
    switch (m && m.kind) {
      case "zoom-in": return [1 + a * u, 0.5, 0.5];
      case "zoom-out": return [1 + a * (1 - u), 0.5, 0.5];
      case "pan-left": return [1 + a, 1 - u, 0.5];
      case "pan-right": return [1 + a, u, 0.5];
      case "pan-up": return [1 + a, 0.5, 1 - u];
      case "pan-down": return [1 + a, 0.5, u];
      default: return [1, 0.5, 0.5];
    }
  }

  function motionActive(m) { return !!m && MOTIONS.includes(m.kind) && m.kind !== "none" && m.amount > 0; }

  /** Potongan sumber (sx, sy, sw, sh) yang terlihat untuk gerak `m` pada kemajuan u. */
  function motionRect(m, u, srcW, srcH) {
    const [z, px, py] = motionAt(m, u);
    const sw = srcW / z;
    const sh = srcH / z;
    return [(srcW - sw) * px, (srcH - sh) * py, sw, sh];
  }

  /**
   * Chroma key pada data RGBA (Uint8ClampedArray), rumus sama dengan filter colorkey ffmpeg:
   * jarak warna ternormalisasi, transparan di bawah similarity, peralihan selebar blend.
   */
  function colorKey(data, rgb, similarity, blend) {
    const [kr, kg, kb] = rgb;
    const norm = 255 * 255 * 3;
    for (let i = 0; i < data.length; i += 4) {
      const dr = data[i] - kr;
      const dg = data[i + 1] - kg;
      const db = data[i + 2] - kb;
      const diff = Math.sqrt((dr * dr + dg * dg + db * db) / norm);
      const a = blend > 0.0001 ? Math.min(1, Math.max(0, (diff - similarity) / blend)) * 255 : diff > similarity ? 255 : 0;
      if (a < data[i + 3]) data[i + 3] = a;
    }
    return data;
  }

  function hexToRgb(hex) {
    const h = String(hex || "").replace("#", "");
    return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) || 0);
  }

  return { place, hitTest, corners, rround, motionAt, motionActive, motionRect, MOTIONS, colorKey, hexToRgb };
});
