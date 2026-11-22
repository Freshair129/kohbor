# Kohbor S2 local account broker

This is a local synthetic S2 broker slice. It uses Node.js 24, PostgreSQL, Node's built-in `http` and `crypto` modules, and the pinned `pg` driver. It does not call Google, load a JWKS, or contact a hosted service. Startup requires `BROKER_MODE=local-synthetic`, an injected RSA identity verification key, an Ed25519 broker signing key, a subject pepper, and a disposable local database.

The HTTP surface is `/health`; nonce, sign-in and refresh under `/v1/auth`; profile read and display-name update under `/v1/account`; friend-code rotation at `/v1/account/friend-code`; session list and sign-out under `/v1/sessions`; export at `/v1/account/export`; and deletion at `/v1/account`. The service binds to loopback. Startup rejects a non-loopback listener and database URLs outside the local S2 database host/path allowlist. Do not publish or tunnel this service.

The broker persists only the account identity hash, account/profile data, public device keys, sessions, separately indexed refresh-token hashes, short-lived nonce hashes, account relationships, share settings, and account rate counters. Unauthenticated source-address limits are held in process memory only. The deletion ledger is an encrypted sidecar file at `BROKER_DELETION_LEDGER_PATH`, outside the database restore target; its encryption key is domain-separated from the configured subject pepper. Startup replays the ledger before migration or serving requests and removes only matching accounts created before each deletion time. An empty ledger file is created only for a fresh database; an initialized database with no ledger stops startup with a non-sensitive error. Keep the ledger with the backup set and retain it until backups that predate a deletion have rotated out.

The broker never stores provider subjects or tokens, emails, IP history, match data, peer samples, or private device keys. Reuse of any rotated refresh token deletes its device key, session and token history. Sign-out deletes the device key and cascades its session and refresh tokens; account deletion cascades through its rows and both directions of account relationships.

Expired nonces, sessions, pending requests and old rate-limit rows are purged only after bearer authentication succeeds, at most once per minute. Anonymous requests do not run cleanup deletes.

## Local run

Use a dedicated disposable PostgreSQL database named `kohbor_s2`, owned by a local `kohbor_s2` role. For a local PostgreSQL administrator connection, create it with SQL equivalent to:

```sql
CREATE ROLE kohbor_s2 LOGIN PASSWORD 'replace-with-a-random-local-only-password';
CREATE DATABASE kohbor_s2 OWNER kohbor_s2;
```

Keep any database password synthetic and local. Do not use a Supabase URL, another product's database, production credentials, or real Google keys.

From this folder, install the reproducible locked dependency set when registry access is available:

```powershell
npm ci --omit=dev --ignore-scripts --no-audit --no-fund
```

Generate one fixed synthetic OIDC fixture key and one broker signing key outside the checkout. The generator refuses to write inside this service and refuses to overwrite an existing file:

```powershell
node scripts/create-local-fixture.mjs "$env:TEMP\kohbor-s2.local.env"
```

The generated file is local test configuration containing synthetic private key material. Its `BROKER_DELETION_LEDGER_PATH` points at an encrypted sidecar beside the environment file. Keep both outside the checkout and remove them when the local run is finished. If the dedicated Postgres role uses a password, edit only `DATABASE_URL` in that file to include its synthetic local password; the server still rejects non-local hosts, query parameters, and databases other than `kohbor_s2`.

Start the broker with Node's built-in environment-file support:

```powershell
node --env-file="$env:TEMP\kohbor-s2.local.env" src/server.mjs
```

The server applies the idempotent migration before listening. It fails closed if any required fixture value is missing, the identity key is not RSA-2048 or stronger, the broker key is not Ed25519, the mode is not `local-synthetic`, the database target is outside the local allowlist, or the listener is not loopback. The default health check is `GET http://127.0.0.1:8080/health`; it returns success only while PostgreSQL responds.

`Dockerfile` packages the local-only Node entrypoint. Build the image from this directory.

```powershell
docker build --platform linux/amd64 -t kohbor-s2-local .
```

For Docker acceptance, run the image with disposable PostgreSQL in the same isolated network namespace and connect over loopback. The test harness mounts only read-only test fixtures and verification inputs. It uses the broker code and dependencies installed in the image.

## Hosted database check

`node scripts/check-supabase.mjs` performs a read-only schema check of the Supabase Postgres target. It verifies the expected migrations and public tables, RLS, relation/function/sequence grants, and the absence of relations in `graphql_public`. It permits only the built-in `graphql_public.graphql(text, text, jsonb, jsonb)` function to be executable by `anon` or `authenticated`, and that function must not be `SECURITY DEFINER`. It prints account and login-identity row counts only when visibility is verified; otherwise it reports them as unavailable and fails. Table ownership is used for row-count visibility only when FORCE RLS is off; a role with `BYPASSRLS` can still read counts. The check requires TLS certificate verification and applies a five-second statement timeout. The broker still refuses hosted databases and does not serve sign-ins against Supabase. Supply `SUPABASE_DATABASE_URL` (and optional `SUPABASE_CA_FILE`) through a protected environment or secret store outside this repository; the URL is a secret and must not be committed or logged.

Supabase's pooler certificate chains to Supabase Root 2021 CA, which is not in Node's public trust store, so `SUPABASE_CA_FILE` must name the CA downloaded from the project's Database Settings (SSL Configuration). The checker uses the fixed `EXPOSED_SCHEMAS` list (`public`, `graphql_public`) because the Dashboard exposed-schema setting cannot be read from the database. If a schema is added under Dashboard → API → Exposed schemas, add it to `EXPOSED_SCHEMAS` too. Apart from the broker tables, `public` may also contain the Supabase-only table listed in `SUPABASE_ONLY_PUBLIC_TABLES` (`alpha_invites`, the alpha download gate; owner decision 2026-11-22). If that table is present, it must also have RLS on and no anon/authenticated grants. Any other extra public table fails the check.

## Protocol and phase boundary

Nonce challenges bind to a device public-key thumbprint and a single purpose. Sign-in and deletion nonces expire after 10 minutes; refresh nonces after 5 minutes. OIDC identity tokens are verified only against the injected fixed RSA key, issuer, and audience. The broker issues 15-minute Ed25519 access tokens with the single audience `broker`; `sub` is the opaque session UUID, and account UUIDs stay inside the broker. Refresh tokens are opaque, rotated and stored only as SHA-256 hashes, with a 30-day idle expiry and 90-day absolute cap. Delete requires a fresh identity token (at most 5 minutes old) whose subject maps to the authenticated account and whose device proof verifies against the public key enrolled in that live session.

Display names are normalized to NFC and limited to 32 grapheme clusters. The account-specific limits with confirmed values are five profile writes per hour, three friend-code rotations per rolling 24 hours and one export per hour; 429 responses include `Retry-After` in whole seconds. Unauthenticated routes allow 60 requests per minute per source address in process memory. Session-administration, deletion and account-creation quotas have no value in the design input and are not enforced until decided.

The account tables include the reviewed future relationship shapes so S2 export and account deletion can be qualified against PostgreSQL. This slice has no social mutation API and no relay-ticket endpoint. It issues no relay tickets or scopes, and there is no fake grant path.

The planned downstream D8 interface remains deferred until pairing authorization and the S3 sharing contract exist. At that phase, a ticket would use the `relay_ticket_domain` issuer label, the single audience `relay`, an Ed25519 signature verified by the relay using public verification material only, and a bounded lifetime no longer than the 10-minute D8 proposal. Claims are limited to mutually authorized pairing IDs and per-direction granted scopes, the device-key thumbprint, and a random subject. They must exclude account IDs, friend codes, display names, identity hashes or tokens, and raw match or team identifiers. No ticket may be issued until the broker has a real, mutually authorized pairing and scope grant to validate.

The independent `test/broker.test.mjs` file is the local wire and persistence contract. It must run only against a disposable synthetic PostgreSQL database. The Docker acceptance run uses the built image and a disposable PostgreSQL database in one loopback-only namespace. It leaves host ports unpublished and keeps the checkout outside the containers.
