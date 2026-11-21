// Kohbor website — the film.
// A 20-second sequence driven by time instead of scroll, using the same stage as the page.
// studio.html encodes it to a video file; the page plays that file, or runs this live if the file is missing.
import { createStage, ROOMS, R } from './stage.js';

export const FILM = { W: 1600, H: 900, FPS: 30, DUR: 20 };

const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const lerp = (a, b, t) => a + (b - a) * t;
const ease = (t) => t * t * (3 - 2 * t);
const seg = (t, a, b) => ease(clamp((t - a) / (b - a)));
const TAN = Math.tan((35 / 2) * Math.PI / 180);
const TOP = (2 * R + 0.5) / (2 * 0.72 * TAN);         // camera height that fits the map into 72% of the frame height
// The page shows the film full-bleed with object-fit: cover, so anything that matters stays inside the
// central 4:3 column (x 262..1338 of 1600) and clear of the bottom strip where the page puts its controls.

/** Piecewise eased interpolation over [time, value] keys; value is a number or an array. */
function track(keys, t) {
  if (t <= keys[0][0]) return keys[0][1];
  for (let i = 1; i < keys.length; i++) {
    if (t <= keys[i][0]) {
      const [t0, a] = keys[i - 1], [t1, b] = keys[i];
      const k = ease((t - t0) / (t1 - t0));
      return Array.isArray(a) ? a.map((v, j) => lerp(v, b[j], k)) : lerp(a, b, k);
    }
  }
  return keys[keys.length - 1][1];
}

const CAM = [[0, [0, 5.4, 14.5]], [5.4, [-5.8, 2.4, 9.6]], [7.4, [0, TOP, 0.02]], [14.5, [0, TOP, 0.02]], [16.4, [0, 6.2, 12.6]], [18.2, [0, 2.3, 7.4]], [20, [0, 2.3, 6.8]]];
const LOOK = [[0, [0, 0.6, 0]], [5.4, [0, 0.8, 0]], [7.4, [0, 0, 0]], [14.5, [0, 0, 0]], [16.4, [0, 0, 0]], [18.2, [0, 1.7, 0]], [20, [0, 1.7, 0]]];
const SHAPE = [[5.4, 0], [7.3, 1], [14.6, 1], [16.4, 2], [16.6, 2], [18.2, 3]];
const SHIFT = [[0, [0.09, 0]], [5.4, [0.1, 0.03]], [7.4, [0.115, -0.02]], [14.5, [0.115, -0.02]], [16.4, [0.07, 0.04]], [18.2, [0.13, 0]], [18.8, [0.13, 0]], [19.5, [0, -0.04]]];
const ORB = [[0, [1.7, 2.5, 1.4]], [5.4, [1.2, 2.2, 1.0]], [7.4, [-0.38, 0.5, 1.8]], [14.5, [-0.38, 0.5, 1.8]], [16.4, [0, 0.5, 0]], [18.2, [0, 1.7, 0]]];

export function filmParams(t) {
  const cam = track(CAM, t), look = track(LOOK, t);
  const dist = Math.hypot(cam[0] - look[0], cam[1] - look[1], cam[2] - look[2]);
  const bone = t >= 12 && t < 14.5, redCard = t >= 18.9;
  const room = bone ? 'b' : redCard ? 'r' : 'c';
  const rot = t < 7.4 ? lerp(-0.55, 0.3, seg(t, 0, 5.4)) * (1 - seg(t, 5.4, 7.2)) : 0.5 * seg(t, 14.6, 20);
  const speak = t > 17.4 ? seg(t, 17.4, 17.9) * (0.45 + 0.55 * Math.abs(Math.sin(t * 6.1) * Math.sin(t * 2.3 + 1))) : 0;
  return {
    time: t, cam, look, fov: 35, fit: 0, shift: track(SHIFT, t), shape: track(SHAPE, t), height: 1.15,
    psize: (2.3 * dist) / 9, intro: seg(t, 0, 2.6), dim: seg(t, 0, 0.5), rot,
    stale: seg(t, 12.0, 12.25) * (1 - seg(t, 14.3, 14.55)), lost: 0,
    amp: [0.55 + 0.25 * Math.sin(t * 1.7), 0.85, 0.35 + 0.15 * Math.sin(t * 0.9 + 2)], voice: speak, sphereY: 1.7,
    orb: track(ORB, t), orbA: seg(t, 1.6, 2.8) * (bone ? 0 : 1), orbS: 1 + speak * 0.25,
    markers: seg(t, 7.4, 7.5) * (1 - seg(t, 11.95, 12.05)), pop: clamp((t - 7.4) / 1.3), e3: seg(t, 9.0, 9.15),
    frame: seg(t, 7.2, 7.8) * (1 - seg(t, 14.4, 14.8)), frameDash: bone ? 1 : 0,
    pointer: null, rooms: { top: room, bottom: room, split: null },
  };
}

const CAPTIONS = [
  { a: 1.1, b: 4.9, main: ['คุณขับ', 'เราเกาะเบาะ'], sub: 'You drive. We ride along.', size: 112 },
  { a: 7.9, b: 11.7, main: ['เห็น'], sub: 'อ่านสนามได้ในแวบเดียว', en: 'Read the field at a glance.', size: 176 },
  { a: 12.1, b: 14.35, main: ['ไม่รู้', 'ก็บอกว่าไม่รู้'], sub: 'ไม่มีข้อมูลการมองเห็น', en: 'No visibility data — not “safe”', size: 70 },
  { a: 15.0, b: 16.7, main: ['ฟัง'], sub: 'เสียงมาเมื่อมีเหตุ', en: 'Sound arrives when there is a reason.', size: 176 },
  { a: 17.1, b: 18.75, main: ['ถาม'], sub: 'กดค้างไว้ แล้วพูดได้เลย', en: 'Hold a key and speak.', size: 176 },
];
const LABELS = [['own', 'คุณ', '#ede6d8'], ['A1', 'A1', '#7fb0e6'], ['A2', 'A2', '#7fb0e6'], ['E1', 'E1', '#f0914f'], ['E2', 'E2', '#f0914f'], ['E3', 'E3', '#f0914f'], ['E4', 'E4', '#f0914f']];
const FONT = '"IBM Plex Sans Thai", "Leelawadee UI", sans-serif';
const MONO = '"IBM Plex Mono", Consolas, monospace';

export async function loadFilmFonts() {
  if (!document.fonts) return;
  const sample = 'คุณขับเราเกาะเบาะเห็นไม่รู้ก็บอกว่าฟังถาม Kohbor 0123';
  await Promise.all([`700 100px ${FONT}`, `500 30px ${FONT}`, `600 22px ${FONT}`, `500 22px ${MONO}`].map((f) => document.fonts.load(f, sample).catch(() => {})));
}

export function drawOverlay(ctx, t, W, H, stage, P, mark) {
  const s = H / 900;
  const room = P.rooms.top;
  const ink = room === 'b' ? '#1c1c1c' : '#ede6d8';
  const ink2 = room === 'b' ? '#3b3936' : '#cbc4b7';
  ctx.textBaseline = 'alphabetic';

  // marker labels ride on the markers
  if (P.markers > 0.5) {
    LABELS.forEach(([id, text, col], i) => {
      if (P.pop * 9 - i < 1) return;
      const p = stage.projectMarker(id);
      const miss = id === 'E3' && P.e3 > 0.5;
      const label = miss ? `E3  ${Math.max(0, t - 9).toFixed(1)} วิ` : text;
      const c = miss ? '#e9b44c' : col;
      ctx.font = `700 ${22 * s}px ${FONT}`;
      const w = ctx.measureText(label).width + 18 * s, h = 34 * s, x = p.x + 22 * s, y = p.y - 44 * s;
      ctx.fillStyle = 'rgba(20,20,20,0.88)';
      ctx.beginPath(); ctx.roundRect(x, y, w, h, 5 * s); ctx.fill();
      ctx.strokeStyle = c; ctx.lineWidth = 1.5 * s; ctx.setLineDash(miss ? [6 * s, 4 * s] : []);
      ctx.stroke(); ctx.setLineDash([]);
      ctx.fillStyle = c; ctx.fillText(label, x + 9 * s, y + 24.5 * s);
    });
  }

  // chapter captions: left of the safe column, above the page's control strip
  for (const c of CAPTIONS) {
    if (t < c.a || t > c.b) continue;
    const o = seg(t, c.a, c.a + 0.35) * (1 - seg(t, c.b - 0.3, c.b));
    const rise = (1 - seg(t, c.a, c.a + 0.6)) * 26 * s;
    const x = 268 * s;
    let y = H - 196 * s + rise;
    ctx.globalAlpha = o;
    if (c.en) { ctx.font = `400 ${24 * s}px ${FONT}`; ctx.fillStyle = ink2; ctx.fillText(c.en, x, y); y -= 40 * s; }
    ctx.font = `500 ${32 * s}px ${FONT}`; ctx.fillStyle = c.en ? ink : ink2; ctx.fillText(c.sub, x, y);
    y -= 58 * s + c.size * 0.1 * s;
    ctx.font = `700 ${c.size * s}px ${FONT}`; ctx.fillStyle = ink;
    for (let i = c.main.length - 1; i >= 0; i--) { ctx.fillText(c.main[i], x - c.size * 0.03 * s, y); y -= c.size * 1.16 * s; }
    ctx.globalAlpha = 1;
  }

  // closing card: the mark and the name, Bone on Barber Red
  if (t >= 18.9) {
    const o = seg(t, 18.95, 19.4);
    ctx.globalAlpha = o;
    const ms = 150 * s, cx = W * 0.5, cy = H * 0.44;
    ctx.fillStyle = '#b3261e'; ctx.globalAlpha = o * 0.82; ctx.fillRect(0, 0, W, H); ctx.globalAlpha = o;
    if (mark?.complete && mark.naturalWidth) {
      // the mark's own red would vanish on a red field, so it sits on a Charcoal disc
      ctx.fillStyle = '#141414'; ctx.beginPath(); ctx.arc(cx, cy - 6 * s, ms * 0.8, 0, Math.PI * 2); ctx.fill();
      ctx.drawImage(mark, cx - ms / 2, cy - ms / 2 - 6 * s, ms, ms);
    }
    ctx.textAlign = 'center'; ctx.fillStyle = '#ede6d8';
    ctx.font = `700 ${108 * s}px ${FONT}`; ctx.fillText('เกาะเบาะ', cx, cy + 250 * s);
    ctx.font = `400 ${38 * s}px ${FONT}`; ctx.fillText('Kohbor', cx, cy + 312 * s);
    ctx.textAlign = 'left'; ctx.globalAlpha = 1;
  }

  // always on screen: what this footage is
  ctx.font = `500 ${20 * s}px ${FONT}`; ctx.textAlign = 'right';
  ctx.fillStyle = ink2; ctx.globalAlpha = 0.9 * seg(t, 0.3, 0.9);
  ctx.fillText('ภาพจำลอง ไม่ใช่ภาพจากเกม', W - 268 * s, 92 * s);
  ctx.textAlign = 'left'; ctx.globalAlpha = 1;
}

export function renderFilmFrame(ctx, stage, glCanvas, t, W, H, mark) {
  const P = filmParams(t);
  stage.render(P);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  ctx.fillStyle = ROOMS[P.rooms.top].paper;
  ctx.fillRect(0, 0, W, H);
  ctx.drawImage(glCanvas, 0, 0, W, H);
  drawOverlay(ctx, t, W, H, stage, P, mark);
  return P;
}

export function loadMark(src = 'assets/mark.svg') {
  return new Promise((resolve) => { const img = new Image(); img.onload = () => resolve(img); img.onerror = () => resolve(null); img.src = src; });
}

/** Live playback on a 2D canvas: the fallback when no rendered file exists. */
export function createLiveFilm(canvas, { W = 1280, H = 720, density = 180 } = {}) {
  canvas.width = W; canvas.height = H;
  const gl = document.createElement('canvas');
  const stage = createStage(gl, { density });
  stage.resize(W, H, 1);
  const ctx = canvas.getContext('2d');
  let t = 0, playing = false, last = performance.now(), mark = null;
  loadMark().then((m) => { mark = m; });
  loadFilmFonts();
  const tick = (now) => {
    requestAnimationFrame(tick);
    const dt = Math.min(0.1, (now - last) / 1000); last = now;
    if (!playing) return;
    t = (t + dt) % FILM.DUR;
    renderFilmFrame(ctx, stage, gl, t, W, H, mark);
  };
  requestAnimationFrame(tick);
  renderFilmFrame(ctx, stage, gl, 9.6, W, H, mark);
  return { setPlaying: (v) => { playing = v; }, time: () => t, seek: (v) => { t = v; renderFilmFrame(ctx, stage, gl, t, W, H, mark); } };
}
