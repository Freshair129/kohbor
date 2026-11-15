# Kohbor S2 local account broker

This is a local synthetic S2 broker slice. It uses Node.js 24, PostgreSQL, Node's built-in `http` and `crypto` modules, and the pinned `pg` driver. It does not call Google, load a JWKS, or contact a hosted service. Startup requires `BROKER_MODE=local-synthetic`, an injected RSA identity verification key, an Ed25519 broker signing key, a subject pepper, and a disposable local database.

The HTTP surface is `/health`, `POST /v1/auth/nonces`, `POST /v1/auth/sessions`, `POST /v1/auth/refresh`, `GET /v1/account/export`, and `DELETE /v1/account`. The service binds to loopback. Startup rejects a non-loopback listener and database URLs outside the local S2 database host/path allowlist. Do not publish or tunnel this service.

The broker persists only the account identity hash, account/profile data, public device keys, sessions with refresh-token hashes, short-lived nonce hashes, account relationships, share settings, and rate counters. It never stores provider subjects or tokens, emails, IP history, match data, peer samples, or private device keys. Refresh replay revokes the live session; deleted accounts cascade through their rows and both directions of account relationships.

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

The generated file is local test configuration containing synthetic private key material. Keep it outside the checkout and remove it when the local run is finished. If the dedicated Postgres role uses a password, edit only `DATABASE_URL` in that file to include its synthetic local password; the server still rejects non-local hosts, query parameters, and databases other than `kohbor_s2`.

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

## Protocol and phase boundary

Nonce challenges bind to a device public-key thumbprint and a single purpose. OIDC identity tokens are verified only against the injected fixed RSA key, issuer, and audience. The broker issues Ed25519 access tokens with the single audience `broker`; refresh tokens are opaque, rotated, and stored only as SHA-256 hashes. Delete requires a fresh identity token whose subject maps to the authenticated account and whose device proof verifies against the public key enrolled in that live session.

The account tables include the reviewed future relationship shapes so S2 export and account deletion can be qualified against PostgreSQL. This slice has no social mutation API and no relay-ticket endpoint. It issues no relay tickets or scopes, and there is no fake grant path.

The planned downstream D8 interface remains deferred until pairing authorization and the S3 sharing contract exist. At that phase, a ticket would use the `relay_ticket_domain` issuer label, the single audience `relay`, an Ed25519 signature verified by the relay using public verification material only, and a bounded lifetime no longer than the 10-minute D8 proposal. Claims are limited to mutually authorized pairing IDs and per-direction granted scopes, the device-key thumbprint, and a random subject. They must exclude account IDs, friend codes, display names, identity hashes or tokens, and raw match or team identifiers. No ticket may be issued until the broker has a real, mutually authorized pairing and scope grant to validate.

The independent `test/broker.test.mjs` file is the local wire and persistence contract. It must run only against a disposable synthetic PostgreSQL database. The Docker acceptance run uses the built image and a disposable PostgreSQL database in one loopback-only namespace. It leaves host ports unpublished and keeps the checkout outside the containers.
