# Alpha launch: release, front end and back end (as built 2026-11-22)

Status: describes what is deployed or being built on 2026-11-22 for the closed alpha. It records owner decisions made in chat that day and the agent's implementation. It is not legal review, not a security review by a human, and not a frozen design. Where it departs from [ACCOUNT-IDENTITY-REVIEW](ACCOUNT-IDENTITY-REVIEW.md), [ACCOUNT-DATABASE-DESIGN](ACCOUNT-DATABASE-DESIGN.md) or [BACKEND-STACK](BACKEND-STACK.md), this document describes the alpha as it runs, and the conflict is listed in section 6 for document control.

## 1. Owner decisions (2026-11-22, chat)

| Decision | Owner words (verbatim) | Consequence |
| --- | --- | --- |
| Use Supabase Auth with Google for alpha accounts | "Supabase Auth" (after being told it stores email and departs from the account design) | Supabase Auth holds the Google email, name and picture. The broker and keyed-hash design are not used for the alpha. |
| Sign-up and sign-in only in the app; the website is a landing page plus a gated download | "ทำระบบล็อกอินและก็ลงทะเบียนผ่านแอปไปเลยก็ได้จุดเดียวไม่ต้องมีฝั่งเว็บไซต์มีแค่เป็น landing page แล้วก็กลุ่มให้ดาวน์โหลด" | The website registration page that was live from about 12:00 to 13:00 was removed. |
| Download needs an invite code | "คนที่ดาวน์โหลดได้จะต้องมี invite code เท่านั้น" | Invite-code gate (section 3). |
| Installers in a private GitHub release repo | Answer "Private release repo" (the main repo is public) | `Freshair129/kohbor-releases` (private). |
| One code per person; the download gate is the only gate | Answers "One code per person", "Download gate only" | Each code allows 3 downloads; anyone with the app can sign in with Google. |
| Google OAuth client type for the app | "Desktop app"; consent screen "External", later published "In production" | See section 4. |

## 2. Release

1. The app session builds the Windows installer. This document does not cover the build.
2. The owner creates a GitHub Release in `Freshair129/kohbor-releases`, not marked pre-release, with one `.exe` or `.msi` asset. Signing and publishing are owner actions.
3. The download gate (section 3) always serves the latest non-pre-release release. Drafts and pre-releases are never served.
4. Not verified on 2026-11-22: an end-to-end download, because no release existed yet.

## 3. Front end: website (landing page and gated download)

- Hosting: Vercel project `kohbor-site-prototype`, production URL `https://kohbor-site-prototype.vercel.app`, source `apps/website/`. Pages: `index.html` only. `/register.html` returns 404 since the 13:00 deployment.
- The "Download for Windows" button opens a dialog (`js/download.js`, `css/download.css`) that asks for an invite code and POSTs it to the Supabase Edge Function `alpha-download`. On success the browser follows a short-lived GitHub asset link. No cookies, no analytics.
- Invite codes look like `KB-XXXX-XXXX-XXXX` (Crockford base 32, 60 random bits). Only their SHA-256 hashes are stored.
- Limit: the GitHub asset link is valid for a short time and is not bound to the person. Anyone who receives that link while it is valid can download.

## 4. Front end: desktop app (sign-in)

- Being built on branch `account/ui-registration` (claim `account__acc-ui-1`); not in the first alpha build, not reviewed, not committed on 2026-11-22.
- Flow: the app opens the system browser at Supabase `/auth/v1/authorize?provider=google` with PKCE S256 and `redirect_to=http://127.0.0.1:<ephemeral port>/callback`; a loopback listener accepts one callback with a code; the app exchanges it at `/auth/v1/token?grant_type=pkce` over WinHTTP; the access token stays in memory; the refresh token is stored DPAPI-protected in the app data directory. No browser hand-off during a live match. Guests keep every local feature.
- The app shows only the state, the display name and the picture. It contains only the Supabase publishable key, never a service-role key.

## 5. Back end: Supabase project `kohbor` (ref `wmmvawvfnwdfzxvumlfc`, region ap-northeast-1)

| Part | What it is | Where it is defined |
| --- | --- | --- |
| Auth | Google provider (Web application OAuth client in the Kohbor Google Cloud project), Email provider disabled. Redirect allow-list: the production site, the project's own Vercel preview pattern, and `http://127.0.0.1:*/callback` for the app. | Supabase dashboard (owner) |
| `public.alpha_invites` | Invite-code hashes, use count (max 3), revoked flag, optional expiry, last use. RLS on, no grants to `anon` or `authenticated`. | Migration `alpha_invites`, applied through the Supabase MCP; source in `supabase/sql/20261122050920_alpha_invites.sql` |
| `redeem_alpha_invite`, `refund_alpha_invite` | Atomic use and refund of a code; executable only by `service_role`. | Migration `alpha_invite_functions`; source in `supabase/sql/20261122050945_alpha_invite_functions.sql` |
| Edge Function `alpha-download` | Checks the code, redeems it, asks the GitHub API for the latest release asset using secret `GITHUB_RELEASES_TOKEN` (fine-grained, Contents read-only, `kohbor-releases` only) and `GITHUB_RELEASES_REPO`, and returns the short-lived link; refunds the use if no asset is available. CORS only for the site and its previews. Never logs the code, token or link. | Deployed through the Supabase MCP; source in `supabase/functions/alpha-download/index.ts` |
| Broker tables (migrations 1-3) and the RLS hardening step | Applied but unused by the alpha. The broker still refuses hosted databases. | `services/kohbor-broker`; the hardening step is `supabase/sql/20261122040048_supabase_close_data_api.sql` |

Secrets live only in Supabase (Edge Function secrets, Google client secret) and GitHub. None is in the repository or in chat.

## 6. Conflicts to resolve through document control

1. **Recorded by DEC-ALPHA-AUTH-DEVIATION (2026-11-22):** the alpha uses Supabase Auth, which stores the email, name and picture and issues its sessions and tokens. This is a time-limited deviation from ACCOUNT-002/005/007 and does not close DEC-PRIVACY-LEGAL or constitute legal or security review.
2. BACKEND-STACK ("Supabase as Postgres only", broker on the owner's PC) does not describe the alpha: Supabase Auth and an Edge Function are used; no broker runs.
3. DEC-PRIVACY-LEGAL is open, but personal data of testers is now collected. The privacy wording in the app is marked pending legal review.
4. **Recorded by DEC-ALPHA-AUTH-DEVIATION:** ACCOUNT-001..010 are now in `contracts/requirements.json`; the alpha deviation from ACCOUNT-002/005/007 is in `contracts/decisions.json`.
5. The Supabase-only migrations and the Edge Function source are now in `supabase/` (copied from live state on 2026-11-22; see its README). The invite-code generator is `supabase/tools/make-invite-codes.mjs` (offline; plain codes to a private file, hashes-only SQL).
