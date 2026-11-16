import { ACCOUNT_STATES } from './account-slice.mjs';

const VALID_STATES = new Set(ACCOUNT_STATES);
const PRIVATE_DATA_STATES = new Set(['signed_in_online', 'signed_in_offline', 'paused_new_device']);

function copyRows(rows) {
  return rows.map((row) => ({ ...row, scopes: row.scopes ? { ...row.scopes } : undefined }));
}

/** In-memory demonstration only. No fetch, host bridge, account token, or persistence. */
export function createFakeAccountController(options = {}) {
  const model = {
    state: VALID_STATES.has(options.state) ? options.state : 'signed_out',
    screen: ['account', 'friends', 'share'].includes(options.screen) ? options.screen : 'account',
    serviceConfigured: options.serviceConfigured === true,
    displayName: options.displayName || 'มะลิ',
    friendCode: options.friendCode || 'KB-DEMO-0001',
    friends: Array.isArray(options.friends) ? copyRows(options.friends) : [
      { id: 'friend-mali', name: 'มะลิ', code: 'KB-DEMO-F001', status: 'friend' },
      { id: 'friend-niran', name: 'นิรันดร์', code: 'KB-DEMO-F002', status: 'friend' },
      { id: 'friend-pending', name: 'ผู้เล่นตัวอย่าง', code: 'KB-DEMO-F003', status: 'pending' },
    ],
    shareList: Array.isArray(options.shareList) ? copyRows(options.shareList) : [
      { id: 'friend-mali', name: 'มะลิ', status: 'active', scopes: { vitals: false, abilities: false } },
      { id: 'friend-niran', name: 'นิรันดร์', status: 'paused', paused: true, scopes: { vitals: false, abilities: false } },
    ],
    requestFailure: '',
    notice: '',
    nextCodeNumber: 2,
  };

  const snapshot = () => ({
    ...model,
    friends: copyRows(model.friends),
    shareList: copyRows(model.shareList),
  });

  return {
    snapshot,
    navigate(screen) { if (['account', 'friends', 'share'].includes(screen)) model.screen = screen; },
    setState(state) {
      if (!VALID_STATES.has(state)) return;
      model.state = state;
      model.notice = '';
      if (state === 'paused_new_device') {
        model.shareList = model.shareList.map((entry) => ({ ...entry, paused: true, status: 'paused', scopes: { vitals: false, abilities: false } }));
      }
    },
    setServiceConfigured(enabled) {
      model.serviceConfigured = enabled === true;
      model.notice = enabled ? 'เปิดตัวควบคุมจำลองในเครื่องแล้ว' : 'ปิดตัวควบคุมจำลองแล้ว';
    },
    startSignIn() { if (model.serviceConfigured && model.state === 'signed_out') model.state = 'browser_handoff'; },
    cancelSignIn() { model.state = 'signed_out'; model.notice = 'ยกเลิกการเข้าสู่ระบบตัวอย่างแล้ว'; },
    returnFromBrowser() { if (model.state === 'browser_handoff') model.state = 'signing_in'; },
    completeSignIn() { if (model.state === 'signing_in') model.state = 'needs_display_name'; },
    saveDisplayName(value) {
      const name = String(value ?? '').trim();
      if (!name || [...name].length > 32) { model.notice = 'กรุณาใส่ชื่อที่แสดง 1–32 ตัวอักษร'; return; }
      model.displayName = name;
      model.state = 'signed_in_online';
      model.notice = 'บันทึกชื่อในข้อมูลตัวอย่างแล้ว';
    },
    setNotice(value) { model.notice = String(value ?? ''); },
    rotateCode() {
      if (model.state !== 'signed_in_online' || !model.serviceConfigured) return;
      model.friendCode = `KB-DEMO-${String(model.nextCodeNumber).padStart(4, '0')}`;
      model.nextCodeNumber += 1;
      model.notice = 'เปลี่ยนรหัสตัวอย่างแล้ว เพื่อนเดิมยังอยู่';
    },
    sendRequest(value) {
      const code = String(value ?? '').trim().toUpperCase();
      if (model.state !== 'signed_in_online' || !model.serviceConfigured) {
        model.requestFailure = '';
        model.notice = 'ออฟไลน์หรือบริการจำลองปิดอยู่ จึงส่งคำขอไม่ได้';
        return;
      }
      model.requestFailure = '';
      if (code === 'KB-DEMO-NEW1') {
        model.friends.push({ id: `friend-request-${model.friends.length + 1}`, name: 'เพื่อนใหม่ตัวอย่าง', code, status: 'pending' });
        model.notice = 'เพิ่มคำขอตัวอย่างลงในรายการแล้ว';
      } else {
        model.requestFailure = 'unavailable';
      }
    },
    removeFriend(id) {
      if (model.state !== 'signed_in_online' || !model.serviceConfigured) return;
      model.friends = model.friends.filter((friend) => friend.id !== id);
      model.shareList = model.shareList.filter((entry) => entry.id !== id);
      model.notice = 'นำเพื่อนออกจากข้อมูลตัวอย่างแล้ว';
    },
    toggleScope(id, scope, enabled) {
      if (model.state !== 'signed_in_online' || !model.serviceConfigured || !['vitals', 'abilities'].includes(scope)) return;
      const entry = model.shareList.find((row) => row.id === id);
      if (!entry || entry.paused === true || entry.status === 'paused' || entry.status === 'paused_new_device') return;
      entry.scopes ||= { vitals: false, abilities: false };
      entry.scopes[scope] = enabled === true;
      model.notice = enabled ? 'เปิดขอบเขตนี้ในข้อมูลตัวอย่างแล้ว' : 'ปิดขอบเขตนี้ในข้อมูลตัวอย่างแล้ว';
    },
    togglePairing(id) {
      if (model.state !== 'signed_in_online' || !model.serviceConfigured) return;
      const entry = model.shareList.find((row) => row.id === id);
      if (!entry) return;
      entry.paused = !entry.paused;
      entry.status = entry.paused ? 'paused' : 'active';
      if (entry.paused) entry.scopes = { vitals: false, abilities: false };
      model.notice = entry.paused ? 'พักการส่งและรับข้อมูลในตัวอย่างแล้ว' : 'เลือกแชร์ต่อในตัวอย่างแล้ว';
    },
    confirmNewDevice() {
      if (model.state !== 'paused_new_device') return;
      model.state = 'signed_in_online';
      model.shareList = model.shareList.map((entry) => ({ ...entry, paused: true, status: 'paused', scopes: { vitals: false, abilities: false } }));
      model.notice = 'ยืนยันอุปกรณ์ตัวอย่างแล้ว; รายการแชร์ยังพักไว้จนกว่าจะเลือกเปิดเอง';
    },
    exportSample() {
      if (!PRIVATE_DATA_STATES.has(model.state)) return null;
      return {
        sample: true,
        displayName: model.displayName,
        friendCode: model.friendCode,
        friends: copyRows(model.friends),
        shareList: copyRows(model.shareList),
      };
    },
    startDelete() { if (model.state === 'signed_in_online' && model.serviceConfigured) model.state = 'deleting'; },
    cancelDelete() { if (model.state === 'deleting') model.state = 'signed_in_online'; },
    confirmDelete() {
      if (model.state !== 'deleting') return;
      model.state = 'signed_out';
      model.displayName = '';
      model.friendCode = '';
      model.friends = [];
      model.shareList = [];
      model.notice = 'ลบข้อมูลตัวอย่างออกจากหน้านี้แล้ว';
    },
    signOutEverywhere() {
      if (model.state === 'signed_out' || model.state === 'deleting') return;
      model.state = 'signed_out';
      model.displayName = '';
      model.friendCode = '';
      model.friends = [];
      model.shareList = [];
      model.notice = 'ออกจากระบบและล้างข้อมูลตัวอย่างในหน้านี้แล้ว';
    },
  };
}
