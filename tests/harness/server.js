"use strict";
/*
 * Harness uji UI di browser biasa (tanpa Tauri): menyajikan ui/, menyuntikkan
 * mock `window.__TAURI__`, dan meneruskan perintah ke implementasi Node
 * sederhana (ffprobe asli, file system, ekspor disimulasikan).
 *   node tests/harness/server.js [port]
 * Hanya untuk pengembangan; tidak ikut ke aplikasi.
 */
const http = require("node:http");
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const ROOT = path.resolve(__dirname, "../../ui");
const PORT = +process.argv[2] || 5179;
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".gif": "image/gif", ".webp": "image/webp", ".mp4": "video/mp4", ".webm": "video/webm", ".mp3": "audio/mpeg", ".wav": "audio/wav", ".m4a": "audio/mp4", ".json": "application/json", ".svg": "image/svg+xml" };

function findTool(name) {
  try { return execFileSync("where", [name], { encoding: "utf8" }).split(/\r?\n/)[0].trim(); } catch { return name; }
}
const FFPROBE = findTool("ffprobe");
const FFMPEG = findTool("ffmpeg");

function probe(file) {
  const out = execFileSync(FFPROBE, ["-v", "error", "-print_format", "json", "-show_format", "-show_streams", file], { encoding: "utf8" });
  const j = JSON.parse(out);
  const v = j.streams.find((s) => s.codec_type === "video" && !(s.disposition && s.disposition.attached_pic));
  const a = j.streams.find((s) => s.codec_type === "audio");
  const e = path.extname(file).slice(1).toLowerCase();
  const image = v && ["png", "mjpeg", "webp", "bmp"].includes(v.codec_name);
  const kind = image ? "image" : v ? "video" : "audio";
  return {
    kind, duration: image ? 0 : +(j.format.duration || 0), width: v ? v.width : 0, height: v ? v.height : 0,
    has_video: !!v, has_audio: !image && !!a, video_codec: v ? v.codec_name : "", audio_codec: a ? a.codec_name : "",
    fps: 30, container: j.format.format_name, playable: ["mp4", "webm", "png", "jpg", "jpeg", "gif", "webp", "mp3", "wav", "m4a"].includes(e),
  };
}

const CACHE = path.join(require("node:os").tmpdir(), "cutkit-harness");
fs.mkdirSync(CACHE, { recursive: true });

const commands = {
  tools_info: () => ({ ffmpeg: FFMPEG, ffprobe: FFPROBE, version: "harness", has_qsv: false }),
  probe_media: ({ paths }) => paths.map((p) => { try { return { path: p, info: probe(p), error: null }; } catch (e) { return { path: p, info: null, error: String(e.message).slice(0, 200) }; } }),
  make_proxy: ({ path: p }) => p,
  make_waveform: ({ path: p }) => {
    const dst = path.join(CACHE, path.basename(p) + ".wave.png");
    if (!fs.existsSync(dst)) execFileSync(FFMPEG, ["-hide_banner", "-y", "-loglevel", "error", "-i", p, "-filter_complex", "aformat=channel_layouts=mono,showwavespic=s=1600x60:colors=#7cc4ff:scale=sqrt:draw=full", "-frames:v", "1", "-update", "1", dst]);
    return dst;
  },
  make_filmstrip: ({ path: p, duration }) => {
    const count = Math.min(60, Math.max(4, Math.ceil(duration / 2)));
    const dst = path.join(CACHE, path.basename(p) + ".strip.jpg");
    if (!fs.existsSync(dst)) execFileSync(FFMPEG, ["-hide_banner", "-y", "-loglevel", "error", "-i", p, "-an", "-vf", `fps=${(count / duration).toFixed(5)},scale=-2:54,tile=${count}x1,format=yuvj420p`, "-frames:v", "1", "-update", "1", dst]);
    return { path: dst, count };
  },
  load_project: ({ path: p }) => JSON.parse(fs.readFileSync(p, "utf8")),
  save_project: ({ path: p, project }) => { fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, JSON.stringify(project, null, 2)); return null; },
  read_text: ({ path: p }) => fs.readFileSync(p, "utf8").replace(/^﻿/, ""),
  write_text: ({ path: p, text }) => { fs.writeFileSync(p, text); return null; },
  write_base64: ({ path: p, data }) => { fs.writeFileSync(p, Buffer.from(data.replace(/^data:[^,]+,/, ""), "base64")); return null; },
  list_dir: ({ path: p }) => fs.readdirSync(p, { withFileTypes: true })
    .filter((d) => d.isDirectory() || /\.(mp4|mov|mkv|webm|gif|png|jpe?g|webp|bmp|mp3|wav|m4a|ogg|flac|srt|json)$/i.test(d.name))
    .map((d) => ({ name: d.name, path: path.join(p, d.name), is_dir: d.isDirectory() }))
    .sort((a, b) => (b.is_dir - a.is_dir) || a.name.localeCompare(b.name)),
  path_exists: ({ path: p }) => fs.existsSync(p),
  export_start: ({ project, options }) => { lastExport = { project, options }; return null; },
  export_cancel: () => null,
  system_info: () => ({ tools: commands.tools_info(), winget: true, cache_dir: CACHE, cache_bytes: 12345678 }),
  clear_cache: () => 12345678,
  install_ffmpeg: () => commands.tools_info(),
  // Memakai media yang dibuat tes Rust `demo_media_is_generated` (cargo test).
  make_demo: () => {
    const dir = path.join(require("node:os").tmpdir(), "cutkit-test", "demo");
    const f = (n) => path.join(dir, n);
    if (!fs.existsSync(f("01-narasi.wav"))) throw new Error("Jalankan dulu: cargo test -p cutkit-core --test export_real");
    return { voice: f("01-narasi.wav"), music: f("musik.wav"), visuals: ["01-latar.png", "02-klip.mp4", "03-latar.png", "04-klip.mp4", "05-latar.png"].map(f), script: "Halo semua! Ini contoh proyek CutKit. Coba potong klip, tambah meme dan caption, lalu ekspor video kamu." };
  },
  open_path: () => null,
  reveal_path: () => null,
};
let lastExport = null;

const MOCK = `
(function () {
  const call = async (cmd, args) => {
    const r = await fetch("/__invoke/" + cmd, { method: "POST", body: JSON.stringify(args || {}) });
    const j = await r.json();
    if (!r.ok) throw j.error;
    return j.result;
  };
  const listeners = {};
  window.__harness = { emit: (ev, payload) => (listeners[ev] || []).forEach((f) => f({ payload })), dialogQueue: [] };
  const nextDialog = async () => window.__harness.dialogQueue.shift() ?? null;
  window.__TAURI__ = {
    core: { invoke: call, convertFileSrc: (p) => "/__file?path=" + encodeURIComponent(p) },
    path: { documentDir: async () => ${JSON.stringify(path.join(CACHE, "docs"))} },
    dialog: { open: nextDialog, save: nextDialog, ask: async () => true },
    event: { listen: async (ev, f) => { (listeners[ev] = listeners[ev] || []).push(f); return () => { listeners[ev] = listeners[ev].filter((x) => x !== f); }; } },
    webview: { getCurrentWebview: () => ({ onDragDropEvent: (f) => { window.__harness.drop = f; } }) },
    window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, destroy: async () => {} }) },
  };
})();
`;

function serveFile(req, res, file) {
  let stat;
  try { stat = fs.statSync(file); } catch { res.writeHead(404); return res.end("not found"); }
  const type = TYPES[path.extname(file).toLowerCase()] || "application/octet-stream";
  const range = req.headers.range && /bytes=(\d*)-(\d*)/.exec(req.headers.range);
  if (range) {
    const start = range[1] ? +range[1] : 0;
    const end = range[2] ? +range[2] : stat.size - 1;
    res.writeHead(206, { "Content-Type": type, "Content-Range": `bytes ${start}-${end}/${stat.size}`, "Accept-Ranges": "bytes", "Content-Length": end - start + 1 });
    fs.createReadStream(file, { start, end }).pipe(res);
  } else {
    res.writeHead(200, { "Content-Type": type, "Content-Length": stat.size, "Accept-Ranges": "bytes" });
    fs.createReadStream(file).pipe(res);
  }
}

http.createServer((req, res) => {
  const url = new URL(req.url, "http://x");
  if (url.pathname.startsWith("/__invoke/")) {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      const cmd = url.pathname.slice(10);
      try {
        if (!commands[cmd]) throw new Error("perintah tidak dikenal: " + cmd);
        const result = commands[cmd](JSON.parse(body || "{}"));
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ result }));
      } catch (e) {
        res.writeHead(500, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: String(e.message || e) }));
      }
    });
    return;
  }
  if (url.pathname === "/__last-export") { res.writeHead(200, { "Content-Type": "application/json" }); return res.end(JSON.stringify(lastExport)); }
  if (url.pathname === "/__file") return serveFile(req, res, url.searchParams.get("path"));
  if (url.pathname === "/__mock.js") { res.writeHead(200, { "Content-Type": "text/javascript" }); return res.end(MOCK); }
  const rel = url.pathname === "/" ? "index.html" : decodeURIComponent(url.pathname.slice(1));
  const file = path.join(ROOT, rel);
  if (!file.startsWith(ROOT)) { res.writeHead(403); return res.end(); }
  if (rel === "index.html") {
    const html = fs.readFileSync(file, "utf8").replace("<script src=\"core/timeline.js\">", "<script src=\"/__mock.js\"></script>\n  <script src=\"core/timeline.js\">");
    res.writeHead(200, { "Content-Type": "text/html" });
    return res.end(html);
  }
  serveFile(req, res, file);
}).listen(PORT, "127.0.0.1", () => console.log(`Harness CutKit di http://localhost:${PORT}`));
