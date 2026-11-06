/*
 * Model timeline CutKit: operasi murni pada objek proyek (bentuknya sama dengan
 * struct `Project` di crates/cutkit-core/src/model.rs) plus riwayat undo/redo.
 * Dipakai UI (sebagai script biasa) dan tes Node (lewat require).
 */
(function (root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  else (root.CK = root.CK || {}).timeline = api;
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  /** Klip tidak boleh lebih pendek dari ini (detik). */
  const MIN_DURATION = 0.1;
  /** Durasi awal klip gambar dan teks. */
  const STILL_DURATION = 3;
  const EPS = 1e-6;

  const RATIOS = {
    "9:16": [1080, 1920],
    "16:9": [1920, 1080],
    "16:9-4k": [3840, 2160],
    "1:1": [1080, 1080],
    "4:5": [1080, 1350],
    "3:4": [1080, 1440],
  };

  /** Label preset kanvas untuk menu. */
  const RATIO_LABELS = {
    "9:16": "Shorts / Reels 9:16 (1080×1920)",
    "16:9": "YouTube 16:9 (1920×1080)",
    "16:9-4k": "YouTube 4K 16:9 (3840×2160)",
    "1:1": "Persegi 1:1 (1080×1080)",
    "4:5": "Feed 4:5 (1080×1350)",
    "3:4": "3:4 (1080×1440)",
  };

  /** Faktor ukuran teks: gaya bawaan dirancang untuk sisi pendek 1080 px. */
  function styleScale(canvas) { return Math.min(canvas.width, canvas.height) / 1080; }

  let counter = 0;
  function uid(prefix) {
    counter += 1;
    return `${prefix}${Date.now().toString(36)}${counter.toString(36)}${Math.floor(Math.random() * 1296).toString(36)}`;
  }

  function defaultTextStyle() {
    return { font: "Impact", size: 96, color: "#ffffff", outline_color: "#000000", outline: 5, shadow: 0, bold: false, italic: false, background: "", align: "center" };
  }

  function defaultCaptionStyle() {
    return {
      font: "Impact", size: 80, color: "#ffffff", highlight_color: "#ffe600", outline_color: "#000000",
      outline: 5, shadow: 2, background: "", bold: false, uppercase: true, words_per_page: 3,
      max_line_chars: 16, y: 0.72, x: 0, highlight_scale: 1.15, animation: "pop",
    };
  }

  function defaultTransform() {
    return { x: 0, y: 0, scale: 1, rotation: 0, opacity: 1 };
  }

  function newProject(name = "Proyek baru", ratio = "9:16") {
    const [width, height] = RATIOS[ratio] || RATIOS["9:16"];
    const k = Math.min(width, height) / 1080;
    const cap = defaultCaptionStyle();
    cap.size = Math.round(cap.size * k);
    cap.outline = Math.round(cap.outline * k * 2) / 2;
    if (width > height) {
      // Video lanskap: subtitle di bawah, lebih banyak kata per halaman.
      Object.assign(cap, { y: 0.86, words_per_page: 6, max_line_chars: 36, size: Math.round(64 * k) });
    }
    return {
      version: 1,
      name,
      canvas: { width, height, fps: 30, background: "#000000" },
      assets: [],
      tracks: [
        { id: "v1", kind: "video", name: "Video utama", muted: false, hidden: false, clips: [] },
        { id: "v2", kind: "video", name: "Overlay", muted: false, hidden: false, clips: [] },
        { id: "a1", kind: "audio", name: "Audio", muted: false, hidden: false, clips: [] },
      ],
      captions: { enabled: false, words: [], style: cap },
    };
  }

  /** Melengkapi field yang hilang (proyek dari versi lama / file buatan tangan). */
  function normalize(p) {
    const base = newProject(p.name || "Proyek");
    p.version = p.version || 1;
    p.canvas = Object.assign({}, base.canvas, p.canvas || {});
    p.assets = Array.isArray(p.assets) ? p.assets : [];
    p.tracks = Array.isArray(p.tracks) && p.tracks.length ? p.tracks : base.tracks;
    for (const t of p.tracks) {
      t.kind = t.kind || "video";
      t.name = t.name || (t.kind === "audio" ? "Audio" : "Video");
      t.muted = !!t.muted;
      t.hidden = !!t.hidden;
      t.clips = Array.isArray(t.clips) ? t.clips : [];
      for (const c of t.clips) normalizeClip(c);
      sortClips(t);
    }
    p.captions = p.captions || {};
    p.captions.enabled = !!p.captions.enabled;
    p.captions.words = Array.isArray(p.captions.words) ? p.captions.words : [];
    p.captions.style = Object.assign(defaultCaptionStyle(), p.captions.style || {});
    return p;
  }

  function normalizeClip(c) {
    c.kind = c.kind || "video";
    c.asset_id = c.asset_id == null ? null : c.asset_id;
    c.start = +c.start || 0;
    c.duration = +c.duration || 0;
    c.in = +c.in || 0;
    c.speed = +c.speed > 0 ? +c.speed : 1;
    c.volume = c.volume == null ? 1 : +c.volume;
    c.muted = !!c.muted;
    c.fade_in = +c.fade_in || 0;
    c.fade_out = +c.fade_out || 0;
    c.fit = c.fit || "contain";
    c.transform = Object.assign(defaultTransform(), c.transform || {});
    c.motion = Object.assign({ kind: "none", amount: 0.15 }, c.motion || {});
    c.background = c.background === "blur" ? "blur" : "none";
    c.chroma = Object.assign({ enabled: false, color: "#00ff00", similarity: 0.3, blend: 0.1 }, c.chroma || {});
    c.transition = Object.assign({ kind: "none", duration: 0.5 }, c.transition || {});
    c.color = Object.assign({ brightness: 1, contrast: 1, saturation: 1, hue: 0, temperature: 0, lut: "" }, c.color || {});
    if (c.kind === "effect") c.effect = Object.assign({ kind: "shake", intensity: 0.5 }, c.effect || {});
    if (c.kind === "text") {
      c.text = c.text || { text: "", style: defaultTextStyle() };
      c.text.style = Object.assign(defaultTextStyle(), c.text.style || {});
    } else if (c.text === undefined) {
      c.text = null;
    }
    return c;
  }

  function clipEnd(c) { return c.start + c.duration; }

  /** Kecepatan efektif (gambar dan teks selalu 1), sama dengan Clip::effective_speed di Rust. */
  function speedOf(c) {
    if (c.kind === "image" || c.kind === "text" || !(c.speed > 0)) return 1;
    return Math.min(10, Math.max(0.1, c.speed));
  }

  /** Posisi di sumber untuk waktu timeline `t` di dalam klip. */
  function sourceTime(c, t) { return c.in + (t - c.start) * speedOf(c); }

  function duration(p) {
    let d = 0;
    for (const t of p.tracks) for (const c of t.clips) d = Math.max(d, clipEnd(c));
    return d;
  }

  function sortClips(track) { track.clips.sort((a, b) => a.start - b.start); }

  function getTrack(p, trackId) { return p.tracks.find((t) => t.id === trackId) || null; }
  function getAsset(p, assetId) { return p.assets.find((a) => a.id === assetId) || null; }

  function findClip(p, clipId) {
    for (const track of p.tracks) {
      const index = track.clips.findIndex((c) => c.id === clipId);
      if (index >= 0) return { track, clip: track.clips[index], index };
    }
    return null;
  }

  function mustFind(p, clipId) {
    const f = findClip(p, clipId);
    if (!f) throw new Error("Klip tidak ditemukan.");
    return f;
  }

  /** Panjang maksimum sisa sumber setelah `in` (Infinity untuk gambar/teks). */
  function sourceLimit(p, clip) {
    if (clip.kind === "image" || clip.kind === "text") return Infinity;
    const a = clip.asset_id ? getAsset(p, clip.asset_id) : null;
    if (!a || !(a.duration > 0)) return Infinity;
    return a.duration;
  }

  /** Klip apa yang boleh ditaruh di track jenis apa. */
  function trackAccepts(track, clip) {
    if (track.kind === "audio") return clip.kind === "audio";
    return clip.kind !== "audio";
  }

  function overlaps(track, start, end, ignoreId) {
    const self = ignoreId ? track.clips.find((c) => c.id === ignoreId) : null;
    const allow = self && transitionActive(self) && Math.abs(start - self.start) < EPS ? self.transition.duration : 0;
    return track.clips.some((c) => {
      if (c.id === ignoreId) return false;
      // Klip berikutnya yang bertransisi boleh menumpuk ujung klip ini.
      const nextAllow = transitionActive(c) && c.start > start && Math.abs(c.start - (end - c.transition.duration)) < EPS ? c.transition.duration : 0;
      const overlap = Math.min(end, clipEnd(c)) - Math.max(start, c.start);
      return overlap > EPS + Math.max(c.start < start ? allow : 0, nextAllow);
    });
  }

  const TRANSITIONS = ["none", "fade", "slide-left", "slide-right", "slide-up", "slide-down", "push-left", "push-right"];
  function transitionActive(c) { return !!c.transition && c.transition.kind !== "none" && TRANSITIONS.includes(c.transition.kind) && c.transition.duration > 0; }

  /** Klip tepat sebelum `clip` di track (berdasarkan waktu mulai). */
  function previousClip(track, clip) {
    let prev = null;
    for (const c of track.clips) if (c !== clip && c.start < clip.start - EPS && (!prev || c.start > prev.start)) prev = c;
    return prev;
  }

  /**
   * Memasang transisi masuk ke klip dari klip sebelumnya yang bersambung. Klip ini dan semua
   * sesudahnya di track maju menumpuk klip sebelumnya sepanjang durasi transisi (seperti CapCut).
   * kind "none" melepas transisi dan mengembalikan posisi.
   */
  function setTransition(p, clipId, kind, duration) {
    const { track, clip } = mustFind(p, clipId);
    if (clip.kind === "audio" || clip.kind === "text") throw new Error("Transisi hanya untuk klip video dan gambar.");
    if (!TRANSITIONS.includes(kind)) throw new Error("Jenis transisi tidak dikenal.");
    const prev = previousClip(track, clip);
    const old = transitionActive(clip) ? clip.transition.duration : 0;
    if (kind !== "none") {
      if (!prev || Math.abs(clipEnd(prev) - old - clip.start) > 0.02) throw new Error("Transisi butuh klip sebelumnya yang menempel langsung di track yang sama.");
    }
    let d = kind === "none" ? 0 : clamp(+duration || 0.5, 0.1, 3);
    if (prev) d = Math.min(d, prev.duration / 2, clip.duration / 2);
    const delta = d - old; // positif = maju menumpuk
    const from = clip.start;
    for (const c of track.clips) if (c.start >= from - EPS) c.start = round(c.start - delta);
    clip.transition = { kind, duration: round(kind === "none" ? 0.5 : d) };
    sortClips(track);
    return clip;
  }

  /**
   * Posisi bebas terdekat dari `desired` di track untuk klip berdurasi `dur`.
   * Kandidat: posisi yang diminta, tepat setelah tiap klip, atau tepat sebelumnya.
   */
  function findFreeStart(track, dur, desired, ignoreId) {
    desired = Math.max(0, desired);
    if (!overlaps(track, desired, desired + dur, ignoreId)) return desired;
    const others = track.clips.filter((c) => c.id !== ignoreId);
    const cands = [];
    for (const c of others) {
      cands.push(clipEnd(c));
      cands.push(c.start - dur);
    }
    let best = null;
    for (const s of cands) {
      if (s < -EPS) continue;
      const st = Math.max(0, s);
      if (overlaps(track, st, st + dur, ignoreId)) continue;
      if (best === null || Math.abs(st - desired) < Math.abs(best - desired)) best = st;
    }
    return best === null ? others.reduce((m, c) => Math.max(m, clipEnd(c)), 0) : best;
  }

  function addTrack(p, kind) {
    const n = p.tracks.filter((t) => t.kind === kind).length + 1;
    const prefix = kind === "audio" ? "a" : "v";
    let id;
    do id = uid(prefix); while (getTrack(p, id));
    const track = { id, kind, name: kind === "audio" ? `Audio ${n}` : `Video ${n}`, muted: false, hidden: false, clips: [] };
    if (kind === "video") {
      // Track video baru masuk tepat di atas track video teratas (sebelum track audio).
      let last = -1;
      p.tracks.forEach((t, i) => { if (t.kind === "video") last = i; });
      p.tracks.splice(last + 1, 0, track);
    } else {
      p.tracks.push(track);
    }
    return track;
  }

  function removeTrack(p, trackId) {
    const t = getTrack(p, trackId);
    if (!t) return false;
    if (t.clips.length) throw new Error("Kosongkan track dulu sebelum menghapusnya.");
    if (p.tracks.filter((x) => x.kind === t.kind).length <= 1) throw new Error("Minimal harus ada satu track " + (t.kind === "audio" ? "audio." : "video."));
    p.tracks.splice(p.tracks.indexOf(t), 1);
    return true;
  }

  /** Menaikkan/menurunkan urutan lapisan track video (dir +1 = ke atas). */
  function moveTrack(p, trackId, dir) {
    const i = p.tracks.findIndex((t) => t.id === trackId);
    if (i < 0) return false;
    const j = i + dir;
    if (j < 0 || j >= p.tracks.length || p.tracks[j].kind !== p.tracks[i].kind) return false;
    [p.tracks[i], p.tracks[j]] = [p.tracks[j], p.tracks[i]];
    return true;
  }

  /** Membuat klip baru dari aset. */
  function clipFromAsset(asset, opts = {}) {
    const kind = asset.kind === "image" ? "image" : asset.kind === "audio" ? "audio" : opts.audioOnly ? "audio" : "video";
    const dur = asset.kind === "image" ? (opts.duration || STILL_DURATION) : Math.max(MIN_DURATION, asset.duration || STILL_DURATION);
    return normalizeClip({
      id: uid("c"),
      kind,
      asset_id: asset.id,
      start: 0,
      duration: dur,
      in: 0,
      fit: opts.fit || "contain",
      transform: Object.assign(defaultTransform(), opts.transform || {}),
    });
  }

  /** Daftar efek: sama dengan EFFECTS di model.rs. */
  const EFFECTS = [
    ["shake", "Getar"], ["punch", "Zoom kejut"], ["flash", "Kilat"], ["bw", "Hitam putih"], ["vignette", "Vignette"],
    ["blur", "Blur"], ["glitch", "Glitch RGB"], ["negate", "Negatif"], ["pixelate", "Sensor piksel"], ["letterbox", "Letterbox"],
  ];

  function effectClip(kind, opts = {}) {
    return normalizeClip({ id: uid("e"), kind: "effect", asset_id: null, start: 0, duration: opts.duration || 1, effect: { kind, intensity: opts.intensity ?? 0.5 } });
  }

  /** Track efek (dibuat di atas semua track video bila belum ada). */
  function effectTrack(p) {
    let t = p.tracks.find((x) => x.kind === "video" && x.role === "effects");
    if (!t) {
      t = addTrack(p, "video");
      t.name = "Efek";
      t.role = "effects";
    }
    return t;
  }

  function textClip(text = "Teks judul", opts = {}) {
    return normalizeClip({
      id: uid("t"),
      kind: "text",
      asset_id: null,
      start: 0,
      duration: opts.duration || STILL_DURATION,
      transform: Object.assign(defaultTransform(), { y: opts.y == null ? -560 : opts.y }),
      text: { text, style: Object.assign(defaultTextStyle(), opts.style || {}) },
    });
  }

  /**
   * Menaruh klip di track. Tanpa `start`, klip ditempel di akhir track.
   * Bila bertabrakan, klip digeser ke slot kosong terdekat.
   */
  function insertClip(p, trackId, clip, start) {
    const track = getTrack(p, trackId);
    if (!track) throw new Error("Track tidak ditemukan.");
    if (!trackAccepts(track, clip)) throw new Error(track.kind === "audio" ? "Track audio hanya untuk klip audio." : "Klip audio harus di track audio.");
    const desired = start == null ? track.clips.reduce((m, c) => Math.max(m, clipEnd(c)), 0) : start;
    clip.start = round(findFreeStart(track, clip.duration, desired, clip.id));
    track.clips.push(clip);
    sortClips(track);
    return clip;
  }

  /** Track pertama yang cocok dan kosong di rentang tersebut; bila tidak ada, buat track baru. */
  function trackFor(p, clip, start, preferId) {
    const fits = (t) => trackAccepts(t, clip) && !overlaps(t, start, start + clip.duration, clip.id);
    const pref = preferId && getTrack(p, preferId);
    if (pref && fits(pref)) return pref;
    const candidates = p.tracks.filter((t) => trackAccepts(t, clip));
    // Overlay (gambar/teks) lebih suka track di atas video utama.
    const ordered = clip.kind === "audio" ? candidates : candidates.slice(1).concat(candidates.slice(0, 1));
    const t = ordered.find(fits);
    return t || addTrack(p, clip.kind === "audio" ? "audio" : "video");
  }

  function round(v) { return Math.round(v * 1e6) / 1e6; }

  /** Memotong klip di waktu timeline `time`. Mengembalikan klip bagian kanan. */
  function splitClip(p, clipId, time) {
    const { track, clip } = mustFind(p, clipId);
    if (time <= clip.start + MIN_DURATION - EPS || time >= clipEnd(clip) - MIN_DURATION + EPS) {
      throw new Error("Posisi potong harus di dalam klip.");
    }
    const offset = round(time - clip.start);
    const right = JSON.parse(JSON.stringify(clip));
    right.id = uid(clip.kind === "text" ? "t" : "c");
    right.start = round(time);
    right.duration = round(clip.duration - offset);
    right.in = clip.kind === "video" || clip.kind === "audio" ? round(clip.in + offset * speedOf(clip)) : clip.in;
    right.fade_in = 0;
    clip.duration = offset;
    clip.fade_out = 0;
    clip.fade_in = Math.min(clip.fade_in, clip.duration);
    right.fade_out = Math.min(right.fade_out, right.duration);
    track.clips.push(right);
    sortClips(track);
    return right;
  }

  /**
   * Memangkas sisi kiri (`edge = "start"`) atau kanan (`"end"`) klip ke waktu timeline `time`.
   * Dibatasi oleh klip tetangga, panjang sumber, dan durasi minimum.
   */
  function trimClip(p, clipId, edge, time) {
    const { track, clip } = mustFind(p, clipId);
    const prevClip = previousClip(track, clip);
    const nextClip = track.clips.filter((c) => c !== clip && c.start > clip.start + EPS).reduce((m, c) => (!m || c.start < m.start ? c : m), null);
    const prevEnd = prevClip ? clipEnd(prevClip) - (transitionActive(clip) ? clip.transition.duration : 0) : 0;
    const nextStart = nextClip ? nextClip.start + (transitionActive(nextClip) ? nextClip.transition.duration : 0) : Infinity;
    const timed = clip.kind === "video" || clip.kind === "audio";
    const sp = speedOf(clip);
    if (edge === "start") {
      const end = clipEnd(clip);
      let lo = prevEnd;
      if (timed) lo = Math.max(lo, clip.start - clip.in / sp);
      const t = clamp(time, lo, end - MIN_DURATION);
      const delta = t - clip.start;
      if (timed) clip.in = round(Math.max(0, clip.in + delta * sp));
      clip.start = round(t);
      clip.duration = round(end - t);
    } else {
      const limit = timed ? (sourceLimit(p, clip) - clip.in) / sp : Infinity;
      const hi = Math.min(nextStart, clip.start + limit);
      const t = clamp(time, clip.start + MIN_DURATION, hi);
      clip.duration = round(t - clip.start);
    }
    clip.fade_in = Math.min(clip.fade_in, clip.duration);
    clip.fade_out = Math.min(clip.fade_out, clip.duration);
    return clip;
  }

  function clamp(v, lo, hi) { return Math.min(Math.max(v, lo), hi); }

  /**
   * Memindah klip ke waktu `start` (dan opsional ke track lain yang cocok).
   * Bila bertabrakan, klip ditaruh di slot kosong terdekat.
   */
  function moveClip(p, clipId, start, trackId) {
    const found = mustFind(p, clipId);
    let target = found.track;
    if (trackId && trackId !== found.track.id) {
      const t = getTrack(p, trackId);
      if (t && trackAccepts(t, found.clip)) target = t;
    }
    const clip = found.clip;
    if (transitionActive(clip) && Math.abs(start - clip.start) > EPS) clip.transition = { kind: "none", duration: 0.5 };
    const follower = found.track.clips.find((c) => c !== clip && transitionActive(c) && previousClip(found.track, c) === clip);
    if (follower && (Math.abs(start - clip.start) > EPS || target !== found.track)) follower.transition = { kind: "none", duration: 0.5 };
    if (target !== found.track) {
      found.track.clips.splice(found.index, 1);
      target.clips.push(clip);
    }
    clip.start = round(findFreeStart(target, clip.duration, start, clip.id));
    sortClips(target);
    return clip;
  }

  /** Menghapus klip. `ripple` menggeser klip setelahnya di track yang sama untuk menutup celah. */
  function deleteClip(p, clipId, ripple = false) {
    const { track, clip, index } = mustFind(p, clipId);
    const follower = track.clips.find((c) => c !== clip && transitionActive(c) && previousClip(track, c) === clip);
    if (follower) {
      // Tumpukan dilepas: klip sesudahnya kembali ke ujung klip yang dihapus.
      const d = follower.transition.duration;
      follower.transition = { kind: "none", duration: 0.5 };
      for (const c of track.clips) if (c.start >= follower.start - EPS) c.start = round(c.start + d);
    }
    track.clips.splice(track.clips.indexOf(clip), 1);
    void index;
    if (ripple) {
      for (const c of track.clips) if (c.start >= clipEnd(clip) - EPS) c.start = round(c.start - clip.duration);
    }
    return clip;
  }

  /** Menukar klip dengan tetangganya di track yang sama (dir -1 = ke kiri, +1 = ke kanan). */
  function swapClip(p, clipId, dir) {
    const { track, index } = mustFind(p, clipId);
    const j = index + dir;
    if (j < 0 || j >= track.clips.length) return false;
    const [a, b] = dir < 0 ? [track.clips[j], track.clips[index]] : [track.clips[index], track.clips[j]];
    // a di kiri, b di kanan: b pindah ke posisi a, a ke setelah b, celah di antara keduanya dipertahankan.
    const gap = b.start - clipEnd(a);
    const s = a.start;
    b.start = round(s);
    a.start = round(s + b.duration + gap);
    sortClips(track);
    return true;
  }

  /** Menutup semua celah di track (seperti track utama yang "magnetis"). */
  function closeGaps(p, trackId) {
    const t = getTrack(p, trackId);
    if (!t) return;
    sortClips(t);
    let pos = 0;
    t.clips.forEach((c, i) => {
      const back = i > 0 && transitionActive(c) ? c.transition.duration : 0;
      c.start = round(pos - back);
      pos = c.start + c.duration;
    });
  }

  /** Mengubah properti klip; nilai dijaga dalam batas yang masuk akal. */
  function updateClip(p, clipId, patch) {
    const { clip } = mustFind(p, clipId);
    for (const [k, v] of Object.entries(patch)) {
      if (k === "transform") Object.assign(clip.transform, v);
      else if (k === "motion") Object.assign(clip.motion, v);
      else if (k === "chroma") Object.assign(clip.chroma, v);
      else if (k === "color") Object.assign(clip.color, v);
      else if (k === "text") {
        clip.text = clip.text || { text: "", style: defaultTextStyle() };
        if (v.text != null) clip.text.text = v.text;
        if (v.style) Object.assign(clip.text.style, v.style);
      } else clip[k] = v;
    }
    clip.volume = clamp(+clip.volume || 0, 0, 4);
    clip.fade_in = clamp(+clip.fade_in || 0, 0, clip.duration);
    clip.fade_out = clamp(+clip.fade_out || 0, 0, clip.duration);
    clip.transform.opacity = clamp(+clip.transform.opacity, 0, 1);
    clip.transform.scale = clamp(+clip.transform.scale || 0.01, 0.01, 20);
    return clip;
  }

  /**
   * Mengubah kecepatan klip video/audio. Bagian sumber yang dipakai tetap sama,
   * jadi durasi di timeline berubah; klip sesudahnya di track yang sama ikut bergeser.
   */
  function setSpeed(p, clipId, speed) {
    const { track, clip } = mustFind(p, clipId);
    if (clip.kind !== "video" && clip.kind !== "audio") throw new Error("Kecepatan hanya untuk klip video dan audio.");
    const next = clamp(+speed || 1, 0.1, 10);
    const span = clip.duration * speedOf(clip);
    const oldEnd = clipEnd(clip);
    clip.speed = round(next);
    clip.duration = round(Math.max(MIN_DURATION, span / next));
    const delta = clipEnd(clip) - oldEnd;
    for (const c of track.clips) if (c !== clip && c.start >= oldEnd - EPS) c.start = round(c.start + delta);
    clip.fade_in = Math.min(clip.fade_in, clip.duration);
    clip.fade_out = Math.min(clip.fade_out, clip.duration);
    return clip;
  }

  /** Mengatur durasi klip gambar/teks (atau video sampai batas sumber). */
  function setDuration(p, clipId, dur) {
    const { clip } = mustFind(p, clipId);
    return trimClip(p, clipId, "end", clip.start + Math.max(MIN_DURATION, dur));
  }

  /**
   * Titik tempel (snap): awal, playhead, dan tepi semua klip kecuali yang dikecualikan.
   * Mengembalikan waktu yang sudah ditempel atau `t` apa adanya.
   */
  function snap(p, t, threshold, opts = {}) {
    const points = [0];
    if (opts.playhead != null) points.push(opts.playhead);
    for (const tr of p.tracks) for (const c of tr.clips) {
      if (opts.exclude && opts.exclude.includes(c.id)) continue;
      points.push(c.start, clipEnd(c));
    }
    if (opts.extra) points.push(...opts.extra);
    let best = t;
    let dist = threshold;
    for (const pt of points) {
      const d = Math.abs(pt - t);
      if (d <= dist) { dist = d; best = pt; }
    }
    return best;
  }

  /** Klip yang aktif di waktu `t` (awal inklusif, akhir eksklusif). */
  function clipsAt(p, t) {
    const out = [];
    for (const track of p.tracks) for (const clip of track.clips) {
      if (t >= clip.start - EPS && t < clipEnd(clip) - EPS) out.push({ track, clip });
    }
    return out;
  }

  /** Gain fade di detik `local` sejak awal klip (0..1). */
  function fadeGain(clip, local) {
    let g = 1;
    if (clip.fade_in > 0 && local < clip.fade_in) g = Math.min(g, Math.max(0, local / clip.fade_in));
    const left = clip.duration - local;
    if (clip.fade_out > 0 && left < clip.fade_out) g = Math.min(g, Math.max(0, left / clip.fade_out));
    return g;
  }

  /** Riwayat undo/redo berbasis snapshot JSON. */
  class History {
    /** limit = jumlah langkah, budget = total ukuran snapshot (karakter); proyek panjang tetap hemat memori. */
    constructor(limit = 200, budget = 64 * 1024 * 1024) {
      this.limit = limit;
      this.budget = budget;
      this.undoStack = [];
      this.redoStack = [];
    }
    _trim() {
      let total = this.undoStack.reduce((n, s) => n + s.length, 0) + this.redoStack.reduce((n, s) => n + s.length, 0);
      while (this.undoStack.length > 1 && (this.undoStack.length > this.limit || total > this.budget)) {
        total -= this.undoStack.shift().length;
      }
    }
    /** Panggil SEBELUM mengubah proyek. */
    record(project) {
      this.redoStack.length = 0;
      this.undoStack.push(JSON.stringify(project));
      this._trim();
    }
    /** Mencatat snapshot JSON yang diambil sebelumnya (untuk perubahan lewat slider/drag). */
    push(json) {
      this.redoStack.length = 0;
      this.undoStack.push(json);
      this._trim();
    }
    /** Membatalkan pencatatan terakhir (mis. operasi ternyata gagal). */
    drop() { this.undoStack.pop(); }
    get canUndo() { return this.undoStack.length > 0; }
    get canRedo() { return this.redoStack.length > 0; }
    undo(project) {
      if (!this.undoStack.length) return null;
      this.redoStack.push(JSON.stringify(project));
      return JSON.parse(this.undoStack.pop());
    }
    redo(project) {
      if (!this.redoStack.length) return null;
      this.undoStack.push(JSON.stringify(project));
      return JSON.parse(this.redoStack.pop());
    }
    clear() { this.undoStack.length = 0; this.redoStack.length = 0; }
  }

  /**
   * Membagi durasi `total` ke daftar visual. Video yang lebih pendek dari jatahnya
   * memakai durasi aslinya, sisanya dibagi rata ke yang lain. Mengembalikan durasi per visual.
   */
  function allocate(total, visuals) {
    const n = visuals.length;
    const out = new Array(n).fill(null);
    let free = visuals.map((_, i) => i);
    let remaining = total;
    for (;;) {
      const slot = remaining / free.length;
      const fixed = free.filter((i) => visuals[i].kind === "video" && visuals[i].duration > 0 && visuals[i].duration < slot);
      if (!fixed.length) {
        for (const i of free) out[i] = slot;
        return out;
      }
      for (const i of fixed) { out[i] = visuals[i].duration; remaining -= visuals[i].duration; }
      free = free.filter((i) => !fixed.includes(i));
      if (!free.length) return out;
    }
  }

  /** Titik potong digeser ke awal kata terdekat (maks `reach` detik) agar pergantian gambar pas dengan ucapan. */
  function snapCuts(cuts, words, reach = 0.6) {
    if (!words || !words.length) return cuts;
    const starts = words.map((w) => w.start).sort((a, b) => a - b);
    return cuts.map((c) => {
      let best = c;
      let d = reach;
      for (const s of starts) {
        const dd = Math.abs(s - c);
        if (dd < d) { d = dd; best = s; }
      }
      return best;
    });
  }

  /** Waktu ketukan (timeline) dari klip audio/video yang asetnya sudah dianalisis. */
  function beatTimes(p, opts = {}) {
    const out = [];
    for (const t of p.tracks) for (const c of t.clips) {
      const a = c.asset_id ? getAsset(p, c.asset_id) : null;
      if (!a || !a.beats || (c.kind !== "audio" && c.kind !== "video")) continue;
      const list = opts.downbeats ? a.beats.downbeats : a.beats.beats;
      const sp = speedOf(c);
      for (const b of list) {
        const t2 = c.start + (b - c.in) / sp;
        if (t2 >= c.start - EPS && t2 <= clipEnd(c) + EPS) out.push(round(t2));
      }
    }
    return out.sort((x, y) => x - y);
  }

  /**
   * Menggeser setiap titik potong antar klip yang bersambung di track ke ketukan terdekat
   * (maks `reach` detik). Klip kiri diperpanjang/dipendekkan, klip kanan maju/mundur
   * bersama sumbernya, sehingga tidak ada celah. Mengembalikan jumlah potongan yang digeser.
   */
  function syncToBeats(p, trackId, beats, reach = 0.5) {
    const track = getTrack(p, trackId);
    if (!track || !beats.length) return 0;
    sortClips(track);
    let moved = 0;
    for (let i = 0; i + 1 < track.clips.length; i++) {
      const a = track.clips[i], b = track.clips[i + 1];
      const cut = clipEnd(a);
      if (Math.abs(b.start - cut) > 0.01) continue; // hanya potongan yang bersambung
      let best = null;
      for (const t of beats) if (Math.abs(t - cut) <= reach && (best === null || Math.abs(t - cut) < Math.abs(best - cut))) best = t;
      if (best === null || Math.abs(best - cut) < 1e-3) continue;
      // Batas: klip tetap >= MIN_DURATION dan sumber video tidak habis.
      const lo = Math.max(a.start + MIN_DURATION, b.kind === "video" || b.kind === "audio" ? b.start - b.in / speedOf(b) : -Infinity);
      const hiA = a.kind === "video" || a.kind === "audio" ? a.start + (sourceLimit(p, a) - a.in) / speedOf(a) : Infinity;
      const hi = Math.min(clipEnd(b) - MIN_DURATION, hiA);
      if (best < lo || best > hi) continue;
      const delta = best - cut;
      a.duration = round(a.duration + delta);
      if (b.kind === "video" || b.kind === "audio") b.in = round(Math.max(0, b.in + delta * speedOf(b)));
      b.start = round(best);
      b.duration = round(b.duration - delta);
      moved++;
    }
    return moved;
  }

  /**
   * Menyusun timeline otomatis ala "voice over + gambar":
   *  - voice (aset audio, opsional) di track audio pertama
   *  - visuals (gambar/video) berurutan di track utama, dibagi sepanjang durasi voice,
   *    titik potong ditempel ke awal kata caption bila ada
   *  - music (opsional) di track audio kedua dengan volume pelan dan fade out
   *  - words (opsional) jadi caption
   * Isi timeline lama dihapus. Mengembalikan durasi total.
   */
  function autoAssemble(p, { voice = null, visuals = [], music = null, words = null, musicVolume = 0.15, fit = "cover", motion = false, beats = null } = {}) {
    if (!visuals.length && !voice) throw new Error("Pilih minimal satu voice over atau gambar/video.");
    for (const t of p.tracks) t.clips = [];
    const wordsEnd = words && words.length ? Math.max(...words.map((w) => w.end)) : 0;
    const natural = visuals.map((v) => (v.kind === "video" && v.duration > 0 ? v.duration : STILL_DURATION));
    const naturalTotal = natural.reduce((s, d) => s + d, 0);
    // Caption yang lebih panjang dari voice over tidak boleh terpotong di akhir video.
    let total = voice ? Math.max(voice.duration, wordsEnd) : Math.max(naturalTotal, wordsEnd);
    total = round(total);
    if (!(total > 0)) throw new Error("Durasi voice over tidak terbaca.");

    if (visuals.length) {
      // Tanpa voice over (dan caption tidak lebih panjang), tiap visual memakai durasi aslinya.
      const durs = !voice && total <= naturalTotal + 1e-6 ? natural : allocate(total, visuals);
      const ends = [];
      let t = 0;
      for (let i = 0; i < durs.length - 1; i++) { t += durs[i]; ends.push(t); }
      // Potongan ditempel ke ketukan musik bila diminta, kalau tidak ke awal kata.
      const cuts = beats && beats.length ? snapCuts(ends, beats.map((t) => ({ start: t }))) : snapCuts(ends, words);
      // Potongan harus tetap naik dan tiap klip minimal MIN_DURATION.
      for (let i = 0; i < cuts.length; i++) {
        const lo = (i ? cuts[i - 1] : 0) + MIN_DURATION;
        cuts[i] = Math.max(cuts[i], lo);
      }
      const bounds = [0, ...cuts, total];
      visuals.forEach((asset, i) => {
        const clip = clipFromAsset(asset, { fit });
        // Gerak otomatis: gambar diam bergantian zoom masuk/keluar agar tidak terasa statis.
        if (motion && asset.kind === "image") clip.motion = { kind: i % 2 ? "zoom-out" : "zoom-in", amount: 0.12 };
        clip.start = round(bounds[i]);
        let dur = round(bounds[i + 1] - bounds[i]);
        if (asset.kind === "video" && asset.duration > 0) dur = Math.min(dur, asset.duration);
        clip.duration = Math.max(MIN_DURATION, dur);
        p.tracks[0].clips.push(clip);
      });
      sortClips(p.tracks[0]);
    }

    const audioTracks = () => p.tracks.filter((tr) => tr.kind === "audio");
    if (voice) {
      const c = clipFromAsset(voice, { audioOnly: true });
      c.duration = round(Math.min(voice.duration, total));
      audioTracks()[0].clips.push(c);
    }
    if (music) {
      let tr = audioTracks()[voice ? 1 : 0];
      if (!tr) tr = addTrack(p, "audio");
      const c = clipFromAsset(music, { audioOnly: true });
      c.duration = round(Math.min(music.duration || total, total));
      c.volume = voice ? musicVolume : 1;
      c.fade_out = Math.min(1.5, c.duration / 3);
      tr.clips.push(c);
      tr.name = "Musik";
    }
    if (words && words.length) {
      p.captions.words = words.map((w) => ({ text: w.text, start: w.start, end: w.end }));
      p.captions.enabled = true;
    }
    return total;
  }

  return {
    EFFECTS, effectClip, effectTrack,
    allocate, snapCuts, autoAssemble, beatTimes, syncToBeats, setTransition, transitionActive, previousClip, TRANSITIONS, speedOf, sourceTime, setSpeed,
    MIN_DURATION, STILL_DURATION, RATIOS, RATIO_LABELS, styleScale,
    uid, newProject, normalize, normalizeClip, defaultTextStyle, defaultCaptionStyle, defaultTransform,
    clipEnd, duration, getTrack, getAsset, findClip, trackAccepts, overlaps, findFreeStart,
    addTrack, removeTrack, moveTrack, clipFromAsset, textClip, insertClip, trackFor,
    splitClip, trimClip, moveClip, deleteClip, swapClip, closeGaps, updateClip, setDuration,
    snap, clipsAt, fadeGain, sourceLimit, History,
  };
});
