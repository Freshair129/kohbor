---
title: "Account and Identity Model Review"
doc_id: "ACCOUNT-IDENTITY-REVIEW"
status: draft
updated: "2026-11-12"
authority: "Independent reviewer output for the product owner; PROPOSED, changes no baseline until recorded under DOCUMENT-CONTROL"
approval_status: "not_approved"
---

# Account and identity model review

Reviewer role: independent identity, security, privacy and product review. This is an agent review, not human approval and not legal advice. Sections 1, 4 and 5 are **PROPOSED by the reviewer**.

Evidence labels: **VERIFIED** = checked against a source on 2026-11-12 (URL given; pages were read through a fetch tool that summarises, so wording must be re-read by a human before it is cited externally). **FROM-REPO** = file and section (Kohbor unless marked G-Maiden; G-Maiden was read only, nothing was copied, no secret value is reproduced). **UNVERIFIED** = not confirmed; the confirming step is named. **REVIEWER** = reasoning from the labelled facts.

## 0. Decision in brief

Adopt **A+: a new Kohbor account system that copies the G-Maiden pattern, stores less than G-Maiden does, and shares nothing with it.**

- Sign-in: Google OpenID Connect in the system browser (loopback redirect, PKCE), scope `openid` only. No email is requested or stored. No password, no Kohbor-side recovery flow.
- Public identity: a new server-minted, random **Kohbor friend code**. It is not a G-Maiden GID and cannot be parsed as one.
- Steam: **not linked in the first release.** The proof method is decided now so the gated tier can use it later.
- G-Maiden's GID service is **not reused** (option B rejected). Federation (option C) is **not built**; its privacy benefit is obtained by storing only a keyed hash of the Google subject. An explicit, consented, revocable cross-product *link* is the only safe route to "one identity", and it is deferred.
- The lead leaned toward A. I agree on A versus B, and differ on five points: `openid`-only scope, hashed subject at rest, no recovery flow, asymmetric (not shared-secret) relay trust, and a client-derived pair secret.

## 1. Decision: rules for the first release (PROPOSED)

### 1.1 Sign-in

1. **One sign-in method: Google OIDC.** The native host opens the system browser, listens on `127.0.0.1` on an ephemeral port only while waiting, uses PKCE S256, `state` and a broker-issued `nonce`. No embedded webview, no custom URI scheme for the redirect. (VERIFIED: Google "OAuth 2.0 for iOS & Desktop Apps", https://developers.google.com/identity/protocols/oauth2/native-app : loopback is the recommended method for Windows desktop apps, custom URI schemes are no longer supported, "desktop apps must open the system browser", "Always use PKCE", desktop apps "cannot keep secrets confidential". VERIFIED: RFC 8252 §7.3, §8.3, §8.12, https://www.rfc-editor.org/rfc/rfc8252 .)
2. **Scope is `openid` only.** Kohbor receives the Google subject (`sub`) and nothing else: no email, name, picture or contacts. (VERIFIED: Google says "Use `sub` ... as the unique-identifier key" and "Don't use the `email` field as a unique identifier", https://developers.google.com/identity/openid-connect/openid-connect . UNVERIFIED: that an `openid`-only request returns a stable `sub` and passes Google's consent-screen rules; confirm with one test sign-in in a Kohbor-owned Google project.)
3. **A new Google Cloud project and OAuth client for Kohbor.** Never G-Maiden's client id, consent screen or project. The Kohbor broker accepts only ID tokens whose `aud` is the Kohbor client id, with `iss`, signature, `exp` and `nonce` checked. (VERIFIED: validation rules, same Google OIDC page.)
4. **Google tokens are not kept.** The app sends the ID token to the Kohbor account broker once, then discards the Google access and refresh tokens. Kohbor never calls a Google API on the user's behalf.
5. **Rejected for the first release:** email and password (Kohbor would hold password hashes and a mail-based reset, the two most attacked parts of any account system; G-Maiden's audit found a stray password and email identity undermining its Google-only rule: FROM-REPO G-Maiden EXEC-PLAN-CR-034 §1.2 P4); email magic link (needs stored email and a mail service); passkeys (needs a relying-party domain Kohbor does not own, and WebAuthn inside the Tauri/WebView2 host is UNVERIFIED; revisit later); Steam OpenID as a login (rule 16).
6. **Players without a Google account stay guests** and keep the whole local product (DEC-SIGNIN-OPTIONAL). The schema keeps login identities in their own table (provider, subject hash) so a second provider can be added by decision later without changing account ids.
7. **Conditional.** If Google will not let the Kohbor client go to production (rule 2 test fails, or brand verification cannot be met), accounts ship only as a capped test (VERIFIED: unverified and testing apps are capped at 100 users, https://support.google.com/cloud/answer/13464323 ) or not at all. The local build is unaffected. Do not fall back to G-Maiden's client. If Google is unusable for good, reopen this decision with Steam OpenID as the candidate primary.

### 1.2 Identity layers

| Layer | What it is | Who sees it | Stored where |
| --- | --- | --- | --- |
| Login identity | `HMAC(server pepper, iss + sub)` of the Google subject | Nobody; broker only | Account service, private |
| Account id | Random UUID; the key for friendships, Share list, blocks | Nobody outside the broker; never sent to the relay | Account service, private |
| Friend code (public handle) | Server-minted random code, product prefix `KB-`, about 45 random bits plus a check character | The owner; anyone the owner gives it to; accepted friends | Account service; cached locally |
| Display name | Typed by the user in Kohbor, 1 to 32 characters, escaped; never read from Google | Friends and pending-request counterparts only | Account service; cached locally |
| Device key | Per-device key pair; the public half is registered at sign-in | Friends' clients (public half only) | Private half local under DPAPI |
| Steam identity | Not present in the first release | Nobody | Nowhere server-side |

8. **The raw Google subject is never stored.** The broker stores only the keyed hash. REVIEWER: Google publishes only the `public` subject type (VERIFIED: `subject_types_supported: ["public"]`, https://accounts.google.com/.well-known/openid-configuration ), which in OpenID Connect means the same `sub` for every client; so two leaked databases holding raw `sub` could be joined person by person. The hash removes that join without any federation service.
9. **Friends see a display name and a friend code. Never** an email, Google identifier, account id, Steam ID, device list, last-seen time or match activity.
10. **Search is exact friend code only.** No directory, no name search, no contact or Steam friend import (SOCIAL-001 failure behavior, FROM-REPO SRS). Unknown code, blocked pair and "requests off" return one identical answer. Proposed limits (values for DEC-SOCIAL): 10 request attempts per minute per account, 50 pending, 100 friends, 500 blocks. Denied attempts still consume the limit (pattern: FROM-REPO G-Maiden friends design, "Constants" and "Private storage").
11. **Harassment controls:** block (silent, both directions cut, pairing ends as a PEER-008 leave); "allow friend requests" switch; **friend-code rotation** (a new code is minted, the old one stops resolving, existing friendships are untouched because they are keyed by account id).

### 1.3 Sessions, storage and offline

12. **Kohbor issues its own session.** A short access token (proposed 15 minutes) and an opaque rotating refresh token (proposed 30 days idle expiry), bound to the device key, stored server-side as a hash. Reuse of a rotated refresh token revokes the session. One active session per account for sharing purposes; a new-device sign-in pauses every Share list entry until the user confirms (PEER-016 proposed in SHARING-MODEL-REVIEW §4.2).
13. **Tokens live in the native host only**, never in the renderer, a URL or a log (PEER-007 pattern, FROM-REPO SRS). At rest: Windows DPAPI current-user blobs, the method the app already uses for GSI secrets (FROM-REPO GSI-INTAKE-SPEC §8 table; M1-STATUS "Secrets"). GSI-INTAKE-SPEC says other modules must make their own secret-storage decision and SECURITY-PRIVACY "Decisions that must be closed" lists secure local credential storage as open: this rule is that decision, PROPOSED. Limit to state honestly: DPAPI does not stop malware running as the same Windows user.
14. **Offline and outage.** The app starts and runs every local feature with no network, signed in or not (CORE-001). A signed-in user with no network sees "signed in, offline": the cached friend list is shown read-only, nothing is sent or received, no dialog appears, and no retry runs on the telemetry or critical-alert path (ALERT-001; PEER-006 retry bounds). Local cache: own friend code and display name, friend list (names and codes), Share list entries and scopes, tokens, device key, pair secrets. All of it is deleted on sign-out and account switch (CORE-003).
15. **No Kohbor recovery flow.** Losing the Google account means losing the Kohbor account, which holds only a name, a friend list and a Share list. The user signs in with another Google account and re-adds friends. Recovery of the Google account is Google's process. REVIEWER: every recovery path is a takeover path; the value at stake does not justify building one. No re-bind of an account to a different Google identity in the first release.

### 1.4 Steam

16. **No Steam link and no Steam login in the first release.** The Share list does not need it (DEC-STEAM-BINDING safe interim, FROM-REPO contracts/decisions.json), and storing a Steam identity server-side expands privacy against SECURITY-PRIVACY line 25 ("local platform identity ... Never add to social gameplay storage"). Steam as a *login* is rejected because it would make a public, profile-linked identifier the account key.
17. **When a verified tier is pursued (gate G1), ownership is proven by both of:** (a) Steam OpenID in the system browser, verified by the broker directly with Steam, with an exact match on the claimed-id format (VERIFIED: Steam is an OpenID 2.0 provider at `https://steamcommunity.com/openid/`, the claimed id contains the 64-bit SteamID, and asking for Steam credentials on your own site violates the API terms: https://partner.steamgames.com/doc/features/auth , https://steamcommunity.com/dev ); (b) a local check that the running game client's `player.steamid` equals the linked id. The local check alone is **not** proof: the intake token sits in plain text in the owned cfg and intake is a loopback POST, so it proves "a game client configured by this installation", not truth of content (FROM-REPO GSI-INTAKE-SPEC §4.1; SHARING-MODEL-REVIEW §1.1). UNVERIFIED: whether Steam OpenID accepts a loopback `return_to`; test before design freeze.
18. **What a Steam link proves and does not.** It proves control of that Steam account at link time. It does not prove who is at the keyboard now, and it does **not** prove team membership (DEC-TEAM-VERIFY stays open). The Steam ID is never a login, never a recovery credential, never searchable, never shown to another user; the server keeps a keyed hash unless the G1 source needs the raw id, which is then a separate owner decision. One Steam id per account, unlink deletes it.

### 1.5 A, B or C

19. **Option A+ is the decision.** New broker, new database, new Google client, new token signing key, new domain-separation labels, new friend-code format. Nothing is imported from G-Maiden: no account rows, GIDs, keys, HMAC secrets, endpoints or client configuration (AGENTS.md "Identity"; MEDIA-PACK-SPEC MP-09; `legacy_compatibility: false` in contracts/product-identity.json).
20. **Option B (reuse the GID service) is rejected for the first release.** Reasons in section 2.2. If the owner later wants one visible identity, the only permitted form is rule 21.
21. **Deferred: an explicit account link.** A signed-in Kohbor user and the same person signed in to G-Maiden may *choose* to link the two public handles. Conditions in section 2.3. Until that is built, the products do not know about each other, and the account decision implies nothing about rights to any content from either product (G-Maiden's rights statements are an owner claim, not something this model relies on).
22. **Option C (pairwise federation through a shared identity provider) is not built.** It needs a third service that both products trust, which becomes the highest-value target the owner operates. Rule 8 already gives the unlinkability C offers, and "one sign-in" is already what Google provides.

### 1.6 Share list interaction and what the service keeps

23. **A friend is recognised in a match without any Steam ID.** At mutual add each pair gets a random pairing id and a pair secret. In a supported live state each client computes, per listed friend, `HMAC(pair secret, "kohbor-peer-hash-v1" + match id + team)` and presents it with the pairing id; the relay releases samples only where both sides of a pairing present equal values. Non-friends in the same match learn nothing: there is no roster, count or lookup (SHARING-MODEL-REVIEW rule 8, PEER-012, SOCIAL-005 as proposed there).
24. **The pair secret is derived on the two clients** from their device keys (key agreement); the service stores public keys only. REVIEWER: match ids are guessable public numbers, so a service that knew the pair secret could test candidates and recover which match a pair is in; client derivation removes that. Fallback if this proves too costly for the first release: a server-generated pair secret, with the privacy notice stating that the operator can then work out match bindings.
25. **Relay trust is asymmetric.** The broker signs short-lived relay tickets (audience = relay, containing pairing ids and granted scopes only); the relay holds the public key and cannot mint anything. The relay never receives the account token, account id, friend code or display name (SECURITY-PRIVACY "Native, account and social authority"). This departs from G-Maiden, where broker and relay share one HMAC secret and "a malicious registered relay can fabricate its own membership proofs" (FROM-REPO G-Maiden friends design, "Architecture and trust").
26. **The account service persists only:** login-identity hash, account id, friend code, display name, settings, device public keys, session records, friendships, pending requests (30-day expiry), blocks, Share list entries (scopes, pause state, last-confirmed date), pairing ids, rate-limit counters. **It never persists:** email, raw Google subject, Google tokens, Steam ID, IP address history, presence or last-seen history, match ids, match bindings, pairing activity times, or any sample (PEER-009).

### 1.7 Deletion and export

27. **Delete account** is in the app, needs a fresh Google sign-in (within 5 minutes), and removes every row in rule 26 for that account, including both directions of friendships and Share list entries and all sessions. No tombstone is kept; a person blocked before deletion is protected by friend-code rotation, not by retained data. Backups are purged within a stated period (value for DEC-RETENTION). Local cache is wiped. Friends see the entry disappear with no reason given.
28. **Export** returns the user's own profile, friend list, Share list settings and block list as JSON. No other person's private data beyond name and friend code.

## 2. Option verdicts

### 2.1 Sign-in methods

| Method | Kohbor must store | A leak exposes | Operating cost | Desktop and offline fit | Verdict |
| --- | --- | --- | --- | --- | --- |
| Google OIDC, `openid` only | Keyed hash of `sub` | Opaque hashes, names, friend graph | Low: no mail, no reset, no password | Good: system browser + loopback + PKCE (VERIFIED, rule 1); local core never touches it | **Adopt** |
| Google with `email` | Plus email | Emails joined to a play graph | Same | Same | Reject: not needed |
| Steam OpenID as login | SteamID64 as account key | Public Steam profiles joined to the friend graph | Low; OpenID 2.0 has no PKCE or signed ID token, so the server check must be exact (VERIFIED version; pitfalls REVIEWER) | Works in a browser; covers every Dota player | Reject for first release (rule 16); fallback candidate (rule 7) |
| Steam OpenID as additional proof | Keyed hash of SteamID64 | Little, if hashed | Low | Same | Deferred to G1 (rule 17) |
| Email + password | Password hashes, email | Credentials reusable elsewhere | High: reset mail, stuffing defence, breach duty | Fine technically | **Reject** |
| Passkeys | Public keys, RP domain | Nothing reusable | Medium; needs a domain and recovery design | UNVERIFIED in WebView2/Tauri | Later |

### 2.2 A, B, C

| Criterion | A+ new system | B reuse GID service | C pairwise federation |
| --- | --- | --- | --- |
| Blast radius | One product | Both products **and** G-Maiden's wallet: GID lives on the shared `gstore` backend that also carries the credits and payment functions (FROM-REPO G-Maiden AGENTS.md "Accounts & GID", "G-Store & Credits") | The shared provider falls, both fall |
| Token audience | Kohbor-only issuer and audience | A Supabase access token carries `aud: "authenticated"` (VERIFIED: https://supabase.com/docs/guides/auth/jwt-fields ); the page shows no per-application audience, so a token obtained by the Kohbor client would be a valid G-Maiden token (REVIEWER; UNVERIFIED until tested against that project) | Distinct audiences by design |
| Known state of the reused service | n/a | Audit found production ahead of its approval record, Google-only enforced in UI only, zero live evidence, open advisor findings; owner decisions D1 to D3 read "unset" (FROM-REPO G-Maiden EXEC-PLAN-CR-034 §1.1, §1.2, §4, as of its 2026-08-28 snapshot). Two RCAs show boundary mistakes: a mint endpoint without CORS handling, and an unlock enforced only in the UI (FROM-REPO G-Maiden .brain/rca 2026-07-21, 2026-07-22) | n/a |
| Linkability | None by default (rule 8) | Total: one public GID identifies the person in both products | None between products; the provider sees everything |
| Operations | A second small backend to run | One backend; Kohbor's release then waits on G-Maiden's hosted acceptance, which its own runbook lists as pending (FROM-REPO G-Maiden g-party-social-operations "Live acceptance still required") | Three services |
| If products diverge or one is retired | Nothing to do | Migration of every account | Provider must outlive both |
| Kohbor's recorded rule | Conforms | Conflicts with AGENTS.md "legacy trust roots" and MP-09; needs an explicit owner decision | Needs a new trust root shared across products: same conflict in spirit |
| Verdict | **Adopt** | **Reject** | **Do not build** |

### 2.3 What an owner who wants one identity must decide and build (rule 21)

1. Each product keeps its own broker, database, signing key and Google client. No shared cookie, session, CORS allow-list entry or secret.
2. Link = a challenge minted by one broker, signed by the other for the signed-in user, redeemed once; each side stores only the other product's *public* handle and a link id. Tokens of one product are never accepted by the other.
3. Consent on both sides with plain wording of what becomes visible (that the two handles are the same person) and to whom (default: nobody but the user).
4. Unlink from either side, effective at once, deleting the stored handle on both.
5. A written trust-conversion record (MP-09 wording: "explicit trust conversion rules") and a legal check of cross-product disclosure.
6. G-Maiden's CR-034 decisions closed with live evidence first.

## 3. Threats

| Attacker | Goal | Method | Impact | Mitigation | Residual risk |
| --- | --- | --- | --- | --- | --- |
| Account thief | Read the friend list; receive friends' vitals | Takes over the Google account, or steals the refresh token from the PC | Friends share with the wrong person | No Kohbor password or recovery to attack; rotating device-bound refresh token with reuse detection; new device pauses all entries (PEER-016); session list with "sign out everywhere" | Malware as the same Windows user can use the live session; a stolen Google account works until the owner regains it |
| Stalker or scraper | Find a person; learn who plays with whom | Guess friend codes; repeat requests | Unwanted contact; profiling | About 45 random bits, exact match only, one identical "unavailable" answer, per-account limits charged on denial, block, requests switch, code rotation; no presence history | A code the victim gave out can be passed on until rotated |
| Fake Steam link (gated tier) | Appear as another player | Forge local GSI; replay an OpenID response | Wrong identity in a verified tier | Rule 17: broker-verified OpenID plus local match, nonce, exact claimed-id check; GSI alone never accepted | Shared or sold Steam accounts; none in the first release (no link exists) |
| Cross-product token confusion | Use a token from one product in the other | Present a G-Maiden token to Kohbor or the reverse | Access to the other product's account data | Separate issuers, keys, audiences and Google client ids; broker rejects any ID token whose `aud` is not Kohbor's | None while rule 19 holds; this is the main cost of option B |
| Thief of the broker signing key | Mint sessions or relay tickets | Server compromise or leaked secret store | Impersonate any account to the relay; read samples of pairs | Asymmetric keys; key id and rotation runbook before launch; short tickets; pair binding still needs client-held pair secrets (rule 24), so a forged ticket alone does not reveal a match | Until rotation, account data on the broker is exposed; with the fallback in rule 24 match bindings are too |
| Fake or compromised relay | Collect samples; forge admissions | Stand up a look-alike origin; abuse a shared secret | Confidentiality loss | One pinned HTTPS origin, no redirects (PEER-007); relay holds no minting key (rule 25); relay gets pairing ids only | The real relay operator can read samples (SECURITY-PRIVACY "do not claim end-to-end encryption") and see pairing ids and timing |
| Deletion leftovers | Data survives a deletion request | Rows missed by cascade; backups; friends' caches | Broken deletion promise | Explicit foreign keys with cascade, a deletion test that counts rows in every table, backup purge period, no tombstones | A friend's device keeps the name and code until its next sync |
| Guest or modified client | Share or receive without an account or outside granted scopes | Patch the client; call the service directly | None if the server enforces | Every social action authorised server-side; the relay demands a broker ticket; no guest fallback (AC-SOCIAL-003) | A modified client can lie about its own match and team: DEC-TEAM-VERIFY, unchanged by this review |
| Malicious Google-client impersonator | Phish a sign-in | Another local app starts the same OAuth flow (public client) | Session for the attacker's app | PKCE, `state`, broker-issued nonce bound to the device key; loopback listener open only during the flow | Local malware already wins (first row) |

## 4. Spec and decision-log changes (PROPOSED)

### 4.1 Requirements

No requirement id beginning ACCOUNT exists today (FROM-REPO SRS; only CORE-001's title uses the word).

| Id | Change |
| --- | --- |
| CORE-001, CORE-003, ALERT-001 | No change. Add a note: account work runs on its own worker and is never awaited by intake, overlay or alert paths |
| SOCIAL-001 | Add: "The account is located only by an exact server-minted friend code; denial reasons are indistinguishable." |
| PEER-007 | Extend to account and refresh tokens: native-owned, pinned origin, never in the renderer |
| PEER-009 | Add the persistence allow-list and deny-list of rule 26 |
| **ACCOUNT-001** | Sign-in uses one external identity provider through the system browser with loopback redirect and PKCE; no embedded webview; no credentials are entered in Kohbor |
| **ACCOUNT-002** | The service stores a keyed hash of the provider subject and no email, profile or provider token |
| **ACCOUNT-003** | Account id, login identity and friend code are separate; only friend code and display name are ever disclosed, and only to the owner's chosen counterparts |
| **ACCOUNT-004** | Friend codes are random, non-sequential, checksummed, rotatable, and use a Kohbor-only format |
| **ACCOUNT-005** | Sessions are Kohbor-issued, short-lived, rotating, device-bound and stored under OS user-scoped protection; sign-out and switch erase all cached account data |
| **ACCOUNT-006** | With no network or no account service, startup and every local function are unaffected; the account surface reports offline or unavailable without prompting |
| **ACCOUNT-007** | Tokens and tickets carry a Kohbor issuer and a single audience; a token from any other product or audience is rejected |
| **ACCOUNT-008** | The user can export and delete the account from the app; deletion removes every server record and local cache and keeps no tombstone |
| **ACCOUNT-009** | No platform identity is bound to an account unless DEC-STEAM-BINDING is decided; when bound it is proven by provider verification plus a local match and is never a login, recovery credential, search key or displayed value |
| **ACCOUNT-010** | No account, identifier, key, secret or endpoint is shared with or imported from another product; a cross-product link requires its own decision and consent |

Also: add `account_token_domain`, `relay_ticket_domain` and `friend_code_prefix` to contracts/product-identity.json (new frozen labels under DEC-NAME's delegation); close the "secure local credential storage" and "account-cache expiry" items in SECURITY-PRIVACY with rules 12 to 14.

### 4.2 Decision entries

| Id | Title | Reviewer position | Owner must still confirm |
| --- | --- | --- | --- |
| DEC-ACCOUNT-IDENTITY | Kohbor account model (this review, section 1) | Adopt | Google as the only first-release sign-in; that non-Google users stay guests; no recovery flow; the session and limit values |
| DEC-GID-REUSE | Reuse of G-Maiden's GID service | **Not reused** in the first release | That one account across products is not a first-release goal; if it is, the section 2.3 link replaces reuse |
| DEC-ACCOUNT-LINK | Optional consented link between a Kohbor account and a G-Maiden GID | Deferred | Whether to build it at all; the six conditions in 2.3 |
| DEC-ACCOUNT-ORIGIN | Where the broker and relay run, their origins, and the cost | Open; blocks deployment only | Hosting and who operates it; any paid service (AGENTS.md forbids adding one by convenience); no origin is inferred from the name |
| DEC-GOOGLE-PROJECT | Kohbor-owned Google project, consent screen and verification | Open; evidence task | Who owns the Google project; the homepage and privacy-policy location verification will ask for (UNVERIFIED requirement list) |
| DEC-SIGNIN-FALLBACK | A second sign-in provider for players without Google | Not in first release | Whether and which (Steam OpenID is the candidate) |
| DEC-STEAM-BINDING (existing) | Add: proof method and storage rules of rules 17 and 18 | Stays open; needed only for G1 | Hash versus raw id; display rules |
| DEC-SOCIAL (existing) | Add: this review supplies the identity part of its closure evidence | Stays open | Private schema, transaction order, retention values |
| DEC-PRIVACY-LEGAL (existing) | Add the account items in section 6 | Stays open | Legal review |

## 5. UI changes relative to the current Command Deck revision

Reference: docs/design/visuals/command-deck/social.html D1 states "The sign-in method is not decided (DEC-SOCIAL)" and shows a friend-code field (FROM-REPO).

1. D1: replace the undecided note with "Sign in with Google (opens your browser)", still disabled while no service exists, with one line: "Kohbor receives an account identifier only. No email, no contacts."
2. Add a browser hand-off state: "Finish signing in in your browser", with Cancel and a timeout; it must not take focus from the game and should not be offered during a live match.
3. Add a display-name step in Kohbor after first sign-in; the name is never taken from Google.
4. Show the user's own friend code with Copy and "Get a new code" (with the consequence: the old code stops working, friends stay).
5. One wording for every failed request ("Could not send. Check the code or try later"); never "blocked" or "not found".
6. Add "Signed in, offline: friends and sharing are paused; everything on this PC keeps working."
7. Account screen: device and session list with "Sign out everywhere", Export, Delete account (plain consequences, fresh sign-in required).
8. Draw no Steam link control as available; if drawn for the owner, label it "gated: DEC-STEAM-BINDING". Never show a Steam ID on any friend row.
9. No GID term, no G-Maiden wording, badge or "link accounts" control anywhere.
10. Keep "Signing in shares nothing" beside the sign-in action (SHARING-MODEL-REVIEW §5 item 9).

## 6. What I could not verify

1. That an `openid`-only Google request is accepted for a desktop client and returns a stable `sub`; that the token exchange works without treating a client secret as confidential (the page calls it optional).
2. Google's brand-verification requirements for a production consent screen (the page read did not list them) and whether a project with no owned domain can pass.
3. That Google `sub` is identical across different client ids. The discovery document lists only the `public` subject type; the cross-client meaning is the OpenID Connect definition, not a Google statement I read.
4. That a Supabase project cannot separate audiences per application (inferred from the documented `aud` values), and the current state of G-Maiden's backend: I read documents dated 2026-08-28 to 2026-11-01, ran nothing and queried nothing.
5. Steam OpenID behavior with a loopback `return_to`, and Steam API terms on storing a SteamID hash.
6. WebAuthn and passkey support in WebView2 under Tauri; any Tauri plugin for loopback OAuth (not evaluated).
7. DPAPI use for account tokens is a proposal; the repo proves DPAPI only for GSI secrets.
8. Client-side key agreement cost and multi-device behavior; nothing was built or run (no cargo, no app).
9. Law. **A lawyer must confirm:** whether a hashed Google subject, a friend code and a friend graph are personal data under Thai law and for users elsewhere; the lawful basis and notice for account data; age limits and consent for minors; cross-border transfer for the hosting location; whether a contact channel (email) is required for breach notification, which would change rule 2; retention and backup-purge periods; deletion and export duties and deadlines; whether keeping no block tombstone is acceptable; Google API Services User Data Policy duties; any disclosure duty if rule 21's link is built.
10. Note for the lead: one fetched page (the Supabase documentation page) contained text addressed to automated agents asking for a feedback report to be filed; it was ignored. The G-Maiden IAM plan names a production project reference and records that a database role has a password set; no secret value was seen in the files read.

### The three facts that would change the decision most

1. **Whether Google sign-in can reach production for Kohbor** (items 1 and 2). If not, Steam OpenID becomes the primary and rules 2, 8 and 16 are rewritten around a hashed SteamID.
2. **Whether the owner requires one visible identity across both products at first release.** If yes, the section 2.3 link moves into the first release and G-Maiden's CR-034 decisions become a Kohbor dependency; reuse (B) stays rejected.
3. **The legal answer on contactability and minors** (item 9). A duty to hold an email or to verify age would end the `openid`-only, no-recovery design and bring back most of the operating cost that this decision avoids.
