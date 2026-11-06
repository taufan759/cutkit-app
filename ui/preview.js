/*
 * Preview langsung: elemen <video>/<audio> HTML5 (di luar DOM) digambar ke
 * <canvas> per frame bersama gambar, teks, dan caption. Semua koordinat
 * memakai piksel kanvas proyek; resolusi internal kanvas diperkecil sesuai
 * ukuran tampil agar ringan di GPU terintegrasi.
 */
(function () {
  "use strict";
  const { timeline: TL, captions: CAP, geometry: GEO } = window.CK;

  const DRIFT = 0.25; // detik selisih sebelum elemen media di-seek ulang saat play
  const PRELOAD = 1.5; // detik sebelum klip mulai, elemennya sudah disiapkan

  /* ---------- teks ala libass ---------- */

  const metricsCache = new Map();
  const measureCtx = document.createElement("canvas").getContext("2d");

  function fontString(font, px, bold, italic) {
    return `${italic ? "italic " : ""}${bold ? "bold " : ""}${px}px "${font}", "Segoe UI", sans-serif`;
  }

  /**
   * libass menskalakan font sehingga tinggi (ascent+descent) = ukuran font ASS.
   * Canvas memakai ukuran em, jadi dikonversi lewat metrik font.
   */
  function fontMetrics(font, bold, italic) {
    const key = `${font}|${bold}|${italic}`;
    let m = metricsCache.get(key);
    if (!m) {
      measureCtx.font = fontString(font, 100, bold, italic);
      const t = measureCtx.measureText("ÁgjH");
      const asc = t.fontBoundingBoxAscent || 90;
      const desc = t.fontBoundingBoxDescent || 22;
      m = { ratio: 100 / (asc + desc), asc: asc / (asc + desc) };
      metricsCache.set(key, m);
    }
    return m;
  }

  /** Mengosongkan cache metrik (dipanggil saat font selesai dimuat). */
  function resetFonts() { metricsCache.clear(); }

  function cssColor(c, fallback = "#ffffff") {
    if (!c) return fallback;
    const h = c.replace("#", "");
    if (h.length === 8) {
      const a = parseInt(h.slice(6, 8), 16) / 255;
      return `rgba(${parseInt(h.slice(0, 2), 16)},${parseInt(h.slice(2, 4), 16)},${parseInt(h.slice(4, 6), 16)},${a.toFixed(3)})`;
    }
    return c;
  }

  /**
   * Menggambar blok teks berpusat di (0,0) pada ctx yang sudah ditranslasi.
   * `lines` = [[{ text, active }]]. Mengembalikan ukuran blok { w, h }.
   */
  function drawTextBlock(ctx, lines, st, opts = {}) {
    const size = st.size;
    const m = fontMetrics(st.font, st.bold, st.italic);
    const px = size * m.ratio;
    const hlScale = opts.highlightScale || 1;
    ctx.font = fontString(st.font, px, st.bold, st.italic);
    const space = ctx.measureText(" ").width;
    const measured = lines.map((line) => {
      const words = line.map((w) => ({ ...w, width: ctx.measureText(w.text).width * (w.active ? hlScale : 1) }));
      const width = words.reduce((s, w) => s + w.width, 0) + space * Math.max(0, words.length - 1);
      return { words, width };
    });
    const lineH = size;
    const total = lineH * lines.length;
    const blockW = measured.reduce((mx, l) => Math.max(mx, l.width), 0);
    const outline = Math.max(0, st.outline || 0);
    const shadow = Math.max(0, st.shadow || 0);
    const boxed = st.background && st.background.trim() !== "";
    const top = -total / 2;

    const passes = [];
    if (boxed) passes.push("box");
    else if (shadow > 0) passes.push("shadow");
    if (!boxed && outline > 0) passes.push("outline");
    passes.push("fill");

    for (const pass of passes) {
      measured.forEach((line, li) => {
        const y0 = top + li * lineH;
        if (pass === "box") {
          ctx.fillStyle = cssColor(st.background, "rgba(0,0,0,.5)");
          ctx.fillRect(-line.width / 2 - outline, y0 - outline, line.width + outline * 2, lineH + outline * 2);
          return;
        }
        let x = -line.width / 2;
        const baseline = y0 + size * m.asc;
        for (const w of line.words) {
          const s = w.active ? hlScale : 1;
          ctx.save();
          // Seperti libass: kata yang diperbesar tumbuh ke atas dari garis dasar.
          ctx.translate(x, baseline);
          if (s !== 1) ctx.scale(s, s);
          if (pass === "shadow") {
            ctx.fillStyle = "rgba(0,0,0,.5)";
            ctx.lineJoin = "round";
            ctx.strokeStyle = "rgba(0,0,0,.5)";
            ctx.lineWidth = outline * 2 / s;
            ctx.translate(shadow / s, shadow / s);
            if (outline > 0) ctx.strokeText(w.text, 0, 0);
            ctx.fillText(w.text, 0, 0);
          } else if (pass === "outline") {
            ctx.lineJoin = "round";
            ctx.miterLimit = 2;
            ctx.strokeStyle = cssColor(st.outline_color, "#000");
            ctx.lineWidth = (outline * 2) / s;
            ctx.strokeText(w.text, 0, 0);
          } else {
            ctx.fillStyle = w.active && opts.highlightColor ? cssColor(opts.highlightColor) : cssColor(st.color);
            ctx.fillText(w.text, 0, 0);
          }
          ctx.restore();
          x += w.width + space;
        }
      });
    }
    return { w: blockW + outline * 2, h: total + outline * 2 };
  }

  /** Ukuran blok teks tanpa menggambar (untuk hit test). */
  function measureTextBlock(lines, st, hlScale = 1) {
    const m = fontMetrics(st.font, st.bold, st.italic);
    const px = st.size * m.ratio;
    measureCtx.font = fontString(st.font, px, st.bold, st.italic);
    const space = measureCtx.measureText(" ").width;
    let w = 0;
    for (const line of lines) {
      const lw = line.reduce((s, x) => s + measureCtx.measureText(x.text).width * (x.active ? hlScale : 1), 0) + space * Math.max(0, line.length - 1);
      w = Math.max(w, lw);
    }
    const o = Math.max(0, st.outline || 0);
    return { w: w + o * 2, h: st.size * lines.length + o * 2 };
  }

  function textLines(text) {
    return String(text).split(/\r?\n/).map((l) => [{ text: l, active: false }]);
  }

  /* ---------- Preview ---------- */

  class Preview {
    /**
     * @param {HTMLCanvasElement} canvas
     * @param {{ project: () => object, src: (asset) => string, selected: () => string|null, onTime: (t:number)=>void, onEnded: ()=>void }} host
     */
    constructor(canvas, host) {
      this.canvas = canvas;
      this.ctx = canvas.getContext("2d");
      this.host = host;
      this.time = 0;
      this.playing = false;
      this.media = new Map(); // clipId -> { el, src }
      this.images = new Map(); // src -> HTMLImageElement
      this.boxes = []; // hasil hit test frame terakhir, urutan dari bawah ke atas
      this.scale = 1;
      this.dirty = true;
      this.showGuides = false;
      this._loop = this._loop.bind(this);
      requestAnimationFrame(this._loop);
    }

    get project() { return this.host.project(); }

    /**
     * Versi klip yang latar hijaunya sudah dihapus, di canvas kecil seukuran tampilan.
     * Di-cache per klip selama frame sumber dan pengaturannya sama.
     */
    _keyed(clip, source, pl, k) {
      const sw = source.videoWidth || source.naturalWidth;
      const sh = source.videoHeight || source.naturalHeight;
      if (!sw || !sh) return source;
      const ch = clip.chroma;
      const w = Math.max(2, Math.min(sw, Math.round(pl.w * k * 1.5)));
      const h = Math.max(2, Math.round((w * sh) / sw));
      const key = `${source.currentTime || 0}|${ch.color}|${ch.similarity}|${ch.blend}|${w}`;
      this.keyCache = this.keyCache || new Map();
      let e = this.keyCache.get(clip.id);
      if (e && e.key === key) return e.canvas;
      if (!e) {
        const canvas = document.createElement("canvas");
        e = { canvas, ctx: canvas.getContext("2d", { willReadFrequently: true }) };
        this.keyCache.set(clip.id, e);
      }
      e.canvas.width = w;
      e.canvas.height = h;
      e.ctx.clearRect(0, 0, w, h);
      e.ctx.drawImage(source, 0, 0, w, h);
      const img = e.ctx.getImageData(0, 0, w, h);
      GEO.colorKey(img.data, GEO.hexToRgb(ch.color), ch.similarity, ch.blend);
      e.ctx.putImageData(img, 0, 0);
      e.key = key;
      return e.canvas;
    }

    /** Menyesuaikan resolusi internal kanvas dengan ukuran tampil. */
    resize(displayW, displayH) {
      const p = this.project;
      const dpr = window.devicePixelRatio || 1;
      const k = Math.min(1, (displayW * dpr) / p.canvas.width, (displayH * dpr) / p.canvas.height);
      const w = Math.max(2, Math.round(p.canvas.width * k));
      const h = Math.max(2, Math.round(p.canvas.height * k));
      if (this.canvas.width !== w || this.canvas.height !== h) {
        this.canvas.width = w;
        this.canvas.height = h;
      }
      this.scale = k;
      this.canvas.style.width = `${Math.round(p.canvas.width * k / dpr)}px`;
      this.canvas.style.height = `${Math.round(p.canvas.height * k / dpr)}px`;
      this.invalidate();
    }

    invalidate() { this.dirty = true; }

    play() {
      const dur = TL.duration(this.project);
      if (dur <= 0) return;
      if (this.time >= dur - 0.01) this.time = 0;
      this.playing = true;
      this._wallStart = performance.now();
      this._timeStart = this.time;
      this.invalidate();
    }

    pause() {
      this.playing = false;
      for (const m of this.media.values()) if (!m.el.paused) m.el.pause();
      this.seek(this.time);
    }

    toggle() { this.playing ? this.pause() : this.play(); }

    seek(t) {
      this.time = Math.max(0, t);
      if (this.playing) {
        this._wallStart = performance.now();
        this._timeStart = this.time;
      }
      this.invalidate();
      this.host.onTime(this.time);
    }

    /** Membuang elemen media milik klip yang sudah tidak ada. */
    gc() {
      const p = this.project;
      const ids = new Set();
      for (const t of p.tracks) for (const c of t.clips) ids.add(c.id);
      for (const [id, m] of this.media) {
        if (!ids.has(id)) {
          m.el.pause();
          m.el.removeAttribute("src");
          m.el.load();
          this.media.delete(id);
        }
      }
    }

    /** Elemen <video>/<audio> untuk satu klip (dibuat bila belum ada). */
    _mediaFor(clip, asset) {
      const src = this.host.src(asset);
      let m = this.media.get(clip.id);
      if (m && m.src !== src) {
        m.el.pause();
        this.media.delete(clip.id);
        m = null;
      }
      if (!m) {
        const el = document.createElement(asset.kind === "audio" ? "audio" : "video");
        el.preload = "auto";
        el.playsInline = true;
        el.src = src;
        el.addEventListener("seeked", () => this.invalidate());
        el.addEventListener("loadeddata", () => this.invalidate());
        el.addEventListener("error", () => { m.error = true; this.invalidate(); });
        m = { el, src, error: false };
        this.media.set(clip.id, m);
      }
      return m;
    }

    _image(asset) {
      const src = this.host.src(asset);
      let img = this.images.get(src);
      if (!img) {
        img = new Image();
        img.decoding = "async";
        img.onload = () => this.invalidate();
        img.src = src;
        this.images.set(src, img);
      }
      return img;
    }

    _loop(now) {
      if (this.playing) {
        const dur = TL.duration(this.project);
        this.time = this._timeStart + (now - this._wallStart) / 1000;
        if (this.time >= dur) {
          this.time = dur;
          this.playing = false;
          for (const m of this.media.values()) if (!m.el.paused) m.el.pause();
          this.host.onEnded();
        }
        this.host.onTime(this.time);
        this.dirty = true;
      }
      if (this.dirty) {
        this.dirty = false;
        try { this.render(); } catch (e) { console.error(e); }
      }
      requestAnimationFrame(this._loop);
    }

    /** Menyelaraskan elemen media dengan waktu sekarang (play/pause/seek/volume). */
    _syncMedia(p, t) {
      const active = new Set();
      for (const track of p.tracks) {
        for (const clip of track.clips) {
          if (clip.kind === "text" || clip.kind === "image") continue;
          const asset = TL.getAsset(p, clip.asset_id);
          if (!asset || asset.missing) continue;
          const inRange = t >= clip.start && t < TL.clipEnd(clip);
          const soon = this.playing && clip.start > t && clip.start - t < PRELOAD;
          if (!inRange && !soon) continue;
          const m = this._mediaFor(clip, asset);
          const el = m.el;
          const local = inRange ? t - clip.start : 0;
          const speed = TL.speedOf(clip);
          const target = clip.in + local * speed;
          if (el.playbackRate !== speed) el.playbackRate = speed;
          const audible = !track.muted && !clip.muted && clip.volume > 0;
          el.muted = !audible;
          el.volume = Math.max(0, Math.min(1, clip.volume * TL.fadeGain(clip, local)));
          if (inRange) active.add(clip.id);
          if (this.playing && inRange) {
            if (el.paused) {
              if (Math.abs(el.currentTime - target) > 0.05) el.currentTime = target;
              el.play().catch(() => {});
            } else if (Math.abs(el.currentTime - target) > DRIFT * speed) {
              el.currentTime = target;
            }
          } else {
            if (!el.paused) el.pause();
            if (el.readyState >= 1 && !el.seeking && Math.abs(el.currentTime - target) > 0.02) el.currentTime = target;
          }
        }
      }
      for (const [id, m] of this.media) {
        if (!active.has(id) && !m.el.paused) m.el.pause();
      }
    }

    render() {
      const p = this.project;
      const ctx = this.ctx;
      const t = this.time;
      this._syncMedia(p, t);
      const k = this.canvas.width / p.canvas.width;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.fillStyle = p.canvas.background || "#000";
      ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
      ctx.setTransform(k, 0, 0, k, 0, 0);
      ctx.imageSmoothingQuality = "medium";
      const boxes = [];
      const textJobs = [];

      p.tracks.forEach((track, ti) => {
        if (track.kind !== "video" || track.hidden) return;
        for (const clip of track.clips) {
          if (!(t >= clip.start && t < TL.clipEnd(clip))) continue;
          if (clip.kind === "text") { textJobs.push({ clip, ti }); continue; }
          const asset = TL.getAsset(p, clip.asset_id);
          if (!asset) continue;
          const pl = GEO.place(p.canvas.width, p.canvas.height, asset.width, asset.height, clip.fit, clip.transform);
          boxes.push({ id: clip.id, pl, kind: clip.kind });
          let source = null;
          if (asset.missing) source = null;
          else if (clip.kind === "image") {
            const img = this._image(asset);
            if (img.complete && img.naturalWidth) source = img;
          } else {
            const m = this.media.get(clip.id);
            if (m && m.el.readyState >= 2) source = m.el;
          }
          if (source && clip.background === "blur") {
            // Sama dengan ekspor: cover ke kanvas lalu boxblur (radius kanvas/27, dua putaran ~ gaussian 0.8r).
            const sw = source.videoWidth || source.naturalWidth || 1;
            const sh = source.videoHeight || source.naturalHeight || 1;
            const s = Math.max(p.canvas.width / sw, p.canvas.height / sh);
            const r = Math.max(4, Math.round(Math.min(p.canvas.width, p.canvas.height) / 27));
            ctx.save();
            ctx.filter = `blur(${(r * 0.8 * k).toFixed(1)}px)`;
            ctx.drawImage(source, (p.canvas.width - sw * s) / 2, (p.canvas.height - sh * s) / 2, sw * s, sh * s);
            ctx.restore();
          }
          if (source && clip.chroma && clip.chroma.enabled) source = this._keyed(clip, source, pl, k);
          ctx.save();
          ctx.globalAlpha = Math.max(0, Math.min(1, clip.transform.opacity));
          ctx.translate(pl.cx, pl.cy);
          ctx.rotate(pl.rad);
          if (source && GEO.motionActive(clip.motion)) {
            const sw = source.videoWidth || source.naturalWidth || source.width || pl.w;
            const sh = source.videoHeight || source.naturalHeight || source.height || pl.h;
            const [sx, sy, rw, rh] = GEO.motionRect(clip.motion, (t - clip.start) / clip.duration, sw, sh);
            ctx.drawImage(source, sx, sy, rw, rh, -pl.w / 2, -pl.h / 2, pl.w, pl.h);
          } else if (source) ctx.drawImage(source, -pl.w / 2, -pl.h / 2, pl.w, pl.h);
          else {
            ctx.fillStyle = asset.missing ? "rgba(200,40,40,.35)" : "rgba(255,255,255,.06)";
            ctx.fillRect(-pl.w / 2, -pl.h / 2, pl.w, pl.h);
          }
          ctx.restore();
        }
      });

      // Teks selalu di atas media (sama seperti ekspor: libass dirender terakhir).
      for (const { clip } of textJobs) {
        const tc = clip.text;
        if (!tc || !tc.text.trim()) continue;
        const tr = clip.transform;
        const lines = textLines(tc.text);
        const size = measureTextBlock(lines, tc.style);
        const pl = { cx: p.canvas.width / 2 + tr.x, cy: p.canvas.height / 2 + tr.y, w: size.w * tr.scale, h: size.h * tr.scale, rad: (tr.rotation * Math.PI) / 180 };
        boxes.push({ id: clip.id, pl, kind: "text" });
        ctx.save();
        ctx.globalAlpha = Math.max(0, Math.min(1, tr.opacity));
        ctx.translate(pl.cx, pl.cy);
        ctx.rotate(pl.rad);
        ctx.scale(tr.scale, tr.scale);
        drawTextBlock(ctx, lines, tc.style);
        ctx.restore();
      }

      // Caption kata-per-kata.
      const cap = p.captions;
      if (cap.enabled && cap.words.length) {
        const st = cap.style;
        const frame = CAP.frameAt(cap.words, st, t);
        const cx = p.canvas.width / 2 + (st.x || 0);
        const cy = p.canvas.height * st.y;
        if (frame) {
          ctx.save();
          ctx.translate(cx, cy);
          const size = drawTextBlock(ctx, frame.lines, st, { highlightColor: st.highlight_color, highlightScale: CAP.activeScale(st, frame.since) });
          ctx.restore();
          boxes.push({ id: "__captions", pl: { cx, cy, w: size.w, h: size.h, rad: 0 }, kind: "captions" });
        } else if (this.host.selected() === "__captions") {
          boxes.push({ id: "__captions", pl: { cx, cy, w: p.canvas.width * 0.6, h: st.size, rad: 0 }, kind: "captions" });
        }
      }

      this.boxes = boxes;
      this._drawSelection(ctx, k);
    }

    _drawSelection(ctx, k) {
      const p = this.project;
      if (this.showGuides) {
        ctx.save();
        ctx.strokeStyle = "rgba(255,255,255,.35)";
        ctx.lineWidth = 1 / k;
        ctx.setLineDash([8 / k, 8 / k]);
        ctx.beginPath();
        ctx.moveTo(p.canvas.width / 2, 0); ctx.lineTo(p.canvas.width / 2, p.canvas.height);
        ctx.moveTo(0, p.canvas.height / 2); ctx.lineTo(p.canvas.width, p.canvas.height / 2);
        ctx.stroke();
        ctx.restore();
      }
      const sel = this.host.selected();
      const box = this.boxes.find((b) => b.id === sel);
      if (!box) return;
      const pl = box.pl;
      const pts = GEO.corners(pl);
      ctx.save();
      ctx.strokeStyle = "#35c9ff";
      ctx.lineWidth = 2 / k;
      ctx.beginPath();
      pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
      ctx.closePath();
      ctx.stroke();
      if (box.kind !== "captions") {
        const hs = 9 / k;
        ctx.fillStyle = "#fff";
        for (const [x, y] of pts) {
          ctx.fillRect(x - hs, y - hs, hs * 2, hs * 2);
          ctx.strokeRect(x - hs, y - hs, hs * 2, hs * 2);
        }
        const r = this.rotateHandle(pl, k);
        ctx.beginPath();
        const top = [(pts[0][0] + pts[1][0]) / 2, (pts[0][1] + pts[1][1]) / 2];
        ctx.moveTo(top[0], top[1]);
        ctx.lineTo(r[0], r[1]);
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(r[0], r[1], hs, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      }
      ctx.restore();
    }

    /** Posisi tombol putar: di atas sisi atas klip. */
    rotateHandle(pl, k = this.canvas.width / this.project.canvas.width) {
      const d = pl.h / 2 + 46 / k;
      return [pl.cx + Math.sin(pl.rad) * d, pl.cy - Math.cos(pl.rad) * d];
    }

    /** Konversi koordinat layar -> kanvas proyek. */
    toCanvas(clientX, clientY) {
      const r = this.canvas.getBoundingClientRect();
      const p = this.project;
      return [((clientX - r.left) / r.width) * p.canvas.width, ((clientY - r.top) / r.height) * p.canvas.height];
    }

    /** Box paling atas di titik (x, y) kanvas. */
    hit(x, y) {
      for (let i = this.boxes.length - 1; i >= 0; i--) {
        if (GEO.hitTest(this.boxes[i].pl, x, y)) return this.boxes[i];
      }
      return null;
    }

    box(id) { return this.boxes.find((b) => b.id === id) || null; }

    /** Mengambil gambar frame saat ini (PNG data URL) untuk tes visual. */
    snapshot() { return this.canvas.toDataURL("image/png"); }
  }

  window.CK.Preview = Preview;
  window.CK.text = { drawTextBlock, measureTextBlock, resetFonts, cssColor };
})();
