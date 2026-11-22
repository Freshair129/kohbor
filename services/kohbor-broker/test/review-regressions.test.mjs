import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  createHash,
  createHmac,
  generateKeyPairSync,
  randomBytes,
  randomUUID,
  sign,
} from 'node:crypto';
import { mkdir, rm, rmdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createBroker } from '../src/broker.mjs';
import { createEncryptedDeletionLedger } from '../src/deletion-ledger.mjs';

const identityKeys = generateKeyPairSync('rsa', { modulusLength: 2048 });
const signingKeys = generateKeyPairSync('ed25519');
const deviceKeys = generateKeyPairSync('ed25519');
const identity = { issuer: 'synthetic-issuer', audience: 'synthetic-client', publicKey: identityKeys.publicKey };
const pepper = 'synthetic-regression-pepper';
const now = 1_800_000_000_000;
const accountId = randomUUID();
const sessionId = randomUUID();
const deviceThumbprint = createHash('sha256')
  .update(deviceKeys.publicKey.export({ type: 'spki', format: 'der' })).digest('hex');
const subject = `rollback-${randomUUID()}`;
const subjectHash = createHmac('sha256', pepper)
  .update(JSON.stringify([identity.issuer, subject])).digest('hex');
const accountCreatedAt = new Date(now - 60_000);

function encode(value) {
  return Buffer.from(JSON.stringify(value)).toString('base64url');
}

function makeJwt(claims, algorithm, key) {
  const header = encode({ alg: algorithm, typ: 'JWT' });
  const payload = encode(claims);
  const input = `${header}.${payload}`;
  const signature = algorithm === 'EdDSA'
    ? sign(null, Buffer.from(input), key)
    : sign('RSA-SHA256', Buffer.from(input), key);
  return `${input}.${signature.toString('base64url')}`;
}

const accessToken = makeJwt({
  iss: 'kohbor-account-token-v1', aud: 'broker', sub: sessionId, dkh: deviceThumbprint,
  iat: Math.floor(now / 1000), exp: Math.floor(now / 1000) + 900,
}, 'EdDSA', signingKeys.privateKey);

function makePool({ commitFailure = false, displayConstraintFailure = false } = {}) {
  const queries = [];
  let accountExists = true;
  let transactionAccountExists = true;
  const sessionRow = { account_id: accountId, device_key_thumbprint: deviceThumbprint };
  const deviceRow = {
    device_key_id: randomUUID(),
    device_public_key: deviceKeys.publicKey.export({ type: 'spki', format: 'pem' }),
  };
  const pool = {
    queries,
    get accountExists() { return accountExists; },
    async query(sql, values = []) {
      queries.push({ sql, values });
      if (sql.includes("to_regclass('public.accounts')")) {
        return { rows: [{ accounts: 'accounts', login_identities: 'login_identities', schema_migrations: 'schema_migrations' }] };
      }
      if (sql.includes('SELECT s.account_id, d.device_key_thumbprint')) {
        return { rows: [sessionRow], rowCount: 1 };
      }
      if (sql.includes('UPDATE accounts SET display_name')) {
        if (displayConstraintFailure) {
          const error = new Error('database detail contains display-name-marker-secret');
          error.code = '23514';
          error.detail = 'display-name-marker-secret';
          throw error;
        }
        return { rows: [{ display_name: values[1] }], rowCount: 1 };
      }
      if (sql.includes('DELETE FROM accounts a')) {
        const [matchingHash, deletedAt] = values;
        const shouldDelete = accountExists && matchingHash === subjectHash && accountCreatedAt < new Date(deletedAt);
        if (shouldDelete) accountExists = false;
        return { rows: [], rowCount: shouldDelete ? 1 : 0 };
      }
      return { rows: [], rowCount: 0 };
    },
    async connect() {
      return {
        async query(sql, values = []) {
          queries.push({ sql, values });
          if (sql === 'BEGIN') {
            transactionAccountExists = accountExists;
            return { rows: [], rowCount: 0 };
          }
          if (sql === 'COMMIT') {
            if (commitFailure) throw new Error('injected commit failure');
            accountExists = transactionAccountExists;
            return { rows: [], rowCount: 0 };
          }
          if (sql === 'ROLLBACK') {
            transactionAccountExists = accountExists;
            return { rows: [], rowCount: 0 };
          }
          if (sql.includes('SELECT d.id AS device_key_id')) {
            return { rows: [deviceRow], rowCount: 1 };
          }
          if (sql.includes('SET consumed_at = $4') && sql.includes('RETURNING id')) {
            return { rows: [{ id: randomUUID() }], rowCount: 1 };
          }
          if (sql.includes('SELECT account_id, subject_hash')) {
            return { rows: [{ account_id: accountId, subject_hash: subjectHash }], rowCount: 1 };
          }
          if (sql.includes('SELECT session_id') && sql.includes('FROM sessions s')) {
            return { rows: [{ session_id: sessionId }], rowCount: 1 };
          }
          if (sql.startsWith('DELETE FROM accounts WHERE id')) {
            transactionAccountExists = false;
            return { rows: [], rowCount: 1 };
          }
          if (sql.includes('SELECT count(*)::integer AS attempts')) {
            return { rows: [{ attempts: 1, window_start: new Date(now) }], rowCount: 1 };
          }
          return { rows: [], rowCount: 1 };
        },
        release() {},
      };
    },
  };
  return pool;
}

async function withBroker(pool, deletionLedger, callback) {
  const server = createBroker({
    pool,
    identity,
    signingPrivateKey: signingKeys.privateKey,
    pepper,
    deletionLedger,
    clock: () => now,
  });
  await new Promise((resolveListen, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolveListen));
  const origin = `http://127.0.0.1:${server.address().port}`;
  try {
    return await callback(origin);
  } finally {
    await new Promise((resolveClose, reject) => server.close(error => error ? reject(error) : resolveClose()));
  }
}

async function sendJson(origin, path, body, token, method = 'POST') {
  const response = await fetch(`${origin}${path}`, {
    method,
    headers: {
      'content-type': 'application/json',
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(body),
  });
  return { status: response.status, body: await response.json() };
}

test('delete commit failure compensates its ledger entry before later replay', async () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), `.tmp-rollback-${process.pid}-${randomUUID()}`);
  await mkdir(root);
  const ledgerPath = resolve(root, 'deletions.enc');
  const ledger = createEncryptedDeletionLedger({ filePath: ledgerPath, pepper });
  const pool = makePool({ commitFailure: true });
  const nonce = randomBytes(32).toString('base64url');
  const nowSeconds = Math.floor(now / 1000);
  const idToken = makeJwt({
    iss: identity.issuer, aud: identity.audience, sub: subject, nonce,
    iat: nowSeconds, exp: nowSeconds + 300,
  }, 'RS256', identityKeys.privateKey);
  const deviceProof = sign(null,
    Buffer.from(`kohbor-broker-proof-v1\ndelete\n${nonce}`), deviceKeys.privateKey).toString('base64url');
  try {
    const response = await withBroker(pool, ledger, origin => sendJson(origin, '/v1/account', {
      idToken, nonce, deviceProof,
    }, accessToken, 'DELETE'));
    assert.equal(response.status, 503);
    assert.equal(pool.queries.some(query => query.sql === 'COMMIT'), true,
      `the injected commit failure must be exercised: ${pool.queries.map(query => query.sql).join(' | ')}`);
    assert.equal(pool.queries.some(query => query.sql.startsWith('DELETE FROM accounts WHERE id')), true,
      `the transactional account delete must be staged before commit fails: ${pool.queries.map(query => query.sql).join(' | ')}`);
    assert.equal(pool.accountExists, true);
    assert.deepEqual(await ledger.entries(), []);
    assert.equal(await ledger.replay(pool), 0);
    assert.equal(pool.accountExists, true);
  } finally {
    await rm(ledgerPath, { force: true });
    await rmdir(root);
  }
});

test('display-name validation rejects 32 graphemes that exceed the SQL code-point limit', async () => {
  const pool = makePool();
  const cluster = `a${'\u20dd'.repeat(4)}`;
  const displayName = cluster.repeat(32);
  assert.equal([...displayName].length, 160);
  assert.equal(Buffer.byteLength(displayName, 'utf8'), 416);
  assert.equal([...new Intl.Segmenter('und', { granularity: 'grapheme' }).segment(displayName)].length, 32);
  const response = await withBroker(pool, { async append() {}, async remove() { return true; } }, origin => sendJson(
    origin, '/v1/account/display-name', { displayName }, accessToken, 'PUT'));
  assert.deepEqual(response, { status: 400, body: { error: 'invalid_display_name', reason: 'too_long' } });
});

test('database name constraint failures return a safe validation response without logging details', async () => {
  const pool = makePool({ displayConstraintFailure: true });
  const logs = [];
  const originalError = console.error;
  console.error = (...values) => logs.push(values.join(' '));
  try {
    const response = await withBroker(pool, { async append() {}, async remove() { return true; } }, origin => sendJson(
      origin, '/v1/account/display-name', { displayName: 'valid name' }, accessToken, 'PUT'));
    assert.equal(response.status, 400);
    assert.equal(response.body.error, 'invalid_display_name');
    assert.equal(JSON.stringify(response).includes('display-name-marker-secret'), false);
    assert.deepEqual(logs, []);
  } finally {
    console.error = originalError;
  }
});

test('unauthenticated requests do not run expired-row purge deletes', async () => {
  const pool = makePool();
  const before = pool.queries.filter(query => query.sql.trimStart().startsWith('DELETE')).length;
  await withBroker(pool, { async append() {}, async remove() { return true; } }, async origin => {
    assert.deepEqual(await fetch(`${origin}/v1/account`).then(async response => ({
      status: response.status, body: await response.json(),
    })), { status: 401, body: { error: 'unauthorized' } });
    const devicePublicKey = deviceKeys.publicKey.export({ type: 'spki', format: 'pem' });
    const nonceResponse = await sendJson(origin, '/v1/auth/nonces', {
      purpose: 'sign_in', devicePublicKey,
    });
    assert.equal(nonceResponse.status, 200);
  });
  const after = pool.queries.filter(query => query.sql.trimStart().startsWith('DELETE')).length;
  assert.equal(after, before);
});
