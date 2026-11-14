---
title: "Kohbor Backend Stack"
doc_id: "BACKEND-STACK"
version: "0.1.0"
status: "draft"
updated: "2026-11-14"
owner_role: "Product and engineering owners"
approval_status: "not_approved"
---

# Kohbor backend stack

S0 working draft. The owner selected the alpha/beta architecture and authorized the local fake first. Provider provisioning, domain registration, public API origins and hosting activation require separate owner decisions. See [owner statements](../governance/DECISION-LOG.md#backend-workstream-owner-statements).

## Alpha sales and spending

Alpha sales are disabled. Use Free tier options with a spending ceiling of THB 0. If a charge is expected, notify the product owner immediately and pause the action before it can incur a charge. Resume that action only after separate spending authorization. This rule is separate from the broker and relay uptime alerts. The remaining DEC-STORE-HOSTING decisions are open.

The first Store release after alpha offers both free and paid announcer packs (DEC-STORE-SCOPE). The owner will decide Premium separately under DEC-PREMIUM. Buyers must sign in to Kohbor before purchase. The Store binds the purchase entitlement to that account so the buyer can sign in on another device and download the pack again.

## Environments and routes

| Component | Local development | Alpha/beta when provisioned |
| --- | --- | --- |
| Website | Static output | Cloudflare Pages in the product owner's account, with its assigned pages.dev address for landing, download links and privacy notice. The preferred custom domain is conditional on availability and owner registration. |
| Account broker | In-memory fake with synthetic identities | Docker on the owner's PC. The product owner operates the named Cloudflare Tunnel when the API opens. The route gets a stable public hostname once a domain is under the owner's control. |
| Relay | Fake peer transport | Separate Docker service on the same PC, published through the approved Tunnel after the hosted sharing gates. |
| Administration | Local only | Owner-only Tailscale access. Keep admin paths out of public Tunnel routes. |
| Database | Local Postgres first for S2 synthetic acceptance | A new Kohbor-only Supabase Free project for alpha, used solely for Postgres. Broker connects over TLS. No Supabase Auth or reuse of another product's project. |
| Pack delivery | Synthetic catalog, entitlement and storage | R2 Standard is the selected storage route, with Supabase Storage as the documented fallback if approved. The Store catalog can be browsed without signing in. Buying or downloading a Store pack requires Kohbor sign-in. When a user downloads a free pack, the Store adds it to that account's library. |
| Updates | Local policy and synthetic checks | GitHub Releases and Tauri updater per DEC-UPDATE-ENDPOINT. Resolve anonymous access, signing and release authorization first. |

Production hosting, recovery objectives and uptime targets remain owner decisions. For home-PC alpha, sleep, reboots, loss of power and ISP outages take online capabilities offline. The desktop local core stays usable. There is no permanent public API address in the initial phase.

## Data flow and custody

1. In S1 the native host talks to a fake provider and broker under an explicit development configuration. In the hosted design, the host opens the system browser for Google OIDC with openid scope and PKCE S256. The broker validates the ID token and nonce, discards Google tokens and persists only the permitted account fields, including a keyed subject hash. Tokens remain in the native host.
2. The broker issues distinct Ed25519 credentials with one audience per credential. The relay checks tickets offline with a public key. Only the allowed pairing metadata and peer DTOs enter the relay, and its state lives in memory. Raw match or team identifiers, account tokens and identity fields stay out of the relay. Draft Bar OCR stays local and in memory. Broker and relay logs must omit credentials, query strings containing signed URLs, identity payloads and peer payloads.
3. When commerce is approved, the broker verifies entitlement and returns a short-lived signed R2 GET URL. The desktop downloads directly, then verifies digest, publisher signature and receipt before the MP-06 install transaction. Activation is separate. Store policy for installed copies, account deletion and revocation remains open.
4. The site links to release assets. The updater verifies signed releases using its own trust root, independently of the account and Store.

Keep database credentials, the OIDC subject pepper, token signing private material, the Tunnel token and any scoped R2 credentials outside the checkout in owner-managed files with restricted access. Compose files reference placeholders. The relay receives public verification material only. Publisher and updater private keys stay outside the broker and out of chat. Use separate trust domains. Containers must not mount the owner's project or app-data directories. A published-port container or Tunnel start requires owner authorization.

## Backups and monitoring

- Before admitting real accounts, schedule a nightly alpha Postgres dump with `pg_dump` and encrypt the backup. Decide backup retention alongside the account-deletion policy under DEC-RETENTION. Backup ownership and restore objectives are pending. Rehearse a restore into an isolated database and verify that backups cannot reintroduce deleted accounts into service. Supabase Free can pause and does not provide automatic daily backups.
- When the API opens, probe the broker and relay health endpoints every five minutes. Track consecutive failures separately for each service and notify the product owner after two consecutive failures for either service. Health responses carry no user identifiers. Broker readiness includes database reachability. The monitoring provider, alert channel and log retention remain open.
- Pages has a 25 MiB per-asset limit, so the site links to separately hosted binaries. R2 Standard has usage-based charges above its included allowances. Presigned R2 GET URLs use the S3 API hostname and can be reused until expiration. Confirm applicable billing limits and range behavior before relying on them. Apply the THB 0 spending rule to expected charges from R2.

Provider references checked 2026-11-14: [Pages domains](https://developers.cloudflare.com/pages/configuration/custom-domains/), [Pages limits](https://developers.cloudflare.com/pages/platform/limits/), [Tunnel prerequisites](https://developers.cloudflare.com/tunnel/get-started/), [Supabase backups](https://supabase.com/kb/faqs/platform/free-plan-backups), [Supabase pausing](https://supabase.com/docs/guides/platform/free-project-pausing), [R2 pricing](https://developers.cloudflare.com/r2/pricing/), [R2 presigned URLs](https://developers.cloudflare.com/r2/api/s3/presigned-urls/).

## Stage plan and acceptance

Use one claim and PR per stage. Run the project gate in an admitted workspace and report skipped checks separately. Source, synthetic, native and live evidence are distinct.

| Stage | Acceptance focus | Gate or dependency |
| --- | --- | --- |
| S0 | Record owner statements and prepare this stack design | Remaining owner decisions, project gate and PR handoff |
| S1 | Account client and fake broker. Test offline start, sign-out, account switch, refresh replay and deletion with synthetic data. | Check the coordination ledger for an existing owner; follow the account-client brief and run tests before host or UI integration. |
| S2 | Broker and migrations against local Postgres. Test token rejection, rate limits and deletion row counts. | Justify implementation language, establish test resource admission and keep Supabase migration gated on the owner's project. |
| S3 | Memory-only relay with offline ticket checks and expiry tests. | Closed peer contracts and consent boundaries. Hosted sharing also needs DEC-ACCOUNT-ORIGIN, DEC-PRIVACY-LEGAL and DEC-VALVE-POLICY. |
| S4 | Catalog, entitlement and resumable signed downloads, with separate MP-06 installation and activation. | Store policy, storage budget, persistence allow-list amendment, signed-authority ADR and pack rights. |
| S5 | Portable static site and updater scaffold. | Privacy copy and publication approval. Owner creates the updater key before key-dependent work. |

## Owner checklist when a stage needs provisioning

1. Use the product owner's Cloudflare account and choose a Pages project. When the public API is needed, check domain availability and renewal terms, decide who registers it, and add the domain to Cloudflare before creating named Tunnel routes. Share only assigned public hostnames or confirmation in the workstream.
2. After the local Postgres checks pass and backup and monitoring ownership are agreed, the owner creates the new Kohbor-only Supabase Free project for alpha. Set connection credentials in the local secret file outside the repository. Confirm the project plan and readiness without sharing a connection string.
3. The product owner owns the new Kohbor-only Google Cloud project and manages its OAuth consent screen. Use the `/privacy` page on Kohbor's assigned Cloudflare Pages hostname as the alpha privacy policy URL, subject to Google accepting that hostname. Verify it against the consent screen requirements before configuring OAuth. Limit alpha Google sign-in to invited testers only, with at most 100 testers. Enter credentials locally.
4. Before any R2 setup, verify that the planned usage and setup fit the THB 0 ceiling. If a charge is expected, notify the owner immediately and pause setup. The owner creates the private pack bucket and scoped credentials when needed and approved. The rights team confirms which packs may be distributed.
5. Create the updater signing key on the owner's machine after choosing release visibility and Windows packaging. Provide only the public key when integration requires it.

## Owner questions

| Group | Open decision |
| --- | --- |
| A | Deferred provisioning details: domain registrant and availability, backup ownership and restore objectives, monitoring provider and alert channel. |
| B | DEC-GOOGLE-PROJECT: Verify that Google accepts the assigned Pages hostname before configuring the consent screen. |
| C | Whether pack updates are included with purchase, payment operator, currency/tax/refunds, creator share, premium, offline and revocation policy, and DEC-PACK-CRYPTO. |
| D | Updater signing custody, installer versus portable, release visibility and Windows Authenticode. |
| E | DEC-PRIVACY-LEGAL, DEC-RETENTION including backup retention and account deletion, policy text and legal reviewer. |

Related baseline: [account identity](ACCOUNT-IDENTITY-REVIEW.md), [backend architecture D8 and ADR-A](REMAINING-SYSTEM-DESIGN.md), [media package transactions](MEDIA-PACK-SPEC.md), and [Draft Bar data boundaries](DRAFT-BAR-SPEC.md). [Store and Pack Delivery](STORE-PACK-DELIVERY.md) is on main through PR #20 and remains a proposal pending owner decisions.
