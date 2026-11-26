# Supabase-only source (alpha)

This folder holds what runs in the Kohbor Supabase project (ref `wmmvawvfnwdfzxvumlfc`) that is not part of the broker. It is a copy of live state, taken on 2026-11-22. Each SQL file matches the statements recorded in `supabase_migrations.schema_migrations` for its version, checked by SHA-256. The function source was checked against the deployed `alpha-download` function the same way.

| Path | Live name | What it does |
|---|---|---|
| `sql/20261122040048_supabase_close_data_api.sql` | migration `supabase_close_data_api` | RLS on all 14 broker tables, with no policies. Revokes every anon/authenticated table grant, including default privileges. |
| `sql/20261122050920_alpha_invites.sql` | migration `alpha_invites` | The invite-code table. It stores only SHA-256 hashes of codes. RLS on, no client grants. |
| `sql/20261122050945_alpha_invite_functions.sql` | migration `alpha_invite_functions` | `redeem_alpha_invite` and `refund_alpha_invite`. Only `service_role` can execute them. |
| `sql/20261126102344_alpha_waitlist.sql` | migration `alpha_waitlist` | `alpha_waitlist` (one row per Google user who joined the test queue) and `alpha_download_links` (SHA-256 hashes of shared download-link keys, revocable, download count). Functions `join_alpha_waitlist` and `use_alpha_download_link`, `service_role` only. RLS on, no client grants. |
| `sql/20261126133502_alpha_admin.sql` | migration `alpha_admin` | `alpha_admins` (the user ids allowed to manage the queue; filled in Supabase, not in the repo) and `alpha_admin_check`, `alpha_admin_queue`, `alpha_admin_set_invited`, `alpha_admin_links`. Each function raises `forbidden` for a caller not in `alpha_admins`. `service_role` only. RLS on, no client grants. |
| `sql/20261126140813_alpha_invite_mail.sql` | migration `alpha_invite_mail` | Adds `user_id` to `alpha_download_links` (personal links), the one-row `alpha_invite_template`, and `alpha_admin_get_template`, `alpha_admin_set_template`, `alpha_admin_recipients`, `alpha_admin_issue_link` (a new personal link; revokes that person's older ones) and `alpha_admin_revoke_link`. Admin-checked, `service_role` only. RLS on, no client grants. |
| `functions/alpha-admin/index.ts`, `mail.ts` | Edge Function `alpha-admin` (deployed with `verify_jwt` on) | Back end of `apps/website/admin.html`: lists the queue and link counts, edits the email template, marks people invited, and sends invites (a personal link each) through Gmail SMTP on port 465 with the secrets `KOHBOR_GMAIL_USER` and `KOHBOR_GMAIL_APP_PASSWORD`, which the owner sets in the dashboard. `mail.ts` is a small SMTP and MIME module; `node --test supabase/functions/alpha-admin/mail.test.mjs` checks it against a fake server. |
| `functions/alpha-waitlist/index.ts` | Edge Function `alpha-waitlist` (to deploy with `verify_jwt` on) | Takes the signed-in user from the verified token and puts them in the queue; returns their place. |
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
