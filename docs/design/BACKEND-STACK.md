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

**Authorship.** The base text (S0 working draft) was written by the `codex-backend` session (gpt-6-luna) and applied here from its prepared patch by the owner's instruction ("ใช้แพตช์ Codex"). The sections marked *added by governance-backend-stack* and the small edits listed in the change note at the end were added by a Claude governance session. Agent work, not a human, legal or security review. Provider facts are UNVERIFIED unless stated (see "Provider facts" below).

S0 working draft. The owner selected the alpha/beta architecture and authorized the local fake first. Provider provisioning, domain registration, public API origins and hosting activation require separate owner decisions. See [owner statements](../governance/DECISION-LOG.md#backend-workstream-owner-statements).

## Alpha sales and spending

Alpha sales are disabled. Use Free tier options with a spending ceiling of THB 0. If a charge is expected, notify the product owner immediately and pause the action before it can incur a charge. Resume that action only after separate spending authorization. This rule is separate from the broker and relay uptime alerts. The remaining DEC-STORE-HOSTING decisions are open.

The first Store release after alpha sells announcer packs only (DEC-STORE-SCOPE). The owner will decide Premium separately under DEC-PREMIUM. Buyers must sign in to Kohbor before purchase. The Store binds the purchase entitlement to that account so the buyer can sign in on another device and download the pack again.

## Environments and routes

| Component | Local development | Alpha/beta when provisioned |
| --- | --- | --- |
| Website | Static output, built host-agnostic so the same output also builds for Vercel (DEC-FRONTEND-HOSTING) | Cloudflare Pages in the product owner's account, with its assigned pages.dev address for landing, download links and privacy notice. The preferred custom domain is conditional on availability and owner registration. |
| Account broker | In-memory fake with synthetic identities | Docker on the owner's PC, reachable only through the Cloudflare Tunnel (no port forwarding). The product owner operates the named Cloudflare Tunnel when the API opens. The route gets a stable public hostname once a domain is under the owner's control. |
| Relay | Fake peer transport | Separate Docker service on the same PC, published through the approved Tunnel after the hosted sharing gates. |
| Administration | Local only | Owner-only Tailscale access. Keep admin paths out of public Tunnel routes. |
| Database | Local Postgres first for S2 synthetic acceptance | A new Kohbor-only Supabase Free project for alpha, used solely for Postgres. Broker connects over TLS. No Supabase Auth or reuse of another product's project. |
| Pack delivery | Synthetic catalog, entitlement and storage | Cloudflare R2 is PROPOSED for pack storage (owner confirmed the stack as proposed; R2 versus Supabase Storage is not final, DEC-STORE-HOSTING), with Supabase Storage as the named alternative. The Store catalog can be browsed without signing in. Buying or downloading purchased packs requires Kohbor sign-in. |
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

Provider references (listed by the `codex-backend` session as checked 2026-11-14; NOT re-verified by the session that applied this patch, so every provider claim in this document is UNVERIFIED until the owner or a reviewer reads the vendor page): [Pages domains](https://developers.cloudflare.com/pages/configuration/custom-domains/), [Pages limits](https://developers.cloudflare.com/pages/platform/limits/), [Tunnel prerequisites](https://developers.cloudflare.com/tunnel/get-started/), [Supabase backups](https://supabase.com/kb/faqs/platform/free-plan-backups), [Supabase pausing](https://supabase.com/docs/guides/platform/free-project-pausing), [R2 pricing](https://developers.cloudflare.com/r2/pricing/), [R2 presigned URLs](https://developers.cloudflare.com/r2/api/s3/presigned-urls/).

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
| C | Remaining Store scope details (free pack availability and download access, and included updates), payment operator, currency/tax/refunds, creator share, premium, offline and revocation policy, and DEC-PACK-CRYPTO. |
| D | Updater signing custody, installer versus portable, release visibility and Windows Authenticode. |
| E | DEC-PRIVACY-LEGAL, DEC-RETENTION including backup retention and account deletion, policy text and legal reviewer. |

Related baseline: [account identity](ACCOUNT-IDENTITY-REVIEW.md), [backend architecture D8 and ADR-A](REMAINING-SYSTEM-DESIGN.md), [media package transactions](MEDIA-PACK-SPEC.md), and [Draft Bar data boundaries](DRAFT-BAR-SPEC.md). [Store and Pack Delivery](STORE-PACK-DELIVERY.md) is on main through PR #20 and remains a proposal pending owner decisions.

## Alpha versus production *(added by governance-backend-stack)*

| Aspect | Alpha/beta (decided, DEC-ACCOUNT-ORIGIN) | Production (open) |
| --- | --- | --- |
| Where the broker and relay run | Docker on the owner's PC, reachable only through the Cloudflare Tunnel; no port forwarding; Tailscale for owner administration only | Owner decision: hosted operator, region and recovery objective |
| Database | New Kohbor-only Supabase project used as Postgres only; no Supabase Auth; never shared with another product; tested against local Postgres first | Owner decision |
| Static front end | Cloudflare Pages, built host-agnostic (also builds for Vercel) | Same build; host may change without code changes |
| Pack storage | Cloudflare R2 PROPOSED, Supabase Storage the alternative; nothing exists yet | Owner decision after provider facts are checked |
| App updates | GitHub Releases (DEC-UPDATE-ENDPOINT); separate signing key and trust root | Same, subject to release visibility and Windows code signing |
| Money | Alpha sells nothing; free tiers only; spending ceiling THB 0; any expected charge pauses the action and goes to the owner | Separate owner decisions (payments are not designed) |
| Uptime | Best effort. Sleep, reboot, power or ISP loss take online features offline; the desktop local core keeps working | A stated target set by the owner |
| Backups | Encrypted nightly `pg_dump`; a free Supabase project has no guaranteed point-in-time recovery (UNVERIFIED); restore rehearsed before real accounts | Provider backups plus dumps; objective set by the owner |
| Signing keys | Online keys on the home PC are acceptable for an alpha with no sales and are a written limit | Key service or hardware-backed signing, owner decision |

## Home-PC risks *(added by governance-backend-stack)*

| Risk | What it means | Mitigation in this design |
| --- | --- | --- |
| Uptime | The PC sleeps, updates, loses power or loses its ISP; sign-in, hosted sharing and downloads stop | The desktop local core never depends on the broker (CORE-001); 5-minute health checks with an alert after two failures; no uptime promise in the alpha |
| Changing address | A home connection may change its public address | A tunnel that dials out needs no public address or inbound port (UNVERIFIED); no port forwarding is allowed |
| Isolation from the development machine | If broker and development tools share a session, a compromise of one reaches the other, including signing keys and the repository | Run the broker in its own container, with its own operating-system user if possible; **containers must not mount the project checkout or any app-data folder**; no secrets or tool caches shared with the build machine |
| Exposure | The service is reachable from the internet through the tunnel | Admin routes are not on the public tunnel; admin only over Tailscale; rate limits per ACCOUNT-IDENTITY-REVIEW; no logs of tokens, signed URLs or identity payloads |
| Single copy of data | One disk, one PC | Encrypted nightly dumps to a second disk and a copy off the PC; restore rehearsal |
| Shared network | Other devices on the home network | Tunnel and containers run with the minimum published ports (none inbound); the owner confirms router settings |

## Secrets handling *(added by governance-backend-stack)*

Secrets never enter the repository, chat, prompts, logs, URLs or screenshots, and agents do not create or hold them. Database credentials, the subject-hash pepper, token-signing private keys, the Tunnel token and any scoped R2 keys live in owner-managed files **outside** the checkout, with file permissions limited to the service account, referenced from compose files only as placeholders. Public verification keys may be committed. Publisher, updater and store-authority keys are separate and the broker never holds the publisher or updater private key. A secret pasted into chat is treated as leaked and rotated. If an agent needs a value, it asks the owner to place it in the local secret file.

## Who does what *(added by governance-backend-stack)*

The things only the owner can create. "Codex backend session" is the `codex-backend` Codex session; "lead" is the Claude governance and engineering lead. Nobody but the owner creates an account, key, certificate, domain or payment.

| Item | Owner | Codex backend session | Lead |
| --- | --- | --- | --- |
| Domain (kohbor.ai only if available; assigned Pages hostname otherwise) | Checks availability and renewal terms, registers it, adds it to Cloudflare | Documents the DNS and route names needed | Records the decision, never registers |
| Cloudflare account, Pages project and Tunnel | Creates the account, the Pages project and the named Tunnel; keeps the Tunnel token | Writes the compose and route configuration with placeholders; no token | Reviews that no port is published and nothing is mounted from the checkout |
| Supabase project (new, Kohbor-only, Postgres only) | Creates it after the local Postgres checks pass; keeps the connection string | Migrations and tests against local Postgres first | Reviews the allow-list and deletion behaviour before hosted use |
| Google Cloud project and OAuth consent screen | Creates and owns the project and consent screen; enters credentials locally; limits alpha sign-in to invited testers (at most 100) | Client and broker code against a fake identity provider | Records the decision; never holds a client secret |
| Update signing key | Creates it on the owner's machine; gives only the public key | Updater scaffold and verification code | Records the trust-root decision and reviews the verification tests |
| Windows code-signing certificate | Decides whether to buy one, buys it and holds it (a paid item: a new decision under the THB 0 rule) | None | Notes the dependency on release visibility; does nothing without the owner |
| R2 bucket or Supabase Storage bucket and scoped keys | Decides which, creates it after the free-tier fit is checked, keeps the keys | Signed-URL code against a fake store | Reviews custody and the spending rule |
| Rights for packs to be distributed | The owner's rights team | None | Never decides rights |
| Monitoring and alert channel | Chooses the provider and the channel | Health endpoints and probe configuration | Records the decision |

## Provider facts *(added by governance-backend-stack)*

All UNVERIFIED by the author of this section. Read the vendor page and record its date before relying on a row.

| Claim made in this document | Needed check |
| --- | --- |
| Cloudflare Pages has a per-asset size limit of 25 MiB | Current Pages limits page |
| A Cloudflare Tunnel needs no inbound port and can run on a free plan | Tunnel documentation and terms |
| A free Supabase project can pause and has no automatic daily backups or guaranteed point-in-time recovery | Current plan and backup pages |
| R2 has usage-based charges above included allowances; its signed GET URLs can be reused until they expire | R2 pricing and presigned URL pages |
| The same static output builds on Cloudflare Pages and Vercel | A real build on both, once there is a site |
| GitHub Releases can carry the installer, its signature and `latest.json` for the updater | Release asset limits and the updater documentation |

## Change note *(added by governance-backend-stack)*

Edits to the `codex-backend` base text: an authorship paragraph; the Website row now states the host-agnostic build; the broker row now states Tunnel only and no port forwarding; the pack-delivery row says R2 is proposed instead of selected (the owner confirmed the stack as proposed, not R2 as final); the provider references are labelled UNVERIFIED. The sections "Alpha versus production", "Home-PC risks", "Secrets handling", "Who does what", "Provider facts" and this note were added.
