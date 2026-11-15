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
const REFRESH_TOKEN_TTL_MS = 30 * 24 * 60 * 60 * 1000;
const AUTH_NONCE_TTL_MS = 5 * 60 * 1000;
const FRESH_REAUTH_TTL_SECONDS = 5 * 60;
const MAX_BODY_BYTES = 64 * 1024;
const FRIEND_CODE_ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
const VALID_PURPOSES = new Set(['sign_in', 'refresh', 'delete']);
const JSON_HEADERS = {
  'content-type': 'application/json; charset=utf-8',
  'cache-control': 'no-store',
  'x-content-type-options': 'nosniff',
};

class HttpError extends Error {
  constructor(status, code) {
    super(code);
    this.status = status;
    this.code = code;
  }
}

function fail(status, code) {
  throw new HttpError(status, code);
}

function sendJson(response, status, value) {
  if (response.writableEnded) return;
  const serialized = JSON.stringify(value);
  response.writeHead(status, {
    ...JSON_HEADERS,
    'content-length': Buffer.byteLength(serialized),
  });
  response.end(serialized);
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
    sub: claims.accountId,
    sid: claims.sessionId,
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
  const { iss, aud, sub, sid, dkh, iat, exp } = parsed.claims;
  const nowSeconds = Math.floor(nowMs / 1000);
  if (iss !== ACCESS_TOKEN_ISSUER || aud !== BROKER_AUDIENCE) return null;
  if (!isString(sub, 64) || !isString(sid, 64) || !isString(dkh, 64)) return null;
  if (!Number.isInteger(iat) || !Number.isInteger(exp) || exp <= nowSeconds || iat > nowSeconds + 30) return null;
  return { accountId: sub, sessionId: sid, deviceKeyThumbprint: dkh };
}

function mintFriendCode() {
  let value = BigInt(`0x${randomBytes(6).toString('hex')}`) >> 3n;
  let encoded = '';
  for (let i = 0; i < 9; i += 1) {
    encoded = FRIEND_CODE_ALPHABET[Number(value & 31n)] + encoded;
    value >>= 5n;
  }
  const check = [...encoded].reduce((total, character) =>
    (total * 33 + FRIEND_CODE_ALPHABET.indexOf(character)) % 32, 0);
  return `KB-${encoded}${FRIEND_CODE_ALPHABET[check]}`;
}

function bearerToken(request) {
  const value = request.headers.authorization;
  const match = typeof value === 'string' && /^Bearer ([A-Za-z0-9._-]{1,16384})$/.exec(value);
  return match?.[1] ?? null;
}

async function transaction(pool, callback) {
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
    if (active) await client.query('ROLLBACK').catch(() => {});
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
    const created = await client.query(
      `INSERT INTO accounts (id, friend_code, created_at, updated_at)
       VALUES ($1, $2, $3, $3)
       ON CONFLICT (friend_code) DO NOTHING
       RETURNING id`,
      [accountId, mintFriendCode(), new Date(now)],
    );
    if (created.rowCount === 0) continue;

    const linked = await client.query(
      `INSERT INTO login_identities (id, account_id, provider, subject_hash, created_at)
       VALUES ($1, $2, 'oidc', $3, $4)
       ON CONFLICT (provider, subject_hash) DO NOTHING
       RETURNING account_id`,
      [randomUUID(), accountId, digest, new Date(now)],
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
  return result.rowCount > 0;
}

async function createSession(client, accountId, thumbprint, now, signingPrivateKey) {
  const sessionId = randomUUID();
  const refreshToken = randomBytes(32).toString('base64url');
  await client.query(
    `INSERT INTO sessions
       (session_id, account_id, device_key_thumbprint, refresh_hash,
        previous_refresh_hash, revoked, created_at, expires_at, updated_at)
     VALUES ($1, $2, $3, $4, ARRAY[]::text[], false, $5, $6, $5)`,
    [sessionId, accountId, thumbprint, sha256(refreshToken),
      new Date(now), new Date(now + REFRESH_TOKEN_TTL_MS)],
  );
  return {
    accessToken: accessToken({ accountId, sessionId, deviceKeyThumbprint: thumbprint }, signingPrivateKey, now),
    refreshToken,
    expiresIn: ACCESS_TOKEN_TTL_SECONDS,
  };
}

function createRateLimiter(pool, limits, clock) {
  const attempts = Number.isInteger(limits?.attempts) && limits.attempts > 0 ? limits.attempts : 10;
  const windowMs = Number.isInteger(limits?.windowMs) && limits.windowMs > 0 ? limits.windowMs : 60_000;
  return async (accountId, scope) => {
    const now = clock();
    const result = await pool.query(
      `INSERT INTO rate_counters (account_id, scope, attempts, window_start, updated_at)
       VALUES ($1, $2, 1, $3, $3)
       ON CONFLICT (account_id, scope) DO UPDATE SET
         attempts = CASE WHEN rate_counters.window_start <= $4 THEN 1 ELSE rate_counters.attempts + 1 END,
         window_start = CASE WHEN rate_counters.window_start <= $4 THEN EXCLUDED.window_start ELSE rate_counters.window_start END,
         updated_at = EXCLUDED.updated_at
       RETURNING attempts`,
      [accountId, scope, new Date(now), new Date(now - windowMs)],
    );
    if (result.rows[0].attempts > attempts) fail(429, 'rate_limited');
  };
}

async function authenticate(request, pool, signingPublicKey, clock) {
  const token = bearerToken(request);
  if (!token) fail(401, 'unauthorized');
  const claims = validateAccessToken(token, signingPublicKey, clock());
  if (!claims) fail(401, 'unauthorized');

  const result = await pool.query(
    `SELECT account_id, device_key_thumbprint
       FROM sessions
      WHERE session_id = $1 AND account_id = $2
        AND revoked = false AND expires_at > $3`,
    [claims.sessionId, claims.accountId, new Date(clock())],
  );
  if (result.rowCount === 0 || result.rows[0].device_key_thumbprint !== claims.deviceKeyThumbprint) {
    fail(401, 'unauthorized');
  }
  return claims;
}

async function exportAccount(pool, accountId) {
  const profile = await pool.query(
    'SELECT friend_code, display_name, settings FROM accounts WHERE id = $1',
    [accountId],
  );
  if (profile.rowCount === 0) fail(401, 'unauthorized');

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
    profile: {
      friendCode: profile.rows[0].friend_code,
      displayName: profile.rows[0].display_name,
      settings: profile.rows[0].settings,
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

export function createBroker({ pool, identity, signingPrivateKey, pepper, clock = () => Date.now(), limits = {} }) {
  if (!pool || typeof pool.query !== 'function' || typeof pool.connect !== 'function') {
    throw new TypeError('createBroker requires a PostgreSQL pool');
  }
  if (!identity || !isString(identity.issuer, 512) || !isString(identity.audience, 512) || !identity.publicKey) {
    throw new TypeError('createBroker requires a fixed OIDC issuer, audience, and public key');
  }
  if (typeof pepper !== 'string' || Buffer.byteLength(pepper) < 16) {
    throw new TypeError('createBroker requires a subject pepper of at least 16 bytes');
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
  const rateLimit = createRateLimiter(pool, limits, clock);
  const preAuthWindows = new Map();

  function allowChallenge(thumbprint) {
    const now = clock();
    const attempts = Number.isInteger(limits?.attempts) && limits.attempts > 0 ? limits.attempts : 10;
    const windowMs = Number.isInteger(limits?.windowMs) && limits.windowMs > 0 ? limits.windowMs : 60_000;
    let window = preAuthWindows.get(thumbprint);
    if (!window && preAuthWindows.size >= 4096) {
      for (const [key, value] of preAuthWindows) {
        if (value.startedAt <= now - windowMs) preAuthWindows.delete(key);
      }
      if (preAuthWindows.size >= 4096) return false;
    }
    if (!window || window.startedAt <= now - windowMs) window = { startedAt: now, attempts: 0 };
    window.attempts += 1;
    preAuthWindows.set(thumbprint, window);
    return window.attempts <= attempts;
  }

  async function issueNonce(body) {
    if (!VALID_PURPOSES.has(body.purpose)) fail(400, 'bad_request');
    const device = parseDevicePublicKey(body.devicePublicKey);
    if (!device) fail(400, 'bad_request');
    if (!allowChallenge(device.thumbprint)) fail(429, 'rate_limited');

    const now = clock();
    const nonce = randomBytes(32).toString('base64url');
    await pool.query(
      `DELETE FROM auth_nonces
        WHERE expires_at <= $1 OR (consumed_at IS NOT NULL AND consumed_at <= $2)`,
      [new Date(now), new Date(now - AUTH_NONCE_TTL_MS)],
    );
    await pool.query(
      `INSERT INTO auth_nonces
         (id, nonce_hash, device_key_thumbprint, purpose, created_at, expires_at)
       VALUES ($1, $2, $3, $4, $5, $6)`,
      [randomUUID(), sha256(nonce), device.thumbprint, body.purpose,
        new Date(now), new Date(now + AUTH_NONCE_TTL_MS)],
    );
    return nonce;
  }

  async function signIn(body) {
    if (!isString(body.nonce, 256) || !isString(body.idToken, 16_384)) fail(400, 'bad_request');
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
      const account = await getOrCreateAccount(client, pepper, fixedIdentity, claims.sub, now);
      const knownDeviceForAccount = await client.query(
        `SELECT id FROM device_keys
          WHERE account_id = $1 AND device_key_thumbprint = $2
          FOR UPDATE`,
        [account.accountId, device.thumbprint],
      );
      const isNewDevice = knownDeviceForAccount.rowCount === 0;
      if (!await registerDevice(client, account.accountId, device, now)) fail(401, 'unauthorized');
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
      const tokens = await createSession(client, account.accountId, device.thumbprint, now, brokerSigningPrivateKey);
      return { tokens };
    });
    if (result.unauthorized) fail(401, 'unauthorized');
    return result.tokens;
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
        `SELECT session_id, account_id, device_key_thumbprint, refresh_hash,
                previous_refresh_hash, revoked, expires_at
           FROM sessions
          WHERE refresh_hash = $1 OR $1 = ANY(previous_refresh_hash)
          FOR UPDATE`,
        [tokenHash],
      );
      if (selected.rowCount === 0) return { unauthorized: true };
      const session = selected.rows[0];
      if (session.revoked || session.device_key_thumbprint !== device.thumbprint ||
          new Date(session.expires_at).getTime() <= now) return { unauthorized: true };
      const registered = await client.query(
        'SELECT id FROM device_keys WHERE account_id = $1 AND device_key_thumbprint = $2',
        [session.account_id, device.thumbprint],
      );
      if (registered.rowCount === 0) return { unauthorized: true };
      await client.query('UPDATE auth_nonces SET account_id = $2 WHERE id = $1', [consumed.id, session.account_id]);
      if (session.previous_refresh_hash.includes(tokenHash)) {
        await client.query('UPDATE sessions SET revoked = true, updated_at = $2 WHERE session_id = $1',
          [session.session_id, new Date(now)]);
        return { unauthorized: true };
      }
      if (session.refresh_hash !== tokenHash) return { unauthorized: true };

      const nextRefreshToken = randomBytes(32).toString('base64url');
      await client.query(
        `UPDATE sessions
            SET previous_refresh_hash = array_append(previous_refresh_hash, refresh_hash),
                refresh_hash = $2,
                expires_at = $3,
                updated_at = $4
          WHERE session_id = $1`,
        [session.session_id, sha256(nextRefreshToken), new Date(now + REFRESH_TOKEN_TTL_MS), new Date(now)],
      );
      return {
        tokens: {
          accessToken: accessToken({
            accountId: session.account_id,
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
    const result = await transaction(pool, async client => {
      const enrolled = await client.query(
        `SELECT d.device_public_key
           FROM sessions s
           JOIN device_keys d
             ON d.account_id = s.account_id
            AND d.device_key_thumbprint = s.device_key_thumbprint
          WHERE s.session_id = $1 AND s.account_id = $2
            AND s.device_key_thumbprint = $3 AND s.revoked = false AND s.expires_at > $4
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
        `SELECT account_id
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
           FROM sessions
          WHERE session_id = $1 AND account_id = $2
            AND device_key_thumbprint = $3 AND revoked = false AND expires_at > $4
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
      await client.query('DELETE FROM accounts WHERE id = $1', [accessClaims.accountId]);
      return { deleted: true };
    });
    if (result.unauthorized) fail(401, 'unauthorized');
    return { deleted: true };
  }

  async function handle(request, response) {
    try {
      let pathname;
      try {
        pathname = new URL(request.url ?? '/', 'http://broker.invalid').pathname;
      } catch {
        fail(400, 'bad_request');
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
        const nonce = await issueNonce(await readJson(request));
        sendJson(response, 200, { nonce });
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

      if (request.method === 'GET' && pathname === '/v1/account/export') {
        const claims = await authenticate(request, pool, brokerSigningPublicKey, clock);
        await rateLimit(claims.accountId, 'account_export');
        sendJson(response, 200, await exportAccount(pool, claims.accountId));
        return;
      }

      if (request.method === 'DELETE' && pathname === '/v1/account') {
        const claims = await authenticate(request, pool, brokerSigningPublicKey, clock);
        await rateLimit(claims.accountId, 'account_delete');
        sendJson(response, 200, await deleteAccount(await readJson(request), claims));
        return;
      }

      sendJson(response, 404, { error: 'not_found' });
    } catch (error) {
      const status = error instanceof HttpError ? error.status : 503;
      const code = error instanceof HttpError ? error.code : 'unavailable';
      sendJson(response, status, { error: code });
    }
  }

  return createServer((request, response) => {
    void handle(request, response);
  });
}
