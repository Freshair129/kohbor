// Generate the original, tiny VP9-with-alpha WebM used by the overlay banner warm-up (see README.md).
// Encodes a colour and an alpha VP9 stream with WebCodecs in headless Edge and muxes them in Node
// (alpha as BlockAdditional). Usage: node gen-warmup.mjs <out.webm> [width] [height] [frames]
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [, , outPath, W = "1280", H = "480", FRAMES = "12"] = process.argv;
const port = 9850 + Math.floor(Math.random() * 100);
// WebCodecs needs a secure context: serve one blank page from loopback for the encoder page.
const page = createServer((_, res) => { res.writeHead(200, { "Content-Type": "text/html" }); res.end("<!doctype html><title>enc</title>"); });
await new Promise((r) => page.listen(0, "127.0.0.1", r));
const pageUrl = `http://127.0.0.1:${page.address().port}/`;
const edge = spawn("C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", [
  "--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "kb-edge-"))}`,
  "--no-first-run", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let t;
for (let i = 0; i < 50; i++) { try { t = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((x) => x.type === "page"); if (t) break; } catch {} await sleep(200); }
const ws = new WebSocket(t.webSocketDebuggerUrl); await new Promise((r) => ws.addEventListener("open", r));
let id = 0; const p = new Map();
ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (p.has(m.id)) { p.get(m.id)(m); p.delete(m.id); } });
const send = (method, params = {}) => new Promise((r) => { const i = ++id; p.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });

const ENCODE = `(async () => {
  const w = ${W}, h = ${H}, n = ${FRAMES};
  const cfg = { codec: 'vp09.00.10.08', width: w, height: h, bitrate: 100000, framerate: 30 };
  const sup = await VideoEncoder.isConfigSupported(cfg);
  if (!sup.supported) return JSON.stringify({ error: 'unsupported', sup });
  const b64 = (u8) => { let s = ''; for (let i = 0; i < u8.length; i += 0x8000) s += String.fromCharCode(...u8.subarray(i, i + 0x8000)); return btoa(s); };
  const encodeAll = async (draw) => {
    const out = [];
    const enc = new VideoEncoder({ output: (c) => { const d = new Uint8Array(c.byteLength); c.copyTo(d); out.push({ type: c.type, ts: c.timestamp, data: b64(d) }); }, error: (e) => out.push({ error: String(e) }) });
    enc.configure(cfg);
    const cv = new OffscreenCanvas(w, h); const g = cv.getContext('2d', { alpha: false });
    for (let i = 0; i < n; i++) { draw(g, i); const f = new VideoFrame(cv, { timestamp: Math.round(i * 1e6 / 30) }); enc.encode(f, { keyFrame: i === 0 }); f.close(); }
    await enc.flush(); enc.close(); return out;
  };
  // colour: black; alpha: black (fully transparent apart from codec range rounding), faint moving bar for real inter frames
  const color = await encodeAll((g, i) => { g.fillStyle = '#000'; g.fillRect(0, 0, w, h); g.fillStyle = '#111'; g.fillRect((i * 32) % w, 0, 64, h); });
  const alpha = await encodeAll((g, i) => { g.fillStyle = '#000'; g.fillRect(0, 0, w, h); g.fillStyle = '#080808'; g.fillRect((i * 32) % w, 0, 64, h); });
  if (color.length !== alpha.length) return JSON.stringify({ error: 'count mismatch', c: color.length, a: alpha.length });
  return JSON.stringify({ chunks: color.map((c, i) => ({ ...c, alpha: alpha[i].type === c.type ? alpha[i].data : null })) });
})()`;
await send("Page.navigate", { url: pageUrl }); await sleep(800);
const r = await send("Runtime.evaluate", { expression: ENCODE, awaitPromise: true, returnByValue: true });
edge.kill(); page.close();
const raw = r.result?.result?.value; if (typeof raw !== "string") { console.log(JSON.stringify(r).slice(0, 600)); process.exit(1); } const res = JSON.parse(raw);
if (res.error || !res.chunks.length || res.chunks.some((c) => c.error || !c.alpha)) { console.log(JSON.stringify(res).slice(0, 400)); process.exit(1); }

// --- minimal EBML / WebM writer ---
const vintSize = (n) => { for (let l = 1; l <= 8; l++) if (n < 2 ** (7 * l) - 1) { const b = Buffer.alloc(l); let v = n; for (let i = l - 1; i >= 0; i--) { b[i] = v & 0xff; v = Math.floor(v / 256); } b[0] |= 0x80 >> (l - 1); return b; } throw new Error("size"); };
const idBuf = (id) => { const h = id.toString(16); return Buffer.from(h.length % 2 ? "0" + h : h, "hex"); };
const el = (id, payload) => Buffer.concat([idBuf(id), vintSize(payload.length), payload]);
const uint = (id, v) => { let h = v.toString(16); if (h.length % 2) h = "0" + h; return el(id, Buffer.from(h, "hex")); };
const str = (id, s) => el(id, Buffer.from(s, "ascii"));
const f64 = (id, v) => { const b = Buffer.alloc(8); b.writeDoubleBE(v); return el(id, b); };
const W_ = Number(W), H_ = Number(H);
const header = el(0x1a45dfa3, Buffer.concat([uint(0x4286, 1), uint(0x42f7, 1), uint(0x42f2, 4), uint(0x42f3, 8), str(0x4282, "webm"), uint(0x4287, 4), uint(0x4285, 2)]));
const durationMs = res.chunks.length * 1000 / 30;
const info = el(0x1549a966, Buffer.concat([uint(0x2ad7b1, 1000000), f64(0x4489, durationMs), str(0x4d80, "kohbor-warmup"), str(0x5741, "kohbor-warmup")]));
const video = el(0xe0, Buffer.concat([uint(0xb0, W_), uint(0xba, H_), uint(0x53c0, 1)]));
const track = el(0xae, Buffer.concat([uint(0xd7, 1), uint(0x73c5, 1), uint(0x9c, 0), str(0x86, "V_VP9"), uint(0x83, 1), uint(0x23e383, 33333333), video]));
const tracks = el(0x1654ae6b, track);
const groups = res.chunks.map((c) => {
  const data = Buffer.from(c.data, "base64");
  const rel = Math.round(c.ts / 1000);
  const blockHead = Buffer.from([0x81, (rel >> 8) & 0xff, rel & 0xff, 0x00]);
  const block = el(0xa1, Buffer.concat([blockHead, data]));
  const more = el(0xa6, Buffer.concat([uint(0xee, 1), el(0xa5, Buffer.from(c.alpha, "base64"))]));
  const parts = [block, el(0x75a1, more), uint(0x9b, 33)];
  if (c.type !== "key") parts.push(el(0xfb, Buffer.from([0x00]))); // ReferenceBlock (any value marks non-key)
  return el(0xa0, Buffer.concat(parts));
});
const cluster = el(0x1f43b675, Buffer.concat([uint(0xe7, 0), ...groups]));
const segment = el(0x18538067, Buffer.concat([info, tracks, cluster]));
const file = Buffer.concat([header, segment]);
writeFileSync(outPath, file);
console.log("wrote", outPath, file.length, "bytes,", res.chunks.length, "frames, alpha on all:", res.chunks.every((c) => c.alpha));
process.exit(0);
