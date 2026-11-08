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
const MODELS_DIR = process.env.CUTKIT_MODELS || "D:/DevTools/cutkit-models";
// Sama dengan katalog di crates/cutkit-core/src/ai.rs (hanya untuk harness).
const AI_MODELS = [
  { id: "whisper-base", kind: "whisper", label: "Cepat (base)", note: "60 MB", file: "ggml-base-q5_1.bin", size: 59707625 },
  { id: "whisper-small", kind: "whisper", label: "Seimbang (small) — disarankan", note: "190 MB", file: "ggml-small-q5_1.bin", size: 190085487 },
  { id: "whisper-medium", kind: "whisper", label: "Akurat (medium)", note: "539 MB", file: "ggml-medium-q5_0.bin", size: 539212467 },
  { id: "vad-silero", kind: "vad", label: "Deteksi suara (Silero VAD)", note: "0,9 MB", file: "ggml-silero-v6.2.0.bin", size: 885098 },
  { id: "piper-engine", kind: "piper-engine", label: "Mesin TTS Piper", note: "22 MB", file: "piper/piper.exe", size: null },
  { id: "piper-id", kind: "piper-voice", label: "Suara Indonesia (news_tts)", note: "63 MB", file: "id_ID-news_tts-medium.onnx", size: 62950044 },
  { id: "piper-id-config", kind: "piper-config", label: "Pengaturan suara Indonesia", note: "5 KB", file: "id_ID-news_tts-medium.onnx.json", size: 5050 },
];
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
  write_binary: ({ path: p, data }) => { fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, Buffer.from(data.replace(/^data:[^,]+,/, ""), "base64")); return null; },
  write_base64: ({ path: p, data }) => { fs.writeFileSync(p, Buffer.from(data.replace(/^data:[^,]+,/, ""), "base64")); return null; },
  list_dir: ({ path: p }) => fs.readdirSync(p, { withFileTypes: true })
    .filter((d) => d.isDirectory() || /\.(mp4|mov|mkv|webm|gif|png|jpe?g|webp|bmp|mp3|wav|m4a|ogg|flac|srt|json)$/i.test(d.name))
    .map((d) => ({ name: d.name, path: path.join(p, d.name), is_dir: d.isDirectory() }))
    .sort((a, b) => (b.is_dir - a.is_dir) || a.name.localeCompare(b.name)),
  path_exists: ({ path: p }) => fs.existsSync(p),
  export_start: ({ project, options }) => { lastExport = { project, options }; return null; },
  export_cancel: () => null,
  system_info: () => ({ tools: commands.tools_info(), winget: true, installer: "winget", install_hint: "winget install Gyan.FFmpeg", os: "windows", whisper: true, cache_dir: CACHE, cache_bytes: 12345678 }),
  clear_cache: () => 12345678,
  install_ffmpeg: () => commands.tools_info(),
  // Memakai media yang dibuat tes Rust `demo_media_is_generated` (cargo test).
  make_demo: () => {
    const dir = path.join(require("node:os").tmpdir(), "cutkit-test", "demo");
    const f = (n) => path.join(dir, n);
    if (!fs.existsSync(f("01-narasi.wav"))) throw new Error("Jalankan dulu: cargo test -p cutkit-core --test export_real");
    return { voice: f("01-narasi.wav"), music: f("musik.wav"), visuals: ["01-latar.png", "02-klip.mp4", "03-latar.png", "04-klip.mp4", "05-latar.png"].map(f), script: "Halo semua! Ini contoh proyek CutKit. Coba potong klip, tambah meme dan caption, lalu ekspor video kamu." };
  },
  // AI: memakai model asli di folder model bila ada (unduh lewat aplikasi; harness tidak mengunduh).
  ai_models: () => ({ models_dir: MODELS_DIR, models: AI_MODELS.map((m) => ({ ...m, installed: fs.existsSync(path.join(MODELS_DIR, m.file)) && (m.size == null || fs.statSync(path.join(MODELS_DIR, m.file)).size === m.size) })) }),
  set_models_dir: () => null,
  ai_cancel: () => null,
  ai_download: ({ id }) => {
    const m = AI_MODELS.find((x) => x.id === id);
    if (!(fs.existsSync(path.join(MODELS_DIR, m.file)) && fs.statSync(path.join(MODELS_DIR, m.file)).size === m.size)) throw new Error("Harness tidak mengunduh model; letakkan file di " + MODELS_DIR);
    return null;
  },
  transcribe: ({ path: media, start, duration, model, language, useVad }) => {
    const m = AI_MODELS.find((x) => x.id === model);
    const vad = AI_MODELS.find((x) => x.id === "vad-silero");
    const out = "cutkit-harness-transcribe.json";
    let filter = `aresample=16000,aformat=sample_fmts=flt:channel_layouts=mono,whisper=model=${m.file}:language=${language}:queue=20:destination=${out}:format=json:max_len=1`;
    if (useVad && fs.existsSync(path.join(MODELS_DIR, vad.file))) filter += `:vad_model=${vad.file}`;
    const args = ["-hide_banner", "-nostats", "-loglevel", "error"];
    if (start > 0) args.push("-ss", start.toFixed(3));
    if (duration > 0) args.push("-t", duration.toFixed(3));
    args.push("-i", media, "-vn", "-map", "0:a:0", "-af", filter, "-f", "null", "-");
    execFileSync(FFMPEG, args, { cwd: MODELS_DIR });
    const text = fs.readFileSync(path.join(MODELS_DIR, out), "utf8");
    fs.unlinkSync(path.join(MODELS_DIR, out));
    const segs = [...text.matchAll(/\{"start":(\d+),"end":(\d+),"text":"(.*?)"\}\s*(?:\n|$)/g)].map((x) => ({ start: +x[1] / 1000, end: +x[2] / 1000, text: x[3] }));
    const words = [];
    for (const sg of segs) {
      const t = sg.text.trim();
      if (!t || /^\[.*\]$/.test(t)) continue;
      if (/^[^\p{L}\p{N}]+$/u.test(t) && words.length) { words[words.length - 1].text += t; words[words.length - 1].end = sg.end; continue; }
      words.push({ text: t, start: sg.start, end: sg.end });
    }
    return words;
  },
  tts: ({ text, lengthScale, gap, outPath }) => {
    const piperDir = path.join(MODELS_DIR, "piper");
    const sentences = text.split(/(?<=[.!?…])\s+|\n+/).map((s) => s.trim()).filter(Boolean);
    const work = fs.mkdtempSync(path.join(CACHE, "tts-"));
    const files = sentences.map((_, i) => path.join(work, `s${i}.wav`));
    const jsonl = sentences.map((s, i) => JSON.stringify({ text: s, output_file: files[i] })).join("\n") + "\n";
    execFileSync(path.join(piperDir, "piper.exe"), ["--model", path.join(MODELS_DIR, "id_ID-news_tts-medium.onnx"), "--json-input", "--length_scale", String(lengthScale)], { cwd: piperDir, input: jsonl });
    const durs = files.map((f) => +execFileSync(FFPROBE, ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", f], { encoding: "utf8" }).trim());
    execFileSync(FFMPEG, ["-y", "-loglevel", "error", "-f", "lavfi", "-i", "anullsrc=r=22050:cl=mono", "-t", String(gap), "-c:a", "pcm_s16le", path.join(work, "jeda.wav")]);
    const list = files.flatMap((f, i) => (i ? [path.join(work, "jeda.wav"), f] : [f])).map((f) => `file '${f.replace(/\\/g, "/")}'`).join("\n");
    fs.writeFileSync(path.join(work, "daftar.txt"), list);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    execFileSync(FFMPEG, ["-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", path.join(work, "daftar.txt"), "-ar", "48000", "-ac", "1", "-c:a", "pcm_s16le", outPath]);
    let t = 0;
    const timed = sentences.map((s, i) => { if (i) t += gap; const r = { text: s, start: t, end: t + durs[i] }; t += durs[i]; return r; });
    return { path: outPath, duration: t, sentences: timed };
  },
  detect_beats: () => ({ bpm: 120, beats: Array.from({ length: 120 }, (_, i) => +(0.25 + i * 0.5).toFixed(3)), downbeats: Array.from({ length: 30 }, (_, i) => +(0.25 + i * 2).toFixed(3)) }),
  list_templates: () => [],
  save_template: ({ name }) => path.join(CACHE, `${name}.json`),
  delete_template: () => null,
  matting_status: () => ({ python_ready: fs.existsSync(path.join(MODELS_DIR, "py", "Scripts", "python.exe")) }),
  matting_setup: () => null,
  // Harness hanya memproses gambar dengan skrip yang sama (video diuji lewat tes Rust).
  remove_bg: ({ path: p, kind, model, outPath }) => {
    if (kind !== "image") throw new Error("Harness hanya mendukung gambar");
    const file = { "matting-fast": "u2netp.onnx", "matting-best": "isnet-general-use.onnx" }[model];
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    execFileSync(path.join(MODELS_DIR, "py", "Scripts", "python.exe"), [path.join(MODELS_DIR, "cutkit-matting.py"), "image", path.join(MODELS_DIR, file), p, outPath]);
    return outPath;
  },
  open_path: () => null,
  reveal_path: () => null,
  launch_project: () => null,
  bank_prepare: () => {
    const dir = path.join(CACHE, "bank");
    const cli = path.resolve(__dirname, "../../target/release", process.platform === "win32" ? "cutkit-cli.exe" : "cutkit-cli");
    execFileSync(cli, ["bank", "make", "--dir", dir]);
    return dir;
  },
  // MCP: status klien disimulasikan di memori; config Claude asli tidak disentuh.
  mcp_info: () => {
    const server = path.resolve(__dirname, "../../mcp/server.js").replace(/\\/g, "/");
    return {
      server, cli: path.resolve(__dirname, "../../target/release/cutkit-cli.exe").replace(/\\/g, "/"), node: mcpState.node,
      command_line: `claude mcp add --scope user cutkit -- node "${server}"`,
      snippet: JSON.stringify({ mcpServers: { cutkit: { command: "node", args: [server] } } }, null, 2),
      clients: [
        { id: "claude-code", name: "Claude Code (juga tab Code di aplikasi Claude)", path: "C:\\Users\\uji\\.claude.json", detected: true, installed: mcpState["claude-code"], registered_server: mcpState["claude-code"] ? server : null },
        { id: "claude-desktop", name: "Claude Desktop (chat)", path: "C:\\Users\\uji\\AppData\\Roaming\\Claude\\claude_desktop_config.json", detected: false, installed: mcpState["claude-desktop"], registered_server: mcpState["claude-desktop"] ? server : null },
      ],
    };
  },
  mcp_install: ({ client }) => { mcpState[client] = true; return "C:\\Users\\uji\\.claude.json.cutkit-backup"; },
  mcp_install_node: () => { mcpState.node = "C:\\Users\\uji\\AppData\\Local\\CutKit\\node\\node.exe"; return "24.11.1"; },
  mcp_uninstall: ({ client }) => { const was = mcpState[client]; mcpState[client] = false; return was; },
};
const mcpState = { "claude-code": false, "claude-desktop": false, node: null };
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
