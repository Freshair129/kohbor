// Kohbor website — scroll score and interactions.
// The page scrolls natively. Nothing is hijacked: the scroll position is read once per frame and
// turned into (a) one blended camera/scene key for the stage and (b) a few per-chapter values.
import { createStage, MARKERS, R } from './stage.js';

const $ = (s, el = document) => el.querySelector(s);
const $$ = (s, el = document) => [...el.querySelectorAll(s)];
const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const lerp = (a, b, t) => a + (b - a) * t;
const ease = (t) => t * t * (3 - 2 * t);
const root = document.documentElement;
const FOV = 35, TAN = Math.tan((FOV / 2) * Math.PI / 180);

// ───────────── language and motion ─────────────
const store = { get: (k) => { try { return localStorage.getItem(k); } catch { return null; } }, set: (k, v) => { try { localStorage.setItem(k, v); } catch { /* private mode */ } } };
let lang = store.get('kohbor.lang') === 'en' ? 'en' : 'th';
const L = (o) => o[lang];
const mqReduce = matchMedia('(prefers-reduced-motion: reduce)');
let reduce = mqReduce.matches || store.get('kohbor.rm') === '1';

// The exact words the app shows for each state of the map picture (Command Deck design notes, §3.1).
const STATES = {
  live: { line: { th: 'ภาพสด', en: 'Live' }, desc: { th: 'เห็นภาพ กรอบเส้นทึบ เครื่องหมายครบ', en: 'A picture, a solid frame, every marker.' }, warn: false, stale: 0, lost: 0, markers: 1, frame: 1, dash: 0, plate: null },
  stale: { line: { th: 'ภาพแผนที่เก่า · 3.2 วิ', en: 'Map image is stale · 3.2 s' }, desc: { th: 'ภาพล่าสุดถูกหรี่และขีดทับ เครื่องหมายถูกเอาออก เพราะตำแหน่งนั้นอาจไม่จริงแล้ว', en: 'The last frame is dimmed and hatched. Markers are removed, because those positions may no longer be true.' }, warn: true, stale: 1, lost: 0, markers: 0, frame: 1, dash: 1, plate: null },
  lost: { line: { th: 'จับภาพมินิแมปไม่ได้', en: 'Minimap capture lost' }, desc: { th: 'ไม่มีภาพให้ดู มีแต่ป้ายที่บอกเหตุ', en: 'No picture. Only a plate that says why.' }, warn: true, stale: 0, lost: 1, markers: 0, frame: 0.7, dash: 1, plate: { th: 'จับภาพมินิแมปไม่ได้', en: 'Minimap capture lost' } },
  none: { line: { th: 'รอเข้าเกม', en: 'Waiting for game' }, desc: { th: 'ไม่มีรายชื่อปลอม ไม่มีไอเทมสมมติ ไม่มีฮีโร่ที่เลือกไว้ให้ล่วงหน้า', en: 'No made-up roster, no pretend inventory, no hero picked for you in advance.' }, warn: false, stale: 0, lost: 1, markers: 0, frame: 0, dash: 0, plate: { th: 'รอเข้าเกม', en: 'Waiting for game' } },
};
const E3 = { seen: { th: '◆ เห็นอยู่', en: '◆ Seen now' }, missing: { th: '◇ หายจากแผนที่', en: '◇ Missing' } };

// ───────────── elements ─────────────
const stageEl = $('#stage');
const sections = $$('main > section').map((el) => ({ el, id: el.id, room: el.dataset.room, pin: $('.pin', el), slot: $('[data-slot]', el), top: 0, h: 0, box: null }));
const byId = Object.fromEntries(sections.map((s, i) => [s.id, { ...s, i }]));
sections.forEach((s, i) => { s.i = i; });
const labels = $$('.lb').map((el) => ({ el, id: el.dataset.m, o: -1 }));
const plate = $('#plate'), plateText = $('#plateText');
const navLinks = $$('.top-nav a');
const topEl = $('#top');
const NAV_FOR = { see: '#see', honest: '#honest', hear: '#hear', ask: '#ask', features: '#features', yours: '#yours', bar: '#bar' };
let topRoom = 'c';

let vw = 1, vh = 1, cw = 1, ch = 1, cl = 0, ct = 0, mobile = false;
let stage = null;
try {
  stage = createStage(stageEl, { density: innerWidth <= 860 ? 132 : 256 });
} catch (err) {
  root.classList.add('no-gl');
  console.warn('WebGL is not available; the page continues without the 3D stage.', err);
}

// ───────────── words: split once per language, reveal by scroll ─────────────
const wordSets = new Map();
function splitWords() {
  $$('.words').forEach((host) => {
    const span = $(`.${lang}`, host);
    if (!span || span.dataset.split) { wordSets.set(host, $$('.w', span)); return; }
    const text = span.textContent.trim();
    const frag = document.createDocumentFragment();
    // punctuation joins the word before it, so a comma never lights up ahead of its sentence
    const push = (t, word) => {
      if (word) { const w = document.createElement('span'); w.className = 'w'; w.textContent = t; frag.append(w); }
      else if (/\S/.test(t) && frag.lastChild?.nodeType === 1) frag.lastChild.textContent += t;
      else frag.append(t);
    };
    if (typeof Intl !== 'undefined' && Intl.Segmenter) {
      for (const seg of new Intl.Segmenter(lang, { granularity: 'word' }).segment(text)) push(seg.segment, seg.isWordLike);
    } else text.split(/(\s+)/).forEach((t) => push(t, /\S/.test(t)));
    span.textContent = '';
    span.append(frag);
    span.dataset.split = '1';
    wordSets.set(host, $$('.w', span));
  });
  wordsShown = -1;
}
let wordsShown = -1;

// ───────────── layout ─────────────
const K = {};      // one scene key per chapter
function measure() {
  vw = innerWidth; vh = innerHeight; mobile = vw <= 860;
  const cr = stageEl.getBoundingClientRect();
  cw = cr.width || vw; ch = cr.height || vh; cl = cr.left; ct = cr.top;
  for (const s of sections) {
    const r = s.el.getBoundingClientRect();
    s.top = r.top + scrollY; s.h = r.height;
    if (s.slot && s.pin) {
      const pr = s.pin.getBoundingClientRect(), sr = s.slot.getBoundingClientRect();
      s.box = { cx: sr.left - pr.left + sr.width / 2, cy: sr.top - pr.top + sr.height / 2, w: sr.width, h: sr.height };
    }
  }
  stage?.resize(Math.round(cw), Math.round(ch), Math.min(devicePixelRatio || 1, 2));
  buildKeys();
}

const BASE = { cam: [0, 4.6, 12], look: [0, 0.5, 0], shift: [0, 0], shape: 0, height: 1, dim: 1, px: 2, fit: 1.3, rotAmt: 1, markers: 0, frame: 0, orb: [1.7, 2.5, 1.4], orbA: 0, orbS: 1, ripple: 0, follow: 0, heroes: 0 };
const own = MARKERS.find((m) => m.id === 'own');
function slotShift(id, ky = 1) { const b = byIdNow(id).box; return b ? [(b.cx - cw / 2) / cw, ((b.cy - ch / 2) / ch) * ky] : [0, 0]; }
function byIdNow(id) { return sections.find((s) => s.id === id); }
function topDown(id, tilt) {
  const b = byIdNow(id).box || { w: cw, h: ch };
  const side = Math.max(120, Math.min(b.w, b.h) * 0.95);
  const d = ((2 * R + 0.5) * ch) / (2 * side * TAN);
  return { cam: [0, d * Math.cos(tilt), d * Math.sin(tilt) + 0.02], look: [0, 0, 0], fit: 0 };
}
function buildKeys() {
  const m = mobile;
  K.hero = { ...BASE, cam: [0, 9.0, 9.6], look: [0, 0, 0.2], height: 1.2, shift: m ? [0, -0.2] : [0.25, 0.05], orbA: 1, ripple: 1, follow: 1, heroes: 1 };
  K.why = { ...BASE, cam: [-8.5, 2.1, 9], look: [0, 0.9, 0], shift: m ? [0, -0.12] : [0.31, 0.2], height: 1.2, dim: 0.34, ripple: 0.5, orb: [0.4, 2.2, 0.6], orbA: 0 };
  K.see = { ...BASE, ...topDown('see', 0), shift: slotShift('see'), shape: 1, px: m ? 1.5 : 1.9, rotAmt: 0, markers: 1, frame: 1, orb: [own.g[0] * R + 0.62, 0.5, own.g[1] * R + 0.5], orbA: 1, orbS: 0.75, ripple: 0.55 };
  K.honest = { ...BASE, ...topDown('honest', 0.2), shift: slotShift('honest'), shape: 1, px: m ? 1.5 : 1.9, rotAmt: 0, markers: 1, frame: 1, orb: [own.g[0] * R + 0.62, 0.5, own.g[1] * R + 0.5], orbA: 0, orbS: 0.75, ripple: 0.2 };
  K.hear = { ...BASE, cam: [0, 6.8, 14], look: [0, 0, 0], shift: slotShift('hear', m ? 1 : 0.4), shape: 2, px: 2.1, rotAmt: 0.5, orb: [0, 0.5, 0], orbA: 1, orbS: 1.1 };
  K.ask = { ...BASE, cam: [0, 2.3, m ? 6.4 : 7.4], look: [0, 1.7, 0], shift: slotShift('ask'), shape: 3, px: 2.1, orb: [0, 1.7, 0], orbA: 1, orbS: 1.3 };
  K.features = { ...BASE, cam: [0, 3.4, 11], look: [0, -0.2, -2], shape: 4, dim: 0.3, px: 2.3, rotAmt: 0.3, ripple: 0.4, orb: [0, 1.7, 0] };
  K.yours ={ ...BASE, cam: [0, 2.6, 9.5], look: [0, 0.1, -3], shape: 4, dim: 0.9, px: 2.3, rotAmt: 0.3, ripple: 0.6, orb: [0, 1.7, 0] };
  K.film = { ...K.yours, dim: 0, ripple: 0 };
  K.bar = { ...BASE, cam: [0, 3.4, 11], look: [0, -0.2, -2], shape: 4, dim: 0.34, px: 2.3, rotAmt: 0.3, ripple: 0.4, orb: [0, 1.7, 0] };
  K.end = { ...BASE, cam: [0, 9.0, 11.5], look: [0, 0.9, 0], height: 1.2, shift: m ? [0, 0.16] : [0.26, 0.0], shape: 5, orbA: 1, ripple: 1, follow: 1, heroes: 1 };
}
function mix(a, b, t) {
  const o = {};
  for (const k in a) { const x = a[k], y = b[k]; o[k] = Array.isArray(x) ? x.map((v, i) => lerp(v, y[i], t)) : lerp(x, y, t); }
  return o;
}
function approach(cur, tgt, k) {
  for (const key in tgt) {
    const x = tgt[key];
    if (Array.isArray(x)) cur[key] = cur[key] ? cur[key].map((v, i) => lerp(v, x[i], k)) : [...x];
    else cur[key] = cur[key] == null ? x : lerp(cur[key], x, k);
  }
}

// ───────────── live state ─────────────
const cur = {};                                                        // smoothed scene key
const ext = { stale: 0, lost: 0, mk: 1, fr: 1, dash: 0, e3: 0, pop: 0, voice: 0, amp: [0.55, 0.8, 0.3], plate: 0, ps: 0, px: 0, pz: 0 };
const mixer = [0.55, 0.8, 0.3];
const hon = { override: null, shown: null };
const ptr = { nx: 0.3, ny: 0.1, at: -1e9, x: 1.2, z: 0.6 };
const drag = { on: false, x: 0, vel: 0, user: 0, auto: 0 };
let held = false, t0 = performance.now(), last = t0, time = 0, intro = 0, started = false, activeId = '', e3Missing = null, e3AgeShown = '', noShown = -1, lastProg = -1;
let filmPaused = false, filmFallback = null;
const video = $('#filmVideo'), filmFrame = $('#filmFrame'), filmTime = $('#filmTime'), filmToggle = $('#filmToggle');

function setHonest(name) {
  if (hon.shown === name + lang) return;
  hon.shown = name + lang;
  const S = STATES[name];
  $$('#stateSeg button').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.state === name)));
  $('#stateLine').textContent = L(S.line);
  $('#stateDesc').textContent = L(S.desc);
  $('#stateWarn').classList.toggle('on', S.warn);
  plateText.textContent = S.plate ? L(S.plate) : '';
}
function setE3(missing, age) {
  if (missing !== e3Missing) {
    e3Missing = missing;
    $('#e3card').className = `ec ${missing ? 'is-missing' : 'is-seen'}`;
    $('#e3state').textContent = L(missing ? E3.missing : E3.seen);
    $('#missCount').textContent = missing ? '1' : '0';
    $('.deck-count').classList.toggle('has-missing', missing);
    labels.find((l) => l.id === 'E3').el.classList.toggle('is-missing', missing);
  }
  const txt = missing ? age.toFixed(1) : '0.2';
  if (txt !== e3AgeShown) { e3AgeShown = txt; $('#e3age').textContent = txt; $('#e3lb').textContent = missing ? `${txt} ${lang === 'th' ? 'วิ' : 's'}` : ''; }
}

function frame(now) {
  requestAnimationFrame(frame);
  const dt = Math.min(0.1, (now - last) / 1000); last = now;
  if (!reduce) time += dt;

  const y = scrollY;
  // which chapter holds the middle of the viewport, and how far through its pinned travel we are
  let act = sections[0];
  for (const s of sections) if (y + vh * 0.5 >= s.top) act = s;
  const q = (s) => clamp((y - s.top) / Math.max(1, s.h - vh));
  const qa = q(act);

  // blended key: each chapter arrives during the viewport-height before it pins
  let tgt = K[sections[0].id];
  for (let i = 1; i < sections.length; i++) {
    const t = clamp((y + vh - sections[i].top) / vh);
    if (t <= 0) break;
    tgt = t >= 1 ? K[sections[i].id] : mix(tgt, K[sections[i].id], ease(t));
  }
  const k = reduce ? 1 : 1 - Math.exp(-dt * 5.5);
  approach(cur, tgt, started ? k : 1);

  // rooms: where a boundary between two colours crosses the stage, the ink flips exactly on that line
  let rooms = { top: act.room, bottom: act.room, split: null };
  for (let i = 0; i < sections.length - 1; i++) {
    const by = sections[i + 1].top - y - ct;
    if (by > 0 && by < ch && sections[i].room !== sections[i + 1].room) rooms = { top: sections[i].room, bottom: sections[i + 1].room, split: by };
  }

  // ── per-chapter values ──
  const see = byIdNow('see'), honest = byIdNow('honest'), why = byIdNow('why'), yours = byIdNow('yours'), film = byIdNow('film');
  const qSee = q(see);
  const afterSee = act.i > see.i;
  const e3On = afterSee || (act === see && qSee > 0.5);
  setE3(e3On && !afterSee ? true : e3On, clamp((qSee - 0.5) / 0.5) * 12);

  const inHonest = act === honest;
  if (!inHonest) hon.override = null;
  const qh = q(honest);
  const hName = inHonest ? (hon.override ?? (qh < 0.26 ? 'live' : qh < 0.5 ? 'stale' : qh < 0.76 ? 'lost' : 'none')) : 'live';
  if (inHonest || hon.shown == null) setHonest(hName);
  const S = STATES[hName];

  const ke = reduce ? 1 : 1 - Math.exp(-dt * 9);
  ext.stale = lerp(ext.stale, S.stale, ke); ext.lost = lerp(ext.lost, S.lost, ke);
  ext.mk = lerp(ext.mk, S.markers, ke); ext.fr = lerp(ext.fr, S.frame, ke); ext.dash = S.dash;
  ext.e3 = lerp(ext.e3, e3On ? 1 : 0, ke);
  ext.pop = act.i < see.i ? 0 : act === see ? clamp(qSee / 0.2) : 1;
  ext.plate = lerp(ext.plate, inHonest && S.plate ? 1 : 0, ke);
  const speak = held ? 0.5 + 0.5 * Math.abs(Math.sin(time * 6.1) * Math.sin(time * 2.3 + 1)) : 0;
  ext.voice = lerp(ext.voice, speak, reduce ? 1 : 1 - Math.exp(-dt * 12));
  for (let i = 0; i < 3; i++) ext.amp[i] = lerp(ext.amp[i], mixer[i], ke);

  // manifesto words
  const words = wordSets.get($('.manifesto'));
  if (words) {
    const n = reduce ? words.length : Math.floor(clamp(q(why) / 0.82) * (words.length + 1));
    if (n !== wordsShown) { words.forEach((w, i) => w.classList.toggle('on', i < n)); wordsShown = n; $('.aside-note').classList.toggle('on', n >= words.length); }
  }
  // the "No" ticker
  const qy = q(yours);
  const ni = clamp(qy / 0.84) * 7;
  const nList = $('#noList');
  nList.style.setProperty('--i', ni.toFixed(3));
  const nr = Math.round(ni);
  if (nr !== noShown) { noShown = nr; [...nList.children].forEach((li, i) => li.classList.toggle('on', i === nr)); $('#noIdx').textContent = String(nr + 1); }
  $('#noClose').classList.toggle('on', qy > 0.9);
  // film: the frame opens with the scroll; it plays only while it is the chapter on screen
  filmFrame.style.setProperty('--f', ease(clamp(q(film) / 0.55)).toFixed(3));
  const filmOn = act === film && !filmPaused;
  if (filmFallback) filmFallback.setPlaying(filmOn);
  else if (filmOn && video.paused) video.play().catch(() => {});
  else if (!filmOn && !video.paused) video.pause();
  const ft = filmFallback ? filmFallback.time() : video.currentTime || 0;
  filmTime.textContent = `${ft.toFixed(1).padStart(4, '0')} / 20.0`;

  // header: it takes the ink of the room under it, and marks the chapter on screen
  let under = sections[0];
  for (const s of sections) if (y + 34 >= s.top) under = s;
  if (under.room !== topRoom) { topRoom = under.room; topEl.dataset.room = topRoom; }
  if (act.id !== activeId) {
    activeId = act.id;
    navLinks.forEach((a) => a.setAttribute('aria-current', String(a.getAttribute('href') === NAV_FOR[act.id])));
  }
  const prog = clamp(y / Math.max(1, document.documentElement.scrollHeight - vh));
  if (Math.abs(prog - lastProg) > 0.0005) { lastProg = prog; root.style.setProperty('--prog', prog.toFixed(4)); }

  if (!stage) return;

  // pointer: a ripple on the field, and the orb rides along behind it
  const fresh = now - ptr.at < 2600 && !reduce;
  const g = stage.ground(ptr.nx, ptr.ny);
  if (g && fresh) { ptr.x = lerp(ptr.x, clamp(g.x, -6, 6), 0.2); ptr.z = lerp(ptr.z, clamp(g.z, -6, 6), 0.2); }
  ext.ps = lerp(ext.ps, fresh ? cur.ripple : 0, 1 - Math.exp(-dt * 4));
  const fol = cur.follow * (fresh ? 1 : 0.35);
  const orbT = [lerp(cur.orb[0], ptr.x, fol * 0.8), cur.orb[1] + Math.sin(time * 1.1) * 0.08, lerp(cur.orb[2], ptr.z, fol * 0.8)];
  ext.orb = ext.orb ? ext.orb.map((v, i) => lerp(v, orbT[i], reduce ? 1 : 1 - Math.exp(-dt * 3.2))) : orbT;

  // rotation: slow drift plus whatever the visitor gave it
  if (!reduce) drag.auto += dt * 0.045;
  if (!drag.on) { drag.user += drag.vel; drag.vel *= 0.94; }
  let rot = (drag.auto + drag.user) % (Math.PI * 2);
  if (rot > Math.PI) rot -= Math.PI * 2; else if (rot < -Math.PI) rot += Math.PI * 2;

  if (!started) { started = true; t0 = now; }
  intro = reduce ? 1 : clamp((now - t0) / 2600);

  const dist = Math.hypot(cur.cam[0] - cur.look[0], cur.cam[1] - cur.look[1], cur.cam[2] - cur.look[2]);
  if (cur.dim > 0.004 || cur.orbA > 0.004) {
    stage.render({
      time, cam: cur.cam, look: cur.look, fov: FOV, shift: cur.shift, fit: cur.fit, shape: cur.shape, height: cur.height,
      psize: (cur.px * dist) / 9, intro: ease(intro), dim: cur.dim, rot: rot * cur.rotAmt,
      stale: ext.stale, lost: ext.lost, amp: ext.amp, voice: ext.voice, sphereY: 1.7,
      orb: ext.orb, orbA: cur.orbA * ease(clamp(intro * 1.6 - 0.6)), orbS: cur.orbS * (1 + ext.voice * 0.25),
      markers: cur.markers * ext.mk, pop: ext.pop, e3: ext.e3, frame: cur.frame * ext.fr, frameDash: ext.dash,
      pointer: { x: ptr.x, z: ptr.z, s: ext.ps }, rooms, heroes: cur.heroes,
    });
  } else stage.renderer.clear();

  // labels ride on the markers
  const mAlpha = cur.markers * ext.mk;
  labels.forEach((l, i) => {
    const on = mAlpha > 0.6 && ext.pop * (MARKERS.length + 2) - i >= 1 ? 1 : 0;
    if (on) { const p = stage.projectMarker(l.id); l.el.style.transform = `translate(${(p.x + 15).toFixed(1)}px, ${(p.y - 27).toFixed(1)}px)`; }
    if (on !== l.o) { l.o = on; l.el.style.opacity = on; }
  });
  if (ext.plate > 0.01) { const p = stage.projectLocal(0, 0, 0); plate.style.transform = `translate(${p.x.toFixed(1)}px, ${p.y.toFixed(1)}px) translate(-50%, -50%)`; }
  plate.style.opacity = ext.plate > 0.5 ? 1 : 0;
}


// ───────────── interactions ─────────────
function setLang(l) {
  lang = l; root.dataset.lang = l; root.lang = l; store.set('kohbor.lang', l);
  $$('[data-set-lang]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.setLang === l)));
  splitWords();
  hon.shown = null; e3Missing = null; e3AgeShown = '';
  requestAnimationFrame(measure);                                      // text lengths changed, so the slots moved
}
$$('[data-set-lang]').forEach((b) => b.addEventListener('click', () => setLang(b.dataset.setLang)));

const motionBtn = $('#motionToggle');
function setReduce(v) { reduce = v; root.classList.toggle('rm', v); motionBtn.setAttribute('aria-pressed', String(v)); wordsShown = -1; }
motionBtn.addEventListener('click', () => { setReduce(!reduce); store.set('kohbor.rm', reduce ? '1' : '0'); });
mqReduce.addEventListener('change', (e) => setReduce(e.matches));

addEventListener('pointermove', (e) => {
  ptr.nx = ((e.clientX - cl) / cw) * 2 - 1; ptr.ny = -(((e.clientY - ct) / ch) * 2 - 1); ptr.at = performance.now();
  if (drag.on) { const dx = e.clientX - drag.x; drag.x = e.clientX; drag.vel = dx * 0.0045; drag.user += drag.vel; }
}, { passive: true });
addEventListener('pointerdown', (e) => {
  if (e.button !== 0 || e.target.closest('button, a, input, label, video, .top')) return;
  drag.on = true; drag.x = e.clientX; drag.vel = 0;
});
const endDrag = () => { drag.on = false; };
addEventListener('pointerup', endDrag); addEventListener('pointercancel', endDrag); addEventListener('blur', endDrag);

$$('#stateSeg button').forEach((b) => b.addEventListener('click', () => { hon.override = b.dataset.state; }));

$$('.ch input').forEach((input) => {
  const out = input.parentElement.querySelector('output');
  const sync = () => { const v = Number(input.value); mixer[Number(input.dataset.ch)] = v / 100; out.textContent = String(v); input.style.setProperty('--v', `${v}%`); };
  input.addEventListener('input', sync); sync();
});

const holdBtn = $('#holdBtn');
const setHeld = (v) => { held = v; holdBtn.setAttribute('aria-pressed', String(v)); };
holdBtn.addEventListener('pointerdown', (e) => { holdBtn.setPointerCapture(e.pointerId); setHeld(true); });
['pointerup', 'pointercancel', 'lostpointercapture', 'blur'].forEach((t) => holdBtn.addEventListener(t, () => setHeld(false)));
holdBtn.addEventListener('keydown', (e) => { if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); setHeld(true); } });
holdBtn.addEventListener('keyup', (e) => { if (e.key === ' ' || e.key === 'Enter') setHeld(false); });
holdBtn.addEventListener('contextmenu', (e) => e.preventDefault());

filmToggle.addEventListener('click', () => { filmPaused = !filmPaused; filmToggle.setAttribute('aria-pressed', String(filmPaused)); });
// If the rendered film file is missing, play the same sequence live from the same code.
async function useLiveFilm() {
  if (filmFallback || !stage) return;
  try {
    const { createLiveFilm } = await import('./film.js');
    const canvas = $('#filmLive');
    video.hidden = true; canvas.hidden = false;
    filmFallback = createLiveFilm(canvas, () => lang);
  } catch (err) { console.warn('Film fallback could not start.', err); }
}
const lastSource = $$('source', video).pop();
lastSource?.addEventListener('error', useLiveFilm);
video.addEventListener('error', useLiveFilm);
// The source error can fire before this module runs (the films are not in git), so also check the state now.
if (video.error || video.networkState === HTMLMediaElement.NETWORK_NO_SOURCE) useLiveFilm();

// ───────────── start ─────────────
root.dataset.lang = lang; root.lang = lang;
$$('[data-set-lang]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.setLang === lang)));
setReduce(reduce);
splitWords();
measure();
addEventListener('resize', () => { clearTimeout(measure.t); measure.t = setTimeout(measure, 120); });
const fontsReady = document.fonts?.ready ?? Promise.resolve();
Promise.race([fontsReady, new Promise((r) => setTimeout(r, 1500))]).then(() => {
  measure();
  // The loader lifts on a timer, not on a frame: a background tab gets no frames, and must not sit behind a loader.
  setTimeout(() => root.classList.add('ready'), 60);
  requestAnimationFrame((now) => { last = now; frame(now); });
});
addEventListener('load', () => setTimeout(measure, 60));
