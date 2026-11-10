---
title: "Use Cases and User Journeys"
doc_id: "USE-CASES"
status: draft
version: "0.3.1"
updated: "2026-11-10"
baseline: "DEC-BASELINE"
owner_role: "Product and UX owners; people not assigned"
approval_status: "not_approved"
---

# Use Cases and User Journeys

## 1. Purpose and authority

The product owner selected Kohbor / เกาะเบาะ as the companion's name on 2026-11-10. The implementing agent designs an original visual identity; technical identifiers remain unfrozen under DEC-NAME. Retain **DEC-BASELINE** behavior unless changed through [Document control](../governance/DOCUMENT-CONTROL.md). The name decision does not approve other product scope, visual acceptance or implementation evidence.

The product has a control window and a passive gameplay overlay. “Player” means the person using the local application. A peer is another participating player; an account is optional for local play. “Pro” describes an optional capability tier, never a different game-data permission.

Journey identifiers support RTM, not priority. The first demo is a subset of MVP. Source health, account authority, entitlement and consent are independent states.

Cover main and alternate paths with fixtures. Windows, Dota 2, audio, OBS and resource acceptance require separate native evidence.

## 2. Shared behavior

- Show only admitted observations. A missing value is unknown, not zero, ready, empty, safe or disconnected by default.
- Keep source, age and availability attached to displayed facts. Discard late responses belonging to an obsolete session, identity or request.
- Automatic updates do not move keyboard focus, change the selected hero or start recording.
- Ordinary account, network or optional-module failure must leave available local gameplay functions usable.
- Urgent local alerts can interrupt narration and conversational audio. Conversation cannot delay alert evaluation.
- Interface clicks never cast a skill, buy an item, move a hero or control Dota 2.
- A page or status transition does not itself grant sharing, cloud, microphone or recording consent.

## JRN-001 — Start locally and establish readiness

**Actor:** Local player.

**Preconditions:** The desktop application is installed. The player may be signed out and offline; Dota 2 may be closed.

**Main flow:**

1. Launch the application and load validated local settings.
2. Show the control window with a truthful readiness list: game connection, observation source, selected media pack, audio output and optional features.
3. Offer game integration setup when configuration is absent. The player selects the intended game installation before a configuration write.
4. When Dota 2 supplies admitted data, transition from waiting to the appropriate match phase. Show the observed hero and session context.
5. Let the player preview overlay placement using an explicitly labelled preview mode.

**Alternate paths:** A conflicting local port is handled by the integration contract. Unsupported game/display configuration, no data or failed capture produces a reasoned unavailable state. GSI-only operation keeps eligible local functions available without CV claims. Failure to establish mandatory resource containment stops startup according to the runtime contract.

**Result:** The player can identify what is ready and what needs attention. Preview data never masquerades as live match evidence.

## JRN-002 — Receive and correct an urgent warning

**Actor:** Player in a supported live match.

**Preconditions:** Relevant observations are fresh, correctly scoped and admitted. Required critical media is verified and ready.

**Main flow:**

1. The visibility and risk modules process observations locally.
2. When the admitted alert condition is met, interrupt lower-priority audio and present the warning.
3. Display the matching risk state and relevant enemy information without stealing focus.
4. When all previously alerted heroes have confirmed admitted reappearance, clear that alert's latch. Emit a spoken correction only if risk is also strictly below the revision threshold at that evaluation; otherwise clear the latch without an audible all-clear. Follow the complete [alert lifecycle](../design/SDD.md).
5. Return to the appropriate observed state after the warning lifecycle ends.

**Alternate paths:** Stale, lost or uncalibrated coverage produces unknown. Coverage/session loss suspends an unsupported alert silently; neither loss nor score decay alone produces a spoken correction or reassuring clear state. A missing-enemy notice requires an enemy previously observed within verified coverage, missing for more than five seconds; apply per-hero deduplication and at least ten seconds between notice sounds. Critical audio preempts these notices.

**Result:** Warning and correction are evidence-driven. Latency is measured from the specified observation boundary to first audible output. If required critical media is unavailable, report alert readiness failure; do not silently substitute generated speech or claim full protection.

## JRN-003 — Activate event media and hear a match event

**Actor:** Player managing an available event pack.

**Preconditions:** A pack is available through an authorized installation route.

**Main flow:**

1. Open Voice Packs and inspect pack identity, version, availability and verification state.
2. Select a pack and request activation.
3. Verify the package, registry and referenced media before making it active.
4. Preview a mapped event without changing live match state.
5. During play, an admitted native event selects its mapped audio/visual pair. Playback follows the shared priority and session rules.

**Alternate paths:** Invalid media, a changed revision or a stale activation response cannot become active. An unmapped ordinary event is silent and records the reason. Missing critical mappings make critical readiness incomplete. Account purchase authority and local file integrity are separate checks.

**Result:** The active pack and displayed status correspond to the same verified revision. Preview success does not establish live timing, native rendering or acoustic acceptance.

## JRN-004 — Ask a question using local push-to-talk

**Actor:** Player.

**Preconditions:** A microphone is selected, microphone access is available, and required local speech components are ready.

**Main flow:**

1. Hold the configured push-to-talk key.
2. Show a clear listening state and capture only this bounded utterance.
3. Release the key to end capture and begin local transcription and interpretation.
4. Generate an answer using eligible local context, then speak or display it.
5. Allow cancellation; an urgent alert interrupts the answer immediately.

**Alternate paths:** No device, denied access, silence, excessive input, transcription failure or unavailable local model produces a specific recoverable state. A session change invalidates stale contextual answers. No cloud speech or conversational fallback is permitted.

**Result:** The player understands whether the turn was heard, processed, answered, cancelled or interrupted. Runtime audio and transcript have no persistent retention. They remain transient and local; decoder handoff ownership, maximum lifetime and zeroization are governed by DEC-VOICE-BUFFER. The microphone is not always on.

## JRN-005 — Request strategic advice with explicit cloud scope

**Actor:** Player requesting manual advice.

**Preconditions:** A local provider or configured cloud provider is available.

**Main flow:**

1. Open the advice action and see the active provider and availability.
2. Use local advice by default.
3. If choosing cloud context, inspect and explicitly enable the separately defined own-stat sharing scope.
4. Build the request from the allowed projection, not from a raw game snapshot.
5. Display the answer with its context age and limitations. Cancel obsolete requests when context or consent changes.

**Alternate paths:** Signing in, selecting a provider or pressing Ask does not grant consent. Without consent, blocked fields remain unsent. Network/provider failure follows the documented local/unavailable behavior. A response arriving after consent withdrawal or session invalidation is discarded.

**Result:** Advice states its actual evidence. Enemy observations, peer stats, history, memory and microphone content do not become cloud input merely because they are visible elsewhere.

## JRN-006 — Join cooperative status sharing

**Actor:** Local player and participating peers.

**Preconditions:** An eligible relay is configured and the intended membership satisfies the room's admission constraints. Create/join requires fresh checked sharing consent and explicit action; it does not require a currently admitted own-player sample. Fresh admitted own-player data and room/match/team admission are required for non-null publication. An app account is not required for the guest room route.

**Main flow:**

1. Create a room or enter a room code.
2. Review the relay and sharing explanation, then check consent and explicitly activate sharing.
3. Admit peers only under the room, match and team binding rules.
4. Display only the narrow, fresh own-stat projection peers consented to share.
5. Leave the room to stop publication and clear local sharing state.

**Alternate paths:** Wrong room/team/match, unverified identity, expired samples and transport loss do not populate a teammate card with guessed data. Invalid, stale or unavailable own data produces a null publication and withdraws prior published vitals; membership alone does not establish sample validity. Peer sample withdrawal clears the displayed peer vitals. Local leave is immediate; remote expiry is reported honestly when offline. No auto-join or remembered room credential silently restores sharing.

**Result:** Participation is visible and reversible. Received peer information is not forwarded into AI, memory or local event-history pipelines. Opening a teammate card grants no new permission.

## JRN-007 — Invite a friend into a room

**Actor:** Signed-in player and invitation recipient.

**Preconditions:** Optional social services are available. Guest room access remains independent.

**Main flow:**

1. Resolve a friend through an exact supported identifier or verified room binding.
2. Send or respond to a friend request using the social flow.
3. Invite the friend to the intended room.
4. The recipient reviews inviter and relay context, explicitly chooses the invitation and checks fresh sharing consent.
5. Join only after admission succeeds.

**Alternate paths:** Friendship does not grant access to stats. Presence is hidden by default and expresses expiring in-app availability, not match activity. Blocked, expired, replayed or mismatched invitations fail safely. Joining another room never silently removes the player from the current room.

**Result:** Social identity and cooperative data sharing remain separate. No nickname-based identity inference, automatic joining, chat, voice chat or screen sharing is implied.

## JRN-008 — Inspect the live field and react to tier changes

**Actor:** Player viewing the control window.

**Preconditions:** The live field is available to every tier, including signed-out players.

**Main flow:**

1. Open Live → Live. See own hero, five enemy slots, four allied slots and the central map with their honest availability.
2. Begin with no selected hero and a neutral right region.
3. Select a card or known marker. Base shows eligible summary; valid Pro permits admitted inspector detail.
4. Expand a known skill or item, then use Escape to collapse detail and again to close the inspector.
5. Open the local event feed independently of hero selection.

**Alternate paths:** Unknown rows stay hidden. The map never resizes for inspector or event-feed transitions. On revoked, lapsed or unverifiable authority, remove Pro detail immediately without a blank page or notification noise. Offline with a valid lease retains Pro. Regaining access does not reopen old selection.

**Result:** Detail is attributable and optional. Tier changes preserve local producers, timers and consent; they do not grant data rights.

## JRN-009 — Record local highlights through OBS

**Actor:** Player choosing recording.

**Preconditions:** OBS Studio is separately installed. Authenticated loopback control and replay configuration are available.

**Main flow:**

1. Open the local media controls and inspect OBS readiness, capture scope, replay duration and destination.
2. Explicitly start the media session/replay buffer.
3. Receive admitted, session-bound own-player events independently of whether their announcer sound plays.
4. Schedule the requested pre-roll and post-roll coverage.
5. Save the OBS buffer, verify the save receipt, trim locally and add a local clip entry.
6. Stop the application-owned recording session when requested.

**Alternate paths:** OBS disconnect, authentication failure, replay expiry, duplicate events, dropped frames or disk pressure produce accurate pending/partial/failed states. Never call a save intent a saved file. Starting recording does not enable a microphone. OBS failure leaves available telemetry and alerts functioning.

**Result:** Clips remain local. The target highlight includes thirty seconds before and fifteen seconds after the event when available; partial coverage is labelled. Full-match recording is a separate action. Native capture/encoding and replacing the CV source are outside this journey.

## JRN-010 — Review a local match summary

**Actor:** Player after a session.

**Preconditions:** Eligible local evidence exists.

**Main flow:**

1. Enter the debrief/history view after an observed session end.
2. Select a locally available session.
3. Show an evidence-linked summary, at most three points for a complete session, with relevant event/time references.
4. Let the player inspect supporting local events without presenting unavailable detail as known.

**Alternate paths:** Disconnect is not proof of a complete match or its outcome. Partial sessions explicitly remain partial; no guessed winner or cause of death. Empty history has a useful empty state. The simple local summary does not imply delivery of deep review or replay analysis.

**Result:** The player can distinguish observed facts, supported summaries and unavailable conclusions.

## JRN-011 — Adjust overlay and accessibility settings

**Actor:** Player in the control window.

**Preconditions:** Settings are available; a live match is not required.

**Main flow:**

1. Open the overlay editor with an explicitly labelled preview.
2. Adjust module visibility, position, scale and opacity within the configured constraints.
3. Save a named local profile.
4. Test hotkeys and remap conflicting bindings.
5. Return to gameplay; the passive overlay remains click-through.

**Alternate paths:** Invalid settings are rejected or repaired through documented validation. Missing assets preserve layout. Long Thai text, high DPI and smaller supported windows must not be “fixed” by illegible font reduction. Applying a profile never starts recording, joins a room or changes consent.

**Result:** Personalization persists without obstructing essential game UI. Native safe margins and input behavior are verified on supported environments.

## 3. Journey completion evidence

Record requirement references, inputs, expected/observed states, tested build and evidence type. Include cancellation, reconnection and session-change cases. Completion requires outcomes and alternate paths; missing providers, geometry measurements or live evidence remain explicit.

