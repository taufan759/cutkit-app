/*
 * Caption kata-per-kata: parser SRT, parser JSON waktu per kata, pembagi teks
 * manual, dan pembagian halaman/baris. `paginate` dan `cues` HARUS sama dengan
 * crates/cutkit-core/src/ass.rs supaya preview cocok dengan hasil ekspor.
 */
(function (root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  else (root.CK = root.CK || {}).captions = api;
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  const PAGE_GAP = 1.5;
  /** Sama dengan POP_FACTOR / POP_MS di model.rs. */
  const POP_FACTOR = 1.25;
  const POP_MS = 120;

  /** Skala kata aktif `since` detik setelah kata mulai (animasi pop libass \t linear). */
  function activeScale(style, since) {
    const hl = style.highlight_scale || 1;
    if (style.animation !== "pop") return hl;
    // libass membulatkan skala ke persen bulat, sama seperti di sini.
    const from = Math.round(hl * POP_FACTOR * 100) / 100;
    const to = Math.round(hl * 100) / 100;
    const u = Math.min(1, Math.max(0, (since * 1000) / POP_MS));
    return from + (to - from) * u;
  }

  function round3(v) { return Math.round(v * 1000) / 1000; }

  /** "00:01:02,345" / "01:02.5" / "62.3" -> detik. */
  function parseTime(s) {
    const m = String(s).trim().match(/^(?:(\d+):)?(?:(\d+):)?(\d+)(?:[.,](\d+))?$/);
    if (!m) return NaN;
    let h = 0, min = 0;
    if (m[1] != null && m[2] != null) { h = +m[1]; min = +m[2]; }
    else if (m[1] != null) min = +m[1];
    const frac = m[4] ? +("0." + m[4]) : 0;
    return h * 3600 + min * 60 + +m[3] + frac;
  }

  /** Mengurai teks SRT menjadi daftar cue { start, end, text }. Toleran terhadap format yang kurang rapi. */
  function parseSrt(text) {
    const src = String(text).replace(/^﻿/, "").replace(/\r\n?/g, "\n");
    const blocks = src.split(/\n\s*\n/);
    const out = [];
    for (const block of blocks) {
      const lines = block.split("\n").map((l) => l.trimEnd()).filter((l) => l.trim() !== "");
      const ti = lines.findIndex((l) => l.includes("-->"));
      if (ti < 0) continue;
      const [a, b] = lines[ti].split("-->");
      const start = parseTime(a);
      const end = parseTime(String(b).trim().split(/\s+/)[0]);
      if (!isFinite(start) || !isFinite(end) || end <= start) continue;
      const body = lines.slice(ti + 1).join("\n")
        .replace(/<[^>]+>/g, "")
        .replace(/\{\\[^}]*\}/g, "")
        .trim();
      if (body) out.push({ start: round3(start), end: round3(end), text: body });
    }
    out.sort((x, y) => x.start - y.start);
    return out;
  }

  function splitWords(text) {
    return String(text).replace(/\s+/g, " ").trim().split(" ").filter(Boolean);
  }

  /**
   * Membagi rentang [start, end] ke kata-kata secara proporsional terhadap
   * panjang huruf (kata panjang diucapkan lebih lama).
   */
  function distribute(text, start, end) {
    const words = splitWords(text);
    if (!words.length || !(end > start)) return [];
    const weights = words.map((w) => Math.max(1, w.replace(/[^\p{L}\p{N}]/gu, "").length) + 1);
    const total = weights.reduce((a, b) => a + b, 0);
    const span = end - start;
    let t = start;
    return words.map((w, i) => {
      const d = (span * weights[i]) / total;
      const word = { text: w, start: round3(t), end: round3(i === words.length - 1 ? end : t + d) };
      t += d;
      return word;
    });
  }

  function srtToWords(cues) {
    return cues.flatMap((c) => distribute(c.text, c.start, c.end));
  }

  /**
   * Mengurai JSON waktu per kata. Format yang diterima:
   *  - { "words": [{ "text", "start", "end" }] }            (format CutKit / VoiceKit)
   *  - [{ "text"|"word", "start", "end" }]                  (larik langsung)
   *  - { "beats" | "segments": [{ "text", "start", "end" }] } (per beat/kalimat, dibagi rata per kata)
   *  - { "characters", "character_start_times_seconds", "character_end_times_seconds" }
   *    (alignment ElevenLabs, boleh dibungkus "alignment" / "normalized_alignment")
   */
  function parseWordsJson(text) {
    let data;
    try {
      data = JSON.parse(String(text).replace(/^﻿/, ""));
    } catch (e) {
      throw new Error("File JSON tidak valid: " + e.message);
    }
    if (data && !Array.isArray(data)) {
      const al = data.alignment || data.normalized_alignment || data;
      if (Array.isArray(al.characters) && Array.isArray(al.character_start_times_seconds)) {
        return alignmentToWords(al.characters, al.character_start_times_seconds, al.character_end_times_seconds || al.character_start_times_seconds);
      }
    }
    let list = Array.isArray(data) ? data : data && (data.words || data.beats || data.segments);
    if (!Array.isArray(list)) throw new Error('JSON harus berisi "words": [{ "text", "start", "end" }].');
    const out = [];
    for (const item of list) {
      if (!item) continue;
      const t = item.text != null ? item.text : item.word;
      const start = num(item.start);
      const end = num(item.end);
      if (t == null || !isFinite(start) || !isFinite(end) || end <= start) continue;
      const words = splitWords(t);
      if (words.length === 1) out.push({ text: words[0], start: round3(start), end: round3(end) });
      else out.push(...distribute(t, start, end));
    }
    if (!out.length) throw new Error("Tidak ada kata dengan waktu yang valid di file JSON.");
    out.sort((a, b) => a.start - b.start);
    return out;
  }

  function num(v) {
    if (typeof v === "number") return v;
    if (typeof v === "string") return v.includes(":") ? parseTime(v) : parseFloat(v);
    return NaN;
  }

  function alignmentToWords(chars, starts, ends) {
    const out = [];
    let cur = null;
    for (let i = 0; i < chars.length; i++) {
      const ch = chars[i];
      if (/\s/.test(ch)) {
        if (cur) { out.push(cur); cur = null; }
        continue;
      }
      if (!cur) cur = { text: "", start: round3(+starts[i]), end: round3(+ends[i]) };
      cur.text += ch;
      cur.end = round3(+ends[i]);
    }
    if (cur) out.push(cur);
    return out.filter((w) => w.end > w.start || (w.end = w.start + 0.05));
  }

  /** Teks yang diketik manual dibagi rata ke rentang waktu. */
  function textToWords(text, start, end) {
    return distribute(text, start, end);
  }

  /** Kata yang valid, diurutkan berdasarkan waktu mulai. */
  function sortedWords(words) {
    return words.filter((w) => String(w.text).trim() && w.end > w.start).slice().sort((a, b) => a.start - b.start);
  }

  function paginate(words, wordsPerPage, maxLineChars) {
    const perPage = Math.max(1, wordsPerPage | 0);
    const maxChars = Math.max(1, maxLineChars | 0);
    const pages = [];
    let current = [];
    words.forEach((w, i) => {
      const last = current[current.length - 1];
      const gapBreak = last != null && w.start - words[last].end > PAGE_GAP;
      if (current.length >= perPage || gapBreak) { pages.push(current); current = []; }
      current.push(i);
    });
    if (current.length) pages.push(current);
    return pages.map((idx) => {
      const lines = [];
      let line = [];
      let len = 0;
      for (const i of idx) {
        const wl = [...String(words[i].text)].length;
        if (line.length && len + 1 + wl > maxChars) { lines.push(line); line = []; len = 0; }
        len += line.length ? wl + 1 : wl;
        line.push(i);
      }
      if (line.length) lines.push(line);
      return { lines };
    });
  }

  function cues(words, pages) {
    const out = [];
    pages.forEach((page, pi) => {
      const idx = page.lines.flat();
      idx.forEach((wi, k) => {
        const start = words[wi].start;
        let end = k + 1 < idx.length ? words[idx[k + 1]].start : words[wi].end;
        if (end <= start) end = start + 0.05;
        out.push({ page: pi, active: wi, start, end });
      });
    });
    return out;
  }

  /** Halaman dan kata aktif yang tampil di waktu `t`, atau null. */
  function frameAt(words, style, t) {
    const sorted = sortedWords(words);
    const pages = paginate(sorted, style.words_per_page, style.max_line_chars);
    const cs = cues(sorted, pages);
    const cue = cs.find((c) => t >= c.start && t < c.end);
    if (!cue) return null;
    const page = pages[cue.page];
    return {
      since: t - cue.start,
      lines: page.lines.map((line) => line.map((i) => ({ text: style.uppercase ? String(sorted[i].text).toUpperCase() : String(sorted[i].text), active: i === cue.active }))),
    };
  }

  function srtTime(t) {
    const ms = Math.round(Math.max(0, t) * 1000);
    const h = Math.floor(ms / 3600000);
    const m = Math.floor(ms / 60000) % 60;
    const s = Math.floor(ms / 1000) % 60;
    const pad = (n, w = 2) => String(n).padStart(w, "0");
    return `${pad(h)}:${pad(m)}:${pad(s)},${pad(ms % 1000, 3)}`;
  }

  /** SRT per halaman caption (berguna untuk subtitle YouTube). */
  function toSrt(words, style) {
    const sorted = sortedWords(words);
    const pages = paginate(sorted, style.words_per_page, style.max_line_chars);
    return pages.map((page, i) => {
      const idx = page.lines.flat();
      const start = sorted[idx[0]].start;
      const end = sorted[idx[idx.length - 1]].end;
      const text = page.lines.map((l) => l.map((w) => sorted[w].text).join(" ")).join("\n");
      return `${i + 1}\n${srtTime(start)} --> ${srtTime(end)}\n${text}\n`;
    }).join("\n");
  }

  /** Format JSON kata (yang juga bisa diimpor ulang). */
  function toWordsJson(words) {
    return JSON.stringify({ words: sortedWords(words).map((w) => ({ text: w.text, start: w.start, end: w.end })) }, null, 2);
  }

  function normTok(s) {
    return String(s).toLowerCase().normalize("NFKD").replace(/[\u0300-\u036f]/g, "").replace(/[^\p{L}\p{N}]/gu, "");
  }

  /** Kemiripan dua kata 0..1 (1 - jarak edit relatif). */
  function similarity(a, b) {
    if (a === b) return 1;
    if (!a.length || !b.length) return 0;
    const prev = new Array(b.length + 1);
    for (let j = 0; j <= b.length; j++) prev[j] = j;
    for (let i = 1; i <= a.length; i++) {
      let diag = prev[0];
      prev[0] = i;
      for (let j = 1; j <= b.length; j++) {
        const tmp = prev[j];
        prev[j] = Math.min(prev[j] + 1, prev[j - 1] + 1, diag + (a[i - 1] === b[j - 1] ? 0 : 1));
        diag = tmp;
      }
    }
    return 1 - prev[b.length] / Math.max(a.length, b.length);
  }

  /**
   * Menyelaraskan naskah yang benar dengan kata hasil transkripsi (yang waktunya benar).
   * Hasil: kata-kata naskah dengan waktu dari kata transkripsi yang cocok; kata naskah
   * tanpa pasangan mendapat waktu dari interpolasi tetangganya. Pencocokan memakai DP
   * dalam pita di sekitar diagonal supaya tetap ringan untuk video panjang.
   */
  function alignScript(script, timed) {
    const sw = splitWords(script);
    const tw = sortedWords(timed);
    if (!sw.length) return [];
    if (!tw.length) return [];
    const a = sw.map(normTok);
    const b = tw.map((w) => normTok(w.text));
    const n = a.length;
    const m = b.length;
    const band = Math.max(60, Math.ceil(Math.abs(n - m) * 1.2) + 40);
    const center = (i) => Math.round((i * m) / n);
    const lo = (i) => Math.max(0, center(i) - band);
    const hi = (i) => Math.min(m, center(i) + band);
    const INF = 1e9;
    // cost[i][j] untuk j dalam [lo(i), hi(i)], disimpan per baris.
    const cost = [];
    const move = [];
    for (let i = 0; i <= n; i++) {
      const l = lo(i), h = hi(i);
      const row = new Float64Array(h - l + 1).fill(INF);
      const mv = new Uint8Array(h - l + 1);
      for (let j = l; j <= h; j++) {
        let best = INF, how = 0;
        if (i === 0 && j === 0) best = 0;
        if (i > 0) {
          const pl = lo(i - 1), ph = hi(i - 1);
          const up = j >= pl && j <= ph ? cost[i - 1][j - pl] : INF; // kata naskah tanpa pasangan
          if (up + 1 < best) { best = up + 1; how = 1; }
          if (j > 0 && j - 1 >= pl && j - 1 <= ph) {
            const sim = similarity(a[i - 1], b[j - 1]);
            const d = cost[i - 1][j - 1 - pl] + (1 - sim) * 1.6;
            if (d < best) { best = d; how = 3; }
          }
        }
        if (j > 0 && j - 1 >= l) {
          const left = row[j - 1 - l] + 1; // kata transkripsi tanpa pasangan
          if (left < best) { best = left; how = 2; }
        }
        row[j - l] = best;
        mv[j - l] = how;
      }
      cost.push(row);
      move.push(mv);
    }
    // Telusur balik: pasangan (i -> j) yang mirip dipakai waktunya.
    const match = new Array(n).fill(-1);
    let i = n, j = Math.min(m, hi(n));
    if (j < m) j = hi(n);
    while (i > 0 || j > 0) {
      const how = move[i][j - lo(i)];
      if (how === 3) { if (similarity(a[i - 1], b[j - 1]) >= 0.34) match[i - 1] = j - 1; i--; j--; }
      else if (how === 1) i--;
      else if (how === 2) j--;
      else break;
    }
    const out = sw.map((text, k) => (match[k] >= 0 ? { text, start: tw[match[k]].start, end: tw[match[k]].end } : { text, start: NaN, end: NaN }));
    // Interpolasi kata tanpa pasangan di antara tetangga yang punya waktu.
    const first = tw[0].start, last = tw[tw.length - 1].end;
    let k = 0;
    while (k < out.length) {
      if (!isNaN(out[k].start)) { k++; continue; }
      let e = k;
      while (e < out.length && isNaN(out[e].start)) e++;
      const from = k > 0 ? out[k - 1].end : first;
      const to = e < out.length ? out[e].start : last;
      const span = Math.max(0.05 * (e - k), to - from);
      const parts = distribute(out.slice(k, e).map((w) => w.text).join(" "), from, from + span);
      parts.forEach((p, q) => { out[k + q].start = p.start; out[k + q].end = p.end; });
      k = e;
    }
    return out.map((w) => ({ text: w.text, start: round3(w.start), end: round3(Math.max(w.end, w.start + 0.05)) }));
  }

  return { PAGE_GAP, POP_FACTOR, alignScript, similarity, POP_MS, activeScale, parseTime, parseSrt, srtToWords, parseWordsJson, textToWords, distribute, sortedWords, paginate, cues, frameAt, toSrt, toWordsJson, srtTime };
});
