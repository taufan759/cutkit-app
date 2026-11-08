"use strict";
// Tes server MCP lewat stdio seperti klien MCP sungguhan.
const test = require("node:test");
const assert = require("node:assert/strict");
const { spawn, execFileSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const ROOT = path.join(__dirname, "..");
const CLI = path.join(ROOT, "target", "release", process.platform === "win32" ? "cutkit-cli.exe" : "cutkit-cli");

function client() {
  const child = spawn(process.execPath, [path.join(ROOT, "mcp", "server.js")], { stdio: ["pipe", "pipe", "inherit"] });
  let buf = "";
  const waiting = new Map();
  const notes = [];
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (d) => {
    buf += d;
    let i;
    while ((i = buf.indexOf("\n")) >= 0) {
      const msg = JSON.parse(buf.slice(0, i));
      buf = buf.slice(i + 1);
      if (msg.id != null && waiting.has(msg.id)) { waiting.get(msg.id)(msg); waiting.delete(msg.id); } else notes.push(msg);
    }
  });
  let next = 1;
  const request = (method, params) => new Promise((resolve) => {
    const id = next++;
    waiting.set(id, resolve);
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  });
  const call = async (name, args, meta) => {
    const r = await request("tools/call", { name, arguments: args, ...(meta ? { _meta: meta } : {}) });
    const text = r.result.content[0].text;
    if (r.result.isError) throw new Error(text);
    return JSON.parse(text);
  };
  const close = () => { child.stdin.end(); setTimeout(() => child.kill(), 2000).unref(); };
  return { request, call, notes, close };
}

test("MCP: initialize, tools/list, dan edit proyek tanpa ffmpeg", async (t) => {
  const c = client();
  t.after(() => c.close());
  const init = await c.request("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "tes", version: "1" } });
  assert.equal(init.result.serverInfo.name, "cutkit");
  assert.ok(init.result.capabilities.tools);
  const list = await c.request("tools/list", {});
  const names = list.result.tools.map((t) => t.name);
  for (const n of ["cutkit_new_project", "cutkit_quick_video", "cutkit_captions_from_audio", "cutkit_tts", "cutkit_export", "cutkit_remove_background", "cutkit_apply_template"]) assert.ok(names.includes(n), n);
  assert.ok(list.result.tools.every((t) => t.inputSchema && t.inputSchema.type === "object"));

  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "cutkit-mcp-"));
  const proj = path.join(dir, "uji.cutkit.json");
  const s = await c.call("cutkit_new_project", { project: proj, name: "Uji MCP", ratio: "16:9" });
  assert.deepEqual([s.canvas.width, s.canvas.height], [1920, 1080]);
  const txt = await c.call("cutkit_add_text", { project: proj, text: "Halo", start: 1, duration: 2 });
  const fx = await c.call("cutkit_add_effect", { project: proj, kind: "flash", start: 0, duration: 0.5 });
  await c.call("cutkit_set_captions", { project: proj, text: "satu dua tiga", start: 0, end: 3, style: "youtube" });
  const sum = await c.call("cutkit_project_summary", { project: proj });
  const clips = sum.tracks.flatMap((t) => t.clips);
  assert.ok(clips.some((x) => x.id === txt.clip_id && x.text === "Halo"));
  assert.ok(clips.some((x) => x.id === fx.clip_id && x.effect === "flash"));
  assert.equal(sum.captions.words, 3);
  assert.equal(sum.captions.style.words_per_page, 8);
  await c.call("cutkit_edit_clip", { project: proj, clip_id: txt.clip_id, set: { start: 2, transform: { y: -200 } } });
  const after = (await c.call("cutkit_project_summary", { project: proj })).tracks.flatMap((t) => t.clips).find((x) => x.id === txt.clip_id);
  assert.equal(after.start, 2);
  const bad = await c.request("tools/call", { name: "cutkit_edit_clip", arguments: { project: proj, clip_id: "tidak-ada", set: {} } });
  assert.ok(bad.result.isError);
  const templates = await c.call("cutkit_list_templates", {});
  assert.ok(templates.some((t) => t.name === "Fakta Shorts" && t.slots.length === 3));

  fs.writeFileSync(path.join(dir, "Kucing Lucu.png"), "x");
  fs.writeFileSync(path.join(dir, "boom.mp3"), "x");
  fs.mkdirSync(path.join(dir, "sub"));
  const media = await c.call("cutkit_list_media", { folder: dir, kind: "image" });
  assert.deepEqual(media.files.map((f) => path.basename(f.path)), ["Kucing Lucu.png"]);
  assert.deepEqual(media.folders.map((f) => path.basename(f)), ["sub"]);
  assert.equal((await c.call("cutkit_list_media", { folder: dir, search: "BOOM" })).files[0].kind, "audio");

  const prompts = await c.request("prompts/list", {});
  assert.ok(prompts.result.prompts.some((p) => p.name === "buat_shorts"));
  const got = await c.request("prompts/get", { name: "buat_shorts", arguments: { folder: "D:\\Bahan" } });
  assert.match(got.result.messages[0].content.text, /D:\\Bahan.*cutkit_list_media/s);
  c.close();
});

test("MCP: video cepat lalu ekspor sungguhan (butuh cutkit-cli dan ffmpeg)", { skip: !fs.existsSync(CLI) && "cutkit-cli belum dibuild" }, async (t) => {
  let ffmpeg;
  try { ffmpeg = execFileSync(process.platform === "win32" ? "where" : "which", ["ffmpeg"], { encoding: "utf8" }).split(/\r?\n/)[0].trim(); } catch { return; }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "cutkit-mcp-ekspor-"));
  const img = (name, color) => { const f = path.join(dir, name); execFileSync(ffmpeg, ["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", `color=c=${color}:s=360x640:d=1`, "-frames:v", "1", "-update", "1", f]); return f; };
  const a = img("01.png", "red"), b = img("02.png", "blue");
  const vo = path.join(dir, "vo.wav");
  execFileSync(ffmpeg, ["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "sine=f=300:d=3", vo]);
  const c = client();
  t.after(() => c.close());
  await c.request("initialize", { protocolVersion: "2025-06-18", capabilities: {} });
  const proj = path.join(dir, "cepat.cutkit.json");
  await c.call("cutkit_new_project", { project: proj, ratio: "9:16" });
  const q = await c.call("cutkit_quick_video", { project: proj, voice: vo, visuals: [a, b], script: "ini video dari mcp" });
  assert.ok(Math.abs(q.duration - 3) < 0.05);
  assert.equal(q.captions.words, 4);
  const frames = await c.request("tools/call", { name: "cutkit_preview_frame", arguments: { project: proj, times: [0.5, 2.5], height: 640 } });
  const images = frames.result.content.filter((x) => x.type === "image");
  assert.equal(images.length, 2, JSON.stringify(frames.result).slice(0, 300));
  const png = Buffer.from(images[0].data, "base64");
  assert.equal(png.toString("latin1", 1, 4), "PNG");
  assert.deepEqual([png.readUInt32BE(16), png.readUInt32BE(20)], [360, 640], "kanvas 1080x1920 diperkecil ke tinggi 640");
  assert.notEqual(images[0].data, images[1].data, "detik 0.5 (merah) beda dengan 2.5 (biru)");
  const car = await c.call("cutkit_export_carousel", { project: proj, folder: path.join(dir, "slide") });
  assert.equal(car.slides.length, 2, "dua visual = dua slide");
  const slide = fs.readFileSync(car.slides[1]);
  assert.deepEqual([slide.readUInt32BE(16), slide.readUInt32BE(20)], [1080, 1920], "slide ukuran penuh kanvas");
  const out = path.join(dir, "hasil.mp4");
  const ex = await c.call("cutkit_export", { project: proj, output: out, quality: "fast" }, { progressToken: "p1" });
  assert.equal(ex.output, out);
  assert.ok(fs.statSync(out).size > 1000);
  assert.ok(c.notes.some((n) => n.method === "notifications/progress" && n.params.progressToken === "p1"), "ada notifikasi progres");
  c.close();
});
