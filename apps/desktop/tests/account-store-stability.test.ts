import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';

registerHooks({
  resolve(specifier, context, nextResolve) {
    if ((specifier.startsWith('./') || specifier.startsWith('../')) && !/\.(?:[cm]?js|ts|mts|cts|json)$/.test(specifier)) {
      try { return nextResolve(`${specifier}.ts`, context); } catch { /* Try Node's normal resolver. */ }
    }
    return nextResolve(specifier, context);
  },
});

const { TauriAccountPort } = await import('../src/account/tauriAccountPort.ts');

type TestStatus = {
  state: 'signed_out' | 'signed_in';
  displayName: string | null;
  avatarUrl: string | null;
  liveMatchBlocksHandoff: boolean;
};

function createPort() {
  let status: TestStatus = {
    state: 'signed_out',
    displayName: null,
    avatarUrl: null,
    liveMatchBlocksHandoff: false,
  };
  const port = new TauriAccountPort(async <T>(command: string): Promise<T> => {
    assert.equal(command, 'account_status');
    return status as T;
  });
  return {
    port,
    setStatus(next: TestStatus) { status = next; },
  };
}

test('Tauri AccountPort get() returns a stable view reference', async () => {
  const { port } = createPort();
  await port.refreshStatus();

  const first = port.get();
  assert.strictEqual(port.get(), first);
});

test('Tauri AccountPort getState() returns the same snapshot until it changes', async () => {
  const { port } = createPort();
  await port.refreshStatus();

  const first = port.getState();
  assert.strictEqual(port.getState(), first);
});

test('Tauri AccountPort view changes with status and is stable afterward', async () => {
  const { port, setStatus } = createPort();
  await port.refreshStatus();
  const before = port.get();

  setStatus({
    state: 'signed_in',
    displayName: 'Mali',
    avatarUrl: 'https://images.example/avatar.png',
    liveMatchBlocksHandoff: false,
  });
  await port.refreshStatus();

  const after = port.get();
  assert.notStrictEqual(after, before);
  assert.deepEqual(after, {
    state: 'online',
    displayName: 'Mali',
    avatar: 'https://images.example/avatar.png',
    canSignIn: false,
    canSignOut: true,
    friendsAvailable: false,
  });
  assert.strictEqual(port.get(), after);
});

test('Tauri AccountPort does not notify listeners when live-match state is unchanged', async () => {
  const { port } = createPort();
  await port.refreshStatus();
  let notifications = 0;
  port.subscribe(() => { notifications += 1; });

  port.setLiveMatch(false);

  assert.equal(notifications, 0);
});
