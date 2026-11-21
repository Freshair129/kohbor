// Local preview server for the Kohbor website prototype. No dependencies.
//   node serve.mjs [port]
// Serves this folder on 127.0.0.1 only, with byte ranges (video seeking needs them).
// POST /__save/<name> writes a rendered film file into assets/film/ (used by studio.html, local only).
import { createServer } from 'node:http';
import { createReadStream, createWriteStream, statSync, mkdirSync } from 'node:fs';
import { extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('.', import.meta.url)));
const PORT = Number(process.argv[2] || 4173);
const TYPES = {
  '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg', '.webp': 'image/webp',
  '.mp4': 'video/mp4', '.webm': 'video/webm', '.md': 'text/markdown; charset=utf-8', '.txt': 'text/plain; charset=utf-8',
};
const SAVE_OK = /^(kohbor-film\.(mp4|webm)|poster\.jpg|still-\d\d\.jpg)$/;

createServer((req, res) => {
  const url = new URL(req.url, 'http://x');
  if (req.method === 'POST' && url.pathname.startsWith('/__save/')) {
    const name = url.pathname.slice('/__save/'.length);
    if (!SAVE_OK.test(name)) { res.writeHead(400).end('name not allowed'); return; }
    mkdirSync(join(ROOT, 'assets', 'film'), { recursive: true });
    const out = createWriteStream(join(ROOT, 'assets', 'film', name));
    let bytes = 0;
    req.on('data', (c) => { bytes += c.length; });
    req.pipe(out);
    out.on('finish', () => { res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ name, bytes })); console.log(`saved ${name} ${bytes} bytes`); });
    out.on('error', (e) => res.writeHead(500).end(String(e)));
    return;
  }
  if (req.method !== 'GET' && req.method !== 'HEAD') { res.writeHead(405).end(); return; }
  let rel = decodeURIComponent(url.pathname);
  if (rel.endsWith('/')) rel += 'index.html';
  const file = normalize(join(ROOT, rel));
  if (file !== ROOT && !file.startsWith(ROOT + sep)) { res.writeHead(403).end(); return; }
  let st;
  try { st = statSync(file); if (!st.isFile()) throw new Error('not a file'); } catch { res.writeHead(404, { 'content-type': 'text/plain' }).end('not found'); return; }
  const head = { 'content-type': TYPES[extname(file).toLowerCase()] || 'application/octet-stream', 'accept-ranges': 'bytes', 'cache-control': 'no-store' };
  const range = /^bytes=(\d*)-(\d*)$/.exec(req.headers.range || '');
  if (range && (range[1] || range[2])) {
    const start = range[1] ? Number(range[1]) : Math.max(0, st.size - Number(range[2]));
    const end = range[1] && range[2] ? Math.min(Number(range[2]), st.size - 1) : st.size - 1;
    if (start > end || start >= st.size) { res.writeHead(416, { 'content-range': `bytes */${st.size}` }).end(); return; }
    res.writeHead(206, { ...head, 'content-range': `bytes ${start}-${end}/${st.size}`, 'content-length': end - start + 1 });
    if (req.method === 'HEAD') { res.end(); return; }
    createReadStream(file, { start, end }).pipe(res);
    return;
  }
  res.writeHead(200, { ...head, 'content-length': st.size });
  if (req.method === 'HEAD') { res.end(); return; }
  createReadStream(file).pipe(res);
}).listen(PORT, '127.0.0.1', () => console.log(`Kohbor website prototype: http://127.0.0.1:${PORT}/`));
