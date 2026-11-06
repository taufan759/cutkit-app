/*
 * Tampilan timeline: header track, penggaris waktu, klip (geser, pangkas,
 * pindah track), playhead, snap, dan zoom. Semua perubahan model lewat
 * window.CK.timeline; tampilan ini hanya menggambar dan meneruskan input.
 */
(function () {
  "use strict";
  const { timeline: TL, captions: CAP } = window.CK;

  const ROW_H = { video: 62, audio: 46, captions: 28 };
  const SNAP_PX = 9;
  const MIN_PPS = 0.05; // cukup untuk melihat video 5 jam dalam satu layar
  const MAX_PPS = 800;

  function el(tag, attrs = {}, ...children) {
    const node = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (k === "class") node.className = v;
      else if (k === "style") node.style.cssText = v;
      else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
      else if (v !== false && v != null) node.setAttribute(k, v === true ? "" : v);
    }
    for (const c of children.flat()) if (c != null) node.append(c);
    return node;
  }

  function fmtTime(t, fps = 30) {
    t = Math.max(0, t);
    if (t >= 3600) {
      const hh = Math.floor(t / 3600);
      const mm = Math.floor((t % 3600) / 60);
      const ss = Math.floor(t % 60);
      return `${hh}:${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
    }
    const m = Math.floor(t / 60);
    const s = Math.floor(t % 60);
    const f = Math.floor((t - Math.floor(t)) * fps + 1e-6);
    return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}:${String(f).padStart(2, "0")}`;
  }

  class TimelineView {
    /**
     * host: { project(), time(), seek(t), selected(), select(id), snapshot(), pushHistory(json),
     *         changed(), src(path), assetName(asset), toast(msg, err) }
     */
    constructor(host) {
      this.host = host;
      this.pps = 60;
      this.snapEnabled = true;
      this.heads = document.getElementById("tl-heads");
      this.scroll = document.getElementById("tl-scroll");
      this.content = document.getElementById("tl-content");
      this.rows = document.getElementById("tl-rows");
      this.ruler = document.getElementById("tl-ruler");
      this.playhead = document.getElementById("tl-playhead");
      this.snapline = document.getElementById("tl-snapline");
      this.drag = null;

      this.scroll.addEventListener("scroll", () => {
        this.heads.scrollTop = this.scroll.scrollTop;
        this.drawRuler();
      });
      this.scroll.addEventListener("wheel", (e) => {
        if (e.ctrlKey) {
          e.preventDefault();
          const anchor = this.xToTime(e.clientX);
          this.setZoom(this.pps * (e.deltaY < 0 ? 1.2 : 1 / 1.2), anchor, e.clientX);
        }
      }, { passive: false });
      this.ruler.addEventListener("pointerdown", (e) => this.startScrub(e));
      this.rows.addEventListener("pointerdown", (e) => this.onPointerDown(e));
      this.rows.addEventListener("dblclick", (e) => {
        const c = e.target.closest(".clip");
        if (c && this.host.onClipDblClick) this.host.onClipDblClick(c.dataset.id);
      });
      window.addEventListener("pointermove", (e) => this.onPointerMove(e));
      window.addEventListener("pointerup", (e) => this.onPointerUp(e));
      new ResizeObserver(() => { this.layoutWidth(); this.drawRuler(); }).observe(this.scroll);
    }

    get project() { return this.host.project(); }

    /** Urutan tampil: track video teratas di atas, lalu audio. */
    displayTracks() {
      const p = this.project;
      const video = p.tracks.filter((t) => t.kind === "video").reverse();
      const audio = p.tracks.filter((t) => t.kind === "audio");
      return [...video, ...audio];
    }

    xToTime(clientX) {
      const r = this.content.getBoundingClientRect();
      return Math.max(0, (clientX - r.left) / this.pps);
    }

    timeToX(t) { return t * this.pps; }

    setZoom(pps, anchorTime, anchorClientX) {
      pps = Math.min(MAX_PPS, Math.max(MIN_PPS, pps));
      const sr = this.scroll.getBoundingClientRect();
      const anchorX = anchorClientX != null ? anchorClientX - sr.left : sr.width / 2;
      const t = anchorTime != null ? anchorTime : this.host.time();
      this.pps = pps;
      this.render();
      this.scroll.scrollLeft = Math.max(0, t * pps - anchorX);
      if (this.host.onZoom) this.host.onZoom(pps);
    }

    /** Zoom agar seluruh proyek muat di layar. */
    fit() {
      const dur = Math.max(5, TL.duration(this.project));
      this.setZoom((this.scroll.clientWidth - 40) / dur, 0, this.scroll.getBoundingClientRect().left);
      this.scroll.scrollLeft = 0;
    }

    layoutWidth() {
      const dur = TL.duration(this.project);
      const w = Math.max(this.scroll.clientWidth, (dur + 30) * this.pps);
      this.content.style.width = `${w}px`;
    }

    render() {
      const p = this.project;
      this.layoutWidth();
      const selected = this.host.selected();
      const heads = [];
      const rows = [];

      // Baris caption (virtual): blok per halaman, klik untuk memilih caption.
      if (p.captions.enabled && p.captions.words.length) {
        const words = CAP.sortedWords(p.captions.words);
        const pages = CAP.paginate(words, p.captions.style.words_per_page, p.captions.style.max_line_chars);
        heads.push(el("div", { class: "tl-head captions", style: `height:${ROW_H.captions}px` }, el("span", { class: "name" }, "Caption")));
        rows.push(el("div", { class: "tl-row captions", "data-track": "__captions", style: `height:${ROW_H.captions}px` },
          pages.map((pg) => {
            const idx = pg.lines.flat();
            const s = words[idx[0]].start;
            const e = words[idx[idx.length - 1]].end;
            return el("div", {
              class: "cap-block" + (selected === "__captions" ? " selected" : ""),
              style: `left:${this.timeToX(s)}px;width:${Math.max(2, this.timeToX(e - s))}px`,
              "data-captions": "1",
              title: idx.map((i) => words[i].text).join(" "),
            }, idx.map((i) => words[i].text).join(" "));
          })));
      }

      for (const track of this.displayTracks()) {
        const h = ROW_H[track.kind];
        const isMain = track === p.tracks[0];
        heads.push(el("div", { class: `tl-head ${track.kind}`, style: `height:${h}px`, "data-track": track.id },
          el("span", { class: "name", title: track.name }, track.name),
          el("div", { class: "head-btns" },
            el("button", { class: "mini" + (track.muted ? " on" : ""), title: track.muted ? "Bunyikan track" : "Bisukan track", "data-act": "mute" }, track.muted ? "🔇" : "🔊"),
            track.kind === "video" ? el("button", { class: "mini" + (track.hidden ? " on" : ""), title: track.hidden ? "Tampilkan track" : "Sembunyikan track", "data-act": "hide" }, track.hidden ? "⦸" : "👁") : null,
            el("button", { class: "mini", title: "Menu track", "data-act": "menu" }, "⋯"))));
        rows.push(el("div", { class: `tl-row ${track.kind}` + (track.hidden ? " hidden-track" : "") + (track.muted ? " muted-track" : "") + (isMain ? " main" : ""), "data-track": track.id, style: `height:${h}px` },
          track.clips.map((c) => this.renderClip(track, c, selected === c.id, h))));
      }
      this.heads.replaceChildren(...heads);
      this.rows.replaceChildren(...rows);
      this.heads.querySelectorAll("button[data-act]").forEach((b) => b.addEventListener("click", (e) => {
        const id = b.closest(".tl-head").dataset.track;
        this.host.onTrackAction(id, b.dataset.act, e);
      }));
      this.drawRuler();
      this.updatePlayhead();
    }

    renderClip(track, clip, selected, h) {
      const p = this.project;
      const asset = clip.asset_id ? TL.getAsset(p, clip.asset_id) : null;
      const left = this.timeToX(clip.start);
      const width = Math.max(3, this.timeToX(clip.duration));
      const fxName = clip.kind === "effect" ? (TL.EFFECTS.find(([k]) => k === clip.effect.kind) || [0, "Efek"])[1] : null;
      const name = clip.kind === "text" ? (clip.text && clip.text.text.split("\n")[0]) || "Teks" : fxName ? `✨ ${fxName}` : asset ? asset.name : "(media hilang)";
      const style = [`left:${left}px`, `width:${width}px`];
      const node = el("div", { class: `clip ${clip.kind}` + (selected ? " selected" : "") + (asset && asset.missing ? " missing" : "") + (clip.muted ? " muted" : ""), "data-id": clip.id });
      const bg = el("div", { class: "clip-bg" });
      if (asset && !asset.missing) {
        if (clip.kind === "video" && asset.filmstrip) {
          const full = this.timeToX(asset.duration || clip.duration);
          bg.style.backgroundImage = `url("${this.host.src(asset.filmstrip.path)}")`;
          const sp = TL.speedOf(clip);
          bg.style.backgroundSize = `${full / sp}px 100%`;
          bg.style.backgroundPosition = `${-this.timeToX(clip.in) / sp}px 0`;
          bg.style.backgroundRepeat = "no-repeat";
        } else if (clip.kind === "image") {
          bg.style.backgroundImage = `url("${this.host.src(asset.proxy || asset.path)}")`;
          bg.style.backgroundSize = "auto 100%";
          bg.style.backgroundRepeat = "repeat-x";
          bg.classList.add("dim");
        }
        if ((clip.kind === "audio" || clip.kind === "video") && asset.waveform) {
          const wave = el("div", { class: "clip-wave" });
          const full = this.timeToX(asset.duration || clip.duration);
          wave.style.backgroundImage = `url("${this.host.src(asset.waveform)}")`;
          const ws = TL.speedOf(clip);
          wave.style.backgroundSize = `${full / ws}px 100%`;
          wave.style.backgroundPosition = `${-this.timeToX(clip.in) / ws}px 0`;
          node.append(wave);
        }
      }
      node.prepend(bg);
      node.append(el("div", { class: "clip-label" }, clip.kind === "text" ? "T  " : "", name));
      const badges = [];
      if (Math.abs(TL.speedOf(clip) - 1) > 1e-3) badges.push(`${+TL.speedOf(clip).toFixed(2)}x`);
      if (clip.kind !== "text" && clip.kind !== "image" && Math.abs(clip.volume - 1) > 1e-3) badges.push(`${Math.round(clip.volume * 100)}%`);
      if (badges.length) node.append(el("div", { class: "clip-vol" }, badges.join(" · ")));
      if (asset && asset.beats && (clip.kind === "audio" || clip.kind === "video")) {
        const sp = TL.speedOf(clip);
        const marks = asset.beats.beats.filter((b) => b >= clip.in && b <= clip.in + clip.duration * sp);
        const down = new Set(asset.beats.downbeats);
        // Terlalu rapat untuk dilihat bila zoom jauh: tampilkan ketukan kuat saja.
        const dense = (60 / asset.beats.bpm / sp) * this.pps < 6;
        for (const b of marks) {
          if (dense && !down.has(b)) continue;
          node.append(el("div", { class: "beat" + (down.has(b) ? " down" : ""), style: `left:${this.timeToX((b - clip.in) / sp)}px` }));
        }
      }
      for (const k of clip.keyframes || []) {
        if (k.t < -1e-6 || k.t > clip.duration + 1e-6) continue;
        node.append(el("div", { class: "kf", title: `Keyframe ${k.t.toFixed(2)} s`, style: `left:${this.timeToX(k.t)}px` }));
      }
      if (TL.transitionActive(clip)) {
        node.append(el("div", { class: "trans", title: `Transisi ${clip.transition.kind} ${clip.transition.duration} s`, style: `width:${Math.max(8, this.timeToX(clip.transition.duration))}px` }, "⧓"));
      }
      if (clip.fade_in > 0) node.append(el("div", { class: "fade in", style: `width:${this.timeToX(clip.fade_in)}px` }));
      if (clip.fade_out > 0) node.append(el("div", { class: "fade out", style: `width:${this.timeToX(clip.fade_out)}px` }));
      node.append(el("div", { class: "h-l", title: "Pangkas awal" }), el("div", { class: "h-r", title: "Pangkas akhir" }));
      node.style.cssText = style.join(";");
      return node;
    }

    drawRuler() {
      const c = this.ruler;
      const dpr = window.devicePixelRatio || 1;
      const w = this.scroll.clientWidth;
      const h = 26;
      if (c.width !== Math.round(w * dpr)) { c.width = Math.round(w * dpr); c.height = Math.round(h * dpr); }
      c.style.width = `${w}px`;
      c.style.height = `${h}px`;
      c.style.left = `${this.scroll.scrollLeft}px`;
      const ctx = c.getContext("2d");
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const css = getComputedStyle(document.documentElement);
      ctx.fillStyle = css.getPropertyValue("--surface-2").trim() || "#222";
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = css.getPropertyValue("--muted").trim() || "#999";
      ctx.strokeStyle = ctx.fillStyle;
      ctx.font = "10px Segoe UI, sans-serif";
      const steps = [1 / 30, 0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200];
      const major = steps.find((s) => s * this.pps >= 70) || 14400;
      const minor = major / (major >= 1 ? 5 : 2);
      const start = this.scroll.scrollLeft / this.pps;
      const end = start + w / this.pps;
      ctx.beginPath();
      for (let t = Math.floor(start / minor) * minor; t <= end; t += minor) {
        const x = Math.round((t - start) * this.pps) + 0.5;
        const isMajor = Math.abs(t / major - Math.round(t / major)) < 1e-6;
        ctx.moveTo(x, h);
        ctx.lineTo(x, isMajor ? h - 10 : h - 5);
        if (isMajor) ctx.fillText(major < 1 ? fmtTime(t, this.project.canvas.fps) : t >= 3600 ? fmtTime(t) : fmtTime(t).slice(0, 5), x + 3, 11);
      }
      ctx.stroke();
    }

    updatePlayhead() {
      const x = this.timeToX(this.host.time());
      this.playhead.style.transform = `translateX(${x}px)`;
    }

    /** Menggeser tampilan agar playhead terlihat (saat play). */
    follow() {
      const x = this.timeToX(this.host.time());
      const sl = this.scroll.scrollLeft;
      const w = this.scroll.clientWidth;
      if (x > sl + w - 40 || x < sl) this.scroll.scrollLeft = Math.max(0, x - 60);
    }

    /** Track dan waktu di bawah kursor, untuk drop dari panel media. */
    dropTarget(clientX, clientY) {
      const sr = this.scroll.getBoundingClientRect();
      if (clientX < sr.left || clientX > sr.right || clientY < sr.top || clientY > sr.bottom) return null;
      const row = this.rowAt(clientY);
      return { trackId: row ? row.dataset.track : null, time: this.xToTime(clientX) };
    }

    rowAt(clientY) {
      for (const row of this.rows.children) {
        const r = row.getBoundingClientRect();
        if (clientY >= r.top && clientY < r.bottom) return row;
      }
      return null;
    }

    snapTime(t, exclude, altKey) {
      if (!this.snapEnabled || altKey) return t;
      const p = this.project;
      const extra = [];
      if (p.captions.enabled) for (const w of p.captions.words) extra.push(w.start);
      for (const b of TL.beatTimes(p)) extra.push(b);
      return TL.snap(p, t, SNAP_PX / this.pps, { playhead: this.host.time(), exclude, extra });
    }

    showSnap(t) {
      if (t == null) { this.snapline.classList.add("hidden"); return; }
      this.snapline.classList.remove("hidden");
      this.snapline.style.transform = `translateX(${this.timeToX(t)}px)`;
    }

    startScrub(e) {
      e.preventDefault();
      this.drag = { mode: "scrub" };
      this.host.seek(this.snapTime(this.xToTime(e.clientX), [], e.altKey || true));
    }

    onPointerDown(e) {
      if (e.button !== 0) return;
      if (e.target.closest(".cap-block")) {
        this.host.select("__captions");
        this.drag = { mode: "scrub" };
        this.host.seek(this.xToTime(e.clientX));
        return;
      }
      const clipEl = e.target.closest(".clip");
      if (!clipEl) {
        this.host.select(null);
        this.startScrub(e);
        return;
      }
      e.preventDefault();
      const id = clipEl.dataset.id;
      const found = TL.findClip(this.project, id);
      if (!found) return;
      this.host.select(id);
      const mode = e.target.classList.contains("h-l") ? "trim-start" : e.target.classList.contains("h-r") ? "trim-end" : "move";
      this.drag = {
        mode,
        id,
        snapshot: this.host.snapshot(),
        grab: this.xToTime(e.clientX) - found.clip.start,
        x0: e.clientX,
        y0: e.clientY,
        moved: false,
      };
    }

    onPointerMove(e) {
      const d = this.drag;
      if (!d) return;
      if (d.mode === "scrub") {
        this.host.seek(this.xToTime(e.clientX));
        return;
      }
      if (!d.moved && Math.abs(e.clientX - d.x0) < 3 && Math.abs(e.clientY - d.y0) < 4) return;
      d.moved = true;
      const p = this.project;
      const found = TL.findClip(p, d.id);
      if (!found) return;
      const clip = found.clip;
      const t = this.xToTime(e.clientX);
      let snapped = null;
      if (d.mode === "move") {
        let start = Math.max(0, t - d.grab);
        const s1 = this.snapTime(start, [d.id], e.altKey);
        const s2 = this.snapTime(start + clip.duration, [d.id], e.altKey) - clip.duration;
        if (Math.abs(s1 - start) > 1e-9 && (Math.abs(s1 - start) <= Math.abs(s2 - start) || Math.abs(s2 - start) < 1e-9)) { start = s1; snapped = s1; }
        else if (Math.abs(s2 - start) > 1e-9) { start = s2; snapped = s2 + clip.duration; }
        const row = this.rowAt(e.clientY);
        const trackId = row && row.dataset.track !== "__captions" ? row.dataset.track : found.track.id;
        TL.moveClip(p, d.id, start, trackId);
      } else {
        const st = this.snapTime(t, [d.id], e.altKey);
        if (Math.abs(st - t) > 1e-9) snapped = st;
        TL.trimClip(p, d.id, d.mode === "trim-start" ? "start" : "end", st);
      }
      this.showSnap(snapped);
      this.render();
      this.host.live();
    }

    onPointerUp() {
      const d = this.drag;
      this.drag = null;
      this.showSnap(null);
      if (!d || d.mode === "scrub") return;
      if (d.moved && d.snapshot !== JSON.stringify(this.project)) {
        this.host.pushHistory(d.snapshot);
        this.host.changed();
      }
    }
  }

  window.CK.TimelineView = TimelineView;
  window.CK.fmtTime = fmtTime;
  window.CK.el = el;
})();
