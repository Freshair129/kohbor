import { createPrivateKey, createPublicKey } from 'node:crypto';
import pg from 'pg';
import { createBroker } from './broker.mjs';
import { migrate } from './migrate.mjs';

const { Pool } = pg;

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error('missing required configuration');
  return value.replaceAll('\\n', '\n');
}

function boundedInteger(name, value, minimum, maximum, fallback) {
  if (value === undefined || value === '') return fallback;
  if (!/^\d+$/.test(value)) throw new Error('invalid numeric configuration');
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < minimum || parsed > maximum) {
    throw new Error('numeric configuration out of range');
  }
  return parsed;
}

function localDatabaseUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error('invalid local database configuration');
  }
  const host = url.hostname.toLowerCase();
  const allowedHosts = new Set(['localhost', '127.0.0.1', '::1', '[::1]']);
  if (!['postgres:', 'postgresql:'].includes(url.protocol) || !allowedHosts.has(host) ||
      url.username !== 'kohbor_s2' || (url.port && url.port !== '5432') || url.search || url.hash ||
      url.pathname !== '/kohbor_s2') {
    throw new Error('database must be the local synthetic S2 database');
  }
  return value;
}

async function main() {
  if (process.env.BROKER_MODE !== 'local-synthetic') {
    throw new Error('broker mode must be local-synthetic');
  }

  const databaseUrl = localDatabaseUrl(required('DATABASE_URL'));
  const issuer = required('BROKER_IDENTITY_ISSUER');
  const audience = required('BROKER_IDENTITY_AUDIENCE');
  if (issuer !== 'synthetic-issuer' || audience !== 'synthetic-client') {
    throw new Error('identity must use the local synthetic issuer and audience');
  }
  const identityPublicKey = createPublicKey(required('BROKER_IDENTITY_PUBLIC_KEY'));
  const signingPrivateKey = createPrivateKey(required('BROKER_SIGNING_PRIVATE_KEY'));
  const pepper = required('BROKER_SUBJECT_PEPPER');
  const host = process.env.HOST || '127.0.0.1';
  if (!['127.0.0.1', '::1'].includes(host)) throw new Error('host must be loopback');
  const port = boundedInteger('PORT', process.env.PORT, 1, 65535, 8080);
  const limits = {
    attempts: boundedInteger('BROKER_RATE_ATTEMPTS', process.env.BROKER_RATE_ATTEMPTS, 1, 10_000, 10),
    windowMs: boundedInteger('BROKER_RATE_WINDOW_MS', process.env.BROKER_RATE_WINDOW_MS, 1000, 3_600_000, 60_000),
  };
  if (Buffer.byteLength(pepper) < 16 || identityPublicKey.asymmetricKeyType !== 'rsa' ||
      Number(identityPublicKey.asymmetricKeyDetails?.modulusLength ?? 0) < 2048 ||
      signingPrivateKey.asymmetricKeyType !== 'ed25519') {
    throw new Error('invalid local synthetic key configuration');
  }

  const pool = new Pool({
    connectionString: databaseUrl,
    max: boundedInteger('PG_POOL_MAX', process.env.PG_POOL_MAX, 1, 10, 5),
    connectionTimeoutMillis: 5000,
    idleTimeoutMillis: 10_000,
    query_timeout: 5000,
    application_name: 'kohbor-s2-local-broker',
  });

  try {
    await migrate(pool);
    await pool.query('SELECT 1');
    const server = createBroker({
      pool,
      identity: { issuer, audience, publicKey: identityPublicKey },
      signingPrivateKey,
      pepper,
      limits,
    });
    await new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(port, host, resolve);
    });

    const shutdown = () => {
      server.close(() => void pool.end().finally(() => process.exit(0)));
    };
    process.once('SIGINT', shutdown);
    process.once('SIGTERM', shutdown);
  } catch (error) {
    await pool.end().catch(() => {});
    throw error;
  }
}

main().catch(() => {
  process.stderr.write('kohbor broker failed to start; check local synthetic configuration and database readiness\n');
  process.exitCode = 1;
});
