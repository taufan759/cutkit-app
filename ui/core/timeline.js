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
    "1:1": [1080, 1080],
    "4:5": [1080, 1350],
    "3:4": [1080, 1440],
    "16:9": [1920, 1080],
  };

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
      max_line_chars: 16, y: 0.72, x: 0, highlight_scale: 1.15,
    };
  }

  function defaultTransform() {
    return { x: 0, y: 0, scale: 1, rotation: 0, opacity: 1 };
  }

  function newProject(name = "Proyek baru", ratio = "9:16") {
    const [width, height] = RATIOS[ratio] || RATIOS["9:16"];
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
      captions: { enabled: false, words: [], style: defaultCaptionStyle() },
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
    c.volume = c.volume == null ? 1 : +c.volume;
    c.muted = !!c.muted;
    c.fade_in = +c.fade_in || 0;
    c.fade_out = +c.fade_out || 0;
    c.fit = c.fit || "contain";
    c.transform = Object.assign(defaultTransform(), c.transform || {});
    if (c.kind === "text") {
      c.text = c.text || { text: "", style: defaultTextStyle() };
      c.text.style = Object.assign(defaultTextStyle(), c.text.style || {});
    } else if (c.text === undefined) {
      c.text = null;
    }
    return c;
  }

  function clipEnd(c) { return c.start + c.duration; }

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
    return track.clips.some((c) => c.id !== ignoreId && start < clipEnd(c) - EPS && end > c.start + EPS);
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
    right.in = clip.kind === "video" || clip.kind === "audio" ? round(clip.in + offset) : clip.in;
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
    const prevEnd = track.clips.filter((c) => c.id !== clip.id && clipEnd(c) <= clip.start + EPS).reduce((m, c) => Math.max(m, clipEnd(c)), 0);
    const nextStart = track.clips.filter((c) => c.id !== clip.id && c.start >= clipEnd(clip) - EPS).reduce((m, c) => Math.min(m, c.start), Infinity);
    const timed = clip.kind === "video" || clip.kind === "audio";
    if (edge === "start") {
      const end = clipEnd(clip);
      let lo = prevEnd;
      if (timed) lo = Math.max(lo, clip.start - clip.in);
      const t = clamp(time, lo, end - MIN_DURATION);
      const delta = t - clip.start;
      if (timed) clip.in = round(Math.max(0, clip.in + delta));
      clip.start = round(t);
      clip.duration = round(end - t);
    } else {
      const limit = timed ? sourceLimit(p, clip) - clip.in : Infinity;
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
    track.clips.splice(index, 1);
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
    for (const c of t.clips) {
      c.start = round(pos);
      pos += c.duration;
    }
  }

  /** Mengubah properti klip; nilai dijaga dalam batas yang masuk akal. */
  function updateClip(p, clipId, patch) {
    const { clip } = mustFind(p, clipId);
    for (const [k, v] of Object.entries(patch)) {
      if (k === "transform") Object.assign(clip.transform, v);
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
    constructor(limit = 200) {
      this.limit = limit;
      this.undoStack = [];
      this.redoStack = [];
    }
    /** Panggil SEBELUM mengubah proyek. */
    record(project) {
      this.undoStack.push(JSON.stringify(project));
      if (this.undoStack.length > this.limit) this.undoStack.shift();
      this.redoStack.length = 0;
    }
    /** Mencatat snapshot JSON yang diambil sebelumnya (untuk perubahan lewat slider/drag). */
    push(json) {
      this.undoStack.push(json);
      if (this.undoStack.length > this.limit) this.undoStack.shift();
      this.redoStack.length = 0;
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

  return {
    MIN_DURATION, STILL_DURATION, RATIOS,
    uid, newProject, normalize, normalizeClip, defaultTextStyle, defaultCaptionStyle, defaultTransform,
    clipEnd, duration, getTrack, getAsset, findClip, trackAccepts, overlaps, findFreeStart,
    addTrack, removeTrack, moveTrack, clipFromAsset, textClip, insertClip, trackFor,
    splitClip, trimClip, moveClip, deleteClip, swapClip, closeGaps, updateClip, setDuration,
    snap, clipsAt, fadeGain, sourceLimit, History,
  };
});
