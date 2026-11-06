/*
 * Penyesuaian warna per klip, sama dengan crates/cutkit-core/src/color.rs.
 * Preview: filter SVG (feComponentTransfer untuk kontras, feColorMatrix untuk
 * sisanya) di ruang sRGB, dijepit 0..1 tiap langkah seperti pipeline 8-bit ffmpeg.
 * LUT .cube diterapkan per piksel (trilinear) di canvas kecil.
 */
(function (root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  else (root.CK = root.CK || {}).color = api;
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  const DEFAULT = { brightness: 1, contrast: 1, saturation: 1, hue: 0, temperature: 0, lut: "" };

  const PRESETS = [
    { name: "Asli", values: {} },
    { name: "Hangat", values: { temperature: 0.4, saturation: 1.1 } },
    { name: "Dingin", values: { temperature: -0.4, saturation: 1.05 } },
    { name: "Cerah", values: { brightness: 1.1, saturation: 1.2 } },
    { name: "Dramatis", values: { contrast: 1.3, saturation: 0.85, brightness: 0.95 } },
    { name: "Vintage", values: { saturation: 0.75, contrast: 0.9, temperature: 0.3, brightness: 1.05 } },
    { name: "Pudar", values: { contrast: 0.8, brightness: 1.08, saturation: 0.8 } },
    { name: "Hitam putih", values: { saturation: 0, contrast: 1.1 } },
  ];

  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

  function mul(a, b) {
    return [0, 1, 2].map((i) => [0, 1, 2].map((j) => a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j]));
  }

  function saturate(s) {
    return [
      [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
      [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
      [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
    ];
  }

  function hueRotate(deg) {
    const r = (deg * Math.PI) / 180;
    const s = Math.sin(r), c = Math.cos(r);
    return [
      [0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928],
      [0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.140, 0.072 - c * 0.072 - s * 0.283],
      [0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072],
    ];
  }

  /** Matriks gabungan suhu x rona x saturasi x kecerahan (sama dengan ColorAdjust::matrix). */
  function matrix(adj) {
    const a = Object.assign({}, DEFAULT, adj || {});
    const b = clamp(a.brightness, 0, 3);
    const t = clamp(a.temperature, -1, 1);
    const bright = [[b, 0, 0], [0, b, 0], [0, 0, b]];
    const temp = [[1 + 0.15 * t, 0, 0], [0, 1 + 0.03 * t, 0], [0, 0, 1 - 0.15 * t]];
    return mul(temp, mul(hueRotate(a.hue), mul(saturate(clamp(a.saturation, 0, 3)), bright)));
  }

  function isIdentity(adj) {
    const a = Object.assign({}, DEFAULT, adj || {});
    return Math.abs(a.brightness - 1) < 1e-6 && Math.abs(a.contrast - 1) < 1e-6 && Math.abs(a.saturation - 1) < 1e-6 && Math.abs(a.hue) < 1e-6 && Math.abs(a.temperature) < 1e-6 && !String(a.lut || "").trim();
  }

  /** Isi elemen <filter> SVG untuk penyesuaian ini (tanpa LUT). */
  function svgFilterMarkup(adj) {
    const a = Object.assign({}, DEFAULT, adj || {});
    const c = clamp(a.contrast, 0, 3);
    const m = matrix(a);
    const ic = (0.5 * (1 - c)).toFixed(6);
    const parts = [];
    if (Math.abs(c - 1) > 1e-6) {
      parts.push(`<feComponentTransfer><feFuncR type="linear" slope="${c}" intercept="${ic}"/><feFuncG type="linear" slope="${c}" intercept="${ic}"/><feFuncB type="linear" slope="${c}" intercept="${ic}"/></feComponentTransfer>`);
    }
    const values = [...m[0], 0, 0, ...m[1], 0, 0, ...m[2], 0, 0, 0, 0, 0, 1, 0].map((v) => +v.toFixed(6)).join(" ");
    parts.push(`<feColorMatrix type="matrix" values="${values}"/>`);
    return parts.join("");
  }

  /** Mengurai file .cube 3D: { size, data: Float32Array (r tercepat), min, max }. */
  function parseCube(text) {
    let size = 0;
    let min = [0, 0, 0], max = [1, 1, 1];
    const vals = [];
    for (const raw of String(text).split(/\r?\n/)) {
      const line = raw.trim();
      if (!line || line.startsWith("#")) continue;
      const parts = line.split(/\s+/);
      const key = parts[0].toUpperCase();
      if (key === "LUT_3D_SIZE") size = +parts[1];
      else if (key === "DOMAIN_MIN") min = parts.slice(1, 4).map(Number);
      else if (key === "DOMAIN_MAX") max = parts.slice(1, 4).map(Number);
      else if (key === "LUT_1D_SIZE") throw new Error("LUT 1D belum didukung, pakai LUT 3D (.cube).");
      else if (/^[-+.\d]/.test(parts[0]) && parts.length >= 3) vals.push(+parts[0], +parts[1], +parts[2]);
    }
    if (!size || vals.length !== size * size * size * 3) throw new Error("File .cube tidak valid (ukuran LUT tidak cocok dengan jumlah baris).");
    return { size, data: new Float32Array(vals), min, max };
  }

  /** Menerapkan LUT 3D (interpolasi trilinear) ke data RGBA. */
  function applyLut(data, lut) {
    const n = lut.size, d = lut.data, n1 = n - 1;
    const idx = (r, g, b) => 3 * (r + n * (g + n * b));
    for (let i = 0; i < data.length; i += 4) {
      const fr = clamp((data[i] / 255 - lut.min[0]) / (lut.max[0] - lut.min[0]), 0, 1) * n1;
      const fg = clamp((data[i + 1] / 255 - lut.min[1]) / (lut.max[1] - lut.min[1]), 0, 1) * n1;
      const fb = clamp((data[i + 2] / 255 - lut.min[2]) / (lut.max[2] - lut.min[2]), 0, 1) * n1;
      const r0 = Math.floor(fr), g0 = Math.floor(fg), b0 = Math.floor(fb);
      const r1 = Math.min(n1, r0 + 1), g1 = Math.min(n1, g0 + 1), b1 = Math.min(n1, b0 + 1);
      const tr = fr - r0, tg = fg - g0, tb = fb - b0;
      for (let ch = 0; ch < 3; ch++) {
        const c000 = d[idx(r0, g0, b0) + ch], c100 = d[idx(r1, g0, b0) + ch], c010 = d[idx(r0, g1, b0) + ch], c110 = d[idx(r1, g1, b0) + ch];
        const c001 = d[idx(r0, g0, b1) + ch], c101 = d[idx(r1, g0, b1) + ch], c011 = d[idx(r0, g1, b1) + ch], c111 = d[idx(r1, g1, b1) + ch];
        const c00 = c000 + (c100 - c000) * tr, c10 = c010 + (c110 - c010) * tr, c01 = c001 + (c101 - c001) * tr, c11 = c011 + (c111 - c011) * tr;
        const c0 = c00 + (c10 - c00) * tg, c1 = c01 + (c11 - c01) * tg;
        data[i + ch] = clamp(Math.round((c0 + (c1 - c0) * tb) * 255), 0, 255);
      }
    }
    return data;
  }

  return { DEFAULT, PRESETS, matrix, isIdentity, svgFilterMarkup, parseCube, applyLut };
});
