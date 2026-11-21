---
title: "Account Database and Registration API Design"
doc_id: "ACCOUNT-DATABASE-DESIGN"
status: draft
updated: "2026-11-21"
authority: "Agent design by the backend designer session; PROPOSED; the owner has not reviewed it; it changes no baseline until recorded under DOCUMENT-CONTROL"
approval_status: "not_approved"
---

# Account database and registration API design

Role: backend designer for the account domain. This is agent work, not human, security or legal review. Nothing here was applied to any database, and no code, migration file or claim was changed. Governing design: [ACCOUNT-IDENTITY-REVIEW](ACCOUNT-IDENTITY-REVIEW.md) (cited as "rule N"), [BACKEND-STACK](BACKEND-STACK.md), [REMAINING-SYSTEM-DESIGN](REMAINING-SYSTEM-DESIGN.md) ADR-A and D8. The registration page and its states are owned by the frontend designer ([ACCOUNT-REGISTRATION-UX](ACCOUNT-REGISTRATION-UX.md), a draft written in parallel); section 7 defines only the server behaviour behind those states. That draft was skimmed, not reviewed, at the end of this work: its asks (a `Retry-After` for the countdown, no "last active" on device rows, the open grapheme-counting question, a 5-minute delete window after re-sign-in) are matched by sections 6, 7.3 and 13.

Evidence labels: **SOURCE** = read in a file at the cited path and line on 2026-11-21. **UNVERIFIED** = not confirmed; the confirming step is named. **PROPOSED** = this document's design, open to R2 review and owner decision. **CONFLICT** = two governing sources or a source and the code disagree; both locations are given and neither side is picked.

Paths: `broker/` means `F:\kohbor-wt\account-client\services\kohbor-broker\` at commit `b75e1c5` on branch `account/s2-local-broker` (not merged, read only). `client/` means `crates/kohbor-account/src/lib.rs` in this worktree.

## 0. Summary

The S2 local broker already implements most of the storage rules: keyed subject hash, no email or raw subject, device-bound rotating refresh tokens stored hashed, fresh-sign-in deletion with foreign-key cascade, per-account rate counters that charge denied calls. It does not yet expose most of what registration needs (display name, profile read, friend-code rotation, session list, sign-out, sign-out everywhere), and it has five design problems worth fixing before any hosted use:

1. **CONFLICT:** the account id is disclosed to the client. The access token's `sub` is the account UUID (broker/src/broker.mjs:187) and the client contract stores and sends it (client lib.rs:73, 116, 128, 180-200). Rule table 1.2 says the account id is seen by "Nobody outside the broker" (ACCOUNT-IDENTITY-REVIEW.md:43).
2. **CONFLICT:** "no tombstone" (ACCOUNT-IDENTITY-REVIEW.md:83) against "verify that backups cannot reintroduce deleted accounts into service" (BACKEND-STACK.md:64). Without some record of deletion, a restore of a 30-day backup brings deleted accounts back.
3. **GAP:** refresh-token history is an unbounded, unindexed array (broker/src/migrate.mjs:34; lookup at broker.mjs:577), and revoked sessions and device keys are never removed until account deletion.
4. **GAP:** reuse of a rotated refresh token from a different device key is refused but does not revoke the session (broker.mjs:583-584); rule 12 says reuse revokes.
5. **GAP:** the sign-in nonce lifetime equals the browser hand-off timeout (broker.mjs:17 and client lib.rs:14, both 300 s), so a user finishing near the end of the hand-off is refused.

The schema delta in section 3 is one migration (or a fold into migration 1, see section 12). It is PROPOSED and not applied.

## 1. Current schema as found

### 1.1 Tables (broker/src/migrate.mjs, migration version 1 `s2_local_account_broker`)

| Table | Lines | Columns and constraints as found | Notes |
| --- | --- | --- | --- |
| `accounts` | 4-11 | `id uuid PK`; `friend_code text NOT NULL UNIQUE`; `display_name text NOT NULL DEFAULT ''`; `settings jsonb NOT NULL DEFAULT '{}'`; `created_at timestamptz NOT NULL DEFAULT now()`; `updated_at timestamptz NOT NULL` (no default) | No CHECK on friend-code format, name length or settings size. Empty string stands for "no name yet" |
| `login_identities` | 12-19 | `id uuid PK`; `account_id uuid NOT NULL → accounts ON DELETE CASCADE`; `provider text NOT NULL`; `subject_hash char(64) NOT NULL`; `created_at NOT NULL`; `UNIQUE (provider, subject_hash)` | Code writes `provider = 'oidc'` (broker.mjs:272, 293). No index on `account_id`; no limit of one identity per account |
| `device_keys` | 20-28 | `id uuid PK`; `account_id → accounts ON DELETE CASCADE`; `device_public_key text`; `device_key_thumbprint char(64) NOT NULL UNIQUE`; timestamps; `UNIQUE (account_id, device_key_thumbprint)` | Thumbprint is globally unique, so one device key belongs to at most one account |
| `sessions` | 29-40 | `session_id uuid PK`; `account_id → accounts ON DELETE CASCADE`; `device_key_thumbprint char(64)`; `refresh_hash char(64) UNIQUE`; `previous_refresh_hash text[] DEFAULT {}`; `revoked boolean`; `created_at`, `expires_at`, `updated_at`; index on `account_id` | No FK to `device_keys`; previous hashes grow by one per refresh; revoked rows stay |
| `auth_nonces` | 41-52 | `id uuid PK`; `account_id → accounts ON DELETE CASCADE` (nullable); `nonce_hash char(64) UNIQUE`; `device_key_thumbprint char(64)`; `purpose CHECK IN ('sign_in','refresh','delete')`; `attempts integer DEFAULT 0`; `created_at`, `expires_at`, `consumed_at`; index on `expires_at` | `attempts` is never written by the code. Expired rows are deleted lazily at the next nonce issue (broker.mjs:487-491) |
| `friendships` | 53-59 | PK `(account_id, friend_id)`, both FK CASCADE, `CHECK account_id <> friend_id` | No index on `friend_id` (cascade from the friend side scans) |
| `pending_requests` | 60-67 | `id uuid PK`, `requester_id`, `recipient_id` both FK CASCADE, `created_at`, `expires_at`, self-check | No uniqueness per pair; no index on either FK; no purge of expired rows |
| `blocks` | 68-74 | PK `(blocker_id, blocked_id)`, both FK CASCADE, self-check | No index on `blocked_id` |
| `share_entries` | 75-88 | PK `(account_id, friend_id)`, both FK CASCADE; `pairing_id uuid UNIQUE`; `pairing_generation bigint DEFAULT 1`; `scopes jsonb`; `paused boolean DEFAULT true`; `last_confirmed_at`; timestamps | One pairing id per directed row, see 1.4 item 9 |
| `rate_counters` | 89-96 | PK `(account_id, scope)`; FK CASCADE; `attempts integer`; `window_start`; `updated_at` | No CHECK on scope or attempts |
| `schema_migrations` | 110-114 | `version integer PK`, `name UNIQUE`, `applied_at` | The runner knows only version 1 (116-126); a second migration needs runner code |

### 1.2 HTTP surface (broker/src/broker.mjs:691-746; broker/README.md:5)

| Route | Lines | Behaviour as found |
| --- | --- | --- |
| `GET /health` | 700-708 | `200 {status:"ok"}` while `SELECT 1` works, else `503 {error:"unavailable"}` |
| `POST /v1/auth/nonces` | 479-500, 710-714 | Body `{purpose, devicePublicKey}` (Ed25519 SPKI PEM). Pre-auth limit in process memory keyed by device thumbprint (460-477). Returns `{nonce}`; stores SHA-256 of the nonce, 5-minute expiry |
| `POST /v1/auth/sessions` | 502-557 | Body `{idToken, nonce, devicePublicKey, deviceProof}`. Consumes the nonce, verifies the ID token (RS256 against one injected key, `iss`, `aud`, `nonce`, `exp`, `iat` within 300 s: 158-179), refuses a device key registered to another account (528-531), creates the account on first sign-in with a minted friend code (266-311), registers the device, pauses and advances every Share entry of the account when the device is new (541-550), creates a session. Returns `{accessToken, refreshToken, expiresIn}` only |
| `POST /v1/auth/refresh` | 559-622 | Body `{refreshToken, nonce, devicePublicKey, deviceProof}`. Rotates; a hash found in `previous_refresh_hash` from the bound device sets `revoked = true` |
| `GET /v1/account/export` | 384-429, 726-731 | Bearer token; per-account limit `account_export`; returns profile (friend code, display name, settings), friends, Share list, blocks. No account id, no pairing id |
| `DELETE /v1/account` | 624-689, 733-738 | Bearer token plus `{idToken, nonce, deviceProof}` with purpose `delete`; checks the ID token maps to the same account; deletes nonces bound to the account's device thumbprints (677-683), then `DELETE FROM accounts` (684) |
| anything else | 740 | `404 {error:"not_found"}` |

Errors are `{error: code}` with codes `bad_request` (400), `unauthorized` (401), `payload_too_large` (413), `unsupported_media_type` (415), `rate_limited` (429), `unavailable` (503 for any unexpected error, 742-744). The broker writes no logs at all; startup failure prints one fixed line (server.mjs:103-105).

### 1.3 Constants and configuration

Access token: Ed25519 JWT, issuer `kohbor-account-token-v1`, audience `broker`, 15 minutes, claims `sub` (account id), `sid`, `dkh`, `iat`, `exp` (broker.mjs:13-15, 181-196). Refresh: 32 random bytes, 30-day sliding expiry reset on each rotation (16, 598-607). Nonce: 5 minutes (17). Fresh re-auth: ID token `iat` at most 300 s old, applied to every ID token, not only delete (18, 176). Friend code: `KB-` + 9 symbols from a 32-symbol alphabet without I, L, O, U (45 random bits from `randomBytes(6) >> 3`) + 1 check symbol (20, 214-224). Rate limit: one shared setting for every scope, default 10 per 60 s (server.mjs:58-61). The server refuses any issuer other than `synthetic-issuer`, any non-loopback listener and any database other than local `kohbor_s2` (server.mjs:24-56).

### 1.4 Findings from reading the code

1. Account id in the access token `sub` (broker.mjs:187); the tests decode it to find rows (broker/test/broker.test.mjs:167-168, 197-198). See CONFLICT C1.
2. `display_name` defaults to `''` and has no length rule (migrate.mjs:7); there is no endpoint to set it, and the sign-in response does not tell the client a name is missing.
3. Refresh-token history is a `text[]` searched with `= ANY(...)` (migrate.mjs:34; broker.mjs:577): unbounded and unindexed.
4. Revocation sets a flag; no route signs out, revokes all sessions or removes device keys, so rows accumulate until account deletion.
5. Replay from a different device key returns 401 without revoking (broker.mjs:583-584).
6. The pre-auth limiter lives in process memory and is keyed by a device key the caller generates freely (460-477): it can be bypassed by minting new keys and resets on restart.
7. The check symbol is `(total*33 + v) mod 32` over the symbols (221-222). Because 33 ≡ 1 (mod 32) this is the plain sum of symbol values mod 32: every single wrong symbol is caught, no transposition is.
8. The deletion test matches rows by the account UUID value only (broker.test.mjs:208-209, 228-229); rows that hold only a device thumbprint or a hash of the account's tokens would pass unnoticed. The test does additionally check the subject digest is gone (232-235).
9. `share_entries.pairing_id` is unique per directed row (migrate.mjs:86), so the two directions of one friendship cannot share one pairing id. Rule 23 speaks of "a random pairing id" per pair. UNVERIFIED whether SHARING-MODEL-REVIEW intends one id per pair or per direction; not in registration scope; flagged for the sharing domain.
10. No client in either worktree calls these HTTP routes: the desktop client talks only to the fake broker through the `Broker` trait (client lib.rs:166-201), whose shape (account id arguments, `ProviderIdentity.subject_hash` at lib.rs:106) does not match the HTTP API. See CONFLICT C7.

### 1.5 What I could not check

- I did not run the broker tests, Node, npm, Postgres, Docker or any build. The S2 claim records "Postgres 7/7" (F:\kohbor-coord\claims\account__s2-local-broker.json); that is a claim, not evidence I checked. The claim's `last_commit` is `7c72109` while the branch head is `b75e1c5`; I report the mismatch and did not edit the claim.
- Real Google token behaviour (JWKS, the two documented `iss` spellings, `openid`-only consent) is not implemented in the broker and was not checked here; rule 2 and §6 items 1-3 of ACCOUNT-IDENTITY-REVIEW still hold as UNVERIFIED.
- Supabase Free logging, statement logging defaults, backup facilities, and Cloudflare Tunnel request logging (whether client IPs are recorded on the provider side): UNVERIFIED, no vendor page was read for this document.
- The desktop host glue (apps/desktop/src/account) was not read. The frontend designer's UX draft was only skimmed for API expectations.
- SHARING-MODEL-REVIEW was not read in full; rules 23-25 are assessed only as far as the broker schema shows.

## 2. Conformance

PASS = the code or schema meets the rule as written for the S2 local scope. GAP = missing or partial. UNVERIFIED = cannot be judged from what exists. CONFLICT = see section 2.3.

### 2.1 Rules 8-12 and 23-28

| Rule | Verdict | Evidence |
| --- | --- | --- |
| 8 raw subject never stored | PASS | `HMAC-SHA256(pepper, JSON [iss, sub])` (broker.mjs:101-105); only the digest is written (migrate.mjs:16); test asserts the raw subject is absent and the digest present (broker.test.mjs:130, 144-145). Proposal: add a version label and key id (section 3, `hash_key_id`) so the pepper can be rotated |
| 9 friends never see email, Google id, account id, Steam, devices, last-seen | PASS for export (friend rows carry code and name only, broker.mjs:391-396); UNVERIFIED for social routes (none exist) | |
| 10 exact code search, identical denial, limits, denied attempts consume | GAP for search (no route); PASS for the counter mechanism: the counter increments before the check and a refused call still increments (broker.mjs:351-361) | Limit values are DEC-SOCIAL proposals; the code uses one value for every scope (server.mjs:58-61) |
| 11 block, requests switch, code rotation | GAP: no rotation, block or switch route | Schema holds `blocks` and `settings` |
| 12 Kohbor session, 15 min access, 30-day idle refresh, device-bound, hashed, reuse revokes, one sharing session | PASS for issue, rotation, binding and hashing (broker.mjs:181-196, 328-344, 559-622); GAP for reuse from another device (583-584), unbounded history (migrate.mjs:34), no sign-out route, and "one active session for sharing" not modelled | |
| 23 pairing id per pair, no Steam id | PASS for no Steam field; UNVERIFIED for one id per pair (1.4 item 9) | |
| 24 pair secret derived on clients; service keeps public keys only | PASS by source: no pair-secret column; `device_public_key` only (migrate.mjs:23) | |
| 25 asymmetric relay tickets | UNVERIFIED: no ticket route exists (README.md:54-56) | Access tokens are Ed25519 with one audience (broker.mjs:181-196) |
| 26 allow-list and deny-list | PASS for the deny-list by source check and the closed column list test (broker.test.mjs:133-142); GAP: `auth_nonces` (nonce hash, device thumbprint, purpose) is persisted but not on the rule 26 allow-list (CONFLICT C4); note that the sliding `expires_at` necessarily encodes the last refresh time of each session (a single overwritten value, never history, never shown to friends) | |
| 27 delete with fresh sign-in, every row, no tombstone, backups purged in a stated period | PASS server-side for fresh sign-in and cascade (broker.mjs:624-689; test 190-236); CONFLICT C2 for backups; GAP for an authoritative result after an ambiguous failure (client lib.rs:195-200 requires one) | |
| 28 export of own profile, friends, Share settings, blocks | PASS (broker.mjs:384-429); export excludes account id and tokens (test 217-223) | Whether outgoing pending requests and own session list belong in an export is a DEC-PRIVACY-LEGAL question (section 13) |

### 2.2 ACCOUNT-001..010 (queued, ACCOUNT-IDENTITY-REVIEW.md:147-156)

| Id | Verdict | Evidence |
| --- | --- | --- |
| ACCOUNT-001 system browser, loopback, PKCE | UNVERIFIED on the broker (client-side duty); broker side PASS for a broker-issued, device-bound, one-use nonce (broker.mjs:250-264, 479-500); client has `pkce_s256` (lib.rs:842) | Hosted ID-token verification against Google keys does not exist yet (server.mjs:49-51 forces the synthetic issuer): expected for S2, a GAP for hosted |
| ACCOUNT-002 keyed hash, no email, profile or provider token | PASS | Same as rule 8; no token column |
| ACCOUNT-003 ids separate; only code and name disclosed | CONFLICT C1 | broker.mjs:187; client lib.rs:116 |
| ACCOUNT-004 random, non-sequential, checksummed, rotatable, Kohbor-only format | PASS for random 45 bits, `KB-` format and a check symbol; GAP for rotation (no route) and for transposition detection (1.4 item 7) | |
| ACCOUNT-005 Kohbor-issued, short, rotating, device-bound sessions; erased on sign-out | PASS server-side for issue; GAP for server-side sign-out | OS storage is client-side, UNVERIFIED here |
| ACCOUNT-006 offline and unavailable never block local use | UNVERIFIED (client duty); broker reports `503 unavailable` for any failure (broker.mjs:741-744) | |
| ACCOUNT-007 one issuer, one audience; others rejected | PASS for access tokens (test 104-108 refuses other `iss`, `aud: relay`, array audience) | The issuer string `kohbor-account-token-v1` is not yet a frozen label (Q-ACC-2 in the account card) |
| ACCOUNT-008 export and delete; delete removes every record; no tombstone | PASS server-side; CONFLICT C2 for backups | |
| ACCOUNT-009 no platform identity bound | PASS: no Steam column | |
| ACCOUNT-010 nothing shared or imported | PASS by source: synthetic keys only, no G-Maiden reference | Hosted configuration UNVERIFIED |

### 2.3 Conflicts (both locations given; neither side picked)

| Id | Source A | Source B | Effect |
| --- | --- | --- | --- |
| C1 | ACCOUNT-IDENTITY-REVIEW.md:43 account id is seen by "Nobody outside the broker" | broker.mjs:187 (`sub` = account id in a token the client can decode); client lib.rs:73, 116, 128, 180-200 (client stores and sends account id); tests rely on it (broker.test.mjs:167, 197) | Recommended in 14.1 (agent proposal, owner confirmation required): token `sub` = opaque session id; account id server-side only; client keys by a local reference, not the friend code (defect noted there) |
| C2 | ACCOUNT-IDENTITY-REVIEW.md:83 "No tombstone is kept" | BACKEND-STACK.md:64 "verify that backups cannot reintroduce deleted accounts into service" | Recommended in 14.2: minimal deletion ledger outside the database, 30-day purge, replayed after restore. **Amends rule 27: owner confirmation required** |
| C3 | docs/governance/DECISION-LOG.md:125 records 30-day encrypted backup retention | contracts/decisions.json:177 says the statement "sets the schedule only; DEC-RETENTION stays open for periods" | The structured record lags the log; document control repairs it. This design uses the recorded 30 days and invents no other value |
| C4 | ACCOUNT-IDENTITY-REVIEW.md:79 allow-list (no nonce records) | broker/src/migrate.mjs:41-52 and broker/README.md:7 (short-lived nonce hashes with device thumbprints are persisted) | Recommended in 14.3: nonces join the allow-list with short expiry and purge. Amends rule 26 text: owner confirmation via document control |
| C5 | REMAINING-SYSTEM-DESIGN.md:259-263 (D9: Rust `kohbor-social-core` with a fake) and :251 (option B: one Rust binary with SQLite) | Node.js broker with Postgres (broker/package.json; README.md:3); BACKEND-STACK.md:78 asks S2 to justify the language | Report only; not a registration decision |
| C6 | Broker nonce lifetime 300 s (broker.mjs:17) | Client hand-off timeout 300 s (client lib.rs:14; REMAINING-SYSTEM-DESIGN.md:273) | A sign-in finished near the timeout fails. Recommended in 14.5: nonce 600 s against a 300 s hand-off |
| C7 | HTTP API: bearer tokens, ID token sent to broker (broker.mjs:502-738) | Client `Broker` trait takes account ids and a `ProviderIdentity` whose `subject_hash` is filled before the broker is called (lib.rs:103-112, 166-201); the pepper is server-only (rule 8; BACKEND-STACK.md:60) | The client contract must be re-cut to the wire API before integration; who computes `subject_hash` on the client side is UNVERIFIED |

## 3. Proposed schema delta (PROPOSED, not applied)

### 3.1 Tables

Design choices made here (reversible, for R2 review):

- **Revocation deletes rows.** A revoked or signed-out session is deleted, not flagged. The outcome for any later use is the same 401. Fewer rows, nothing to purge, no history.
- **A session is tied to one device key row; signing out deletes the device key row.** Rule 14 wipes the local device key at sign-out, so the server-side public key has no further use. Consequence for UX: every sign-in after a sign-out is a "new device" and pauses the Share list (PEER-016). The UX owner should know this.
- **Refresh-token hashes move to their own table** with a primary-key lookup and one current token per session.
- **"No name yet" is `NULL`, not `''`.** Unknown is a real state.
- **No friend-code history is stored** (section 4).

```sql
-- PROPOSED migration 2 "registration_hardening". NOT APPLIED ANYWHERE.
-- Target: PostgreSQL, local disposable kohbor_s2 first; Supabase only after the owner's gate.
BEGIN;
SELECT pg_advisory_xact_lock(6467885033235427409);

-- 1. accounts: unknown name is NULL; shape checks.
ALTER TABLE accounts ALTER COLUMN display_name DROP DEFAULT;
ALTER TABLE accounts ALTER COLUMN display_name DROP NOT NULL;
UPDATE accounts SET display_name = NULL WHERE display_name = '';
ALTER TABLE accounts ALTER COLUMN updated_at SET DEFAULT now();
ALTER TABLE accounts
  ADD CONSTRAINT accounts_friend_code_format
      CHECK (friend_code ~ '^KB-[0-9A-HJKMNP-TV-Z]{10}$'),
  -- Storage backstop only. The application enforces 1..32 user-perceived
  -- characters (section 7.4); Thai clusters can take 3 code points each.
  ADD CONSTRAINT accounts_display_name_shape CHECK (
      display_name IS NULL OR (
        char_length(display_name) BETWEEN 1 AND 128
        AND octet_length(display_name) <= 512
        AND display_name = btrim(display_name)
        AND display_name !~ '[\u0001-\u001F\u007F-\u009F\u200B-\u200F\u202A-\u202E\u2060-\u2069\uFEFF]'
      )),
  ADD CONSTRAINT accounts_settings_shape CHECK (
      jsonb_typeof(settings) = 'object' AND pg_column_size(settings) <= 2048);

-- 2. login_identities: one identity per provider per account; pepper key id.
ALTER TABLE login_identities
  ADD COLUMN hash_key_id smallint NOT NULL DEFAULT 1 CHECK (hash_key_id > 0),
  ADD CONSTRAINT login_identities_provider CHECK (provider IN ('oidc')),
  ADD CONSTRAINT login_identities_hash_hex CHECK (subject_hash ~ '^[0-9a-f]{64}$'),
  ADD CONSTRAINT login_identities_one_per_provider UNIQUE (account_id, provider);
  -- the UNIQUE also indexes account_id for the cascade

-- 3. device_keys: shape check (thumbprint stays globally unique).
ALTER TABLE device_keys
  ADD CONSTRAINT device_keys_thumbprint_hex CHECK (device_key_thumbprint ~ '^[0-9a-f]{64}$');

-- 4. sessions: bind to the device row; drop the flag and the array.
ALTER TABLE sessions ADD COLUMN device_key_id uuid;
UPDATE sessions s SET device_key_id = d.id
  FROM device_keys d
 WHERE d.account_id = s.account_id AND d.device_key_thumbprint = s.device_key_thumbprint;
DELETE FROM sessions WHERE revoked OR device_key_id IS NULL;   -- synthetic data only (section 12)

CREATE TABLE refresh_tokens (
  token_hash  char(64)    PRIMARY KEY CHECK (token_hash ~ '^[0-9a-f]{64}$'),
  session_id  uuid        NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
  generation  integer     NOT NULL CHECK (generation >= 1),
  created_at  timestamptz NOT NULL DEFAULT now(),
  rotated_at  timestamptz,
  UNIQUE (session_id, generation)            -- also the FK index
);
CREATE UNIQUE INDEX refresh_tokens_one_current
  ON refresh_tokens (session_id) WHERE rotated_at IS NULL;
INSERT INTO refresh_tokens (token_hash, session_id, generation, created_at)
  SELECT refresh_hash, session_id, 1, updated_at FROM sessions;
  -- older hashes in previous_refresh_hash are dropped: reuse of a pre-migration
  -- token then reads as "unknown" (401) instead of "reuse" (401 + revoke)

ALTER TABLE sessions
  ALTER COLUMN device_key_id SET NOT NULL,
  ADD CONSTRAINT sessions_device_fk FOREIGN KEY (device_key_id)
      REFERENCES device_keys(id) ON DELETE CASCADE,
  ADD CONSTRAINT sessions_one_per_device UNIQUE (device_key_id),
  ADD CONSTRAINT sessions_expiry_after_start CHECK (expires_at > created_at),
  DROP COLUMN previous_refresh_hash,
  DROP COLUMN refresh_hash,
  DROP COLUMN revoked,
  DROP COLUMN device_key_thumbprint;          -- read through device_keys
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);   -- purge

-- 5. auth_nonces: drop the unused column; shape checks.
ALTER TABLE auth_nonces
  DROP COLUMN attempts,
  ADD CONSTRAINT auth_nonces_expiry CHECK (expires_at > created_at),
  ADD CONSTRAINT auth_nonces_thumbprint_hex CHECK (device_key_thumbprint ~ '^[0-9a-f]{64}$');
CREATE INDEX auth_nonces_thumbprint_idx ON auth_nonces (device_key_thumbprint);
CREATE INDEX auth_nonces_account_idx ON auth_nonces (account_id) WHERE account_id IS NOT NULL;

-- 6. Rate counters: closed scope list; a global counter with no identifier.
ALTER TABLE rate_counters
  ADD CONSTRAINT rate_counters_attempts CHECK (attempts >= 0),
  ADD CONSTRAINT rate_counters_scope CHECK (scope IN (
    'profile_write', 'friend_code_rotate', 'session_admin',
    'account_export', 'account_delete', 'friend_request'));
CREATE TABLE global_rate_counters (
  scope        text        PRIMARY KEY CHECK (scope IN ('account_create')),
  attempts     integer     NOT NULL CHECK (attempts >= 0),
  window_start timestamptz NOT NULL,
  updated_at   timestamptz NOT NULL
);

-- 7. Social tables touched only for cascade cost and duplicates.
CREATE INDEX friendships_friend_idx    ON friendships (friend_id);
CREATE INDEX blocks_blocked_idx        ON blocks (blocked_id);
CREATE INDEX share_entries_friend_idx  ON share_entries (friend_id);
CREATE INDEX pending_recipient_idx     ON pending_requests (recipient_id);
ALTER TABLE pending_requests
  ADD CONSTRAINT pending_requests_one_per_pair UNIQUE (requester_id, recipient_id),
  ADD CONSTRAINT pending_requests_expiry CHECK (expires_at > created_at);
CREATE INDEX pending_expires_idx       ON pending_requests (expires_at);

-- Not in this migration (sharing domain, S3): "one active session per account
-- for sharing" (rule 12) could be a column sessions.sharing_active boolean plus
--   CREATE UNIQUE INDEX ... ON sessions (account_id) WHERE sharing_active;
-- left out until the S3 ticket contract exists.

INSERT INTO schema_migrations (version, name, applied_at)
VALUES (2, 'registration_hardening', now());
COMMIT;
```

Notes on the SQL:

- Every foreign key to `accounts` is `ON DELETE CASCADE` (existing). The new chain is `accounts → device_keys → sessions → refresh_tokens`, all cascade, so sign-out (delete one `device_keys` row) and account deletion need one statement each.
- The SQL uses PostgreSQL ARE `\uXXXX` escapes with `standard_conforming_strings = on` (the default). UNVERIFIED until run on the local S2 database.
- `ALTER TABLE ... DROP COLUMN` and `ADD CONSTRAINT ... CHECK` take an `ACCESS EXCLUSIVE` lock. Acceptable only because no hosted database exists (section 12).
- `subject_hash` stays `HMAC-SHA256(pepper_k, JSON [canonical_issuer, sub])` where `k = hash_key_id`. PROPOSED: prefix the input with a version label (`"kohbor-login-identity-v1"`) before any real account exists; after that it cannot change without every user signing in again.
- Pepper rotation (PROPOSED design, owner custody): add key `k+1`; at sign-in compute both hashes, find the row by either, rewrite it to `k+1`. Accounts that never sign in again stay on `k`, so key `k` can be retired only when the owner accepts that those accounts become unreachable. Losing the pepper makes every account unreachable (users would get new, empty accounts). Pepper backup custody is an owner decision (section 13).

### 3.2 Access-token claims (PROPOSED, resolves C1 only if R2 and the owner agree)

`{iss: <account_token_domain label>, aud: "broker", sid, dkh, iat, exp}`: no `sub`. (Superseded in form by 14.1: the same opaque session id is carried as `sub` and `sid` is dropped; the effect is identical.) The broker reads the account from the session row (it already queries it at broker.mjs:371-377). The client keys its local blobs by a random local reference created at sign-in, not by the account id. The tests stop decoding `sub` and read the database directly.

## 4. Friend codes

### 4.1 Format and generation

- Format `KB-` + 9 data symbols + 1 check symbol, alphabet `0123456789ABCDEFGHJKMNPQRSTVWXYZ` (as found, broker.mjs:20). 9 symbols carry exactly 45 random bits from a CSPRNG (as found). The prefix is the proposed `friend_code_prefix` label (Q-ACC-2); until it is frozen the code treats `KB-` as a constant.
- Check symbol, PROPOSED replacement: `c = Σ w_i · v_i mod 32` with odd weights `w = 1, 3, 5, ..., 17`. Odd weights are invertible mod 32, so every single wrong symbol is caught; an adjacent swap is caught unless the two symbol values differ by exactly 16. The current plain sum catches no swap. This is a typing aid, not a security control. Change it before any real code is issued; afterwards both checks would have to be accepted.
- Input normalisation on lookup (PROPOSED): uppercase; drop spaces and hyphens after the prefix; map `O→0`, `I→1`, `L→1`; accept the code with or without `KB-`. A code that fails format or check is refused before any database read, as `invalid_code`. That answer reveals nothing about existence because no lookup happened, and it still charges the requester's limit.

### 4.2 Collision handling

- Insert and rotate both take `pg_advisory_xact_lock(key("friend-code", candidate))`, check `NOT EXISTS (SELECT 1 FROM accounts WHERE friend_code = candidate)`, then write. The `UNIQUE` constraint stays as a backstop. The current insert path uses `ON CONFLICT (friend_code) DO NOTHING` (broker.mjs:283-288), which is also fine.
- Rule: never rely on catching `unique_violation` (SQLSTATE 23505) in normal flow. PostgreSQL error details include the conflicting key value, which would put another user's friend code into the database server log. UNVERIFIED: what Supabase retains from server logs.
- Retry up to 5 candidates (as found), then fail with `503 unavailable`. With 2^45 codes, five consecutive collisions mean a broken random source, not bad luck, so the event is an operator alarm (logged as a count only, no code).

### 4.3 Rotation

- `UPDATE accounts SET friend_code = <new>, updated_at = now() WHERE id = <account>` under the lock above, in one transaction. The old code stops resolving at commit. Friendships, pending requests, blocks and Share entries are keyed by account id and are untouched (rule 11).
- No history of old codes is stored (decision made here, reversible): a table of retired codes would be a tombstone of sorts and would survive account deletion if unlinked. A retired code can in theory be minted again for someone else; the chance per new code is (number of retired codes) / 2^45, negligible at alpha scale. If the owner wants a guarantee, the alternative is a retired-code table holding only the code hash, which section 13 lists as an owner decision.
- Rate limit scope `friend_code_rotate`, charged before work, also when refused. Value: owner (no recorded proposal).
- Not idempotent. On an ambiguous failure (timeout, connection reset) the client must not retry the rotation; it reads `GET /v1/account` to learn the current code.

### 4.4 Identical denial (for the later friend-request route; recorded here because it is a property of the code store)

A lookup by code has exactly two outcomes visible to the requester: `sent` or `unavailable`. `unavailable` covers, with one body, one status (`200 {"result":"unavailable"}`, PROPOSED) and the same query path: unknown code; retired code; the requester's own code; a block in either direction; the target's "requests off" switch; the target at its friend or pending cap. Errors about the requester's own state stay distinct because they reveal nothing about anyone else: `rate_limited`, `limit_reached` (requester's own pending or friend cap), `invalid_code` (format, 4.1).

Mechanics: increment the requester's `friend_request` counter in its own committed statement before the lookup, so a denial (which may roll back the main transaction) cannot undo the charge. Run the same SELECTs (code lookup, block check both ways, settings read, cap counts) for every outcome, and decide only at the end.

## 5. Sessions and refresh rotation

### 5.1 Model

- One `device_keys` row per device enrolment, one `sessions` row per device row, one current `refresh_tokens` row per session plus its rotated predecessors.
- Sign-in: verify, create or find the account, insert the device row (refuse with the identical 401 if the thumbprint belongs to another account, as now at broker.mjs:528-531), insert the session, insert the refresh token at generation 1.
- Refresh (one transaction, `SELECT ... FOR UPDATE` on the session):
  1. Consume the refresh nonce bound to the presented device thumbprint.
  2. `SELECT r.session_id, r.rotated_at, s.*, d.device_key_thumbprint FROM refresh_tokens r JOIN sessions s USING (session_id) JOIN device_keys d ON d.id = s.device_key_id WHERE r.token_hash = $1`.
  3. Not found → 401.
  4. Found with `rotated_at IS NOT NULL` → **reuse**: `DELETE FROM device_keys WHERE id = s.device_key_id` (cascades the session and every token), commit, 401. This happens **whatever device key was presented** (fixes 1.4 item 5). Presenting a rotated token requires having had it, so it cannot be used to revoke someone else's session blindly.
  5. Found and current, but the device thumbprint differs or the session has expired → 401, no change.
  6. Otherwise set `rotated_at = now()` on the current row, insert the new hash at `generation + 1`, extend `expires_at` by the idle window, issue a new access token.
- Rotated rows are kept while the session lives, so reuse is detected for the whole session. PROPOSED bound: keep the newest N rotated rows per session (N is an implementation constant for R2; an older replay then reads as "unknown", still 401 but without revocation).
- Expired sessions are purged by a periodic `DELETE FROM device_keys d USING sessions s WHERE s.device_key_id = d.id AND s.expires_at <= now()`; nonces by `DELETE FROM auth_nonces WHERE expires_at <= now()`; expired pending requests the same way. Schedule and job host: owner/R2 (no new retention value is set here: these rows already carry their expiry).
- Values: access 15 minutes and refresh 30 days idle are proposals the owner still confirms (ACCOUNT-IDENTITY-REVIEW.md:56, 164). There is no absolute session lifetime today; whether to have one is an owner decision.

### 5.2 Concurrency

Two refreshes racing with the same current token: the second waits on `FOR UPDATE`, then sees `rotated_at` set and is treated as reuse, which revokes the session. This is the intended strict behaviour, but a client that retries a refresh after a timeout will sign itself out. The client must therefore never retry a refresh with the same token after an ambiguous failure; it keeps the old token and treats the state as `signed_in_offline` until the next scheduled attempt, which will fail with 401 if the first call had committed. UNVERIFIED that the client does this (ADR-A says "backoff 5 s to 5 min", REMAINING-SYSTEM-DESIGN.md:275). An optional grace window for an exact duplicate is an R2 choice; it weakens reuse detection and is not proposed here.

## 6. Rate-limit counters

| Scope | Key | Where | Charged when | Value |
| --- | --- | --- | --- | --- |
| pre-auth nonce issue | device thumbprint | process memory (as found) | every call, before work | Current default 10/60 s; owner |
| pre-auth, per source (PROPOSED, optional) | keyed hash of the client address with a key regenerated at process start; window lifetime only; never written to disk or logs | process memory | every call | Owner; needs DEC-PRIVACY-LEGAL because an address is personal data even when held briefly |
| `account_create` | none (global) | `global_rate_counters` | when a sign-in would create an account | Owner. Trade-off: a global cap lets an attacker with many Google accounts block new sign-ups; Google's 100-tester cap bounds this in alpha |
| `profile_write` | account | `rate_counters` | every call, before validation | Owner |
| `friend_code_rotate` | account | `rate_counters` | every call | Owner |
| `session_admin` (list, sign out, sign out everywhere) | account | `rate_counters` | every call | Owner |
| `account_export` | account | `rate_counters` (as found) | every call | Owner |
| `account_delete` | account | `rate_counters` (as found) | every call, before the ID-token check | Owner |
| `friend_request` (later) | account | `rate_counters` | every call, in its own committed statement (4.4) | 10 per minute proposed in rule 10; owner (DEC-SOCIAL) |

Rules:

- Denied attempts consume the limit. The existing upsert already does this (broker.mjs:351-361). Keep the increment outside any transaction that a denial can roll back.
- A limited call answers `429 rate_limited` with `Retry-After` in whole seconds. That reveals only the caller's own window.
- Counters are deleted with the account (FK cascade). `global_rate_counters` holds no identifier.
- The in-memory pre-auth limiter is bypassable (1.4 item 6). It is a cost raiser, not a control. The real controls are the one-use device-bound nonce, Google's own limits, and `account_create`.

## 7. Registration API contract (PROPOSED)

### 7.1 Common rules

- Base path `/v1`, JSON only, `cache-control: no-store` (as found). Called only by the native host. The broker sends no CORS headers, so a web page cannot read its responses. The website entry page calls nothing.
- Error body: `{"error": "<code>"}` plus `{"reason": "<code>"}` only where listed. Codes: `bad_request` 400, `invalid_display_name` 400, `invalid_code` 400, `unauthorized` 401, `account_not_found` 404 (probe only), `not_found` 404 (unknown route), `payload_too_large` 413, `unsupported_media_type` 415, `rate_limited` 429, `unavailable` 503.
- **Identical 401.** Every authentication failure on every route has one status, one body `{"error":"unauthorized"}`, and no reason header. This covers: bad, expired, wrong-audience or wrong-issuer tokens; unknown, used or expired nonces; a bad device proof; a device key that belongs to another account; refresh reuse; an expired or deleted session; a deleted account; and a delete re-auth that is the wrong Google account or too old. PROPOSED: run the ID-token verification even when the nonce is already known to be bad, to keep timing similar (today the nonce is consumed first, broker.mjs:510-516).
- Every response body and log line is free of: account id, session id, subject or its hash, device public key or thumbprint, nonce, tokens, ID token, device proof.
- **Logged** (every route, PROPOSED): timestamp, a random request id not linked to any user, method, route template, status, error code, duration bucket. Aggregate counters for 401 and 429 per route.
- **Never logged**: request or response bodies, `Authorization` header, query strings, client address, user agent, account id, session id, friend code, display name, subject or hash, device key or thumbprint, nonces, tokens, and PostgreSQL statement parameters. The database role should run with statement logging off (UNVERIFIED on Supabase). Log retention is open (BACKEND-STACK.md:65).

### 7.2 Frontend states and API behaviour

| Page state | Calls | Server behaviour the state relies on |
| --- | --- | --- |
| Guest | none | — |
| Start sign-in | `POST /v1/auth/nonces {purpose:"sign_in"}` | 200 → open browser. 429 → "rate limited". 503 or no connection → "service unavailable"; the browser is not opened |
| Waiting in the browser | none (host loopback listener, `state`, PKCE are local) | Nonce stays valid for the hand-off time plus a margin (7.3). Cancel: the client drops the nonce; no call; it expires unused |
| Browser closed or timed out | none | Client-local timeout; the nonce expires on its own |
| Google declined | none | The loopback receives `error=access_denied`; client-local |
| Completing | `POST /v1/auth/sessions` | 200 with profile. 401 → one "could not sign in" message. 503 → "service unavailable" |
| Choose display name | `PUT /v1/account/display-name` | 200, or `400 invalid_display_name` with `reason`, or 429 |
| Account ready | `GET /v1/account`; copy is local; "Get a new code" → `POST /v1/account/friend-code` | Rotation result is the new code |
| Signed in but offline | `POST /v1/auth/refresh` on the client's backoff schedule only | No connection or 503 keeps the offline state; 401 → signed out (`session_revoked`) |
| Account screen | `GET /v1/sessions`; `POST /v1/sessions/sign-out-everywhere`; `GET /v1/account/export`; delete = nonce `purpose:"delete"` + browser re-auth + `DELETE /v1/account` | Endpoints 6-9 in 7.3; delete is refused unless the Google re-sign-in is at most 5 minutes old |
| Errors | — | Browser closed or timed out: local. Service unavailable: 503 or transport failure. Google declined: local. Name invalid: 400 `invalid_display_name`. Rate limited: 429 |

### 7.3 Endpoints

**1. Start sign-in: `POST /v1/auth/nonces`** (exists)
- Request: `{"purpose": "sign_in" | "refresh" | "delete", "devicePublicKey": "<Ed25519 SPKI PEM>"}`. `state` and the PKCE verifier are generated and checked by the host only and never sent to the broker.
- Response 200: `{"nonce": "<base64url 32 bytes>", "expiresIn": <seconds>}` (`expiresIn` is new).
- Errors: 400 `bad_request`, 429, 503.
- Change (C6, PROPOSED): nonce lifetime for `sign_in` and `delete` = client hand-off timeout + a margin for code exchange and the broker call. Both values are owner-confirmed session values; the rule to fix now is only that the nonce must outlive the hand-off.
- Idempotency: each call mints a new nonce; unused nonces expire.
- Logged: route, status. Never: the key, thumbprint, nonce.

**2. Complete sign-in: `POST /v1/auth/sessions`** (exists; response extended)
- Request: `{"idToken", "nonce", "devicePublicKey", "deviceProof", "create": true}`. `create` defaults to `true`; `false` is the probe in 7.4.
- Response 200: `{"accessToken", "refreshToken", "expiresIn", "profile": {"friendCode": "KB-…", "displayName": null | "<name>"}, "newDevice": true | false}`. `displayName: null` puts the page into "choose display name". `newDevice: true` means the Share list was paused (`paused_new_device`). No account id.
- Errors: 400 `bad_request` (shape only), identical 401, 404 `account_not_found` only when `create:false` and the fully verified fresh identity has no account, 429 (`account_create`), 503.
- Indistinguishable: every 401 cause listed in 7.1.
- Idempotency: the nonce is one-use. A retry after a lost response needs a new nonce and a new browser sign-in. The earlier session, if it committed, is orphaned and is replaced when the same device key signs in again (`sessions_one_per_device`: the new sign-in deletes the old session row of that device first).
- Logged: status, whether an account was created (count only). Never: anything from the ID token.

**3. Set display name: `PUT /v1/account/display-name`** (new; bearer)
- Request: `{"displayName": "<string>"}`.
- Server validation (PROPOSED): Unicode NFC normalisation; trim; 1-32 extended grapheme clusters; no control, format, bidi-override or zero-width characters (the same classes as the SQL backstop); not required to be unique (no name search exists, and uniqueness would turn names into an existence oracle). "1 to 32 characters" in rule table 1.2 does not say graphemes or code points: owner/UX decision; graphemes are proposed because Thai clusters span several code points.
- Response 200: `{"displayName": "<stored value>"}`.
- Errors: 400 `invalid_display_name` with `reason` ∈ `empty`, `too_long`, `disallowed_character`; identical 401; 429 `profile_write`; 503. The reason is about the user's own input and leaks nothing.
- Idempotent: the same value gives the same result.
- Logged: status, reason code. Never: the name.

**4. Read own profile: `GET /v1/account`** (new; bearer)
- Response 200: `{"friendCode", "displayName": null | string, "friendRequestsAllowed": bool}` (the last field comes from `settings`, default true; it belongs to the social domain and can be omitted until that exists).
- Errors: identical 401, 503. No rate limit proposed (read of own row), or `session_admin` if R2 prefers.
- Idempotent (read). Logged: status. Never: the body.

**5. Rotate friend code: `POST /v1/account/friend-code`** (new; bearer)
- Request: `{}`. Response 200: `{"friendCode": "KB-…"}`.
- Errors: identical 401, 429 `friend_code_rotate`, 503 (including five failed candidates).
- Not idempotent (4.3); the client re-reads with endpoint 4 after an ambiguous failure.
- Logged: status. Never: old or new code.

**6. List sessions: `GET /v1/sessions`** (new; bearer)
- Response 200: `{"sessions": [{"current": bool, "createdOn": "YYYY-MM-DD"}]}`, newest first. No device name (none is collected), no session id, no address, no "last active". Whether to show a coarse last-active day is an owner decision: it would expose the single stored value implied by `expires_at`, which rule 26 does not forbid (it forbids history) but which the page does not need.
- Errors: identical 401, 429 `session_admin`, 503. Idempotent.

**7a. Sign out this device: `POST /v1/sessions/sign-out`** (new; bearer). Deletes this session's `device_keys` row (cascade). `204`. The client treats 204 and 401 alike as "signed out" and wipes locally either way (ADR-A best-effort revoke, REMAINING-SYSTEM-DESIGN.md:277).

**7b. Sign out everywhere: `POST /v1/sessions/sign-out-everywhere`** (new; bearer)
- Deletes every `device_keys` row of the account (cascade to sessions and refresh tokens), including the caller's. `204`.
- Errors: identical 401, 429 `session_admin`, 503.
- Idempotent in effect: a retry after success gets 401, which the client also treats as done.
- Logged: status, number of sessions ended (count). Never: identifiers.

**8. Export: `GET /v1/account/export`** (exists; body versioned)
- Response 200: `{"exportVersion": 1, "exportedAt": "<UTC>", "profile": {"friendCode", "displayName", "settings"}, "friends": [{"friendCode", "displayName"}], "shareList": [{"friendCode", "displayName", "scopes", "paused", "lastConfirmedAt"}], "blocks": [{"friendCode", "displayName"}]}`. Possible additions (outgoing pending requests, own session summary) are listed in section 13 as DEC-PRIVACY-LEGAL questions.
- Errors: identical 401, 429 `account_export`, 503. Idempotent read.
- Logged: status. Never: the body.

**9. Delete: `DELETE /v1/account`** (exists)
- Preconditions: a nonce with purpose `delete`, a fresh Google sign-in in the browser with that nonce, an ID token with `iat` at most 300 s old (rule 27 "within 5 minutes"; as found broker.mjs:176, 652), a device proof from the enrolled device key, and a bearer token for a live session on that device.
- Request: `{"idToken", "nonce", "deviceProof"}`. Response 200: `{"deleted": true}`.
- Errors: identical 401 for every failure (wrong Google account, stale sign-in, nonce reused, session gone), 429 `account_delete`, 503.
- Idempotency without a tombstone: a second call gets 401. The client contract treats a transport failure as ambiguous (`DeletePending`, lib.rs:195-200). Reconciliation, PROPOSED:
  1. Try `POST /v1/auth/refresh` with the stored token. 200 → the account exists, deletion did not commit; clear `DeletePending` and tell the user it did not complete. 401 → still ambiguous.
  2. Ask the user for one more Google sign-in and call endpoint 2 with `create:false`. `404 account_not_found` → deletion committed; wipe locally. 200 → not deleted (and a new session exists); clear the marker and report. The probe's 404 goes only to someone holding a fresh ID token for that very subject, so it leaks nothing to anyone else.
- Logged: status, a deletion counter. Never: any identifier, not even a hash: a log line saying "hash X deleted" would be a tombstone.

### 7.4 Refresh (exists, behaviour changed per 5.1)

`POST /v1/auth/refresh {refreshToken, nonce, devicePublicKey, deviceProof}` → 200 `{accessToken, refreshToken, expiresIn}`; identical 401 for every failure; reuse revokes whatever key is presented; no retry with the same token after an ambiguous failure (5.2).

## 8. Deletion

### 8.1 Cascade map

| Store | How the account's data goes | Ordering |
| --- | --- | --- |
| `accounts` | `DELETE FROM accounts WHERE id = $1` | last statement |
| `login_identities` | FK cascade | — |
| `device_keys` | FK cascade | — |
| `sessions` | FK cascade (from `accounts` and from `device_keys`) | — |
| `refresh_tokens` | FK cascade from `sessions` | — |
| `auth_nonces` with `account_id` set | FK cascade | — |
| `auth_nonces` with `account_id` NULL but bound to one of the account's device thumbprints | explicit `DELETE ... WHERE device_key_thumbprint IN (SELECT ... FROM device_keys WHERE account_id = $1)` | **before** the account delete, while `device_keys` still exists (as found, broker.mjs:677-684) |
| `friendships` both columns | FK cascade | — |
| `pending_requests` both columns | FK cascade | — |
| `blocks` both columns | FK cascade | — |
| `share_entries` both columns | FK cascade | — |
| `rate_counters` | FK cascade | — |
| `global_rate_counters` | untouched; holds no identifier | — |
| `schema_migrations` | untouched; holds no user data | — |
| Process memory: pre-auth limiter entries for the account's thumbprints | PROPOSED: removed in the same request; otherwise they expire with their window | after commit |
| Logs | hold no identifier by rule 7.1 | — |
| Backups | 30-day rotation (section 9) | — |
| Friends' local caches | until their next sync (residual risk, ACCOUNT-IDENTITY-REVIEW §3) | — |

All in one transaction (as found, broker.mjs:629-686). Nonces of another device that never enrolled for this account are not linked to it and expire within the nonce lifetime.

### 8.2 Row-count deletion test (specification)

Class: synthetic integration against a disposable local Postgres (S2).

1. Fixture: accounts A (to delete) and B (to keep). Give A at least one row in **every** table: two device enrolments (one signed out and one live), a session with three refresh generations, an unconsumed and a consumed nonce bound to A's device with `account_id` NULL and one with `account_id` set, a friendship both ways with B, a pending request each way, a block each way, Share entries both ways, a counter in every scope, a rotated friend code. Give B the mirror rows with a third account C so B keeps rows after A goes. Give A and B display names containing a unique synthetic marker string.
2. Before: snapshot every base table in `public` from `information_schema.tables`, discovered dynamically.
3. **Closed table set:** assert the discovered set equals the expected set listed in the test. A new table without a deletion rule fails the test until someone adds it on purpose.
4. Run the full delete flow over HTTP.
5. After, for every table, count rows that contain any of A's markers, compared as values in any column: account id; subject digest; each device thumbprint; each device public key; each refresh-token hash; each nonce hash; A's current and former friend codes; A's display-name marker. Assert 0 for every table.
6. Assert B's rows are unchanged except for rows that pointed at A, which are gone, and that C's rows are untouched.
7. Assert `global_rate_counters` has no column whose name ends in `_id`, `_hash`, `thumbprint` or `code` (schema guard).
8. Assert a later sign-in with A's subject creates a **new** account with a different friend code, no friends and no Share entries (no resurrection).
9. Assert the old access and refresh tokens get the identical 401.

## 9. Backups and deletion

Recorded (DECISION-LOG.md:98, 125, 126; BACKEND-STACK.md:33, 64): nightly encrypted `pg_dump`; encrypted alpha backups kept 30 days, then deleted through scheduled rotation; deletion removes the account from the active database and the 30-day rotation applies to earlier snapshots; a restore rehearsal must show that backups cannot reintroduce deleted accounts. This document adds no retention value.

Consequences:

- A deleted account stays in up to 30 days of backups (plus the time a dump file takes to reach its rotation). The privacy page must say so; the wording is DEC-PRIVACY-LEGAL.
- The subject pepper is needed to use any restored row. Where it is backed up, and who holds it, is an owner decision (BACKEND-STACK.md:60, 130). A backup stored together with its pepper is a full identity-hash dataset.
- **CONFLICT C2.** A restore of snapshot *t* brings back every account deleted after *t*. With no tombstone, the database alone cannot tell which. Options, none picked:
  - (a) **Short-lived deletion list outside the database:** on deletion, append the keyed subject hash (never the account id or code) to an encrypted list stored beside the backups. Entries older than the backup retention are dropped, so the list never outlives the last backup that holds the account. After any restore, delete those accounts before the service reopens. This is a time-bounded tombstone and needs an owner decision against rule 27.
  - (b) **Never restore into service:** restores only go to an isolated database for inspection; a loss of the live database means users sign in again to new, empty accounts. No tombstone, but backups then protect almost nothing.
  - (c) **Restore and re-delete by hand:** keep no list, restore, and rely on users to delete again. This breaks the deletion promise and is listed only for completeness.
- The restore rehearsal (BACKEND-STACK.md:64) should use the 8.2 fixture: delete A after a dump, restore the dump into an isolated database, and show that the chosen option removes A before any service process can read the database.

## 10. Threat additions

| Attacker or failure | Goal | Method | Impact | Mitigation (PROPOSED unless marked as found) | Residual |
| --- | --- | --- | --- | --- | --- |
| Curious or compromised client, or anyone who reads a token | Learn the stable account id | Decode the access-token payload | Account id becomes a cross-session, cross-log join key, against rule table 1.2 | Drop `sub` (3.2); client uses a local reference | Session id is still visible to its own holder |
| Backup restore | Bring back deleted accounts | Restore an older dump | Broken deletion promise | Section 9 options; rehearsal with the 8.2 fixture | Depends on the owner's choice |
| Database or provider log reader | Read identifiers | Constraint-violation details or statement logging on the database server | Friend codes or hashes in provider logs | No reliance on `unique_violation` (4.2); statement logging off; parameters never logged | Provider defaults UNVERIFIED |
| Pepper loss | — | Secret file lost | Every account unreachable; users get new accounts | Owner-held backup of the pepper apart from database dumps | Owner custody decision |
| Pepper theft together with a dump | Re-identify | Compute `HMAC(pepper, iss+sub)` for known Google subjects | Links a known Google identity to a Kohbor friend graph | Pepper outside the checkout and outside dumps (BACKEND-STACK.md:60); key id for rotation (3.1) | Anyone with both still links known subjects |
| Pre-auth flooder | Exhaust database or nonce table | Generate fresh device keys and request nonces | Load; nonce table growth | In-memory per-key limit (as found) is bypassable; optional per-source in-memory limit (6); purge of expired nonces; one-use nonces | Distributed flooding; Cloudflare-side protection UNVERIFIED |
| Mass sign-up | Fill the database | Many Google accounts | Storage use | `account_create` global limit; Google's 100-tester cap in alpha | Global cap can block real sign-ups |
| Refresh racer | Lose own session | Client retries a refresh after timeout | Self-revocation | Client never reuses a token after an ambiguous refresh (5.2) | Mis-behaving client signs itself out |
| Thief holding refresh token and device key | Use the session | Steal both from the PC | Session use until detection | Device binding; reuse from any key revokes (5.1 step 4); sign out everywhere | Same-user malware (ACCOUNT-IDENTITY-REVIEW §3) |
| Display-name abuse | Impersonate or confuse | Bidi overrides, zero-width characters, look-alike names | Friend lists show misleading names | Character rules in 7.3 item 3 and the SQL backstop; names shown only with the friend code | Look-alike scripts remain possible |
| Session list as a presence signal | Learn activity | Read the list | Activity timing | List shows creation day only; never shown to friends | Owner decision on "last active" |
| Stale device keys | Keep public keys after sign-out | Never delete | Rows beyond need, against rule 26's intent | Sign-out deletes the device row (3, 7a) | None |
| Friend-code timing oracle (later) | Tell "unknown" from "blocked" | Measure response time | Denial reasons leak | One query path for every outcome (4.4) | Network jitter hides most, not all |

## 11. Test matrix (source and synthetic only)

Evidence classes: **SRC** source check (read or static assertion, no execution of the service); **SU** synthetic unit (pure function, no database); **SI** synthetic integration against a disposable local Postgres; **SD** synthetic Docker acceptance (built image, loopback-only namespace). No row here is native, live or a measurement. None was run for this document.

| # | Test | Class | Status |
| --- | --- | --- | --- |
| 1 | Closed column list: no column outside the allow-list (extend for `hash_key_id`, `device_key_id`, `generation`, `rotated_at`, `token_hash`) | SI | Exists (broker.test.mjs:133-142); extend |
| 2 | Migration text contains no `email`, `ip`, `steam`, `sub`, `token` column other than hashes | SRC | New |
| 3 | Subject stored only as HMAC; raw subject absent | SI | Exists (124-146) |
| 4 | Friend code: format regex, 45-bit entropy source, check symbol catches every single substitution, swap detection per 4.1 | SU | New |
| 5 | Friend-code normalisation (`O/I/L`, spaces, missing prefix) | SU | New |
| 6 | Collision path: forced duplicate candidate retries without raising `23505` | SI | New |
| 7 | Rotation: old code stops resolving; friendships, blocks and Share entries unchanged; counter charged | SI | New |
| 8 | Display-name validator: graphemes 1..32 with Thai samples; control, bidi and zero-width refused; NFC | SU | New |
| 9 | Display-name SQL backstop refuses what the validator refuses | SI | New |
| 10 | Sign-in response carries profile and `newDevice`, never an account id | SI | New |
| 11 | Access token has no `sub`; other `iss` or `aud` refused | SI | Partly exists (104-108) |
| 12 | Identical 401: byte-equal bodies across every listed cause | SI | New |
| 13 | Refresh rotation and device binding | SI | Exists (148-159) |
| 14 | Reuse from a **different** device key revokes the session | SI | New (today it does not) |
| 15 | Concurrent refresh with one token: one succeeds, session then revoked | SI | New |
| 16 | Sign-out deletes the device row; next sign-in on the same key is `newDevice: true` and pauses the Share list | SI | New |
| 17 | Sign out everywhere ends every session; old tokens get 401 | SI | New |
| 18 | Session list shows no identifiers and no last-active value | SI | New |
| 19 | Rate limit: refused calls still increment; per-scope values; `Retry-After` | SI | Partly exists (238-257) |
| 20 | Friend-request denial charge survives a rolled-back denial (later social route) | SI | New, social |
| 21 | Nonce outlives the hand-off by the margin (C6) | SU + SI | New |
| 22 | Delete requires fresh matching identity | SI | Exists (190-236) |
| 23 | Deletion row-count per 8.2, closed table set | SI | Replaces the value-match in 208-235 |
| 24 | Delete reconciliation: `create:false` probe gives 404 after delete, 200 before | SI | New |
| 25 | Export contains no account id, pairing id, token or hash; `exportVersion` present | SI | Partly exists (217-223) |
| 26 | Responses carry no CORS headers; `Origin` requests get no `Access-Control-*` | SI | New |
| 27 | Log capture during the whole suite contains none of the 7.1 never-logged values | SI | New |
| 28 | Migration 2 on a database at version 1 with synthetic rows; idempotent re-run | SI | New |
| 29 | Image runs the suite with no published port and no checkout mount | SD | Exists per README.md:58 (not run here) |
| 30 | Backup restore rehearsal with the 8.2 fixture | SI (isolated restore) | New; blocked on the C2 decision |

## 12. Migration and rollout

State: branch `account/s2-local-broker` at `b75e1c5`, not merged; only disposable local databases are expected to hold version 1 (UNVERIFIED: I did not check any machine for a persistent `kohbor_s2` database). No hosted database exists (docs/domains/account/CARD.md:31: "nothing is provisioned").

Plan, PROPOSED:

1. R2 review of this document, with the account steward and the client-contract owner present for C1 and C7.
2. Decide the form of the change:
   - **Fold** the delta into migration 1 before merge. Simplest; acceptable only if no database worth keeping is at version 1. Recommended for review because no real data exists, but R2 decides.
   - **Additive migration 2** as in section 3. Needs a migration runner that handles more than one version (migrate.mjs:116-126 knows only version 1).
3. Tests first: rows 4-28 of section 11 against the local disposable database, written by one session and implemented by another (the S2 pattern recorded in the claim).
4. The broker changes: routes 3-7, refresh per 5.1, token claims per 3.2 if C1 is resolved that way, nonce margin per C6, logging per 7.1.
5. A new claim for `services/kohbor-broker/**` by whoever implements; this design session does not write there.
6. Project gate; PR by the owner's authorization only.
7. Hosted application (Supabase) only after the owner's gates in BACKEND-STACK "Owner checklist" item 2, the C2 decision, and the restore rehearsal.

R2 or the owner must decide before step 4: C1 (account id to the client), C2 (backup restore), C4 (nonces in the allow-list), C6 margin value, fold or migration 2, the refresh-history cap N, display-name counting rule, and whether sign-out deletes the device row (and so re-pauses sharing at every re-sign-in).

## 13. UNVERIFIED items and open owner decisions

UNVERIFIED (confirming step in brackets):

1. The S2 test suite passes at `b75e1c5` [run it against a disposable local Postgres].
2. The SQL in section 3 parses and applies on the local S2 version of PostgreSQL [apply to a disposable database].
3. Google ID tokens: both documented `iss` spellings, JWKS rotation, `openid`-only consent, stable `sub` [one real sign-in in the Kohbor-owned project, DEC-GOOGLE-PROJECT].
4. Supabase Free statement and error logging, log retention, and backup facilities [vendor pages; BACKEND-STACK "Provider facts"].
5. Cloudflare Tunnel: whether client addresses are logged on the provider side and for how long [vendor terms].
6. How the desktop host fills `ProviderIdentity.subject_hash` and holds account ids today (C7) [read apps/desktop/src/account and the host glue].
7. One pairing id per pair versus per direction (1.4 item 9) [SHARING-MODEL-REVIEW owner].
8. Whether any persistent database already holds migration version 1 [ask the S2 implementer].

Owner decisions (named, not decided). "Agent recommendation" points to section 14: an agent proposal made after the owner said "let the agent decide for me", which under AGENTS.md is **not** owner approval. Each still needs the owner's confirmation.

| Decision | Home | Agent recommendation |
| --- | --- | --- |
| Backup restore against "no tombstone" (C2); amends rule 27 | DEC-RETENTION, DEC-PRIVACY-LEGAL | 14.2: deletion ledger, option (a) |
| Repair `contracts/decisions.json:177` to match the recorded 30 days (C3) | Document control | — (repair, not a choice) |
| Nonce records on the rule 26 allow-list (C4); amends rule 26 text | Document control; DEC-SOCIAL | 14.3: yes, short expiry and purge |
| Account id visible to the client or not (C1) | R2; DEC-ACCOUNT-IDENTITY confirmation | 14.1: not visible |
| Refresh reuse from any device key revokes | DEC-ACCOUNT-IDENTITY | 14.4: yes |
| Access 15 min, refresh 30 days idle, an absolute session lifetime or none | DEC-ACCOUNT-IDENTITY ("session and limit values") | none beyond the review's proposals; absolute lifetime stays open |
| Nonce lifetime against hand-off timeout (C6) | DEC-ACCOUNT-IDENTITY | 14.5: 600 s against 300 s |
| Rate-limit values in section 6 | DEC-ACCOUNT-IDENTITY, DEC-SOCIAL | 14.8: friend-code rotation 3 per day; every other value stays open |
| A per-source in-memory limiter that briefly handles client addresses | DEC-PRIVACY-LEGAL | open (legal) |
| Display-name counting rule and renaming | UX owner / owner | 14.6: 32 grapheme clusters, not unique, renaming allowed |
| Session list contents | Owner | 14.7: "this PC" / "other device" and sign-in date only |
| Export additions: outgoing pending requests, own session summary | DEC-PRIVACY-LEGAL | open (legal) |
| Retired friend codes: no history or a hash-only table; pending requests on rotation | Owner | 14.8: no history; pending requests survive rotation |
| Pepper custody, backup and rotation | Owner (secrets, BACKEND-STACK.md:130) |
| Log retention and the monitoring provider | BACKEND-STACK.md:65, Group A |
| Freezing `account_token_domain` and `friend_code_prefix` | Q-ACC-2, DEC-NAME delegation |
| Implementation language and runtime of the broker (C5) | R2 / owner |
| Legal items: personal-data status of hashes and graphs, minors, notice wording for the 30-day backup tail, deletion deadlines | DEC-PRIVACY-LEGAL |
| Hosting, origin, any paid service | DEC-ACCOUNT-ORIGIN, DEC-STORE-HOSTING (not addressed here) |

## 14. Recommended resolutions (agent proposal, owner confirmation required)

**Owner confirmation recorded 2026-11-21 (chat, owner's words: "ยืนยันข้อ 1-5").** The owner confirmed five items put to them in that order: (1) the deletion ledger of 14.2, kept outside the database for 30 days and replayed after a restore; (2) nonces on the persistence allow-list with an expiry of at most 10 minutes; (3) the pause of the Share list on a new device (PEER-016) stays; (4) the daily new-friend-code limit counts a rolling 24 hours; (5) a lost device can only be signed out together with all others, per-device sign-out deferred. **Second confirmation, same day (owner's words: "ยืนยันตามคำแนะนำของagent")**: the owner confirmed the remaining recommendations of this section as the agent wrote them, including C1 (opaque session id as `sub`; the random local account reference instead of friend-code keying), C4, the refresh-reuse revoke, C6 and the name, device-row and rate-limit picks. Items this section leaves open (legal, hosting, retention values other than 30 days, pepper backup, log retention, session lifetimes and the other rate limits) were not covered and stay open. Confirmation in chat does not amend rule 27 or the allow-list by itself: that still needs document control (DOCUMENT-CONTROL) and a decision record, which this PR does not do.

Origin: the owner told the coordinator "let the agent decide for me" on the open items. Under AGENTS.md that is **not** owner approval, and it cannot be recorded as one. The picks below came from the coordinator and were checked by this design session; where a concrete defect was found the pick is adjusted and the defect stated. Legal questions, hosting, retention values other than the recorded 30 days, pepper backup and log retention are **not** resolved here and stay in section 13. Nothing in this section changes a baseline until the owner confirms it and document control records it.

### 14.0 Values by common practice (owner: "follow the usual standard", 2026-11-21)

The owner asked that the still-open numeric items take the usual industry defaults. These are common-practice values chosen by an agent. They were **not** checked against a source in this session (OAuth security guidance such as RFC 9700 recommends rotating refresh tokens for public clients; re-read it before citing), they were **not** measured, and they are tunable constants, not requirements. Legal, hosting and anything that needs a lawyer are **not** covered and stay open.

| Item | Value | Note |
| --- | --- | --- |
| Access token lifetime | 15 minutes | as in rule 12 |
| Refresh token | idle expiry 30 days, absolute cap 90 days, rotated on every use, reuse revokes the session | the cap is new |
| Sign-in nonce / browser hand-off | 10 minutes / 5 minutes | per 14.5 and the UX document |
| Fresh sign-in for delete | within 5 minutes | as in rule 27 |
| Friend requests | 10 per minute per account, 50 pending, 100 friends, 500 blocks | the review's proposed values |
| Display-name change | 5 per hour per account | |
| New friend code | 3 per rolling 24 hours | confirmed above |
| Export | 1 per hour per account | |
| Unauthenticated endpoints | 60 per minute per source address, counted in memory only and never written to the database | keeps the "no IP history" rule |
| Application log retention | 30 days, matching the backup rotation; logs carry no tokens, signed URLs or identity payloads | the retention number is an owner value, taken here by the owner's instruction |
| Subject-hash pepper | versioned key id; one encrypted offline copy kept apart from the database backups; rotation needs a re-hash plan (open) | backup ownership stays with the owner |

### 14.1 C1: account id is not disclosed

- Access-token claims become `{iss, aud: "broker", sub: <session id>, dkh, iat, exp}`. `sub` is the opaque random session id (today's `sid`), so the separate `sid` claim is dropped. The broker reads the account from the session row (as it already does, broker.mjs:371-377). The account id never leaves the broker: not in tokens, responses, exports or logs.
- **Adjusted, defect found in "client keys by friend code":** the friend code is rotatable (rule 11, 14.8). Local blobs and the `DeletePending` / `WipePending` recovery markers (client lib.rs:71-75) keyed by friend code would be orphaned or mis-matched after a rotation, and a crash between rotation and re-keying would leave a marker that names no current account. Recommended instead: the host creates a random **local account reference** at sign-in, stores it only on that PC under DPAPI, and keys every blob and marker by it. The friend code stays a displayed value. Owner or R2 may still choose friend-code keying if a re-key-on-rotation step is specified.
- Consequences: client contract change in `crates/kohbor-account` (C7) and the S2 tests that decode `sub` (broker.test.mjs:167, 197) must read the database instead.

### 14.2 C2: minimal deletion ledger (amends rule 27)

**This amends rule 27 ("No tombstone is kept", ACCOUNT-IDENTITY-REVIEW.md:83) and therefore needs the owner's confirmation.** It is a time-bounded tombstone.

- Content per deletion: the keyed login-identity hash (`subject_hash`, already `HMAC(pepper, iss+sub)`) and the deletion time. Nothing else: no account id, friend code, display name or device data.
- **Adjusted, defect found in "deletion date" only:** a user may delete and then sign in again before 30 days pass, which legitimately creates a new account with the same login hash. Replaying the ledger by hash alone would delete that new account. The ledger therefore stores the deletion **timestamp**, and replay deletes an account only when its login hash matches **and** `accounts.created_at` is earlier than the recorded deletion time.
- **Adjusted, location:** the ledger must live **outside** the database being restored (an encrypted file beside the backups, same custody), because a restore overwrites anything inside it. Where exactly and who holds it is owner custody (with the backups).
- Purge: an entry is removed 30 days after the deletion, matching the recorded backup rotation (DECISION-LOG.md:125). It must not be purged before the last backup taken before that deletion has been rotated out; if rotation ever runs late, the purge waits for it. No new retention value is introduced.
- Replay: after any restore, before any broker process reads the database, delete every account matching a ledger entry under the rule above (cascade per 8.1). The restore rehearsal (11 row 30) proves it.
- The privacy notice must mention the ledger and the 30-day backup tail: DEC-PRIVACY-LEGAL.

### 14.3 C4: nonces join the allow-list

Recommended rule 26 amendment (document control; owner confirms): "single-use sign-in, refresh and delete challenges (nonce hash, device-key thumbprint, purpose, times), kept until consumed or expired and then purged". Lifetime per 14.5; purge as in 5.1. Unused `attempts` column dropped (section 3).

### 14.4 Refresh reuse revokes from any device key

As in 5.1 step 4: a rotated token presented with any device key deletes the device row, the session and its tokens. No defect found. Note the concurrency consequence in 5.2 (a client retrying with the same token signs itself out).

### 14.5 C6: nonce outlives the hand-off

Sign-in and delete nonces: 600 s; client hand-off timeout: 300 s (as found). The 300 s ID-token freshness check stays. The values are session values the owner confirms; the agent recommendation is only that the nonce must outlive the hand-off, and 600 against 300 meets it.

### 14.6 Display name

Counted in extended grapheme clusters after NFC, 1 to 32; trimmed; control, bidi-override and zero-width characters refused; **not unique**; **renaming allowed** at any time through `PUT /v1/account/display-name`, charged to `profile_write` (value open). The SQL backstop in section 3 stays a code-point and byte limit. No defect found.

### 14.7 Device rows in the sessions list

Each row shows only "this PC" or "other device" and the sign-in date (UTC day). No last-active time, no IP address, no place, no device name. This matches the UX draft (ACCOUNT-REGISTRATION-UX.md:126). Response shape as in 7.3 item 6 with `current` rendered as "this PC".

### 14.8 Friend-code rotation

- Limit: 3 new codes per account per day (`friend_code_rotate`, charged before work, also when refused). "Day" = a rolling 24-hour window, which fits the existing counter mechanism; a calendar day would need a time-zone choice.
- Pending requests survive rotation (they are keyed by account id, as found), so do friendships, blocks and Share entries. No defect found: a recipient who rotated to escape someone can still decline or block a pending request from that person.
- No history of old codes is kept (4.3).

### 14.9 Still open after this section

Legal (DEC-PRIVACY-LEGAL), hosting and origin (DEC-ACCOUNT-ORIGIN), every retention value other than the recorded 30 days, pepper backup and rotation custody, log retention and monitoring, access and refresh lifetimes and any absolute session lifetime, every rate-limit value except rotation, the per-source limiter, export additions, broker language (C5), the decisions.json repair (C3).
