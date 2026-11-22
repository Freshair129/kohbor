-- Migration 2: registration hardening. Migration 1 is immutable.

ALTER TABLE accounts ALTER COLUMN display_name DROP DEFAULT;
ALTER TABLE accounts ALTER COLUMN display_name DROP NOT NULL;
UPDATE accounts SET display_name = NULL WHERE display_name = '';
ALTER TABLE accounts ALTER COLUMN updated_at SET DEFAULT now();
ALTER TABLE accounts
  ADD CONSTRAINT accounts_friend_code_format
    CHECK (friend_code ~ '^KB-[0-9A-HJKMNP-TV-Z]{10}$'),
  ADD CONSTRAINT accounts_display_name_shape CHECK (
    display_name IS NULL OR (
      char_length(display_name) BETWEEN 1 AND 128
      AND octet_length(display_name) <= 512
      AND display_name = btrim(display_name)
      AND display_name !~ '[\u0001-\u001F\u007F-\u009F\u200B-\u200F\u202A-\u202E\u2060-\u2069\uFEFF]'
    )
  ),
  ADD CONSTRAINT accounts_settings_shape CHECK (
    jsonb_typeof(settings) = 'object' AND pg_column_size(settings) <= 2048
  );

ALTER TABLE login_identities
  ADD COLUMN hash_key_id smallint NOT NULL DEFAULT 1 CHECK (hash_key_id > 0),
  ADD CONSTRAINT login_identities_provider CHECK (provider IN ('oidc')),
  ADD CONSTRAINT login_identities_hash_hex CHECK (subject_hash ~ '^[0-9a-f]{64}$'),
  ADD CONSTRAINT login_identities_one_per_provider UNIQUE (account_id, provider);

ALTER TABLE device_keys
  ADD CONSTRAINT device_keys_thumbprint_hex
    CHECK (device_key_thumbprint ~ '^[0-9a-f]{64}$');

ALTER TABLE sessions ADD COLUMN device_key_id uuid;
UPDATE sessions s
   SET device_key_id = d.id
  FROM device_keys d
 WHERE d.account_id = s.account_id
   AND d.device_key_thumbprint = s.device_key_thumbprint;
DELETE FROM sessions WHERE revoked OR device_key_id IS NULL;
WITH duplicate_sessions AS (
  SELECT session_id,
         row_number() OVER (
           PARTITION BY device_key_id ORDER BY updated_at DESC, created_at DESC, session_id DESC
         ) AS position
    FROM sessions
)
DELETE FROM sessions s
 USING duplicate_sessions d
 WHERE s.session_id = d.session_id AND d.position > 1;
UPDATE sessions
   SET expires_at = LEAST(expires_at, created_at + INTERVAL '90 days');

CREATE TABLE refresh_tokens (
  token_hash char(64) PRIMARY KEY CHECK (token_hash ~ '^[0-9a-f]{64}$'),
  session_id uuid NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
  generation integer NOT NULL CHECK (generation >= 1),
  created_at timestamptz NOT NULL DEFAULT now(),
  rotated_at timestamptz,
  UNIQUE (session_id, generation)
);
CREATE UNIQUE INDEX refresh_tokens_one_current
  ON refresh_tokens (session_id) WHERE rotated_at IS NULL;
INSERT INTO refresh_tokens (token_hash, session_id, generation, created_at)
  SELECT refresh_hash, session_id, 1, updated_at FROM sessions;

ALTER TABLE sessions
  ALTER COLUMN device_key_id SET NOT NULL,
  ADD CONSTRAINT sessions_device_fk FOREIGN KEY (device_key_id)
    REFERENCES device_keys(id) ON DELETE CASCADE,
  ADD CONSTRAINT sessions_one_per_device UNIQUE (device_key_id),
  ADD CONSTRAINT sessions_expiry_after_start CHECK (expires_at > created_at),
  DROP COLUMN previous_refresh_hash,
  DROP COLUMN refresh_hash,
  DROP COLUMN revoked,
  DROP COLUMN device_key_thumbprint;
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);

ALTER TABLE auth_nonces
  DROP COLUMN attempts,
  ADD CONSTRAINT auth_nonces_expiry CHECK (expires_at > created_at),
  ADD CONSTRAINT auth_nonces_thumbprint_hex
    CHECK (device_key_thumbprint ~ '^[0-9a-f]{64}$');
CREATE INDEX auth_nonces_thumbprint_idx ON auth_nonces (device_key_thumbprint);
CREATE INDEX auth_nonces_account_idx ON auth_nonces (account_id) WHERE account_id IS NOT NULL;

ALTER TABLE rate_counters
  ADD CONSTRAINT rate_counters_attempts CHECK (attempts >= 0),
  ADD CONSTRAINT rate_counters_scope CHECK (scope IN (
    'profile_write', 'friend_code_rotate', 'session_admin',
    'account_export', 'account_delete', 'friend_request'
  ));
CREATE TABLE global_rate_counters (
  scope text PRIMARY KEY CHECK (scope IN ('account_create')),
  attempts integer NOT NULL CHECK (attempts >= 0),
  window_start timestamptz NOT NULL,
  updated_at timestamptz NOT NULL
);

-- Per-attempt timestamps make the explicitly rolling quotas exact. No source
-- address or unauthenticated caller state is persisted here.
CREATE TABLE rate_limit_events (
  id uuid PRIMARY KEY,
  account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  scope text NOT NULL CHECK (scope IN (
    'profile_write', 'friend_code_rotate', 'session_admin',
    'account_export', 'account_delete', 'friend_request'
  )),
  occurred_at timestamptz NOT NULL
);
CREATE INDEX rate_limit_events_window_idx
  ON rate_limit_events (account_id, scope, occurred_at);

CREATE INDEX friendships_friend_idx ON friendships (friend_id);
CREATE INDEX blocks_blocked_idx ON blocks (blocked_id);
CREATE INDEX share_entries_friend_idx ON share_entries (friend_id);
CREATE INDEX pending_recipient_idx ON pending_requests (recipient_id);
WITH duplicate_requests AS (
  SELECT id,
         row_number() OVER (
           PARTITION BY requester_id, recipient_id ORDER BY created_at DESC, id DESC
         ) AS position
    FROM pending_requests
)
DELETE FROM pending_requests p
 USING duplicate_requests d
 WHERE p.id = d.id AND d.position > 1;
ALTER TABLE pending_requests
  ADD CONSTRAINT pending_requests_one_per_pair UNIQUE (requester_id, recipient_id),
  ADD CONSTRAINT pending_requests_expiry CHECK (expires_at > created_at);
CREATE INDEX pending_expires_idx ON pending_requests (expires_at);
