// Synthetic, offline Stage C contract. ACCOUNT identifiers below refer to proposed requirements.
import test from 'node:test';
import assert from 'node:assert/strict';

async function render(fixture) {
  let module;
  try {
    module = await import('../src/account/s1c/account-slice.mjs');
  } catch (error) {
    if (error.code === 'ERR_MODULE_NOT_FOUND' && String(error.message).includes('account-slice.mjs')) {
      assert.fail('Stage C account renderer is not implemented');
    }
    throw error;
  }
  assert.equal(typeof module.renderAccountSlice, 'function', 'export renderAccountSlice');
  const html = module.renderAccountSlice({ serviceConfigured: true, screen: 'account', ...fixture });
  assert.equal(typeof html, 'string');
  return html;
}

const stateCases = [
  ['signed_out', /เข้าสู่ระบบ/],
  ['browser_handoff', /เบราว์เซอร์/],
  ['signing_in', /กำลังเข้าสู่ระบบ/],
  ['needs_display_name', /ชื่อที่แสดง/],
  ['signed_in_online', /ออนไลน์/],
  ['signed_in_offline', /ออฟไลน์/],
  ['service_unavailable', /บริการ.*(ยังไม่|ไม่พร้อม|ไม่สามารถ)/],
  ['session_revoked', /(เซสชัน|การเข้าสู่ระบบ).*(สิ้นสุด|ถูกยกเลิก|หมดอายุ)/],
  ['paused_new_device', /(อุปกรณ์ใหม่|เครื่องใหม่)/],
  ['deleting', /กำลังลบ/],
];
for (const [state, expected] of stateCases) {
  test(`ACCOUNT state ${state} has a meaningful Thai status`, async () => {
    assert.match(await render({ state }), expected);
  });
}

function actionTag(html, name) {
  const tag = html.match(new RegExp(`<button\\b[^>]*data-action=["']${name}["'][^>]*>`, 'i'))?.[0];
  assert.ok(tag, `button data-action="${name}" must exist`);
  return tag;
}

test('ACCOUNT-001 pending service explains browser and privacy; sign-in disabled', async () => {
  const html = await render({ state: 'signed_out', serviceConfigured: false });
  assert.match(html, /Google/);
  assert.match(html, /เบราว์เซอร์/);
  assert.match(html, /เกาะเบาะได้รับอีเมล ชื่อ และรูปโปรไฟล์จาก Google ผ่าน Supabase Auth/);
  assert.match(html, /โหมดจำลองนี้ไม่ส่งข้อมูลจริง/);
  assert.match(html, /ไม่มีการเรียกเครือข่าย/);
  assert.match(actionTag(html, 'sign-in'), /\bdisabled\b/i);
});

test('configured fake enables sign-in; handoff offers cancellation', async () => {
  const signedOut = await render({ state: 'signed_out' });
  assert.doesNotMatch(actionTag(signedOut, 'sign-in'), /\bdisabled\b/i);
  const handoff = await render({ state: 'browser_handoff' });
  assert.match(actionTag(handoff, 'cancel-sign-in'), /<button/i);
});

test('ACCOUNT-004 online identity offers copy and rotation with consequences', async () => {
  const html = await render({ state: 'signed_in_online', displayName: 'มะลิ', friendCode: 'KB-EXAMPLE' });
  assert.match(html, /มะลิ/);
  assert.match(html, /KB-EXAMPLE/);
  actionTag(html, 'copy-code');
  actionTag(html, 'rotate-code');
  assert.match(html, /รหัสเก่า.*(ใช้ไม่ได้|หยุดใช้)/);
  assert.match(html, /เพื่อน.*(ยังอยู่|เหมือนเดิม|คงอยู่)/);
});

test('SOCIAL-001 all denied requests render the same generic failure', async () => {
  const results = [];
  for (const requestFailure of ['unknown', 'blocked', 'requests_off']) {
    const html = await render({ state: 'signed_in_online', screen: 'friends', requestFailure });
    const notice = html.match(/<[^>]+data-message=["']request-failure["'][^>]*>([^<]+)<\//i)?.[1];
    assert.ok(notice, 'generic request-failure message is visible');
    assert.match(notice, /ส่ง.*ไม่|ไม่.*ส่ง/);
    assert.doesNotMatch(notice, /บล็อก|ไม่พบ/);
    results.push(notice);
  }
  assert.equal(new Set(results).size, 1);
});

test('ACCOUNT-006 offline friends are visible read-only and local functions remain available', async () => {
  const html = await render({ state: 'signed_in_offline', screen: 'friends', friends: [{ name: 'มะลิ', code: 'KB-EXAMPLE' }] });
  assert.match(html, /มะลิ/);
  assert.match(html, /ออฟไลน์/);
  assert.match(html, /(เครื่องนี้|ในเครื่อง).*(ใช้งาน|ทำงาน)/);
  assert.match(actionTag(html, 'send-request'), /\bdisabled\b/i);
});

test('Share list communicates explicit consent and new-device pause', async () => {
  const html = await render({ state: 'paused_new_device', screen: 'share', shareList: [{ name: 'มะลิ', status: 'paused_new_device' }] });
  assert.match(html, /มะลิ/);
  assert.match(html, /(หยุด|พัก).*แชร์|แชร์.*(หยุด|พัก)/);
  assert.match(html, /(ยืนยัน|เลือก).*แชร์|แชร์.*(ยืนยัน|เลือก)/);
  assert.match(html, /(ทีม|ฝ่าย).*(ยืนยัน|ตรวจสอบ)|ไม่.*(ยืนยัน|ตรวจสอบ).*(ทีม|ฝ่าย)/);
});

test('account exposes session, export and deletion controls with fresh sign-in explanation', async () => {
  const html = await render({ state: 'signed_in_online' });
  for (const action of ['sign-out-everywhere', 'export', 'delete-account']) actionTag(html, action);
  assert.match(html, /(อุปกรณ์|เซสชัน)/);
  assert.match(html, /ลบ.*(เข้าสู่ระบบ|ยืนยันตัวตน)|(เข้าสู่ระบบ|ยืนยันตัวตน).*ลบ/);
});

test('friends and share lists declare panel-local scrolling, not page scrolling', async () => {
  for (const screen of ['friends', 'share']) {
    const html = await render({ state: 'signed_in_online', screen });
    assert.match(html, /data-panel=["'](?:friends|share)-list["']/);
    assert.match(html, /(?:overflow-y\s*:\s*auto|data-scroll=["']panel["'])/i);
    assert.doesNotMatch(html, /(?:<body[^>]*style=["'][^"']*overflow-y\s*:\s*(?:auto|scroll))/i);
  }
});

test('untrusted display names are escaped in rendered markup', async () => {
  const html = await render({ state: 'signed_in_online', displayName: '<img src=x onerror=alert(1)>' });
  assert.doesNotMatch(html, /<img\b/i);
  assert.match(html, /&lt;img/);
});
