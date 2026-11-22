import { test, before, beforeEach, after } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign, verify, randomUUID, createHash, createHmac } from 'node:crypto';
import { createBroker } from '../src/broker.mjs';
import { createEncryptedDeletionLedger } from '../src/deletion-ledger.mjs';
import { migrate } from '../src/migrate.mjs';
import pg from 'pg';
import { mkdir, rm, rmdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

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
let deletionLedger;
const testRoot = resolve(dirname(fileURLToPath(import.meta.url)), `.tmp-deletion-${process.pid}-${randomUUID()}`);
const ledgerPath = resolve(testRoot, `deletions-${process.pid}.enc`);

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
async function request(path, body, token, method = 'POST', headers = {}) {
  const response = await fetch(`${origin}${path}`, {
    method,
    headers: { ...(body === undefined ? {} : { 'content-type': 'application/json' }),
      ...(token ? { authorization: `Bearer ${token}` } : {}), ...headers },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await response.text();
  const retryAfter = response.headers.get('retry-after');
  return {
    status: response.status,
    body: text ? JSON.parse(text) : null,
    ...(retryAfter === null ? {} : { retryAfter }),
  };
}
async function rawRequest(path, body, headers = {}, method = 'POST', token = undefined) {
  const response = await fetch(`${origin}${path}`, {
    method,
    headers: { ...headers, ...(token ? { authorization: `Bearer ${token}` } : {}) },
    body,
  });
  const text = await response.text();
  return { status: response.status, body: text ? JSON.parse(text) : null };
}
async function challenge(d, purpose) {
  const r = await request('/v1/auth/nonces', { devicePublicKey: d.publicText, purpose });
  assert.equal(r.status, 200);
  assert.equal(typeof r.body.nonce, 'string');
  assert.equal(r.body.expiresIn, purpose === 'refresh' ? 300 : 600);
  return r.body.nonce;
}
async function login(sub, d = device(), changes = {}, key, create = undefined) {
  const nonce = await challenge(d, 'sign_in');
  const body = { idToken: idToken(sub, nonce, changes, key), devicePublicKey: d.publicText,
    nonce, deviceProof: proof(d, 'sign_in', nonce) };
  if (create !== undefined) body.create = create;
  return { ...(await request('/v1/auth/sessions', body)), d, nonce, requestBody: body };
}
async function refresh(session, d = session.d, token = session.body.refreshToken) {
  const nonce = await challenge(d, 'refresh');
  return request('/v1/auth/refresh', { refreshToken: token,
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
async function sessionClaims(session) {
  return JSON.parse(Buffer.from(session.body.accessToken.split('.')[1], 'base64url'));
}
async function accountIdFor(session) {
  const claims = await sessionClaims(session);
  const result = await pool.query('SELECT account_id FROM sessions WHERE session_id = $1', [claims.sub]);
  assert.equal(result.rowCount, 1);
  return result.rows[0].account_id;
}
async function setName(session, displayName) {
  return request('/v1/account/display-name', { displayName }, session.body.accessToken, 'PUT');
}
const digest = value => createHash('sha256').update(value).digest('hex');
const hmacSubject = subject => createHmac('sha256', pepper)
  .update(JSON.stringify([identity.issuer, subject])).digest('hex');
const FRIEND_CODE_ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
function assertValidFriendCode(code) {
  assert.match(code, /^KB-[0-9A-HJKMNP-TV-Z]{10}$/);
  const data = code.slice(3, 12);
  const check = [...data].reduce((total, character, index) =>
    (total + (index * 2 + 1) * FRIEND_CODE_ALPHABET.indexOf(character)) % 32, 0);
  assert.equal(code[12], FRIEND_CODE_ALPHABET[check]);
}
const markerRows = (snapshot, markers) => Object.fromEntries(Object.entries(snapshot).map(([table, records]) => [
  table,
  records.filter(record => Object.values(record).some(value =>
    value !== null && markers.has(String(value).trimEnd()))),
]));
const EXPECTED_TABLES = [
  'accounts', 'auth_nonces', 'blocks', 'device_keys', 'friendships', 'global_rate_counters',
  'login_identities', 'pending_requests', 'rate_counters', 'rate_limit_events', 'refresh_tokens',
  'schema_migrations', 'sessions', 'share_entries',
];

before(async () => {
  assert.ok(process.env.TEST_DATABASE_URL, 'TEST_DATABASE_URL must name a disposable synthetic Postgres database');
  await mkdir(testRoot);
  deletionLedger = createEncryptedDeletionLedger({ filePath: ledgerPath, pepper });
  await migrate(pool);
  server = await createBroker({ pool, identity, signingPrivateKey: signing.privateKey,
    pepper, deletionLedger, clock: () => now });
  await new Promise((resolve, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
});
beforeEach(() => { now += 60_001; });
after(async () => {
  if (server?.listening) await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  await pool.end();
  await rm(ledgerPath, { force: true });
  await rmdir(testRoot);
});

test('health tracer and database readiness', async () => {
  const health = await request('/health', undefined, undefined, 'GET');
  assert.equal(health.status, 200);
});

test('sign-in nonces survive the browser hand-off and expired nonce rows are purged', async () => {
  const d = device();
  const nonce = await challenge(d, 'sign_in');
  now += 5 * 60 * 1000 + 1;
  const completed = await request('/v1/auth/sessions', {
    idToken: idToken(`handoff-${randomUUID()}`, nonce),
    nonce,
    devicePublicKey: d.publicText,
    deviceProof: proof(d, 'sign_in', nonce),
  });
  assert.equal(completed.status, 200);
  assert.equal(completed.body.profile.displayName, null);

  const expired = await challenge(device(), 'sign_in');
  now += 10 * 60 * 1000 + 1;
  const next = await challenge(device(), 'sign_in');
  assert.equal(typeof next, 'string');
  // Expired rows are purged only after authentication (review finding 4), so trigger it with an authenticated call.
  assert.equal((await request('/v1/account', undefined, completed.body.accessToken, 'GET')).status, 200);
  assert.equal((await pool.query('SELECT count(*)::integer AS count FROM auth_nonces WHERE nonce_hash = $1',
    [digest(expired)])).rows[0].count, 0);
});

test('only verified one-use, device-bound identity can create a session', async () => {
  const good = await login(`subject-${randomUUID()}`);
  assert.equal(good.status, 200);
  assert.equal(typeof good.body.accessToken, 'string');
  assert.equal(typeof good.body.refreshToken, 'string');
  assertValidFriendCode(good.body.profile.friendCode);
  const [header, payload, signature] = good.body.accessToken.split('.');
  assert.equal(JSON.parse(Buffer.from(header, 'base64url')).alg, 'EdDSA');
  assert.equal(JSON.parse(Buffer.from(payload, 'base64url')).aud, 'broker');
  assert.ok(verify(null, Buffer.from(`${header}.${payload}`), signing.publicKey, Buffer.from(signature, 'base64url')));
  const claims = JSON.parse(Buffer.from(payload, 'base64url'));
  assert.equal(claims.sub.length, 36);
  assert.equal(typeof claims.dkh, 'string');
  assert.equal(Object.hasOwn(claims, 'sid'), false);
  assert.equal(Object.hasOwn(claims, 'accountId'), false);
  assert.equal(claims.exp - claims.iat, 15 * 60);
  const ownProfile = await request('/v1/account', undefined, good.body.accessToken, 'GET');
  assert.equal(ownProfile.status, 200);
  assert.equal(Object.hasOwn(ownProfile.body, 'accountId'), false);
  assert.equal(good.body.profile.displayName, null);
  assert.equal(Object.hasOwn(good.body, 'accountId'), false);
  const accessDenials = [];
  for (const changes of [{ iss: 'other-product' }, { aud: 'relay' }, { aud: ['broker', 'relay'] }]) {
    const input = `${header}.${encode({ ...claims, ...changes })}`;
    const token = `${input}.${sign(null, Buffer.from(input), signing.privateKey).toString('base64url')}`;
    const denial = await request('/v1/account/export', undefined, token, 'GET');
    accessDenials.push(denial);
    assert.equal(denial.status, 401);
  }
  const reusedNonce = await request('/v1/auth/sessions', good.requestBody);
  accessDenials.push(reusedNonce);
  assert.equal(reusedNonce.status, 401);
  const otherKey = generateKeyPairSync('rsa', { modulusLength: 2048 }).privateKey;
  for (const [change, key] of [[{}, otherKey], [{ iss: 'wrong-issuer' }],
    [{ aud: 'wrong-client' }], [{ aud: [identity.audience, 'other'] }],
    [{ exp: Math.floor(now / 1000) }], [{ nonce: 'wrong-nonce' }]]) {
    const failure = await login(`bad-${randomUUID()}`, device(), change, key);
    accessDenials.push({ status: failure.status, body: failure.body });
    assert.equal(failure.status, 401);
  }
  const a = device();
  const b = device();
  const nonce = await challenge(a, 'sign_in');
  const invalid = await request('/v1/auth/sessions', { idToken: idToken('device-mismatch', nonce),
    devicePublicKey: a.publicText, nonce, deviceProof: proof(b, 'sign_in', nonce) });
  accessDenials.push(invalid);
  assert.equal(invalid.status, 401);
  assert.ok(accessDenials.every(failure =>
    JSON.stringify(failure) === JSON.stringify({ status: 401, body: { error: 'unauthorized' } })));
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
    'provider', 'subject_hash', 'hash_key_id', 'friend_code', 'display_name', 'settings',
    'public_key', 'device_public_key', 'device_key_thumbprint', 'device_key_id', 'session_id',
    'token_hash', 'generation', 'rotated_at', 'created_at', 'expires_at', 'updated_at',
    'nonce_hash', 'purpose', 'consumed_at', 'attempts', 'window_start', 'occurred_at', 'owner_id',
    'friend_id', 'requester_id', 'recipient_id', 'blocked_id', 'blocker_id',
    'pairing_id', 'pairing_generation', 'scope', 'scopes', 'paused', 'last_confirmed_at']);
  assert.deepEqual(columns.rows.filter(row => !allowedColumns.has(row.column_name)), []);
  assert.equal(columns.rows.some(row => ['refresh_hash', 'previous_refresh_hash', 'revoked'].includes(row.column_name)), false);
  assert.equal((await pool.query('SELECT name FROM schema_migrations WHERE version = 2')).rows[0].name,
    'registration_hardening');
  assert.equal((await pool.query('SELECT name FROM schema_migrations WHERE version = 3')).rows[0].name,
    'expired_row_purge_indexes');
  const purgeIndexes = await pool.query(
    `SELECT indexname FROM pg_indexes
      WHERE schemaname = 'public' AND indexname = ANY($1::text[])
      ORDER BY indexname`,
    [['auth_nonces_expiry_idx', 'rate_counters_window_start_idx', 'rate_limit_events_occurred_at_idx']],
  );
  assert.deepEqual(purgeIndexes.rows.map(row => row.indexname), [
    'auth_nonces_expiry_idx', 'rate_counters_window_start_idx', 'rate_limit_events_occurred_at_idx',
  ]);
  // The broker must persist a keyed identity, never the raw provider subject.
  const digest = createHmac('sha256', pepper).update(JSON.stringify([identity.issuer, subject])).digest('hex');
  assert.ok(all.includes(digest), 'expected HMAC-SHA256(pepper, JSON issuer/subject tuple) in persisted rows');
});

test('rotating refresh is device-bound and reuse revokes the session', async () => {
  const session = await login(`refresh-${randomUUID()}`);
  assert.equal(session.status, 200);
  assert.equal((await refresh(session, device())).status, 401);
  const previousToken = session.body.refreshToken;
  const rotated = await refresh(session);
  assert.equal(rotated.status, 200);
  assert.notEqual(rotated.body.refreshToken, previousToken);
  session.body = rotated.body;
  assert.equal((await request('/v1/account', undefined, rotated.body.accessToken, 'GET')).status, 200);
  assert.equal((await refresh(session, device(), previousToken)).status, 401);
  assert.equal((await request('/v1/account', undefined, rotated.body.accessToken, 'GET')).status, 401);
  assert.equal((await refresh(session)).status, 401);
});

test('refresh idle expiry extends 30 days and stops at the 90-day absolute cap', async () => {
  const session = await login(`refresh-cap-${randomUUID()}`);
  const claims = await sessionClaims(session);
  const createdAt = (await pool.query('SELECT created_at FROM sessions WHERE session_id = $1', [claims.sub])).rows[0]
    .created_at.getTime();
  for (const [day, expectedExpiryDay] of [[29, 59], [58, 88], [87, 90]]) {
    now = createdAt + day * 24 * 60 * 60 * 1000;
    const rotated = await refresh(session);
    assert.equal(rotated.status, 200);
    session.body = rotated.body;
    const expiry = (await pool.query('SELECT expires_at FROM sessions WHERE session_id = $1', [claims.sub])).rows[0]
      .expires_at.getTime();
    assert.equal(expiry, createdAt + expectedExpiryDay * 24 * 60 * 60 * 1000);
  }
  now = createdAt + 90 * 24 * 60 * 60 * 1000 + 1;
  assert.deepEqual(await refresh(session), { status: 401, body: { error: 'unauthorized' } });
});

test('new device pauses only the account owner share entries and advances their generation', async () => {
  const sub = `new-device-${randomUUID()}`;
  const first = await login(sub);
  const second = await login(`counterpart-${randomUUID()}`);
  assert.equal(first.status, 200);
  assert.equal(second.status, 200);
  const accountA = await accountIdFor(first);
  const accountB = await accountIdFor(second);
  await pool.query(`INSERT INTO share_entries (account_id, friend_id, pairing_id, pairing_generation, scopes, paused)
    VALUES ($1, $2, $3, 7, $4, false), ($2, $1, $5, 12, $4, false)`,
  [accountA, accountB, randomUUID(), JSON.stringify(['health']), randomUUID()]);
  const shareState = async () => (await pool.query(
    'SELECT account_id, paused, pairing_generation FROM share_entries WHERE account_id = ANY($1::uuid[]) ORDER BY account_id',
    [[accountA, accountB]],
  )).rows;
  const before = await shareState();
  const sameDevice = await login(sub, first.d);
  assert.equal(sameDevice.status, 200);
  assert.equal(sameDevice.body.newDevice, false);
  assert.deepEqual(await request('/v1/account', undefined, first.body.accessToken, 'GET'),
    { status: 401, body: { error: 'unauthorized' } });
  assert.deepEqual(await shareState(), before, 'same device must not change existing consent');
  const newDevice = await login(sub, device());
  assert.equal(newDevice.status, 200);
  assert.equal(newDevice.body.newDevice, true);
  const after = await shareState();
  const aBefore = before.find(row => row.account_id === accountA);
  const bBefore = before.find(row => row.account_id === accountB);
  const aAfter = after.find(row => row.account_id === accountA);
  const bAfter = after.find(row => row.account_id === accountB);
  assert.equal(aAfter.paused, true, 'new device must pause the account owner share');
  assert.equal(BigInt(aAfter.pairing_generation), BigInt(aBefore.pairing_generation) + 1n);
  assert.deepEqual(bAfter, bBefore, 'counterpart pause preference and generation stay unchanged');
});

test('delete counts every table by every account marker and replay cannot resurrect it', async () => {
  const firstSub = `delete-${randomUUID()}`;
  const secondSub = `preserve-${randomUUID()}`;
  const thirdSub = `survive-${randomUUID()}`;
  const first = await login(firstSub);
  const signedOut = await login(firstSub, device());
  const second = await login(secondSub);
  const third = await login(thirdSub);
  assert.equal(first.status, 200);
  assert.equal(signedOut.status, 200);
  assert.equal(second.status, 200);
  assert.equal(third.status, 200);

  const firstAccount = await accountIdFor(first);
  const secondAccount = await accountIdFor(second);
  const thirdAccount = await accountIdFor(third);
  const firstFriendCode = first.body.profile.friendCode;
  const markerA = `delete-m-${randomUUID().slice(0, 8)}`;
  const markerB = `preserve-m-${randomUUID().slice(0, 8)}`;
  const markerC = `survive-m-${randomUUID().slice(0, 8)}`;
  assert.equal((await setName(first, markerA)).status, 200);
  assert.equal((await setName(second, markerB)).status, 200);
  assert.equal((await setName(third, markerC)).status, 200);

  const relationshipTime = new Date(now);
  const requestExpiry = new Date(now + 30 * 24 * 60 * 60 * 1000);
  await pool.query('INSERT INTO friendships (account_id, friend_id) VALUES ($1, $2), ($2, $1), ($2, $3), ($3, $2)',
    [firstAccount, secondAccount, thirdAccount]);
  await pool.query(`INSERT INTO pending_requests (id, requester_id, recipient_id, created_at, expires_at)
    VALUES ($1, $2, $3, $4, $5), ($6, $3, $2, $4, $5),
           ($7, $3, $8, $4, $5), ($9, $8, $3, $4, $5)`,
  [randomUUID(), firstAccount, secondAccount, relationshipTime, requestExpiry,
    randomUUID(), randomUUID(), thirdAccount, randomUUID()]);
  await pool.query('INSERT INTO blocks (blocker_id, blocked_id, created_at) VALUES ($1, $2, $3), ($2, $1, $3), ($2, $4, $3), ($4, $2, $3)',
    [firstAccount, secondAccount, relationshipTime, thirdAccount]);
  await pool.query(`INSERT INTO share_entries (account_id, friend_id, pairing_id, scopes, paused)
    VALUES ($1, $2, $3, $4, false), ($2, $1, $5, $4, false),
           ($2, $6, $7, $4, false), ($6, $2, $8, $4, false)`,
  [firstAccount, secondAccount, randomUUID(), JSON.stringify(['health']), randomUUID(),
    thirdAccount, randomUUID(), randomUUID()]);

  const rotated = await request('/v1/account/friend-code', {}, first.body.accessToken);
  assert.equal(rotated.status, 200);
  const currentFriendCode = rotated.body.friendCode;
  assert.notEqual(currentFriendCode, firstFriendCode);
  assertValidFriendCode(firstFriendCode);
  assertValidFriendCode(currentFriendCode);
  assert.equal((await pool.query('SELECT count(*)::integer AS count FROM pending_requests WHERE requester_id = $1 OR recipient_id = $1',
    [firstAccount])).rows[0].count, 2, 'friend-code rotation leaves pending requests in place');

  const generations = [first.body.refreshToken];
  let next = await refresh(first);
  assert.equal(next.status, 200);
  first.body = next.body;
  generations.push(first.body.refreshToken);
  next = await refresh(first);
  assert.equal(next.status, 200);
  first.body = next.body;
  generations.push(first.body.refreshToken);

  const signedOutClaims = await sessionClaims(signedOut);
  const signedOutDeviceId = (await pool.query(
    'SELECT device_key_id FROM sessions WHERE session_id = $1', [signedOutClaims.sub],
  )).rows[0].device_key_id;
  const signedOutThumbprint = createHash('sha256')
    .update(signedOut.d.publicKey.export({ type: 'spki', format: 'der' })).digest('hex');
  const signedOutNonce = await challenge(signedOut.d, 'refresh');
  const signedOutResult = await request('/v1/sessions/sign-out', undefined, signedOut.body.accessToken);
  assert.equal(signedOutResult.status, 204);
  assert.equal(signedOutResult.body, null);
  assert.equal((await pool.query('SELECT count(*)::integer AS count FROM sessions WHERE device_key_id = $1',
    [signedOutDeviceId])).rows[0].count, 0);
  assert.equal((await pool.query('SELECT count(*)::integer AS count FROM auth_nonces WHERE device_key_thumbprint = $1',
    [signedOutThumbprint])).rows[0].count, 0, 'sign-out removes outstanding device-bound nonces');

  const unexpectedKey = generateKeyPairSync('rsa', { modulusLength: 2048 }).privateKey;
  assert.equal((await login(firstSub, first.d, {}, unexpectedKey)).status, 401);
  await challenge(first.d, 'refresh');
  assert.equal((await deleteAccount(first, secondSub)).status, 401);
  const staleNonce = await challenge(first.d, 'delete');
  const stale = await request('/v1/account', { idToken: idToken(firstSub, staleNonce,
    { iat: Math.floor(now / 1000) - 301 }), nonce: staleNonce,
    deviceProof: proof(first.d, 'delete', staleNonce) }, first.body.accessToken, 'DELETE');
  assert.equal(stale.status, 401);

  const exported = await request('/v1/account/export', undefined, first.body.accessToken, 'GET');
  assert.equal(exported.status, 200);
  assert.equal(exported.body.exportVersion, 1);
  assert.equal(typeof exported.body.exportedAt, 'string');
  assert.equal(Object.hasOwn(exported.body, 'accountId'), false);
  assert.ok(!JSON.stringify(exported.body).includes(firstSub));
  assert.ok(!JSON.stringify(exported.body).includes(first.body.refreshToken));
  assert.ok(!JSON.stringify(exported.body).includes(first.body.accessToken));

  const firstClaims = await sessionClaims(first);
  const liveKey = (await pool.query(
    `SELECT d.id, d.device_key_thumbprint, d.device_public_key
       FROM sessions s JOIN device_keys d ON d.id = s.device_key_id
      WHERE s.session_id = $1`, [firstClaims.sub],
  )).rows[0];
  const sessionRows = await pool.query('SELECT session_id FROM sessions WHERE account_id = $1', [firstAccount]);
  const tokenRows = await pool.query(
    `SELECT r.token_hash FROM refresh_tokens r JOIN sessions s USING (session_id)
      WHERE s.account_id = $1`, [firstAccount],
  );
  const nonceRows = await pool.query(
    'SELECT nonce_hash, account_id FROM auth_nonces WHERE device_key_thumbprint = $1',
    [liveKey.device_key_thumbprint],
  );
  assert.ok(nonceRows.rows.some(row => row.account_id === firstAccount));
  assert.ok(nonceRows.rows.some(row => row.account_id === null));
  const markerSetA = new Set([
    firstAccount, hmacSubject(firstSub), liveKey.id, liveKey.device_key_thumbprint.trim(),
    liveKey.device_public_key, firstFriendCode, currentFriendCode, markerA,
    ...sessionRows.rows.map(row => row.session_id),
    ...tokenRows.rows.map(row => row.token_hash.trim()),
    ...nonceRows.rows.map(row => row.nonce_hash.trim()),
    ...generations.map(digest), digest(signedOutNonce), signedOutClaims.sub, signedOutDeviceId, signedOutThumbprint,
    signedOut.d.publicText,
  ].map(String));
  await pool.query(`INSERT INTO global_rate_counters (scope, attempts, window_start, updated_at)
    VALUES ('account_create', 1, $1, $1) ON CONFLICT (scope) DO UPDATE SET attempts = global_rate_counters.attempts + 1`,
  [new Date(now)]);
  await pool.query(`INSERT INTO rate_counters (account_id, scope, attempts, window_start, updated_at)
    SELECT $1, scope, 1, $2, $2 FROM unnest(ARRAY[
      'profile_write', 'friend_code_rotate', 'session_admin', 'account_export', 'account_delete', 'friend_request'
    ]::text[]) AS scopes(scope)
    ON CONFLICT (account_id, scope) DO UPDATE SET attempts = rate_counters.attempts + 1`,
  [firstAccount, new Date(now)]);
  const allScopes = await pool.query('SELECT scope FROM rate_counters WHERE account_id = $1 ORDER BY scope', [firstAccount]);
  assert.deepEqual(allScopes.rows.map(row => row.scope), [
    'account_delete', 'account_export', 'friend_code_rotate', 'friend_request', 'profile_write', 'session_admin',
  ]);
  const snapshotBeforeDelete = await rows();
  assert.deepEqual(Object.keys(snapshotBeforeDelete), EXPECTED_TABLES);
  const beforeByMarker = markerRows(snapshotBeforeDelete, markerSetA);
  assert.ok(beforeByMarker.accounts.length > 0);
  assert.ok(beforeByMarker.login_identities.length > 0);
  assert.ok(beforeByMarker.refresh_tokens.length >= 3);
  assert.ok(beforeByMarker.friendships.length > 0);
  assert.ok(beforeByMarker.pending_requests.length > 0);
  assert.ok(beforeByMarker.blocks.length > 0);
  assert.ok(beforeByMarker.share_entries.length > 0);
  assert.ok(beforeByMarker.rate_counters.length > 0);
  assert.ok(beforeByMarker.rate_limit_events.length > 0);
  const globalColumns = await pool.query(`SELECT column_name FROM information_schema.columns
    WHERE table_schema = 'public' AND table_name = 'global_rate_counters'`);
  assert.equal(globalColumns.rows.some(row => /(_id|_hash|thumbprint|code)$/.test(row.column_name)), false);

  now += 1000;
  const deletionNonce = await challenge(first.d, 'delete');
  const deletionRequest = { idToken: idToken(firstSub, deletionNonce), nonce: deletionNonce,
    deviceProof: proof(first.d, 'delete', deletionNonce) };
  markerSetA.add(digest(deletionNonce));
  assert.equal((await request('/v1/account', deletionRequest, first.body.accessToken, 'DELETE')).status, 200);
  const ledgerRows = await deletionLedger.entries();
  const deletionRecord = ledgerRows.find(entry => entry.subjectHash === hmacSubject(firstSub));
  assert.ok(deletionRecord);
  const deletionTime = new Date(deletionRecord.deletedAt);

  const snapshotAfterDelete = await rows();
  assert.deepEqual(Object.keys(snapshotAfterDelete), EXPECTED_TABLES);
  const afterByMarker = markerRows(snapshotAfterDelete, markerSetA);
  for (const table of EXPECTED_TABLES) {
    assert.equal(afterByMarker[table].length, 0, `${table} retained a value matching a deleted-account marker`);
  }

  const markerSetB = new Set([secondAccount, hmacSubject(secondSub), markerB]);
  const markerSetC = new Set([thirdAccount, hmacSubject(thirdSub), markerC]);
  for (const markers of [markerSetB, markerSetC]) {
    const beforeSurvivors = markerRows(snapshotBeforeDelete, markers);
    const afterSurvivors = markerRows(snapshotAfterDelete, markers);
    for (const table of EXPECTED_TABLES) {
      const expected = beforeSurvivors[table].filter(row =>
        !Object.values(row).some(value => value !== null && markerSetA.has(String(value).trimEnd())));
      assert.deepEqual(afterSurvivors[table], expected, `${table} changed unrelated counterpart rows`);
    }
  }

  const schema = await pool.query(`SELECT table_name FROM information_schema.tables
    WHERE table_schema = 'public' AND table_type = 'BASE TABLE' ORDER BY table_name`);
  assert.deepEqual(schema.rows.map(row => row.table_name), EXPECTED_TABLES);

  const oldAccountId = randomUUID();
  await pool.query(`INSERT INTO accounts (id, friend_code, display_name, created_at, updated_at)
    VALUES ($1, $2, $3, $4, $5)`,
  [oldAccountId, firstFriendCode, markerA, new Date(deletionTime.getTime() - 1000), new Date(deletionTime.getTime() - 1000)]);
  await pool.query(`INSERT INTO login_identities (id, account_id, provider, subject_hash, hash_key_id, created_at)
    VALUES ($1, $2, 'oidc', $3, 1, $4)`,
  [randomUUID(), oldAccountId, hmacSubject(firstSub), new Date(deletionTime.getTime() - 1000)]);
  assert.equal(await deletionLedger.replay(pool), 1);
  assert.equal((await pool.query('SELECT count(*)::integer AS count FROM accounts WHERE id = $1', [oldAccountId])).rows[0].count, 0);

  now = deletionTime.getTime() + 1000;
  const rejoined = await login(firstSub);
  assert.equal(rejoined.status, 200);
  assert.notEqual(rejoined.body.profile.friendCode, firstFriendCode);
  assert.notEqual(rejoined.body.profile.friendCode, currentFriendCode);
  assert.equal(await deletionLedger.replay(pool), 0);
  const rejoinedExport = await request('/v1/account/export', undefined, rejoined.body.accessToken, 'GET');
  assert.equal(rejoinedExport.status, 200);
  assert.deepEqual(rejoinedExport.body.friends, []);
  assert.deepEqual(rejoinedExport.body.shareList, []);
  const oldAccess = await request('/v1/account', undefined, first.body.accessToken, 'GET');
  const oldRefresh = await refresh(first, device(), generations[2]);
  assert.deepEqual(oldAccess, { status: 401, body: { error: 'unauthorized' } });
  assert.deepEqual(oldRefresh, { status: 401, body: { error: 'unauthorized' } });
});

test('registration routes preserve unknown names, normalize graphemes, and keep session details coarse', async () => {
  const first = await login(`routes-${randomUUID()}`);
  const second = await login(`routes-other-${randomUUID()}`);
  assert.equal(first.body.profile.displayName, null);
  assert.equal((await request('/v1/account', undefined, first.body.accessToken, 'GET')).body.displayName, null);
  assert.deepEqual(await setName(first), { status: 400, body: { error: 'invalid_display_name', reason: 'empty' } });
  assert.equal((await setName(first, `${'ก'.repeat(33)}`)).body.reason, 'too_long');
  assert.equal((await setName(first, 'a\u202eb')).body.reason, 'disallowed_character');
  const normalized = await setName(first, `  ${'e\u0301'.repeat(32)}  `);
  assert.equal(normalized.status, 200);
  assert.equal(normalized.body.displayName, 'é'.repeat(32));
  assert.equal((await setName(second, normalized.body.displayName)).status, 200, 'display names are not unique');
  const profile = await request('/v1/account', undefined, first.body.accessToken, 'GET');
  assert.equal(profile.body.displayName, normalized.body.displayName);
  assert.equal(profile.body.friendRequestsAllowed, true);

});

test('session listing, single-device sign-out, and sign-out-everywhere revoke tokens', async () => {
  const firstSub = `sessions-${randomUUID()}`;
  const current = await login(firstSub);
  const other = await login(firstSub, device());
  assert.equal(current.status, 200);
  assert.equal(other.status, 200);
  const listed = await request('/v1/sessions', undefined, current.body.accessToken, 'GET');
  assert.equal(listed.status, 200);
  assert.equal(listed.body.sessions.length, 2);
  assert.deepEqual(Object.keys(listed.body.sessions[0]).sort(), ['createdOn', 'current']);
  assert.ok(listed.body.sessions.every(row => /^\d{4}-\d{2}-\d{2}$/.test(row.createdOn)));
  assert.equal(listed.body.sessions.filter(row => row.current).length, 1);
  const out = await request('/v1/sessions/sign-out', undefined, other.body.accessToken);
  assert.equal(out.status, 204);
  assert.deepEqual(await request('/v1/account', undefined, other.body.accessToken, 'GET'),
    { status: 401, body: { error: 'unauthorized' } });
  assert.equal((await request('/v1/sessions/sign-out-everywhere', undefined, current.body.accessToken)).status, 204);
  assert.deepEqual(await request('/v1/account', undefined, current.body.accessToken, 'GET'),
    { status: 401, body: { error: 'unauthorized' } });
});

test('nonce, body, validation, unknown-route, and unavailable errors use the specified codes', async () => {
  const badPurpose = await request('/v1/auth/nonces', { purpose: 'unknown', devicePublicKey: device().publicText });
  assert.deepEqual(badPurpose, { status: 400, body: { error: 'bad_request' } });
  const unsupported = await rawRequest('/v1/auth/nonces', '{}', { 'content-type': 'text/plain' });
  assert.deepEqual(unsupported, { status: 415, body: { error: 'unsupported_media_type' } });
  const malformed = await rawRequest('/v1/auth/nonces', '{', { 'content-type': 'application/json' });
  assert.deepEqual(malformed, { status: 400, body: { error: 'bad_request' } });
  const oversized = await rawRequest('/v1/auth/nonces', 'x'.repeat(64 * 1024 + 1),
    { 'content-type': 'application/json' });
  assert.deepEqual(oversized, { status: 413, body: { error: 'payload_too_large' } });
  assert.deepEqual(await request('/v1/not-a-route', undefined, undefined, 'GET'),
    { status: 404, body: { error: 'not_found' } });
  assert.deepEqual(await request('/v1/account', undefined, undefined, 'GET'),
    { status: 401, body: { error: 'unauthorized' } });

  const absentSubject = `probe-${randomUUID()}`;
  const probeDevice = device();
  const probeNonce = await challenge(probeDevice, 'sign_in');
  const probe = await request('/v1/auth/sessions', {
    idToken: idToken(absentSubject, probeNonce),
    nonce: probeNonce,
    devicePublicKey: probeDevice.publicText,
    deviceProof: proof(probeDevice, 'sign_in', probeNonce),
    create: false,
  });
  assert.deepEqual(probe, { status: 404, body: { error: 'account_not_found' } });

  const unavailable = createBroker({
    pool: { query: async () => { throw new Error('synthetic database unavailable'); }, connect: async () => { throw new Error('unavailable'); } },
    identity,
    signingPrivateKey: signing.privateKey,
    pepper,
    deletionLedger,
    clock: () => now,
  });
  await new Promise((resolve, reject) => unavailable.once('error', reject).listen(0, '127.0.0.1', resolve));
  const savedOrigin = origin;
  origin = `http://127.0.0.1:${unavailable.address().port}`;
  try {
    assert.deepEqual(await request('/health', undefined, undefined, 'GET'),
      { status: 503, body: { error: 'unavailable' } });
  } finally {
    origin = savedOrigin;
    await new Promise((resolve, reject) => unavailable.close(error => error ? reject(error) : resolve()));
  }
});

test('rolling profile, friend-code, and export limits charge denied attempts', async () => {
  const first = await login(`limits-${randomUUID()}`);
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    assert.equal((await setName(first, `name-${attempt}`)).status, 200);
  }
  assert.deepEqual(await setName(first, 'sixth'), {
    status: 429, body: { error: 'rate_limited' }, retryAfter: '3600',
  });
  assert.deepEqual(await setName(first, 'seventh'), {
    status: 429, body: { error: 'rate_limited' }, retryAfter: '3600',
  });

  const codeAccount = await login(`code-limit-${randomUUID()}`);
  for (let attempt = 0; attempt < 3; attempt += 1) {
    assert.equal((await request('/v1/account/friend-code', {}, codeAccount.body.accessToken)).status, 200);
  }
  const limitedCode = await request('/v1/account/friend-code', {}, codeAccount.body.accessToken);
  assert.deepEqual(limitedCode, {
    status: 429, body: { error: 'rate_limited' }, retryAfter: '86400',
  });
  now += 24 * 60 * 60 * 1000 + 1;
  const refreshed = await refresh(codeAccount);
  assert.equal(refreshed.status, 200);
  codeAccount.body = refreshed.body;
  assert.equal((await request('/v1/account/friend-code', {}, codeAccount.body.accessToken)).status, 200);

  const exportFirst = await login(`export-limit-${randomUUID()}`);
  assert.equal((await request('/v1/account/export', undefined, exportFirst.body.accessToken, 'GET')).status, 200);
  assert.deepEqual(await request('/v1/account/export', undefined, exportFirst.body.accessToken, 'GET'),
    { status: 429, body: { error: 'rate_limited' }, retryAfter: '3600' });
});

test('unauthenticated rate limits are per source and never persisted', async () => {
  for (let attempt = 1; attempt <= 60; attempt += 1) {
    assert.equal((await request('/health', undefined, undefined, 'GET')).status, 200);
  }
  assert.deepEqual(await request('/health', undefined, undefined, 'GET'),
    { status: 429, body: { error: 'rate_limited' }, retryAfter: '60' });
  const columns = await pool.query('SELECT column_name FROM information_schema.columns WHERE table_name = $1', ['rate_limit_events']);
  assert.equal(columns.rows.some(row => /address|ip/i.test(row.column_name)), false);
});
