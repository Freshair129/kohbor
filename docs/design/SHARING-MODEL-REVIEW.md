---
title: "Sharing and Sign-in Model Review"
doc_id: "SHARING-MODEL-REVIEW"
status: draft
updated: "2026-11-12"
authority: "Independent reviewer output for the product owner; PROPOSED, changes no baseline until recorded under DOCUMENT-CONTROL"
approval_status: "not_approved"
---

# Sharing and sign-in model review

Reviewer role: independent product, security and privacy review. This is an agent review, not human approval and not legal advice. Everything in sections 2, 4 and 5 is **PROPOSED by the reviewer**.

Evidence labels: **VERIFIED** = checked against a source on 2026-11-12 (URL given; summaries were read through a fetch tool, so quote wording should be re-read by a human before it is cited externally). **FROM-REPO** = file and section. **UNVERIFIED** = not confirmed; the confirming step is named.

## 0. Decision in brief

Adopt **(D): Share list first, team-wide sharing built but gated.**

- Sign-in stays optional. Mandatory sign-in (C) is rejected.
- First release transmits gameplay data only between **mutual Share list friends** on the same team in the same match.
- **Team-wide automatic sharing with strangers does not ship** until a verifiable source of team membership exists (gate G1, section 2.3). The owner's idea is sound as a product nudge; it is unsafe today because the "same team" test rests on a claim any player can forge with one HTTP request.
- Guests keep the full local product and get a static, honest explanation of what an account adds. No live count for guests in the first release.

## 1. Verdicts on H1 to H7

| # | Verdict | Evidence |
| --- | --- | --- |
| H1 Mandatory sign-in contradicts the baseline | **Confirmed** | FROM-REPO: SRS CORE-001 ("shall start without account, network or premium authority"; failure behavior "Do not lock gameplay functionality behind sign-in"); AC-CORE-001 cold start with no account and no network; PRD principle 1 and PN-001; SOCIAL-001; UX-SPEC §7 last paragraph; DECISION-LOG "What needs a product or contract decision" lists new account services. Mandatory sign-in also makes an account service outage a full product outage, and no account service exists yet (DEC-SOCIAL open). |
| H2 Auto-share with strangers departs from the consent model | **Confirmed** for the baseline; **legal part UNVERIFIED** | FROM-REPO: PEER-001 (no autojoin), SOCIAL-002, PEER-009 failure behavior ("Friendship/premium/cloud consent cannot expand recipients"), PRD principle 4 ("Sign-in or friendship does not substitute"), SECURITY-PRIVACY line 15 ("account session ... cannot expand a data permission"). Sign-in acting as sharing consent is exactly what PN-010 forbids. PDPA: only secondary summaries were found (section 6). |
| H3 Team spoofing | **Confirmed, and worse than stated** | See 1.1. No verifiable team source was found for live public matches. The forgery needs no modified client. |
| H4 Network-effect problem | **Confirmed by reasoning; no usage data exists** | A guest sees a locked state only when a signed-in Kohbor user is on the same team. With few users this is rare, so the nudge is weak at exactly the time it is needed. A live locked state for a guest also requires the guest's client to tell a server which match it is in, which ends "guest = local only" (FROM-REPO: PRD principle 1; AGENTS.md "do not add cloud data"). Guest mode must stand alone. |
| H5 Reciprocity is fairer and nudges | **Partly** | Fair as a tier rule ("you receive team vitals only while you send them"). It is **not** a security control: a spoofer can send fabricated vitals at no cost. It also changes DESIGN-NOTES §13 safeguard 3 (each direction separate), and "must send to receive" may weaken how freely consent is given (UNVERIFIED legal point). Keep per-direction scopes on the Share list; apply reciprocity only to the team tier. |
| H6 Data minimisation by tier | **Partly** | Direction is right and matches the baseline: PEER-003 DTO is hero, level, HP, max HP, mana percent, alive, respawn (FROM-REPO: contracts/peer-sample.schema.json); abilities are a separate scope (PEER-010); gold and net worth are local-only data today (SECURITY-PRIVACY line 23) and PEER-003 excludes them. Two corrections: (a) "a teammate can already see this in-game" is UNVERIFIED here (confirm in the client: ally HP, mana, level, respawn, ability cooldowns, gold); (b) minimisation limits the damage of spoofing but does not remove it: enemy HP, mana and respawn through fog of war are hidden information. Position must never enter any peer DTO (the capture shows `hero.xpos/ypos` exist in the payload: FROM-REPO GSI-INTAKE-SPEC §10.1 A-5). |
| H7 Count-only placeholder for guests | **Partly** | Naming is not acceptable: agreed. A count is acceptable for **signed-in, opted-in** users, whose clients already send a match binding. For guests it needs a network call from a signed-out client (see H4), and a count of 1 inside a two-person party identifies the person. Decision: guests see static copy, no live count, in the first release. |

### 1.1 H3 in detail: is team membership verifiable?

| Question | Finding | Label |
| --- | --- | --- |
| What does GSI give a playing client? | Own player and own hero only; all players only when spectating. | VERIFIED (community): MrBean355/dota2-gsi wiki "Library Guide", page edited 2026-03-05, https://github.com/MrBean355/dota2-gsi/wiki/Library-Guide ; antonpup/Dota2GSI README, https://github.com/antonpup/Dota2GSI . FROM-REPO: GSI-INTAKE-SPEC §10.1 capture (own `player.steamid`, `player.team_name`, `map.matchid`; spectator shape A-6 **not tested**). No official Valve Dota 2 GSI specification was found. |
| Can a player forge their own GSI input? | Yes. The install token sits in plain text in the owned cfg file, and intake is a loopback POST. Any local program can post a body with a real `matchid` and the opposite `team_name`. | FROM-REPO: GSI-INTAKE-SPEC §2.3, §3, §4.1 (authenticity proves "a game client configured by this installation", not truth of content). |
| Steam Web API: live match and team for a given account? | `IDOTA2MatchStats_570/GetRealtimeStats` takes only `server_steam_id`. No method was found that maps an account to its live match and side. Response schema, coverage of ordinary public matches, delay, and how a third party would obtain `server_steam_id` for an arbitrary pub game: not confirmed. | VERIFIED for the parameter list: https://steamapi.xpaw.me/IDOTA2MatchStats_570 (community reference). UNVERIFIED for the rest: call the endpoint with a key during a real pub match and record the response and delay. |
| `GetLiveLeagueGames` | Returns in-progress games with `players[].account_id`, `hero_id`, `team`; described with `league_id`. Whether ordinary matchmaking games appear is not stated. | VERIFIED (community wiki): https://wiki.teamfortress.com/wiki/WebAPI/GetLiveLeagueGames . Pub coverage UNVERIFIED. |
| Steam Web API terms | 100,000 calls per day; retrieve a user's Steam Data only "as requested by the end user"; privacy policy required; Valve may suspend or terminate use "at any time for any reason, without notice". A verification service built on this is rate-limited and revocable. | VERIFIED: https://steamcommunity.com/dev/apiterms (page shows last update July 2010). |
| Steam rich presence, lobby, party | `ISteamFriends` rich-presence calls are a Steamworks client API for a game's own process and are scoped to friends, with a request call for users in the same lobby or server. Use by a non-game companion app, and what Dota 2 publishes there, were not established. | VERIFIED for the API description: https://partner.steamgames.com/doc/api/ISteamFriends . Applicability to Kohbor UNVERIFIED. |
| Does any client-side roster exist? | Overwolf's Dota 2 events API documents a `roster` with `steamId` and `teamId` (2 Radiant, 3 Dire) and a `party` list, and notes Valve disabled some functionality (names and ids empty until strategy time). So a roster is obtainable by some client-side means on that platform. How Overwolf obtains it, and whether a standalone app may, is unknown. | VERIFIED that the documentation says so: https://dev.overwolf.com/ow-native/live-game-data-gep/supported-games/dota-2 . Method and permissibility UNVERIFIED. Adopting Overwolf is a new platform dependency outside the baseline. |
| Local log files listing all ten players | A known community lead (a server log in the game folder). Not checked this session. | UNVERIFIED: inspect the game folder during a live pub match in the M0-style spike; record format, timing and whether sides are derivable. |
| Does cross-checking client reports stop one liar? | Weakly. (1) More than five claimants on one side of one match is detectable only when all five real players run Kohbor; the liar cannot be singled out, so the only safe response is to stop the whole side (which also gives a griefer a denial-of-service). (2) Hero uniqueness does not help: heroes are unique across both teams, so a Dire player claiming "Radiant, hero X" contradicts nothing. (3) Timing and vitals plausibility cannot distinguish sides. (4) Reciprocity is satisfied by fake vitals. Cross-checking is useful only as a tripwire. | Reviewer analysis from the GSI field set above. |
| Post-match audit | Final rosters are published after a match for many games, so a liar could be detected later and the account closed. Coverage depends on player privacy settings. Deterrence, not prevention. | UNVERIFIED: confirm `GetMatchDetails` roster availability and the effect of the "expose public match data" setting. |

**Conclusion.** With GSI alone, the "same match, same team" predicate is two self-asserted fields. The sound architecture is **sender-side or server-side verification against an identity the claimant cannot choose**: the receiver's account is bound to a Steam identity proven at sign-in, and the *sender's* side (its own client's roster, or a Valve-operated API) confirms that identity is on the sender's team. Neither source is confirmed today. This is gate G1.

### 1.2 Valve policy

- VERIFIED (secondary reporting of Valve's post "Cheaters Will Never Be Welcome in Dota", 2023-02-21; the post body could not be fetched directly): Valve banned over 40,000 accounts for software that read client data "that would never be read during normal gameplay", and warned that running any application that "reads data from the Dota client as you're playing games" can lead to a permanent ban. Sources: https://www.pcgamer.com/valve-lured-40000-dota-cheaters-into-a-trap-before-banning-them-in-one-day , https://gamespot.com/articles/valve-bans-more-than-40000-players-in-huge-dota-2-cheater-purge/1100-6511752/ .
- VERIFIED: Steam Subscriber Agreement §4 (page shows "Last updated on September 10, 2026") forbids using "Cheats, mods, hacks, or any other unauthorized third-party software" and says the subscriber "will not create Cheats or assist third parties in any way to create or use Cheats"; Valve may terminate accounts without notice. https://store.steampowered.com/subscriber_agreement/
- UNVERIFIED: whether Valve treats a GSI consumer as permitted in matchmade games, and whether relaying a player's own GSI-derived state to teammates is acceptable. GSI is a Valve-built output feature, which argues for the first; no Valve text was found for either. Confirm by asking Valve (Dota team or Steamworks support) in writing.
- Reviewer judgement, not a Valve statement: a relay that can deliver enemy HP, mana and respawn through fog of war gives information not visible in normal play. That is the category Valve described in 2023. The exposure would fall on users' accounts and, under the "create Cheats or assist" wording, plausibly on the owner as developer. This is the main reason the team tier is gated.

## 2. Decision (PROPOSED)

### 2.1 Rules for the first release

1. **Sign-in is optional.** No local function, alert, overlay, voice cue or setting depends on an account or network (CORE-001 unchanged).
2. **Guest (signed out):** sends nothing and receives nothing over the network. No match binding, team or identity leaves the device. Guest room codes are not in the first release (owner confirmation needed: this narrows SOCIAL-001's guest-room clause).
3. **Signed-in, no Share list entries:** same network behavior as a guest. Signing in is never sharing consent.
4. **Who can send and receive:** only two signed-in accounts that have **each added the other** to their Share list, and only while both report the same match and the same team in a supported live state (PEER-002 conditions unchanged).
5. **Data per scope:** `vitals` = the existing PEER-003 DTO, nothing more. `abilities` = the PEER-010 DTO, separate opt-in per friend, off by default. `economy` = **not in the first release**. No position, items, enemy data or selected-unit data in any scope.
6. **Default state:** everything off. An entry is created only through the first-time sheet with explicit, unticked choices (DESIGN-NOTES §13 safeguard 6). After that, sharing with that friend starts without a per-match prompt.
7. **Direction:** each direction and each scope is granted separately (no forced reciprocity on the Share list), but nothing flows either way until the listing is mutual.
8. **Blind matching:** the two clients derive the match-and-team binding as a keyed hash under a secret that exists only for that pair, created at mutual add. The service compares tokens for equality and never receives the raw match id, team, or SteamID (keeps SECURITY-PRIVACY line 25 true without rooms).
9. **Pause and revoke:** global pause, per-friend pause, scope withdrawal and removal each act as a PEER-008 leave: stop sending at once, clear received data locally, fence late responses. Pause stops both sending and receiving for that pairing. Resume is always an explicit action. The other side clears through the 2,000 ms staleness rule; immediate remote erasure is never promised.
10. **Expiry:** a Share list entry that has not been used or re-confirmed for 90 days goes inactive until re-confirmed (period is the owner's to set).
11. **Visibility:** the deck indicator names who is receiving and which scopes, in every match, with the pause control beside it.
12. **No persistence:** samples are transient on client and service; never logged, never an AI or memory input (PEER-009). The service stores the list, scopes and pause state only.
13. **Unverifiable team membership (today's state):** the Share list ships because a mutual, deliberate listing bounds who can ever receive. The residual risk (a listed friend on the enemy team forging their side) is stated to the user in the first-time sheet in plain words. Team-wide sharing does not ship.

### 2.2 Nudge without data

- Guests and unlisted signed-in users see one static, labelled panel: what a Share list does, that it needs an account and a friend who also uses Kohbor. No live count, no names.
- An invite path (a link or code a player gives a friend to become friends in Kohbor) is the growth mechanism. It carries no gameplay data.

### 2.3 Gate G1 and the conditional team tier

**G1 passes** when a spike shows, with recorded evidence, a source that (a) identifies the accounts on the sender's team in a live public match, (b) cannot be chosen by the receiving claimant, (c) is available within the first minutes of the match, and (d) is permitted by Valve in writing or by published terms. Candidates: a Valve Web API mapping, or a sender-side roster the game itself provides.

| | If G1 passes | If G1 does not pass |
| --- | --- | --- |
| Team tier | Ships in a later release as **Team vitals**: signed-in accounts bound to a proven Steam identity; one explicit, separate, unticked opt-in (not part of sign-up); vitals DTO only; reciprocity (receive only while sending); teammates shown by hero slot only, never by account name; pause on the deck; a signed-in count of opted-in teammates | Not built into any release. Not replaced by quorum heuristics |
| Share list | Gains the same verification; rule 13's warning is removed | Stays as in 2.1 |
| Abilities, economy | Never on the team tier; Share list only | Same |
| Guest | Still local only; may get an explicit opt-in "check for teammates" count later, owner's choice | Local only |

Until G1 passes, the lead may build everything in 2.1 and 2.2. Nothing in the team tier column is needed for it.

### 2.4 Why not the other options

- **(A) Share list only, forever:** correct for now; rejected as the end state because verified team vitals are low-harm and genuinely useful.
- **(B) as proposed:** rejected for the first release. It puts a forgeable predicate between strangers and makes sign-in an implicit consent.
- **(C):** rejected (H1).

## 3. Threat model

| Attacker | Goal | Method | Impact | Mitigation | Residual risk |
| --- | --- | --- | --- | --- | --- |
| Enemy player, stranger | Enemy vitals through fog | Post forged GSI to own client, or call the service directly, with real match id and the opposite team | Hidden-information cheat; Valve-policy exposure for users and owner | No stranger tier until G1; receivers limited to mutual listings | None for strangers in the first release |
| Listed friend playing against you | Your vitals or cooldowns | Same forgery, but already mutual-listed | Cheat against a friend | Mutual deliberate listing; plain warning in the first-time sheet; pause; tripwire on impossible states (two sides claimed by one pair in one match); G1 later | Real until G1. Bounded to people the user chose |
| Third party not in the match | Feed an enemy | Obtain a live match id, claim a team | Same as row 1 | Same as row 1; pair-keyed binding means a non-listed account has no token to present | None for strangers |
| Scraper | Learn who uses Kohbor, who plays with whom | Enumerate presence, counts or team listings | Profiling of players | No public directory (SOCIAL-001); no live count for guests; signed-in count only on the gated tier; rate limits per account | Friends see presence if the user enabled it |
| Stalker or harasser | Track when a person plays; pressure them | Use presence, "now receiving" notices, or repeated list requests | Harassment | Presence off by default and "app online" only (SOCIAL-002); block invalidates listing and requests (SOCIAL-003); one-sided adds reveal nothing sent; no notification to the other side on pause or removal; request rate limits | A mutual friend learns you are in a match with them, which they already see in-game |
| Malicious or compromised relay / service operator | Read samples; build a social-play graph | Log traffic; correlate accounts, times, tokens | Loss of confidentiality; a "who plays with whom, when" record | Pair-keyed blind binding; transient samples; no raw match id or SteamID; retention values set before any deployment (SECURITY-PRIVACY line 102); state honestly that the operator can read samples (line 66) unless pairwise encryption is added | Operator still sees account pairs and timing. A team tier would force a globally comparable match binding, which is reversible to the real match id: a further reason it is gated |
| Stale data after leave | Keep seeing a player who left, paused or removed | Client ignores clear; delayed responses | Short over-exposure | PEER-008 fencing; service stops fan-out on pause, removal or match end; 2,000 ms staleness; 30 s sync expiry kept from PEER-004 | A hostile receiver keeps what it already received; only freshness is lost |
| Guest with a modified client | Receive without an account | Patch the client | None if enforcement is on the service | Receiving requires an authenticated account and a mutual pair token; the client is never the enforcement point; no guest fallback (AC-SOCIAL-003) | None beyond what an account holder can do |
| Account sharing or sale | Use another person's listings | Sign in with a shared or stolen account | Receiver is not the person the sender chose | Single active session per account; new-device sign-in pauses all listings until re-confirmed; with G1, the Steam identity binding must match the running client | Possible until G1 |
| Griefer teammate | Mislead allies | Send fabricated vitals | Wrong picture on the ally card | PEER-003 bounds and semantic admission; values are labelled as shared by the teammate, never as observed | Plausible false values are undetectable |
| Spectator or coach client | Publish or collect all-player data | Run intake while spectating | All ten players' state | PEER-002: no publication unless playing and owner verified; spectator shape is unsupported scope | A-6 (spectator payload shape) is untested: FROM-REPO GSI-INTAKE-SPEC §10.1 |

## 4. Spec and decision-log changes (PROPOSED)

The Share list itself is not yet a recorded decision: `contracts/decisions.json` has no entry for it, and DESIGN-NOTES §13 says each safeguard "needs an owner decision" (FROM-REPO). Record it before anything is built.

### 4.1 Existing requirements

| Id | Change |
| --- | --- |
| CORE-001, CORE-003 | No change. Add a note that no sharing tier may weaken them |
| PEER-001 | Replace per-join consent with: "Sharing shall require a mutual Share list entry created by an explicit, unticked first-time consent action per friend; no data flows from sign-in, friendship or presence alone." Keep "no view-only mode" as "nothing is received from a friend before the listing is mutual" |
| PEER-002 | Add: "Team and match conditions are re-evaluated on every admitted sample; a change withdraws publication." |
| PEER-003 | No change to the DTO. Add: "No tier or scope may add fields to this DTO; economy requires a separate DTO and decision." |
| PEER-004 | Replace rooms with: "A pairing is active only within one match binding; at most four recipients; unsynced pairings expire after 30 s; an entry unused for the re-confirmation period becomes inactive." |
| PEER-008 | Extend "leave" to: pause, per-friend pause, scope withdrawal, removal, block, sign-out, account switch, match end, and team or match change |
| PEER-009 | Add: "The service persists only list membership, scopes, pause state and the pair secret reference; never samples, match bindings or timing history." Keep the failure behavior; add "sign-in" to the list that cannot expand recipients |
| PEER-010 | Add: "The abilities scope exists only on a Share list entry." |
| SOCIAL-001 | Keep "optional account". Replace "without gating guest rooms" with "guests have no network sharing in this release" (owner confirmation: DEC-GUEST-PEER) |
| SOCIAL-002 | Replace the acceptance case: "friends with presence enabled exchange no gameplay data until each has added the other to the Share list through the consent action." |
| SOCIAL-003 | Add: block or removal racing a pairing resolves to no pairing; no fallback to any other tier |
| DATA-005 | No change. Add a cross-reference: peer data is never a source of enemy state |

### 4.2 New requirements (one line each)

- **PEER-011 Mutual listing:** data flows only when both accounts list each other; a one-sided entry sends nothing and reveals no scopes.
- **PEER-012 Blind match binding:** match and team are compared as a pair-keyed hash; the service never receives raw match id, team or platform identity.
- **PEER-013 Standing-consent visibility:** while any recipient is active, the Live destination names each recipient and scope and offers a one-press pause.
- **PEER-014 Entry expiry:** an entry inactive for the configured period requires re-confirmation before sharing resumes.
- **PEER-015 Team-membership verification gate:** no sharing tier may deliver data to an account outside the sender's Share list unless team membership is verified by a source the receiver cannot choose (G1).
- **PEER-016 Session change pauses listings:** a new-device or switched-account sign-in pauses all entries until the user confirms.
- **SOCIAL-004 No live discovery for guests:** a signed-out client sends no match, team or presence signal; teasers are static.
- **SOCIAL-005 No roster disclosure:** no surface lists or counts Kohbor users in a match except to an opted-in member of a verified tier, and never by account name to non-friends.

### 4.3 New decision entries

| Id | Title | Reviewer position | Owner must still confirm |
| --- | --- | --- | --- |
| DEC-SHARE-LIST | Standing Share list replaces per-join consent | Adopt (2.1) | The baseline change to PEER-001/004/008/009 and SOCIAL-002; first-time sheet wording after a comprehension test |
| DEC-SIGNIN-OPTIONAL | Mandatory sign-in rejected | Decided by this review, for owner acknowledgement | That the nudge is the static panel and invites |
| DEC-TEAM-SHARE | Team-wide vitals among signed-in strangers | Deferred behind G1 | Whether to fund the G1 spike; tier wording; reciprocity |
| DEC-TEAM-VERIFY | G1 spike: verifiable team membership | Open; evidence task | Tester, accounts and a Web API key; whether a platform dependency is acceptable |
| DEC-VALVE-POLICY | Written position on GSI-derived sharing | Open | Who contacts Valve; what is asked; stop condition if the answer is no |
| DEC-PRIVACY-LEGAL | Legal and privacy review before any hosted sharing | Open; blocks deployment, not local build | Items in section 6 |
| DEC-GUEST-PEER | Guest codes | Not in first release | Drop or defer; SOCIAL-001 wording |
| DEC-PEER-ECONOMY | Gold / net worth scope | Not in first release | Whether to pursue at all; fields; DTO; consent text |
| DEC-SHARE-RECONFIRM | Re-confirmation period | 90 days proposed | The value; whether friends are told |
| DEC-STEAM-BINDING | Binding an account to a proven Steam identity | Needed only for G1 tier | This is a privacy expansion against SECURITY-PRIVACY line 25; storage and display rules |

## 5. UI changes relative to the current Command Deck revision

1. Remove the economy chip, the `econ=1` state and the "Gold · proposed" row from deck and sheets for the first release (keep the drawing only in an appendix page marked not in scope).
2. Remove "Join with a code" (social.html D1, D11) pending DEC-GUEST-PEER.
3. Pause: change "stops only what you send" to "stops sending and receiving for that pairing"; update copy (open decision 6 in DESIGN-NOTES §11).
4. First-time sheet: add one plain sentence on the unverified team check ("Kohbor cannot yet confirm which team a friend is on; a listed friend playing against you could still receive this"). Remove it when G1 passes.
5. Add an "inactive, confirm to resume" entry state for expiry, and a "paused after new sign-in" state.
6. Add the static guest/unlisted panel (2.2). No count, no names, no lock icon that implies teammates are present.
7. Do not draw any team-tier surface as available. If drawn for the owner, label it "gated: DEC-TEAM-VERIFY".
8. Reword "Not receiving: other team" so the reason appears only on the Share list screen (already the rule in §13; check `social.html` rows are not reachable from the deck).
9. Account screen: state "Signing in shares nothing" beside the sign-in action.
10. Re-check the indicator with four recipients and long names (already listed as untested in DESIGN-NOTES §12).

## 6. What I could not verify

1. The text of Valve's 2023-02-21 post directly (page body did not load); I relied on press quotations.
2. Any Valve statement on GSI consumers in matchmade games, or on relaying GSI-derived data to other players.
3. `GetRealtimeStats` response fields, delay and pub-match coverage; how to obtain `server_steam_id`; whether `GetLiveLeagueGames` includes ordinary matchmaking; `GetPlayerSummaries` live-server fields (the Valve wiki page returned 403).
4. Whether a local roster source exists for a playing client (log file lead), and how Overwolf produces its roster.
5. The spectator GSI payload shape (repo A-6 untested) and DotaTV delay for public matches.
6. That allies can see each other's HP, mana, level, respawn, cooldowns and gold in the client (expected, not checked).
7. Post-match roster availability and privacy-setting effects.
8. Thailand PDPA: only law-firm summaries were found (for example https://www.linklaters.com/en/insights/data-protected/data-protected---thailand ); the Act's text was not read. **A lawyer must confirm:** whether SteamID-linked live game state and the play graph are personal data; the lawful basis for standing and automatic sharing; whether consent tied to reciprocity or to an account is validly given; rules for minors; cross-border transfer for the hosting location; duties toward players outside Thailand (for example EU residents); retention, access and deletion duties; breach notification; what the privacy notice must say.
9. No user research: whether people understand standing sharing, and whether the static panel nudges anyone.
10. Nothing was run: no cargo, no app, no protocol test. All protocol statements here are design proposals.

### The three facts that would change the decision most

1. **A verifiable team-membership source (G1).** If it exists and is permitted, the team tier ships and the Share list loses its residual risk. If it provably does not exist, DEC-TEAM-SHARE should be closed as rejected, not left deferred.
2. **Valve's written position.** If Valve objects to relaying GSI-derived state to other players at all, even the Share list must stop. If Valve explicitly permits ally-visible data, the policy risk on the team tier falls to the spoofing question alone.
3. **The legal answer on consent.** If standing or automatic sharing needs per-recipient or per-session consent, the team tier is dead and the Share list needs a re-confirmation step stronger than rule 10.
