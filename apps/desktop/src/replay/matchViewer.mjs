// Lab Match view over validated replay-library bundles, with patch-scoped tables and calibration.
import { matchViewerCopy } from "./matchViewerCopy.mjs";
import { baseAttackSpeed, isConsumableItem, isControlEffect, isMobilityAbility, isMobilityItem, isNotableItem, isUltimateAbility } from "./replayPack.mjs";
import { heroCardView } from "../analytics/heroCardView.mjs";

export function mountMatchView(root, bundle, { locale, markup, style, icon, statCatalog, statIcon, hasMap, header, patchData, mapCalibration, initialTime = null, disableKeyboard = false, statLabelMode = "icon" }) {
  const viewerStrings = matchViewerCopy[locale].viewerStrings;
  const translationEntries = Object.entries(viewerStrings).sort(([left], [right]) => right.length - left.length);
  const translate = value => {
    const text = String(value ?? "");
    if (locale !== "en") return text;
    if (viewerStrings[text]) return viewerStrings[text];
    return translationEntries.reduce((result, [thai, englishText]) => result.includes(thai) ? result.replaceAll(thai, englishText) : result, text);
  };
  const template = window.document.createElement("template");
  template.innerHTML = markup;
  for (const element of template.content.querySelectorAll("[aria-label],[title]")) {
    for (const name of ["aria-label", "title"]) if (element.hasAttribute(name)) element.setAttribute(name, translate(element.getAttribute(name)));
  }
  const walk = window.document.createTreeWalker(template.content, NodeFilter.SHOW_TEXT);
  for (let node = walk.nextNode(); node; node = walk.nextNode()) {
    const key = node.nodeValue.trim();
    if (key) node.nodeValue = node.nodeValue.replace(key, translate(key));
  }
  const styles = window.document.createElement("style");
  styles.textContent = style;
  root.replaceChildren(styles, template.content);
  for (const [id, value] of Object.entries(header)) { const el = root.getElementById(id); if (el) el.textContent = value; }

  let alive = true;
  const cleanups = [];
  const translateNode = node => {
    if (node.nodeType === window.Node.TEXT_NODE) {
      const next = translate(node.nodeValue);
      if (next !== node.nodeValue) node.nodeValue = next;
      return;
    }
    if (node.nodeType !== window.Node.ELEMENT_NODE) return;
    for (const name of ["title", "aria-label"]) {
      if (node.hasAttribute(name)) {
        const next = translate(node.getAttribute(name));
        if (next !== node.getAttribute(name)) node.setAttribute(name, next);
      }
    }
    for (const child of node.childNodes) translateNode(child);
  };
  const copyObserver = new window.MutationObserver(mutations => {
    for (const mutation of mutations) {
      if (mutation.type === "characterData") translateNode(mutation.target);
      else if (mutation.type === "attributes") translateNode(mutation.target);
      else for (const node of mutation.addedNodes) translateNode(node);
    }
  });
  copyObserver.observe(root, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["title", "aria-label"] });
  cleanups.push(() => copyObserver.disconnect());
  const on = (target, type, fn, options) => { target.addEventListener(type, fn, options); cleanups.push(() => target.removeEventListener(type, fn, options)); };
  const editing = ev => { const el = ev.composedPath()[0]; return el instanceof Element && !!el.closest("input, select, textarea"); };
  const requestAnimationFrame = cb => alive ? window.requestAnimationFrame(cb) : 0;
  const setTimeout = (cb, ms) => alive ? window.setTimeout(cb, ms) : 0;
  class ResizeObserver extends window.ResizeObserver { constructor(cb){ super(cb); cleanups.push(() => this.disconnect()); } }
  // The prototype addressed one page; inside the shadow root these resolve against it.
  const document = {
    getElementById: id => root.getElementById(id),
    querySelectorAll: sel => root.querySelectorAll(sel),
    createElement: tag => window.document.createElement(tag),
    documentElement: root.host,
  };

  const $ = id => document.getElementById(id);
  const cv = $("rv-canvas"), ctx = cv.getContext("2d"), wrap = $("rv-mapwrap"), tip = $("rv-tip"), status = $("rv-status");
  const startTime = Number.isFinite(initialTime) ? Math.max(bundle.match.t0, Math.min(bundle.match.t1, initialTime)) : bundle.match.t0;
  const S = {D: null, t: startTime, playing: false, speed: 10, mode: "route", heatStyle: "hero", trail: 45, range: [bundle.match.t0, bundle.match.t1],
    vis: new Set(), focus: null, follow: false, names: false, autoPause: false, stopAt: null, review: null, lastCardKey: "",
    view: {k: 1, ox: 0, oy: 0}, img: null, port: {}, alerts: [], hits: [], W: null, dmgFloat: true, fog: "all", patchData, mapCalibration,
    layers: {lane: true, neut: true, rune: true, ward: true, bld: true, rosh: true}};
  if (!patchData.ultimate_classification.available) {
    const ultimateCopy = $("al-ult-copy");
    if (ultimateCopy) ultimateCopy.textContent = header["al-ult-copy"] ?? "";
  }
  const css = n => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  const clk = t => { const s = Math.round(t), a = Math.abs(s); return (s < 0 ? "−" : "") + String(Math.floor(a / 60)).padStart(2, "0") + ":" + String(a % 60).padStart(2, "0"); };
  const fmt = v => Number.isFinite(v) ? Math.round(v).toLocaleString("en-US") : "—";
  const esc = s => String(s).replace(/[&<>"]/g, c => ({"&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;"}[c]));
  const statDefinition = key => statCatalog?.[key] ?? { label: key.toUpperCase(), accessibleLabel: key.toUpperCase(), iconIds: [] };
  const statGlyphs = key => {
    const def = statDefinition(key);
    if (statLabelMode === "text") return `<span class="stat-glyph" data-stat-label="text" role="img" aria-label="${esc(def.accessibleLabel)}">${esc(def.label)}</span>`;
    const images = def.iconIds.map(id => statIcon?.(id)).filter(Boolean);
    const glyph = images.length
      ? images.map(src => `<img src="${esc(src)}" alt="" aria-hidden="true" draggable="false">`).join("")
      : esc(def.label);
    return `<span class="stat-glyph" data-stat-label="icon" role="img" aria-label="${esc(def.accessibleLabel)}">${glyph}</span>`;
  };
  const slug = id => id.replace("npc_dota_hero_", "");
  const portSrc = id => icon("hero", slug(id));
  let W = 600, H = 600, dpr = 1;

  function nameOf(n){ return (S.D && S.D.names[n]) || prettyMod(n); }
  function prettyMod(n, srcHero){
    let s = n.replace(/^modifier_/, "").replace(/^item_/, "");
    if (srcHero) s = s.replace(slug(srcHero) + "_", "");
    return s.replace(/_/g, " ");
  }
  function shortOf(name){
    const w = name.split(/\s+/).filter(x => !/^(of|the)$/i.test(x));
    return w.length > 1 ? w.map(x => x[0]).join("").toUpperCase().slice(0, 4) : name.slice(0, 3).toUpperCase();
  }
  const idx = t => Math.max(0, Math.min(S.D.heroes[0].hp.length - 1, Math.floor(t) - S.D.t0));
  function posAt(h, t){
    if (S.W){ const w = wHeroPos(h.id, t); if (w) return w; }
    const i = idx(t), j = Math.min(i + 1, h.pos.length - 1), f = t - Math.floor(t), a = h.pos[i], b = h.pos[j];
    if (!b || Math.hypot(a[0] - b[0], a[1] - b[1]) > 1800) return a;
    return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
  }
  // world (GSI units) -> base canvas px -> screen px with zoom
  const base = p => [(mapCalibration.a * p[0] + mapCalibration.b) / mapCalibration.size * W,
    (mapCalibration.c * p[1] + mapCalibration.d) / mapCalibration.size * H];
  const scr = p => { const [x, y] = base(p), v = S.view; return [(x - v.ox) * v.k, (y - v.oy) * v.k]; };
  function clampView(){ const v = S.view; v.k = Math.max(1, Math.min(6, v.k)); v.ox = Math.max(0, Math.min(W - W / v.k, v.ox)); v.oy = Math.max(0, Math.min(H - H / v.k, v.oy)); $("z-lvl").textContent = v.k.toFixed(1) + "×"; }
  function zoomAt(sx, sy, k){ const v = S.view, bx = sx / v.k + v.ox, by = sy / v.k + v.oy; v.k = Math.max(1, Math.min(6, k)); v.ox = bx - sx / v.k; v.oy = by - sy / v.k; clampView(); draw(); }

  /* ---------- alerts ---------- */
  function buildAlerts(){
    const D = S.D, cfg = {
      hp: $("al-hp").checked, pct: +$("al-hp-pct").value / 100, win: +$("al-hp-win").value,
      low: $("al-low").checked, lowPct: +$("al-low-pct").value / 100, cc: $("al-cc").checked, ccS: +$("al-cc-s").value,
      ult: $("al-ult").checked, death: $("al-death").checked};
    const out = [];
    for (const h of D.heroes){
      if (cfg.hp){
        let cur = null;
        for (let i = 1; i < h.hp.length; i++){
          let best = 0, from = 0;
          if (h.alive[i]) for (let k = 1; k <= cfg.win && i - k >= 0; k++){
            if (!h.alive[i - k]) break;
            const dr = (h.hp[i - k] - h.hp[i]) / (h.mhp[i] || 1);
            if (dr > best){ best = dr; from = i - k; }
          }
          if (best >= cfg.pct){
            const t = i + D.t0;
            if (cur && from + D.t0 <= cur.t1 + 1){ cur.t1 = t; cur.lo = Math.min(cur.lo, h.hp[i] / h.mhp[i]); cur.drop = Math.max(cur.drop, best); }
            else { cur = {hero: h.id, type: "hp", t0: from + D.t0, ts: t, t1: t, hi: h.hp[from] / h.mhp[from], lo: h.hp[i] / h.mhp[i], drop: best}; out.push(cur); }
          }
        }
        for (const a of out) if (a.type === "hp" && a.hero === h.id) a.text = `HP ร่วง ${Math.round(a.hi * 100)}% → ${Math.round(a.lo * 100)}% (−${Math.round((a.hi - a.lo) * 100)}%) ใน ${Math.max(1, Math.round(a.t1 - a.t0))} วินาที`;
      }
      if (cfg.low){
        let inLow = false;
        for (let i = 0; i < h.hp.length; i++){
          const f = h.hp[i] / (h.mhp[i] || 1), low = h.alive[i] && f < cfg.lowPct;
          if (low && !inLow) out.push({hero: h.id, type: "low", t0: i + D.t0, t1: i + D.t0 + 2, text: `HP เหลือ ${Math.round(f * 100)}%`});
          else if (low) out[out.length - 1].t1 = i + D.t0;
          inLow = low;
        }
      }
      if (cfg.cc){
        for (const [a, b, n, src] of h.mods){
          const sh = D.byId[src];
          if (!sh || sh.team === h.team || b - a < cfg.ccS || !isControlEffect(patchData, n)) continue;
          out.push({hero: h.id, type: "cc", t0: a, t1: b, text: `โดน ${prettyMod(n, src)} ${(b - a).toFixed(1)} วินาที จาก ${sh.name}`});
        }
      }
      if (cfg.ult){
        const lastU = {};  // a repeated cast of the same ultimate within 20 s (e.g. Largo's song notes) stays one event
        for (const [t, n] of h.casts){
          if (!isUltimateAbility(patchData, h.id, n) && !isNotableItem(patchData, n)) continue;
          const prev = lastU[n]; lastU[n] = t;
          if (prev != null && t - prev < 20) continue;
          out.push({hero: h.id, type: "ult", t0: t, t1: t + 1.5, text: `ใช้ ${nameOf(n)}`});
        }
      }
    }
    if (cfg.death) for (const d of D.deaths) out.push({hero: d.hero, type: "death", t0: d.t, t1: d.t + 2, text: `ตาย · ${nameOf(d.killer)}`, d});
    out.sort((x, y) => x.t0 - y.t0);
    S.alerts = out;
  }
  const TYPE = {hp: "HP ลดเร็ว", low: "HP ต่ำ", cc: "โดน CC", ult: "Ultimate", death: "ตาย"};
  function activeAlert(h, t){ return S.alerts.find(a => a.hero === h.id && t >= (a.ts != null ? a.ts - 1 : a.t0) && t <= a.t1 + 2 && a.type !== "ult") || S.alerts.find(a => a.hero === h.id && a.type === "ult" && t >= a.t0 && t <= a.t1); }

  /* ---------- drawing ---------- */
  function resize(){
    W = wrap.clientWidth; H = W; dpr = window.devicePixelRatio || 1;
    cv.width = Math.round(W * dpr); cv.height = Math.round(H * dpr); ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    S.heat = null; clampView(); draw(); renderMulti();
  }
  function visHeroes(){ return S.D.heroes.filter(h => S.vis.has(h.id)); }
  function drawMap(c, w, h, view){
    c.fillStyle = "#0b0f0d"; c.fillRect(0, 0, w, h);
    if (S.img){ const v = view || {k: 1, ox: 0, oy: 0}; c.drawImage(S.img, -v.ox * v.k, -v.oy * v.k, w * v.k, h * v.k); }
  }
  function draw(){
    if (!S.D) return;
    if (S.follow && S.focus){
      const h = S.D.byId[S.focus], [bx, by] = base(posAt(h, S.t)), v = S.view;
      if (v.k < 2) v.k = 2.5;
      v.ox = bx - W / (2 * v.k); v.oy = by - H / (2 * v.k); clampView();
    }
    drawMap(ctx, W, H, S.view);
    S.hits = [];
    if (S.mode === "heat") drawHeat(); else drawRoute();
  }
  function label(x, y, text, color, align){
    ctx.font = `600 ${Math.max(10, Math.min(13, W * .017))}px ${css("--f-mono")}`; ctx.textAlign = align || "left"; ctx.lineJoin = "round";
    ctx.lineWidth = 3; ctx.strokeStyle = "rgba(0,0,0,.85)"; ctx.strokeText(translate(text), x, y); ctx.fillStyle = color || "#fff"; ctx.fillText(translate(text), x, y);
  }
  function drawIcon(h, x, y, r, alive, alert){
    const im = S.port[h.id], now = performance.now() / 1000;
    if (alert && alive){
      const pulse = .5 + .5 * Math.sin(now * 7);
      const g = ctx.createRadialGradient(x, y, r * .8, x, y, r * (2.4 + pulse * .5));
      g.addColorStop(0, "rgba(255,40,40,.85)"); g.addColorStop(1, "rgba(255,40,40,0)");
      ctx.fillStyle = g; ctx.beginPath(); ctx.arc(x, y, r * (2.4 + pulse * .5), 0, 7); ctx.fill();
      ctx.strokeStyle = `rgba(255,70,70,${.6 + .4 * pulse})`; ctx.lineWidth = 2.5; ctx.beginPath(); ctx.arc(x, y, r + 7 + pulse * 3, 0, 7); ctx.stroke();
    }
    ctx.save(); ctx.beginPath(); ctx.arc(x, y, r, 0, 7); ctx.closePath(); ctx.fillStyle = "#000"; ctx.fill(); ctx.clip();
    if (!alive) ctx.filter = "grayscale(1) brightness(.6)";
    if (im && im.complete && im.naturalWidth) ctx.drawImage(im, 56, 0, 144, 144, x - r, y - r, 2 * r, 2 * r);
    else { ctx.fillStyle = h.color; ctx.fillRect(x - r, y - r, 2 * r, 2 * r); }
    ctx.restore(); ctx.filter = "none";
    ctx.lineWidth = 3; ctx.strokeStyle = h.color; ctx.beginPath(); ctx.arc(x, y, r, 0, 7); ctx.stroke();
    if (S.focus === h.id){ ctx.lineWidth = 2; ctx.strokeStyle = "#fff"; ctx.beginPath(); ctx.arc(x, y, r + 3.5, 0, 7); ctx.stroke(); }
  }
  // floating damage numbers: hits in the last 1.2 s, grouped per 0.3 s and damage type, rising and fading
  const DMG_COL = ["#ff6b5e", "#8fb6ff", "#f4ecd8"];
  function drawDmgNums(h, x, y, r, t){
    const T = h.DT; if (!T || !S.dmgFloat) return;
    const LIFE = 1.2, groups = [];
    for (let k = firstAtOrAfter(T, t - LIFE); k < T.length && T[k] <= t; k++){
      const g = groups.find(q => q.ty === h.DY[k] && T[k] - q.t0 < .3);
      if (g){ g.v += h.DV[k]; g.t1 = T[k]; } else groups.push({t0: T[k], t1: T[k], ty: h.DY[k], v: h.DV[k]});
    }
    const shown = groups.filter(g => g.v >= 5).slice(-4);
    ctx.textAlign = "center"; ctx.lineJoin = "round";
    shown.forEach((g, n) => {
      const age = t - g.t0, a = Math.max(0, 1 - age / LIFE), big = g.v >= (h.mhp[idx(t)] || 1000) * .08;
      const yy = y - r - 10 - age * 34 - n * 2, xx = x + (n % 2 ? 9 : -9) * (shown.length > 1 ? 1 : 0);
      ctx.font = `800 ${big ? 15 : 12}px ${css("--f-mono")}`; ctx.globalAlpha = a;
      ctx.lineWidth = 3.5; ctx.strokeStyle = "rgba(0,0,0,.9)"; ctx.strokeText("−" + Math.round(g.v), xx, yy);
      ctx.fillStyle = DMG_COL[g.ty] || "#fff"; ctx.fillText("−" + Math.round(g.v), xx, yy);
    });
    ctx.globalAlpha = 1;
  }
  function drawRoute(){
    const t = S.t, r = Math.max(11, Math.min(17, W * .021));
    drawWorld(t);
    for (const h of visHeroes()){
      if (fogBit() && h.team !== fogTeam()) continue;
      if (S.W && S.W.heroes[h.id]){
        ctx.strokeStyle = h.color; ctx.lineCap = "round"; ctx.lineJoin = "round"; ctx.lineWidth = 3;
        const K = S.W.heroes[h.id], ts = Math.max(S.D.t0, t - S.trail), j0 = keyIndex(K.T, ts), j1 = keyIndex(K.T, t);
        let prev = wHeroPos(h.id, ts), pt = ts;
        for (let j = j0 + 1; j <= j1 + 1; j++){
          const last = j > j1, tt = last ? t : K.T[j], p = last ? wHeroPos(h.id, t) : [K.X[j], K.Y[j]];
          if (!last && K.B[j]){ prev = p; pt = tt; continue; }
          if (h.alive[idx(tt)] && h.alive[idx(pt)]){
            const a = scr(prev), b = scr(p); ctx.globalAlpha = .2 + .8 * (tt - ts) / Math.max(1, t - ts);
            ctx.beginPath(); ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.stroke();
          }
          prev = p; pt = tt;
        }
        ctx.globalAlpha = 1; continue;
      }
      const i1 = idx(t), i0 = Math.max(0, i1 - S.trail);
      ctx.strokeStyle = h.color; ctx.lineCap = "round"; ctx.lineJoin = "round"; let prev = null;
      for (let i = i0; i <= i1; i++){
        const p = i === i1 ? posAt(h, t) : h.pos[i];
        if (!h.alive[i] || !prev || Math.hypot(p[0] - prev[0], p[1] - prev[1]) > 1800){ prev = h.alive[i] ? p : null; continue; }
        const a = scr(prev), b = scr(p);
        ctx.globalAlpha = .2 + .8 * (i - i0) / Math.max(1, i1 - i0); ctx.lineWidth = 3;
        ctx.beginPath(); ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.stroke(); prev = p;
      }
      ctx.globalAlpha = 1;
    }
    // draw dead first, then alive, focused last
    const list = visHeroes().map(h => ({h, i: idx(t)})).sort((a, b) => a.h.alive[a.i] - b.h.alive[b.i] || (a.h.id === S.focus) - (b.h.id === S.focus));
    for (const {h, i} of list){
      if (!heroSeen(h, t)) continue;
      const alive = h.alive[i], [x, y] = scr(posAt(h, t)), al = activeAlert(h, t);
      drawIcon(h, x, y, alive ? r : r * .75, alive, al);
      if (alive){
        const f = h.mhp[i] ? hpNowAt(h, t) / h.mhp[i] : 0;
        ctx.lineWidth = 3; ctx.strokeStyle = "rgba(0,0,0,.7)"; ctx.beginPath(); ctx.arc(x, y, r + 1.5, Math.PI * .75, Math.PI * 2.25); ctx.stroke();
        ctx.strokeStyle = f < .25 ? "#ff4d4d" : f < .5 ? "#ffcc33" : "#5fe08a"; ctx.beginPath(); ctx.arc(x, y, r + 1.5, Math.PI * .75, Math.PI * (.75 + 1.5 * f)); ctx.stroke();
        const c = h.casts.filter(c => c[0] <= t && t - c[0] < 1.8);
        if (c.length) label(x, y - r - 8, nameOf(c[c.length - 1][1]), "#ffe08a", "center");
        if (S.names || S.focus === h.id) label(x, y + r + 14, h.short, "#fff", "center");
      } else {
        const rs = respawnIn(h, t); label(x, y + r + 4, rs != null ? Math.ceil(rs) + "s" : "", "#d0d0d0", "center");
      }
      if (al && alive && al.type !== "ult") label(x, y + r + (S.names || S.focus === h.id ? 28 : 16), al.text, "#ff8080", "center");
      drawDmgNums(h, x, y, r, t);
      S.hits.push({x, y, r, h});
    }
  }
  function heatGrid(heroes, N){
    const D = S.D, grids = heroes.map(() => new Float32Array(N * N));
    heroes.forEach((h, k) => {
      const g = grids[k];
      for (let i = 0; i < h.pos.length; i++){
        const t = i + D.t0; if (!h.alive[i] || t < S.range[0] || t >= S.range[1]) continue;
        const p = h.pos[i], gx = Math.floor((mapCalibration.a * p[0] + mapCalibration.b) / mapCalibration.size * N),
          gy = Math.floor((mapCalibration.c * p[1] + mapCalibration.d) / mapCalibration.size * N);
        if (gx >= 0 && gy >= 0 && gx < N && gy < N) g[gy * N + gx] += 1;
      }
      // two box-blur passes ≈ gaussian
      for (let pass = 0; pass < 2; pass++){
        const tmp = new Float32Array(N * N), R = 2;
        for (let y = 0; y < N; y++) for (let x = 0; x < N; x++){ let s = 0; for (let d = -R; d <= R; d++){ const xx = x + d; if (xx >= 0 && xx < N) s += g[y * N + xx]; } tmp[y * N + x] = s; }
        for (let y = 0; y < N; y++) for (let x = 0; x < N; x++){ let s = 0; for (let d = -R; d <= R; d++){ const yy = y + d; if (yy >= 0 && yy < N) s += tmp[yy * N + x]; } g[y * N + x] = s; }
      }
    });
    return grids;
  }
  const rgb = hex => [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16));
  function heatCanvas(heroes, style, N){
    const grids = heatGrid(heroes, N), off = document.createElement("canvas"); off.width = off.height = N;
    const o = off.getContext("2d"), img = o.createImageData(N, N), a = img.data, cols = heroes.map(h => rgb(h.color));
    let mx = 0; const tot = new Float32Array(N * N);
    for (let j = 0; j < N * N; j++){ let s = 0; for (const g of grids) s += g[j]; tot[j] = s; if (s > mx) mx = s; }
    for (let j = 0; j < N * N; j++){
      const v = tot[j] / (mx || 1); if (v < .02) continue;
      const s = Math.pow(v, .5);
      if (style === "sum" || heroes.length === 1 && style !== "hero1"){
        a[j * 4] = 255; a[j * 4 + 1] = Math.round(120 + 115 * s); a[j * 4 + 2] = Math.round(30 + 150 * s * s);
      } else {
        let bi = 0, bv = -1; grids.forEach((g, k) => { if (g[j] > bv){ bv = g[j]; bi = k; } });
        const c = cols[bi]; a[j * 4] = c[0]; a[j * 4 + 1] = c[1]; a[j * 4 + 2] = c[2];
      }
      a[j * 4 + 3] = Math.round(70 + 175 * s);
    }
    o.putImageData(img, 0, 0);
    return {off, grids, tot, mx, N};
  }
  function drawHeat(){
    const hs = visHeroes(), key = hs.map(h => h.id).join() + S.range + S.heatStyle;
    if (!S.heat || S.heat.key !== key){ S.heat = heatCanvas(hs, S.heatStyle, 160); S.heat.key = key; S.heat.heroes = hs; }
    const v = S.view; ctx.imageSmoothingEnabled = true;
    ctx.drawImage(S.heat.off, -v.ox * v.k, -v.oy * v.k, W * v.k, H * v.k);
    const r = Math.max(5, W * .009);
    for (const d of S.D.deaths){
      if (!S.vis.has(d.hero) || d.t < S.range[0] || d.t >= S.range[1]) continue;
      const h = S.D.byId[d.hero], [x, y] = scr(h.pos[idx(d.t)]);
      ctx.beginPath(); ctx.arc(x, y, r, 0, 7); ctx.fillStyle = h.color; ctx.fill(); ctx.lineWidth = 2.5; ctx.strokeStyle = "#ff3b3b"; ctx.stroke();
      ctx.beginPath(); ctx.arc(x, y, r + 2.5, 0, 7); ctx.lineWidth = 1; ctx.strokeStyle = "#fff"; ctx.stroke();
      S.hits.push({x, y, r: r + 4, d});
    }
  }
  function heatTip(sx, sy){
    const H0 = S.heat; if (!H0) return null;
    const v = S.view, bx = sx / v.k + v.ox, by = sy / v.k + v.oy, gx = Math.floor(bx / W * H0.N), gy = Math.floor(by / H * H0.N);
    if (gx < 0 || gy < 0 || gx >= H0.N || gy >= H0.N) return null;
    const j = gy * H0.N + gx, tot = H0.tot[j]; if (tot / (H0.mx || 1) < .02) return null;
    const share = H0.heroes.map((h, k) => [h, H0.grids[k][j] / tot]).filter(x => x[1] > .04).sort((a, b) => b[1] - a[1]).slice(0, 4);
    return `<b>ใครอยู่ตรงนี้</b><br>` + share.map(([h, f]) => `<span class="swatch" style="background:${h.color}"></span>${esc(h.name)} ${Math.round(f * 100)}%`).join("<br>");
  }
  function renderMulti(){
    const box = $("rv-multi"); box.hidden = S.mode !== "heat"; if (box.hidden || !S.D) return;
    box.innerHTML = "";
    for (const h of visHeroes()){
      const b = document.createElement("button"); b.className = "rv-mini"; b.style.setProperty("--c", h.color); b.title = "ดูเฉพาะ " + h.name;
      const c = document.createElement("canvas"); c.width = c.height = 220; const o = c.getContext("2d");
      drawMap(o, 220, 220); o.fillStyle = "rgba(0,0,0,.35)"; o.fillRect(0, 0, 220, 220);
      const hc = heatCanvas([h], "hero1", 110); o.drawImage(hc.off, 0, 0, 220, 220);
      b.appendChild(c);
      const sp = document.createElement("span"); sp.innerHTML = `<img src="${portSrc(h.id)}" width="18" height="18" alt="">${esc(h.name)}`; b.appendChild(sp);
      b.addEventListener("click", () => { S.vis = new Set([h.id]); changedVis(); });
      box.appendChild(b);
    }
  }
  function respawnIn(h, t){
    const i = idx(t); if (h.alive[i]) return null;
    for (let j = i + 1; j < h.alive.length; j++) if (h.alive[j]) return j + S.D.t0 - t;
    return null;
  }

  /* ---------- cards ---------- */
  function invAt(h, t){ let cur = h.inv[0] ? h.inv[0][1] : []; for (const [tt, v] of h.inv){ if (tt <= t) cur = v; else break; } return cur; }
  function modsAt(h, t){
    const out = [];
    for (const [a, b, n, src] of h.mods){
      if (a > t) break; if (b < t) continue;
      const sh = S.D.byId[src], enemy = sh && sh.team !== h.team, cc = enemy && isControlEffect(patchData, n);
      out.push({n: prettyMod(n, src), rem: b - t, cls: cc ? "cc" : enemy ? "deb" : ""});
    }
    out.sort((x, y) => (y.cls === "cc") - (x.cls === "cc") || (y.cls === "deb") - (x.cls === "deb"));
    const seen = new Set(); return out.filter(m => !seen.has(m.n) && seen.add(m.n)).slice(0, 6);
  }
  const abilSrc = n => icon("ability", n);
  function iconSlot(kind, name, o){
    o = o || {};
    if (!name) return `<span class="ic ${kind} empty"></span>`;
    const cd = o.cd > 0 ? o.cd : 0, p = cd && o.cl ? Math.min(100, cd / o.cl * 100) : (cd ? 100 : 0);
    const title = esc(name) + (o.extra ? " · " + esc(o.extra) : "") + (cd ? ` · cooldown ${cd.toFixed(1)} วินาที` : "");
    return `<span class="ic ${kind}${o.cls ? " " + o.cls : ""}${cd ? " oncd" : ""}" style="--p:${p}%" title="${title}"${o.attrs || ""}><b>${esc(name.slice(0, 3))}</b>${o.src ? `<img src="${o.src}" alt="">` : ""}${o.pips != null ? `<i class="pp">${"●".repeat(o.pips)}</i>` : ""}${o.ch ? `<em>${o.ch}</em>` : ""}${cd ? `<s>${cd < 10 ? cd.toFixed(1) : Math.ceil(cd)}</s>` : ""}</span>`;
  }
  function findAt(h, t){ let f = null; for (const x of h.finds || []) if (x[0] <= t) f = x; return f; }
  function cardHTML(h, t){
    const i = idx(t), alive = h.alive?.[i], hp = h.hp?.[i], mhp = h.mhp?.[i], mp = h.mp?.[i], mmp = h.mmp?.[i], frac = t - Math.floor(t), rs = respawnIn(h, t);
    const kills = S.D.deaths.filter(d => d.killer === h.id && d.t <= t).length, deaths = S.D.deaths.filter(d => d.hero === h.id && d.t <= t).length;
    const [gold, nw, lh, dn] = h.eco?.[i] || [], al = activeAlert(h, t);
    const statValue = key => Number.isFinite(h.st?.[key]?.[i]) ? h.st[key][i] : null;
    const derivedView = derived(h, t);
    const sharedView = heroCardView({
      top: { name: h.name ?? "—", kills, deaths, lh, dn, nw }, bars: { hp, mhp, mp, mmp },
      ab: { abilities: h.abil?.length ?? null }, it: { items: h.inv ? "known" : null, gold },
      st: { str: statValue("strT"), agi: statValue("agiT"), int: statValue("intT"), dmg: statValue("dmin"), reg: statValue("hpreg") },
      dv: Object.fromEntries(["armor", "mr", "amp", "as", "ms", "ev", "sres", "ls"].map(key => [key, derivedView?.[key] ?? null])),
    });
    const rowAttr = id => {
      const row = sharedView.rows.find(candidate => candidate.id === id);
      return `data-card-row="${id}"${row?.unknownFields.length ? ` data-unknown-fields="${esc(row.unknownFields.join(","))}"` : ""}`;
    };
    const abil = h.abil.map((a, k) => {
      const lv = a.lv[i], cd = a.cd[i] ? Math.max(0, a.cd[i] - frac) : 0;
      return iconSlot("ab", nameOf(a.n), {attrs: ` data-ab="${k}"`, src: abilSrc(a.n), cd, cl: a.cl[i], pips: Math.max(...a.lv) > 1 ? lv : null, extra: "Lv " + lv + (a.mc[i] ? " · มานา " + a.mc[i] : ""), cls: lv === 0 ? "off" : (!cd && a.mc[i] > mp ? "nomana" : "")});
    }).join("");
    const bySlot = {}; for (const [s, n, c] of invAt(h, t)) bySlot[s] = [n, c];
    const icd = s => { const v = h.icd[s] && h.icd[s][Math.floor(t)]; return v ? [Math.max(0, v[0] - frac), v[1]] : [0, 0]; };
    const item = (s, kind) => { const v = bySlot[s]; if (!v) return iconSlot(kind, null); const [cd, cl] = icd(s); return iconSlot(kind, nameOf(v[0]), {src: iconSrc(v[0]), cd, cl, ch: v[1] || ""}); };
    const f = findAt(h, t), nv = bySlot[16];
    const neutral = nv ? (() => { const [cd, cl] = icd(16); return iconSlot("nt", nameOf(nv[0]), {src: iconSrc(nv[0]), cd, cl, extra: "ไอเทมป่า" + (f ? ` · enchantment ${f[2]} ระดับ ${f[3]} · ดรอปล่าสุด ${clk(f[0])}` : "")}); })() : iconSlot("nt", null);
    const mods = modsAt(h, t).slice(0, 3);
    return `<div class="pt"><img src="${portSrc(h.id)}" alt=""><span class="lvb">${fmt(h.lvl?.[i])}</span>${alive === false ? `<span class="rsp">${rs != null ? Math.ceil(rs) : "—"}</span>` : ""}</div>
      <div class="top" ${rowAttr("top")}><span class="nm">${esc(h.name ?? "—")}</span><span class="kd num" title="Kill / Death">${kills}/${deaths}</span><span class="num" title="Last hit / Deny">${fmt(lh)}/${fmt(dn)}</span><span class="num nw" title="Net worth">${Number.isFinite(nw) ? (nw >= 10000 ? (nw / 1000).toFixed(1) + "k" : fmt(nw)) : "—"}</span></div>
      ${barsHTML(h, t, i, alive, hp, mhp, mp, mmp, rowAttr("bars"))}
      <div class="row ab" ${rowAttr("ab")}>${abil}<span class="mods">${al ? `<span class="mod cc">⚠ ${esc(al.text)}</span>` : mods.map(m => `<span class="mod ${m.cls}">${esc(m.n)} <span class="num">${m.rem < 10 ? m.rem.toFixed(1) : Math.round(m.rem)}</span></span>`).join("")}</span></div>
      <div class="row it" ${rowAttr("it")}>${[0, 1, 2, 3, 4, 5].map(s => item(s, "it")).join("")}<span class="bp">${[6, 7, 8].map(s => item(s, "bk")).join("")}</span>${item(15, "tp")}${neutral}${f && nv ? `<span class="enh" title="Enchantment ของไอเทมป่า">${esc(f[2])} ${f[3]}</span>` : ""}<span class="gold num" title="ทองในมือ">${fmt(gold)}</span></div>
      ${statsHTML(h, i, rowAttr("st"))}
      ${derivedHTML(h, i, t, rowAttr("dv"))}`;
  }
  /* Patch-scoped dotaconstants tables plus observed replay attributes, items and modifiers. */
  function abilLv(h, name, i){ const a = h && h.abil.find(x => x.n === name); return a ? Math.max(1, a.lv[i]) : 1; }
  function derived(h, t){
    const db = S.D.statdb; if (!db || !h.st) return null;
    const hb = db.heroes[h.id]; if (!hb) return null;
    const i = idx(t), s = h.st, melee = hb.melee, src = {};
    const mrPerIntPct = patchData.rules?.mr_per_int_pct
      ?? patchData.formulas?.magic_resistance_per_intelligence_pct?.value;
    const armorPerAgi = patchData.rules?.armor_per_agi
      ?? patchData.formulas?.armor_per_agility?.value;
    if (!Number.isFinite(mrPerIntPct) || !Number.isFinite(armorPerAgi)) return null;
    const add = (k, label, v) => { if (!v) return; (src[k] = src[k] || []).push([label, v]); };
    const agi = s.agiT[i], int = s.intT[i];
    // Armor, resistance and attack-speed formulas come from this patch's local replay pack.
    add("armor", "พื้นฐาน", hb.armor); add("armor", `AGI ${Math.round(agi)} × ${armorPerAgi}`, agi * armorPerAgi);
    add("mr", "พื้นฐาน", hb.mr); add("mrint", `INT ${Math.round(int)} × ${mrPerIntPct}%`, int * mrPerIntPct);
    const bas = baseAttackSpeed(patchData, h.id) ?? hb.bas;
    add("as", `พื้นฐานของฮีโร่ (${bas})`, bas); add("as", `AGI ${Math.round(agi)}`, agi);
    add("ms", "พื้นฐาน", hb.ms);
    let boots = 0, bootsName = "";
    for (const [slot, n] of invAt(h, t)){
      if (slot > 5 && slot !== 16) continue;
      const e = db.items[n]; if (!e) continue; const nm = nameOf(n);
      add("armor", nm, e.armor); add("mrm", nm, e.mr); add("amp", nm, e.amp); add("ls", nm, e.ls); add("sls", nm, e.sls);
      add("evm", nm, e.ev); add("as", nm, e.as); add("ms", nm, e.msflat); add("mspct", nm, e.mspct);
      const b = melee ? e.boots : (e.boots_r || e.boots); if (b && b > boots){ boots = b; bootsName = nm; }
    }
    if (boots) add("ms", bootsName, boots);
    const f = findAt(h, t), hasNeutral = invAt(h, t).some(x => x[0] === 16);
    if (f && hasNeutral && db.enh[f[2]]){
      const e = db.enh[f[2]], L = k => e[k] ? e[k][Math.min(f[3], e[k].length) - 1] : 0, lab = `${f[2]} ${f[3]}`;
      add("armor", lab, L("armor")); add("mrm", lab, L("mr")); add("evm", lab, L("ev")); add("as", lab, L("as")); add("ms", lab, L("msflat")); add("amp", lab, L("amp")); add("srm", lab, L("sres"));
    }
    let haste = false;
    for (const [a, b, n, from] of h.mods){
      if (a > t) break; if (b < t) continue;
      const bf = db.buffs[n]; if (!bf) continue;
      const lv = bf.abil ? abilLv(S.D.byId[from], bf.abil, i) : 1, V = k => bf[k] ? bf[k][Math.min(lv, bf[k].length) - 1] : 0;
      add("armor", bf.label, V("armor")); add("mrm", bf.label, V("mr")); add("evm", bf.label, V("ev")); add("as", bf.label, V("as"));
      add("mspct", bf.label, melee ? V("mspct") : (bf.mspct_r ? V("mspct_r") : V("mspct"))); add("srm", bf.label, V("sres"));
      if (bf.haste) haste = true;
    }
    const sum = k => (src[k] || []).reduce((x, y) => x + y[1], 0);
    const mult = k => 1 - (src[k] || []).reduce((x, y) => x * (1 - y[1] / 100), 1);
    const armor = sum("armor"), pr = .06 * armor / (1 + .06 * Math.abs(armor));
    // base + INT magic resistance add together, then every other source multiplies
    const mrBase = (hb.mr + sum("mrint")) / 100, mr = 1 - (1 - mrBase) * (1 - mult("mrm"));
    // Liquipedia Attack Speed: min = 20 shifted by (base AS − 100), max 700
    const as = Math.max(20 + bas - 100, Math.min(700, sum("as"))), aps = as / 100 / (s.bat[i] || 1.7);
    let ms = sum("ms") * (1 + sum("mspct") / 100); ms = haste ? 550 : Math.max(100, Math.min(550, ms));
    return {src, armor, pr, mr, amp: sum("amp"), ls: sum("ls"), sls: sum("sls"), ev: mult("evm"), sres: mult("srm"), as, aps, ms, haste};
  }
  function derivedHTML(h, i, t, rowAttributes = ""){
    const d = derived(h, t);
    const tip = (title, keys, extra) => esc(title + "\n" + keys.flatMap(k => (d.src[k] || []).map(([l, v]) => `• ${l}: ${v > 0 && k !== "mspct" ? "+" : ""}${Math.round(v * 10) / 10}${/^(mr|mrm|mrint|amp|ls|sls|evm|srm|mspct)$/.test(k) ? "%" : ""}`)).join("\n") + (extra ? "\n" + extra : ""));
    const pct = v => (Math.round(v * 1000) / 10) + "%";
    const cell = (key, val, title, cls) => {
      const label = esc(statDefinition(key).accessibleLabel);
      return `<span class="dv${cls ? " " + cls : ""}" title="${label}\n${title}">${statGlyphs(key)} <b>${val}</b></span>`;
    };
    if (!d) return `<div class="row dvs" ${rowAttributes}>${["arm", "mr", "amp", "as", "ms", "ev", "sr", "ls"].map(key => `<span class="dv" title="${esc(statDefinition(key).accessibleLabel)}: ยังไม่มีข้อมูล">${statGlyphs(key)} <b>—</b></span>`).join("")}</div>`;
    return `<div class="row dvs" ${rowAttributes}>
      ${cell("arm", d.armor.toFixed(1), tip(`เกราะ ≈ ${d.armor.toFixed(1)} (ลดดาเมจกายภาพ ${pct(d.pr)})`, ["armor"], "สูตร: 0.06×เกราะ ÷ (1 + 0.06×|เกราะ|)"))}
      ${cell("mr", pct(d.mr), tip(`Magic resistance ≈ ${pct(d.mr)}`, ["mr", "mrint", "mrm"], `Patch ${patchData.patch}: base resistance plus ${patchData.rules?.mr_per_int_pct ?? patchData.formulas?.magic_resistance_per_intelligence_pct?.value}% per INT, then other sources multiply.`))}
      ${cell("amp", d.amp + "%", tip(`Spell amp ≈ ${d.amp}%`, ["amp"], "แหล่งต่าง ๆ บวกกัน"), d.amp ? "" : "z")}
      ${cell("as", Math.round(d.as), tip(`Attack speed ≈ ${Math.round(d.as)} (${d.aps.toFixed(2)} ครั้ง/วินาที, BAT ${h.st.bat[i]})`, ["as"], `Patch ${patchData.patch}: hero base attack speed + AGI + bonuses, bounded by ${20 + (baseAttackSpeed(patchData, h.id) ?? S.D.statdb.heroes[h.id].bas) - 100}–700.`))}
      ${cell("ms", Math.round(d.ms), tip(`ความเร็ววิ่ง ≈ ${Math.round(d.ms)}${d.haste ? " (haste = ความเร็วสูงสุด)" : ""}`, ["ms", "mspct"], "สูตร: (พื้นฐาน + รองเท้าคู่ที่ดีที่สุด + ค่าคงที่อื่น) × (1 + % รวม) จำกัด 100–550"))}
      ${cell("ev", pct(d.ev), tip(`Evasion ≈ ${pct(d.ev)}`, ["evm"], "สูตร: 1 − Π(1 − แหล่ง)"), d.ev ? "" : "z")}
      ${cell("sr", pct(d.sres), tip(`Status resistance ≈ ${pct(d.sres)}`, ["srm"], "สูตร (Liquipedia): 1 − Π(1 − แหล่ง) · Sange หลายชิ้นเอาค่าสูงสุดค่าเดียว"), d.sres ? "" : "z")}
      ${cell("ls", (d.ls || 0) + "%", tip(`Lifesteal ≈ ${d.ls}% · Spell lifesteal ≈ ${d.sls}%`, ["ls", "sls"]), d.ls || d.sls ? "" : "z")}
    </div>`;
  }
  function statsHTML(h, i, rowAttributes = ""){
    const s = h.st;
    const at = (key, p) => { const def = statDefinition(key), tot = Number.isFinite(s?.[key + "T"]?.[i]) ? s[key + "T"][i] : null, b = Number.isFinite(s?.[key]?.[i]) ? s[key][i] : null, bonus = tot !== null && b !== null ? Math.round(tot - b) : null; return `<span class="at${h.prim === p ? " pr" : ""}" title="${esc(def.accessibleLabel)}: ${tot === null ? "ยังไม่มีข้อมูล" : `พื้นฐาน ${b} · โบนัส +${bonus}`}">${statGlyphs(key)} <b>${tot === null ? "—" : Math.round(tot)}</b>${bonus !== null && bonus > 0 ? `<em>+${bonus}</em>` : ""}</span>`; };
    const cell = (key, val, detail) => `<span class="at" title="${esc(statDefinition(key).accessibleLabel)}: ${esc(detail)}">${statGlyphs(key)} <b>${val}</b></span>`;
    const known = (key, fallback = "—") => Number.isFinite(s?.[key]?.[i]) ? s[key][i] : fallback;
    const dmin = known("dmin", null), dmax = known("dmax", null), dbonus = known("dbonus", null), range = known("range"), bat = known("bat");
    const hpreg = known("hpreg"), mpreg = known("mpreg");
    return `<div class="row st" ${rowAttributes}>${at("str", 0)}${at("agi", 1)}${at("int", 2)}
      ${cell("dmg", dmin === null || dmax === null ? "—" : `${dmin}–${dmax}${dbonus ? `<em>+${dbonus}</em>` : ""}`, `ดาเมจโจมตีพื้นฐาน ${dmin === null || dmax === null ? "—" : `${dmin}–${dmax}`} · โบนัสจากไอเทมและบัฟ +${dbonus ?? "—"} · ระยะตี ${range} · BAT ${bat}`)}
      <span class="at" title="${esc(statDefinition("reg").accessibleLabel)}: HP regen ${hpreg}/วินาที · Mana regen ${mpreg}/วินาที">${statGlyphs("reg")} <b>${hpreg}</b>/<b class="m">${mpreg}</b></span></div>`;
  }
  function firstAtOrAfter(T, a){ let lo = 0, hi = T.length; while (lo < hi){ const m = (lo + hi) >> 1; if (T[m] < a - 1e-3) lo = m + 1; else hi = m; } return lo; }
  function healIn(h, a, b){ const T = h.HT; if (!T) return 0; let s = 0; for (let k = firstAtOrAfter(T, a); k < T.length && T[k] <= b; k++) s += h.HV[k]; return s; }
  function dmgIn(h, a, b){
    // damage events (combat log, tick resolution) that hit this hero in [a, b]; entity samples are taken at tick start, before that tick's damage
    const T = h.DT; if (!T) return {sum: 0, last: null, ty: [0, 0, 0]};
    const lo = firstAtOrAfter(T, a);
    let sum = 0, last = null; const ty = [0, 0, 0];
    for (let k = lo; k < T.length && T[k] <= b; k++){ sum += h.DV[k]; ty[h.DY[k]] += h.DV[k]; last = T[k]; }
    return {sum, last, ty};
  }
  function hpNowAt(h, t){
    const i = idx(t); let si = i; while (si > 0 && S.D.sampleT && S.D.sampleT[si] > t) si--;
    if (!S.D.sampleT || !h.alive[si]) return h.hp[i];
    const ts = S.D.sampleT[si], mhp = h.mhp[si] || 1;
    return Math.max(0, Math.min(mhp, h.hp[si] - dmgIn(h, ts, t).sum + healIn(h, ts, t)));
  }
  function barsHTML(h, t, i, alive, hp, mhp, mp, mmp, rowAttributes = ""){
    // HP: latest entity sample (exact sample time) minus combat-log damage plus heals since that sample
    const hpNow = alive === false ? 0 : hpNowAt(h, t);
    const hpKnown = Number.isFinite(hpNow) && Number.isFinite(mhp) && mhp > 0;
    const mpKnown = Number.isFinite(mp) && Number.isFinite(mmp) && mmp > 0;
    const rec = dmgIn(h, t - 1, t), chunk = alive ? Math.min(rec.sum, mhp - hpNow) : 0, fade = rec.last == null ? 0 : Math.max(0, 1 - (t - rec.last) / 1.2);
    const tyTop = rec.ty.indexOf(Math.max(...rec.ty)), tyLab = ["กายภาพ", "เวท", "pure"][tyTop];
    const hpBar = `<div class="bar hp${hpKnown && hpNow / mhp < .25 ? " low" : ""}">${hpKnown ? `<i style="width:${hpNow / mhp * 100}%"></i>` : ""}` +
      (chunk > 0 && fade > 0 ? `<b class="chunk" style="left:${hpNow / mhp * 100}%;width:${chunk / mhp * 100}%;opacity:${fade.toFixed(2)}"></b>` : "") +
      `<span class="num">${alive === false ? "ตาย" : hpKnown ? fmt(hpNow) : "—"} / ${fmt(mhp)}</span>` +
      (rec.sum > 0 && fade > 0 ? `<em class="dmgnum ty${tyTop}" style="opacity:${Math.max(.35, fade).toFixed(2)}" title="ดาเมจใน 1 วินาทีล่าสุด (ส่วนใหญ่เป็น${tyLab})">−${fmt(rec.sum)}</em>` : "") + `</div>`;
    let cost = "";
    if (S.hoverAb && S.hoverAb.id === h.id && mpKnown){
      const a = h.abil[S.hoverAb.k], mc = a ? a.mc[i] : 0;
      if (mc > 0){
        const enough = Number.isFinite(mp) && mc <= mp, from = Math.max(0, mp - mc);
        cost = `<b class="cost${enough ? "" : " short"}" style="left:${from / mmp * 100}%;width:${Math.min(mp, mc) / mmp * 100}%"></b><em class="costnum${enough ? "" : " short"}">${enough ? "−" + fmt(mc) : `ขาด ${fmt(mc - mp)}`}</em>`;
      }
    }
    const mpBar = `<div class="bar mp">${mpKnown ? `<i style="width:${mp / mmp * 100}%"></i>` : ""}${cost}<span class="num">${fmt(mp)} / ${fmt(mmp)}</span></div>`;
    return `<div class="bars" ${rowAttributes}>${hpBar}${mpBar}</div>`;
  }
  function renderCards(force){
    const key = Math.floor(S.t * 10) + [...S.vis].join() + S.focus + S.alerts.length + JSON.stringify(S.hoverAb || "");
    if (!force && key === S.lastCardKey) return; S.lastCardKey = key;
    for (const [team, el] of [[2, $("rv-cards-r")], [3, $("rv-cards-d")]]){
      const hs = S.D.heroes.filter(h => h.team === team);
      if (!el.children.length){
        for (const h of hs){
          const c = document.createElement("article"); c.className = "rc"; c.id = "card-" + h.pid; c.tabIndex = 0; c.style.setProperty("--c", h.color);
          c.addEventListener("click", ev => { if (!S.vis.has(h.id)){ S.vis.add(h.id); changedVis(); } setFocus(h.id); });
          el.appendChild(c);
        }
      }
      for (const h of hs){
        const c = $("card-" + h.pid), i = idx(S.t);
        c.innerHTML = cardHTML(h, S.t);
        c.classList.toggle("dead", !h.alive[i]); c.classList.toggle("focus", S.focus === h.id);
        c.classList.toggle("alert", !!activeAlert(h, S.t)); c.classList.toggle("hid", !S.vis.has(h.id));
      }
    }
    renderScore();
  }
  /* score + momentum */
  function teamNW(team, i){ let s = 0; for (const h of S.D.heroes) if (h.team === team) s += h.eco[i][1]; return s; }
  function advSeries(){
    if (S.adv) return S.adv;
    const n = S.D.heroes[0].eco.length, a = new Float32Array(n); let mx = 1;
    for (let i = 0; i < n; i++){ a[i] = teamNW(2, i) - teamNW(3, i); mx = Math.max(mx, Math.abs(a[i])); }
    return S.adv = {a, mx};
  }
  function renderScore(){
    const t = S.t, i = idx(t), {a, mx} = advSeries(), adv = a[i];
    const kr = S.D.deaths.filter(d => d.t <= t && S.D.byId[d.hero].team === 3).length, kd = S.D.deaths.filter(d => d.t <= t && S.D.byId[d.hero].team === 2).length;
    $("sc-r").textContent = kr; $("sc-d").textContent = kd; $("sc-clock").textContent = clk(t);
    const f = Math.min(1, Math.abs(adv) / Math.max(10000, mx * .6));
    const fill = $("mo-fill"); fill.style.width = (f * 50) + "%";
    fill.style.left = adv >= 0 ? (50 - f * 50) + "%" : "50%"; fill.className = adv >= 0 ? "rad" : "dire";
    $("mo-txt").textContent = Math.abs(adv) < 300 ? "สูสี" : `${adv > 0 ? "Radiant" : "Dire"} นำ ${(Math.abs(adv) / 1000).toFixed(1)}k`;
    drawMomentum();
  }
  function drawMomentum(){
    const cvm = $("mo-graph"), w = cvm.clientWidth, h = cvm.clientHeight, d = window.devicePixelRatio || 1;
    if (cvm.width !== Math.round(w * d)){ cvm.width = Math.round(w * d); cvm.height = Math.round(h * d); }
    const c = cvm.getContext("2d"); c.setTransform(d, 0, 0, d, 0, 0); c.clearRect(0, 0, w, h);
    const {a, mx} = advSeries(), n = a.length, mid = h / 2, y = v => mid - v / mx * (mid - 3), x = k => k / (n - 1) * w;
    for (const [sign, col] of [[1, css("--good") || "#2f9e55"], [-1, css("--bad") || "#c63c3b"]]){
      c.beginPath(); c.moveTo(0, mid);
      for (let k = 0; k < n; k += 3){ const v = sign > 0 ? Math.max(0, a[k]) : Math.min(0, a[k]); c.lineTo(x(k), y(v)); }
      c.lineTo(w, mid); c.closePath(); c.fillStyle = col; c.globalAlpha = .55; c.fill(); c.globalAlpha = 1;
    }
    c.fillStyle = css("--line") || "#ccc"; c.fillRect(0, mid, w, 1);
    const xc = x(idx(S.t)); c.fillStyle = css("--ink") || "#111"; c.fillRect(xc - 1, 0, 2, h);
  }

  /* ---------- roster, lists, timeline ---------- */
  function renderRoster(){
    for (const [team, el] of [[2, $("rv-r")], [3, $("rv-d")]]){
      if (!el.children.length){
        for (const h of S.D.heroes.filter(h => h.team === team)){
          const b = document.createElement("button"); b.className = "rv-chip"; b.id = "chip-" + h.pid; b.style.setProperty("--c", h.color);
          b.innerHTML = `<img src="${portSrc(h.id)}" width="24" height="24" alt=""><span class="nm">${esc(h.name)}</span><span class="st"></span>`;
          b.addEventListener("click", () => { S.vis.has(h.id) ? S.vis.delete(h.id) : S.vis.add(h.id); changedVis(); });
          el.appendChild(b);
        }
      }
      for (const h of S.D.heroes.filter(h => h.team === team)){
        const b = $("chip-" + h.pid); b.setAttribute("aria-pressed", S.vis.has(h.id));
        const rs = respawnIn(h, S.t), al = activeAlert(h, S.t);
        b.querySelector(".st").textContent = rs != null ? "ตาย " + Math.ceil(rs) + "s" : al && al.type !== "ult" ? "⚠" : "";
      }
    }
  }
  function changedVis(){ S.heat = null; if (S.focus && !S.vis.has(S.focus)) S.focus = null; renderRoster(); renderLists(); renderTicks(); renderCards(true); renderMulti(); draw(); tlDraw(true); }
  function evItem(a, onClick, on){
    const li = document.createElement("li"), b = document.createElement("button"), h = S.D.byId[a.hero];
    b.style.setProperty("--c", h.color);
    b.innerHTML = `<span class="num">${clk(a.t0)}</span><img src="${portSrc(h.id)}" width="20" height="20" alt=""><span><span class="ty">${esc(TYPE[a.type])} · ${esc(h.name)}</span>${esc(a.text)}</span>`;
    if (on) b.classList.add("on"); b.addEventListener("click", onClick); li.appendChild(b); return li;
  }
  function renderLists(){
    const al = $("rv-alist"); al.innerHTML = "";
    const list = S.alerts.filter(a => S.vis.has(a.hero));
    $("al-count").textContent = `${list.length} เหตุการณ์ของฮีโร่ที่เลือก · กดเพื่อเล่นตั้งแต่ 5 วินาทีก่อนเกิด`;
    for (const a of list) al.appendChild(evItem(a, () => playClip(a.t0 - 5, a.t1 + 3, a.hero)));
    const dl = $("rv-dlist"); dl.innerHTML = "";
    for (const d of S.D.deaths.filter(d => S.vis.has(d.hero))) dl.appendChild(evItem({hero: d.hero, type: "death", t0: d.t, text: "โดย " + nameOf(d.killer)}, () => playDeath(d), S.review === d));
    if (!dl.children.length) dl.innerHTML = `<li class="note">ไม่มีการตายของฮีโร่ที่เลือก</li>`;
  }
  function renderTicks(){
    const el = $("rv-ticks"), span = S.D.t1 - S.D.t0; el.innerHTML = "";
    const at = t => `calc(8px + (100% - 16px) * ${(t - S.D.t0) / span})`;
    for (const a of S.alerts.filter(a => S.vis.has(a.hero) && a.type !== "death")){ const i = document.createElement("i"); i.className = "al"; i.style.left = at(a.t0); el.appendChild(i); }
    for (const d of S.D.deaths.filter(d => S.vis.has(d.hero))){ const i = document.createElement("i"); i.style.left = at(d.t); i.style.background = S.D.byId[d.hero].color; el.appendChild(i); }
  }
  function renderReview(d){
    const total = d.by.reduce((s, x) => s + x[2], 0) || 1, mx = Math.max(...d.by.map(x => x[2]), 1);
    $("rv-review").innerHTML = `<div><b>${esc(nameOf(d.hero))}</b> ตายที่ <span class="num">${clk(d.t)}</span></div>
      <div>ปิดคิลโดย <b>${esc(nameOf(d.killer))}</b>${d.last ? ` · ดาเมจสุดท้ายจาก ${esc(nameOf(d.last[0]))} (${esc(nameOf(d.last[1]))})` : ""}</div>
      <div style="font:600 12px var(--f-mono);color:var(--muted);letter-spacing:.06em">ดาเมจที่ได้รับ 10 วินาทีก่อนตาย · รวม ${fmt(total)}</div>
      <div class="bars">${d.by.map(([src, inf, v]) => `<div class="bar"><span style="--w:${v / mx * 100}%"><span>${esc(nameOf(src))} · ${esc(nameOf(inf))}</span></span><span class="num">${fmt(v)}</span></div>`).join("")}</div>`;
  }
  function playClip(from, to, hero){
    setMode("route"); setSpeed(1); if (hero){ if (!S.vis.has(hero)){ S.vis.add(hero); changedVis(); } S.focus = hero; }
    S.stopAt = to; setT(Math.max(S.D.t0, from)); play();
  }
  function playDeath(d){ S.review = d; renderReview(d); renderDeathLab(d); showTab("death"); playClip(d.t - 15, d.t, d.hero); renderLists(); }
  function setFocus(id){
    S.focus = S.focus === id ? null : id; renderCards(true); draw();
    if (S.focus){ const c = $("card-" + S.D.byId[id].pid); if (c) c.scrollIntoView({block: "nearest", behavior: "smooth"}); }
  }
  function showTab(k){ document.querySelectorAll("[data-tab]").forEach(b => b.setAttribute("aria-pressed", b.dataset.tab === k)); $("pane-alert").hidden = k !== "alert"; $("pane-death").hidden = k !== "death"; }

  /* ---------- playback ---------- */
  const playBtn = $("rv-play"), scrub = $("rv-scrub"), clock = $("rv-clock");
  function setT(t){
    S.t = Math.max(S.D.t0, Math.min(S.D.t1, t)); scrub.value = Math.floor(S.t);
    clock.textContent = clk(S.t) + " / " + clk(S.D.t1); renderRoster(); renderCards(); draw(); tlDraw();
  }
  let last = 0;
  function frame(ts){
    if (!S.playing) return;
    const dt = last ? Math.min(.25, (ts - last) / 1000) : 0; last = ts;
    const prev = S.t; let t = S.t + dt * S.speed;
    if (S.stopAt != null && t >= S.stopAt){ S.stopAt = null; setT(Math.min(t, S.D.t1)); pause(); return; }
    if (S.autoPause && S.stopAt == null){
      const at = a => a.ts != null ? a.ts : a.t0, a = S.alerts.find(a => S.vis.has(a.hero) && at(a) > prev && at(a) <= t);
      if (a){ S.focus = a.hero; setT(at(a)); pause(); return; }
    }
    if (t >= S.D.t1){ setT(S.D.t1); pause(); return; }
    setT(t); requestAnimationFrame(frame);
  }
  function play(){ if (S.t >= S.D.t1) S.t = S.D.t0; S.playing = true; last = 0; playBtn.textContent = "หยุด"; playBtn.setAttribute("aria-label", "หยุด (Space)"); requestAnimationFrame(frame); }
  function pause(){ S.playing = false; playBtn.textContent = "เล่น"; playBtn.setAttribute("aria-label", "เล่น (Space)"); renderCards(true); }
  const SPEEDS = [1, 2, 5, 10, 30];
  function setSpeed(v){ S.speed = v; document.querySelectorAll("[data-sp]").forEach(b => b.setAttribute("aria-pressed", +b.dataset.sp === v)); }
  function setMode(m){
    S.mode = m; document.querySelectorAll("[data-mode]").forEach(b => b.setAttribute("aria-pressed", b.dataset.mode === m));
    $("rv-heat-opts").hidden = m !== "heat"; $("rv-route-opts").hidden = m !== "route"; if (m === "heat") pause();
    legend(); renderMulti(); draw();
  }
  function legend(){
    $("rv-legend").innerHTML = S.mode === "route"
      ? `<span><svg width="22" height="8"><line x1="1" y1="4" x2="21" y2="4" stroke="var(--ink-2)" stroke-width="3" stroke-linecap="round"/></svg>เส้นทาง ${S.trail} วินาทีล่าสุด (สีตามรายชื่อ)</span>
         <span><svg width="16" height="16"><path d="M3 12 A6 6 0 1 1 13 12" fill="none" stroke="#5fe08a" stroke-width="2.5"/></svg>ส่วนโค้งใต้ไอคอน = HP</span>
         <span><svg width="18" height="18"><circle cx="9" cy="9" r="8" fill="rgba(255,40,40,.35)"/><circle cx="9" cy="9" r="5" fill="none" stroke="#ff4646" stroke-width="2"/></svg>วงแดง = เหตุการณ์สำคัญ</span>
         <span>ไอคอนสีเทา = ตาย พร้อมเวลาเกิด · ข้อความสีเหลือง = สกิลที่เพิ่งใช้</span>`
      : `<span>${S.heatStyle === "hero" && S.vis.size > 1 ? "แต่ละจุดเป็นสีของฮีโร่ที่อยู่ตรงนั้นนานที่สุด ยิ่งทึบยิ่งอยู่นาน ชี้เพื่อดูสัดส่วน" : `<i style="display:inline-block;width:70px;height:8px;border-radius:4px;background:linear-gradient(90deg,rgba(255,120,30,.4),#ffebb4)"></i>อยู่บ่อยขึ้น`}</span>
         <span><svg width="14" height="14"><circle cx="7" cy="7" r="4.5" fill="var(--ink-2)" stroke="#ff3b3b" stroke-width="2"/></svg>จุดตาย (คลิกเพื่อเล่น 15 วินาทีก่อนตาย)</span>
         <span>การ์ดเล็กด้านล่างคือ heatmap ของแต่ละตัว คลิกเพื่อดูเฉพาะตัวนั้น</span>`;
  }

  /* ---------- events ---------- */
  playBtn.addEventListener("click", () => { S.stopAt = null; S.playing ? pause() : play(); });
  scrub.addEventListener("input", () => { pause(); S.stopAt = null; setT(+scrub.value); });
  document.querySelectorAll("[data-sp]").forEach(b => b.addEventListener("click", () => setSpeed(+b.dataset.sp)));
  document.querySelectorAll("[data-mode]").forEach(b => b.addEventListener("click", () => setMode(b.dataset.mode)));
  document.querySelectorAll("[data-hs]").forEach(b => b.addEventListener("click", () => { S.heatStyle = b.dataset.hs; document.querySelectorAll("[data-hs]").forEach(x => x.setAttribute("aria-pressed", x === b)); S.heat = null; legend(); draw(); }));
  document.querySelectorAll("[data-tab]").forEach(b => b.addEventListener("click", () => showTab(b.dataset.tab)));
  $("rv-trail").addEventListener("change", e => { S.trail = +e.target.value; legend(); draw(); });
  $("rv-range").addEventListener("change", e => { S.range = e.target.value.split(",").map(v => v === "start" ? S.D.t0 : v === "end" ? S.D.t1 : Number(v)); S.heat = null; renderMulti(); draw(); });
  $("rv-names").addEventListener("change", e => { S.names = e.target.checked; draw(); });
  $("rv-dmgf").addEventListener("change", e => { S.dmgFloat = e.target.checked; draw(); });
  $("rv-fog").addEventListener("change", e => { S.fog = e.target.value; draw(); });
  document.querySelectorAll("[data-layer]").forEach(c => c.addEventListener("change", () => { S.layers[c.dataset.layer] = c.checked; draw(); }));
  $("rv-autopause").addEventListener("change", e => { S.autoPause = e.target.checked; });
  $("rv-follow").addEventListener("change", e => { S.follow = e.target.checked; if (!S.follow){ S.view = {k: S.view.k, ox: S.view.ox, oy: S.view.oy}; } draw(); });
  $("z-in").addEventListener("click", () => zoomAt(W / 2, H / 2, S.view.k * 1.5));
  $("z-out").addEventListener("click", () => zoomAt(W / 2, H / 2, S.view.k / 1.5));
  $("z-reset").addEventListener("click", () => { S.follow = false; $("rv-follow").checked = false; S.view = {k: 1, ox: 0, oy: 0}; clampView(); draw(); });
  for (const id of ["al-hp", "al-hp-pct", "al-hp-win", "al-low", "al-low-pct", "al-cc", "al-cc-s", "al-ult", "al-death"])
    $(id).addEventListener("change", () => { buildAlerts(); renderLists(); renderTicks(); renderCards(true); draw(); });
  document.querySelectorAll("[data-g]").forEach(b => b.addEventListener("click", () => {
    const g = b.dataset.g;
    S.vis = new Set(S.D.heroes.filter(h => g === "all" || String(h.team) === g).map(h => h.id));
    changedVis();
  }));
  function hit(sx, sy){
    let best = null, bd = 1e9;
    for (const o of S.hits){ const d = Math.hypot(o.x - sx, o.y - sy); if (d <= (o.r || 10) + 4 && d < bd){ bd = d; best = o; } }
    return best;
  }
  let drag = null;
  cv.addEventListener("pointerdown", ev => { drag = {x: ev.clientX, y: ev.clientY, ox: S.view.ox, oy: S.view.oy, moved: false}; });
  on(window, "pointerup", () => { if (drag && drag.moved) cv.classList.remove("dragging"); setTimeout(() => drag = null, 0); });
  cv.addEventListener("pointermove", ev => {
    const r = cv.getBoundingClientRect(), sx = ev.clientX - r.left, sy = ev.clientY - r.top;
    if (drag && ev.buttons){
      const dx = ev.clientX - drag.x, dy = ev.clientY - drag.y;
      if (Math.hypot(dx, dy) > 4 && S.view.k > 1){ drag.moved = true; cv.classList.add("dragging"); S.follow = false; $("rv-follow").checked = false; S.view.ox = drag.ox - dx / S.view.k; S.view.oy = drag.oy - dy / S.view.k; clampView(); draw(); }
      return;
    }
    const o = hit(sx, sy); let html = null;
    if (o && o.d) html = `<b>${clk(o.d.t)}</b> ${esc(nameOf(o.d.hero))} ตาย · ${esc(nameOf(o.d.killer))}`;
    else if (o && o.h){ const h = o.h, i = idx(S.t), al = activeAlert(h, S.t); html = `<b>${esc(h.name)}</b> · Lv ${h.lvl[i]}<br>HP ${fmt(h.alive[i] ? h.hp[i] : 0)} / ${fmt(h.mhp[i])} · MP ${fmt(h.mp[i])}${al ? `<br><span style="color:var(--bad)">⚠ ${esc(al.text)}</span>` : ""}`; }
    else if (o && o.lab) html = esc(o.lab);
    else if (S.mode === "heat") html = heatTip(sx, sy);
    if (!html){ tip.hidden = true; return; }
    tip.innerHTML = html; tip.style.left = Math.max(110, Math.min(r.width - 110, sx)) + "px"; tip.style.top = sy + "px"; tip.hidden = false;
  });
  cv.addEventListener("pointerleave", () => tip.hidden = true);
  cv.addEventListener("click", ev => {
    if (drag && drag.moved) return;
    const r = cv.getBoundingClientRect(), o = hit(ev.clientX - r.left, ev.clientY - r.top); if (!o) return;
    if (o.d) playDeath(o.d); else if (o.h) setFocus(o.h.id);
  });
  cv.addEventListener("wheel", ev => { ev.preventDefault(); const r = cv.getBoundingClientRect(); S.follow = false; $("rv-follow").checked = false; zoomAt(ev.clientX - r.left, ev.clientY - r.top, S.view.k * (ev.deltaY < 0 ? 1.2 : 1 / 1.2)); }, {passive: false});
  on(window.document, "keydown", ev => {
    if (disableKeyboard || !S.D || editing(ev) || ev.altKey || ev.ctrlKey || ev.metaKey) return;
    const rect = $("replay").getBoundingClientRect(); if (rect.bottom < 0 || rect.top > innerHeight) return;
    const step = ev.shiftKey ? 10 : 1;
    if (ev.code === "Space"){ ev.preventDefault(); S.stopAt = null; S.playing ? pause() : play(); }
    else if (ev.key === "ArrowRight"){ ev.preventDefault(); pause(); setT(S.t + step); }
    else if (ev.key === "ArrowLeft"){ ev.preventDefault(); pause(); setT(S.t - step); }
    else if (ev.key === "]" || ev.key === "["){ const i = SPEEDS.indexOf(S.speed); setSpeed(SPEEDS[Math.max(0, Math.min(SPEEDS.length - 1, i + (ev.key === "]" ? 1 : -1)))]); }
    else if (ev.key === "f" || ev.key === "F"){ S.follow = !S.follow; $("rv-follow").checked = S.follow; draw(); }
    else if (ev.key === "n" || ev.key === "N"){ const a = S.alerts.find(a => S.vis.has(a.hero) && a.t0 > S.t + .5); if (a){ pause(); S.focus = a.hero; setT(a.t0 - 2); } }
  });
  // keep auras pulsing while paused
  (function pulse(){ if (S.D && !S.playing && S.mode === "route" && visHeroes().some(h => activeAlert(h, S.t))) draw(); setTimeout(() => requestAnimationFrame(pulse), 50); })();
  new ResizeObserver(() => S.D && resize()).observe(wrap);

  /* ---------- hero timeline ---------- */
  const iconSrc = n => icon("item", n.startsWith("item_recipe_") ? "recipe" : n.replace(/^item_/, ""));
  const TL = {metric: "nw", v0: -90, v1: 2222, follow: true, rows: [], icons: {}, clusters: [], raf: 0, dragging: false};
  const tlc = $("tl-canvas"), tctx = tlc.getContext("2d"), tlTip = $("tl-tip"), ov = $("tl-ov"), octx = ov.getContext("2d");
  const LABEL = 168, ROW = 74, BUB = 11, TOP = 22, BOT = 6;
  let TW = 800, TH = 400;
  function tlIcon(n){
    if (!(n in TL.icons)){ const im = new Image(); TL.icons[n] = im; im.onload = () => tlDraw(true); im.onerror = () => { TL.icons[n] = null; }; im.src = iconSrc(n); }
    return TL.icons[n];
  }
  function metricVal(h, i){
    const e = h.eco[i];
    return TL.metric === "nw" ? e[1] : TL.metric === "lh" ? e[2] : TL.metric === "gold" ? e[0] : h.lvl[i];
  }
  const METRIC_LABEL = {nw: "Net worth", lh: "Last hit", gold: "ทองในมือ", lvl: "Level"};
  function tlSize(){
    const rows = S.D.heroes.filter(h => S.vis.has(h.id));
    TL.rows = rows;
    TW = $("tl-wrap").clientWidth; TH = TOP + Math.max(1, rows.length) * ROW + BOT;
    const d = window.devicePixelRatio || 1;
    tlc.width = Math.round(TW * d); tlc.height = Math.round(TH * d); tlc.style.height = TH + "px"; tctx.setTransform(d, 0, 0, d, 0, 0);
    const bw = $("tl-brush").clientWidth, bh = $("tl-brush").clientHeight;
    ov.width = Math.round(bw * d); ov.height = Math.round(bh * d); octx.setTransform(d, 0, 0, d, 0, 0);
    // one y-scale for every row so heroes compare directly
    let mx = 1; for (const h of rows) for (let i = 0; i < h.eco.length; i++) mx = Math.max(mx, metricVal(h, i));
    TL.max = mx;
  }
  const tx = t => LABEL + (t - TL.v0) / (TL.v1 - TL.v0) * (TW - LABEL - 12);
  const ttime = x => TL.v0 + (x - LABEL) / (TW - LABEL - 12) * (TL.v1 - TL.v0);
  function tlDraw(full){
    if (!S.D) return;
    if (full || !TL.rows.length && S.vis.size) tlSize();
    if (TL.follow && S.playing && (S.t > TL.v1 - (TL.v1 - TL.v0) * .1 || S.t < TL.v0)){
      const w = TL.v1 - TL.v0; TL.v0 = Math.max(S.D.t0, Math.min(S.D.t1 - w, S.t - w * .2)); TL.v1 = TL.v0 + w;
    }
    const c = tctx, muted = css("--muted") || "#888", ink = css("--ink") || "#111", grid = css("--grid") || "#ddd", surf = css("--surface") || "#fff";
    c.clearRect(0, 0, TW, TH);
    // time axis
    const span = TL.v1 - TL.v0, step = [10, 15, 30, 60, 120, 300, 600].find(s => span / s <= 9) || 600;
    c.font = `11px ${css("--f-mono")}`; c.textAlign = "center"; c.fillStyle = muted;
    for (let t = Math.ceil(TL.v0 / step) * step; t <= TL.v1; t += step){
      const x = tx(t); c.fillStyle = grid; c.fillRect(x, TOP - 4, 1, TH - TOP); c.fillStyle = muted; c.fillText(clk(t), x, 13);
    }
    TL.clusters = [];
    const i0 = idx(S.t);
    TL.rows.forEach((h, r) => {
      const y0 = TOP + r * ROW, base = y0 + ROW - 6, top = y0 + BUB * 2 + 8, hgt = base - top;
      // row label with value at replay time
      c.fillStyle = r % 2 ? "transparent" : (css("--surface") || "#fff"); c.globalAlpha = .55; c.fillRect(0, y0, TW, ROW); c.globalAlpha = 1;
      const im = S.port[h.id];
      c.save(); c.beginPath(); c.arc(20, y0 + 22, 14, 0, 7); c.clip(); if (im && im.complete && im.naturalWidth) c.drawImage(im, 56, 0, 144, 144, 6, y0 + 8, 28, 28); c.restore();
      c.strokeStyle = h.color; c.lineWidth = 2.5; c.beginPath(); c.arc(20, y0 + 22, 14, 0, 7); c.stroke();
      c.textAlign = "left"; c.fillStyle = ink; c.font = `600 12.5px ${css("--f-body")}`; c.fillText(h.name, 40, y0 + 19);
      const e = h.eco[i0]; c.font = `11px ${css("--f-mono")}`; c.fillStyle = css("--ink-2") || "#555";
      c.fillText(`LH ${e[2]}/${e[3]} · NW ${fmt(e[1])}`, 40, y0 + 35);
      c.fillText(translate(`Lv ${h.lvl[i0]} · ทอง ${fmt(e[0])}`), 40, y0 + 50);
      // metric area
      const a = Math.max(0, Math.floor(TL.v0) - S.D.t0), b = Math.min(h.eco.length - 1, Math.ceil(TL.v1) - S.D.t0);
      const stride = Math.max(1, Math.floor((b - a) / (TW - LABEL)));
      c.beginPath(); c.moveTo(tx(a + S.D.t0), base);
      for (let i = a; i <= b; i += stride) c.lineTo(tx(i + S.D.t0), base - metricVal(h, i) / TL.max * hgt);
      c.lineTo(tx(b + S.D.t0), base - metricVal(h, b) / TL.max * hgt); c.lineTo(tx(b + S.D.t0), base); c.closePath();
      c.fillStyle = h.color; c.globalAlpha = .22; c.fill(); c.globalAlpha = 1;
      c.beginPath();
      for (let i = a; i <= b; i += stride){ const x = tx(i + S.D.t0), y = base - metricVal(h, i) / TL.max * hgt; i === a ? c.moveTo(x, y) : c.lineTo(x, y); }
      c.strokeStyle = h.color; c.lineWidth = 2; c.stroke();
      c.fillStyle = grid; c.fillRect(LABEL, base, TW - LABEL - 12, 1);
      // deaths
      for (const d of S.D.deaths) if (d.hero === h.id && d.t >= TL.v0 && d.t <= TL.v1){
        const x = tx(d.t); c.strokeStyle = "#e14b4b"; c.lineWidth = 2.5; c.beginPath(); c.moveTo(x - 4, base - 4); c.lineTo(x + 4, base + 4); c.moveTo(x + 4, base - 4); c.lineTo(x - 4, base + 4); c.stroke();
      }
      // purchases, clustered in screen space
      const buys = h.buys.concat((h.finds || []).map(f => [f[0], f[1], "find", f[2], f[3]])).sort((p, q) => p[0] - q[0]).filter(b => b[0] >= TL.v0 - 5 && b[0] <= TL.v1 + 5);
      let cl = null;
      for (const bb of buys){
        const x = tx(bb[0]);
        if (cl && x - cl.x < BUB * 2 + 3){ cl.items.push(bb); continue; }
        cl = {x, hero: h, row: r, y: y0 + BUB + 3, items: [bb]}; TL.clusters.push(cl);
      }
      for (const k of TL.clusters.filter(k => k.row === r)){
        const main = [...k.items].reverse().find(b => b[2] === "find") || [...k.items].reverse().find(b => !isConsumableItem(patchData, b[1]) && !b[1].startsWith("item_recipe_")) || k.items[k.items.length - 1];
        k.main = main;
        c.strokeStyle = h.color; c.globalAlpha = .45; c.lineWidth = 1; c.beginPath(); c.moveTo(k.x, k.y + BUB); c.lineTo(k.x, base); c.stroke(); c.globalAlpha = 1;
        const ic = tlIcon(main[1]);
        c.save(); c.beginPath(); c.arc(k.x, k.y, BUB, 0, 7); c.fillStyle = surf; c.fill(); c.clip();
        if (ic && ic.complete && ic.naturalWidth) c.drawImage(ic, 12, 0, 64, 64, k.x - BUB, k.y - BUB, BUB * 2, BUB * 2);
        else { c.fillStyle = ink; c.font = `600 8px ${css("--f-mono")}`; c.textAlign = "center"; c.fillText(nameOf(main[1]).slice(0, 3), k.x, k.y + 3); }
        c.restore();
        c.strokeStyle = main[2] === "find" ? "#3bb36b" : isConsumableItem(patchData, main[1]) ? muted : h.color; c.lineWidth = main[2] === "find" ? 3 : 2; c.beginPath(); c.arc(k.x, k.y, BUB, 0, 7); c.stroke();
        if (k.items.length > 1){
          const t = "+" + (k.items.length - 1); c.font = `600 9px ${css("--f-mono")}`; const w = c.measureText(t).width + 6;
          c.fillStyle = ink; c.beginPath(); c.roundRect ? c.roundRect(k.x + BUB - 5, k.y - BUB - 3, w, 12, 6) : c.rect(k.x + BUB - 5, k.y - BUB - 3, w, 12); c.fill();
          c.fillStyle = surf; c.textAlign = "left"; c.fillText(t, k.x + BUB - 2, k.y - BUB + 6);
        }
      }
    });
    if (!TL.rows.length){ c.fillStyle = muted; c.textAlign = "left"; c.font = `13px ${css("--f-body")}`; c.fillText(translate("เลือกฮีโร่จากรายชื่อด้านบนเพื่อแสดงไทม์ไลน์"), 16, TOP + 30); }
    // replay cursor
    if (S.t >= TL.v0 && S.t <= TL.v1){
      const x = tx(S.t); c.fillStyle = css("--rad") || "#2a78d6"; c.fillRect(x - 1, TOP - 6, 2, TH - TOP + 6);
      c.font = `600 11px ${css("--f-mono")}`; const lbl = clk(S.t), w = c.measureText(lbl).width + 10;
      c.beginPath(); c.roundRect ? c.roundRect(x - w / 2, 1, w, 16, 8) : c.rect(x - w / 2, 1, w, 16); c.fill(); c.fillStyle = "#fff"; c.textAlign = "center"; c.fillText(lbl, x, 13);
    }
    tlBrush();
  }
  function tlBrush(){
    const el = $("tl-brush"), bw = el.clientWidth, bh = el.clientHeight, span = S.D.t1 - S.D.t0;
    octx.clearRect(0, 0, bw, bh);
    for (const h of TL.rows){
      octx.beginPath();
      for (let i = 0; i < h.eco.length; i += 6){ const x = i / (h.eco.length - 1) * bw, y = bh - 4 - metricVal(h, i) / TL.max * (bh - 8); i ? octx.lineTo(x, y) : octx.moveTo(x, y); }
      octx.strokeStyle = h.color; octx.globalAlpha = .6; octx.lineWidth = 1.2; octx.stroke(); octx.globalAlpha = 1;
    }
    const xt = (S.t - S.D.t0) / span * bw; octx.fillStyle = css("--rad") || "#2a78d6"; octx.fillRect(xt - 1, 0, 2, bh);
    const l = (TL.v0 - S.D.t0) / span * 100, r = (TL.v1 - S.D.t0) / span * 100;
    $("tl-win").style.left = l + "%"; $("tl-win").style.width = (r - l) + "%";
    $("tl-shade-l").style.width = l + "%"; $("tl-shade-r").style.width = (100 - r) + "%";
    const m = (TL.v1 - TL.v0) / 60; $("tz-lvl").textContent = m >= 1 ? `${m.toFixed(m < 10 ? 1 : 0)} นาที` : `${Math.round(TL.v1 - TL.v0)} วินาที`;
  }
  function tlSetWindow(a, b){
    const min = 20, D = S.D; if (b - a < min){ const c = (a + b) / 2; a = c - min / 2; b = c + min / 2; }
    if (a < D.t0){ b += D.t0 - a; a = D.t0; } if (b > D.t1){ a -= b - D.t1; b = D.t1; }
    TL.v0 = Math.max(D.t0, a); TL.v1 = Math.min(D.t1, b); tlDraw();
  }
  function tlZoom(f, center){ const c = center != null ? center : (S.t >= TL.v0 && S.t <= TL.v1 ? S.t : (TL.v0 + TL.v1) / 2), w = (TL.v1 - TL.v0) * f; tlSetWindow(c - (c - TL.v0) * f, c - (c - TL.v0) * f + w); }
  function tlHit(x, y){
    let best = null, bd = BUB + 4;
    for (const k of TL.clusters){ const d = Math.hypot(k.x - x, k.y - y); if (d < bd){ bd = d; best = k; } }
    return best;
  }
  function tlInit(){
    document.querySelectorAll("[data-tm]").forEach(b => b.addEventListener("click", () => { TL.metric = b.dataset.tm; document.querySelectorAll("[data-tm]").forEach(x => x.setAttribute("aria-pressed", x === b)); tlDraw(true); }));
    $("tl-follow").addEventListener("change", e => { TL.follow = e.target.checked; });
    $("tz-in").addEventListener("click", () => tlZoom(1 / 1.6));
    $("tz-out").addEventListener("click", () => tlZoom(1.6));
    $("tz-all").addEventListener("click", () => tlSetWindow(S.D.t0, S.D.t1));
    tlc.addEventListener("wheel", ev => { ev.preventDefault(); const r = tlc.getBoundingClientRect(), x = ev.clientX - r.left; if (x < LABEL) return; tlZoom(ev.deltaY < 0 ? 1 / 1.25 : 1.25, ttime(x)); }, {passive: false});
    const seek = ev => { const r = tlc.getBoundingClientRect(), x = ev.clientX - r.left; if (x < LABEL) return; pause(); S.stopAt = null; setT(Math.round(ttime(x))); };
    tlc.addEventListener("pointerdown", ev => { const r = tlc.getBoundingClientRect(); if (tlHit(ev.clientX - r.left, ev.clientY - r.top)) return; TL.dragging = true; tlc.setPointerCapture(ev.pointerId); seek(ev); });
    tlc.addEventListener("pointerup", () => { TL.dragging = false; });
    tlc.addEventListener("pointermove", ev => {
      if (TL.dragging){ seek(ev); return; }
      const r = tlc.getBoundingClientRect(), x = ev.clientX - r.left, y = ev.clientY - r.top;
      const k = tlHit(x, y); let html = null, px = x, py = y;
      if (k){
        html = `<b>${esc(k.hero.name)}</b> · ${k.items.length} ชิ้น<div class="tl-tip-items">` +
          k.items.map(b => `<div><img src="${iconSrc(b[1])}" alt=""><span class="num">${clk(b[0])}</span>${esc(nameOf(b[1]))}${b[2] === "find" ? ` <span style="color:var(--good)">(ไอเทมป่า · ${esc(b[3])} ${b[4]})</span>` : isConsumableItem(patchData, b[1]) ? ' <span style="color:var(--muted)">(ของใช้)</span>' : ""}</div>`).join("") + "</div>";
        px = k.x; py = k.y - BUB;
      } else if (x > LABEL){
        const row = Math.floor((y - TOP) / ROW), h = TL.rows[row];
        if (h){
          const t = Math.round(ttime(x)), i = idx(t), e = h.eco[i];
          html = `<b>${esc(h.name)} · ${clk(t)}</b><br>LH ${e[2]} / DN ${e[3]}<br>Net worth ${fmt(e[1])} · ทอง ${fmt(e[0])}<br>Level ${h.lvl[i]}${h.alive[i] ? "" : ' · <span style="color:var(--bad)">ตาย</span>'}`;
          py = TOP + row * ROW + 10;
        }
      }
      if (!html){ tlTip.hidden = true; return; }
      tlTip.innerHTML = html; tlTip.style.left = Math.max(120, Math.min(TW - 140, px)) + "px"; tlTip.style.top = Math.max(60, py) + "px"; tlTip.hidden = false;
    });
    tlc.addEventListener("pointerleave", () => { tlTip.hidden = true; });
    // brush: round handles resize, middle pans
    const brush = $("tl-brush"); let bd = null;
    const bt = clientX => { const r = brush.getBoundingClientRect(); return S.D.t0 + (clientX - r.left) / r.width * (S.D.t1 - S.D.t0); };
    brush.addEventListener("pointerdown", ev => {
      const part = ev.target.id === "tl-hl" ? "l" : ev.target.id === "tl-hr" ? "r" : ev.target.id === "tl-win" ? "m" : "jump";
      if (part === "jump"){ const c = bt(ev.clientX), w = TL.v1 - TL.v0; tlSetWindow(c - w / 2, c + w / 2); }
      bd = {part: part === "jump" ? "m" : part, t: bt(ev.clientX), v0: TL.v0, v1: TL.v1}; brush.setPointerCapture(ev.pointerId); TL.follow = false; $("tl-follow").checked = false;
    });
    brush.addEventListener("pointermove", ev => {
      if (!bd) return; const dt = bt(ev.clientX) - bd.t;
      if (bd.part === "l") tlSetWindow(Math.min(bd.v0 + dt, bd.v1 - 20), bd.v1);
      else if (bd.part === "r") tlSetWindow(bd.v0, Math.max(bd.v1 + dt, bd.v0 + 20));
      else tlSetWindow(bd.v0 + dt, bd.v1 + dt);
    });
    brush.addEventListener("pointerup", () => { bd = null; });
    brush.addEventListener("dblclick", () => tlSetWindow(S.D.t0, S.D.t1));
    for (const [id, side] of [["tl-hl", "l"], ["tl-hr", "r"]]) $(id).addEventListener("keydown", ev => {
      const d = ev.key === "ArrowLeft" ? -10 : ev.key === "ArrowRight" ? 10 : 0; if (!d) return; ev.preventDefault(); ev.stopPropagation();
      side === "l" ? tlSetWindow(Math.min(TL.v0 + d, TL.v1 - 20), TL.v1) : tlSetWindow(TL.v0, Math.max(TL.v1 + d, TL.v0 + 20));
    });
    new ResizeObserver(() => tlDraw(true)).observe($("tl-wrap"));
    tlSetWindow(S.t - 180, S.t + 420); tlDraw(true);
    $("mo-graph").addEventListener("click", ev => { const r = ev.currentTarget.getBoundingClientRect(); pause(); S.stopAt = null; setT(S.D.t0 + (ev.clientX - r.left) / r.width * (S.D.t1 - S.D.t0)); });
  }

  /* ---------- death breakdown + what-if simulation ---------- */
  const DL = {d: null, rem: "", add: "", ownBkb: false, X: 3};
  const TYPE_LAB = {p: "กายภาพ", m: "เวท", u: "Pure"};
  const srcIcon = (n, sz) => n.startsWith("npc_dota_hero_") ? `<img class="si" src="${portSrc(n)}" width="${sz}" height="${sz}" alt="">` : `<span class="si dot" style="width:${sz}px;height:${sz}px"></span>`;
  const infIcon = n => n === "attack" ? `<span class="ii atk" title="โจมตีปกติ">⚔</span>` : `<img class="ii" src="${n.startsWith("item_") ? iconSrc(n) : abilSrc(n)}" alt="">`;
  function simDelta(n, sign){
    const e = S.D.statdb.sim[n]; if (!e) return null;
    const all = e.all || 0;
    return {hp: sign * ((e.hp || 0) + 22 * ((e.str || 0) + all)), armor: sign * ((e.armor || 0) + ((e.agi || 0) + all) / 6), int: sign * ((e.int || 0) + all),
      regen: sign * ((e.regen || 0) + .1 * ((e.str || 0) + all)), mr: e.mr || 0, mrSign: sign, bkb: sign > 0 ? e.bkb : 0, bkbmr: e.bkbmr || 0};
  }
  function simDesc(n){
    const e = S.D.statdb.sim[n]; if (!e) return "";
    const p = []; const all = e.all || 0;
    if (e.hp || e.str || all) p.push(`HP +${Math.round((e.hp || 0) + 22 * ((e.str || 0) + all))}`);
    if (e.armor || e.agi || all) p.push(`เกราะ +${(((e.armor || 0) + ((e.agi || 0) + all) / 6)).toFixed(1)}`);
    if (e.mr) p.push(`MR ${e.mr}%`); if (e.int || all) p.push(`INT +${(e.int || 0) + all}`);
    if (e.regen) p.push(`regen +${e.regen}`); if (e.bkb) p.push(`BKB ${e.bkb} วินาที`);
    return p.join(" · ");
  }
  function runSim(d){
    const h = S.D.byId[d.hero], [t0, T] = d.win, i0 = idx(t0 - 1);
    const mhp = h.mhp[idx(t0)] || 1, hpEntity = Math.min(mhp, h.alive[i0] ? h.hp[i0] : h.hp[idx(t0)]);
    // HP is exactly 0 at the death tick, so anchor the start HP to the logged damage minus heals; fall back to the entity sample when that exceeds max HP
    const net = d.hits.reduce((s, x) => s + x[4], 0) - d.heals.reduce((s, x) => s + x[3], 0);
    const hp0 = net > 0 && net <= mhp ? net : hpEntity;
    const deltas = [DL.rem ? simDelta(DL.rem, -1) : null, DL.add ? simDelta(DL.add, 1) : null].filter(Boolean);
    const dHP = deltas.reduce((s, x) => s + x.hp, 0), dArm = deltas.reduce((s, x) => s + x.armor, 0), dInt = deltas.reduce((s, x) => s + x.int, 0), dReg = deltas.reduce((s, x) => s + x.regen, 0);
    let mrRatio = 1; for (const x of deltas) if (x.mr) mrRatio *= x.mrSign > 0 ? (1 - x.mr / 100) : 1 / (1 - x.mr / 100);
    const bkb = deltas.find(x => x.bkb) || (DL.ownBkb && S.D.statdb.sim.item_black_king_bar ? {bkb: 9, bkbmr: S.D.statdb.sim.item_black_king_bar.bkbmr} : null);
    const ev = d.hits.map(x => ({t: x[0], src: x[1], inf: x[2], ty: x[3], v: x[4]})).concat(d.heals.map(x => ({t: x[0], src: x[1], inf: x[2], heal: x[3]}))).sort((a, b) => a.t - b.t);
    let cur = hp0, sim = Math.max(1, hp0 + dHP * (hp0 / mhp)), simDeath = null, lastT = t0, cache = {};
    const st = t => { const k = Math.floor(t); return cache[k] || (cache[k] = derived(h, t)); };
    const series = [[t0, cur, sim]];
    for (const e of ev){
      sim += dReg * (e.t - lastT); lastT = e.t;
      if (e.heal){ cur = Math.min(mhp, cur + e.heal); sim = Math.min(mhp + dHP, sim + e.heal); }
      else {
        const s = st(e.t); let mult = 1, nm = 1;
        if (s && e.ty === "p"){ mult = 1 - s.pr; const a2 = s.armor + dArm; nm = 1 - .06 * a2 / (1 + .06 * Math.abs(a2)); }
        else if (s && e.ty === "m"){
          mult = 1 - s.mr; const baseOld = 1 - (s.src.mr[0][1] / 100 + h.st.intT[idx(e.t)] * .001), baseNew = 1 - (s.src.mr[0][1] / 100 + (h.st.intT[idx(e.t)] + dInt) * .001);
          nm = mult * (baseNew / baseOld) * mrRatio;
          if (bkb && e.t <= t0 + bkb.bkb) nm *= (1 - bkb.bkbmr / 100);
        }
        const raw = e.v / Math.max(.02, mult); e.raw = raw; e.sim = raw * nm;
        if (bkb && isBlockedDot(h, e, t0, t0 + bkb.bkb)){ e.sim = 0; e.blocked = true; }
        cur -= e.v; sim -= e.sim;
        if (sim <= 0 && simDeath == null) simDeath = e.t;
      }
      e.hpAfter = cur; series.push([e.t, cur, sim]);
    }
    return {h, t0, T, mhp, hp0, hpEntity, ev, series, simDeath, simEnd: sim, dHP, dArm, dInt, mrRatio, bkb, any: deltas.length > 0 || !!DL.ownBkb};
  }
  function groupHits(ev){
    const rows = [];
    for (const e of ev){
      const last = rows[rows.length - 1];
      if (last && !last.heal === !e.heal && last.src === e.src && last.inf === e.inf && e.t - last.t1 <= 1){ last.n++; last.v += e.heal || e.v; last.t1 = e.t; last.hpAfter = e.hpAfter; continue; }
      rows.push({t: e.t, t1: e.t, src: e.src, inf: e.inf, ty: e.ty, heal: !!e.heal, v: e.heal || e.v, n: 1, hpAfter: e.hpAfter});
    }
    return rows;
  }
  function renderDeathLab(d){
    if (!d) return;
    DL.d = d; const R = runSim(d), h = R.h, tot = d.hits.reduce((s, x) => s + x[4], 0);
    const byType = {p: 0, m: 0, u: 0}; for (const x of d.hits) byType[x[3]] += x[4];
    const bySrc = {}; for (const x of d.hits){ const k = x[1]; (bySrc[k] = bySrc[k] || {p: 0, m: 0, u: 0, t: 0})[x[3]] += x[4]; bySrc[k].t += x[4]; }
    const byAb = {}; for (const x of d.hits){ const k = x[1] + "|" + x[2]; byAb[k] = (byAb[k] || 0) + x[4]; }
    const srcRows = Object.entries(bySrc).sort((a, b) => b[1].t - a[1].t);
    const abRows = Object.entries(byAb).sort((a, b) => b[1] - a[1]).slice(0, 8);
    const heals = d.heals.reduce((s, x) => s + x[3], 0);
    $("dl-head").innerHTML = `<img src="${portSrc(h.id)}" width="44" height="44" alt="" style="--c:${h.color}"><div><h3>${esc(h.name)} ตายที่ <span class="num">${clk(d.t)}</span></h3>
      <span class="note">ปิดคิลโดย <b>${esc(nameOf(d.killer))}</b> · ไฟต์เริ่มโดนดาเมจที่ ${clk(R.t0)} (${(R.T - R.t0).toFixed(1)} วินาทีก่อนตาย) · HP ตอนเริ่ม ≈ ${fmt(R.hp0)} / ${fmt(R.mhp)}${Math.abs(R.hp0 - R.hpEntity) > 40 ? ` <span title="ค่า HP จาก entity ในวินาทีก่อนไฟต์คือ ${fmt(R.hpEntity)} ส่วนต่างมาจาก regen หรือดาเมจที่เกิดก่อนช่วงที่นับ">(ⓘ)</span>` : ""} · ดาเมจรวม ${fmt(tot)}${heals ? ` · ฮีลระหว่างไฟต์ ${fmt(heals)}` : ""}</span></div>`;
    const tb = `<div class="dl-typebar">${["p", "m", "u"].filter(k => byType[k]).map(k => `<span class="ty-${k}" style="flex:${byType[k]}" title="${TYPE_LAB[k]} ${fmt(byType[k])}">${TYPE_LAB[k]} ${Math.round(byType[k] / tot * 100)}%</span>`).join("")}</div>`;
    const mx = srcRows.length ? srcRows[0][1].t : 1;
    $("dl-src").innerHTML = tb + `<div class="lab">ใครทำดาเมจ</div>` + srcRows.map(([k, v]) => `<div class="dl-row">${srcIcon(k, 20)}<span class="nm">${esc(nameOf(k))}</span><span class="dl-bar" style="width:${v.t / mx * 100}%">${["p", "m", "u"].filter(t => v[t]).map(t => `<i class="ty-${t}" style="flex:${v[t]}"></i>`).join("")}</span><span class="num">${fmt(v.t)}</span></div>`).join("")
      + `<div class="lab">สกิลและไอเทมที่ทำดาเมจมากสุด</div>` + abRows.map(([k, v]) => { const [s, inf] = k.split("|"); return `<div class="dl-row">${infIcon(inf)}<span class="nm">${esc(nameOf(inf))} <span class="note">· ${esc(nameOf(s))}</span></span><span class="num">${fmt(v)}</span></div>`; }).join("");
    $("dl-list").innerHTML = `<table><thead><tr><th>เวลา</th><th>จาก</th><th>สกิล / ไอเทม</th><th>ประเภท</th><th>ดาเมจ</th><th>HP หลังโดน</th></tr></thead><tbody>` +
      groupHits(R.ev).map(r => `<tr class="${r.heal ? "heal" : ""}${r.v >= R.mhp * .08 && !r.heal ? " big" : ""}"><td class="num">${clk(r.t)}<span class="rel"> ${(r.t - R.T).toFixed(1)}s</span></td><td>${srcIcon(r.src, 18)} ${esc(nameOf(r.src))}</td><td>${infIcon(r.inf)} ${esc(nameOf(r.inf))}${r.n > 1 ? ` <span class="note">×${r.n}</span>` : ""}</td><td>${r.heal ? `<span class="chip heal">ฮีล</span>` : `<span class="chip ty-${r.ty}">${TYPE_LAB[r.ty]}</span>`}</td><td class="num">${r.heal ? "+" : "−"}${fmt(r.v)}</td><td class="num">${Math.max(0, Math.round(r.hpAfter / R.mhp * 100))}%</td></tr>`).join("") + `</tbody></table>`;
    // selects
    const inv = invAt(h, R.t0).filter(x => x[0] <= 5 && S.D.statdb.sim[x[1]]);
    const remSel = $("dl-rem"), addSel = $("dl-add");
    remSel.innerHTML = `<option value="">ไม่ถอด</option>` + inv.map(x => `<option value="${x[1]}"${DL.rem === x[1] ? " selected" : ""}>${esc(nameOf(x[1]))} (${simDesc(x[1])})</option>`).join("");
    if (!inv.some(x => x[1] === DL.rem)) DL.rem = "";
    if (!addSel.dataset.filled){ addSel.dataset.filled = "1";
      const list = Object.entries(S.D.statdb.sim).filter(([, e]) => e.cost >= 900).sort((a, b) => a[1].cost - b[1].cost);
      addSel.innerHTML = `<option value="">ไม่เพิ่ม</option>` + list.map(([k, e]) => `<option value="${k}">${esc(e.name)} · ${e.cost} ทอง · ${simDesc(k)}</option>`).join("");
    }
    addSel.value = DL.add;
    const hasBkb = invAt(h, R.t0).some(x => x[0] <= 5 && x[1] === "item_black_king_bar");
    $("dl-ownbkb-wrap").hidden = !hasBkb; if (!hasBkb) DL.ownBkb = false; $("dl-ownbkb").checked = DL.ownBkb;
    renderSimResult(R);
    const A = retreat(R, d, DL.X), sm = retreatSummary(A);
    $("dl-xv").textContent = DL.X + " วินาที";
    $("dl-retres").innerHTML = `<div class="verdict ${A.late != null && A.late > .5 ? "no" : "ok"}">${esc(sm.head)}</div>` +
      `<ul class="evid">${A.ev.map(e => `<li class="w${e.w || 0}">${esc(e.txt)}</li>`).join("")}</ul>` +
      `<p class="note">วิธีคิด: ใช้ DPS ที่โดนจริงใน 3 วินาทีล่าสุด (ไม่ต่ำกว่าค่าเฉลี่ยของไฟต์) × เวลาที่ต้องใช้หนี (${DL.X} วินาที) เป็น HP ที่ต้องเหลือไว้ จุดถอยสุดท้ายคือเวลาสุดท้ายที่ HP ยังมากกว่าค่านี้ ส่วน "เริ่มถอยจริง" ดูจากตำแหน่งที่วิ่งออกจากกลุ่มศัตรูที่ตีอยู่ ≥150 หน่วย/วินาที ต่อเนื่อง 2 วินาที ข้อที่ตัวหนาคือปัจจัยที่น่าจะทำให้ถอยช้า เป็นการตีความจากข้อมูล ไม่ใช่สิ่งที่ผู้เล่นคิดจริง</p>`;
    drawDeathChart(R, d, A);
  }
  /* ---------- BKB detail + retreat analysis ---------- */
  function ccSourceAbility(mod, src, a){
    const sh = S.D.byId[src]; if (!sh) return null;
    const stem = mod.replace(/^modifier_/, "");
    const byName = sh.abil.find(x => stem.startsWith(x.n) || stem.includes(x.n.replace(slug(src) + "_", "")));
    if (byName) return byName.n;
    const cast = [...sh.casts].reverse().find(c => c[0] <= a + .05 && a - c[0] <= 1.5);
    return cast ? cast[1] : null;
  }
  function bkbBlocks(R, d, bkb){
    if (!bkb) return {list: [], freed: 0};
    const h = R.h, s0 = R.t0, s1 = R.t0 + bkb.bkb, list = [];
    for (const [a, b, n, src] of h.mods){
      const sh = S.D.byId[src]; if (!sh || sh.team === h.team || a < s0 - .05 || a > s1 || !isControlEffect(patchData, n)) continue;
      const ab = ccSourceAbility(n, src, a), pierce = ab ? (S.D.statdb.abpierce[ab] || S.D.statdb.pierce[ab] || 0) : 0;
      if (b - a >= .1) list.push({mod: n, src, ab, a, b, dur: b - a, pierce, known: !!ab});
    }
    return {list, freed: list.filter(x => !x.pierce).reduce((s, x) => s + x.dur, 0)};
  }
  function isBlockedDot(h, e, s0, s1){
    // damage that comes from a debuff (DoT) of a non-piercing ability, applied while debuff immune
    if (e.t < s0 || e.t > s1 || e.ty === "u") return false;
    const ab = e.inf, stem = ab.replace(/^item_/, "");
    if ((S.D.statdb.pierce[ab] || 0) === 1) return false;
    return h.mods.some(([a, b, n, src]) => src === e.src && a >= s0 - .05 && a <= e.t && b >= e.t - .05 && n.includes(stem) && !/aura|immolation|flame_guard/.test(n));
  }
  function hpAt(R, t){ let v = R.series[0][1]; for (const p of R.series){ if (p[0] <= t) v = p[1]; else break; } return v; }
  function retreat(R, d, X){
    const h = R.h, enemies = Object.keys(d.seen || {}).map(k => S.D.byId[k]).filter(Boolean);
    const allies = S.D.heroes.filter(x => x.team === h.team && x.id !== h.id);
    const tot = d.hits.reduce((s, x) => s + x[4], 0), avg = tot / Math.max(1, R.T - R.t0);
    const dps = t => { let s = 0; for (const e of R.ev) if (!e.heal && e.t > t - 3 && e.t <= t) s += e.v; return Math.max(avg, s / 3); };
    const dist = (p, q) => Math.hypot(p[0] - q[0], p[1] - q[1]);
    const centroid = t => { const ps = enemies.map(e => posAt(e, t)); return ps.length ? [ps.reduce((s, p) => s + p[0], 0) / ps.length, ps.reduce((s, p) => s + p[1], 0) / ps.length] : null; };
    // when did the hero actually start moving away from the attackers (≥150 units/s, two seconds in a row)
    let retreatStart = null, run = 0;
    for (let t = Math.floor(R.t0) - 6; t < R.T; t++){
      const p = posAt(h, t), q = posAt(h, t + 1), c = centroid(t); if (!c) break;
      const away = ((q[0] - p[0]) * (p[0] - c[0]) + (q[1] - p[1]) * (p[1] - c[1])) / Math.max(1, dist(p, c));
      run = away > 150 ? run + 1 : 0;
      if (run >= 2){ retreatStart = t - 1; break; }
    }
    // last moment where current HP still covered the damage that would land during X seconds of escaping
    const buf = []; let tStar = null;
    for (let t = R.t0; t <= R.T + 1e-6; t += .25){ const need = dps(t) * X, hp = hpAt(R, t); buf.push([t, need]); if (hp > need) tStar = t; }
    const late = tStar == null ? null : (retreatStart != null ? retreatStart - tStar : R.T - tStar);
    // evidence
    const ev = [], at = tStar != null ? tStar : R.t0, i = idx(at);
    const pct = v => Math.round(v / R.mhp * 100) + "%";
    if (tStar != null) ev.push({k: "time", txt: `จุดถอยสุดท้ายประมาณ ${clk(tStar)} (${(tStar - R.T).toFixed(1)}s) ตอนนั้น HP ${pct(hpAt(R, tStar))}` });
    ev.push({k: "time", txt: retreatStart != null ? `เริ่มถอยจริง ${clk(retreatStart)} (${(retreatStart - R.T).toFixed(1)}s) HP ${pct(hpAt(R, retreatStart))}` : "ไม่พบช่วงที่วิ่งออกจากศัตรูต่อเนื่อง 2 วินาทีก่อนตาย"});
    const lo = Math.min(at, retreatStart != null ? retreatStart : R.T) - .5, hi = retreatStart != null ? Math.max(retreatStart, at) : R.T;
    const ccs = h.mods.filter(([a, b, n, src]) => { const sh = S.D.byId[src]; return sh && sh.team !== h.team && isControlEffect(patchData, n) && b > lo && a < hi; });
    const ccTime = ccs.reduce((s, [a, b]) => s + Math.min(b, hi) - Math.max(a, lo), 0);
    if (ccs.length) ev.push({k: "cc", w: ccTime >= 1 ? 3 : 1, txt: `ติด CC รวม ${ccTime.toFixed(1)} วินาทีในช่วงที่ควรถอย: ${[...new Set(ccs.map(c => prettyMod(c[2], c[3])))].join(", ")}`});
    for (const [k, [first, seen]] of Object.entries(d.seen || {})){
      const nm = nameOf(k);
      if (seen == null) ev.push({k: "fog", w: 3, txt: `${nm} ไม่ปรากฏในสายตาทีมเลยก่อนโจมตีครั้งแรก`});
      else if (first - seen < 2) ev.push({k: "fog", w: 2, txt: `${nm} โผล่จาก fog แค่ ${(first - seen).toFixed(1)} วินาทีก่อนโจมตี`});
      else ev.push({k: "seen", w: 0, txt: `ทีมมองเห็น ${nm} มาแล้ว ${(first - seen).toFixed(1)} วินาทีก่อนโดนตี`});
    }
    const P = posAt(h, at), nearE = S.D.heroes.filter(x => x.team !== h.team && x.alive[i] && dist(posAt(x, at), P) < 1200), nearA = allies.filter(x => x.alive[i] && dist(posAt(x, at), P) < 1200);
    ev.push({k: "num", w: nearE.length - nearA.length >= 2 ? 2 : 0, txt: `ในระยะ 1,200: ศัตรู ${nearE.length} (${nearE.map(x => x.name).join(", ") || "-"}) · เพื่อน ${nearA.length} (${nearA.map(x => x.name).join(", ") || "-"})`});
    if (d.out > 0) ev.push({k: "fight", w: 1, txt: `กำลังสู้อยู่: ทำดาเมจใส่ฮีโร่ศัตรู ${fmt(d.out)} ในไฟต์นี้`});
    else ev.push({k: "fight", w: 0, txt: "ไม่ได้ทำดาเมจใส่ฮีโร่ศัตรูเลยในไฟต์นี้"});
    const used = h.casts.filter(c => c[0] >= R.t0 - 2 && c[0] <= R.T).map(c => nameOf(c[1]));
    if (used.length) ev.push({k: "cast", w: 0, txt: `ใช้ระหว่างไฟต์: ${[...new Set(used)].join(", ")}`});
    // escape tools at the decision point
    const tools = [];
    for (const a of h.abil) if (isMobilityAbility(patchData, h.id, a.n) && a.lv[i] > 0) tools.push([nameOf(a.n), a.cd[i], a.mc[i] > h.mp[i]]);
    for (const [s, n] of invAt(h, at)) if ((s <= 5 || s === 15) && isMobilityItem(patchData, n)){ const v = h.icd[s] && h.icd[s][Math.floor(at)]; tools.push([nameOf(n), v ? v[0] : 0, false]); }
    if (tools.length){
      const TP = /Town Portal|Boots of Travel/, ready = tools.filter(x => !x[1] && !x[2] && !TP.test(x[0])), usedNames = new Set(used);
      ev.push({k: "tool", w: ready.some(x => !usedNames.has(x[0])) ? 2 : 0, txt: "ตัวช่วยหนีตอนนั้น: " + tools.map(([n, cd, nm]) => `${n} ${cd ? `ติด cd ${Math.ceil(cd)}s` : nm ? "มานาไม่พอ" : (usedNames.has(n) ? "พร้อม (ใช้แล้ว)" : TP.test(n) ? "พร้อม (ต้องร่าย 3 วินาที ใช้หนีกลางไฟต์ยาก)" : "พร้อมแต่ไม่ได้ใช้")}`).join(" · ")});
    }
    const ms = derived(h, at), fast = enemies.map(e => [e, derived(e, at)]).filter(x => x[1]).sort((a, b) => b[1].ms - a[1].ms)[0];
    if (ms && fast) ev.push({k: "ms", w: fast[1].ms > ms.ms + 15 ? 1 : 0, txt: `ความเร็ววิ่ง ≈ ${Math.round(ms.ms)} เทียบกับ ${fast[0].name} ≈ ${Math.round(fast[1].ms)}`});
    return {tStar, retreatStart, late, buf, ev, X};
  }
  function retreatSummary(A){
    const top = A.ev.filter(e => e.w >= 2).map(e => e.txt);
    let head;
    if (A.tStar == null) head = "ตั้งแต่ดาเมจแรก HP ก็ไม่พอให้หนีทันตามสมมติฐานนี้แล้ว (โดนเก็บแบบหนีไม่ทัน)";
    else if (A.late != null && A.late > .5) head = `ถอยช้าไปประมาณ ${A.late.toFixed(1)} วินาที ถ้าเริ่มถอยก่อน ${clk(A.tStar)} มีโอกาสรอดตามสมมติฐานนี้`;
    else head = "เริ่มถอยทันเวลาตามสมมติฐานนี้ การตายน่าจะมาจากปัจจัยอื่น เช่น โดนไล่ทันหรือโดนสตันระหว่างหนี";
    return {head, top};
  }

  function renderSimResult(R){
    const box = $("dl-res");
    if (!R.any){ box.innerHTML = `<p class="note">เลือกไอเทมที่จะถอดหรือเพิ่ม แล้วระบบจะคำนวณดาเมจชุดเดิมใหม่ด้วยค่า HP, เกราะ และ magic resistance ใหม่</p>`; return; }
    const cost = (DL.add ? S.D.statdb.sim[DL.add].cost : 0) - (DL.rem ? S.D.statdb.sim[DL.rem].cost : 0);
    const verdict = R.simDeath == null
      ? `<div class="verdict ok">รอด · เหลือ HP ประมาณ ${fmt(R.simEnd)} (${Math.round(R.simEnd / (R.mhp + R.dHP) * 100)}%) ตอนที่เดิมตาย</div>`
      : `<div class="verdict ${R.simDeath > R.T - .05 ? "ok" : "no"}">ยังตาย ${R.simDeath >= R.T - .05 ? "ในจังหวะเดิม" : `เร็วขึ้น ${(R.T - R.simDeath).toFixed(1)} วินาที`}</div>`;
    const extra = R.simDeath == null ? "" : (R.simDeath < R.T - .05 ? "" : `<p class="note">ดาเมจชุดเดิมยังมากกว่า HP ที่เพิ่มมา ต้องลดดาเมจลงอีกประมาณ ${fmt(-R.simEnd)} ถึงจะรอด</p>`);
    let bk = "";
    if (R.bkb){
      const B = bkbBlocks(R, DL.d, R.bkb), blockedDmg = R.ev.filter(e => e.blocked).reduce((s, e) => s + e.v, 0);
      bk = `<div class="bkbbox"><b>BKB ${R.bkb.bkb} วินาที (${clk(R.t0)}–${clk(R.t0 + R.bkb.bkb)})</b> ดาเมจเวท ×${(1 - R.bkb.bkbmr / 100).toFixed(2)}${blockedDmg ? ` · ดีบัฟ DoT ที่ไม่ทะลุโดนกันทั้งหมด ${fmt(blockedDmg)}` : ""}<br>` +
        (B.list.length ? B.list.map(x => `<span class="chip ${x.pierce ? "ty-u" : "heal"}">${x.pierce ? "ทะลุ BKB" : "กันได้"}</span> ${esc(x.ab ? nameOf(x.ab) : prettyMod(x.mod, x.src))} ${x.dur.toFixed(1)}s ${x.known ? "" : "(ระบุสกิลไม่ได้)"}`).join("<br>") + `<br>เวลาที่ไม่ต้องติด CC เพิ่ม ${B.freed.toFixed(1)} วินาที ซึ่งในเกมจริงอาจใช้เดินหนีหรือสู้กลับได้ แต่การจำลองนี้ยังไม่ได้คิดผลตรงนั้น` : "ไม่มี CC จากศัตรูในช่วงที่ BKB เปิด") + `</div>`;
    }
    box.innerHTML = verdict + extra + bk + `<p class="note">HP สูงสุด ${R.dHP >= 0 ? "+" : ""}${Math.round(R.dHP)} · เกราะ ${R.dArm >= 0 ? "+" : ""}${R.dArm.toFixed(1)} · ดาเมจเวทที่รับ ×${R.mrRatio.toFixed(2)}${R.dInt ? ` · INT ${R.dInt > 0 ? "+" : ""}${R.dInt}` : ""}${R.bkb ? ` · สมมติว่าเปิด BKB ตั้งแต่ ${clk(R.t0)} นาน ${R.bkb.bkb} วินาที` : ""} · ต่างกัน ${cost >= 0 ? "+" : ""}${fmt(cost)} ทอง</p>`;
  }
  function drawDeathChart(R, d, A){
    const cvd = $("dl-chart"), w = cvd.clientWidth, hgt = cvd.clientHeight, dp = window.devicePixelRatio || 1;
    cvd.width = Math.round(w * dp); cvd.height = Math.round(hgt * dp);
    const c = cvd.getContext("2d"); c.setTransform(dp, 0, 0, dp, 0, 0); c.clearRect(0, 0, w, hgt);
    const L = 44, Rm = 10, Tm = 14, B = 22, t0 = R.t0 - .5, t1 = R.T + .5, ymax = Math.max(R.mhp, R.mhp + R.dHP);
    const x = t => L + (t - t0) / (t1 - t0) * (w - L - Rm), y = v => Tm + (1 - Math.max(0, v) / ymax) * (hgt - Tm - B);
    const muted = css("--muted"), grid = css("--grid"), ink = css("--ink");
    c.font = `11px ${css("--f-mono")}`; c.fillStyle = muted; c.textAlign = "right";
    for (const f of [0, .25, .5, .75, 1]){ const yy = y(ymax * f); c.fillStyle = grid; c.fillRect(L, yy, w - L - Rm, 1); c.fillStyle = muted; c.fillText(Math.round(ymax * f), L - 6, yy + 4); }
    c.textAlign = "center"; for (let t = Math.ceil(t0); t <= t1; t += Math.max(1, Math.round((t1 - t0) / 8))) c.fillText((t - R.T).toFixed(0) + "s", x(t), hgt - 6);
    // CC bands on the victim
    const h = R.h;
    for (const [a, b, n, src] of h.mods){
      const sh = S.D.byId[src]; if (!sh || sh.team === h.team || b < t0 || a > t1 || !isControlEffect(patchData, n)) continue;
      c.fillStyle = "rgba(225,75,75,.14)"; c.fillRect(x(Math.max(a, t0)), Tm, x(Math.min(b, t1)) - x(Math.max(a, t0)), hgt - Tm - B);
      c.fillStyle = "#c63c3b"; c.textAlign = "left"; c.fillText(prettyMod(n, src), x(Math.max(a, t0)) + 3, Tm + 11);
    }
    const line = (k, col, dash) => { c.beginPath(); c.setLineDash(dash || []); R.series.forEach((p, j) => { const xx = x(p[0]), yy = y(p[k]); if (j){ c.lineTo(xx, y(R.series[j - 1][k])); } j ? c.lineTo(xx, yy) : c.moveTo(xx, yy); }); c.strokeStyle = col; c.lineWidth = 2.2; c.stroke(); c.setLineDash([]); };
    if (R.any) line(2, css("--good"), [6, 4]);
    line(1, css("--bad"));
    // label big hits
    c.textAlign = "center";
    for (const e of R.ev) if (!e.heal && e.v >= R.mhp * .08){ const xx = x(e.t), yy = y(e.hpAfter); c.fillStyle = ink; c.beginPath(); c.arc(xx, yy, 3, 0, 7); c.fill(); c.fillStyle = css("--ink-2"); c.fillText(`${nameOf(e.inf)} −${Math.round(e.v)}`, Math.min(w - 60, Math.max(L + 50, xx)), Math.max(Tm + 22, yy - 8)); }
    if (A){
      c.beginPath(); A.buf.forEach((p, j) => { const xx = x(p[0]), yy = y(Math.min(ymax, p[1])); j ? c.lineTo(xx, yy) : c.moveTo(xx, yy); });
      c.strokeStyle = "#e0a030"; c.setLineDash([2, 3]); c.lineWidth = 1.6; c.stroke(); c.setLineDash([]);
      const mark = (t, col, lab) => { if (t == null || t < t0 || t > t1) return; c.fillStyle = col; c.fillRect(x(t) - 1, Tm, 2, hgt - Tm - B); c.textAlign = "left"; c.fillText(translate(lab), x(t) + 4, Tm + 24); };
      mark(A.tStar, "#e0a030", "ควรถอย"); mark(A.retreatStart, "#4a7fe0", "ถอยจริง");
    }
    c.strokeStyle = ink; c.setLineDash([3, 3]); c.beginPath(); c.moveTo(x(R.T), Tm); c.lineTo(x(R.T), hgt - B); c.stroke(); c.setLineDash([]);
    c.fillStyle = ink; c.textAlign = "right"; c.fillText(translate("ตาย " + clk(R.T)), x(R.T) - 4, hgt - B - 4);
    $("dl-legend").innerHTML = `<span><i style="background:var(--bad)"></i>HP จริง (คำนวณจาก combat log)</span>${R.any ? `<span><i class="dash" style="background:var(--good)"></i>HP ในสถานการณ์จำลอง</span>` : ""}<span><i style="background:rgba(225,75,75,.3)"></i>ช่วงที่โดน CC</span>${A ? `<span><i style="background:#e0a030;height:2px"></i>HP ที่ต้องเหลือไว้เพื่อหนีทัน (เส้นประ)</span><span><i style="background:#e0a030"></i>จุดถอยสุดท้าย</span><span><i style="background:#4a7fe0"></i>เริ่มถอยจริง</span>` : ""}`;
  }

  /* ---------- world layer: 15 Hz heroes, creeps, camps, runes, Roshan, Tormentor, wards, buildings, fog ---------- */
  const RUNE_COL = {"Bounty": "#e8b84a", "Water": "#5fb8ff", "Wisdom": "#b07cff", "Double Damage": "#4aa3ff", "Haste": "#ff5a4a", "Illusion": "#ffd24a",
    "Invisibility": "#b9a6ff", "Regeneration": "#6fe08a", "Arcane": "#ff6ad5", "Shield": "#8fd3ff"};
  function decodeWorld(W){
    const cum = a => { const o = new Float64Array(a.length); let s = 0; for (let i = 0; i < a.length; i++){ s += a[i]; o[i] = s; } return o; };
    for (const id in W.heroes){
      const h = W.heroes[id], kt = cum(h.t);
      h.T = kt.map(v => W.t0 + v / 30); h.X = cum(h.x).map(v => v * W.qh); h.Y = cum(h.y).map(v => v * W.qh);
      h.B = new Uint8Array(h.T.length); for (const i of h.brk) h.B[i] = 1;
    }
    for (const tr of W.tracks){
      const kt = cum(tr.p[0]), kx = cum(tr.p[1]), ky = cum(tr.p[2]), n = Math.round((tr.t1 - tr.t0) * 2) + 1;
      tr.X = new Float32Array(n); tr.Y = new Float32Array(n);
      let j = 0;
      for (let i = 0; i < n; i++){
        while (j + 1 < kt.length && kt[j + 1] <= i) j++;
        const k2 = Math.min(kt.length - 1, j + 1), f = kt[k2] > kt[j] ? Math.min(1, (i - kt[j]) / (kt[k2] - kt[j])) : 0;
        tr.X[i] = (kx[j] + (kx[k2] - kx[j]) * f) * W.qu; tr.Y[i] = (ky[j] + (ky[k2] - ky[j]) * f) * W.qu;
      }
    }
    W.tracks.sort((p, q) => p.t0 - q.t0);
    return W;
  }
  const changeAt = (list, t, def) => { let v = def; if (!list) return v; for (const [tt, x] of list){ if (tt <= t) v = x; else break; } return v; };
  function keyIndex(T, t){ let lo = 0, hi = T.length - 1; if (t <= T[0]) return 0; if (t >= T[hi]) return hi; while (hi - lo > 1){ const m = (lo + hi) >> 1; if (T[m] <= t) lo = m; else hi = m; } return lo; }
  function wHeroPos(id, t){
    const W = S.W, h = W && W.heroes[id]; if (!h || !h.T.length) return null;
    const j = keyIndex(h.T, t), k = j + 1;
    if (k >= h.T.length || h.B[k] || t <= h.T[j]) return [h.X[j], h.Y[j]];
    const f = (t - h.T[j]) / (h.T[k] - h.T[j]);
    return [h.X[j] + (h.X[k] - h.X[j]) * f, h.Y[j] + (h.Y[k] - h.Y[j]) * f];
  }
  function trackPos(tr, t){
    const f = (t - tr.t0) * 2, i = Math.max(0, Math.min(tr.X.length - 1, Math.floor(f))), j = Math.min(tr.X.length - 1, i + 1), a = Math.max(0, Math.min(1, f - Math.floor(f)));
    return [tr.X[i] + (tr.X[j] - tr.X[i]) * a, tr.Y[i] + (tr.Y[j] - tr.Y[i]) * a];
  }
  // what the chosen side can see: own units always, enemy units only while that team's vision bit is set in the replay
  const fogBit = () => S.fog === "r" ? 1 : S.fog === "d" ? 2 : 0;
  const fogTeam = () => S.fog === "r" ? 2 : S.fog === "d" ? 3 : 0;
  function seenBy(team, visList, t){ const b = fogBit(); if (!b || team === fogTeam()) return true; return (changeAt(visList, t, 0) & b) !== 0; }
  function heroSeen(h, t){ if (!fogBit() || h.team === fogTeam()) return true; const w = S.W && S.W.heroes[h.id]; return w ? (changeAt(w.vis, t, 0) & fogBit()) !== 0 : true; }
  const isNight = t => t >= 0 && Math.floor(t / 300) % 2 === 1;
  function drawFog(t){
    if (!fogBit() || !S.W) return;
    const team = fogTeam(), night = isNight(t);
    if (!S.fogCv){ S.fogCv = document.createElement("canvas"); }
    const fc = S.fogCv; fc.width = cv.width; fc.height = cv.height; const f = fc.getContext("2d");
    f.setTransform(dpr, 0, 0, dpr, 0, 0); f.fillStyle = "rgba(4,8,12,.62)"; f.fillRect(0, 0, W, H);
    f.globalCompositeOperation = "destination-out";
    const pxPerUnit = mapCalibration.a / mapCalibration.size * W * S.view.k;
    const hole = (p, r) => { const [x, y] = scr(p), R = r * pxPerUnit; const g = f.createRadialGradient(x, y, R * .82, x, y, R); g.addColorStop(0, "rgba(0,0,0,1)"); g.addColorStop(1, "rgba(0,0,0,0)"); f.fillStyle = g; f.beginPath(); f.arc(x, y, R, 0, 7); f.fill(); };
    for (const h of S.D.heroes) if (h.team === team && h.alive[idx(t)]) hole(posAt(h, t), night ? 800 : 1800);
    for (const tr of S.W.tracks){
      if (tr.team !== team || t < tr.t0 || t > tr.t1 + .5) continue;
      const r = tr.k === "bld" ? (night ? 800 : 1900) : tr.k === "ward" ? 1600 : tr.k === "lane" ? 750 : 0;
      if (r) hole(trackPos(tr, t), r);
    }
    f.globalCompositeOperation = "source-over";
    ctx.save(); ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.drawImage(fc, 0, 0); ctx.restore();
  }
  function inVision(p, t){
    const team = fogTeam(), night = isNight(t), d = q => Math.hypot(q[0] - p[0], q[1] - p[1]);
    for (const h of S.D.heroes) if (h.team === team && h.alive[idx(t)] && d(posAt(h, t)) < (night ? 800 : 1800)) return true;
    for (const tr of S.W.tracks){ if (tr.team !== team || t < tr.t0 || t > tr.t1 + .5) continue; const r = tr.k === "bld" ? (night ? 800 : 1900) : tr.k === "ward" ? 1600 : tr.k === "lane" ? 750 : 0; if (r && d(trackPos(tr, t)) < r) return true; }
    return false;
  }
  function drawWorld(t){
    if (!S.W) return;
    drawFog(t);
    const L = S.layers, k = Math.max(.8, Math.min(2.2, S.view.k * .9));
    for (const tr of S.W.tracks){
      if (t < tr.t0 || t > tr.t1 + .5 || !L[tr.k] && !(tr.k === "torm" && L.rosh)) continue;
      if (!seenBy(tr.team, tr.vis, t)) continue;
      const [x, y] = scr(trackPos(tr, t)), col = tr.team === 2 ? "#57d16a" : tr.team === 3 ? "#ff5a4f" : "#e8c35a";
      if (tr.k === "lane"){ const s = 2.6 * k; ctx.fillStyle = col; ctx.fillRect(x - s, y - s, 2 * s, 2 * s); ctx.strokeStyle = "rgba(0,0,0,.7)"; ctx.lineWidth = 1; ctx.strokeRect(x - s, y - s, 2 * s, 2 * s); }
      else if (tr.k === "neut"){ ctx.fillStyle = "#e8c35a"; ctx.beginPath(); ctx.arc(x, y, 2.4 * k, 0, 7); ctx.fill(); ctx.strokeStyle = "rgba(0,0,0,.7)"; ctx.lineWidth = 1; ctx.stroke(); }
      else if (tr.k === "ward"){
        const sentry = /TrueSight|sentry/i.test(tr.n || ""); ctx.strokeStyle = sentry ? "#5fb8ff" : "#ffd24a"; ctx.lineWidth = 2;
        ctx.beginPath(); ctx.arc(x, y, 5 * k, 0, 7); ctx.stroke(); ctx.fillStyle = col; ctx.beginPath(); ctx.arc(x, y, 2 * k, 0, 7); ctx.fill();
        S.hits.push({x, y, r: 7, lab: `${sentry ? "Sentry" : "Observer"} ward · ${tr.team === 2 ? "Radiant" : "Dire"} · วางเมื่อ ${clk(tr.t0)}`});
      } else if (tr.k === "bld"){
        const pct = changeAt(tr.hp, t, 100), s = (/fort/i.test(tr.n || "") ? 6 : /rax/i.test(tr.n || "") ? 3.5 : 4.5) * k;
        ctx.fillStyle = col; ctx.globalAlpha = .9; ctx.fillRect(x - s, y - s, 2 * s, 2 * s); ctx.globalAlpha = 1; ctx.strokeStyle = "#000"; ctx.lineWidth = 1.5; ctx.strokeRect(x - s, y - s, 2 * s, 2 * s);
        if (pct < 100){ ctx.fillStyle = "#000"; ctx.fillRect(x - s, y + s + 2, 2 * s, 3); ctx.fillStyle = col; ctx.fillRect(x - s, y + s + 2, 2 * s * pct / 100, 3); }
        S.hits.push({x, y, r: s + 3, lab: `${prettyMod(tr.n || "building")} · HP ${pct}%`});
      } else if (tr.k === "rosh" || tr.k === "torm"){
        const pct = changeAt(tr.hp, t, 100), r = 9 * k;
        ctx.fillStyle = tr.k === "rosh" ? "#6b3fa0" : "#2d8a8a"; ctx.beginPath(); ctx.arc(x, y, r, 0, 7); ctx.fill();
        ctx.strokeStyle = "#000"; ctx.lineWidth = 2; ctx.stroke();
        ctx.strokeStyle = pct < 50 ? "#ff5a4f" : "#5fe08a"; ctx.lineWidth = 2.5; ctx.beginPath(); ctx.arc(x, y, r + 3, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * pct / 100); ctx.stroke();
        label(x, y + 4, tr.k === "rosh" ? "R" : "T", "#fff", "center");
        S.hits.push({x, y, r: r + 4, lab: `${tr.k === "rosh" ? "Roshan" : "Tormentor"} · HP ${pct}%`});
      }
    }
    if (L.rune) for (const r of S.W.runes){
      if (t < r.t0 || t > r.t1) continue;
      if (fogBit() && !inVision([r.x, r.y], t)) continue;
      const [x, y] = scr([r.x, r.y]), s = 6 * k;
      ctx.fillStyle = RUNE_COL[r.type] || "#fff"; ctx.strokeStyle = "#000"; ctx.lineWidth = 1.5;
      ctx.beginPath(); ctx.moveTo(x, y - s); ctx.lineTo(x + s, y); ctx.lineTo(x, y + s); ctx.lineTo(x - s, y); ctx.closePath(); ctx.fill(); ctx.stroke();
      const who = r.by ? nameOf(r.by) : null;
      S.hits.push({x, y, r: s + 3, lab: `รูน ${r.type} · เกิด ${clk(r.t0)} · ${r.out === "bottled" ? "ใส่ขวดโดย " + who : r.out === "picked_up" ? "เก็บโดย " + who : r.out === "denied" ? "deny โดย " + who : "ไม่มีใครเก็บ"} ${r.out !== "still_there" ? "ที่ " + clk(r.t1) : ""}`});
    }
  }

  {
    const D = bundle.match;
    D.statdb = D.statdb || {};
    D.statdb.buffs = patchData.buffs;
    D.statdb.heroes = D.statdb.heroes || {};
    D.statdb.rules = { ...(D.statdb.rules || {}), ...patchData.rules };
    D.byId = {}; for (const h of D.heroes){ D.byId[h.id] = h; h.short = shortOf(h.name);
      const speed = baseAttackSpeed(patchData, h.id);
      if (D.statdb.heroes[h.id] && speed !== null) D.statdb.heroes[h.id].bas = speed;
      const dec = a => { const o = new Float64Array(a.length); let s = 0; for (let k = 0; k < a.length; k++){ s += a[k]; o[k] = s / (D.dtick || 10); } return o; };
      if (h.din){ h.DT = dec(h.din[0]); h.DV = h.din[1]; h.DY = h.din[2]; }
      if (h.hin){ h.HT = dec(h.hin[0]); h.HV = h.hin[1]; } const im = new Image(); im.onload = () => draw(); im.src = portSrc(h.id); S.port[h.id] = im; }
    D.deaths.sort((a, b) => a.t - b.t);
    S.D = D; S.vis = new Set(D.heroes.map(h => h.id)); scrub.min = D.t0; scrub.max = D.t1; status.textContent = "";
    S.range = [D.t0, D.t1]; TL.v0 = D.t0; TL.v1 = D.t1;
    if (hasMap){ const im = new Image(); im.onload = () => { S.img = im; draw(); renderMulti(); }; im.onerror = () => { status.textContent = "โหลดภาพมินิแมปไม่สำเร็จ เส้นทางยังแสดงได้บนพื้นสีเข้ม"; }; im.src = icon("map", "map"); }
    else status.textContent = "ไม่มีภาพมินิแมป เส้นทางแสดงบนพื้นสีเข้ม · No minimap image";
    buildAlerts(); legend(); renderRoster(); renderLists(); renderTicks(); resize(); setT(S.t); tlInit();
    S.W = decodeWorld(bundle.world); $("rv-world-note").textContent = "ตำแหน่งฮีโร่จากข้อมูล 30 ครั้ง/วินาที เก็บเฉพาะจุดที่เปลี่ยนทิศ · ยูนิตอื่น 2 ครั้ง/วินาที"; draw();
    for (const box of [$("rv-cards-r"), $("rv-cards-d")]){
      box.addEventListener("mouseover", ev => {
        const ic = ev.target.closest(".ic.ab[data-ab]"), card = ev.target.closest(".rc"); if (!card) return;
        const h = S.D.heroes.find(x => "card-" + x.pid === card.id), next = ic && h ? {id: h.id, k: +ic.dataset.ab} : null;
        if (JSON.stringify(next) !== JSON.stringify(S.hoverAb || null)){ S.hoverAb = next; renderCards(true); }
      });
      box.addEventListener("mouseleave", () => { if (S.hoverAb){ S.hoverAb = null; renderCards(true); } });
    }
    $("dl-rem").addEventListener("change", e => { DL.rem = e.target.value; renderDeathLab(DL.d); });
    $("dl-add").addEventListener("change", e => { DL.add = e.target.value; renderDeathLab(DL.d); });
    $("dl-ownbkb").addEventListener("change", e => { DL.ownBkb = e.target.checked; renderDeathLab(DL.d); });
    $("dl-x").addEventListener("input", e => { DL.X = +e.target.value; renderDeathLab(DL.d); });
    new ResizeObserver(() => DL.d && renderDeathLab(DL.d)).observe($("dl"));
    if (D.deaths[0]) renderDeathLab(D.deaths[0]);
  }

  return () => {
    alive = false;
    S.playing = false;
    for (const off of cleanups.splice(0)) off();
    root.replaceChildren();
  };
}
