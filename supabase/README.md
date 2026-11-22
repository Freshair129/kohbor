# Supabase-only source (alpha)

This folder holds what runs in the Kohbor Supabase project (ref `wmmvawvfnwdfzxvumlfc`) that is not part of the broker. It is a copy of live state, taken on 2026-11-22. Each SQL file matches the statements recorded in `supabase_migrations.schema_migrations` for its version, checked by SHA-256. The function source was checked against the deployed `alpha-download` function the same way.

| Path | Live name | What it does |
|---|---|---|
| `sql/20261122040048_supabase_close_data_api.sql` | migration `supabase_close_data_api` | RLS on all 14 broker tables, with no policies. Revokes every anon/authenticated table grant, including default privileges. |
| `sql/20261122050920_alpha_invites.sql` | migration `alpha_invites` | The invite-code table. It stores only SHA-256 hashes of codes. RLS on, no client grants. |
| `sql/20261122050945_alpha_invite_functions.sql` | migration `alpha_invite_functions` | `redeem_alpha_invite` and `refund_alpha_invite`. Only `service_role` can execute them. |
| `functions/alpha-download/index.ts` | Edge Function `alpha-download` (deployed with `verify_jwt` off) | The download gate described in [ALPHA-LAUNCH](../docs/design/ALPHA-LAUNCH.md). |
| `tools/make-invite-codes.mjs` | none (offline tool) | Generates invite codes. See "Making invite codes" below. |

## Order and what is not here

The broker's own migrations 1-3 live in `services/kohbor-broker` and were applied first, as Supabase versions `20261122040010`, `20261122040028` and `20261122040033`. Apply them before the files above. The folder is deliberately named `sql/`, not `migrations/`, so the Supabase CLI does not mistake these three files for the full history.

Not in the repository, by design:
- **Invite codes and their hashes.** The rows were inserted as data, not as a migration. The plain codes exist only with the owner.
- **Secrets.** `GITHUB_RELEASES_TOKEN`, `GITHUB_RELEASES_REPO`, the Google client secret and the database password live only in the Supabase and GitHub dashboards. The function reads `SUPABASE_URL` and `SUPABASE_SERVICE_ROLE_KEY` from the Supabase runtime.
- **Supabase Auth configuration** (Google provider, disabled email provider, redirect URLs). It is dashboard settings only. Its gap against ACCOUNT-002/005/007 is recorded as DEC-ALPHA-AUTH-DEVIATION.

## Changing live state

A change to the live project should land here in the same change: add a new SQL file named after the version Supabase assigns, or update the function source. `services/kohbor-broker/scripts/check-supabase.mjs` checks the resulting lock-down read-only. Any new table in `public` must also be added to its allowlist, and the check must still require RLS on and no client grants.

## Making invite codes

`tools/make-invite-codes.mjs` runs offline. It never connects to a database and needs no secret.

```powershell
node supabase/tools/make-invite-codes.mjs --count 20 --out "$env:USERPROFILE\Documents\kohbor-invites-wave2.txt" --sql "$env:USERPROFILE\Documents\kohbor-invites-wave2.sql" --label "wave 2"
```

- `--out` gets the plain codes, one per line. They are credentials: give each code to one person and keep the file private.
- `--sql` gets an `INSERT` into `public.alpha_invites` that holds only the SHA-256 hashes. Without `--sql`, the `INSERT` is printed to stdout.
- Run the `INSERT` in the Supabase SQL editor. It is data, not a migration, so it does not belong in `sql/`.
- Both files must be new and outside the repository; the tool refuses otherwise.
- Optional flags:
  - `--max-downloads` (1-100, default 3) sets how many downloads one code allows;
  - `--expires` takes an ISO 8601 time with a zone;
  - `--label` takes 1-64 characters and is stored with each row.

Codes are `KB-XXXX-XXXX-XXXX`, using 12 random Crockford base32 symbols (60 bits). The tool copies the alphabet, normalization and hash from `functions/alpha-download/index.ts`. `tools/make-invite-codes.test.mjs` reads that file and fails if the two drift apart. Run it with `node --test supabase/tools/make-invite-codes.test.mjs`.
