/*
 * Template proyek: proyek yang medianya diganti "slot" sehingga bisa dipakai ulang
 * dengan gambar/video/suara lain. Logika murni, dites di Node.
 */
(function (root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  else (root.CK = root.CK || {}).templates = api;
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  const KIND_LABEL = { video: "Video", image: "Gambar", audio: "Audio" };

  function clone(v) { return JSON.parse(JSON.stringify(v)); }

  /** Membuat template dari proyek. Media jadi slot berlabel, kata caption dikosongkan (gayanya disimpan). */
  function makeTemplate(project, meta = {}) {
    const t = clone(project);
    const used = new Set();
    for (const tr of t.tracks) for (const c of tr.clips) if (c.asset_id) used.add(c.asset_id);
    const counters = {};
    t.assets = t.assets.filter((a) => used.has(a.id)).map((a) => {
      counters[a.kind] = (counters[a.kind] || 0) + 1;
      return {
        id: a.id,
        kind: a.kind,
        width: a.width,
        height: a.height,
        duration: a.duration,
        has_audio: a.has_audio,
        has_video: a.has_video,
        slot: true,
        label: (meta.labels && meta.labels[a.id]) || `${KIND_LABEL[a.kind] || "Media"} ${counters[a.kind]}`,
      };
    });
    t.captions.words = [];
    t.template = {
      name: meta.name || project.name || "Template",
      description: meta.description || "",
      ratio: `${project.canvas.width}×${project.canvas.height}`,
      duration: Math.round(projectDuration(project) * 10) / 10,
    };
    delete t.name;
    return t;
  }

  function projectDuration(p) {
    let d = 0;
    for (const tr of p.tracks) for (const c of tr.clips) d = Math.max(d, c.start + c.duration);
    return d;
  }

  /** Slot yang perlu diisi: { id, kind, label, need } — need = durasi sumber terpanjang yang dipakai (detik). */
  function slots(template) {
    return template.assets.filter((a) => a.slot).map((a) => {
      let need = 0;
      for (const tr of template.tracks) for (const c of tr.clips) {
        if (c.asset_id !== a.id) continue;
        const sp = c.kind === "image" ? 1 : c.speed > 0 ? c.speed : 1;
        need = Math.max(need, (c.in || 0) + c.duration * sp);
      }
      // Gambar diam tidak punya batas durasi sumber.
      return { id: a.id, kind: a.kind, label: a.label, need: a.kind === "image" ? 0 : Math.round(need * 10) / 10 };
    });
  }

  /**
   * Proyek baru dari template. `picks` = { slotId: aset nyata | null }. Klip dari slot yang
   * dilewati dihapus; klip video/audio dipangkas bila medianya lebih pendek.
   */
  function applyTemplate(template, picks, name) {
    const p = clone(template);
    delete p.template;
    p.name = name || template.template?.name || "Proyek";
    const idMap = new Map();
    const assets = [];
    for (const a of template.assets) {
      const real = picks[a.id];
      if (!real) continue;
      if (!assets.some((x) => x.id === real.id)) assets.push(clone(real));
      idMap.set(a.id, real);
    }
    p.assets = assets;
    for (const tr of p.tracks) {
      tr.clips = tr.clips.filter((c) => !c.asset_id || idMap.has(c.asset_id));
      for (const c of tr.clips) {
        if (!c.asset_id) continue;
        const real = idMap.get(c.asset_id);
        c.asset_id = real.id;
        // Jenis klip mengikuti media pengganti (gambar bisa diganti video dan sebaliknya).
        if (real.kind === "image" && c.kind === "video") c.kind = "image";
        if (real.kind === "video" && c.kind === "image") c.kind = "video";
        if ((c.kind === "video" || c.kind === "audio") && real.duration > 0) {
          const sp = c.speed > 0 ? c.speed : 1;
          if ((c.in || 0) >= real.duration) c.in = 0;
          const room = (real.duration - (c.in || 0)) / sp;
          if (c.duration > room) c.duration = Math.max(0.1, Math.round(room * 1e6) / 1e6);
        }
      }
    }
    return p;
  }

  return { makeTemplate, slots, applyTemplate };
});
