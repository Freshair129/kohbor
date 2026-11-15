import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign, verify, randomUUID, createHmac } from 'node:crypto';
import { createBroker } from '../src/broker.mjs';
import { migrate } from '../src/migrate.mjs';
import pg from 'pg';

// Synthetic identities and keys exist only in this process. No external IdP is used.
const { Pool } = pg;
const pool = new Pool({ connectionString: process.env.TEST_DATABASE_URL });
const identityKeys = generateKeyPairSync('rsa', { modulusLength: 2048 });
const signing = generateKeyPairSync('ed25519');
const identity = { issuer: 'synthetic-issuer', audience: 'synthetic-client', publicKey: identityKeys.publicKey };
const pepper = 'synthetic-test-pepper-only';
let now = 1_800_000_000_000;
let server;
let origin;

const encode = value => Buffer.from(JSON.stringify(value)).toString('base64url');
function idToken(sub, nonce, changes = {}, key = identityKeys.privateKey) {
  const head = encode({ alg: 'RS256', typ: 'JWT' });
  const body = encode({ iss: identity.issuer, aud: identity.audience, sub, nonce,
    iat: Math.floor(now / 1000), exp: Math.floor(now / 1000) + 300, ...changes });
  const input = `${head}.${body}`;
  return `${input}.${sign('RSA-SHA256', Buffer.from(input), key).toString('base64url')}`;
}
function device() {
  const keys = generateKeyPairSync('ed25519');
  return { ...keys, publicText: keys.publicKey.export({ type: 'spki', format: 'pem' }) };
}
function proof(d, purpose, nonce) {
  return sign(null, Buffer.from(`kohbor-broker-proof-v1\n${purpose}\n${nonce}`), d.privateKey).toString('base64url');
}
async function request(path, body, token, method = 'POST') {
  const response = await fetch(`${origin}${path}`, {
    method,
    headers: { ...(body === undefined ? {} : { 'content-type': 'application/json' }),
      ...(token ? { authorization: `Bearer ${token}` } : {}) },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  return { status: response.status, body: await response.json() };
}
async function challenge(d, purpose) {
  const r = await request('/v1/auth/nonces', { devicePublicKey: d.publicText, purpose });
  assert.equal(r.status, 200);
  assert.equal(typeof r.body.nonce, 'string');
  return r.body.nonce;
}
async function login(sub, d = device(), changes = {}, key) {
  const nonce = await challenge(d, 'sign_in');
  const body = { idToken: idToken(sub, nonce, changes, key), devicePublicKey: d.publicText,
    nonce, deviceProof: proof(d, 'sign_in', nonce) };
  return { ...(await request('/v1/auth/sessions', body)), d, nonce, requestBody: body };
}
async function refresh(session, d = session.d) {
  const nonce = await challenge(d, 'refresh');
  return request('/v1/auth/refresh', { refreshToken: session.body.refreshToken,
    devicePublicKey: d.publicText, nonce, deviceProof: proof(d, 'refresh', nonce) });
}
async function deleteAccount(session, sub) {
  const nonce = await challenge(session.d, 'delete');
  return request('/v1/account', { idToken: idToken(sub, nonce), nonce,
    deviceProof: proof(session.d, 'delete', nonce) }, session.body.accessToken, 'DELETE');
}
async function rows() {
  const tables = await pool.query(`SELECT table_name FROM information_schema.tables
    WHERE table_schema = 'public' AND table_type = 'BASE TABLE' ORDER BY table_name`);
  const result = {};
  for (const { table_name: name } of tables.rows) {
    const quoted = `"${name.replaceAll('"', '""')}"`;
    result[name] = (await pool.query(`SELECT * FROM ${quoted}`)).rows;
  }
  return result;
}

before(async () => {
  assert.ok(process.env.TEST_DATABASE_URL, 'TEST_DATABASE_URL must name a disposable synthetic Postgres database');
  await migrate(pool);
  server = await createBroker({ pool, identity, signingPrivateKey: signing.privateKey,
    pepper, clock: () => now, limits: { attempts: 1000, windowMs: 60_000 } });
  await new Promise((resolve, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
});
after(async () => {
  if (server?.listening) await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  await pool.end();
});

test('health tracer and database readiness', async () => {
  const health = await request('/health', undefined, undefined, 'GET');
  assert.equal(health.status, 200);
});

test('only verified one-use, device-bound identity can create a session', async () => {
  const good = await login(`subject-${randomUUID()}`);
  assert.equal(good.status, 200);
  assert.equal(typeof good.body.accessToken, 'string');
  assert.equal(typeof good.body.refreshToken, 'string');
  const [header, payload, signature] = good.body.accessToken.split('.');
  assert.equal(JSON.parse(Buffer.from(header, 'base64url')).alg, 'EdDSA');
  assert.equal(JSON.parse(Buffer.from(payload, 'base64url')).aud, 'broker');
  assert.ok(verify(null, Buffer.from(`${header}.${payload}`), signing.publicKey, Buffer.from(signature, 'base64url')));
  const claims = JSON.parse(Buffer.from(payload, 'base64url'));
  for (const changes of [{ iss: 'other-product' }, { aud: 'relay' }, { aud: ['broker', 'relay'] }]) {
    const input = `${header}.${encode({ ...claims, ...changes })}`;
    const token = `${input}.${sign(null, Buffer.from(input), signing.privateKey).toString('base64url')}`;
    assert.equal((await request('/v1/account/export', undefined, token, 'GET')).status, 401);
  }
  assert.equal((await request('/v1/auth/sessions', good.requestBody)).status, 401);
  const otherKey = generateKeyPairSync('rsa', { modulusLength: 2048 }).privateKey;
  for (const [change, key] of [[{}, otherKey], [{ iss: 'wrong-issuer' }],
    [{ aud: 'wrong-client' }], [{ aud: [identity.audience, 'other'] }],
    [{ exp: Math.floor(now / 1000) }], [{ nonce: 'wrong-nonce' }]]) {
    assert.equal((await login(`bad-${randomUUID()}`, device(), change, key)).status, 401);
  }
  const a = device();
  const b = device();
  const nonce = await challenge(a, 'sign_in');
  const invalid = await request('/v1/auth/sessions', { idToken: idToken('device-mismatch', nonce),
    devicePublicKey: a.publicText, nonce, deviceProof: proof(b, 'sign_in', nonce) });
  assert.equal(invalid.status, 401);
});

test('migrated persistence excludes identity and forbidden private fields', async () => {
  const subject = `private-subject-${randomUUID()}`;
  const session = await login(subject);
  assert.equal(session.status, 200);
  const snapshot = await rows();
  const all = JSON.stringify(snapshot);
  assert.ok(!all.includes(subject));
  assert.ok(!all.includes(session.body.refreshToken));
  assert.ok(!all.includes(session.requestBody.idToken));
  const columns = await pool.query(`SELECT table_name, column_name FROM information_schema.columns WHERE table_schema = 'public'`);
  // A closed list forces migrations introducing new persistent data through review.
  const allowedColumns = new Set(['version', 'name', 'applied_at', 'id', 'account_id',
    'provider', 'subject_hash', 'friend_code', 'display_name', 'settings', 'public_key',
    'device_public_key', 'device_key_thumbprint', 'session_id', 'refresh_hash',
    'previous_refresh_hash', 'revoked', 'created_at', 'expires_at', 'updated_at',
    'nonce_hash', 'purpose', 'consumed_at', 'attempts', 'window_start', 'owner_id',
    'friend_id', 'requester_id', 'recipient_id', 'blocked_id', 'blocker_id',
    'pairing_id', 'pairing_generation', 'scope', 'scopes', 'paused', 'last_confirmed_at']);
  assert.deepEqual(columns.rows.filter(row => !allowedColumns.has(row.column_name)), []);
  // The broker must persist a keyed identity, never the raw provider subject.
  const digest = createHmac('sha256', pepper).update(JSON.stringify([identity.issuer, subject])).digest('hex');
  assert.ok(all.includes(digest), 'expected HMAC-SHA256(pepper, JSON issuer/subject tuple) in persisted rows');
});

test('rotating refresh is device-bound and reuse revokes the session', async () => {
  const session = await login(`refresh-${randomUUID()}`);
  assert.equal(session.status, 200);
  assert.equal((await refresh(session, device())).status, 401);
  const rotated = await refresh(session);
  assert.equal(rotated.status, 200);
  assert.notEqual(rotated.body.refreshToken, session.body.refreshToken);
  assert.equal((await refresh(session, device())).status, 401);
  assert.equal((await request('/v1/account/export', undefined, rotated.body.accessToken, 'GET')).status, 200);
  assert.equal((await refresh(session)).status, 401);
  assert.equal((await refresh({ body: rotated.body, d: session.d })).status, 401);
});

test('new device pauses only the account owner share entries and advances their generation', async () => {
  const sub = `new-device-${randomUUID()}`;
  const first = await login(sub);
  const second = await login(`counterpart-${randomUUID()}`);
  assert.equal(first.status, 200);
  assert.equal(second.status, 200);
  const accountA = JSON.parse(Buffer.from(first.body.accessToken.split('.')[1], 'base64url')).sub;
  const accountB = JSON.parse(Buffer.from(second.body.accessToken.split('.')[1], 'base64url')).sub;
  await pool.query(`INSERT INTO share_entries (account_id, friend_id, pairing_id, pairing_generation, scopes, paused)
    VALUES ($1, $2, $3, 7, $4, false), ($2, $1, $5, 12, $4, false)`,
  [accountA, accountB, randomUUID(), JSON.stringify(['health']), randomUUID()]);
  const shareState = async () => (await pool.query(
    'SELECT account_id, paused, pairing_generation FROM share_entries WHERE account_id = ANY($1::uuid[]) ORDER BY account_id',
    [[accountA, accountB]],
  )).rows;
  const before = await shareState();
  assert.equal((await login(sub, first.d)).status, 200);
  assert.deepEqual(await shareState(), before, 'same device must not change existing consent');
  assert.equal((await login(sub, device())).status, 200);
  const after = await shareState();
  const aBefore = before.find(row => row.account_id === accountA);
  const bBefore = before.find(row => row.account_id === accountB);
  const aAfter = after.find(row => row.account_id === accountA);
  const bAfter = after.find(row => row.account_id === accountB);
  assert.equal(aAfter.paused, true, 'new device must pause the account owner share');
  assert.equal(BigInt(aAfter.pairing_generation), BigInt(aBefore.pairing_generation) + 1n);
  assert.deepEqual(bAfter, bBefore, 'counterpart pause preference and generation stay unchanged');
});

test('delete requires fresh matching identity and preserves another account', async () => {
  const firstSub = `delete-${randomUUID()}`;
  const secondSub = `preserve-${randomUUID()}`;
  const first = await login(firstSub);
  const second = await login(secondSub);
  assert.equal(first.status, 200);
  assert.equal(second.status, 200);
  const firstAccount = JSON.parse(Buffer.from(first.body.accessToken.split('.')[1], 'base64url')).sub;
  const secondAccount = JSON.parse(Buffer.from(second.body.accessToken.split('.')[1], 'base64url')).sub;
  assert.equal(typeof firstAccount, 'string');
  assert.equal(typeof secondAccount, 'string');
  assert.notEqual(firstAccount, secondAccount);
  // Seed both directions explicitly: the S2 slice does not yet expose social mutation endpoints.
  await pool.query('INSERT INTO friendships (account_id, friend_id) VALUES ($1, $2), ($2, $1)', [firstAccount, secondAccount]);
  await pool.query(`INSERT INTO share_entries (account_id, friend_id, pairing_id, scopes, paused)
    VALUES ($1, $2, $3, $4, false), ($2, $1, $5, $4, false)`,
  [firstAccount, secondAccount, randomUUID(), JSON.stringify(['health']), randomUUID()]);
  const beforeDelete = await rows();
  const countAccountRows = (snapshot, account) => Object.fromEntries(Object.entries(snapshot)
    .map(([table, records]) => [table, records.filter(record => Object.values(record).some(value => value === account)).length]));
  assert.ok(Object.values(countAccountRows(beforeDelete, firstAccount)).some(count => count > 0));
  assert.equal((await deleteAccount(first, secondSub)).status, 401);
  const oldNonce = await challenge(first.d, 'delete');
  const stale = await request('/v1/account', { idToken: idToken(firstSub, oldNonce,
    { iat: Math.floor(now / 1000) - 301 }), nonce: oldNonce,
    deviceProof: proof(first.d, 'delete', oldNonce) }, first.body.accessToken, 'DELETE');
  assert.equal(stale.status, 401);
  const exported = await request('/v1/account/export', undefined, first.body.accessToken, 'GET');
  assert.equal(exported.status, 200);
  const exportText = JSON.stringify(exported.body);
  assert.ok(!exportText.includes(firstSub));
  assert.ok(!exportText.includes(first.body.refreshToken));
  assert.ok(!exportText.includes(first.body.accessToken));
  assert.ok(!/subject_hash|refresh_token|private_key/i.test(exportText));
  assert.equal((await deleteAccount(first, firstSub)).status, 200);
  assert.equal((await request('/v1/account/export', undefined, first.body.accessToken, 'GET')).status, 401);
  assert.equal((await request('/v1/account/export', undefined, second.body.accessToken, 'GET')).status, 200);
  const afterDelete = await rows();
  assert.ok(Object.values(countAccountRows(afterDelete, firstAccount)).every(count => count === 0),
    'no migrated table may retain a row referencing the deleted account');
  assert.ok(Object.values(countAccountRows(afterDelete, secondAccount)).some(count => count > 0));
  const snapshot = JSON.stringify(afterDelete);
  const firstDigest = createHmac('sha256', pepper).update(JSON.stringify([identity.issuer, firstSub])).digest('hex');
  const secondDigest = createHmac('sha256', pepper).update(JSON.stringify([identity.issuer, secondSub])).digest('hex');
  assert.ok(!snapshot.includes(firstDigest));
  assert.ok(snapshot.includes(secondDigest));
});

test('configurable per-account rate limit rejects excess requests', async () => {
  const first = await login(`limit-${randomUUID()}`);
  const second = await login(`limit-other-${randomUUID()}`);
  const limited = await createBroker({ pool, identity, signingPrivateKey: signing.privateKey,
    pepper, clock: () => now, limits: { attempts: 2, windowMs: 60_000 } });
  await new Promise((resolve, reject) => limited.once('error', reject).listen(0, '127.0.0.1', resolve));
  const savedOrigin = origin;
  origin = `http://127.0.0.1:${limited.address().port}`;
  try {
    assert.equal((await request('/v1/account/export', undefined, first.body.accessToken, 'GET')).status, 200);
    assert.equal((await request('/v1/account/export', undefined, first.body.accessToken, 'GET')).status, 200);
    assert.equal((await request('/v1/account/export', undefined, first.body.accessToken, 'GET')).status, 429);
    assert.equal((await request('/v1/account/export', undefined, second.body.accessToken, 'GET')).status, 200);
    now += 60_001;
    assert.equal((await request('/v1/account/export', undefined, first.body.accessToken, 'GET')).status, 200);
  } finally {
    origin = savedOrigin;
    await new Promise((resolve, reject) => limited.close(error => error ? reject(error) : resolve()));
  }
});
