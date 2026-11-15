import { invoke } from '@tauri-apps/api/core';
import { mountAccountSlice } from './account-slice.mjs';
import { createFakeAccountController } from './fake-account.mjs';
import './account-slice.css';

const COMMAND = 'account_fake_native';
const ACCOUNT_FAKE_DISABLED = 'โหมดทดสอบบัญชี native ยังไม่พร้อม โปรไฟล์และตัวควบคุมจำลองยังปิดอยู่';
const NATIVE_SIGN_IN_PENDING = 'กำลังเรียก DesktopAccountHost พร้อม provider และ broker จำลองในเครื่อง';
const NATIVE_SIGN_IN_FAILED = 'การทดสอบ native ไม่สำเร็จ ไม่มีการเข้าสู่ระบบกับบริการจริง';
const NATIVE_DISPLAY_NAME_PENDING = 'กำลังบันทึกและตรวจสอบชื่อใน DesktopAccountHost';
const NATIVE_DISPLAY_NAME_FAILED = 'บันทึกหรืออ่านชื่อจาก DesktopAccountHost ไม่สำเร็จ ลองอีกครั้งได้';
const NATIVE_DISPLAY_NAME_SAVED = 'DesktopAccountHost accepted · save_display_name · broker 1 · persisted · guarded readback';
const NATIVE_DISPLAY_NAME_RESTORED = 'DesktopAccountHost accepted · read_display_name · persisted · guarded readback';
const NATIVE_SIGN_OUT_PENDING = 'กำลังออกจากระบบบัญชีจำลองผ่าน DesktopAccountHost';
const NATIVE_SIGN_OUT_FAILED = 'ออกจากระบบบัญชีจำลองไม่สำเร็จ; กำลังตรวจสอบสถานะที่บันทึกไว้';
const NATIVE_SIGN_OUT_ACCEPTED = 'DesktopAccountHost accepted · sign_out · pairing 1 · revocation 1 · recovery clear · session absent';
const NATIVE_DELETE_PENDING = 'กำลังยืนยันบัญชีจำลองใหม่และลบผ่าน DesktopAccountHost';
const NATIVE_DELETE_FAILED = 'ลบบัญชีจำลองไม่สำเร็จ; กำลังตรวจสอบสถานะที่บันทึกไว้';
const NATIVE_DELETE_ACCEPTED = 'DesktopAccountHost accepted · delete_account · provider 1 · broker sign-in 1 · broker delete 1 · recovery clear · session absent';
const nativeMounts = new WeakSet();

function isRecord(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isCheckReceipt(value) {
  return isRecord(value)
    && value.operation === 'profile_check'
    && value.outcome === 'ready'
    && value.hostState === 'fake_gate_ready'
    && value.providerExchanges === 0
    && value.brokerSignIns === 0
    && value.sessionPersisted === null;
}

function isSignInReceipt(value) {
  return isRecord(value)
    && value.operation === 'fake_sign_in'
    && value.outcome === 'accepted'
    && value.hostState === 'signed_in_online'
    && value.providerExchanges === 1
    && value.brokerSignIns === 1
    && value.recoveryMarkerClear === true
    && value.sessionPersisted === true;
}

function isSignOutReceipt(value) {
  return isRecord(value)
    && value.operation === 'sign_out'
    && value.outcome === 'accepted'
    && value.hostState === 'signed_out'
    && value.providerExchanges === 0
    && value.brokerSignIns === 0
    && value.brokerPairingLeaves === 1
    && value.brokerSessionRevocations === 1
    && value.brokerAccountDeletions === 0
    && value.recoveryMarkerClear === true
    && value.sessionPersisted === false;
}

function isDeleteAccountReceipt(value) {
  return isRecord(value)
    && value.operation === 'delete_account'
    && value.outcome === 'accepted'
    && value.hostState === 'signed_out'
    && value.providerExchanges === 1
    && value.brokerSignIns === 1
    && value.brokerPairingLeaves === 0
    && value.brokerSessionRevocations === 0
    && value.brokerAccountDeletions === 1
    && value.recoveryMarkerClear === true
    && value.sessionPersisted === false;
}

function isValidDisplayName(value) {
  return typeof value === 'string'
    && value === value.trim()
    && [...value].length >= 1
    && [...value].length <= 32
    && ![...value].some((character) => /\p{Cc}/u.test(character));
}

function isDisplayNameSaveReceipt(value, expectedName) {
  return isRecord(value)
    && value.operation === 'save_display_name'
    && value.outcome === 'accepted'
    && value.hostState === 'signed_in_online'
    && value.providerExchanges === 0
    && value.brokerSignIns === 0
    && value.recoveryMarkerClear === true
    && value.sessionPersisted === true
    && value.brokerNameAcceptances === 1
    && value.displayNamePersisted === true
    && value.displayNameReadBack === true
    && value.displayName === expectedName;
}

function isDisplayNameReadReceipt(value) {
  return isRecord(value)
    && value.operation === 'read_display_name'
    && value.outcome === 'accepted'
    && value.hostState === 'signed_in_online'
    && value.providerExchanges === 0
    && value.brokerSignIns === 0
    && value.recoveryMarkerClear === true
    && value.sessionPersisted === true
    && value.brokerNameAcceptances === 0
    && value.displayNamePersisted === true
    && value.displayNameReadBack === true
    && isValidDisplayName(value.displayName);
}

function isTauriWindow() {
  return typeof window !== 'undefined'
    && typeof window.__TAURI_INTERNALS__ === 'object'
    && window.__TAURI_INTERNALS__ !== null;
}

function createNativeController(refresh, options = {}) {
  const fake = createFakeAccountController(options);
  let receiptMessage = options.displayNameRestored ? NATIVE_DISPLAY_NAME_RESTORED : '';
  let signInPending = false;
  let displayNamePending = false;
  let teardownPending = '';

  async function restoreOrHideAfterTeardownFailure(message) {
    try {
      const receipt = await invoke(COMMAND, { action: 'readDisplayName' });
      if (!isDisplayNameReadReceipt(receipt)) throw new Error('native display-name read receipt rejected');
      fake.setState('signed_in_online');
      fake.saveDisplayName(receipt.displayName);
    } catch {
      // A pending recovery marker or absent session means cached renderer data
      // is not authoritative; clear it before showing the unavailable state.
      if (fake.snapshot().state === 'deleting') fake.cancelDelete();
      fake.signOutEverywhere();
      fake.setState('service_unavailable');
    }
    receiptMessage = '';
    fake.setNotice(message);
    refresh();
  }

  const controller = {
    ...fake,
    snapshot() {
      const snapshot = fake.snapshot();
      if (signInPending && snapshot.state === 'signing_in') {
        snapshot.notice = NATIVE_SIGN_IN_PENDING;
      } else if (displayNamePending && snapshot.state === 'needs_display_name') {
        snapshot.notice = NATIVE_DISPLAY_NAME_PENDING;
      } else if (teardownPending === 'sign_out') {
        snapshot.notice = NATIVE_SIGN_OUT_PENDING;
      } else if (teardownPending === 'delete_account') {
        snapshot.notice = NATIVE_DELETE_PENDING;
      } else if (
        receiptMessage
        && !snapshot.notice
        && ['needs_display_name', 'signed_in_online'].includes(snapshot.state)
      ) {
        snapshot.notice = receiptMessage;
      }
      return snapshot;
    },
    setState(state) {
      if (signInPending || displayNamePending || teardownPending) return;
      receiptMessage = '';
      fake.setState(state);
    },
    setServiceConfigured(enabled) {
      if (signInPending || displayNamePending || teardownPending) return;
      fake.setServiceConfigured(enabled);
    },
    completeSignIn() {
      const current = fake.snapshot();
      if (current.state !== 'signing_in' || !current.serviceConfigured || signInPending || displayNamePending || teardownPending) return;

      signInPending = true;
      fake.setNotice(NATIVE_SIGN_IN_PENDING);
      refresh();

      void invoke(COMMAND, { action: 'signIn' })
        .then((result) => {
          if (!isSignInReceipt(result) || fake.snapshot().state !== 'signing_in') {
            throw new Error('native fake receipt rejected');
          }
          fake.completeSignIn();
          receiptMessage = 'DesktopAccountHost accepted · signed_in_online · provider 1 · broker 1 · session persisted';
          fake.setNotice(receiptMessage);
        })
        .catch(() => {
          if (fake.snapshot().state === 'signing_in') {
            fake.setState('service_unavailable');
            fake.setNotice(NATIVE_SIGN_IN_FAILED);
          }
        })
        .finally(() => {
          signInPending = false;
          refresh();
        });
    },
    cancelSignIn() {
      if (signInPending || displayNamePending || teardownPending) {
        fake.setNotice(NATIVE_SIGN_IN_PENDING);
        return;
      }
      receiptMessage = '';
      fake.cancelSignIn();
    },
    saveDisplayName(value) {
      if (signInPending || displayNamePending || teardownPending || fake.snapshot().state !== 'needs_display_name') return;
      const name = String(value ?? '').trim();
      if (!isValidDisplayName(name)) {
        fake.setNotice('กรุณาใส่ชื่อที่แสดง 1–32 ตัวอักษร');
        return;
      }

      displayNamePending = true;
      fake.setNotice(NATIVE_DISPLAY_NAME_PENDING);
      refresh();

      void invoke(COMMAND, { action: 'saveDisplayName', displayName: name })
        .then((saveReceipt) => {
          if (!isDisplayNameSaveReceipt(saveReceipt, name)) {
            throw new Error('native display-name save receipt rejected');
          }
          return invoke(COMMAND, { action: 'readDisplayName' });
        })
        .then((readReceipt) => {
          if (!isDisplayNameReadReceipt(readReceipt) || readReceipt.displayName !== name) {
            throw new Error('native display-name read receipt rejected');
          }
          if (fake.snapshot().state !== 'needs_display_name') return;
          fake.saveDisplayName(readReceipt.displayName);
          receiptMessage = NATIVE_DISPLAY_NAME_SAVED;
          fake.setNotice(receiptMessage);
        })
        .catch(() => {
          if (fake.snapshot().state === 'needs_display_name') {
            fake.setNotice(NATIVE_DISPLAY_NAME_FAILED);
          }
        })
        .finally(() => {
          displayNamePending = false;
          refresh();
        });
    },
    startDelete() {
      if (signInPending || displayNamePending || teardownPending || fake.snapshot().state !== 'signed_in_online') return;
      receiptMessage = '';
      fake.setNotice('');
      fake.startDelete();
    },
    cancelDelete() {
      if (teardownPending) {
        fake.setNotice(NATIVE_DELETE_PENDING);
        return;
      }
      fake.cancelDelete();
    },
    confirmDelete() {
      if (signInPending || displayNamePending || teardownPending || fake.snapshot().state !== 'deleting') return;

      teardownPending = 'delete_account';
      fake.setNotice(NATIVE_DELETE_PENDING);
      refresh();

      void invoke(COMMAND, { action: 'deleteAccount', confirmed: true })
        .then((result) => {
          if (!isDeleteAccountReceipt(result) || fake.snapshot().state !== 'deleting') {
            throw new Error('native account-delete receipt rejected');
          }
          fake.confirmDelete();
          receiptMessage = NATIVE_DELETE_ACCEPTED;
          fake.setNotice(receiptMessage);
        })
        .catch(() => restoreOrHideAfterTeardownFailure(NATIVE_DELETE_FAILED))
        .finally(() => {
          teardownPending = '';
          refresh();
        });
    },
    signOutEverywhere() {
      if (signInPending || displayNamePending || teardownPending) return;
      const current = fake.snapshot();
      if (!['signed_in_online', 'signed_in_offline', 'paused_new_device'].includes(current.state)) return;

      teardownPending = 'sign_out';
      receiptMessage = '';
      fake.setNotice(NATIVE_SIGN_OUT_PENDING);
      refresh();

      void invoke(COMMAND, { action: 'signOut' })
        .then((result) => {
          if (!isSignOutReceipt(result) || fake.snapshot().state !== current.state) {
            throw new Error('native sign-out receipt rejected');
          }
          fake.signOutEverywhere();
          receiptMessage = NATIVE_SIGN_OUT_ACCEPTED;
          fake.setNotice(receiptMessage);
        })
        .catch(() => restoreOrHideAfterTeardownFailure(NATIVE_SIGN_OUT_FAILED))
        .finally(() => {
          teardownPending = '';
          refresh();
        });
    },
  };

  return controller;
}

/** Mount S1-C only after the native demo gate confirms its isolated profile. */
export async function mountNativeAccount(root) {
  if (!(root instanceof HTMLElement) || !isTauriWindow() || !root.isConnected || nativeMounts.has(root)) return false;

  nativeMounts.add(root);
  let mounted = false;
  const isCurrent = () => nativeMounts.has(root) && root.isConnected;
  try {
    root.textContent = 'กำลังตรวจสอบโปรไฟล์บัญชี native fake';

    let probe;
    try {
      probe = await invoke(COMMAND, { action: 'check' });
    } catch {
      if (isCurrent()) root.textContent = ACCOUNT_FAKE_DISABLED;
      return false;
    }
    if (!isCurrent()) return false;
    if (!isCheckReceipt(probe)) {
      root.textContent = ACCOUNT_FAKE_DISABLED;
      return false;
    }

    let controllerOptions = { state: 'signed_out', serviceConfigured: false };
    try {
      const readReceipt = await invoke(COMMAND, { action: 'readDisplayName' });
      if (isDisplayNameReadReceipt(readReceipt)) {
        controllerOptions = {
          state: 'signed_in_online',
          serviceConfigured: true,
          displayName: readReceipt.displayName,
          displayNameRestored: true,
        };
      }
    } catch {
      // Refused reads leave the signed-out options intact.
    }
    // Missing or malformed name receipts also keep the normal signed-out UI path.
    if (!isCurrent()) return false;

    let view;
    const controller = createNativeController(() => view?.render(), controllerOptions);
    if (!controllerOptions.displayNameRestored) {
      controller.setNotice('โปรไฟล์ native fake พร้อมแล้ว; เปิดตัวควบคุมจำลองเพื่อเริ่ม sign-in');
    }
    view = mountAccountSlice(root, controller);
    mounted = true;
    return true;
  } finally {
    if (!mounted) nativeMounts.delete(root);
  }
}
