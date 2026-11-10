/* Standalone design fixture. No device, transport, production entitlement or game APIs. */
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const state = { scenario: 'waiting', pro: false, selected: null, inline: false, tab: 'summary' };
  const icon = name => `<svg class="icon" aria-hidden="true"><use href="#i-${name}"/></svg>`;
  const announce = text => { $('board-status').textContent = text; };
  const hasRoster = () => state.scenario === 'live' || state.scenario === 'stale';
  const rect = id => {
    const r = $(id).getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  };
  function fitStage() {
    const container = $('stage-fit');
    const scale = container.clientWidth / 1420;
    $('app-stage').style.transform = `scale(${scale})`;
    container.style.height = `${760 * scale}px`;
    $('scale-readout').textContent = `Board fit ${(scale * 100).toFixed(1)}%`;
  }
  function makeSlots() {
    for (const [kind, count] of [['enemy', 5], ['ally', 4]]) {
      const host = $(`${kind}-row`);
      host.replaceChildren();
      for (let i = 0; i < count; i++) {
        const button = document.createElement('button');
        button.type = 'button'; button.className = 'slot'; button.id = `${kind}-${i}`;
        button.setAttribute('aria-pressed', 'false');
        button.innerHTML = `<span class="slot-portrait">${icon('user')}</span><span class="slot-text"><strong></strong><small></small></span>`;
        button.addEventListener('click', () => {
          if (!hasRoster()) return;
          state.selected = { kind, index: i }; state.inline = false; state.tab = 'summary';
          renderSlots(); renderRight();
          announce(`เลือก${kind === 'enemy' ? 'ศัตรู' : 'เพื่อน'}ช่อง ${i + 1} ในข้อมูลตัวอย่าง`);
        });
        host.append(button);
      }
    }
  }
  function renderSlots() {
    for (const [kind, count] of [['enemy', 5], ['ally', 4]]) {
      for (let i = 0; i < count; i++) {
        const button = $(`${kind}-${i}`);
        const known = hasRoster(); const live = state.scenario === 'live';
        const label = kind === 'enemy' ? 'ศัตรู' : 'เพื่อน';
        button.disabled = !known;
        button.querySelector('strong').textContent = known ? `${label} ${i + 1}` : 'ยังไม่ทราบ';
        let detail = '—';
        if (known && kind === 'ally') detail = 'ยังไม่แชร์';
        if (known && kind === 'enemy') detail = live ? (i < 2 ? 'พบ · 0.4 วิ' : 'หาย · 6.2 วิ') : 'ข้อมูลเก่า';
        button.querySelector('small').textContent = detail;
        button.title = `${label}ช่อง ${i + 1} · ${known ? 'ตัวอย่างสังเคราะห์' : 'ไม่มีข้อมูล'} · ${detail}`;
        button.setAttribute('aria-label', button.title);
        button.setAttribute('aria-pressed', String(Boolean(state.selected && state.selected.kind === kind && state.selected.index === i)));
        button.classList.toggle('seen', known && live && kind === 'enemy' && i < 2);
        button.classList.toggle('missing', known && live && kind === 'enemy' && i >= 2);
      }
    }
  }
  function closeInspector({ restoreFocus = true } = {}) {
    const selected = state.selected; state.selected = null; state.inline = false;
    renderSlots(); renderRight();
    if (restoreFocus && selected) $(`${selected.kind}-${selected.index}`)?.focus();
  }
  function renderRight() {
    $('summary-tab').classList.toggle('active', state.tab === 'summary');
    $('summary-tab').setAttribute('aria-selected', String(state.tab === 'summary')); $('summary-tab').tabIndex = state.tab === 'summary' ? 0 : -1;
    $('events-tab').classList.toggle('active', state.tab === 'events');
    $('events-tab').setAttribute('aria-selected', String(state.tab === 'events')); $('events-tab').tabIndex = state.tab === 'events' ? 0 : -1; $('right-content').setAttribute('aria-labelledby', state.tab === 'summary' ? 'summary-tab' : 'events-tab');
    const container = $('right-content');
    container.scrollTop = 0;
    if (state.tab === 'events') {
      container.innerHTML = hasRoster() ? `<div class="event-list"><div class="event-item">${icon('eye')}<div><strong>ตัวอย่างเหตุการณ์จากภาพ</strong><p>ตัวอย่างนี้ไม่มีเสียงและไม่อ้างอิงแมตช์จริง</p><small>00:24 · ข้อมูลสังเคราะห์</small></div></div><div class="event-item">${icon('clock')}<div><strong>ตรวจสอบอายุข้อมูล</strong><p>เหตุการณ์ในเครื่องยังเข้าถึงได้ใน Base</p><small>00:21 · ข้อมูลสังเคราะห์</small></div></div></div>` : `<div class="right-neutral">${icon('clock')}<h3>ยังไม่มีเหตุการณ์</h3><p>เหตุการณ์ที่สังเกตได้จะปรากฏที่นี่ โดยไม่ต้องเปิดสิทธิ์เพิ่มเติม</p><span class="neutral-rule"></span></div>`;
      return;
    }
    if (!state.selected) {
      container.innerHTML = `<div class="right-neutral">${icon('user')}<h3>เลือกฮีโร่เพื่อดูสรุป</h3><p>${hasRoster() ? 'เลือกการ์ดตัวอย่างด้านบนหรือด้านล่าง รายละเอียดจะเปิดในพื้นที่นี้' : 'เมื่อมีข้อมูลที่ตรวจสอบได้ เลือกการ์ดเพื่ออ่านรายละเอียด'}</p><span class="neutral-rule"></span></div>`;
      return;
    }
    const { kind, index } = state.selected;
    const title = `${kind === 'enemy' ? 'ศัตรู' : 'เพื่อน'} ${index + 1}`;
    const status = kind === 'ally' ? 'ยังไม่มีการแชร์' : state.scenario === 'live' ? (index < 2 ? 'พบในภาพตัวอย่าง' : 'ไม่อยู่ในภาพตัวอย่าง') : 'ข้อมูลเก่า';
    container.innerHTML = `<div class="inspector-heading"><div><span class="micro-label">${state.pro ? 'PRO FIXTURE' : 'BASE SUMMARY'}</span><h3>${title}</h3></div><button class="icon-button" id="close-inspector" aria-label="ปิดรายละเอียด">${icon('close')}</button></div><p class="inspector-meta">ตัวอย่างสังเคราะห์ · การเลือกโดยผู้ใช้</p><dl class="detail-list"><div><dt>สถานะ</dt><dd>${status}</dd></div><div><dt>แหล่งข้อมูล</dt><dd>${kind === 'ally' ? 'ยังไม่แชร์ข้อมูล' : 'ภาพตัวอย่าง'}</dd></div><div><dt>อายุข้อมูล</dt><dd>${kind === 'ally' ? '—' : state.scenario === 'live' ? (index < 2 ? '0.4 วินาที' : '6.2 วินาที') : '5.2 วินาที'}</dd></div></dl><div class="detail-note">${kind === 'ally' ? 'ไม่มีข้อมูลเพื่อนที่ได้รับอนุญาต จึงไม่แสดงพลังชีวิต สกิล หรือไอเทม' : 'แสดงเฉพาะข้อมูลที่มองเห็น ไม่แสดงพลังชีวิต สกิล หรือไอเทมที่ไม่มีหลักฐาน'}</div>${state.pro ? `<button class="secondary-button inline-toggle" id="inline-toggle" aria-expanded="${state.inline}">ที่มาของข้อมูลตัวอย่าง ${icon('arrow')}</button>${state.inline ? '<div class="inline-detail" id="inline-detail">ข้อมูลชุดนี้ใช้ตรวจรูปแบบการแสดงผล ไม่ใช่ข้อมูลจากเกมจริง และไม่เปิดสิทธิ์ Pro ในแอป กด Escape เพื่อปิดส่วนนี้ก่อนปิด inspector</div>' : ''}` : '<p class="inspector-meta">พื้นที่สรุปและเหตุการณ์ยังใช้ได้ใน Base</p>'}`;
    $('close-inspector').addEventListener('click', () => closeInspector());
    $('inline-toggle')?.addEventListener('click', () => { state.inline = !state.inline; renderRight(); $('inline-toggle').focus(); });
  }
  function renderScenario() {
    const scenario = state.scenario;
    const known = scenario !== 'waiting';
    const live = scenario === 'live';
    const scenarioLabels = { waiting: 'รอเข้าเกม', live: 'ข้อมูลสังเคราะห์ · 24:18', stale: 'ข้อมูลสังเคราะห์ · ข้อมูลเก่า', unavailable: 'ข้อมูลสังเคราะห์ · GSI only' };
    $('session-label').textContent = scenarioLabels[scenario];
    $('own-name').textContent = known ? 'ฮีโร่ของคุณ' : 'ยังไม่มีข้อมูล';
    $('own-meta').textContent = known ? (scenario === 'stale' ? 'ค่าล่าสุด · 5.2 วินาที' : 'เลเวล 12 · ตัวอย่าง') : 'รอข้อมูลจากเกม';
    $('own-values').innerHTML = known ? `<div><span>พลังชีวิต</span><strong>${scenario === 'stale' ? '720 ล่าสุด' : '720 / 1,200'}</strong></div><div class="meter"><i style="width:60%;${scenario === 'stale' ? 'background:var(--kb-color-warning)' : ''}"></i></div><div><span>มานา</span><strong>${scenario === 'stale' ? '62% ล่าสุด' : '62%'}</strong></div>` : '<div><span>พลังชีวิต</span><strong>—</strong></div><div class="meter"><i></i></div><div><span>มานา</span><strong>—</strong></div>';
    const content = {
      waiting: ['ยังไม่มีภาพ', 'neutral', 'รอภาพแผนที่จากเกม', 'เมื่อแหล่งข้อมูลพร้อม จะแสดงภาพที่ตรวจสอบได้ที่นี่', 'แหล่งภาพ: ยังไม่พร้อม', '—'],
      live: ['แบบจัดวาง', 'info', 'แผนที่มีพื้นที่หายใจ', 'ภาพพื้นหลังเป็นตัวอย่างองค์ประกอบ ไม่ใช่ภาพจากเกมจริง', 'แหล่งภาพ: ตัวอย่างสังเคราะห์', '—'],
      stale: ['ข้อมูลเก่า', 'warning', 'ภาพล่าสุดไม่ใช่สถานะปัจจุบัน', 'ไม่แสดงตำแหน่งสดหรือคำยืนยันว่าปลอดภัย เมื่อข้อมูลหมดอายุ', 'แหล่งภาพ: หยุดอัปเดต · ตัวอย่าง', '5.2 วิ'],
      unavailable: ['ภาพไม่พร้อม', 'neutral', 'แผนที่ยังแสดงไม่ได้', 'ข้อมูลตัวเราที่พร้อมยังแสดงได้ ตรวจสอบแหล่งภาพในตั้งค่า', 'แหล่งภาพ: ไม่พร้อม · ตัวอย่าง', '—']
    }[scenario];
    $('map-state').textContent = content[0]; $('map-state').className = `badge ${content[1]}`;
    $('map-title').textContent = content[2]; $('map-description').textContent = content[3];
    $('source-status').textContent = content[4]; $('source-age').textContent = content[5];
    renderSlots(); renderRight();
  }
  function setPro(pro) {
    const hadDetailFocus = $('right-content').contains(document.activeElement);
    const previous = state.selected;
    state.pro = pro; state.selected = null; state.inline = false;
    $('base-mode').classList.toggle('selected', !pro); $('base-mode').setAttribute('aria-pressed', String(!pro));
    $('pro-mode').classList.toggle('selected', pro); $('pro-mode').setAttribute('aria-pressed', String(pro));
    $('authority-label').textContent = pro ? 'Pro fixture' : 'Base';
    renderSlots(); renderRight();
    if (hadDetailFocus && previous) $(`${previous.kind}-${previous.index}`)?.focus();
  }
  $('scenario').addEventListener('change', event => { state.scenario = event.target.value; state.selected = null; state.inline = false; renderScenario(); announce('เปลี่ยนสถานะตัวอย่างแล้ว'); });
  $('base-mode').addEventListener('click', () => setPro(false)); $('pro-mode').addEventListener('click', () => setPro(true));
  $('summary-tab').addEventListener('click', () => { state.tab = 'summary'; renderRight(); });
  $('events-tab').addEventListener('click', () => { state.tab = 'events'; renderRight(); });
  for (const id of ['summary-tab', 'events-tab']) $(id).addEventListener('keydown', event => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const target = event.key === 'Home' ? 'summary' : event.key === 'End' ? 'events' : state.tab === 'summary' ? 'events' : 'summary';
    state.tab = target; renderRight(); $(`${target}-tab`).focus();
  });
  $('contrast-toggle').addEventListener('click', event => { const on = event.currentTarget.getAttribute('aria-pressed') !== 'true'; document.documentElement.dataset.contrast = on ? 'high' : 'normal'; document.documentElement.dataset.kbContrast = on ? 'more' : 'standard'; event.currentTarget.setAttribute('aria-pressed', String(on)); });
  $('motion-toggle').addEventListener('click', event => { const on = event.currentTarget.getAttribute('aria-pressed') !== 'true'; document.documentElement.dataset.motion = on ? 'reduced' : 'normal'; document.documentElement.dataset.kbMotion = on ? 'reduced' : 'normal'; event.currentTarget.setAttribute('aria-pressed', String(on)); });
  document.addEventListener('keydown', event => {
    if (event.key !== 'Escape' || event.target.matches?.('input,select,textarea')) return;
    if (state.inline) { state.inline = false; renderRight(); $('inline-toggle')?.focus(); event.preventDefault(); }
    else if (state.selected) { closeInspector(); event.preventDefault(); }
  });
  const voice = {
    off: ['ไมโครโฟนปิด', 'กดค้างหลังอนุญาต เพื่อถามในเครื่อง'],
    listening: ['กำลังฟัง · ตัวอย่าง', 'ปล่อยปุ่มเมื่อพูดจบ'],
    thinking: ['กำลังคิดในเครื่อง · ตัวอย่าง', 'ไม่ส่งเสียงหรือบทสนทนาไปคลาวด์'],
    interrupted: ['หยุดเพื่อแจ้งเตือน · ตัวอย่าง', 'คำเตือนสำคัญมาก่อนคำตอบ']
  };
  document.querySelectorAll('[data-voice]').forEach(button => button.addEventListener('click', () => { const [name, help] = voice[button.dataset.voice]; $('voice-state').textContent = name; $('voice-help').textContent = help; announce(name); }));
  $('example-primary').addEventListener('click', () => { $('sample-name').focus(); announce('โฟกัสอยู่ที่ช่องชื่อโปรไฟล์ตัวอย่าง'); });
  $('example-secondary').addEventListener('click', () => { $('states').scrollIntoView({ behavior: document.documentElement.dataset.motion === 'reduced' || matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth' }); });
  $('consent-sample').addEventListener('change', event => { $('consent-note').textContent = event.target.checked ? 'เปิดเฉพาะสวิตช์ตัวอย่าง · ไม่มีอุปกรณ์ถูกเปิดและไม่มีข้อมูลถูกส่ง' : 'ตัวอย่างปิดอยู่ · การเปิดไมโครโฟนจริงต้องมีความยินยอมเฉพาะและกด PTT ค้าง'; });
  function download(filename, content, type) {
    const url = URL.createObjectURL(new Blob([content], { type }));
    const link = document.createElement('a'); link.href = url; link.download = filename;
    document.body.append(link); link.click(); link.remove(); URL.revokeObjectURL(url);
  }
  $('download-tokens').addEventListener('click', async () => {
    const data = JSON.parse($('kb-token-data').textContent);
    if (!data.note) { download('kohbor.tokens.json', $('kb-token-data').textContent, 'application/json'); return; }
    try { const response = await fetch('../tokens/kohbor.tokens.json'); if (!response.ok) throw new Error('Tokens missing'); download('kohbor.tokens.json', await response.text(), 'application/json'); }
    catch { announce('ใช้ kohbor-design-board.html เพื่อดาวน์โหลดโทเคนจริงแบบออฟไลน์'); }
  });
  $('download-css').addEventListener('click', async () => {
    const embedded = document.querySelector('style[data-embed="tokens"]');
    if (embedded) { download('kohbor-tokens.css', embedded.textContent, 'text/css'); return; }
    try { const response = await fetch('../tokens/tokens.css'); if (!response.ok) throw new Error('CSS missing'); download('kohbor-tokens.css', await response.text(), 'text/css'); }
    catch { announce('ใช้ kohbor-design-board.html เพื่อดาวน์โหลด CSS แบบออฟไลน์'); }
  });
  makeSlots(); renderScenario(); fitStage();
  new ResizeObserver(fitStage).observe($('stage-fit'));
  /* A read-only snapshot used by manual fixture review; it grants no application capability. */
  Object.defineProperty(window, 'kohborBoardSnapshot', { value: () => Object.freeze({ fixture: true, scenario: state.scenario, proFixture: state.pro, selection: state.selected ? { ...state.selected } : null, inlineOpen: state.inline, tab: state.tab, map: rect('map-region'), stage: rect('app-stage'), enemySlots: $('enemy-row').children.length, allySlots: $('ally-row').children.length }), writable: false });
})();
