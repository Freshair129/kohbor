export const ACCOUNT_STATES = Object.freeze([
  'signed_out',
  'browser_handoff',
  'signing_in',
  'needs_display_name',
  'signed_in_online',
  'signed_in_offline',
  'service_unavailable',
  'session_revoked',
  'paused_new_device',
  'deleting',
]);

const PRIVATE_DATA_STATES = new Set(['signed_in_online', 'signed_in_offline', 'paused_new_device']);

const STATE_LABELS = {
  signed_out: 'ไม่ได้เข้าสู่ระบบ',
  browser_handoff: 'ไปต่อในเบราว์เซอร์',
  signing_in: 'กำลังเข้าสู่ระบบ',
  needs_display_name: 'ตั้งชื่อที่แสดง',
  signed_in_online: 'ออนไลน์ · จำลอง',
  signed_in_offline: 'ออฟไลน์',
  service_unavailable: 'บริการไม่พร้อม',
  session_revoked: 'เซสชันสิ้นสุด',
  paused_new_device: 'พักไว้ · อุปกรณ์ใหม่',
  deleting: 'กำลังลบ',
};

const STATE_COPY = {
  signed_out: ['ยังไม่ได้เข้าสู่ระบบ', 'ฟีเจอร์ในเครื่องยังใช้ได้ตามปกติ'],
  browser_handoff: ['ทำรายการต่อในเบราว์เซอร์', 'หน้าจอนี้เป็นการจำลองและจะไม่เปิดเบราว์เซอร์จริง'],
  signing_in: ['กำลังเข้าสู่ระบบ', 'กำลังจำลองการตรวจสอบบัญชีในเครื่องนี้'],
  needs_display_name: ['ตั้งชื่อที่แสดง', 'ชื่อที่คุณพิมพ์จะใช้ในรายการเพื่อน ไม่ดึงจาก Google'],
  signed_in_online: ['ออนไลน์ · จำลองเท่านั้น', 'เชื่อมต่อกับตัวควบคุมปลอมในหน้านี้ ไม่มีบริการจริง'],
  signed_in_offline: ['ลงชื่อแล้ว · ออฟไลน์', 'เพื่อนและการแชร์หยุดชั่วคราว ทุกอย่างในเครื่องนี้ยังใช้งานได้'],
  service_unavailable: ['บริการบัญชียังไม่พร้อม', 'บริการจริงยังไม่ได้ตั้งค่า ฟีเจอร์ในเครื่องยังใช้งานได้'],
  session_revoked: ['เซสชันสิ้นสุดแล้ว', 'การเข้าสู่ระบบถูกยกเลิกในตัวอย่างนี้; ข้อมูลจริงไม่ได้เชื่อมต่อ'],
  paused_new_device: ['พักการแชร์หลังใช้อุปกรณ์ใหม่', 'ยืนยันอุปกรณ์นี้ก่อนเลือกเปิดการแชร์อีกครั้ง'],
  deleting: ['กำลังลบข้อมูลตัวอย่าง', 'การลบนี้มีผลกับข้อมูลจำลองในหน้านี้เท่านั้น'],
};

const SCREEN_LABELS = {
  account: 'บัญชี',
  friends: 'เพื่อน',
  share: 'รายการแชร์',
};

const FAILURE_COPY = 'ส่งคำขอไม่สำเร็จ ตรวจสอบรหัสหรือลองอีกครั้ง';

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function selected(actual, expected) {
  return actual === expected ? ' aria-current="page"' : '';
}

function disabled(shouldDisable) {
  return shouldDisable ? ' disabled' : '';
}

function statusBanner(state, notice = '') {
  const [title, description] = STATE_COPY[state];
  const tone = state === 'signed_in_online' ? 'online' : (state === 'signed_in_offline' || state === 'paused_new_device' ? 'paused' : 'neutral');
  const transient = notice ? `<p class="s1c-inline-notice" role="status" aria-live="polite">${escapeHtml(notice)}</p>` : '';
  return `<section class="s1c-state-banner s1c-tone-${tone}" aria-label="สถานะบัญชี">
    <span class="s1c-state-dot" aria-hidden="true"></span>
    <div class="s1c-state-copy"><strong>${escapeHtml(title)}</strong><span>${escapeHtml(description)}</span>${transient}</div>
  </section>`;
}

function stateChooser(state, serviceConfigured) {
  const options = ACCOUNT_STATES.map((value) => `<option value="${value}"${value === state ? ' selected' : ''}>${escapeHtml(STATE_LABELS[value])}</option>`).join('');
  return `<div class="s1c-demo-controls">
    <label class="s1c-control-label" for="s1c-state">เลือกสถานะจำลอง</label>
    <select id="s1c-state" data-action="choose-state" aria-label="เลือกสถานะบัญชีจำลอง">${options}</select>
    <label class="s1c-toggle-row"><input type="checkbox" data-action="set-service"${serviceConfigured ? ' checked' : ''}><span>เปิดบริการจำลองในเครื่อง</span></label>
    <p class="s1c-footnote">ข้อมูลและปุ่มทั้งหมดเป็นตัวอย่าง ไม่มีการเรียกเครือข่าย</p>
  </div>`;
}

function screenNav(screen) {
  return `<nav class="s1c-nav" aria-label="หน้าบัญชี">
    ${Object.entries(SCREEN_LABELS).map(([key, label]) => `<button type="button" class="s1c-nav-button${key === screen ? ' is-active' : ''}" data-action="navigate" data-screen="${key}"${selected(key, screen)}><span aria-hidden="true">${key === 'account' ? '◉' : key === 'friends' ? '♧' : '⇄'}</span>${escapeHtml(label)}</button>`).join('')}
  </nav>`;
}

// TODO(legal-review): confirm this Supabase account-data disclosure before shipping.
function signInContent(state, serviceConfigured) {
  switch (state) {
    case 'signed_out':
      return `<section class="s1c-panel s1c-signin-panel">
        <div class="s1c-section-heading"><span class="s1c-eyebrow">บัญชี Kohbor</span><h2>เริ่มใช้งานบัญชี</h2></div>
        <p class="s1c-copy">เข้าสู่ระบบด้วย Google ในเบราว์เซอร์ระบบเมื่อมีบริการจริงให้ใช้งาน</p>
        <button type="button" class="s1c-primary-button" data-action="sign-in"${disabled(!serviceConfigured)}>เข้าสู่ระบบด้วย Google</button>
        ${serviceConfigured ? '<p class="s1c-inline-note">โหมดจำลอง: ปุ่มนี้ไม่เปิดเบราว์เซอร์จริง</p>' : '<p class="s1c-inline-note">บริการยังไม่ตั้งค่า ปุ่มเข้าสู่ระบบจึงปิดอยู่</p>'}
        <div class="s1c-privacy-promise"><strong>ข้อมูลบัญชีที่ Supabase จัดเก็บ</strong><span>เกาะเบาะได้รับอีเมล ชื่อ และรูปโปรไฟล์จาก Google ผ่าน Supabase Auth; โหมดจำลองนี้ไม่ส่งข้อมูลจริง</span></div>
      </section>`;
    case 'browser_handoff':
      return `<section class="s1c-panel s1c-signin-panel">
        <div class="s1c-section-heading"><span class="s1c-eyebrow">ขั้นตอนจำลอง</span><h2>ทำรายการต่อในเบราว์เซอร์</h2></div>
        <p class="s1c-copy">หน้าจอนี้จำลองขั้นตอนในเบราว์เซอร์เท่านั้น ไม่ได้เชื่อมต่อบริการจริง</p>
        <div class="s1c-actions"><button type="button" class="s1c-primary-button" data-action="simulate-browser-return">จำลองการกลับจากเบราว์เซอร์</button><button type="button" data-action="cancel-sign-in">ยกเลิก</button></div>
      </section>`;
    case 'signing_in':
      return `<section class="s1c-panel s1c-signin-panel">
        <div class="s1c-section-heading"><span class="s1c-eyebrow">ขั้นตอนจำลอง</span><h2>กำลังเข้าสู่ระบบ</h2></div>
        <p class="s1c-copy">ไม่มีคำขอเครือข่ายในตัวอย่างนี้ เลือกจำลองผลลัพธ์เพื่อไปต่อ</p>
        <div class="s1c-actions"><button type="button" class="s1c-primary-button" data-action="complete-sign-in">จำลองเข้าสู่ระบบสำเร็จ</button><button type="button" data-action="cancel-sign-in">ยกเลิก</button></div>
      </section>`;
    case 'needs_display_name':
      return `<section class="s1c-panel s1c-signin-panel">
        <div class="s1c-section-heading"><span class="s1c-eyebrow">ขั้นตอนจำลอง</span><h2>ตั้งชื่อที่แสดง</h2></div>
        <p class="s1c-copy">พิมพ์ชื่อที่เพื่อนจะเห็น ชื่อนี้ไม่ได้มาจาก Google</p>
        <label class="s1c-control-label" for="s1c-display-name">ชื่อที่แสดง (1–32 ตัวอักษร)</label>
        <input id="s1c-display-name" name="displayName" data-field="display-name" maxlength="32" autocomplete="nickname" placeholder="เช่น มะลิ">
        <button type="button" class="s1c-primary-button" data-action="save-display-name">บันทึกชื่อที่แสดง</button>
      </section>`;
    default:
      return '';
  }
}

function profileContent(fixture, state, serviceConfigured) {
  if (['signed_out', 'browser_handoff', 'signing_in', 'needs_display_name'].includes(state)) {
    return signInContent(state, serviceConfigured);
  }

  if (state === 'session_revoked') {
    return `<section class="s1c-panel s1c-signin-panel">
      <div class="s1c-section-heading"><span class="s1c-eyebrow">เซสชัน</span><h2>เซสชันสิ้นสุดแล้ว</h2></div>
      <p class="s1c-copy">ไม่มีข้อมูลบัญชีหรือรายชื่อเพื่อนให้ดูในสถานะนี้ เลือกสถานะอื่นจากตัวควบคุมจำลองเพื่อทดสอบต่อ</p>
    </section>`;
  }

  if (state === 'service_unavailable') {
    return `<section class="s1c-panel s1c-signin-panel">
      <div class="s1c-section-heading"><span class="s1c-eyebrow">บริการบัญชี</span><h2>บริการบัญชียังไม่พร้อม</h2></div>
      <p class="s1c-copy">ไม่มีข้อมูลบัญชีที่ยืนยันแล้วในสถานะนี้ ฟีเจอร์ในเครื่องยังใช้งานได้</p>
    </section>`;
  }

  if (state === 'deleting') {
    return `<section class="s1c-panel s1c-danger-panel">
      <div class="s1c-section-heading"><span class="s1c-eyebrow">ตัวอย่างเท่านั้น</span><h2>กำลังลบบัญชี</h2></div>
      <p class="s1c-copy">ยืนยันเพื่อลบชื่อ เพื่อน และรายการแชร์จำลองออกจากหน้านี้ การลบไม่ได้ส่งข้อมูลไปที่บริการใด</p>
      <div class="s1c-actions"><button type="button" class="s1c-danger-button" data-action="confirm-delete">ยืนยันลบข้อมูลตัวอย่าง</button><button type="button" data-action="cancel-delete">ยกเลิก</button></div>
    </section>`;
  }

  const displayName = escapeHtml(fixture.displayName || 'มะลิ');
  const friendCode = escapeHtml(fixture.friendCode || 'KB-DEMO-0001');
  const localOnly = state === 'signed_in_offline' || state === 'service_unavailable' || state === 'session_revoked';
  const requireDeviceConfirmation = state === 'paused_new_device';
  return `<section class="s1c-panel s1c-profile-panel">
    <div class="s1c-section-heading"><span class="s1c-eyebrow">ข้อมูลจำลอง</span><h2>บัญชีของคุณ</h2></div>
    <dl class="s1c-identity-list"><div><dt>ชื่อที่แสดง</dt><dd>${displayName}</dd></div><div><dt>รหัสเพื่อนตัวอย่าง</dt><dd class="s1c-mono">${friendCode}</dd></div></dl>
    <div class="s1c-actions"><button type="button" data-action="copy-code">คัดลอกรหัสเพื่อน</button><button type="button" data-action="rotate-code"${disabled(localOnly || requireDeviceConfirmation)}>สร้างรหัสใหม่</button></div>
    <p class="s1c-inline-note">เมื่อเปลี่ยนรหัส รหัสเก่าจะหยุดใช้ แต่เพื่อนเดิมยังอยู่เหมือนเดิม</p>
    ${requireDeviceConfirmation ? '<div class="s1c-device-pause"><strong>อุปกรณ์ใหม่</strong><span>การแชร์ถูกพักไว้จนกว่าจะยืนยันอุปกรณ์นี้</span><button type="button" class="s1c-primary-button" data-action="confirm-new-device">ยืนยันอุปกรณ์นี้ในตัวอย่าง</button></div>' : ''}
  </section>`;
}

function accountActions(state, serviceConfigured) {
  const canManage = PRIVATE_DATA_STATES.has(state);
  const canDelete = state === 'signed_in_online' && serviceConfigured;
  const sessionRow = canManage
    ? `<div class="s1c-session-row"><span class="s1c-session-icon" aria-hidden="true">▣</span><span><strong>อุปกรณ์นี้</strong><small>เซสชันตัวอย่าง · ไม่มีกุญแจหรือโทเคนจริง</small></span><span class="s1c-session-state">${state === 'signed_in_online' ? 'ใช้งาน' : 'พักไว้'}</span></div>`
    : '<p class="s1c-inline-note">ไม่มีเซสชันบัญชีให้จัดการ</p>';
  return `<section class="s1c-panel s1c-account-actions">
    <div class="s1c-section-heading"><span class="s1c-eyebrow">ควบคุมข้อมูล</span><h2>เซสชันและบัญชี</h2></div>
    ${sessionRow}
    <button type="button" data-action="export"${disabled(!canManage)}>ส่งออกข้อมูลตัวอย่าง JSON</button>
    <p class="s1c-inline-note">ไฟล์ส่งออกมีเฉพาะข้อมูลจำลอง ไม่มีข้อมูลบัญชีจริง</p>
    <button type="button" class="s1c-danger-button" data-action="delete-account"${disabled(!canDelete)}>ลบบัญชีตัวอย่าง</button>
    <p class="s1c-inline-note">การลบบัญชีจริงต้องเข้าสู่ระบบใหม่ภายใน 5 นาทีเพื่อยืนยัน</p>
    <button type="button" data-action="sign-out-everywhere"${disabled(!canManage)}>ออกจากระบบทุกอุปกรณ์</button>
    ${!serviceConfigured ? '<p class="s1c-inline-note">บริการจริงยังไม่ตั้งค่า</p>' : ''}
  </section>`;
}

function renderAccount(fixture, state, serviceConfigured) {
  return `<div class="s1c-page-grid s1c-account-grid">
    <div class="s1c-column-stack">${profileContent(fixture, state, serviceConfigured)}
      <section class="s1c-panel s1c-static-guest"><strong>บัญชีเป็นทางเลือก</strong><span>เมื่อลงชื่อออก ฟีเจอร์ในเครื่องยังใช้ได้ และไม่มีการส่งหรือรับข้อมูลเพื่อน</span></section>
    </div>
    ${accountActions(state, serviceConfigured)}
  </div>`;
}

function renderFriendRow(friend, index, canCallService) {
  const id = escapeHtml(friend.id || `friend-${index + 1}`);
  const name = escapeHtml(friend.name || 'เพื่อนตัวอย่าง');
  const code = escapeHtml(friend.code || 'KB-DEMO-FRIEND');
  const relation = escapeHtml(friend.status === 'pending' ? 'คำขอค้าง · ตัวอย่าง' : 'เพื่อน · ตัวอย่าง');
  return `<li class="s1c-friend-row"><div class="s1c-avatar" aria-hidden="true">${escapeHtml((friend.name || 'พ').trim().slice(0, 1) || 'พ')}</div><div class="s1c-friend-copy"><strong>${name}</strong><span class="s1c-mono">${code}</span></div><span class="s1c-friend-status">${relation}</span><button type="button" class="s1c-icon-button" data-action="remove-friend" data-friend-id="${id}" aria-label="นำ ${name} ออกจากรายชื่อตัวอย่าง"${disabled(!canCallService)}>นำออก</button></li>`;
}

function renderFriends(fixture, state, serviceConfigured) {
  const online = state === 'signed_in_online' && serviceConfigured;
  const offline = state === 'signed_in_offline';
  const canViewFriends = PRIVATE_DATA_STATES.has(state);
  const friends = canViewFriends && Array.isArray(fixture.friends) ? fixture.friends : [];
  const failure = canViewFriends && fixture.requestFailure ? `<p class="s1c-request-message" data-message="request-failure" role="status" aria-live="polite">${FAILURE_COPY}</p>` : '';
  const list = friends.length
    ? friends.map((friend, index) => renderFriendRow(friend || {}, index, online)).join('')
    : `<li class="s1c-empty-row">${canViewFriends ? 'ยังไม่มีเพื่อนในรายการจำลอง' : 'เข้าสู่ระบบเพื่อดูรายชื่อเพื่อนของคุณ'}</li>`;
  return `<div class="s1c-page-grid s1c-friends-grid">
    <section class="s1c-panel s1c-request-panel">
      <div class="s1c-section-heading"><span class="s1c-eyebrow">ค้นหาแบบตรงตัว</span><h2>เพิ่มเพื่อน</h2></div>
      <p class="s1c-copy">ใช้รหัสเพื่อนที่ได้รับโดยตรง ระบบตัวอย่างไม่ค้นหาชื่อหรือรายชื่อผู้ติดต่อ</p>
      <label class="s1c-control-label" for="s1c-friend-code">รหัสเพื่อน</label>
      <div class="s1c-request-controls"><input id="s1c-friend-code" data-field="friend-code" maxlength="24" autocomplete="off" placeholder="KB-…"><button type="button" class="s1c-primary-button" data-action="send-request"${disabled(!online)}>ส่งคำขอตัวอย่าง</button></div>
      ${failure}
      <p class="s1c-inline-note">คำขอที่ส่งไม่ได้จะแสดงข้อความเดียวกันเสมอ</p>
      ${offline || state === 'service_unavailable' ? '<p class="s1c-offline-note">ออฟไลน์: รายชื่อที่บันทึกไว้ดูได้ แต่ส่งคำขอไม่ได้</p>' : ''}
    </section>
    <section class="s1c-panel s1c-list-panel">
      <div class="s1c-section-heading"><span class="s1c-eyebrow">ในเครื่องนี้</span><h2>เพื่อน</h2></div>
      <ul class="s1c-list-scroll" data-panel="friends-list" data-scroll="panel" aria-label="รายชื่อเพื่อนจำลอง">${list}</ul>
    </section>
  </div>`;
}

function renderShareRow(entry, index, canChange, globallyPaused) {
  const id = escapeHtml(entry.id || `share-${index + 1}`);
  const name = escapeHtml(entry.name || 'เพื่อนตัวอย่าง');
  const pairPaused = entry.status === 'paused_new_device' || entry.status === 'paused' || entry.paused === true;
  const paused = pairPaused || globallyPaused;
  const status = paused ? 'หยุดชั่วคราว' : 'ตัวอย่าง';
  const vitalsChecked = !pairPaused && entry.scopes?.vitals === true;
  const abilitiesChecked = !pairPaused && entry.scopes?.abilities === true;
  const canEditScopes = canChange && !pairPaused;
  return `<li class="s1c-share-row">
    <div class="s1c-share-heading"><span class="s1c-avatar" aria-hidden="true">${escapeHtml((entry.name || 'พ').trim().slice(0, 1) || 'พ')}</span><strong>${name}</strong><span class="s1c-friend-status">${status}</span></div>
    <div class="s1c-scope-list" aria-label="ขอบเขตข้อมูลสำหรับ ${name}">
      <label><input type="checkbox" data-action="toggle-scope" data-friend-id="${id}" data-scope="vitals"${vitalsChecked ? ' checked' : ''}${disabled(!canEditScopes)}>สถานะในเกม · เลือกเอง</label>
      <label><input type="checkbox" data-action="toggle-scope" data-friend-id="${id}" data-scope="abilities"${abilitiesChecked ? ' checked' : ''}${disabled(!canEditScopes)}>ความสามารถ · ปิดไว้</label>
    </div>
    <button type="button" data-action="toggle-pairing" data-friend-id="${id}"${disabled(!canChange)}>${paused ? 'เลือกแชร์ต่อ' : 'พักการส่งและรับข้อมูล'}</button>
  </li>`;
}

function renderShare(fixture, state, serviceConfigured) {
  const canChange = state === 'signed_in_online' && serviceConfigured;
  const canViewShareList = PRIVATE_DATA_STATES.has(state);
  const globallyPaused = state === 'signed_in_offline' || state === 'paused_new_device';
  const entries = canViewShareList && Array.isArray(fixture.shareList) ? fixture.shareList : [];
  const list = entries.length
    ? entries.map((entry, index) => renderShareRow(entry || {}, index, canChange, globallyPaused)).join('')
    : `<li class="s1c-empty-row">${canViewShareList ? 'ยังไม่มีรายการแชร์ · การเข้าสู่ระบบไม่ได้เปิดแชร์' : 'เข้าสู่ระบบเพื่อดูรายการแชร์ของคุณ'}</li>`;
  const newDeviceNotice = state === 'paused_new_device'
    ? '<p class="s1c-pause-note" role="status">การแชร์หยุดชั่วคราวหลังใช้อุปกรณ์ใหม่ ยืนยันอุปกรณ์ก่อนเลือกแชร์ต่อ</p><button type="button" data-action="confirm-new-device">ยืนยันอุปกรณ์นี้ในตัวอย่าง</button>'
    : '';
  return `<div class="s1c-page-grid s1c-share-grid">
    <section class="s1c-panel s1c-share-intro">
      <div class="s1c-section-heading"><span class="s1c-eyebrow">ความยินยอมเป็นรายคน</span><h2>รายการแชร์</h2></div>
      <p class="s1c-copy">การเข้าสู่ระบบไม่ได้เปิดแชร์ เลือกขอบเขตแยกเป็นรายเพื่อน และเริ่มต้นปิดไว้</p>
      <p class="s1c-caution">Kohbor ไม่สามารถตรวจสอบได้ว่าเพื่อนอยู่ฝ่ายเดียวกับคุณ เพื่อนที่อยู่อีกฝ่ายอาจได้รับข้อมูลที่คุณเลือกแชร์</p>
      ${newDeviceNotice}
    </section>
    <section class="s1c-panel s1c-list-panel">
      <div class="s1c-section-heading"><span class="s1c-eyebrow">สถานะจำลอง</span><h2>เพื่อนในรายการแชร์</h2></div>
      <ul class="s1c-list-scroll" data-panel="share-list" data-scroll="panel" aria-label="รายการแชร์จำลอง">${list}</ul>
    </section>
  </div>`;
}

/** Render an escaped, deterministic slice. It performs no I/O and has no live-service path. */
export function renderAccountSlice(fixture = {}) {
  const input = fixture && typeof fixture === 'object' ? fixture : {};
  const state = ACCOUNT_STATES.includes(input.state) ? input.state : 'signed_out';
  const screen = Object.hasOwn(SCREEN_LABELS, input.screen) ? input.screen : 'account';
  const serviceConfigured = input.serviceConfigured === true;
  const page = screen === 'friends'
    ? renderFriends(input, state, serviceConfigured)
    : screen === 'share'
      ? renderShare(input, state, serviceConfigured)
      : renderAccount(input, state, serviceConfigured);

  return `<div class="s1c-app" data-state="${state}" data-screen="${screen}">
    <header class="s1c-topbar"><div class="s1c-brand"><span class="s1c-brand-mark" aria-hidden="true">K</span><div><strong>Kohbor</strong><span>บัญชีและเพื่อน · ตัวอย่างในเครื่อง</span></div></div><div class="s1c-top-state">${escapeHtml(STATE_LABELS[state])}</div></header>
    <div class="s1c-workspace">
      <aside class="s1c-sidebar" aria-label="เมนูและตัวควบคุมตัวอย่าง">${screenNav(screen)}${stateChooser(state, serviceConfigured)}<div class="s1c-sidebar-note"><strong>ไม่มีบริการจริง</strong><span>ไม่มีเครือข่าย โทเคน หรือข้อมูลจากแอป</span></div></aside>
      <main class="s1c-main" aria-label="หน้าจอบัญชี">${statusBanner(state, input.notice || '')}<section class="s1c-screen" aria-labelledby="s1c-screen-title"><h1 id="s1c-screen-title" tabindex="-1">${escapeHtml(SCREEN_LABELS[screen])}</h1>${page}</section></main>
    </div>
  </div>`;
}

/** Mount the browser fixture with an injected fake-only controller. */
export function mountAccountSlice(root, controller) {
  if (!root || !controller) throw new TypeError('root and fake controller are required');

  const focusIdentity = (element) => JSON.stringify([
    element.tagName,
    element.id || '',
    element.dataset.action || '',
    element.dataset.screen || '',
    element.dataset.friendId || '',
    element.dataset.scope || '',
    element.dataset.field || '',
  ]);
  let destroyed = false;
  const render = () => {
    if (destroyed) return;
    const active = document.activeElement;
    const restoreKey = active && root.contains(active) ? focusIdentity(active) : '';
    root.innerHTML = renderAccountSlice(controller.snapshot());
    if (!restoreKey) return;
    const controls = root.querySelectorAll('button, input, select, textarea, [tabindex]');
    const next = [...controls].find((element) => focusIdentity(element) === restoreKey && !element.matches(':disabled'));
    (next || root.querySelector('.s1c-screen h1'))?.focus({ preventScroll: true });
  };
  const act = (method, ...args) => {
    if (typeof controller[method] === 'function') controller[method](...args);
    render();
  };

  const handleClick = async (event) => {
    const target = event.target instanceof Element ? event.target.closest('[data-action]') : null;
    if (!target || target.matches(':disabled')) return;
    const action = target.dataset.action;
    const friendId = target.dataset.friendId;
    switch (action) {
      case 'navigate': act('navigate', target.dataset.screen); break;
      case 'sign-in': act('startSignIn'); break;
      case 'cancel-sign-in': act('cancelSignIn'); break;
      case 'simulate-browser-return': act('returnFromBrowser'); break;
      case 'complete-sign-in': act('completeSignIn'); break;
      case 'save-display-name': {
        const name = root.querySelector('[data-field="display-name"]')?.value ?? '';
        act('saveDisplayName', name);
        break;
      }
      case 'copy-code': {
        const before = controller.snapshot();
        const code = before.friendCode;
        let notice;
        try {
          if (!navigator.clipboard?.writeText) throw new Error('clipboard unavailable');
          await navigator.clipboard.writeText(code);
          notice = 'คัดลอกรหัสตัวอย่างแล้ว';
        } catch {
          notice = 'คัดลอกอัตโนมัติไม่ได้ เลือกรหัสตัวอย่างแล้วคัดลอกเองได้';
        }
        const current = controller.snapshot();
        if (destroyed || current.state !== before.state || current.screen !== before.screen || current.friendCode !== code || current.notice !== before.notice) return;
        controller.setNotice(notice);
        render();
        break;
      }
      case 'rotate-code': act('rotateCode'); break;
      case 'send-request': {
        const code = root.querySelector('[data-field="friend-code"]')?.value ?? '';
        act('sendRequest', code);
        break;
      }
      case 'remove-friend': act('removeFriend', friendId); break;
      case 'toggle-pairing': act('togglePairing', friendId); break;
      case 'confirm-new-device': act('confirmNewDevice'); break;
      case 'export': {
        const snapshot = controller.exportSample();
        const blob = new Blob([JSON.stringify(snapshot, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = 'kohbor-account-sample.json';
        link.click();
        URL.revokeObjectURL(url);
        controller.setNotice('สร้างไฟล์ส่งออกข้อมูลตัวอย่างแล้ว');
        render();
        break;
      }
      case 'delete-account': act('startDelete'); break;
      case 'confirm-delete': act('confirmDelete'); break;
      case 'cancel-delete': act('cancelDelete'); break;
      case 'sign-out-everywhere': act('signOutEverywhere'); break;
      default: break;
    }
  };

  const handleChange = (event) => {
    const target = event.target instanceof Element ? event.target : null;
    if (!target) return;
    if (target.matches('[data-action="choose-state"]')) act('setState', target.value);
    if (target.matches('[data-action="set-service"]')) act('setServiceConfigured', target.checked);
    if (target.matches('[data-action="toggle-scope"]')) act('toggleScope', target.dataset.friendId, target.dataset.scope, target.checked);
  };

  root.addEventListener('click', handleClick);
  root.addEventListener('change', handleChange);
  render();
  return {
    render,
    destroy() {
      if (destroyed) return;
      destroyed = true;
      root.removeEventListener('click', handleClick);
      root.removeEventListener('change', handleChange);
      root.replaceChildren();
    },
  };
}
