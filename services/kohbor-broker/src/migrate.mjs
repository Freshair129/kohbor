import { readFile } from 'node:fs/promises';

const MIGRATION_LOCK = '6467885033235427409';

const REGISTRATION_HARDENING = await readFile(
  new URL('./migrations/002_registration_hardening.sql', import.meta.url),
  'utf8',
);
const EXPIRED_ROW_PURGE_INDEXES = await readFile(
  new URL('./migrations/003_expired_row_purge_indexes.sql', import.meta.url),
  'utf8',
);

const INITIAL_SCHEMA = [
  `CREATE TABLE accounts (
     id uuid PRIMARY KEY,
     friend_code text NOT NULL UNIQUE,
     display_name text NOT NULL DEFAULT '',
     settings jsonb NOT NULL DEFAULT '{}'::jsonb,
     created_at timestamptz NOT NULL DEFAULT now(),
     updated_at timestamptz NOT NULL
   )`,
  `CREATE TABLE login_identities (
     id uuid PRIMARY KEY,
     account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     provider text NOT NULL,
     subject_hash char(64) NOT NULL,
     created_at timestamptz NOT NULL,
     UNIQUE (provider, subject_hash)
   )`,
  `CREATE TABLE device_keys (
     id uuid PRIMARY KEY,
     account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     device_public_key text NOT NULL,
     device_key_thumbprint char(64) NOT NULL UNIQUE,
     created_at timestamptz NOT NULL,
     updated_at timestamptz NOT NULL,
     UNIQUE (account_id, device_key_thumbprint)
   )`,
  `CREATE TABLE sessions (
     session_id uuid PRIMARY KEY,
     account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     device_key_thumbprint char(64) NOT NULL,
     refresh_hash char(64) NOT NULL UNIQUE,
     previous_refresh_hash text[] NOT NULL DEFAULT ARRAY[]::text[],
     revoked boolean NOT NULL DEFAULT false,
     created_at timestamptz NOT NULL,
     expires_at timestamptz NOT NULL,
     updated_at timestamptz NOT NULL
   )`,
  `CREATE INDEX sessions_account_id_idx ON sessions(account_id)`,
  `CREATE TABLE auth_nonces (
     id uuid PRIMARY KEY,
     account_id uuid REFERENCES accounts(id) ON DELETE CASCADE,
     nonce_hash char(64) NOT NULL UNIQUE,
     device_key_thumbprint char(64) NOT NULL,
     purpose text NOT NULL CHECK (purpose IN ('sign_in', 'refresh', 'delete')),
     attempts integer NOT NULL DEFAULT 0,
     created_at timestamptz NOT NULL,
     expires_at timestamptz NOT NULL,
     consumed_at timestamptz
   )`,
  `CREATE INDEX auth_nonces_expiry_idx ON auth_nonces(expires_at)`,
  `CREATE TABLE friendships (
     account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     friend_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     created_at timestamptz NOT NULL DEFAULT now(),
     PRIMARY KEY (account_id, friend_id),
     CHECK (account_id <> friend_id)
   )`,
  `CREATE TABLE pending_requests (
     id uuid PRIMARY KEY,
     requester_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     recipient_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     created_at timestamptz NOT NULL,
     expires_at timestamptz NOT NULL,
     CHECK (requester_id <> recipient_id)
   )`,
  `CREATE TABLE blocks (
     blocker_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     blocked_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     created_at timestamptz NOT NULL,
     PRIMARY KEY (blocker_id, blocked_id),
     CHECK (blocker_id <> blocked_id)
   )`,
  `CREATE TABLE share_entries (
     account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     friend_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     pairing_id uuid NOT NULL,
     pairing_generation bigint NOT NULL DEFAULT 1,
     scopes jsonb NOT NULL DEFAULT '[]'::jsonb,
     paused boolean NOT NULL DEFAULT true,
     last_confirmed_at timestamptz,
     created_at timestamptz NOT NULL DEFAULT now(),
     updated_at timestamptz NOT NULL DEFAULT now(),
     PRIMARY KEY (account_id, friend_id),
     UNIQUE (pairing_id),
     CHECK (account_id <> friend_id)
   )`,
  `CREATE TABLE rate_counters (
     account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
     scope text NOT NULL,
     attempts integer NOT NULL,
     window_start timestamptz NOT NULL,
     updated_at timestamptz NOT NULL,
     PRIMARY KEY (account_id, scope)
   )`,
];

export async function migrate(pool) {
  if (!pool || typeof pool.connect !== 'function') {
    throw new TypeError('migrate requires a PostgreSQL pool');
  }

  const client = await pool.connect();
  let inTransaction = false;
  try {
    await client.query('BEGIN');
    inTransaction = true;
    await client.query('SELECT pg_advisory_xact_lock($1::bigint)', [MIGRATION_LOCK]);
    await client.query(`CREATE TABLE IF NOT EXISTS schema_migrations (
      version integer PRIMARY KEY,
      name text NOT NULL UNIQUE,
      applied_at timestamptz NOT NULL
    )`);

    const existing = await client.query('SELECT version, name FROM schema_migrations');
    const versions = new Map(existing.rows.map(row => [row.version, row.name]));
    if (versions.has(1) && versions.get(1) !== 's2_local_account_broker') {
      throw new Error('unexpected migration 1 name');
    }
    if (versions.has(2) && versions.get(2) !== 'registration_hardening') {
      throw new Error('unexpected migration 2 name');
    }
    if (versions.has(3) && versions.get(3) !== 'expired_row_purge_indexes') {
      throw new Error('unexpected migration 3 name');
    }
    if (!versions.has(1)) {
      for (const statement of INITIAL_SCHEMA) await client.query(statement);
      await client.query(
        'INSERT INTO schema_migrations (version, name, applied_at) VALUES ($1, $2, $3)',
        [1, 's2_local_account_broker', new Date()],
      );
    }
    if (!versions.has(2)) {
      await client.query(REGISTRATION_HARDENING);
      await client.query(
        'INSERT INTO schema_migrations (version, name, applied_at) VALUES ($1, $2, $3)',
        [2, 'registration_hardening', new Date()],
      );
    }
    if (!versions.has(3)) {
      await client.query(EXPIRED_ROW_PURGE_INDEXES);
      await client.query(
        'INSERT INTO schema_migrations (version, name, applied_at) VALUES ($1, $2, $3)',
        [3, 'expired_row_purge_indexes', new Date()],
      );
    }

    await client.query('COMMIT');
    inTransaction = false;
  } catch (error) {
    if (inTransaction) await client.query('ROLLBACK').catch(() => {});
    throw error;
  } finally {
    client.release();
  }
}
