import { createServer } from 'node:http';
import {
  createHash,
  createHmac,
  createPrivateKey,
  createPublicKey,
  randomBytes,
  randomUUID,
  sign,
  verify,
} from 'node:crypto';

const ACCESS_TOKEN_ISSUER = 'kohbor-account-token-v1';
const BROKER_AUDIENCE = 'broker';
const ACCESS_TOKEN_TTL_SECONDS = 15 * 60;
const REFRESH_IDLE_TTL_MS = 30 * 24 * 60 * 60 * 1000;
const REFRESH_ABSOLUTE_TTL_MS = 90 * 24 * 60 * 60 * 1000;
const SIGN_IN_NONCE_TTL_MS = 10 * 60 * 1000;
// Refresh has no browser hand-off; retain the existing five-minute nonce lifetime.
const REFRESH_NONCE_TTL_MS = 5 * 60 * 1000;
const DELETE_NONCE_TTL_MS = 10 * 60 * 1000;
const FRESH_REAUTH_TTL_SECONDS = 5 * 60;
const EXPIRED_ROW_PURGE_INTERVAL_MS = 60 * 1000;
const MAX_BODY_BYTES = 64 * 1024;
const FRIEND_CODE_ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
const VALID_PURPOSES = new Set(['sign_in', 'refresh', 'delete']);
const SUBJECT_HASH_KEY_ID = 1;
// The design asks for a bounded history but gives no bound; keep all generations until it is decided.
const REFRESH_TOKEN_HISTORY_LIMIT = null;
// These quotas have no numeric values in section 14.0 and stay disabled until decided.
const UNRESOLVED_RATE_LIMITS = Object.freeze({
  account_create: null,
  session_admin: null,
  account_delete: null,
});
const MINUTE_MS = 60 * 1000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;
const ACCOUNT_RATE_LIMITS = Object.freeze({
  profile_write: { attempts: 5, windowMs: HOUR_MS },
  friend_code_rotate: { attempts: 3, windowMs: DAY_MS },
  session_admin: UNRESOLVED_RATE_LIMITS.session_admin,
  account_export: { attempts: 1, windowMs: HOUR_MS },
  account_delete: UNRESOLVED_RATE_LIMITS.account_delete,
  friend_request: { attempts: 10, windowMs: MINUTE_MS },
});
const UNAUTHENTICATED_RATE_LIMIT = Object.freeze({ attempts: 60, windowMs: MINUTE_MS });
const NONCE_TTL_BY_PURPOSE = Object.freeze({
  sign_in: SIGN_IN_NONCE_TTL_MS,
  refresh: REFRESH_NONCE_TTL_MS,
  delete: DELETE_NONCE_TTL_MS,
});
const UNAUTHENTICATED_ROUTES = new Set([
  'GET /health',
  'POST /v1/auth/nonces',
  'POST /v1/auth/sessions',
  'POST /v1/auth/refresh',
]);
const JSON_HEADERS = {
  'content-type': 'application/json; charset=utf-8',
  'cache-control': 'no-store',
  'x-content-type-options': 'nosniff',
};

class HttpError extends Error {
  constructor(status, code, reason = undefined, retryAfterSeconds = undefined) {
    super(code);
    this.status = status;
    this.code = code;
    this.reason = reason;
    this.retryAfterSeconds = retryAfterSeconds;
  }
}

function fail(status, code, reason = undefined, retryAfterSeconds = undefined) {
  throw new HttpError(status, code, reason, retryAfterSeconds);
}

function sendJson(response, status, value, headers = {}) {
  if (response.writableEnded) return;
  const serialized = JSON.stringify(value);
  response.writeHead(status, {
    ...JSON_HEADERS,
    ...headers,
    'content-length': Buffer.byteLength(serialized),
  });
  response.end(serialized);
}

function sendEmpty(response, status) {
  if (response.writableEnded) return;
  response.writeHead(status, { 'cache-control': 'no-store', 'x-content-type-options': 'nosniff' });
  response.end();
}

async function readJson(request) {
  const contentType = request.headers['content-type'] ?? '';
  if (!/^application\/json(?:\s*;|\s*$)/i.test(contentType)) fail(415, 'unsupported_media_type');

  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) fail(413, 'payload_too_large');
    chunks.push(chunk);
  }

  let value;
  try {
    value = JSON.parse(Buffer.concat(chunks).toString('utf8'));
  } catch {
    fail(400, 'bad_request');
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail(400, 'bad_request');
  return value;
}

function decodeBase64Url(value) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9_-]+$/.test(value)) return null;
  const decoded = Buffer.from(value, 'base64url');
  if (decoded.toString('base64url') !== value) return null;
  return decoded;
}

function parseJwt(token) {
  if (typeof token !== 'string' || token.length > 16_384) return null;
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  const headerBytes = decodeBase64Url(parts[0]);
  const payloadBytes = decodeBase64Url(parts[1]);
  const signature = decodeBase64Url(parts[2]);
  if (!headerBytes || !payloadBytes || !signature) return null;
  try {
    const header = JSON.parse(headerBytes.toString('utf8'));
    const claims = JSON.parse(payloadBytes.toString('utf8'));
    if (!header || typeof header !== 'object' || !claims || typeof claims !== 'object') return null;
    return { header, claims, signature, signingInput: Buffer.from(`${parts[0]}.${parts[1]}`) };
  } catch {
    return null;
  }
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function subjectDigest(pepper, issuer, subject) {
  return createHmac('sha256', pepper)
    .update(JSON.stringify([issuer, subject]))
    .digest('hex');
}

function signedAdvisoryKey(namespace, value) {
  const scopedDigest = sha256(`${namespace}\n${value}`);
  const unsigned = BigInt(`0x${scopedDigest.slice(0, 16)}`);
  const signBit = 1n << 63n;
  return (unsigned >= signBit ? unsigned - (1n << 64n) : unsigned).toString();
}

async function lockAdvisoryTransaction(client, namespace, value) {
  await client.query('SELECT pg_advisory_xact_lock($1::bigint)', [signedAdvisoryKey(namespace, value)]);
}

function parseDevicePublicKey(value) {
  if (typeof value !== 'string' || value.length < 40 || value.length > 4096) return null;
  try {
    const key = createPublicKey(value);
    if (key.asymmetricKeyType !== 'ed25519') return null;
    const der = key.export({ type: 'spki', format: 'der' });
    return {
      key,
      pem: key.export({ type: 'spki', format: 'pem' }),
      thumbprint: sha256(der),
    };
  } catch {
    return null;
  }
}

function asPublicKey(value) {
  return value?.type === 'public' ? value : createPublicKey(value);
}

function asPrivateKey(value) {
  return value?.type === 'private' ? value : createPrivateKey(value);
}

function isString(value, max = 8192) {
  return typeof value === 'string' && value.length > 0 && value.length <= max;
}

function verifyDeviceProof(publicKey, purpose, nonce, proof) {
  if (!isString(proof, 256) || !isString(nonce, 256)) return false;
  const signature = decodeBase64Url(proof);
  if (!signature) return false;
  const message = Buffer.from(`kohbor-broker-proof-v1\n${purpose}\n${nonce}`, 'utf8');
  try {
    return verify(null, message, publicKey, signature);
  } catch {
    return false;
  }
}

function verifyIdentityToken(token, identity, nowMs, expectedNonce) {
  const parsed = parseJwt(token);
  if (!parsed || parsed.header.alg !== 'RS256' || parsed.header.typ !== 'JWT') return null;
  if (Object.hasOwn(parsed.header, 'crit') || Object.hasOwn(parsed.header, 'b64')) return null;

  let signatureIsValid = false;
  try {
    signatureIsValid = verify('RSA-SHA256', parsed.signingInput, identity.publicKey, parsed.signature);
  } catch {
    return null;
  }
  if (!signatureIsValid) return null;

  const { iss, aud, sub, nonce, iat, exp, nbf } = parsed.claims;
  const nowSeconds = Math.floor(nowMs / 1000);
  if (iss !== identity.issuer || aud !== identity.audience) return null;
  if (!isString(sub, 512) || nonce !== expectedNonce) return null;
  if (!Number.isInteger(iat) || !Number.isInteger(exp) || exp <= nowSeconds) return null;
  if (iat > nowSeconds + 30 || nowSeconds - iat > FRESH_REAUTH_TTL_SECONDS) return null;
  if (nbf !== undefined && (!Number.isInteger(nbf) || nbf > nowSeconds + 30)) return null;
  return parsed.claims;
}

function accessToken(claims, signingPrivateKey, nowMs) {
  const issuedAt = Math.floor(nowMs / 1000);
  const header = Buffer.from(JSON.stringify({ alg: 'EdDSA', typ: 'JWT' })).toString('base64url');
  const payload = Buffer.from(JSON.stringify({
    iss: ACCESS_TOKEN_ISSUER,
    aud: BROKER_AUDIENCE,
    sub: claims.sessionId,
    dkh: claims.deviceKeyThumbprint,
    iat: issuedAt,
    exp: issuedAt + ACCESS_TOKEN_TTL_SECONDS,
  })).toString('base64url');
  const input = `${header}.${payload}`;
  const signature = sign(null, Buffer.from(input), signingPrivateKey).toString('base64url');
  return `${input}.${signature}`;
}

function validateAccessToken(token, signingPublicKey, nowMs) {
  const parsed = parseJwt(token);
  if (!parsed || parsed.header.alg !== 'EdDSA' || parsed.header.typ !== 'JWT') return null;
  try {
    if (!verify(null, parsed.signingInput, signingPublicKey, parsed.signature)) return null;
  } catch {
    return null;
  }
  const { iss, aud, sub, dkh, iat, exp } = parsed.claims;
  const nowSeconds = Math.floor(nowMs / 1000);
  if (iss !== ACCESS_TOKEN_ISSUER || aud !== BROKER_AUDIENCE) return null;
  if (!isString(sub, 64) || !isString(dkh, 64)) return null;
  if (!Number.isInteger(iat) || !Number.isInteger(exp) || exp <= nowSeconds || iat > nowSeconds + 30) return null;
  return { sessionId: sub, deviceKeyThumbprint: dkh };
}

function mintFriendCode() {
  let value = BigInt(`0x${randomBytes(6).toString('hex')}`) >> 3n;
  let encoded = '';
  for (let i = 0; i < 9; i += 1) {
    encoded = FRIEND_CODE_ALPHABET[Number(value & 31n)] + encoded;
    value >>= 5n;
  }
  const check = [...encoded].reduce((total, character, index) =>
    (total + (index * 2 + 1) * FRIEND_CODE_ALPHABET.indexOf(character)) % 32, 0);
  return `KB-${encoded}${FRIEND_CODE_ALPHABET[check]}`;
}

function nonceTtl(purpose) {
  return NONCE_TTL_BY_PURPOSE[purpose];
}

function validateDisplayName(value) {
  if (typeof value !== 'string') return { error: 'empty' };
  const normalized = value.normalize('NFC').trim();
  if (!normalized) return { error: 'empty' };
  if (/[\p{Cc}\p{Cf}]/u.test(normalized)) return { error: 'disallowed_character' };
  if ([...normalized].length > 128 || Buffer.byteLength(normalized, 'utf8') > 512) {
    return { error: 'too_long' };
  }
  const segmenter = new Intl.Segmenter('und', { granularity: 'grapheme' });
  let clusters = 0;
  for (const _ of segmenter.segment(normalized)) clusters += 1;
  if (clusters > 32) return { error: 'too_long' };
  return { value: normalized };
}

function bearerToken(request) {
  const value = request.headers.authorization;
  const match = typeof value === 'string' && /^Bearer ([A-Za-z0-9._-]{1,16384})$/.exec(value);
  return match?.[1] ?? null;
}

async function transaction(pool, callback, compensateAfterRollback = undefined) {
  const client = await pool.connect();
  let active = false;
  try {
    await client.query('BEGIN');
    active = true;
    const result = await callback(client);
    await client.query('COMMIT');
    active = false;
    return result;
  } catch (error) {
    if (active) {
      let rollbackSucceeded = false;
      try {
        await client.query('ROLLBACK');
        rollbackSucceeded = true;
      } catch {
        // Keep the ledger entry when transaction state is uncertain.
      }
      if (rollbackSucceeded && compensateAfterRollback) {
        try {
          await compensateAfterRollback();
        } catch {
          // Preserve the original error without emitting driver or identity details.
        }
      }
    }
    throw error;
  } finally {
    client.release();
  }
}

async function consumeNonce(client, { nonce, purpose, thumbprint, now }) {
  if (!isString(nonce, 256)) return null;
  const result = await client.query(
    `UPDATE auth_nonces
        SET consumed_at = $4
      WHERE nonce_hash = $1
        AND purpose = $2
        AND device_key_thumbprint = $3
        AND consumed_at IS NULL
        AND expires_at > $4
      RETURNING id`,
    [sha256(nonce), purpose, thumbprint, new Date(now)],
  );
  return result.rows[0] ?? null;
}

async function getOrCreateAccount(client, pepper, identity, subject, now) {
  const digest = subjectDigest(pepper, identity.issuer, subject);
  await lockAdvisoryTransaction(client, 'login-identity', digest);
  const existing = await client.query(
    `SELECT account_id
       FROM login_identities
      WHERE provider = 'oidc' AND subject_hash = $1
      FOR UPDATE`,
    [digest],
  );
  if (existing.rowCount > 0) {
    return { accountId: existing.rows[0].account_id, subjectHash: digest };
  }

  for (let attempt = 0; attempt < 5; attempt += 1) {
    const accountId = randomUUID();
    const friendCode = mintFriendCode();
    await lockAdvisoryTransaction(client, 'friend-code', friendCode);
    const collision = await client.query('SELECT 1 FROM accounts WHERE friend_code = $1', [friendCode]);
    if (collision.rowCount > 0) continue;
    const created = await client.query(
      `INSERT INTO accounts (id, friend_code, created_at, updated_at)
       VALUES ($1, $2, $3, $3)
       ON CONFLICT (friend_code) DO NOTHING
       RETURNING id`,
      [accountId, friendCode, new Date(now)],
    );
    if (created.rowCount === 0) continue;

    const linked = await client.query(
      `INSERT INTO login_identities (id, account_id, provider, subject_hash, hash_key_id, created_at)
       VALUES ($1, $2, 'oidc', $3, $4, $5)
       ON CONFLICT (provider, subject_hash) DO NOTHING
       RETURNING account_id`,
      [randomUUID(), accountId, digest, SUBJECT_HASH_KEY_ID, new Date(now)],
    );
    if (linked.rowCount > 0) return { accountId, subjectHash: digest };

    await client.query('DELETE FROM accounts WHERE id = $1', [accountId]);
    const raced = await client.query(
      `SELECT account_id
         FROM login_identities
        WHERE provider = 'oidc' AND subject_hash = $1
        FOR UPDATE`,
      [digest],
    );
    if (raced.rowCount > 0) return { accountId: raced.rows[0].account_id, subjectHash: digest };
  }
  throw new Error('Account allocation failed');
}

async function registerDevice(client, accountId, device, now) {
  const result = await client.query(
    `INSERT INTO device_keys
       (id, account_id, device_public_key, device_key_thumbprint, created_at, updated_at)
     VALUES ($1, $2, $3, $4, $5, $5)
     ON CONFLICT (device_key_thumbprint) DO UPDATE
       SET device_public_key = EXCLUDED.device_public_key,
           updated_at = EXCLUDED.updated_at
       WHERE device_keys.account_id = EXCLUDED.account_id
     RETURNING id`,
    [randomUUID(), accountId, device.pem, device.thumbprint, new Date(now)],
  );
  return result.rows[0]?.id ?? null;
}

async function createSession(client, accountId, deviceKeyId, thumbprint, now, signingPrivateKey) {
  await client.query('DELETE FROM sessions WHERE device_key_id = $1', [deviceKeyId]);
  const sessionId = randomUUID();
  const refreshToken = randomBytes(32).toString('base64url');
  await client.query(
    `INSERT INTO sessions (session_id, account_id, device_key_id, created_at, expires_at, updated_at)
     VALUES ($1, $2, $3, $4, $5, $4)`,
    [sessionId, accountId, deviceKeyId, new Date(now), new Date(now + REFRESH_IDLE_TTL_MS)],
  );
  await client.query(
    `INSERT INTO refresh_tokens (token_hash, session_id, generation, created_at)
     VALUES ($1, $2, 1, $3)`,
    [sha256(refreshToken), sessionId, new Date(now)],
  );
  return {
    accessToken: accessToken({ sessionId, deviceKeyThumbprint: thumbprint }, signingPrivateKey, now),
    refreshToken,
    expiresIn: ACCESS_TOKEN_TTL_SECONDS,
  };
}

async function chargeAccountRate(pool, accountId, scope, now) {
  const limit = ACCOUNT_RATE_LIMITS[scope];
  if (limit === undefined) throw new Error('unknown account rate scope');
  if (limit === null) return;

  const active = await transaction(pool, async client => {
    await lockAdvisoryTransaction(client, 'rate-limit', `${accountId}\n${scope}`);
    const windowStart = new Date(now - limit.windowMs);
    await client.query(
      'DELETE FROM rate_limit_events WHERE account_id = $1 AND scope = $2 AND occurred_at <= $3',
      [accountId, scope, windowStart],
    );
    await client.query(
      `INSERT INTO rate_limit_events (id, account_id, scope, occurred_at)
       VALUES ($1, $2, $3, $4)`,
      [randomUUID(), accountId, scope, new Date(now)],
    );
    const active = await client.query(
      `SELECT count(*)::integer AS attempts, min(occurred_at) AS window_start
         FROM rate_limit_events
        WHERE account_id = $1 AND scope = $2`,
      [accountId, scope],
    );
    const attempts = active.rows[0].attempts;
    const firstAttempt = active.rows[0].window_start;
    await client.query(
      `INSERT INTO rate_counters (account_id, scope, attempts, window_start, updated_at)
       VALUES ($1, $2, $3, $4, $5)
       ON CONFLICT (account_id, scope) DO UPDATE SET
         attempts = EXCLUDED.attempts,
         window_start = EXCLUDED.window_start,
         updated_at = EXCLUDED.updated_at`,
      [accountId, scope, attempts, firstAttempt, new Date(now)],
    );
    return { attempts, windowStart: firstAttempt };
  });
  if (active.attempts > limit.attempts) {
    const retryAfterSeconds = Math.max(1, Math.ceil(
      (new Date(active.windowStart).getTime() + limit.windowMs - now) / 1000,
    ));
    fail(429, 'rate_limited', undefined, retryAfterSeconds);
  }
}

function createUnauthenticatedRateLimiter(clock) {
  const windows = new Map();
  const addressKey = randomBytes(32);
  let nextSweepAt = 0;
  return sourceAddress => {
    const now = clock();
    if (now >= nextSweepAt) {
      for (const [key, value] of windows) {
        if (value.startedAt <= now - UNAUTHENTICATED_RATE_LIMIT.windowMs) windows.delete(key);
      }
      nextSweepAt = now + UNAUTHENTICATED_RATE_LIMIT.windowMs;
    }
    if (typeof sourceAddress !== 'string' || sourceAddress.length === 0) {
      return Math.ceil(UNAUTHENTICATED_RATE_LIMIT.windowMs / 1000);
    }
    const key = createHmac('sha256', addressKey).update(sourceAddress).digest('hex');
    let window = windows.get(key);
    if (!window || window.startedAt <= now - UNAUTHENTICATED_RATE_LIMIT.windowMs) {
      window = { startedAt: now, attempts: 0 };
    }
    window.attempts += 1;
    windows.set(key, window);
    if (window.attempts <= UNAUTHENTICATED_RATE_LIMIT.attempts) return null;
    return Math.max(1, Math.ceil(
      (window.startedAt + UNAUTHENTICATED_RATE_LIMIT.windowMs - now) / 1000,
    ));
  };
}

async function authenticate(request, pool, signingPublicKey, clock) {
  const token = bearerToken(request);
  if (!token) fail(401, 'unauthorized');
  const claims = validateAccessToken(token, signingPublicKey, clock());
  if (!claims) fail(401, 'unauthorized');

  const result = await pool.query(
    `SELECT s.account_id, d.device_key_thumbprint
       FROM sessions s
       JOIN device_keys d ON d.id = s.device_key_id
      WHERE s.session_id = $1 AND s.expires_at > $2`,
    [claims.sessionId, new Date(clock())],
  );
  if (result.rowCount === 0 || result.rows[0].device_key_thumbprint !== claims.deviceKeyThumbprint) {
    fail(401, 'unauthorized');
  }
  return { ...claims, accountId: result.rows[0].account_id };
}

async function readProfile(pool, accountId) {
  const profile = await pool.query(
    'SELECT friend_code, display_name, settings FROM accounts WHERE id = $1',
    [accountId],
  );
  if (profile.rowCount === 0) fail(401, 'unauthorized');
  return {
    friendCode: profile.rows[0].friend_code,
    displayName: profile.rows[0].display_name,
    friendRequestsAllowed: profile.rows[0].settings?.friendRequestsAllowed !== false,
  };
}

async function exportAccount(pool, accountId, now) {
  const ownProfile = await pool.query(
    'SELECT friend_code, display_name, settings FROM accounts WHERE id = $1',
    [accountId],
  );
  if (ownProfile.rowCount === 0) fail(401, 'unauthorized');

  const friends = await pool.query(
    `SELECT a.friend_code, a.display_name
       FROM friendships f JOIN accounts a ON a.id = f.friend_id
      WHERE f.account_id = $1
      ORDER BY a.friend_code`,
    [accountId],
  );
  const shareList = await pool.query(
    `SELECT a.friend_code, a.display_name, s.scopes, s.paused, s.last_confirmed_at
       FROM share_entries s JOIN accounts a ON a.id = s.friend_id
      WHERE s.account_id = $1
      ORDER BY a.friend_code`,
    [accountId],
  );
  const blocks = await pool.query(
    `SELECT a.friend_code, a.display_name
       FROM blocks b JOIN accounts a ON a.id = b.blocked_id
      WHERE b.blocker_id = $1
      ORDER BY a.friend_code`,
    [accountId],
  );

  return {
    exportVersion: 1,
    exportedAt: new Date(now).toISOString(),
    profile: {
      friendCode: ownProfile.rows[0].friend_code,
      displayName: ownProfile.rows[0].display_name,
      settings: ownProfile.rows[0].settings,
    },
    friends: friends.rows,
    shareList: shareList.rows.map(row => ({
      friendCode: row.friend_code,
      displayName: row.display_name,
      scopes: row.scopes,
      paused: row.paused,
      lastConfirmedAt: row.last_confirmed_at,
    })),
    blocks: blocks.rows,
  };
}

async function purgeExpiredRows(pool, now) {
  const instant = new Date(now);
  await pool.query(
    `DELETE FROM auth_nonces n
      USING device_keys d, sessions s
     WHERE s.device_key_id = d.id
       AND s.expires_at <= $1
       AND n.device_key_thumbprint = d.device_key_thumbprint`,
    [instant],
  );
  await pool.query(
    `DELETE FROM device_keys d
      USING sessions s
     WHERE s.device_key_id = d.id AND s.expires_at <= $1`,
    [instant],
  );
  await pool.query('DELETE FROM auth_nonces WHERE expires_at <= $1', [instant]);
  await pool.query('DELETE FROM pending_requests WHERE expires_at <= $1', [instant]);
  await pool.query('DELETE FROM rate_limit_events WHERE occurred_at <= $1', [new Date(now - DAY_MS)]);
  await pool.query('DELETE FROM rate_counters WHERE window_start <= $1', [new Date(now - DAY_MS)]);
}

export function createBroker({ pool, identity, signingPrivateKey, pepper, deletionLedger, clock = () => Date.now() }) {
  if (!pool || typeof pool.query !== 'function' || typeof pool.connect !== 'function') {
    throw new TypeError('createBroker requires a PostgreSQL pool');
  }
  if (!identity || !isString(identity.issuer, 512) || !isString(identity.audience, 512) || !identity.publicKey) {
    throw new TypeError('createBroker requires a fixed OIDC issuer, audience, and public key');
  }
  if (typeof pepper !== 'string' || Buffer.byteLength(pepper) < 16) {
    throw new TypeError('createBroker requires a subject pepper of at least 16 bytes');
  }
  if (!deletionLedger || typeof deletionLedger.append !== 'function' || typeof deletionLedger.remove !== 'function') {
    throw new TypeError('createBroker requires an external deletion ledger');
  }

  let identityPublicKey;
  let brokerSigningPrivateKey;
  let brokerSigningPublicKey;
  try {
    identityPublicKey = asPublicKey(identity.publicKey);
    brokerSigningPrivateKey = asPrivateKey(signingPrivateKey);
    brokerSigningPublicKey = createPublicKey(brokerSigningPrivateKey);
  } catch {
    throw new TypeError('createBroker received invalid signing key material');
  }
  if (identityPublicKey.asymmetricKeyType !== 'rsa' ||
      Number(identityPublicKey.asymmetricKeyDetails?.modulusLength ?? 0) < 2048 ||
      brokerSigningPrivateKey.asymmetricKeyType !== 'ed25519') {
    throw new TypeError('createBroker requires RSA-2048+ identity verification and Ed25519 broker signing keys');
  }

  const fixedIdentity = { issuer: identity.issuer, audience: identity.audience, publicKey: identityPublicKey };
  const allowUnauthenticated = createUnauthenticatedRateLimiter(clock);
  const rateLimit = (accountId, scope) => chargeAccountRate(pool, accountId, scope, clock());
  let lastExpiredRowPurgeAt = Number.NEGATIVE_INFINITY;
  let expiredRowPurge = null;

  async function purgeExpiredRowsIfDue() {
    const now = clock();
    if (now - lastExpiredRowPurgeAt < EXPIRED_ROW_PURGE_INTERVAL_MS) return;
    if (expiredRowPurge) return expiredRowPurge;

    lastExpiredRowPurgeAt = now;
    expiredRowPurge = purgeExpiredRows(pool, now).catch(() => {}).finally(() => {
      expiredRowPurge = null;
    });
    await expiredRowPurge;
  }

  async function authenticateRequest(request) {
    const claims = await authenticate(request, pool, brokerSigningPublicKey, clock);
    await purgeExpiredRowsIfDue();
    return claims;
  }

  async function issueNonce(body) {
    if (!VALID_PURPOSES.has(body.purpose)) fail(400, 'bad_request');
    const device = parseDevicePublicKey(body.devicePublicKey);
    if (!device) fail(400, 'bad_request');

    const now = clock();
    const ttl = nonceTtl(body.purpose);
    const nonce = randomBytes(32).toString('base64url');
    await pool.query(
      `INSERT INTO auth_nonces
         (id, nonce_hash, device_key_thumbprint, purpose, created_at, expires_at)
       VALUES ($1, $2, $3, $4, $5, $6)`,
      [randomUUID(), sha256(nonce), device.thumbprint, body.purpose,
        new Date(now), new Date(now + ttl)],
    );
    return { nonce, expiresIn: ttl / 1000 };
  }

  async function signIn(body) {
    if (!isString(body.nonce, 256) || !isString(body.idToken, 16_384)) fail(400, 'bad_request');
    if (body.create !== undefined && typeof body.create !== 'boolean') fail(400, 'bad_request');
    const create = body.create !== false;
    const device = parseDevicePublicKey(body.devicePublicKey);
    if (!device) fail(401, 'unauthorized');
    if (!verifyDeviceProof(device.key, 'sign_in', body.nonce, body.deviceProof)) fail(401, 'unauthorized');

    const now = clock();
    const result = await transaction(pool, async client => {
      const consumed = await consumeNonce(client, {
        nonce: body.nonce, purpose: 'sign_in', thumbprint: device.thumbprint, now,
      });
      if (!consumed) return { unauthorized: true };

      const claims = verifyIdentityToken(body.idToken, fixedIdentity, now, body.nonce);
      if (!claims) return { unauthorized: true };
      await lockAdvisoryTransaction(client, 'device-thumbprint', device.thumbprint);
      const subjectHash = subjectDigest(pepper, fixedIdentity.issuer, claims.sub);
      const knownIdentity = await client.query(
        `SELECT account_id FROM login_identities
          WHERE provider = 'oidc' AND subject_hash = $1`,
        [subjectHash],
      );
      const knownDevice = await client.query(
        'SELECT account_id FROM device_keys WHERE device_key_thumbprint = $1',
        [device.thumbprint],
      );
      if (knownDevice.rowCount > 0 &&
          (knownIdentity.rowCount === 0 || knownDevice.rows[0].account_id !== knownIdentity.rows[0].account_id)) {
        return { unauthorized: true };
      }
      if (knownIdentity.rowCount === 0 && !create) return { accountNotFound: true };
      const account = knownIdentity.rowCount > 0
        ? { accountId: knownIdentity.rows[0].account_id, subjectHash }
        : await getOrCreateAccount(client, pepper, fixedIdentity, claims.sub, now);
      const knownDeviceForAccount = await client.query(
        `SELECT id FROM device_keys
          WHERE account_id = $1 AND device_key_thumbprint = $2
          FOR UPDATE`,
        [account.accountId, device.thumbprint],
      );
      const isNewDevice = knownDeviceForAccount.rowCount === 0;
      const deviceKeyId = await registerDevice(client, account.accountId, device, now);
      if (!deviceKeyId) fail(401, 'unauthorized');
      if (isNewDevice) {
        await client.query(
          `UPDATE share_entries
              SET paused = true,
                  pairing_generation = pairing_generation + 1,
                  updated_at = $2
            WHERE account_id = $1`,
          [account.accountId, new Date(now)],
        );
      }
      await client.query('UPDATE auth_nonces SET account_id = $2 WHERE id = $1', [consumed.id, account.accountId]);
      const tokens = await createSession(
        client, account.accountId, deviceKeyId, device.thumbprint, now, brokerSigningPrivateKey,
      );
      const profile = await client.query(
        'SELECT friend_code, display_name, settings FROM accounts WHERE id = $1',
        [account.accountId],
      );
      return {
        tokens,
        newDevice: isNewDevice,
        profile: {
          friendCode: profile.rows[0].friend_code,
          displayName: profile.rows[0].display_name,
        },
      };
    });
    if (result.unauthorized) fail(401, 'unauthorized');
    if (result.accountNotFound) fail(404, 'account_not_found');
    return { ...result.tokens, profile: result.profile, newDevice: result.newDevice };
  }

  async function refresh(body) {
    if (!isString(body.nonce, 256) || !isString(body.refreshToken, 256)) fail(400, 'bad_request');
    const device = parseDevicePublicKey(body.devicePublicKey);
    if (!device) fail(401, 'unauthorized');
    if (!verifyDeviceProof(device.key, 'refresh', body.nonce, body.deviceProof)) fail(401, 'unauthorized');

    const now = clock();
    const tokenHash = sha256(body.refreshToken);
    const result = await transaction(pool, async client => {
      const consumed = await consumeNonce(client, {
        nonce: body.nonce, purpose: 'refresh', thumbprint: device.thumbprint, now,
      });
      if (!consumed) return { unauthorized: true };

      const selected = await client.query(
        `SELECT r.token_hash, r.session_id, r.generation, r.rotated_at,
                s.account_id, s.device_key_id, s.created_at, s.expires_at,
                d.device_key_thumbprint
           FROM refresh_tokens r
           JOIN sessions s ON s.session_id = r.session_id
           JOIN device_keys d ON d.id = s.device_key_id
          WHERE r.token_hash = $1
          FOR UPDATE OF r, s, d`,
        [tokenHash],
      );
      if (selected.rowCount === 0) return { unauthorized: true };
      const session = selected.rows[0];
      if (session.rotated_at !== null) {
        await client.query(
          'DELETE FROM auth_nonces WHERE device_key_thumbprint = $1',
          [session.device_key_thumbprint],
        );
        await client.query('DELETE FROM device_keys WHERE id = $1', [session.device_key_id]);
        return { unauthorized: true };
      }
      if (session.device_key_thumbprint !== device.thumbprint ||
          new Date(session.expires_at).getTime() <= now) return { unauthorized: true };

      const nextRefreshToken = randomBytes(32).toString('base64url');
      const nextGeneration = Number(session.generation) + 1;
      const absoluteExpiry = new Date(session.created_at).getTime() + REFRESH_ABSOLUTE_TTL_MS;
      const nextExpiry = Math.min(now + REFRESH_IDLE_TTL_MS, absoluteExpiry);
      if (nextExpiry <= now) return { unauthorized: true };
      await client.query(
        `UPDATE refresh_tokens SET rotated_at = $2
          WHERE token_hash = $1 AND rotated_at IS NULL`,
        [tokenHash, new Date(now)],
      );
      await client.query(
        `INSERT INTO refresh_tokens (token_hash, session_id, generation, created_at)
         VALUES ($1, $2, $3, $4)`,
        [sha256(nextRefreshToken), session.session_id, nextGeneration, new Date(now)],
      );
      await client.query(
        `UPDATE sessions SET expires_at = $2, updated_at = $3 WHERE session_id = $1`,
        [session.session_id, new Date(nextExpiry), new Date(now)],
      );
      if (REFRESH_TOKEN_HISTORY_LIMIT !== null) {
        await client.query(
          `DELETE FROM refresh_tokens
            WHERE session_id = $1 AND rotated_at IS NOT NULL
              AND generation <= $2`,
          [session.session_id, nextGeneration - REFRESH_TOKEN_HISTORY_LIMIT],
        );
      }
      await client.query('UPDATE auth_nonces SET account_id = $2 WHERE id = $1', [consumed.id, session.account_id]);
      return {
        tokens: {
          accessToken: accessToken({
            sessionId: session.session_id,
            deviceKeyThumbprint: device.thumbprint,
          }, brokerSigningPrivateKey, now),
          refreshToken: nextRefreshToken,
          expiresIn: ACCESS_TOKEN_TTL_SECONDS,
        },
      };
    });
    if (result.unauthorized) fail(401, 'unauthorized');
    return result.tokens;
  }

  async function deleteAccount(body, accessClaims) {
    if (!isString(body.nonce, 256) || !isString(body.idToken, 16_384)) fail(400, 'bad_request');
    if (!isString(body.deviceProof, 256)) fail(401, 'unauthorized');

    const now = clock();
    let deletionEntry;
    const result = await transaction(pool, async client => {
      const enrolled = await client.query(
        `SELECT d.id AS device_key_id, d.device_public_key
           FROM sessions s
           JOIN device_keys d ON d.id = s.device_key_id
          WHERE s.session_id = $1 AND s.account_id = $2
            AND d.device_key_thumbprint = $3 AND s.expires_at > $4
          FOR UPDATE OF s, d`,
        [accessClaims.sessionId, accessClaims.accountId, accessClaims.deviceKeyThumbprint, new Date(now)],
      );
      if (enrolled.rowCount === 0) return { unauthorized: true };
      const device = parseDevicePublicKey(enrolled.rows[0].device_public_key);
      if (!device || device.thumbprint !== accessClaims.deviceKeyThumbprint ||
          !verifyDeviceProof(device.key, 'delete', body.nonce, body.deviceProof)) {
        return { unauthorized: true };
      }
      const consumed = await consumeNonce(client, {
        nonce: body.nonce, purpose: 'delete', thumbprint: device.thumbprint, now,
      });
      if (!consumed) return { unauthorized: true };
      const claims = verifyIdentityToken(body.idToken, fixedIdentity, now, body.nonce);
      if (!claims || Math.floor(now / 1000) - claims.iat > FRESH_REAUTH_TTL_SECONDS) {
        return { unauthorized: true };
      }

      const identityRow = await client.query(
        `SELECT account_id, subject_hash
           FROM login_identities
          WHERE provider = 'oidc' AND subject_hash = $1
          FOR UPDATE`,
        [subjectDigest(pepper, fixedIdentity.issuer, claims.sub)],
      );
      if (identityRow.rowCount === 0 || identityRow.rows[0].account_id !== accessClaims.accountId) {
        return { unauthorized: true };
      }
      const session = await client.query(
        `SELECT session_id
           FROM sessions s
           JOIN device_keys d ON d.id = s.device_key_id
          WHERE s.session_id = $1 AND s.account_id = $2
            AND d.device_key_thumbprint = $3 AND s.expires_at > $4
          FOR UPDATE`,
        [accessClaims.sessionId, accessClaims.accountId, device.thumbprint, new Date(now)],
      );
      if (session.rowCount === 0) return { unauthorized: true };

      await client.query('UPDATE auth_nonces SET account_id = $2 WHERE id = $1', [consumed.id, accessClaims.accountId]);
      await client.query(
        `DELETE FROM auth_nonces
          WHERE device_key_thumbprint IN (
            SELECT device_key_thumbprint FROM device_keys WHERE account_id = $1
          )`,
        [accessClaims.accountId],
      );
      deletionEntry = {
        subjectHash: identityRow.rows[0].subject_hash.trim(),
        deletedAt: new Date(now),
      };
      await deletionLedger.append(deletionEntry);
      await client.query('DELETE FROM accounts WHERE id = $1', [accessClaims.accountId]);
      return { deleted: true };
    }, async () => {
      if (deletionEntry) await deletionLedger.remove(deletionEntry);
    });
    if (result.unauthorized) fail(401, 'unauthorized');
    return { deleted: true };
  }

  async function setDisplayName(body, accountId) {
    const validation = validateDisplayName(body.displayName);
    if (validation.error) fail(400, 'invalid_display_name', validation.error);
    let updated;
    try {
      updated = await pool.query(
        `UPDATE accounts SET display_name = $2, updated_at = $3
          WHERE id = $1 RETURNING display_name`,
        [accountId, validation.value, new Date(clock())],
      );
    } catch (error) {
      if (typeof error?.code === 'string' && error.code.startsWith('23')) {
        fail(400, 'invalid_display_name', 'disallowed_character');
      }
      throw error;
    }
    if (updated.rowCount === 0) fail(401, 'unauthorized');
    return { displayName: updated.rows[0].display_name };
  }

  async function rotateFriendCode(accountId) {
    return transaction(pool, async client => {
      for (let attempt = 0; attempt < 5; attempt += 1) {
        const friendCode = mintFriendCode();
        await lockAdvisoryTransaction(client, 'friend-code', friendCode);
        const collision = await client.query('SELECT 1 FROM accounts WHERE friend_code = $1', [friendCode]);
        if (collision.rowCount > 0) continue;
        const result = await client.query(
          `UPDATE accounts SET friend_code = $2, updated_at = $3
            WHERE id = $1 RETURNING friend_code`,
          [accountId, friendCode, new Date(clock())],
        );
        if (result.rowCount === 0) fail(401, 'unauthorized');
        return { friendCode: result.rows[0].friend_code };
      }
      fail(503, 'unavailable');
    });
  }

  async function listSessions(accountId, sessionId) {
    const result = await pool.query(
      `SELECT s.session_id, s.created_at
         FROM sessions s
        WHERE s.account_id = $1
        ORDER BY s.created_at DESC, s.session_id`,
      [accountId],
    );
    return {
      sessions: result.rows.map(row => ({
        current: row.session_id === sessionId,
        createdOn: new Date(row.created_at).toISOString().slice(0, 10),
      })),
    };
  }

  async function signOutSession(accountId, sessionId) {
    const signedOut = await transaction(pool, async client => {
      const selected = await client.query(
        `SELECT s.device_key_id, d.device_key_thumbprint
           FROM sessions s JOIN device_keys d ON d.id = s.device_key_id
          WHERE s.session_id = $1 AND s.account_id = $2
          FOR UPDATE OF s, d`,
        [sessionId, accountId],
      );
      if (selected.rowCount === 0) return false;
      await client.query(
        'DELETE FROM auth_nonces WHERE device_key_thumbprint = $1',
        [selected.rows[0].device_key_thumbprint],
      );
      await client.query('DELETE FROM device_keys WHERE id = $1', [selected.rows[0].device_key_id]);
      return true;
    });
    if (!signedOut) fail(401, 'unauthorized');
  }

  async function signOutEverywhere(accountId) {
    await transaction(pool, async client => {
      await client.query(
        `DELETE FROM auth_nonces
          WHERE device_key_thumbprint IN (
            SELECT device_key_thumbprint FROM device_keys WHERE account_id = $1
          )`,
        [accountId],
      );
      await client.query('DELETE FROM device_keys WHERE account_id = $1', [accountId]);
    });
  }

  async function handle(request, response) {
    try {
      let pathname;
      try {
        pathname = new URL(request.url ?? '/', 'http://broker.invalid').pathname;
      } catch {
        fail(400, 'bad_request');
      }

      if (UNAUTHENTICATED_ROUTES.has(`${request.method} ${pathname}`)) {
        const retryAfterSeconds = allowUnauthenticated(request.socket.remoteAddress);
        if (retryAfterSeconds !== null) fail(429, 'rate_limited', undefined, retryAfterSeconds);
      }
      if (request.method === 'GET' && pathname === '/health') {
        try {
          await pool.query('SELECT 1');
          sendJson(response, 200, { status: 'ok' });
        } catch {
          sendJson(response, 503, { error: 'unavailable' });
        }
        return;
      }

      if (request.method === 'POST' && pathname === '/v1/auth/nonces') {
        sendJson(response, 200, await issueNonce(await readJson(request)));
        return;
      }

      if (request.method === 'POST' && pathname === '/v1/auth/sessions') {
        sendJson(response, 200, await signIn(await readJson(request)));
        return;
      }

      if (request.method === 'POST' && pathname === '/v1/auth/refresh') {
        sendJson(response, 200, await refresh(await readJson(request)));
        return;
      }

      if (request.method === 'PUT' && pathname === '/v1/account/display-name') {
        const claims = await authenticateRequest(request);
        await rateLimit(claims.accountId, 'profile_write');
        sendJson(response, 200, await setDisplayName(await readJson(request), claims.accountId));
        return;
      }

      if (request.method === 'GET' && pathname === '/v1/account') {
        const claims = await authenticateRequest(request);
        sendJson(response, 200, await readProfile(pool, claims.accountId));
        return;
      }

      if (request.method === 'POST' && pathname === '/v1/account/friend-code') {
        const claims = await authenticateRequest(request);
        await rateLimit(claims.accountId, 'friend_code_rotate');
        const body = await readJson(request);
        if (Object.keys(body).length !== 0) fail(400, 'bad_request');
        sendJson(response, 200, await rotateFriendCode(claims.accountId));
        return;
      }

      if (request.method === 'GET' && pathname === '/v1/sessions') {
        const claims = await authenticateRequest(request);
        await rateLimit(claims.accountId, 'session_admin');
        sendJson(response, 200, await listSessions(claims.accountId, claims.sessionId));
        return;
      }

      if (request.method === 'POST' && pathname === '/v1/sessions/sign-out') {
        const claims = await authenticateRequest(request);
        await rateLimit(claims.accountId, 'session_admin');
        await signOutSession(claims.accountId, claims.sessionId);
        sendEmpty(response, 204);
        return;
      }

      if (request.method === 'POST' && pathname === '/v1/sessions/sign-out-everywhere') {
        const claims = await authenticateRequest(request);
        await rateLimit(claims.accountId, 'session_admin');
        await signOutEverywhere(claims.accountId);
        sendEmpty(response, 204);
        return;
      }

      if (request.method === 'GET' && pathname === '/v1/account/export') {
        const claims = await authenticateRequest(request);
        await rateLimit(claims.accountId, 'account_export');
        sendJson(response, 200, await exportAccount(pool, claims.accountId, clock()));
        return;
      }

      if (request.method === 'DELETE' && pathname === '/v1/account') {
        const claims = await authenticateRequest(request);
        await rateLimit(claims.accountId, 'account_delete');
        sendJson(response, 200, await deleteAccount(await readJson(request), claims));
        return;
      }

      sendJson(response, 404, { error: 'not_found' });
    } catch (error) {
      const status = error instanceof HttpError ? error.status : 503;
      const code = error instanceof HttpError ? error.code : 'unavailable';
      const headers = error instanceof HttpError && error.retryAfterSeconds !== undefined
        ? { 'retry-after': String(error.retryAfterSeconds) }
        : {};
      sendJson(response, status, {
        error: code,
        ...(error instanceof HttpError && error.reason ? { reason: error.reason } : {}),
      }, headers);
    }
  }

  return createServer((request, response) => {
    void handle(request, response);
  });
}
