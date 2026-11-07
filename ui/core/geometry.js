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

  /** Easing smoothstep, sama dengan ekspresi ffmpeg di export.rs. */
  function ease(e) { e = Math.min(1, Math.max(0, e)); return e * e * (3 - 2 * e); }

  /* Transisi: konstanta dan rumus sama dengan crates/cutkit-core/src/transition.rs. */
  const TRANSITION = { ZOOM_FROM: 1.25, SPIN_SCALE_FROM: 0.6, BLUR_MAX: 0.02, WHIP_BLUR_MAX: 0.03, PIXEL_MAX: 0.05, GLITCH_MAX: 0.025, MASK_DIV: 8 };
  const isPush = (k) => k.startsWith("push-") || k.startsWith("whip-");
  const isMask = (k) => k.startsWith("wipe-") || k === "clock" || k === "iris" || k === "dissolve";
  const swapsAtHalf = (k) => k === "dip-black" || k === "dip-white" || k === "pixelate" || k === "glitch";
  const fadesAlpha = (k) => k === "fade" || k === "blur" || k === "zoom" || k === "spin";
  const kindEase = (k, p) => (k.startsWith("whip-") ? ease(ease(p)) : ease(p));

  /** Apakah sel mask (X, Y) dari mask WxH sudah memperlihatkan klip baru pada kemajuan e (sama dengan mask_geq). */
  function maskReveal(kind, e, X, Y, W, H) {
    const x = X + 0.5, y = Y + 0.5;
    switch (kind) {
      case "wipe-left": return x >= W * (1 - e);
      case "wipe-right": return x <= W * e;
      case "wipe-up": return y >= H * (1 - e);
      case "wipe-down": return y <= H * e;
      case "clock": { let a = Math.atan2(x - W / 2, H / 2 - y); if (a < 0) a += 2 * Math.PI; return a < 2 * Math.PI * e; }
      case "iris": return Math.hypot(x - W / 2, y - H / 2) <= e * Math.hypot(W / 2, H / 2);
      default: { const n = Math.sin(X * 12.9898 + Y * 78.233) * 43758.5453; return n - Math.floor(n) < e; }
    }
  }

  function postAmount(kind, p, cw) {
    const bump = Math.sin(Math.PI * Math.min(1, Math.max(0, p)));
    const k = kind === "blur" ? TRANSITION.BLUR_MAX : kind.startsWith("whip-") ? TRANSITION.WHIP_BLUR_MAX : kind === "pixelate" ? TRANSITION.PIXEL_MAX : kind === "glitch" ? TRANSITION.GLITCH_MAX : 0;
    return k * cw * bump;
  }

  /**
   * Efek transisi di waktu t untuk satu klip: alpha, geser (dx/dy, piksel kanvas), skala dan putaran
   * di sekitar pusat klip, terlihat atau belum, mask, efek komposisi (post), dan lapisan warna (dip).
   * `incoming` = transisi klip ini; `outgoing` = transisi klip berikutnya (dorong/whip).
   */
  function transitionAt(t, start, incoming, outgoing, outgoingStart, cw, ch) {
    const r = { alpha: 1, dx: 0, dy: 0, scale: 1, rot: 0, visible: true, mask: null, post: null, dip: null };
    if (incoming && incoming.kind !== "none") {
      const k = incoming.kind;
      const p = Math.min(1, Math.max(0, (t - start) / Math.max(0.001, incoming.duration)));
      const e = kindEase(k, p);
      if (fadesAlpha(k)) r.alpha = p;
      switch (k) {
        case "slide-left": case "push-left": case "whip-left": r.dx = cw * (1 - e); break;
        case "slide-right": case "push-right": case "whip-right": r.dx = -cw * (1 - e); break;
        case "slide-up": r.dy = ch * (1 - e); break;
        case "slide-down": r.dy = -ch * (1 - e); break;
        case "zoom": r.scale = 1 + (TRANSITION.ZOOM_FROM - 1) * (1 - e); break;
        case "spin": r.scale = TRANSITION.SPIN_SCALE_FROM + (1 - TRANSITION.SPIN_SCALE_FROM) * e; r.rot = (-Math.PI / 2) * (1 - e); break;
      }
      if (swapsAtHalf(k)) r.visible = p >= 0.5;
      if (isMask(k) && p < 1) r.mask = { kind: k, e };
      if (p > 0 && p < 1 && (k === "blur" || k.startsWith("whip-") || k === "pixelate" || k === "glitch")) r.post = { kind: k, amount: postAmount(k, p, cw) };
      if ((k === "dip-black" || k === "dip-white") && p > 0 && p < 1) r.dip = { color: k === "dip-black" ? "#000000" : "#ffffff", alpha: p < 0.5 ? 2 * p : 2 - 2 * p };
    }
    if (outgoing && isPush(outgoing.kind)) {
      const p = (t - outgoingStart) / Math.max(0.001, outgoing.duration);
      const e = kindEase(outgoing.kind, Math.min(1, Math.max(0, p)));
      r.dx += outgoing.kind.endsWith("-left") ? -cw * e : cw * e;
    }
    return r;
  }

  return { ease, transitionAt, maskReveal, postAmount, TRANSITION, place, hitTest, corners, rround, motionAt, motionActive, motionRect, MOTIONS, colorKey, hexToRgb };
});
