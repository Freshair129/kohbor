import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { readFileSync } from 'node:fs';

registerHooks({
  resolve(specifier, context, nextResolve) {
    if ((specifier.startsWith('./') || specifier.startsWith('../')) && !/\.(?:[cm]?js|ts|mts|cts|json)$/.test(specifier)) {
      try { return nextResolve(`${specifier}.ts`, context); } catch { /* Try Node's normal resolver. */ }
    }
    return nextResolve(specifier, context);
  },
});

const viewModelModule = await import('../src/account/accountViewModel.ts').catch((error: unknown) => {
  if ((error as NodeJS.ErrnoException).code === 'ERR_MODULE_NOT_FOUND'
    && String((error as Error).message).includes('accountViewModel.ts')) return null;
  throw error;
});
const portModule = await import('../src/account/accountView.ts');
const fakeModule = await import('../src/account/FakeAccountPort.ts').catch((error: unknown) => {
  if ((error as NodeJS.ErrnoException).code === 'ERR_MODULE_NOT_FOUND'
    && String((error as Error).message).includes('FakeAccountPort.ts')) return null;
  throw error;
});
const tauriPortModule = await import('../src/account/tauriAccountPort.ts');

type TestSnapshot = {
  state: string;
  serviceOpen: boolean;
  liveMatch: boolean;
  displayName: string | null;
  friendCode: { value: string | null; freshness: 'live' | 'cached' | 'unknown' };
  sessions: Array<{ thisDevice: boolean; signedInOn: string | null }> | null;
  handoffDeadline: number | null;
  handoffPurpose: 'sign_in' | 'delete';
  firstSignIn: boolean;
  dialog: string | null;
  nameError: string | null;
  notice: string | null;
  rateLimitDeadline: number | null;
  deleteStep: string | null;
  deleteError: string | null;
  freshSignInDeadline: number | null;
  codeReplacementsRemaining: number | null;
  requestFailed: boolean;
};

function snapshot(state: string, values: Partial<TestSnapshot> = {}): TestSnapshot {
  return {
    state, serviceOpen: true, liveMatch: false, displayName: null,
    friendCode: { value: null, freshness: 'unknown' }, sessions: null,
    handoffDeadline: null, handoffPurpose: 'sign_in', firstSignIn: false,
    dialog: null, nameError: null, notice: null, rateLimitDeadline: null,
    deleteStep: null, deleteError: null, freshSignInDeadline: null,
    codeReplacementsRemaining: 3, requestFailed: false, ...values,
  };
}

const now = 1_000_000;
const screenCases: Array<[string, TestSnapshot]> = [
  ['A1', snapshot('signed_out')],
  ['A1b', snapshot('signed_out', { serviceOpen: false })],
  ['A1c', snapshot('signed_out', { liveMatch: true })],
  ['A2', snapshot('browser_handoff', { handoffDeadline: now + 30_000 })],
  ['A3', snapshot('signing_in')],
  ['A4', snapshot('needs_display_name')],
  ['A4e', snapshot('needs_display_name', { nameError: 'refused' })],
  ['A5', snapshot('signed_in_online', { firstSignIn: true, displayName: 'Mali' })],
  ['A5b', snapshot('signed_in_online', { dialog: 'replace_code' })],
  ['A6', snapshot('signed_in_offline', { friendCode: { value: 'KB-CACHED', freshness: 'cached' } })],
  ['A7', snapshot('signed_in_online', { displayName: 'Mali' })],
  ['A7b', snapshot('signed_in_online', { dialog: 'sign_out_everywhere' })],
  ['A7c', snapshot('signed_in_online', { dialog: 'export' })],
  ['A8', snapshot('deleting', { deleteStep: 'review' })],
  ['A8b', snapshot('deleting', { deleteStep: 'confirm', freshSignInDeadline: now + 30_000 })],
  ['E1', snapshot('browser_handoff', { handoffDeadline: now - 1 })],
  ['E2', snapshot('service_unavailable')],
  ['E3', snapshot('signed_out', { notice: 'provider_declined' })],
  ['E5', snapshot('signed_out', { rateLimitDeadline: now + 30_000 })],
  ['E6', snapshot('deleting', { deleteError: 'mismatch' })],
  ['X1', snapshot('session_revoked')],
  ['X2', snapshot('paused_new_device')],
];

test('account screen view-model API is implemented', () => {
  assert.ok(viewModelModule, 'accountViewModel.ts is not implemented');
  assert.equal(typeof viewModelModule.buildAccountViewModel, 'function');
});

test('every specified account screen has Thai and English view-models', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  assert.deepEqual([...viewModelModule.ACCOUNT_SCREEN_IDS], screenCases.map(([id]) => id));
  for (const [screenId, state] of screenCases) {
    for (const locale of ['th', 'en'] as const) {
      const viewModel = viewModelModule.buildAccountViewModel(state, locale, now);
      assert.equal(viewModel.screenId, screenId, `${screenId} in ${locale}`);
      assert.ok(viewModel.title.length > 0, `${screenId} title in ${locale}`);
      assert.ok(viewModel.copyKeys.length > 0, `${screenId} copy keys in ${locale}`);
      for (const key of viewModel.copyKeys) assert.ok(viewModelModule.ACCOUNT_COPY[locale][key], `${screenId} uses ${key} in ${locale}`);
    }
  }
});

test('Thai and English copy key sets are identical', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  assert.deepEqual(Object.keys(viewModelModule.ACCOUNT_COPY.th).sort(), Object.keys(viewModelModule.ACCOUNT_COPY.en).sort());
});

test('account UI copy contains none of the prohibited identity terms', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const forbidden = /\bpassword\b|g-maiden|\bgid\b|\bsteam\b/i;
  for (const [locale, values] of Object.entries(viewModelModule.ACCOUNT_COPY)) {
    for (const [key, value] of Object.entries(values as Record<string, string>)) {
      assert.doesNotMatch(value, forbidden, `${locale}.${key}`);
    }
  }
});

test('account disclosure names the Google fields Supabase receives', () => {
  assert.match(viewModelModule.ACCOUNT_COPY.en['signin.receives'], /Google email, name, and profile picture through Supabase Auth/);
  assert.match(viewModelModule.ACCOUNT_COPY.th['signin.receives'], /อีเมล ชื่อ และรูปโปรไฟล์ Google/);
});

test('missing display name stays null in the view-model', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const viewModel = viewModelModule.buildAccountViewModel(snapshot('signed_in_online'), 'en', now);
  assert.equal(viewModel.identity.displayName, null);
  assert.equal(viewModel.identity.displayNameLabel, null);
});

test('display-name counter uses grapheme clusters', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  assert.equal(viewModelModule.countGraphemeClusters('กิ่'), 1);
  assert.equal(viewModelModule.countGraphemeClusters('👩‍🚀'), 1);
});

test('display-name count is advisory and does not replace service validation', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const viewModel = viewModelModule.buildAccountViewModel(snapshot('needs_display_name'), 'en', now);
  assert.equal(viewModel.graphemeCount, 0);
  assert.equal(viewModel.actions.saveName.disabled, false);
  assert.equal(viewModel.actions.saveName.reason, null);
});

test('display-name form sends counted input for service validation', () => {
  const page = readFileSync(new URL('../src/account/AccountPages.tsx', import.meta.url), 'utf8');
  assert.match(page, /type="submit" disabled=\{!hasPort\}/);
  assert.doesNotMatch(page, /graphemeCount\s*[<>]=?\s*(?:1|32)/);
});

test('cached friend code is marked cached and never live', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const viewModel = viewModelModule.buildAccountViewModel(screenCases[9]![1], 'en', now);
  assert.equal(viewModel.friendCode.value, 'KB-CACHED');
  assert.equal(viewModel.friendCode.presentation, 'cached');
  assert.match(viewModel.friendCode.className, /is-cached/);
  assert.doesNotMatch(viewModel.friendCode.className, /is-live/);
});

test('cached friend-code presentation has a dashed token border', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const css = readFileSync(new URL('../src/account/account.css', import.meta.url), 'utf8');
  assert.match(css, /\.account-code\.is-cached\s*\{[^}]*border-style:\s*dashed/s);
  assert.match(css, /\.account-code\.is-live\s*\{[^}]*border-style:\s*solid/s);
});

test('delete and sign-in stay disabled while a match is live', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const viewModel = viewModelModule.buildAccountViewModel(
    snapshot('signed_in_online', { liveMatch: true }), 'en', now,
  );
  assert.equal(viewModel.actions.deleteAccount.disabled, true);
  assert.ok(viewModel.actions.deleteAccount.reason);
  assert.equal(viewModel.actions.signIn.disabled, true);
});

test('delete stays disabled when the fresh-sign-in deadline is unknown', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const viewModel = viewModelModule.buildAccountViewModel(
    snapshot('deleting', { deleteStep: 'confirm', freshSignInDeadline: null }), 'en', now,
  );
  assert.equal(viewModel.screenId, 'A8b');
  assert.equal(viewModel.actions.confirmDelete.disabled, true);
  assert.ok(viewModel.actions.confirmDelete.reason);
});

test('a hand-off deadline that has passed resolves to E1', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const active = viewModelModule.buildAccountViewModel(screenCases[3]![1], 'en', now);
  const expired = viewModelModule.buildAccountViewModel(snapshot('browser_handoff', { handoffDeadline: now }), 'en', now);
  assert.equal(active.screenId, 'A2');
  assert.equal(expired.screenId, 'E1');
  const deleteExpired = viewModelModule.buildAccountViewModel(
    snapshot('browser_handoff', { handoffPurpose: 'delete', handoffDeadline: now }), 'en', now,
  );
  assert.equal(deleteExpired.screenId, 'E6');
});

test('delete hand-off can be reopened until its deadline unless a match is live', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const handoff = snapshot('browser_handoff', {
    handoffPurpose: 'delete', handoffDeadline: now + 30_000, deleteStep: 'reauth',
  });
  assert.equal(viewModelModule.buildAccountViewModel(handoff, 'en', now).actions.continueDelete.disabled, false);
  assert.equal(viewModelModule.buildAccountViewModel({ ...handoff, liveMatch: true }, 'en', now).actions.continueDelete.disabled, true);
});

test('all friend-request failure causes use the same wording', (context) => {
  if (!viewModelModule) return context.skip('accountViewModel.ts not implemented yet');
  const reasons = ['unknown_code', 'blocked', 'requests_off', 'rate_limited', 'service_error'];
  for (const locale of ['th', 'en'] as const) {
    const messages = reasons.map((reason) => viewModelModule.friendRequestFailureCopy(locale, reason));
    assert.equal(new Set(messages).size, 1, `${locale} friend-request failure wording`);
  }
});

test('the default account port is the guest port', () => {
  assert.ok(portModule.guestAccountPort instanceof portModule.GuestAccountPort);
  assert.equal(portModule.guestAccountPort.get().state, 'guest');
  assert.equal(portModule.guestAccountPort.getState().state, 'signed_out');
  assert.equal(portModule.guestAccountPort.getState().displayName, null);
});

test('Tauri account port maps native state, display name, avatar URL, and live-match block', async () => {
  const calls: string[] = [];
  const nativeStatus = {
    state: 'signed_in', displayName: 'Mali', avatarUrl: 'https://images.example/avatar.png',
    liveMatchBlocksHandoff: false,
  };
  const port = new tauriPortModule.TauriAccountPort(async <T>(command: string): Promise<T> => {
    calls.push(command);
    return nativeStatus as T;
  });
  await port.refreshStatus();
  assert.deepEqual(calls, ['account_status']);
  assert.deepEqual(port.get(), {
    state: 'online', displayName: 'Mali', avatar: nativeStatus.avatarUrl,
    canSignIn: false, canSignOut: true, friendsAvailable: false,
  });
  assert.equal(port.getState().state, 'signed_in_online');
  assert.equal(port.getState().liveMatch, false);
});

test('Tauri account port maps sign-in, cancellation, and sign-out commands', async () => {
  const calls: string[] = [];
  let signInCount = 0;
  const port = new tauriPortModule.TauriAccountPort(async <T>(command: string): Promise<T> => {
    calls.push(command);
    if (command === 'account_status') {
      return { state: 'signed_out', displayName: null, avatarUrl: null, liveMatchBlocksHandoff: false } as T;
    }
    if (command === 'account_sign_in') {
      signInCount += 1;
      return (signInCount === 1
        ? { state: 'browser_handoff', displayName: null, avatarUrl: null, liveMatchBlocksHandoff: false }
        : { state: 'signed_in', displayName: 'Mali', avatarUrl: null, liveMatchBlocksHandoff: false }) as T;
    }
    return undefined as T;
  });
  await port.refreshStatus();
  await port.signIn();
  await port.cancelHandoff();
  await port.signIn();
  await port.signOutHere();
  assert.deepEqual(calls, [
    'account_status', 'account_sign_in', 'account_cancel_sign_in', 'account_sign_in', 'account_sign_out',
  ]);
  assert.equal(port.getState().state, 'signed_out');
  assert.equal(port.get().avatar, null);
});

test('guest account port remains the selected default outside Tauri', () => {
  const selected = tauriPortModule.accountPortForEnvironment(false, async <T>(): Promise<T> => {
    throw new Error('guest account must not invoke native commands');
  });
  assert.strictEqual(selected, portModule.guestAccountPort);
});

test('Tauri account port refuses browser hand-off during a live match', async () => {
  const calls: string[] = [];
  const port = new tauriPortModule.TauriAccountPort(async <T>(command: string): Promise<T> => {
    calls.push(command);
    return {
      state: 'signed_out', displayName: null, avatarUrl: null, liveMatchBlocksHandoff: false,
    } as T;
  });
  await port.refreshStatus();
  port.setLiveMatch(true);
  await port.signIn();
  assert.deepEqual(calls, ['account_status']);
  assert.equal(port.getState().liveMatch, true);
  assert.equal(port.get().canSignIn, false);
});

test('explicit fake account port stays in memory and never replaces the guest default', (context) => {
  if (!fakeModule) return context.skip('FakeAccountPort.ts not implemented yet');
  const fake = new fakeModule.FakeAccountPort({ state: 'signed_out', serviceOpen: true });
  assert.equal(fake.getState().state, 'signed_out');
  assert.ok(portModule.guestAccountPort instanceof portModule.GuestAccountPort);
  assert.notEqual(fake, portModule.guestAccountPort);
  assert.equal(typeof fake.setScenario, 'function', 'dev preview scenarios are explicit');
});

test('fake port hand-off actions are explicit and remain local', async (context) => {
  if (!fakeModule) return context.skip('FakeAccountPort.ts not implemented yet');
  let nowValue = 500;
  const fake = new fakeModule.FakeAccountPort({ state: 'signed_out', serviceOpen: true, now: () => nowValue });
  await fake.signIn();
  assert.equal(fake.getState().state, 'browser_handoff');
  assert.equal(fake.getState().handoffDeadline, nowValue + 5 * 60_000);
  await fake.cancelHandoff();
  assert.equal(fake.getState().state, 'signed_out');
  fake.setScenario({ state: 'signed_in_online', liveMatch: true, displayName: 'Mali' });
  await fake.beginDelete();
  assert.equal(fake.getState().state, 'signed_in_online', 'fake blocks delete while a match is live');
  nowValue += 1;
  assert.ok(portModule.guestAccountPort instanceof portModule.GuestAccountPort);
});

test('fake port applies name limits as its service authority', async (context) => {
  if (!fakeModule) return context.skip('FakeAccountPort.ts not implemented yet');
  const fake = new fakeModule.FakeAccountPort({ state: 'needs_display_name' });
  const result = await fake.saveDisplayName('');
  assert.deepEqual(result, { ok: false, reason: 'failed' });
  assert.equal(fake.getState().nameError, 'empty');
  assert.equal(fake.getState().state, 'needs_display_name');
});
