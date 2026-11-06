"use strict";

const tauri = window.__TAURI__;
const invoke = tauri ? tauri.core.invoke : async () => { throw new Error("Jalankan lewat aplikasi CutKit (Tauri)."); };
const { timeline: TL, captions: CAP, geometry: GEO, el, fmtTime } = window.CK;

const LIBRARY_PLACES = [
  { name: "Meme Templates", path: "D:\\Master\\Pictures\\Meme Templates" },
  { name: "Sound Effect Meme", path: "D:\\Master\\Music\\Sound Effect Meme" },
];
const FONTS = ["Impact", "Arial Black", "Arial", "Segoe UI", "Segoe UI Black", "Bahnschrift", "Verdana", "Tahoma", "Trebuchet MS", "Georgia", "Comic Sans MS", "Consolas"];
const VIDEO_EXT = ["mp4", "mov", "mkv", "webm", "avi", "m4v", "gif", "wmv", "flv", "ts", "mts", "3gp"];
const IMAGE_EXT = ["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "avif"];
const AUDIO_EXT = ["mp3", "wav", "m4a", "aac", "ogg", "opus", "flac", "wma"];
const LIB_PAGE = 120;

const state = {
  project: TL.newProject(),
  path: null,
  history: new TL.History(),
  selected: null,
  tools: null,
  dirty: false,
  saveTimer: null,
  extras: new Map(), // assetId -> { proxy, filmstrip, waveform } (dipulihkan setelah undo)
  busy: new Map(), // assetId -> label proses
  lib: { dir: null, entries: [], filter: "", shown: LIB_PAGE },
  editSnapshot: null,
  exporting: false,
  leftTab: "media",
};

const $ = (s) => document.querySelector(s);
const ext = (p) => (String(p).split(".").pop() || "").toLowerCase();
const baseName = (p) => String(p).split(/[\\/]/).pop();
const dirName = (p) => String(p).replace(/[\\/][^\\/]*$/, "");
const joinPath = (a, b) => (a.endsWith("\\") || a.endsWith("/") ? a + b : `${a}\\${b}`);
const safeName = (s) => String(s).replace(/[<>:"/\\|?*\x00-\x1f]/g, "").trim() || "Proyek";
const clamp = (v, lo, hi) => Math.min(Math.max(v, lo), hi);

function src(pathOrAsset) {
  const path = typeof pathOrAsset === "string" ? pathOrAsset : pathOrAsset.proxy || pathOrAsset.path;
  return tauri && tauri.core.convertFileSrc ? tauri.core.convertFileSrc(path) : path;
}

/* ---------- umpan balik ---------- */

let toastTimer;
function toast(msg, isError = false) {
  const box = $("#toast");
  box.textContent = msg;
  box.className = isError ? "toast err" : "toast";
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => box.classList.add("hidden"), isError ? 7000 : 3000);
}

function errMsg(e) { return e && e.message ? e.message : String(e); }

function openModal(...children) {
  $("#modal-box").replaceChildren(...children);
  $("#modal").classList.remove("hidden");
}
function closeModal() {
  $("#modal").classList.add("hidden");
  $("#modal-box").replaceChildren();
}

function promptModal(title, value) {
  return new Promise((resolve) => {
    const input = el("input", { type: "text", value });
    const done = (v) => { closeModal(); resolve(v); };
    input.addEventListener("keydown", (e) => { if (e.key === "Enter") done(input.value); if (e.key === "Escape") done(null); });
    openModal(el("h2", {}, title), input, el("div", { class: "modal-actions" },
      el("button", { class: "btn", onclick: () => done(null) }, "Batal"),
      el("button", { class: "btn primary", onclick: () => done(input.value) }, "OK")));
    setTimeout(() => { input.focus(); input.select(); }, 0);
  });
}

async function ask(message) {
  if (tauri && tauri.dialog && tauri.dialog.ask) return tauri.dialog.ask(message, { title: "CutKit", kind: "warning" });
  return window.confirm(message);
}

/* ---------- perubahan & riwayat ---------- */

function snapshot() { return JSON.stringify(state.project); }

/** Menjalankan satu operasi yang bisa di-undo. */
function commit(fn, opts = {}) {
  state.history.record(state.project);
  try {
    const r = fn(state.project);
    changed(opts);
    return r;
  } catch (e) {
    state.history.drop();
    toast(errMsg(e), true);
    return undefined;
  }
}

/** Perubahan sementara saat drag/slider: preview dan timeline ikut, tanpa riwayat. */
function live() {
  preview.invalidate();
  tlView.render();
  markDirty();
}

function beginEdit() { if (!state.editSnapshot) state.editSnapshot = snapshot(); }
function endEdit() {
  const snap = state.editSnapshot;
  state.editSnapshot = null;
  if (snap && snap !== snapshot()) {
    state.history.push(snap);
    changed({ inspector: false });
  }
}

function changed(opts = {}) {
  markDirty();
  applyExtras();
  if (state.selected && state.selected !== "__captions" && !TL.findClip(state.project, state.selected)) state.selected = null;
  tlView.render();
  preview.gc();
  preview.invalidate();
  if (opts.inspector !== false) renderInspector();
  if (opts.captions) renderCaptionsPanel();
  renderBin();
  updateUndo();
  updateDuration();
  updateEmptyHints();
}

function markDirty() {
  state.dirty = true;
  setStatus("Belum disimpan");
  clearTimeout(state.saveTimer);
  if (state.path) state.saveTimer = setTimeout(() => save().catch((e) => toast(errMsg(e), true)), 1200);
}

function setStatus(text) { $("#save-status").textContent = text; }

function updateUndo() {
  $("#undo-btn").disabled = !state.history.canUndo;
  $("#redo-btn").disabled = !state.history.canRedo;
}

function undo() {
  const p = state.history.undo(state.project);
  if (!p) return;
  state.project = p;
  changed({ captions: true });
  syncProjectControls();
}

function redo() {
  const p = state.history.redo(state.project);
  if (!p) return;
  state.project = p;
  changed({ captions: true });
  syncProjectControls();
}

/** Proxy/filmstrip/gelombang dibuat di latar; setelah undo snapshot lama belum punya, jadi dipasang ulang. */
function applyExtras() {
  for (const a of state.project.assets) {
    const x = state.extras.get(a.id);
    if (x) Object.assign(a, x);
  }
}

function setExtra(asset, patch) {
  state.extras.set(asset.id, Object.assign(state.extras.get(asset.id) || {}, patch));
  const live = TL.getAsset(state.project, asset.id);
  if (live) Object.assign(live, patch);
}

/* ---------- proyek ---------- */

function projectFileName(name) { return `${safeName(name)}.cutkit.json`; }

function cleanProject() {
  return JSON.parse(JSON.stringify(state.project, (k, v) => (k === "missing" ? undefined : v)));
}

async function save() {
  if (!state.path) return saveAs();
  clearTimeout(state.saveTimer);
  setStatus("Menyimpan…");
  await invoke("save_project", { path: state.path, project: cleanProject() });
  state.dirty = false;
  setStatus("Tersimpan");
  rememberRecent(state.path, state.project.name);
}

async function saveAs() {
  if (!tauri) return;
  const path = await tauri.dialog.save({
    defaultPath: state.path || projectFileName(state.project.name),
    filters: [{ name: "Proyek CutKit", extensions: ["json"] }],
  });
  if (!path) return;
  state.path = path.endsWith(".json") ? path : `${path}.cutkit.json`;
  await save();
}

function rememberRecent(path, name) {
  try {
    const list = JSON.parse(localStorage.getItem("cutkit.recent") || "[]").filter((r) => r.path !== path);
    list.unshift({ path, name, time: Date.now() });
    localStorage.setItem("cutkit.recent", JSON.stringify(list.slice(0, 12)));
  } catch { /* abaikan */ }
}

function recentList() {
  try { return JSON.parse(localStorage.getItem("cutkit.recent") || "[]"); } catch { return []; }
}

async function setProject(p, path) {
  preview.pause();
  state.project = TL.normalize(p);
  state.path = path;
  state.history.clear();
  state.selected = null;
  state.extras.clear();
  for (const a of state.project.assets) {
    const x = {};
    if (a.proxy) x.proxy = a.proxy;
    if (a.filmstrip) x.filmstrip = a.filmstrip;
    if (a.waveform) x.waveform = a.waveform;
    state.extras.set(a.id, x);
  }
  state.dirty = false;
  preview.seek(0);
  syncProjectControls();
  resizePreview();
  changed({ captions: true });
  state.dirty = false;
  setStatus(path ? "Tersimpan" : "");
  tlView.fit();
  if (path) rememberRecent(path, state.project.name);
  await checkAssets();
}

function syncProjectControls() {
  $("#project-name").value = state.project.name;
  const c = state.project.canvas;
  const ratio = Object.entries(TL.RATIOS).find(([, [w, h]]) => w === c.width && h === c.height);
  $("#ratio-select").value = ratio ? ratio[0] : "9:16";
  resizePreview();
}

/** Mengecek file yang hilang dan membuat ulang proxy/gelombang yang tidak ada di cache. */
async function checkAssets() {
  for (const a of state.project.assets) {
    let exists = true;
    try { exists = await invoke("path_exists", { path: a.path }); } catch { /* anggap ada */ }
    a.missing = !exists;
    if (exists) {
      for (const key of ["proxy", "waveform"]) {
        if (a[key]) {
          let ok = true;
          try { ok = await invoke("path_exists", { path: a[key] }); } catch { /* */ }
          if (!ok) { delete a[key]; const x = state.extras.get(a.id); if (x) delete x[key]; }
        }
      }
      if (a.filmstrip) {
        let ok = true;
        try { ok = await invoke("path_exists", { path: a.filmstrip.path }); } catch { /* */ }
        if (!ok) { delete a.filmstrip; const x = state.extras.get(a.id); if (x) delete x.filmstrip; }
      }
      queueExtras(a);
    }
  }
  const missing = state.project.assets.filter((a) => a.missing);
  if (missing.length) toast(`${missing.length} file media tidak ditemukan. Klik kanan di panel Media untuk mencari ulang.`, true);
  renderBin();
  tlView.render();
  preview.invalidate();
}

function welcome() {
  const recents = recentList();
  openModal(el("div", { class: "welcome" },
    el("h2", {}, "Selamat datang di CutKit"),
    el("p", { class: "hint" }, "Editor video ringan untuk Shorts 9:16. Semua diproses di komputermu dengan ffmpeg."),
    el("div", { class: "big-actions" },
      el("button", { class: "btn primary", onclick: () => quickDialog({ fromWelcome: true }) }, "⚡ Buat Shorts cepat", el("small", {}, "dari voice over + gambar")),
      el("button", { class: "btn", onclick: newProjectDialog }, "＋ Proyek kosong", el("small", {}, "susun sendiri dari awal")),
      el("button", { class: "btn", onclick: openProjectDialog }, "📂 Buka proyek", el("small", {}, "file .cutkit.json")),
      el("button", { class: "btn", id: "demo-btn", onclick: () => demoProject() }, "🎬 Coba contoh", el("small", {}, "proyek latihan siap putar"))),
    recents.length ? el("h4", {}, "Terakhir dibuka") : null,
    recents.length ? el("div", { class: "recent" }, recents.map((r) => el("button", { onclick: () => openProjectPath(r.path) },
      el("span", {}, r.name || baseName(r.path)), el("span", { class: "p" }, r.path)))) : null));
}

async function newProjectDialog() {
  let parent = "";
  try { parent = joinPath(await tauri.path.documentDir(), "CutKit"); } catch { parent = "C:\\CutKit"; }
  const now = new Date();
  const name = el("input", { type: "text", value: `Shorts ${now.toLocaleDateString("id-ID").replace(/\//g, "-")}` });
  const folder = el("input", { type: "text" });
  let customParent = false;
  const updateFolder = () => { if (!customParent) folder.value = joinPath(parent, safeName(name.value)); };
  name.addEventListener("input", updateFolder);
  updateFolder();
  let ratio = "9:16";
  const ratioSeg = segmented(Object.keys(TL.RATIOS).map((r) => [r, r]), ratio, (v) => { ratio = v; });
  const fps = el("select", {}, [24, 25, 30, 60].map((f) => el("option", { value: f, selected: f === 30 }, `${f} fps`)));
  const pick = async () => {
    const dir = await tauri.dialog.open({ directory: true, defaultPath: parent });
    if (dir) { customParent = true; folder.value = joinPath(dir, safeName(name.value)); }
  };
  const create = async () => {
    const p = TL.newProject(name.value.trim() || "Proyek baru", ratio);
    p.canvas.fps = +fps.value;
    const path = joinPath(folder.value.trim(), projectFileName(p.name));
    try {
      const exists = await invoke("path_exists", { path });
      if (exists && !(await ask(`Sudah ada proyek di\n${path}\nTimpa?`))) return;
      closeModal();
      await setProject(p, path);
      await save();
    } catch (e) { toast(errMsg(e), true); }
  };
  openModal(el("h2", {}, "Proyek baru"),
    field("Nama", name),
    field("Rasio", ratioSeg),
    field("Frame rate", fps),
    field("Folder", el("div", { class: "ctl" }, folder, el("button", { class: "btn", onclick: pick }, "Pilih…"))),
    el("p", { class: "hint" }, "Proyek disimpan sebagai file JSON di folder ini dan tersimpan otomatis."),
    el("div", { class: "modal-actions" },
      el("button", { class: "btn", onclick: () => (state.path ? closeModal() : welcome()) }, "Batal"),
      el("button", { class: "btn primary", onclick: create }, "Buat proyek")));
  setTimeout(() => name.select(), 0);
}

async function openProjectDialog() {
  const path = await tauri.dialog.open({ filters: [{ name: "Proyek CutKit", extensions: ["json"] }] });
  if (path) await openProjectPath(path);
}

async function openProjectPath(path) {
  try {
    if (state.dirty && state.path) await save();
    const p = await invoke("load_project", { path });
    closeModal();
    await setProject(p, path);
  } catch (e) {
    toast(errMsg(e), true);
  }
}

/* ---------- impor media ---------- */

function kindOfPath(p) {
  const e = ext(p);
  if (IMAGE_EXT.includes(e)) return "image";
  if (AUDIO_EXT.includes(e)) return "audio";
  if (VIDEO_EXT.includes(e)) return "video";
  return null;
}

/** Mengimpor file ke proyek (tanpa menaruh di timeline). Mengembalikan aset sesuai urutan path. */
async function importPaths(paths) {
  paths = paths.filter((p) => kindOfPath(p) || ext(p) === "");
  if (!paths.length) { toast("Tidak ada file media yang didukung.", true); return []; }
  const existing = new Map(state.project.assets.map((a) => [a.path.toLowerCase(), a]));
  const fresh = paths.filter((p) => !existing.has(p.toLowerCase()));
  const result = new Map();
  if (fresh.length) {
    setStatus(`Membaca ${fresh.length} file…`);
    let probed;
    try { probed = await invoke("probe_media", { paths: fresh }); } catch (e) { toast(errMsg(e), true); return []; }
    const errors = [];
    const added = [];
    for (const r of probed) {
      if (!r.info) { errors.push(`${baseName(r.path)}: ${r.error}`); continue; }
      const i = r.info;
      const asset = {
        id: TL.uid("m"), path: r.path, kind: i.kind, name: baseName(r.path), duration: i.duration,
        width: i.width, height: i.height, has_audio: i.has_audio, has_video: i.has_video,
        proxy: null, waveform: null, playable: i.playable,
      };
      added.push(asset);
      result.set(r.path.toLowerCase(), asset);
    }
    if (added.length) commit((p) => { p.assets.push(...added); }, { inspector: false });
    for (const a of added) queueExtras(TL.getAsset(state.project, a.id));
    if (errors.length) toast(`Gagal membaca:\n${errors.join("\n")}`, true);
    setStatus(state.dirty ? "Belum disimpan" : "Tersimpan");
  }
  return paths.map((p) => result.get(p.toLowerCase()) || existing.get(p.toLowerCase())).filter(Boolean);
}

const jobs = [];
let jobRunning = false;

/** Proxy (bila perlu), filmstrip, dan gelombang dibuat bergiliran di latar. */
function queueExtras(asset) {
  if (!asset || asset.missing) return;
  if (asset.playable === false && !asset.proxy) jobs.push({ asset, type: "proxy" });
  if (asset.kind === "video" && !asset.filmstrip) jobs.push({ asset, type: "filmstrip" });
  if (asset.has_audio && !asset.waveform) jobs.push({ asset, type: "waveform" });
  runJobs();
}

async function runJobs() {
  if (jobRunning) return;
  jobRunning = true;
  while (jobs.length) {
    // Proxy didahulukan karena preview membutuhkannya.
    const i = Math.max(0, jobs.findIndex((j) => j.type === "proxy"));
    const job = jobs.splice(i, 1)[0];
    const a = job.asset;
    state.busy.set(a.id, job.type === "proxy" ? "Menyiapkan preview…" : job.type === "filmstrip" ? "Gambar mini…" : "Gelombang…");
    renderBin();
    try {
      if (job.type === "proxy") {
        const path = await invoke("make_proxy", { path: a.path, kind: a.kind });
        setExtra(a, { proxy: path });
        markDirty();
      } else if (job.type === "filmstrip") {
        const fs = await invoke("make_filmstrip", { path: a.proxy || a.path, duration: a.duration || 1 });
        setExtra(a, { filmstrip: fs });
      } else {
        const path = await invoke("make_waveform", { path: a.path });
        setExtra(a, { waveform: path });
      }
    } catch (e) {
      if (job.type === "proxy") toast(`Preview ${a.name} gagal dibuat: ${errMsg(e)}`, true);
    }
    state.busy.delete(a.id);
    renderBin();
    tlView.render();
    preview.invalidate();
  }
  jobRunning = false;
}

async function pickAndImport() {
  if (!tauri) return;
  const picked = await tauri.dialog.open({
    multiple: true,
    filters: [
      { name: "Media", extensions: [...VIDEO_EXT, ...IMAGE_EXT, ...AUDIO_EXT] },
      { name: "Video", extensions: VIDEO_EXT },
      { name: "Gambar", extensions: IMAGE_EXT },
      { name: "Audio", extensions: AUDIO_EXT },
    ],
  });
  if (!picked) return;
  const assets = await importPaths(Array.isArray(picked) ? picked : [picked]);
  if (assets.length) toast(`${assets.length} media diimpor. Klik ＋ atau seret ke timeline.`);
}

async function relink(asset) {
  const picked = await tauri.dialog.open({ defaultPath: dirName(asset.path) });
  if (!picked) return;
  try {
    const [r] = await invoke("probe_media", { paths: [picked] });
    if (!r.info) throw new Error(r.error);
    commit((p) => {
      const a = TL.getAsset(p, asset.id);
      Object.assign(a, { path: picked, name: baseName(picked), missing: false, duration: r.info.duration || a.duration, width: r.info.width, height: r.info.height, has_audio: r.info.has_audio, playable: r.info.playable, proxy: null, waveform: null, filmstrip: null });
    });
    state.extras.delete(asset.id);
    queueExtras(TL.getAsset(state.project, asset.id));
  } catch (e) { toast(errMsg(e), true); }
}

/* ---------- operasi timeline ---------- */

/**
 * Menaruh aset di timeline.
 * opt.trackId/time: hasil drop; opt.overlay: dari pustaka (meme/SFX) -> di playhead, bukan di track utama.
 */
function addAssetToTimeline(asset, opt = {}) {
  return commit((p) => {
    const overlay = opt.overlay || (opt.trackId && opt.trackId !== p.tracks[0].id);
    const clip = TL.clipFromAsset(asset, overlay && asset.kind === "image" ? { transform: { scale: 0.6 } } : {});
    let track = opt.trackId ? TL.getTrack(p, opt.trackId) : null;
    let start = opt.time;
    if (track && !TL.trackAccepts(track, clip)) track = null;
    if (!track) {
      if (clip.kind !== "audio" && !overlay && opt.time == null) {
        track = p.tracks[0];
        start = null; // tempel di akhir track utama
      } else {
        start = start == null ? preview.time : start;
        track = TL.trackFor(p, clip, start);
      }
    }
    TL.insertClip(p, track.id, clip, start);
    state.selected = clip.id;
    return clip;
  });
}

function addText(preset) {
  commit((p) => {
    const clip = TL.textClip(preset.text, { style: preset.style, y: preset.y });
    const track = TL.trackFor(p, clip, preview.time, p.tracks.find((t) => t.kind === "video" && t !== p.tracks[0])?.id);
    TL.insertClip(p, track.id, clip, preview.time);
    state.selected = clip.id;
  });
}

function selectedClip() {
  if (!state.selected || state.selected === "__captions") return null;
  return TL.findClip(state.project, state.selected);
}

function split() {
  const t = preview.time;
  let f = selectedClip();
  // Tanpa pilihan: potong klip di track utama yang berada di bawah playhead.
  if (!f || !(t > f.clip.start && t < TL.clipEnd(f.clip))) {
    const under = TL.clipsAt(state.project, t);
    const pick = under.find((u) => u.track === state.project.tracks[0]) || under[0];
    f = pick ? { clip: pick.clip, track: pick.track } : null;
  }
  if (!f) return toast("Taruh playhead di atas klip untuk memotong.");
  commit((p) => { TL.splitClip(p, f.clip.id, t); });
}

function trimToPlayhead(edge) {
  const f = selectedClip();
  if (!f) return toast("Pilih klip dulu.");
  commit((p) => { TL.trimClip(p, f.clip.id, edge, preview.time); });
}

function deleteSelected(ripple) {
  if (state.selected === "__captions") {
    commit((p) => { p.captions.enabled = false; }, { captions: true });
    state.selected = null;
    return;
  }
  const f = selectedClip();
  if (!f) return;
  commit((p) => { TL.deleteClip(p, f.clip.id, ripple); });
  state.selected = null;
  renderInspector();
}

function duplicate() {
  const f = selectedClip();
  if (!f) return;
  commit((p) => {
    const copy = JSON.parse(JSON.stringify(f.clip));
    copy.id = TL.uid(copy.kind === "text" ? "t" : "c");
    TL.insertClip(p, f.track.id, copy, TL.clipEnd(f.clip));
    state.selected = copy.id;
  });
}

function select(id) {
  if (state.selected === id) return;
  state.selected = id;
  tlView.render();
  preview.invalidate();
  renderInspector();
}

function onTrackAction(trackId, act, ev) {
  const track = TL.getTrack(state.project, trackId);
  if (!track) return;
  if (act === "mute") return commit((p) => { TL.getTrack(p, trackId).muted = !track.muted; });
  if (act === "hide") return commit((p) => { TL.getTrack(p, trackId).hidden = !track.hidden; });
  contextMenu(ev.clientX, ev.clientY, [
    track.kind === "video" ? ["Naikkan lapisan", () => commit((p) => { if (!TL.moveTrack(p, trackId, 1)) throw new Error("Sudah lapisan teratas."); })] : null,
    track.kind === "video" ? ["Turunkan lapisan", () => commit((p) => { if (!TL.moveTrack(p, trackId, -1)) throw new Error("Sudah lapisan terbawah."); })] : null,
    ["Ganti nama…", async () => { const n = await promptModal("Nama track", track.name); if (n) commit((p) => { TL.getTrack(p, trackId).name = n; }); }],
    ["Tutup semua celah", () => commit((p) => TL.closeGaps(p, trackId))],
    ["Hapus track", () => commit((p) => TL.removeTrack(p, trackId))],
  ]);
}

function contextMenu(x, y, items) {
  document.querySelectorAll(".ctx-menu").forEach((m) => m.remove());
  const menu = el("div", { class: "menu ctx-menu" }, items.filter(Boolean).map(([label, fn]) =>
    label === "-" ? el("hr") : el("button", { onclick: () => { menu.remove(); fn(); } }, label)));
  document.body.append(menu);
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(x, innerWidth - r.width - 8)}px`;
  menu.style.top = `${Math.min(y, innerHeight - r.height - 8)}px`;
  setTimeout(() => document.addEventListener("pointerdown", function off(e) {
    if (!menu.contains(e.target)) { menu.remove(); document.removeEventListener("pointerdown", off); }
  }), 0);
}

/* ---------- panel media ---------- */

function usedAssetIds() {
  const s = new Set();
  for (const t of state.project.tracks) for (const c of t.clips) if (c.asset_id) s.add(c.asset_id);
  return s;
}

function renderBin() {
  const bin = $("#media-bin");
  const assets = state.project.assets;
  if (!assets.length) {
    bin.replaceChildren(el("div", { class: "bin-empty" }, "Belum ada media. Klik ＋ Impor atau seret video, gambar, dan audio ke jendela ini."));
    return;
  }
  const used = usedAssetIds();
  bin.replaceChildren(...assets.map((a) => {
    const thumb = el("div", { class: `thumb ${a.kind}` });
    if (a.missing) thumb.textContent = "⚠";
    else if (a.kind === "image") thumb.style.backgroundImage = `url("${src(a)}")`;
    else if (a.kind === "video" && a.filmstrip) {
      thumb.style.backgroundImage = `url("${src(a.filmstrip.path)}")`;
      thumb.style.backgroundSize = `${a.filmstrip.count * 100}% 100%`;
      thumb.style.backgroundPosition = "0 0";
    } else thumb.textContent = a.kind === "audio" ? "♪" : "🎞";
    const busy = state.busy.get(a.id);
    const node = el("div", { class: "asset" + (used.has(a.id) ? " used" : ""), "data-id": a.id, title: a.path },
      thumb,
      busy ? el("span", { class: "badge" }, busy) : a.missing ? el("span", { class: "badge warn" }, "Hilang") : null,
      el("button", { class: "add", title: "Tambah ke timeline", onclick: (e) => { e.stopPropagation(); addAssetToTimeline(a); } }, "＋"),
      el("div", { class: "meta" },
        el("div", { class: "n" }, a.name),
        el("div", { class: "d" }, a.kind === "image" ? `Gambar ${a.width}×${a.height}` : `${fmtTime(a.duration).slice(0, 5)}${a.kind === "video" ? ` · ${a.width}×${a.height}` : ""}`)));
    node.addEventListener("pointerdown", (e) => { if (e.button === 0 && !e.target.closest("button")) startItemDrag(e, a.name, (t) => dropAsset(a, t)); });
    node.addEventListener("dblclick", () => addAssetToTimeline(a));
    node.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      contextMenu(e.clientX, e.clientY, [
        ["Tambah ke timeline", () => addAssetToTimeline(a)],
        ["Tambah di playhead (overlay)", () => addAssetToTimeline(a, { overlay: true })],
        ["Cari ulang file…", () => relink(a)],
        ["Tampilkan di folder", () => invoke("reveal_path", { path: a.path }).catch((err) => toast(errMsg(err), true))],
        ["Hapus dari proyek", () => {
          if (used.has(a.id)) return toast("Media masih dipakai di timeline.", true);
          commit((p) => { p.assets = p.assets.filter((x) => x.id !== a.id); });
        }],
      ]);
    });
    return node;
  }));
}

/* ---------- seret item ke timeline / preview ---------- */

let itemDrag = null;

function startItemDrag(e, label, onDrop) {
  itemDrag = { x0: e.clientX, y0: e.clientY, label, onDrop, active: false };
}

window.addEventListener("pointermove", (e) => {
  if (!itemDrag) return;
  if (!itemDrag.active && Math.hypot(e.clientX - itemDrag.x0, e.clientY - itemDrag.y0) < 6) return;
  itemDrag.active = true;
  const g = $("#ghost-drag");
  g.textContent = itemDrag.label;
  g.classList.remove("hidden");
  g.style.left = `${e.clientX}px`;
  g.style.top = `${e.clientY}px`;
  document.body.style.cursor = "copy";
});

window.addEventListener("pointerup", async (e) => {
  const d = itemDrag;
  itemDrag = null;
  $("#ghost-drag").classList.add("hidden");
  document.body.style.cursor = "";
  if (!d || !d.active) return;
  const target = dropTargetAt(e.clientX, e.clientY);
  if (target) await d.onDrop(target);
});

/** Ke mana item dijatuhkan: { trackId, time } di timeline, { preview: true }, atau null. */
function dropTargetAt(x, y) {
  const t = tlView.dropTarget(x, y);
  if (t) return t.trackId === "__captions" ? { time: t.time } : t;
  const r = $("#preview-area").getBoundingClientRect();
  if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return { preview: true };
  return null;
}

function dropAsset(asset, target) {
  if (target.preview) return addAssetToTimeline(asset, { overlay: true });
  return addAssetToTimeline(asset, { trackId: target.trackId, time: target.time });
}

/* ---------- pustaka ---------- */

function renderLibraryPlaces() {
  const custom = (() => { try { return JSON.parse(localStorage.getItem("cutkit.libplaces") || "[]"); } catch { return []; } })();
  const places = [...LIBRARY_PLACES, ...custom];
  $("#lib-places").replaceChildren(
    ...places.map((pl) => el("button", { class: "btn small" + (state.lib.root === pl.path ? " primary" : ""), title: pl.path, onclick: () => openLibrary(pl.path, pl.path) }, pl.name)),
    el("button", { class: "btn small ghost", onclick: async () => {
      const dir = await tauri.dialog.open({ directory: true });
      if (!dir) return;
      custom.push({ name: baseName(dir), path: dir });
      localStorage.setItem("cutkit.libplaces", JSON.stringify(custom.slice(-8)));
      renderLibraryPlaces();
      openLibrary(dir, dir);
    } }, "＋ Folder…"));
}

async function openLibrary(dir, root) {
  try {
    const entries = await invoke("list_dir", { path: dir });
    state.lib = { dir, root: root || state.lib.root || dir, entries, filter: "", shown: LIB_PAGE };
    $("#lib-search").value = "";
    localStorage.setItem("cutkit.libdir", JSON.stringify({ dir, root: state.lib.root }));
    renderLibraryPlaces();
    renderLibrary();
  } catch (e) {
    $("#lib-grid").replaceChildren(el("p", { class: "hint" }, errMsg(e)));
  }
}

let libAudio = null;
function playSample(path, btn) {
  if (libAudio) { libAudio.pause(); if (libAudio.dataset.path === path) { libAudio = null; btn.textContent = "▶"; return; } }
  document.querySelectorAll(".lib-item .play").forEach((b) => (b.textContent = "▶"));
  libAudio = new Audio(src(path));
  libAudio.dataset.path = path;
  libAudio.play().catch((e) => toast(`Tidak bisa memutar: ${errMsg(e)}`, true));
  btn.textContent = "■";
  libAudio.onended = () => { btn.textContent = "▶"; libAudio = null; };
}

async function addFromLibrary(path, target) {
  const [asset] = await importPaths([path]);
  if (!asset) return;
  if (target && target.trackId) return addAssetToTimeline(asset, { trackId: target.trackId, time: target.time });
  return addAssetToTimeline(asset, { overlay: true, time: target && target.time });
}

function renderLibrary() {
  const { dir, root, entries, filter } = state.lib;
  const grid = $("#lib-grid");
  if (!dir) { grid.replaceChildren(el("p", { class: "hint" }, "Pilih folder di atas.")); return; }
  // jalur remah roti relatif ke folder akar
  const crumbs = [];
  let cur = dir;
  while (cur && cur.length >= root.length) {
    crumbs.unshift(cur);
    if (cur === root) break;
    cur = dirName(cur);
  }
  $("#lib-path").replaceChildren(...crumbs.flatMap((c, i) => [i ? el("span", {}, "›") : null, el("button", { onclick: () => openLibrary(c, root) }, baseName(c) || c)]).filter(Boolean));
  const q = filter.trim().toLowerCase();
  const list = entries.filter((e) => (e.is_dir || kindOfPath(e.path)) && (!q || e.name.toLowerCase().includes(q)));
  const shown = list.slice(0, state.lib.shown);
  const items = shown.map((e) => {
    if (e.is_dir) return el("div", { class: "lib-item dir", title: e.name, onclick: () => openLibrary(e.path, root) }, el("div", { class: "ico" }, "📁"), el("div", { class: "n" }, e.name));
    const kind = kindOfPath(e.path);
    const media = kind === "image" ? el("img", { src: src(e.path), loading: "lazy", alt: "" }) : el("div", { class: "ico" }, kind === "audio" ? "♪" : "🎞");
    const acts = el("div", { class: "acts" });
    if (kind === "audio") {
      const play = el("button", { class: "play", title: "Dengarkan" }, "▶");
      play.addEventListener("click", (ev) => { ev.stopPropagation(); playSample(e.path, play); });
      acts.append(play);
    }
    acts.append(el("button", { title: "Tambah di playhead", onclick: (ev) => { ev.stopPropagation(); addFromLibrary(e.path); } }, "＋"));
    const node = el("div", { class: "lib-item", title: `${e.name}\nKlik ＋ atau seret ke timeline` }, media, acts, el("div", { class: "n" }, e.name));
    node.addEventListener("pointerdown", (ev) => { if (ev.button === 0 && !ev.target.closest("button")) startItemDrag(ev, e.name, (t) => addFromLibrary(e.path, t.preview ? null : t)); });
    node.addEventListener("dblclick", () => addFromLibrary(e.path));
    return node;
  });
  if (list.length > shown.length) {
    items.push(el("button", { class: "btn lib-more", onclick: () => { state.lib.shown += LIB_PAGE * 2; renderLibrary(); } }, `Tampilkan lebih banyak (${list.length - shown.length} lagi)`));
  }
  if (!items.length) items.push(el("p", { class: "hint" }, q ? "Tidak ada yang cocok." : "Folder kosong."));
  grid.replaceChildren(...items);
}

/* ---------- teks ---------- */

const TEXT_PRESETS = [
  { name: "Judul", text: "JUDUL VIDEO", y: -620, style: { font: "Impact", size: 120, color: "#ffffff", outline_color: "#000000", outline: 7 } },
  { name: "Kotak hitam", text: "Teks dalam kotak", y: -560, style: { font: "Segoe UI", bold: true, size: 70, color: "#ffffff", outline: 16, background: "#000000c0" } },
  { name: "Kuning", text: "WADUH!", y: -400, style: { font: "Arial Black", size: 100, color: "#ffe600", outline_color: "#000000", outline: 7 } },
  { name: "Merah", text: "FAKTA", y: -400, style: { font: "Impact", size: 110, color: "#ff3b3b", outline_color: "#ffffff", outline: 6 } },
  { name: "Subjudul", text: "Subjudul di sini", y: 300, style: { font: "Segoe UI", bold: true, size: 64, color: "#ffffff", outline: 3, shadow: 3 } },
  { name: "Kotak kuning", text: "PENTING", y: -500, style: { font: "Arial Black", size: 80, color: "#111111", outline: 14, background: "#ffe600" } },
];

function renderTextPresets() {
  $("#text-presets").replaceChildren(...TEXT_PRESETS.map((pr) => {
    const s = Object.assign(TL.defaultTextStyle(), pr.style);
    const sample = el("span", { style: `font-family:"${s.font}";font-weight:${s.bold ? 700 : 400};color:${s.color};-webkit-text-stroke:${Math.min(2, s.outline / 3)}px ${s.outline_color};${s.background ? `background:${window.CK.text.cssColor(s.background)};padding:2px 6px;` : ""}` }, pr.name);
    return el("button", { class: "preset", title: `Tambah teks "${pr.name}"`, onclick: () => addText(pr) }, sample);
  }));
}

/* ---------- caption ---------- */

const CAPTION_PRESETS = [
  { name: "Kuning klasik", style: { font: "Impact", color: "#ffffff", highlight_color: "#ffe600", outline_color: "#000000", outline: 5, shadow: 2, background: "", uppercase: true, highlight_scale: 1.15 } },
  { name: "Hijau", style: { font: "Arial Black", color: "#ffffff", highlight_color: "#38ff6a", outline_color: "#000000", outline: 6, shadow: 0, background: "", uppercase: true, highlight_scale: 1.1 } },
  { name: "Kotak", style: { font: "Segoe UI", bold: true, color: "#ffffff", highlight_color: "#35c9ff", outline_color: "#000000", outline: 12, shadow: 0, background: "#000000b0", uppercase: false, highlight_scale: 1 } },
  { name: "Putih", style: { font: "Bahnschrift", color: "#dddddd", highlight_color: "#ffffff", outline_color: "#000000", outline: 3, shadow: 3, background: "", uppercase: false, highlight_scale: 1.08 } },
];

function setCaptions(fn, opts = {}) {
  commit((p) => fn(p.captions), Object.assign({ captions: true }, opts));
}

async function importCaptionFile(kind) {
  const filters = kind === "srt" ? [{ name: "Subtitle SRT", extensions: ["srt"] }] : [{ name: "JSON waktu per kata", extensions: ["json"] }];
  const path = await tauri.dialog.open({ filters });
  if (!path) return;
  try {
    const text = await invoke("read_text", { path });
    const words = kind === "srt" ? CAP.srtToWords(CAP.parseSrt(text)) : CAP.parseWordsJson(text);
    if (!words.length) throw new Error("Tidak ada caption yang terbaca dari file ini.");
    const offset = +($("#cap-offset")?.value || 0);
    for (const w of words) { w.start = +(w.start + offset).toFixed(3); w.end = +(w.end + offset).toFixed(3); }
    setCaptions((c) => { c.words = words; c.enabled = true; });
    toast(`${words.length} kata diimpor.`);
  } catch (e) { toast(errMsg(e), true); }
}

async function exportCaptionFile(kind) {
  const words = state.project.captions.words;
  if (!words.length) return toast("Belum ada caption.");
  const path = await tauri.dialog.save({ defaultPath: `${safeName(state.project.name)}.${kind === "srt" ? "srt" : "words.json"}`, filters: [kind === "srt" ? { name: "SRT", extensions: ["srt"] } : { name: "JSON", extensions: ["json"] }] });
  if (!path) return;
  const text = kind === "srt" ? CAP.toSrt(words, state.project.captions.style) : CAP.toWordsJson(words);
  try { await invoke("write_text", { path, text }); toast("Tersimpan."); } catch (e) { toast(errMsg(e), true); }
}

function renderCaptionsPanel() {
  const panel = $("#captions-panel");
  const cap = state.project.captions;
  const st = cap.style;
  const p = state.project;
  const sel = selectedClip();
  const defEnd = sel ? TL.clipEnd(sel.clip) : TL.duration(p) || preview.time + 10;
  const defStart = sel ? sel.clip.start : 0;

  const script = el("textarea", { placeholder: "Ketik atau tempel naskah di sini. Waktu tiap kata dibagi rata di rentang di bawah, lalu bisa dirapikan satu per satu." });
  const from = el("input", { type: "number", step: "0.1", min: "0", value: defStart.toFixed(2) });
  const to = el("input", { type: "number", step: "0.1", min: "0", value: defEnd.toFixed(2) });

  const wordRows = CAP.sortedWords(cap.words).map((w) => {
    const ref = cap.words.indexOf(w);
    const upd = (key, val) => setCaptions((c) => { c.words[ref][key] = key === "text" ? val : +(+val).toFixed(3); });
    return el("div", { class: "cap-word" },
      el("input", { type: "text", value: w.text, onchange: (e) => upd("text", e.target.value), onfocus: () => preview.seek(w.start) }),
      el("input", { type: "number", step: "0.05", value: w.start, onchange: (e) => upd("start", e.target.value) }),
      el("input", { type: "number", step: "0.05", value: w.end, onchange: (e) => upd("end", e.target.value) }),
      el("button", { title: "Hapus kata", onclick: () => setCaptions((c) => { c.words.splice(ref, 1); }) }, "✕"));
  });

  panel.replaceChildren(
    el("label", { class: "check" }, el("input", { type: "checkbox", checked: cap.enabled, onchange: (e) => setCaptions((c) => { c.enabled = e.target.checked; }) }), el("b", {}, "Tampilkan caption kata-per-kata")),
    el("div", { class: "insp" },
      el("section", {},
        el("h4", {}, "Impor"),
        el("div", { class: "btn-row" },
          el("button", { class: "btn", onclick: () => importCaptionFile("srt") }, "Impor .srt"),
          el("button", { class: "btn", onclick: () => importCaptionFile("json") }, "Impor JSON kata")),
        field("Geser waktu", el("div", { class: "ctl" }, el("input", { type: "number", id: "cap-offset", step: "0.1", value: "0" }), el("span", { class: "unit" }, "s"))),
        el("p", { class: "hint" }, 'JSON: { "words": [{ "text", "start", "end" }] }. Alignment ElevenLabs juga diterima.')),
      el("section", {},
        el("h4", {}, "Ketik manual"),
        script,
        el("div", { class: "field" }, el("label", {}, "Rentang (detik)"), el("div", { class: "ctl" }, from, el("span", {}, "–"), to)),
        el("div", { class: "btn-row" },
          el("button", { class: "btn small ghost", onclick: () => { from.value = preview.time.toFixed(2); } }, "Mulai = playhead"),
          el("button", { class: "btn small ghost", onclick: () => { from.value = "0"; to.value = (TL.duration(p) || 10).toFixed(2); } }, "Seluruh video")),
        el("button", { class: "btn primary", onclick: () => {
          const a = +from.value, b = +to.value;
          const words = CAP.textToWords(script.value, a, b);
          if (!words.length) return toast(b > a ? "Naskah masih kosong." : "Rentang waktu tidak valid.", true);
          setCaptions((c) => {
            c.words = c.words.filter((w) => w.end <= a || w.start >= b).concat(words).sort((x, y) => x.start - y.start);
            c.enabled = true;
          });
        } }, "Bagi ke kata")),
      el("section", {},
        el("h4", {}, `Kata (${cap.words.length})`),
        cap.words.length ? el("div", { class: "cap-head" }, el("span", {}, "Teks"), el("span", {}, "Mulai"), el("span", {}, "Selesai"), el("span")) : null,
        cap.words.length ? el("div", { class: "cap-words" }, wordRows) : el("p", { class: "hint" }, "Belum ada kata."),
        el("div", { class: "btn-row" },
          el("button", { class: "btn small", onclick: () => exportCaptionFile("srt") }, "Ekspor .srt"),
          el("button", { class: "btn small", onclick: () => exportCaptionFile("json") }, "Ekspor JSON"),
          cap.words.length ? el("button", { class: "btn small danger", onclick: () => setCaptions((c) => { c.words = []; }) }, "Hapus semua") : null)),
      el("section", {},
        el("h4", {}, "Gaya"),
        el("div", { class: "btn-row" }, CAPTION_PRESETS.map((pr) => el("button", { class: "btn small", onclick: () => setCaptions((c) => Object.assign(c.style, pr.style)) }, pr.name))),
        styleControls(st, (patch) => Object.assign(state.project.captions.style, patch), { caption: true }))));
}

/* ---------- kontrol inspector ---------- */

function field(label, control) {
  return el("div", { class: "field" }, el("label", {}, label), control.classList && control.classList.contains("ctl") ? control : el("div", { class: "ctl" }, control));
}

function segmented(choices, value, onPick) {
  const wrap = el("div", { class: "seg" });
  for (const [v, label] of choices) {
    const b = el("button", { class: v === value ? "on" : "" }, label);
    b.addEventListener("click", () => { wrap.querySelectorAll("button").forEach((x) => x.classList.remove("on")); b.classList.add("on"); onPick(v); });
    wrap.append(b);
  }
  return wrap;
}

/** Slider + angka yang mengubah proyek langsung; satu langkah undo per geseran. */
function slider(label, value, { min, max, step = 1, unit = "", toModel = (v) => v, fromModel = (v) => v, apply }) {
  const shown = +fromModel(value).toFixed(4);
  const range = el("input", { type: "range", min, max, step, value: shown });
  const num = el("input", { type: "number", min, max, step, value: shown });
  const set = (v, from) => {
    beginEdit();
    if (from !== range) range.value = v;
    if (from !== num) num.value = v;
    apply(toModel(+v));
    live();
  };
  range.addEventListener("input", () => set(range.value, range));
  num.addEventListener("input", () => { if (num.value !== "" && isFinite(+num.value)) set(num.value, num); });
  range.addEventListener("change", endEdit);
  num.addEventListener("change", endEdit);
  return field(label, el("div", { class: "ctl" }, range, num, unit ? el("span", { class: "unit" }, unit) : null));
}

function colorInput(value, apply, opts = {}) {
  const hex = (value || "#000000").slice(0, 7);
  const alpha = value && value.length === 9 ? parseInt(value.slice(7), 16) : 255;
  const c = el("input", { type: "color", value: hex });
  const parts = [c];
  let a = null;
  if (opts.alpha) {
    a = el("input", { type: "range", min: 0, max: 255, value: alpha, title: "Transparansi" });
    parts.push(a);
  }
  const emit = () => {
    beginEdit();
    const v = a ? c.value + (+a.value).toString(16).padStart(2, "0") : c.value;
    apply(v);
    live();
  };
  c.addEventListener("input", emit);
  c.addEventListener("change", endEdit);
  if (a) { a.addEventListener("input", emit); a.addEventListener("change", endEdit); }
  return el("div", { class: "ctl" }, ...parts);
}

function instant(apply) {
  return (e) => { beginEdit(); apply(e); live(); endEdit(); };
}

/** Kontrol gaya teks yang dipakai klip teks dan caption. */
function styleControls(st, apply, opts = {}) {
  const font = el("select", { onchange: instant((e) => apply({ font: e.target.value })) },
    (FONTS.includes(st.font) ? FONTS : [st.font, ...FONTS]).map((f) => el("option", { value: f, selected: f === st.font, style: `font-family:"${f}"` }, f)));
  const hasBox = !!(st.background && st.background.trim());
  const nodes = [
    field("Font", font),
    slider("Ukuran", st.size, { min: 20, max: 240, apply: (v) => apply({ size: v }) }),
    field("Warna", colorInput(st.color, (v) => apply({ color: v }))),
  ];
  if (opts.caption) nodes.push(field("Kata aktif", colorInput(st.highlight_color, (v) => apply({ highlight_color: v }))));
  nodes.push(
    field("Garis tepi", colorInput(st.outline_color, (v) => apply({ outline_color: v }))),
    slider(hasBox ? "Padding kotak" : "Tebal tepi", st.outline, { min: 0, max: 30, step: 0.5, apply: (v) => apply({ outline: v }) }),
    slider("Bayangan", st.shadow, { min: 0, max: 20, step: 0.5, apply: (v) => apply({ shadow: v }) }),
    field("Kotak latar", el("div", { class: "ctl" },
      el("input", { type: "checkbox", checked: hasBox, onchange: instant((e) => { apply({ background: e.target.checked ? "#000000b0" : "" }); setTimeout(rerenderStylePanel, 0); }) }),
      hasBox ? colorInput(st.background, (v) => apply({ background: v }), { alpha: true }) : el("span", { class: "hint" }, "tanpa kotak"))),
    el("div", { class: "btn-row" },
      el("label", { class: "check" }, el("input", { type: "checkbox", checked: !!st.bold, onchange: instant((e) => apply({ bold: e.target.checked })) }), "Tebal"),
      opts.caption ? el("label", { class: "check" }, el("input", { type: "checkbox", checked: !!st.uppercase, onchange: instant((e) => apply({ uppercase: e.target.checked })) }), "HURUF BESAR")
        : el("label", { class: "check" }, el("input", { type: "checkbox", checked: !!st.italic, onchange: instant((e) => apply({ italic: e.target.checked })) }), "Miring")));
  if (opts.caption) {
    nodes.push(
      slider("Kata/halaman", st.words_per_page, { min: 1, max: 10, apply: (v) => apply({ words_per_page: Math.round(v) }) }),
      slider("Huruf/baris", st.max_line_chars, { min: 4, max: 40, apply: (v) => apply({ max_line_chars: Math.round(v) }) }),
      slider("Posisi tinggi", st.y, { min: 5, max: 95, unit: "%", fromModel: (v) => v * 100, toModel: (v) => v / 100, apply: (v) => apply({ y: v }) }),
      slider("Sorot besar", st.highlight_scale, { min: 100, max: 160, unit: "%", fromModel: (v) => v * 100, toModel: (v) => v / 100, apply: (v) => apply({ highlight_scale: v }) }));
  }
  return el("div", { class: "insp-style", style: "display:flex;flex-direction:column;gap:8px" }, nodes);
}

function rerenderStylePanel() {
  if (state.selected === "__captions" || state.leftTab === "captions") renderCaptionsPanel();
  renderInspector();
}

function renderInspector() {
  const box = $("#inspector");
  const p = state.project;
  if (state.selected === "__captions") {
    box.replaceChildren(el("div", { class: "insp" },
      el("h3", {}, "Caption"),
      el("div", { class: "sub" }, `${p.captions.words.length} kata. Seret di preview untuk memindah posisi.`),
      el("section", {}, styleControls(p.captions.style, (patch) => Object.assign(state.project.captions.style, patch), { caption: true })),
      el("section", {}, el("button", { class: "btn", onclick: () => switchTab("captions") }, "Edit kata & impor…"))));
    return;
  }
  const f = selectedClip();
  if (!f) {
    box.replaceChildren(projectInspector());
    return;
  }
  const c = f.clip;
  const asset = c.asset_id ? TL.getAsset(p, c.asset_id) : null;
  const upd = (patch) => { const live = TL.findClip(state.project, c.id); if (live) TL.updateClip(state.project, c.id, patch); };
  const title = c.kind === "text" ? "Teks" : c.kind === "image" ? "Gambar" : c.kind === "audio" ? "Audio" : "Video";
  const sections = [];

  sections.push(el("section", {},
    el("div", { class: "export-info" },
      el("span", {}, "Mulai"), el("b", {}, fmtTime(c.start, p.canvas.fps)),
      el("span", {}, "Durasi"), el("b", {}, `${c.duration.toFixed(2)} s`),
      c.kind === "video" || c.kind === "audio" ? el("span", {}, "Dari sumber") : null,
      c.kind === "video" || c.kind === "audio" ? el("b", {}, `${c.in.toFixed(2)} s`) : null),
    c.kind === "text" || c.kind === "image"
      ? field("Durasi", el("div", { class: "ctl" }, el("input", { type: "number", min: 0.1, step: 0.1, value: c.duration.toFixed(2), onchange: (e) => commit((pp) => TL.setDuration(pp, c.id, +e.target.value)) }), el("span", { class: "unit" }, "s")))
      : null));

  if (c.kind === "text") {
    const ta = el("textarea", { id: "text-edit" }, c.text.text);
    ta.addEventListener("input", () => { beginEdit(); upd({ text: { text: ta.value } }); live(); });
    ta.addEventListener("change", endEdit);
    sections.push(el("section", {}, el("h4", {}, "Teks"), ta, styleControls(c.text.style, (patch) => upd({ text: { style: patch } }))));
  }

  if (c.kind !== "audio") {
    const t = c.transform;
    sections.push(el("section", {},
      el("h4", {}, "Posisi & ukuran"),
      c.kind !== "text" ? field("Isi kanvas", segmented([["contain", "Pas"], ["cover", "Penuh"]], c.fit, (v) => commit((pp) => TL.updateClip(pp, c.id, { fit: v })))) : null,
      c.kind !== "text" ? field("Latar", segmented([["none", "Warna kanvas"], ["blur", "Blur"]], c.background, (v) => commit((pp) => TL.updateClip(pp, c.id, { background: v })))) : null,
      slider("X", t.x, { min: -p.canvas.width, max: p.canvas.width, apply: (v) => upd({ transform: { x: v } }) }),
      slider("Y", t.y, { min: -p.canvas.height, max: p.canvas.height, apply: (v) => upd({ transform: { y: v } }) }),
      slider("Skala", t.scale, { min: 5, max: 400, unit: "%", fromModel: (v) => v * 100, toModel: (v) => v / 100, apply: (v) => upd({ transform: { scale: v } }) }),
      slider("Rotasi", t.rotation, { min: -180, max: 180, unit: "°", apply: (v) => upd({ transform: { rotation: v } }) }),
      slider("Opacity", t.opacity, { min: 0, max: 100, unit: "%", fromModel: (v) => v * 100, toModel: (v) => v / 100, apply: (v) => upd({ transform: { opacity: v } }) }),
      el("div", { class: "btn-row" },
        el("button", { class: "btn small", onclick: () => commit((pp) => TL.updateClip(pp, c.id, { transform: { x: 0, y: 0 } })) }, "Ke tengah"),
        el("button", { class: "btn small", onclick: () => commit((pp) => TL.updateClip(pp, c.id, { transform: TL.defaultTransform() })) }, "Reset"))));
  }

  if (c.kind === "video" || c.kind === "image") {
    const MOTION_LABELS = [["none", "Diam"], ["zoom-in", "Zoom masuk"], ["zoom-out", "Zoom keluar"], ["pan-left", "Geser ←"], ["pan-right", "Geser →"], ["pan-up", "Geser ↑"], ["pan-down", "Geser ↓"]];
    sections.push(el("section", {},
      el("h4", {}, "Gerak (Ken Burns)"),
      el("div", { class: "btn-row" }, MOTION_LABELS.map(([k, label]) => el("button", {
        class: "btn small" + (c.motion.kind === k ? " primary" : ""),
        onclick: () => commit((pp) => TL.updateClip(pp, c.id, { motion: { kind: k } })),
      }, label))),
      c.motion.kind !== "none" ? slider("Kekuatan", c.motion.amount, { min: 2, max: 60, unit: "%", fromModel: (v) => v * 100, toModel: (v) => v / 100, apply: (v) => upd({ motion: { amount: v } }) }) : null,
      el("p", { class: "hint" }, "Isi klip bergerak pelan di dalam bingkainya, cocok untuk gambar diam.")));
  }

  if (c.kind === "video" || c.kind === "audio") {
    const sp = TL.speedOf(c);
    const speedInput = el("input", { type: "number", min: 0.1, max: 10, step: 0.05, value: sp });
    speedInput.addEventListener("change", () => commit((pp) => TL.setSpeed(pp, c.id, +speedInput.value)));
    sections.push(el("section", {},
      el("h4", {}, "Kecepatan"),
      el("div", { class: "btn-row" }, [0.5, 0.75, 1, 1.25, 1.5, 2, 3].map((v) => el("button", {
        class: "btn small" + (Math.abs(sp - v) < 1e-3 ? " primary" : ""),
        onclick: () => commit((pp) => TL.setSpeed(pp, c.id, v)),
      }, `${v}x`))),
      field("Kustom", el("div", { class: "ctl" }, speedInput, el("span", { class: "unit" }, "x"))),
      el("p", { class: "hint" }, "Durasi di timeline ikut berubah dan klip sesudahnya bergeser. Suara dipercepat tanpa mengubah nada.")));
  }

  if (c.kind === "audio" || (c.kind === "video" && asset && asset.has_audio)) {
    sections.push(el("section", {},
      el("h4", {}, "Audio"),
      el("label", { class: "check" }, el("input", { type: "checkbox", checked: c.muted, onchange: (e) => commit((pp) => TL.updateClip(pp, c.id, { muted: e.target.checked })) }), "Bisukan klip ini"),
      slider("Volume", c.volume, { min: 0, max: 400, unit: "%", fromModel: (v) => v * 100, toModel: (v) => v / 100, apply: (v) => upd({ volume: v }) }),
      slider("Fade in", c.fade_in, { min: 0, max: Math.min(10, c.duration), step: 0.1, unit: "s", apply: (v) => upd({ fade_in: v }) }),
      slider("Fade out", c.fade_out, { min: 0, max: Math.min(10, c.duration), step: 0.1, unit: "s", apply: (v) => upd({ fade_out: v }) }),
      c.volume > 1 ? el("p", { class: "hint" }, "Volume di atas 100% terdengar di hasil ekspor; preview dibatasi 100%.") : null));
  }

  sections.push(el("section", {},
    el("h4", {}, "Urutan"),
    el("div", { class: "btn-row" },
      el("button", { class: "btn small", onclick: () => commit((pp) => { if (!TL.swapClip(pp, c.id, -1)) throw new Error("Sudah paling kiri."); }) }, "← Tukar"),
      el("button", { class: "btn small", onclick: () => commit((pp) => { if (!TL.swapClip(pp, c.id, 1)) throw new Error("Sudah paling kanan."); }) }, "Tukar →"),
      el("button", { class: "btn small", onclick: duplicate }, "Duplikat"),
      el("button", { class: "btn small danger", onclick: () => deleteSelected(false) }, "Hapus"))));

  box.replaceChildren(el("div", { class: "insp" },
    el("h3", {}, title),
    el("div", { class: "sub" }, asset ? asset.name : c.kind === "text" ? "Klik dua kali di preview untuk mengedit teks" : ""),
    ...sections));
}

function projectInspector() {
  const p = state.project;
  const fps = el("select", { onchange: (e) => commit((pp) => { pp.canvas.fps = +e.target.value; }) },
    [24, 25, 30, 50, 60].map((f) => el("option", { value: f, selected: f === p.canvas.fps }, `${f} fps`)));
  return el("div", { class: "insp" },
    el("h3", {}, "Proyek"),
    el("div", { class: "sub" }, state.path || "Belum disimpan"),
    el("section", {},
      el("div", { class: "export-info" },
        el("span", {}, "Kanvas"), el("b", {}, `${p.canvas.width} × ${p.canvas.height}`),
        el("span", {}, "Durasi"), el("b", {}, fmtTime(TL.duration(p)).slice(0, 5)),
        el("span", {}, "Klip"), el("b", {}, String(p.tracks.reduce((n, t) => n + t.clips.length, 0)))),
      field("Frame rate", fps),
      field("Latar", colorInput(p.canvas.background, (v) => { state.project.canvas.background = v; }))),
    el("section", {},
      el("h4", {}, "Tips"),
      el("p", { class: "hint" }, "Pilih klip di timeline atau preview untuk mengatur posisi, skala, rotasi, volume, dan fade. Spasi = putar, S = potong, Delete = hapus, Ctrl+Z = urungkan.")));
}

/* ---------- interaksi preview ---------- */

let pdrag = null;

function previewPointerDown(e) {
  if (e.button !== 0) return;
  const [x, y] = preview.toCanvas(e.clientX, e.clientY);
  const p = state.project;
  const r = preview.canvas.getBoundingClientRect();
  const px = p.canvas.width / r.width; // piksel kanvas per piksel layar
  const sel = preview.box(state.selected);
  let mode = null;
  let box = null;
  if (sel && sel.kind !== "captions") {
    const rh = preview.rotateHandle(sel.pl);
    if (Math.hypot(x - rh[0], y - rh[1]) < 16 * px) { mode = "rotate"; box = sel; }
    else if (GEO.corners(sel.pl).some(([cx, cy]) => Math.hypot(x - cx, y - cy) < 14 * px)) { mode = "scale"; box = sel; }
  }
  if (!mode) {
    box = preview.hit(x, y);
    if (!box) { select(null); return; }
    mode = "move";
    select(box.id);
  }
  e.preventDefault();
  preview.canvas.setPointerCapture(e.pointerId);
  const clip = box.kind === "captions" ? null : TL.findClip(p, box.id).clip;
  pdrag = {
    mode, box, x0: x, y0: y,
    orig: clip ? { ...clip.transform } : { x: p.captions.style.x || 0, y: p.captions.style.y },
    dist0: Math.hypot(x - box.pl.cx, y - box.pl.cy) || 1,
    snapshot: snapshot(),
  };
}

function previewPointerMove(e) {
  const [x, y] = preview.toCanvas(e.clientX, e.clientY);
  if (!pdrag) {
    const sel = preview.box(state.selected);
    preview.canvas.style.cursor = preview.hit(x, y) || sel ? "move" : "default";
    return;
  }
  const p = state.project;
  const d = pdrag;
  const r = preview.canvas.getBoundingClientRect();
  const px = p.canvas.width / r.width;
  if (d.box.kind === "captions") {
    const st = p.captions.style;
    st.y = clamp(d.orig.y + (y - d.y0) / p.canvas.height, 0.03, 0.97);
    let nx = d.orig.x + (x - d.x0);
    if (Math.abs(nx) < 10 * px) nx = 0;
    st.x = Math.round(nx);
    preview.showGuides = st.x === 0;
  } else {
    const clip = TL.findClip(p, d.box.id).clip;
    const t = clip.transform;
    if (d.mode === "move") {
      let nx = d.orig.x + (x - d.x0);
      let ny = d.orig.y + (y - d.y0);
      const snapX = Math.abs(nx) < 10 * px;
      const snapY = Math.abs(ny) < 10 * px;
      if (snapX && !e.altKey) nx = 0;
      if (snapY && !e.altKey) ny = 0;
      t.x = Math.round(nx);
      t.y = Math.round(ny);
      preview.showGuides = (snapX || snapY) && !e.altKey;
    } else if (d.mode === "scale") {
      const dist = Math.hypot(x - d.box.pl.cx, y - d.box.pl.cy);
      t.scale = clamp(+(d.orig.scale * (dist / d.dist0)).toFixed(4), 0.02, 20);
    } else {
      let deg = (Math.atan2(y - d.box.pl.cy, x - d.box.pl.cx) * 180) / Math.PI + 90;
      if (deg > 180) deg -= 360;
      if (e.shiftKey) deg = Math.round(deg / 15) * 15;
      else for (const s of [-180, -90, 0, 90, 180]) if (Math.abs(deg - s) < 3) deg = s;
      t.rotation = Math.round(deg * 10) / 10;
    }
  }
  preview.invalidate();
  markDirty();
}

function previewPointerUp() {
  const d = pdrag;
  pdrag = null;
  preview.showGuides = false;
  preview.invalidate();
  if (!d) return;
  if (d.snapshot !== snapshot()) {
    state.history.push(d.snapshot);
    changed();
  }
}

let wheelTimer;
function previewWheel(e) {
  const f = selectedClip();
  if (!f) return;
  const [x, y] = preview.toCanvas(e.clientX, e.clientY);
  const box = preview.box(f.clip.id);
  if (!box || !GEO.hitTest(box.pl, x, y)) return;
  e.preventDefault();
  beginEdit();
  const t = f.clip.transform;
  if (e.shiftKey) t.rotation = Math.round((t.rotation + (e.deltaY < 0 ? 2 : -2)) * 10) / 10;
  else t.scale = clamp(+(t.scale * (e.deltaY < 0 ? 1.05 : 1 / 1.05)).toFixed(4), 0.02, 20);
  live();
  clearTimeout(wheelTimer);
  wheelTimer = setTimeout(() => { endEdit(); renderInspector(); }, 400);
}

function resizePreview() {
  const area = $("#preview-area");
  preview.resize(area.clientWidth - 24, area.clientHeight - 24);
}

/* ---------- buat Shorts cepat ---------- */

const naturalSort = (a, b) => a.localeCompare(b, "id", { numeric: true, sensitivity: "base" });

/** Menebak isi folder: audio dengan nama vo/voice/narasi = voice over, file audio lain = musik. */
function classifyFolder(entries) {
  const files = entries.filter((e) => !e.is_dir).sort((a, b) => naturalSort(a.name, b.name));
  const audio = files.filter((e) => kindOfPath(e.path) === "audio");
  const voiceRe = /(^|[^a-z])(vo|voice|voiceover|narasi|narration|suara|tts)([^a-z]|$)/i;
  const musicRe = /(music|musik|bgm|backsound|lagu)/i;
  let voice = audio.find((e) => voiceRe.test(e.name)) || null;
  let music = audio.find((e) => e !== voice && musicRe.test(e.name)) || null;
  if (!voice) voice = audio.find((e) => e !== music) || null;
  if (!music && audio.length === 2) music = audio.find((e) => e !== voice) || null;
  const visuals = files.filter((e) => ["image", "video"].includes(kindOfPath(e.path)));
  const caption = files.find((e) => /\.(json)$/i.test(e.name) && /(word|kata|caption|align|timing|beat)/i.test(e.name))
    || files.find((e) => /\.srt$/i.test(e.name))
    || files.find((e) => /\.json$/i.test(e.name) && !/cutkit/i.test(e.name))
    || null;
  return { voice: voice && voice.path, music: music && music.path, visuals: visuals.map((e) => e.path), caption: caption && caption.path };
}

async function readCaptionWords(path) {
  const text = await invoke("read_text", { path });
  return /\.srt$/i.test(path) ? CAP.srtToWords(CAP.parseSrt(text)) : CAP.parseWordsJson(text);
}

function quickDialog(opts = {}) {
  const sel = { voice: null, visuals: [], music: null, caption: null, script: "", musicVolume: 15, fit: "cover", capPreset: 0, motion: true };
  const needsProject = !state.path || opts.fromWelcome;
  const name = el("input", { type: "text", value: `Shorts ${new Date().toLocaleDateString("id-ID").replace(/\//g, "-")}` });
  const summary = el("div", { class: "quick-summary" });
  const show = (path) => (path ? baseName(path) : "—");
  const render = () => {
    summary.replaceChildren(
      el("div", {}, el("span", {}, "Voice over"), el("b", {}, show(sel.voice))),
      el("div", {}, el("span", {}, "Gambar/klip"), el("b", {}, sel.visuals.length ? `${sel.visuals.length} file (${sel.visuals.slice(0, 3).map(baseName).join(", ")}${sel.visuals.length > 3 ? ", …" : ""})` : "—")),
      el("div", {}, el("span", {}, "Caption"), el("b", {}, sel.caption ? show(sel.caption) : sel.script.trim() ? "dari naskah yang diketik" : "—")),
      el("div", {}, el("span", {}, "Musik latar"), el("b", {}, show(sel.music))));
    go.disabled = !sel.voice && !sel.visuals.length;
  };
  const pickFolder = async () => {
    const dir = await tauri.dialog.open({ directory: true });
    if (!dir) return;
    try {
      const found = classifyFolder(await invoke("list_dir", { path: dir }));
      Object.assign(sel, found);
      if (needsProject && !name.dataset.touched) name.value = baseName(dir);
      render();
      if (!found.visuals.length && !found.voice) toast("Folder ini tidak berisi audio, gambar, atau video.", true);
    } catch (e) { toast(errMsg(e), true); }
  };
  const pickFile = (key, filters, multiple = false) => async () => {
    const picked = await tauri.dialog.open({ multiple, filters });
    if (!picked) return;
    if (key === "visuals") sel.visuals = (Array.isArray(picked) ? picked : [picked]).sort(naturalSort);
    else sel[key] = picked;
    render();
  };
  const clear = (key) => () => { sel[key] = key === "visuals" ? [] : null; render(); };
  const script = el("textarea", { placeholder: "Opsional: tempel naskah voice over. Waktunya dibagi rata sepanjang voice over (lebih pas pakai JSON kata atau SRT)." });
  script.addEventListener("input", () => { sel.script = script.value; render(); });
  name.addEventListener("input", () => { name.dataset.touched = "1"; });
  const vol = el("input", { type: "range", min: 0, max: 60, value: sel.musicVolume });
  const volLabel = el("span", { class: "unit", style: "width:34px" }, `${sel.musicVolume}%`);
  vol.addEventListener("input", () => { sel.musicVolume = +vol.value; volLabel.textContent = `${vol.value}%`; });
  const go = el("button", { class: "btn primary", disabled: true }, "Susun timeline");
  go.addEventListener("click", async () => {
    go.disabled = true;
    go.textContent = "Menyusun…";
    try { await runQuick(sel, needsProject ? name.value : null); } catch (e) { toast(errMsg(e), true); go.disabled = false; go.textContent = "Susun timeline"; }
  });
  const row = (label, ...btns) => el("div", { class: "field" }, el("label", {}, label), el("div", { class: "ctl" }, ...btns));
  openModal(
    el("h2", {}, "⚡ Buat Shorts cepat"),
    el("p", { class: "hint" }, "Pilih voice over dan gambar/klip; CutKit membagi gambar sepanjang suara, memotong tepat di awal kata, menambah caption dan musik pelan. Hasilnya tetap bisa diedit."),
    needsProject ? field("Nama proyek", name) : null,
    el("button", { class: "btn", onclick: pickFolder }, "📁 Pilih satu folder berisi semuanya…"),
    el("p", { class: "hint" }, "Contoh isi folder: vo.mp3, 01.png, 02.png, 03.mp4, words.json (atau .srt), musik.mp3. Urutan gambar mengikuti nama file."),
    row("Voice over", el("button", { class: "btn small", onclick: pickFile("voice", [{ name: "Audio", extensions: AUDIO_EXT }]) }, "Pilih…"), el("button", { class: "btn small ghost", onclick: clear("voice") }, "Kosongkan")),
    row("Gambar/klip", el("button", { class: "btn small", onclick: pickFile("visuals", [{ name: "Gambar & video", extensions: [...IMAGE_EXT, ...VIDEO_EXT] }], true) }, "Pilih…"), el("button", { class: "btn small ghost", onclick: clear("visuals") }, "Kosongkan")),
    row("Caption", el("button", { class: "btn small", onclick: pickFile("caption", [{ name: "SRT / JSON kata", extensions: ["srt", "json"] }]) }, "Pilih…"), el("button", { class: "btn small ghost", onclick: clear("caption") }, "Kosongkan")),
    row("Musik latar", el("button", { class: "btn small", onclick: pickFile("music", [{ name: "Audio", extensions: AUDIO_EXT }]) }, "Pilih…"), el("button", { class: "btn small ghost", onclick: clear("music") }, "Kosongkan")),
    row("Volume musik", vol, volLabel),
    row("Gambar", segmented([["cover", "Penuh layar"], ["contain", "Utuh (ada pinggiran)"]], sel.fit, (v) => { sel.fit = v; })),
    row("Gerak", el("label", { class: "check" }, el("input", { type: "checkbox", checked: sel.motion, onchange: (e) => { sel.motion = e.target.checked; } }), "Zoom pelan otomatis di gambar (Ken Burns)")),
    row("Gaya caption", segmented(CAPTION_PRESETS.map((pr, i) => [i, pr.name]), sel.capPreset, (v) => { sel.capPreset = v; })),
    script,
    summary,
    el("div", { class: "modal-actions" },
      el("button", { class: "btn", onclick: () => (state.path ? closeModal() : welcome()) }, "Batal"),
      go));
  render();
}

async function runQuick(sel, newName) {
  if (newName != null) {
    let parent;
    try { parent = joinPath(await tauri.path.documentDir(), "CutKit"); } catch { parent = "C:\\CutKit"; }
    const p = TL.newProject(newName.trim() || "Shorts");
    await setProject(p, joinPath(joinPath(parent, safeName(p.name)), projectFileName(p.name)));
  } else if (TL.duration(state.project) > 0 && !(await ask("Isi timeline sekarang akan diganti hasil susunan otomatis (bisa di-undo dengan Ctrl+Z). Lanjut?"))) {
    return;
  }
  const paths = [sel.voice, ...sel.visuals, sel.music].filter(Boolean);
  const assets = await importPaths(paths);
  const byPath = new Map(assets.map((a) => [a.path.toLowerCase(), a]));
  const get = (p) => (p ? byPath.get(p.toLowerCase()) || null : null);
  const voice = get(sel.voice);
  const music = get(sel.music);
  const visuals = sel.visuals.map(get).filter(Boolean);
  let words = null;
  if (sel.caption) words = await readCaptionWords(sel.caption);
  else if (sel.script.trim()) {
    const end = voice ? voice.duration : visuals.length * TL.STILL_DURATION;
    words = CAP.textToWords(sel.script, 0, end);
  }
  commit((p) => {
    TL.autoAssemble(p, { voice, visuals, music, words, musicVolume: sel.musicVolume / 100, fit: sel.fit, motion: sel.motion !== false });
    Object.assign(p.captions.style, CAPTION_PRESETS[sel.capPreset].style);
  }, { captions: true });
  closeModal();
  await save().catch(() => {});
  preview.seek(0);
  tlView.fit();
  toast(`Timeline tersusun: ${visuals.length} visual${voice ? ", voice over" : ""}${words ? `, ${words.length} kata caption` : ""}${music ? ", musik" : ""}. Tekan Spasi untuk memutar.`);
}

/* ---------- untuk pemula: contoh proyek, cek sistem, panduan ---------- */

async function demoProject() {
  if (!state.tools) {
    toast("Contoh proyek butuh ffmpeg. Pasang dulu lewat Cek sistem.", true);
    return systemDialog();
  }
  openModal(el("h2", {}, "Menyiapkan contoh proyek…"), el("p", { class: "hint" }, "Membuat narasi, gambar latar, klip, dan musik contoh dengan ffmpeg. Biasanya beberapa detik."), el("div", { class: "progress indeterminate" }, el("div")));
  try {
    let parent;
    try { parent = joinPath(await tauri.path.documentDir(), "CutKit"); } catch { parent = "C:\\CutKit"; }
    const media = await invoke("make_demo", { dir: joinPath(joinPath(parent, "Contoh CutKit"), "media") });
    await runQuick({ voice: media.voice, visuals: media.visuals, music: media.music, caption: null, script: media.script, musicVolume: 15, fit: "cover", capPreset: 0 }, "Contoh CutKit");
    commit((p) => {
      const title = TL.textClip("CONTOH CUTKIT", { y: -640, duration: 2.5, style: { size: 110, outline: 7 } });
      const track = TL.trackFor(p, title, 0);
      TL.insertClip(p, track.id, title, 0);
    });
    state.selected = null;
    renderInspector();
    if (!localStorage.getItem("cutkit.tourDone")) setTimeout(startTour, 400);
  } catch (e) {
    openModal(el("h2", {}, "Contoh proyek gagal dibuat"), el("pre", { style: "white-space:pre-wrap;font-size:12px;color:var(--danger)" }, errMsg(e)),
      el("div", { class: "modal-actions" }, el("button", { class: "btn primary", onclick: welcome }, "Kembali")));
  }
}

function fmtBytes(n) {
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

function onToolsReady(tools) {
  state.tools = tools;
  $("#ffmpeg-banner").classList.add("hidden");
}

async function systemDialog(autoInstall = false) {
  const body = el("div", { class: "checks" }, el("p", { class: "hint" }, "Memeriksa…"));
  const log = el("pre", { class: "setup-log hidden" });
  const actions = el("div", { class: "modal-actions" }, el("button", { class: "btn", onclick: () => (state.path ? closeModal() : welcome()) }, "Tutup"));
  openModal(el("h2", {}, "Cek sistem"), body, log, actions);
  let info;
  try { info = await invoke("system_info"); } catch (e) { body.replaceChildren(el("p", { class: "hint" }, errMsg(e))); return; }
  const item = (ok, title, detail) => el("div", { class: "check-row" + (ok === true ? " ok" : ok === false ? " bad" : "") },
    el("span", { class: "mark" }, ok === true ? "✓" : ok === false ? "✕" : "–"), el("div", {}, el("b", {}, title), el("div", { class: "hint" }, detail)));
  const t = info.tools;
  const install = el("button", { class: "btn primary" }, "Pasang ffmpeg otomatis");
  const clearBtn = el("button", { class: "btn small" }, "Bersihkan cache");
  clearBtn.addEventListener("click", async () => {
    try { const freed = await invoke("clear_cache"); toast(`Cache dibersihkan (${fmtBytes(freed)}). Preview dibuat ulang saat dibutuhkan.`); systemDialog(); }
    catch (e) { toast(errMsg(e), true); }
  });
  body.replaceChildren(
    item(!!t, t ? `ffmpeg ${t.version}` : "ffmpeg belum terpasang", t ? t.ffmpeg : "Dibutuhkan untuk impor, preview format khusus, dan ekspor."),
    item(t ? (t.has_qsv ? true : null) : null, t && t.has_qsv ? "Intel Quick Sync tersedia" : "Intel Quick Sync tidak terdeteksi", t && t.has_qsv ? "Pilih encoder Quick Sync saat ekspor agar lebih cepat dan hemat baterai." : "Ekspor memakai CPU (libx264); hasil tetap sama bagusnya."),
    item(info.winget ? true : null, info.winget ? "winget tersedia" : "winget tidak ditemukan", info.winget ? "Dipakai untuk memasang dan memperbarui ffmpeg." : "Pasang \"App Installer\" dari Microsoft Store bila ingin memasang ffmpeg otomatis."),
    el("div", { class: "check-row" }, el("span", { class: "mark" }, "🗂"), el("div", {}, el("b", {}, `Cache preview ${fmtBytes(info.cache_bytes)}`), el("div", { class: "hint" }, info.cache_dir)), clearBtn));
  if (!t && info.winget) actions.prepend(install);
  install.addEventListener("click", async () => {
    install.disabled = true;
    install.textContent = "Memasang… (bisa beberapa menit)";
    log.classList.remove("hidden");
    log.textContent = "winget install Gyan.FFmpeg\n";
    const un = await tauri.event.listen("setup-log", (ev) => {
      const lines = log.textContent.split("\n");
      // Baris progres winget (persen/bar) menimpa baris terakhir, seperti di terminal.
      if (/[█▒]|\d+%|MB \/|KB \//.test(ev.payload) && /[█▒]|\d+%|MB \/|KB \//.test(lines[lines.length - 1])) lines[lines.length - 1] = ev.payload;
      else lines.push(ev.payload);
      log.textContent = lines.slice(-60).join("\n");
      log.scrollTop = log.scrollHeight;
    });
    try {
      const tools = await invoke("install_ffmpeg");
      onToolsReady(tools);
      toast(`ffmpeg ${tools.version} siap dipakai.`);
      systemDialog();
    } catch (e) {
      log.textContent += `\n\nGagal: ${errMsg(e)}`;
      install.disabled = false;
      install.textContent = "Coba lagi";
    } finally { un(); }
  });
  if (autoInstall && !t && info.winget) install.click();
}

const TOUR = [
  { sel: ".panel.left", text: "Media: impor video, gambar, dan audio di sini (atau seret file dari Explorer). Klik ＋ untuk menaruh ke timeline." },
  { sel: "#left-tabs [data-tab=library]", text: "Pustaka: meme dan sound effect dari foldermu. Seret ke timeline atau ke preview." },
  { sel: "#left-tabs [data-tab=captions]", text: "Caption: impor .srt atau JSON kata dari VoiceKit, atau ketik naskah. Kata yang sedang diucapkan disorot." },
  { sel: ".preview-area", text: "Preview: klik gambar/teks untuk memilih, seret untuk memindah, tarik pojok untuk memperbesar, bulatan atas untuk memutar." },
  { sel: "#inspector", text: "Inspector: atur posisi, skala, rotasi, transparansi, volume, dan fade klip yang dipilih." },
  { sel: ".tl-toolbar", text: "Timeline: S memotong di playhead, Delete menghapus, Ctrl+Z membatalkan. Seret tepi klip untuk memangkas." },
  { sel: "#export-btn", text: "Selesai? Klik Ekspor untuk membuat MP4 1080×1920 siap upload ke YouTube Shorts." },
];

function startTour(i = 0) {
  document.querySelectorAll(".tour").forEach((n) => n.remove());
  const step = TOUR[i];
  if (!step) { localStorage.setItem("cutkit.tourDone", "1"); return; }
  const target = document.querySelector(step.sel);
  if (!target) return startTour(i + 1);
  const r = target.getBoundingClientRect();
  const spot = el("div", { class: "tour-spot", style: `left:${r.left - 4}px;top:${r.top - 4}px;width:${r.width + 8}px;height:${r.height + 8}px` });
  const end = () => { document.querySelectorAll(".tour").forEach((n) => n.remove()); localStorage.setItem("cutkit.tourDone", "1"); };
  const bubble = el("div", { class: "tour-bubble" },
    el("div", { class: "hint" }, `Panduan ${i + 1}/${TOUR.length}`),
    el("p", {}, step.text),
    el("div", { class: "modal-actions" },
      el("button", { class: "btn small ghost", onclick: end }, "Lewati"),
      el("button", { class: "btn small primary", onclick: () => startTour(i + 1) }, i === TOUR.length - 1 ? "Selesai" : "Lanjut →")));
  const wrap = el("div", { class: "tour" }, spot, bubble);
  document.body.append(wrap);
  const b = bubble.getBoundingClientRect();
  let left = r.right + 14;
  let top = r.top;
  if (left + b.width > innerWidth - 10) { left = r.left - b.width - 14; }
  if (left < 10) { left = Math.min(Math.max(10, r.left), innerWidth - b.width - 10); top = r.bottom + 12; }
  if (top + b.height > innerHeight - 10) top = Math.max(10, r.top - b.height - 12);
  bubble.style.left = `${left}px`;
  bubble.style.top = `${Math.max(10, top)}px`;
}

function updateEmptyHints() {
  const empty = TL.duration(state.project) <= 0;
  $("#empty-hint").classList.toggle("hidden", !empty);
}

/* ---------- ekspor ---------- */

function exportDialog() {
  const p = state.project;
  const dur = TL.duration(p);
  if (dur <= 0) return toast("Timeline masih kosong.", true);
  let folder = localStorage.getItem("cutkit.exportdir") || (state.path ? dirName(state.path) : "");
  let quality = localStorage.getItem("cutkit.quality") || "balanced";
  let encoder = state.tools && state.tools.has_qsv ? localStorage.getItem("cutkit.encoder") || "x264" : "x264";
  const name = el("input", { type: "text", value: `${safeName(p.name)}.mp4` });
  const folderInput = el("input", { type: "text", value: folder });
  const pick = async () => {
    const dir = await tauri.dialog.open({ directory: true, defaultPath: folderInput.value || undefined });
    if (dir) folderInput.value = dir;
  };
  const encChoices = [["x264", "CPU (libx264)"]];
  if (state.tools && state.tools.has_qsv) encChoices.push(["qsv", "Intel Quick Sync"]);
  const start = async () => {
    let file = name.value.trim() || "video.mp4";
    if (!/\.mp4$/i.test(file)) file += ".mp4";
    folder = folderInput.value.trim();
    if (!folder) return toast("Pilih folder tujuan.", true);
    const output = joinPath(folder, safeName(file));
    if (await invoke("path_exists", { path: output }).catch(() => false)) {
      if (!(await ask(`${output}\nsudah ada. Timpa?`))) return;
    }
    localStorage.setItem("cutkit.exportdir", folder);
    localStorage.setItem("cutkit.quality", quality);
    localStorage.setItem("cutkit.encoder", encoder);
    runExport({ output, quality, encoder });
  };
  openModal(el("h2", {}, "Ekspor video"),
    el("div", { class: "export-info" },
      el("span", {}, "Resolusi"), el("b", {}, `${p.canvas.width} × ${p.canvas.height}`),
      el("span", {}, "Frame rate"), el("b", {}, `${p.canvas.fps} fps`),
      el("span", {}, "Durasi"), el("b", {}, `${dur.toFixed(2)} detik`),
      el("span", {}, "Format"), el("b", {}, "MP4 · H.264 + AAC 192 kbps")),
    field("Nama file", name),
    field("Folder", el("div", { class: "ctl" }, folderInput, el("button", { class: "btn", onclick: pick }, "Pilih…"))),
    field("Kualitas", segmented([["fast", "Cepat"], ["balanced", "Seimbang"], ["best", "Terbaik"]], quality, (v) => { quality = v; })),
    field("Encoder", segmented(encChoices, encoder, (v) => { encoder = v; })),
    state.tools && !state.tools.has_qsv ? el("p", { class: "hint" }, "Intel Quick Sync tidak terdeteksi; memakai CPU.") : null,
    el("div", { class: "modal-actions" },
      el("button", { class: "btn", onclick: closeModal }, "Batal"),
      el("button", { class: "btn primary", onclick: start }, "Ekspor")));
}

async function runExport(options) {
  const bar = el("div");
  const info = el("p", { class: "hint" }, "Memulai ffmpeg…");
  const cancel = el("button", { class: "btn", onclick: () => invoke("export_cancel").catch(() => {}) }, "Batalkan");
  openModal(el("h2", {}, "Mengekspor…"), el("div", { class: "progress" }, bar), info, el("div", { class: "modal-actions" }, cancel));
  state.exporting = true;
  const started = performance.now();
  const unlisten = [];
  const finish = (payload) => {
    state.exporting = false;
    unlisten.forEach((u) => u());
    if (payload.ok) {
      const secs = ((performance.now() - started) / 1000).toFixed(1);
      openModal(el("h2", {}, "Selesai ✓"),
        el("p", {}, `Video tersimpan dalam ${secs} detik:`),
        el("p", { class: "hint", style: "user-select:text" }, payload.output),
        el("div", { class: "modal-actions" },
          el("button", { class: "btn", onclick: closeModal }, "Tutup"),
          el("button", { class: "btn", onclick: () => invoke("reveal_path", { path: payload.output }) }, "Tampilkan di folder"),
          el("button", { class: "btn primary", onclick: () => invoke("open_path", { path: payload.output }) }, "Putar video")));
    } else {
      openModal(el("h2", {}, payload.error && payload.error.includes("dibatalkan") ? "Ekspor dibatalkan" : "Ekspor gagal"),
        el("pre", { style: "white-space:pre-wrap;user-select:text;font-size:12px;color:var(--danger)" }, payload.error || ""),
        el("div", { class: "modal-actions" }, el("button", { class: "btn primary", onclick: closeModal }, "Tutup")));
    }
  };
  try {
    unlisten.push(await tauri.event.listen("export-progress", (ev) => {
      const pr = ev.payload;
      bar.style.width = `${pr.percent.toFixed(1)}%`;
      const elapsed = (performance.now() - started) / 1000;
      const eta = pr.percent > 2 ? (elapsed * (100 - pr.percent)) / pr.percent : null;
      info.textContent = `${pr.percent.toFixed(0)}% · ${pr.seconds.toFixed(1)} s dirender${pr.speed ? ` · kecepatan ${pr.speed}` : ""}${eta != null ? ` · sisa ±${Math.ceil(eta)} detik` : ""}`;
    }));
    unlisten.push(await tauri.event.listen("export-done", (ev) => finish(ev.payload)));
    await invoke("export_start", { project: cleanProject(), options });
  } catch (e) {
    finish({ ok: false, error: errMsg(e) });
  }
}

/* ---------- keyboard ---------- */

function shortcutsDialog() {
  const rows = [
    ["Spasi", "Putar / jeda"], ["← / →", "Mundur / maju 1 frame (Shift: 1 detik)"], ["Home / End", "Ke awal / akhir"],
    ["S atau Ctrl+B", "Potong di playhead"], ["Q / W", "Pangkas awal / akhir klip ke playhead"],
    ["Delete", "Hapus klip"], ["Shift+Delete", "Hapus dan tutup celah"], ["Ctrl+D", "Duplikat klip"],
    ["Ctrl+Z / Ctrl+Y", "Urungkan / ulangi"], ["Ctrl+S", "Simpan"], ["Ctrl+E", "Ekspor"],
    ["+ / −", "Zoom timeline"], ["N", "Snap aktif/mati (tahan Alt saat menyeret)"], ["Ctrl+scroll", "Zoom timeline di kursor"],
    ["Scroll di preview", "Skala klip terpilih (Shift: putar)"], ["Esc", "Batalkan pilihan"],
  ];
  openModal(el("h2", {}, "Pintasan keyboard"), el("div", { class: "shortcuts" }, rows.flatMap(([k, v]) => [el("kbd", {}, k), el("span", {}, v)])),
    el("div", { class: "modal-actions" }, el("button", { class: "btn primary", onclick: closeModal }, "Tutup")));
}

function frameStep(n) {
  preview.pause();
  const fps = state.project.canvas.fps;
  preview.seek(Math.round((preview.time + n / fps) * fps) / fps);
}

document.addEventListener("keydown", (e) => {
  const tag = (e.target.tagName || "").toLowerCase();
  const typing = tag === "textarea" || (tag === "input" && !["checkbox", "range", "radio", "button", "color"].includes(e.target.type)) || tag === "select";
  const ctrl = e.ctrlKey || e.metaKey;
  if (ctrl && e.key.toLowerCase() === "s") { e.preventDefault(); save().catch((err) => toast(errMsg(err), true)); return; }
  if (ctrl && e.key.toLowerCase() === "e") { e.preventDefault(); exportDialog(); return; }
  if (typing || !$("#modal").classList.contains("hidden")) {
    if (e.key === "Escape" && !$("#modal").classList.contains("hidden") && state.path && !state.exporting) closeModal();
    return;
  }
  const k = e.key.toLowerCase();
  if (ctrl && k === "z" && !e.shiftKey) { e.preventDefault(); undo(); }
  else if (ctrl && (k === "y" || (k === "z" && e.shiftKey))) { e.preventDefault(); redo(); }
  else if (ctrl && k === "b") { e.preventDefault(); split(); }
  else if (ctrl && k === "d") { e.preventDefault(); duplicate(); }
  else if (ctrl) return;
  else if (e.key === " ") { e.preventDefault(); preview.toggle(); }
  else if (k === "s") split();
  else if (k === "q") trimToPlayhead("start");
  else if (k === "w") trimToPlayhead("end");
  else if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); deleteSelected(e.shiftKey); }
  else if (e.key === "ArrowLeft") { e.preventDefault(); frameStep(e.shiftKey ? -state.project.canvas.fps : -1); }
  else if (e.key === "ArrowRight") { e.preventDefault(); frameStep(e.shiftKey ? state.project.canvas.fps : 1); }
  else if (e.key === "Home") preview.seek(0);
  else if (e.key === "End") preview.seek(TL.duration(state.project));
  else if (e.key === "+" || e.key === "=") tlView.setZoom(tlView.pps * 1.25);
  else if (e.key === "-") tlView.setZoom(tlView.pps / 1.25);
  else if (k === "n") { const t = $("#snap-toggle"); t.checked = !t.checked; tlView.snapEnabled = t.checked; toast(t.checked ? "Snap aktif" : "Snap mati"); }
  else if (e.key === "Escape") select(null);
});

/* ---------- inisialisasi ---------- */

const preview = new window.CK.Preview($("#preview"), {
  project: () => state.project,
  src: (asset) => src(asset),
  selected: () => state.selected,
  onTime: (t) => {
    $("#tc-now").textContent = fmtTime(t, state.project.canvas.fps);
    tlView.updatePlayhead();
    if (preview.playing) tlView.follow();
    $("#play-btn").textContent = preview.playing ? "❚❚" : "▶";
  },
  onEnded: () => { $("#play-btn").textContent = "▶"; },
});

const tlView = new window.CK.TimelineView({
  project: () => state.project,
  time: () => preview.time,
  seek: (t) => { if (preview.playing) preview.pause(); preview.seek(t); $("#play-btn").textContent = "▶"; },
  selected: () => state.selected,
  select,
  snapshot,
  pushHistory: (json) => state.history.push(json),
  changed: () => changed(),
  live: () => { preview.invalidate(); markDirty(); },
  src: (p) => src(p),
  onTrackAction,
  onClipDblClick: (id) => { const f = TL.findClip(state.project, id); if (f && f.clip.kind === "text") { select(id); setTimeout(() => $("#text-edit")?.focus(), 0); } },
  onZoom: (pps) => { $("#zoom").value = Math.round((Math.log(pps / 4) / Math.log(800 / 4)) * 100); },
});

function updateDuration() {
  $("#tc-dur").textContent = fmtTime(TL.duration(state.project), state.project.canvas.fps);
}

function switchTab(tab) {
  state.leftTab = tab;
  document.querySelectorAll("#left-tabs button").forEach((b) => b.classList.toggle("active", b.dataset.tab === tab));
  document.querySelectorAll(".panel.left .tab-body").forEach((s) => s.classList.toggle("hidden", s.dataset.tab !== tab));
  if (tab === "captions") renderCaptionsPanel();
  if (tab === "library" && !state.lib.dir) {
    let last = null;
    try { last = JSON.parse(localStorage.getItem("cutkit.libdir") || "null"); } catch { /* */ }
    openLibrary(last ? last.dir : LIBRARY_PLACES[0].path, last ? last.root : LIBRARY_PLACES[0].path);
  }
}

function wire() {
  document.querySelectorAll("#left-tabs button").forEach((b) => b.addEventListener("click", () => switchTab(b.dataset.tab)));
  $("#import-btn").addEventListener("click", pickAndImport);
  $("#empty-import").addEventListener("click", pickAndImport);
  $("#empty-quick").addEventListener("click", () => quickDialog());
  $("#empty-demo").addEventListener("click", () => demoProject());
  $("#lib-search").addEventListener("input", (e) => { state.lib.filter = e.target.value; state.lib.shown = LIB_PAGE; renderLibrary(); });
  $("#undo-btn").addEventListener("click", undo);
  $("#redo-btn").addEventListener("click", redo);
  $("#export-btn").addEventListener("click", exportDialog);
  $("#play-btn").addEventListener("click", () => preview.toggle());
  $("#start-btn").addEventListener("click", () => preview.seek(0));
  $("#end-btn").addEventListener("click", () => preview.seek(TL.duration(state.project)));
  $("#prev-frame").addEventListener("click", () => frameStep(-1));
  $("#next-frame").addEventListener("click", () => frameStep(1));
  $("#guides-btn").addEventListener("click", (e) => { preview.showGuides = !preview.showGuides; e.currentTarget.classList.toggle("on", preview.showGuides); preview.invalidate(); });
  $("#split-btn").addEventListener("click", split);
  $("#trim-left-btn").addEventListener("click", () => trimToPlayhead("start"));
  $("#trim-right-btn").addEventListener("click", () => trimToPlayhead("end"));
  $("#delete-btn").addEventListener("click", (e) => deleteSelected(e.shiftKey));
  $("#dup-btn").addEventListener("click", duplicate);
  $("#add-vtrack").addEventListener("click", () => commit((p) => TL.addTrack(p, "video")));
  $("#add-atrack").addEventListener("click", () => commit((p) => TL.addTrack(p, "audio")));
  $("#snap-toggle").addEventListener("change", (e) => { tlView.snapEnabled = e.target.checked; });
  $("#zoom").addEventListener("input", (e) => tlView.setZoom(4 * Math.pow(800 / 4, +e.target.value / 100)));
  $("#zoom-in").addEventListener("click", () => tlView.setZoom(tlView.pps * 1.25));
  $("#zoom-out").addEventListener("click", () => tlView.setZoom(tlView.pps / 1.25));
  $("#zoom-fit").addEventListener("click", () => tlView.fit());
  $("#ratio-select").addEventListener("change", (e) => {
    const [w, h] = TL.RATIOS[e.target.value];
    commit((p) => { p.canvas.width = w; p.canvas.height = h; });
    resizePreview();
  });
  $("#project-name").addEventListener("change", (e) => commit((p) => { p.name = e.target.value.trim() || "Proyek"; }));
  $("#menu-btn").addEventListener("click", (e) => { e.stopPropagation(); $("#project-menu").classList.toggle("hidden"); });
  document.addEventListener("click", () => $("#project-menu").classList.add("hidden"));
  $("#project-menu").addEventListener("click", (e) => {
    const cmd = e.target.closest("button")?.dataset.cmd;
    if (cmd === "quick") quickDialog();
    else if (cmd === "new") newProjectDialog();
    else if (cmd === "open") openProjectDialog();
    else if (cmd === "save") save().catch((err) => toast(errMsg(err), true));
    else if (cmd === "saveas") saveAs().catch((err) => toast(errMsg(err), true));
    else if (cmd === "reveal" && state.path) invoke("reveal_path", { path: state.path });
    else if (cmd === "shortcuts") shortcutsDialog();
    else if (cmd === "tour") startTour();
    else if (cmd === "system") systemDialog();
    else if (cmd === "demo") demoProject();
  });

  const canvas = $("#preview");
  canvas.addEventListener("pointerdown", previewPointerDown);
  canvas.addEventListener("pointermove", previewPointerMove);
  canvas.addEventListener("pointerup", previewPointerUp);
  canvas.addEventListener("wheel", previewWheel, { passive: false });
  canvas.addEventListener("dblclick", () => { const f = selectedClip(); if (f && f.clip.kind === "text") setTimeout(() => $("#text-edit")?.focus(), 0); });
  new ResizeObserver(resizePreview).observe($("#preview-area"));

  $("#tl-rows").addEventListener("contextmenu", (e) => {
    const c = e.target.closest(".clip");
    if (!c) return;
    e.preventDefault();
    select(c.dataset.id);
    contextMenu(e.clientX, e.clientY, [
      ["Potong di playhead", split],
      ["Duplikat", duplicate],
      ["← Tukar dengan kiri", () => commit((p) => TL.swapClip(p, c.dataset.id, -1))],
      ["Tukar dengan kanan →", () => commit((p) => TL.swapClip(p, c.dataset.id, 1))],
      ["-"],
      ["Hapus", () => deleteSelected(false)],
      ["Hapus & tutup celah", () => deleteSelected(true)],
    ]);
  });

  // Ubah tinggi timeline dengan menyeret tepi atasnya.
  const tl = $("#timeline");
  tl.addEventListener("pointerdown", (e) => {
    const r = tl.getBoundingClientRect();
    if (e.clientY - r.top > 4) return;
    e.preventDefault();
    const move = (ev) => {
      const h = clamp(innerHeight - ev.clientY, 160, innerHeight - 260);
      document.body.style.setProperty("--tl-h", `${h}px`);
    };
    const up = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up); localStorage.setItem("cutkit.tlh", document.body.style.getPropertyValue("--tl-h")); };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  });
  const savedH = localStorage.getItem("cutkit.tlh");
  if (savedH) document.body.style.setProperty("--tl-h", savedH);

  // File dari Explorer: impor; bila dijatuhkan di timeline/preview langsung ditaruh di sana.
  if (tauri && tauri.webview) {
    tauri.webview.getCurrentWebview().onDragDropEvent(async (event) => {
      const { type, paths, position } = event.payload;
      const hint = $("#drop-hint");
      if (type === "enter" || type === "over") { hint.classList.remove("hidden"); return; }
      hint.classList.add("hidden");
      if (type !== "drop" || !paths || !paths.length) return;
      const projectFile = paths.find((p) => /\.cutkit\.json$/i.test(p));
      if (projectFile) return openProjectPath(projectFile);
      const subs = paths.filter((p) => ["srt"].includes(ext(p)));
      if (subs.length) {
        try {
          const words = CAP.srtToWords(CAP.parseSrt(await invoke("read_text", { path: subs[0] })));
          setCaptions((c) => { c.words = words; c.enabled = true; });
          toast(`${words.length} kata caption diimpor dari SRT.`);
        } catch (e) { toast(errMsg(e), true); }
      }
      const media = paths.filter((p) => kindOfPath(p));
      if (!media.length) return;
      const assets = await importPaths(media);
      const dpr = window.devicePixelRatio || 1;
      const target = position ? dropTargetAt(position.x / dpr, position.y / dpr) : null;
      if (target) {
        let time = target.time;
        for (const a of assets) {
          const clip = target.preview ? addAssetToTimeline(a, { overlay: true }) : addAssetToTimeline(a, { trackId: target.trackId, time });
          if (clip && time != null) time = TL.clipEnd(clip);
        }
      } else if (assets.length) toast(`${assets.length} media diimpor ke panel Media.`);
    });
  }

  if (tauri && tauri.window) {
    const win = tauri.window.getCurrentWindow();
    win.onCloseRequested(async (e) => {
      if (state.dirty && state.path) {
        e.preventDefault();
        try { await save(); } catch { /* tetap tutup */ }
        state.dirty = false;
        await win.destroy();
      }
    });
  }
  document.fonts && document.fonts.ready.then(() => { window.CK.text.resetFonts(); preview.invalidate(); });
}

async function init() {
  wire();
  renderTextPresets();
  renderLibraryPlaces();
  syncProjectControls();
  changed();
  state.dirty = false;
  setStatus("");
  welcome();
  try {
    state.tools = await invoke("tools_info");
  } catch (e) {
    const b = $("#ffmpeg-banner");
    b.replaceChildren(el("b", {}, "ffmpeg belum terpasang. "), "Impor dan ekspor butuh ffmpeg. ",
      el("button", { class: "btn small primary", onclick: () => systemDialog(true) }, "Pasang otomatis"), " ",
      el("span", { class: "hint" }, "atau lewat terminal: "), el("code", {}, "winget install Gyan.FFmpeg"));
    b.classList.remove("hidden");
  }
}

init();
