---
title: "Event and Audio Specification (M2)"
doc_id: "EVENT-AUDIO-SPEC"
status: draft
version: "0.3.6"
updated: "2026-11-11"
authority: "DEC-BASELINE; elaborates MEDIA-001..008, ALERT-001, ALERT-007, VOICE-006, QUALITY-001, QUALITY-007 under DEC-EVENT-AUDIO and DEC-HEALTH-DANGER"
owner_role: "Integration owner (catalog, arbitration, formats); product owner (health danger, mute)"
approval_status: "not_approved"
---

# Event and Audio Specification (M2)

## 1. Status and authority

This document closes the M2 design gaps:

- the canonical event catalog;
- the GSI-driven critical detector;
- audio priority and preemption;
- audio outcome reasons;
- the media format and decode checks;
- the installation request and result contracts.

It elaborates the [Media Package Specification](MEDIA-PACK-SPEC.md) (MP-01 to MP-10) and the requirements listed in the front matter. It does not change any requirement statement, failure behavior or acceptance predicate.

Everything here is a **proposal**:

- **DEC-EVENT-AUDIO** covers the builder-delegated parts: identifiers, ranks of events whose order the baseline does not fix, arbitration mechanics, formats and contracts.
- **DEC-HEALTH-DANGER** covers the one new product behavior, the health-danger detector. The product owner **approved it as proposed on 2026-11-11**.

Nothing has been approved or executed.

## 2. Canonical event catalog (MP-01, MEDIA-001)

[event-catalog.json](../../contracts/event-catalog.json) is the single versioned source. [event-catalog.schema.json](../../contracts/event-catalog.schema.json) defines its shape.

The package validator also enforces these semantic rules:

- Every ID is unique.
- Every voiced ordinary event has a unique, non-zero rank.
- No other category carries a rank.
- Every tunable default lies inside its bounds.
- Every re-arm value lies on the safe side of its threshold.
- A `tier_range` has `min` ≤ `max` and appears only on a media-mappable event.
- The `event_type` enum in [event.schema.json](../../contracts/event.schema.json) must equal the set of non-test catalog IDs. This is the "generated consumer checked against the source" rule.

Taxonomy version `1.1`. Version 1.1 adds tiers to `multi_kill` and `streak`; it adds no ID and changes no meaning:

| ID | Category | Source | Detector milestone | Ordinary rank | Voiced by default | Notes |
| --- | --- | --- | --- | ---: | --- | --- |
| `multi_kill` | ordinary | gsi | M2 | 90 | yes | A run of ≥2 own kills with no more than 18,000 ms between consecutive kills (MP-02). Tier 2–5 |
| `streak` | ordinary | gsi | M2 | 80 | yes | The game's kill streak (own kills since the last own death) reaches 3 or more; each further kill re-emits. Tier 3–10 |
| `first_blood` | ordinary | gsi | M2 | 70 | yes | First hero kill of the match, credited to self (E-1) |
| `hero_kill` | ordinary | gsi | M2 | 60 | yes | Own kill counter increments |
| `hero_death` | ordinary | gsi | M2 | 50 | yes | Own `alive` goes from true to false |
| `level_milestone` | ordinary | gsi | M2 | 40 | yes | Level reaches 6, 12, 18 or 25 |
| `health_low` | ordinary | gsi | M2 | 30 | yes | HP falls to ≤20% of max HP; re-arms at ≥30% or after respawn |
| `mana_low` | ordinary | gsi | M2 | 20 | yes | Mana falls to ≤15%; re-arms at ≥25% or after respawn |
| `match_ended` | ordinary | gsi | M2 | 15 | yes | Game state enters post-game (E-2) |
| `match_started` | ordinary | gsi | M2 | 10 | yes | Game state enters live play for the current session (E-2) |
| `hero_respawn` | ordinary | gsi | M2 | — | **no** | Detected and recorded; not voiced and not mappable in taxonomy 1.x (MEDIA-003) |
| `health_danger` | critical | gsi | M2 | — | yes | Approved 2026-11-11 (DEC-HEALTH-DANGER, §3); audible only with verified critical media (ALERT-007) |
| `enemy_missing` | notice | vision | M3 | — | yes | Absence >5,000 ms with valid coverage; once per hero per episode; global cooldown ≥10,000 ms (ALERT-006) |
| `risk_alert` | critical | native | M3 | — | yes | Admission per SDD §6 and DEC-RISK-DETAIL; cooldowns 30 s global and 60 s per contributor |
| `risk_revision` | correction | native | M3 | — | yes | Evidence-based retraction (ALERT-005) |
| `risk_unavailable` | system | native | M3 | — | no | Status only |
| `clip_requested` | recording | manual | M4b | — | no | Recorder marker |
| `latency_probe` | test | native | M2 | — | test only | Engineering latency fixture. Excluded from production catalogs, mappings, history and `event.schema` |

**Rank provenance.** MP-02 fixes multi-kill > streak > first blood > ordinary kill. The product owner confirmed the remaining ranks as proposed on 2026-11-11: death 50, level milestone 40, low health 30, low mana 20, match ended 15, match started 10.

**Tiers (owner decision 2026-11-11).** The two counted events carry a tier, so a package can voice each step of a kill run differently.

- The tier is the event's `payload.count`, capped at the top of the catalog `tier_range`: `multi_kill` 2–5 (length of the current run; 5 means five or more), `streak` 3–10 (kills since the last own death; 10 means ten or more).
- A tier is not a new event. Rank, arbitration, cooldowns and baselines are those of the event; `multi_kill` still outranks `streak` at every tier.
- A package binds a clip to a tier with `bindings[].tier` (§7). The lookup for an occurrence of tier T takes the binding of that event with the highest tier not above T. A binding without `tier` covers every tier and loses to any tiered binding that qualifies.
- If no binding qualifies, the occurrence is unmapped and the slot stays silent. A clip of a higher tier is never played for a lower one, and another event's clip never takes its place.
- A tier-less event (`first_blood`, `hero_kill` and the rest) accepts only untiered bindings.

**Alignment with the game (owner direction 2026-11-11).** The event set and the counting follow Dota 2 itself, not any package's source game.

- **Multi-kill is a chain.** An own kill no more than `window_ms` (18,000 ms) after the previous own kill extends the run. The window restarts at each kill, so a run can last longer than 18 s in total. An own death does not end the run. Only kills credited to the player count; assists do not.
- **Streak is the game's kill streak.** It counts own kills since the last own death and resets on death.
- **Timing source.** The game sends no multi-kill field. The run is built from increases of `player.kills`, timed by the monotonic arrival of the admitted payloads that carry them (GSI-INTAKE-SPEC §5). An increase of n in one payload counts as n kills at that arrival time.
- **No foreign events.** An event that exists only in another game (for example smackdown, humiliation, annihilation or genocide in a Heroes of Newerth pack) is not a catalog event. Such a clip cannot be bound and must not be mapped to a nearby Dota 2 event.

For package authors, the game's announcements per tier are:

| Event | Tier | Game announcement |
| --- | ---: | --- |
| `multi_kill` | 2 | Double Kill |
| `multi_kill` | 3 | Triple Kill |
| `multi_kill` | 4 | Ultra Kill |
| `multi_kill` | 5 | Rampage (five or more) |
| `streak` | 3 | Killing Spree |
| `streak` | 4 | Dominating |
| `streak` | 5 | Mega Kill |
| `streak` | 6 | Unstoppable |
| `streak` | 7 | Wicked Sick |
| `streak` | 8 | Monster Kill |
| `streak` | 9 | Godlike |
| `streak` | 10 | Beyond Godlike (ten or more) |

The names are reference only; the contract uses the numeric tier. The 18-second chain and tiers 3–7 and 9 were checked against public descriptions of the game on 2026-11-11. Tiers 8 and 10 follow the commonly cited ladder and were not confirmed by a source in that check.

**Arbitration.** Ordinary arbitration (MEDIA-002) runs once per admitted GSI tick:

1. Collect every voiced ordinary event the tick emitted.
2. Pick the highest rank.
3. Look up that event's mapping (by tier for a counted event).
4. If the mapping is missing or invalid, the slot stays silent. A lower-ranked event never takes its place.
5. Every non-selected event gets the outcome `lower_rank_same_tick`.

**Baseline and replay.** The first admitted payload of a session, or of a resumed session, only establishes baselines: kill and death counters, level and thresholds. It emits no achievement, death, level or match-start event (MEDIA-002 failure behavior; GSI-INTAKE-SPEC §6). Counter regressions, generation changes and late samples reset the affected baseline without emitting anything.

**Detector assumptions** (closed by the M0/M2 capture corpus, like GSI-INTAKE-SPEC §10):

| ID | Assumption | Check |
| --- | --- | --- |
| E-1 | `map.radiant_score` and `map.dire_score` (plus `player.kills`) let first blood be credited to self: the combined score goes from 0 to 1 in the same admitted tick as the own kill count goes from 0 to 1 | Record a bot match where the player draws first blood and one where another player does. If this fails, `first_blood` stays unavailable |
| E-2 | `map.game_state` transitions identify the start of live play and post-game | Record a full match. If this fails, `match_started` and `match_ended` stay unavailable |
| E-3 | `player.kill_streak` is the game's own count of kills since the last own death | Record a match with at least three own kills without dying, then a death. If confirmed, `streak` uses this field. If not, `streak` is computed from `player.kills` and the own death transition |
| E-4 | Own kills arrive as increases of `player.kills` soon enough that arrival times reproduce the game's 18-second chain | Record a double and a triple kill, and two kills more than 18 s apart; compare the detected run with the game's announcement. If this fails, `multi_kill` stays unavailable |


**Capture of 2026-11-11 (GSI-INTAKE-SPEC §10.1):**

- **E-2: confirmed.** Pre-game, live and post-game all appeared as `map.game_state` transitions. `map.win_team` changed from `none` to a team at post-game.
- **E-3, E-4: not tested.** `player.kill_streak` and `player.kills` are present in the captured payloads, but the minimized fixtures show a streak of 0 only and no multi-kill.
- **E-1: partly.** The negative case was observed: the combined score went 0→1 while the own kill count stayed 0, so the kill was not credited to self. The positive case (first blood by self) was not captured, so `first_blood` stays unavailable until it is.

## 3. Health danger (DEC-HEALTH-DANGER; approved 2026-11-11)

MP-01 names "health danger" as a native safety detector but gives no definition. The product owner approved the definition below **as proposed** on 2026-11-11. The catalog entry is `active`.

Approval does not make the cue audible by itself:

- It still needs verified, prewarmed critical media (ALERT-007, DEC-CRITICAL-MEDIA).
- Its behavior is pinned by the golden traces in [health-danger-golden.json](../../fixtures/health-danger-golden.json), which `tools/validate_spec.py` replays.

Trigger, re-arm and cooldown windows are inclusive (≤25%, ≥15 points, a [t−2,000 ms, t] window, ≥20,000 ms since the last cue). Percentages are `hp × 100 / max_hp`, not rounded.

| Parameter | Approved value | Meaning |
| --- | --- | --- |
| Precondition | Own source `fresh`, ownership `verified`, `alive = true`, HP and max HP observed | Never computed from stale or unknown values |
| Trigger | HP ≤ 25% of max HP **and** HP fell by ≥15 percentage points of max HP within the last 2,000 ms of admitted samples | Signals sudden burst danger, as opposed to a slow drain (which `health_low` covers) |
| Re-arm | HP ≥ 40% of max HP, death, or a new session | One cue per danger episode |
| Cooldown | 20,000 ms between cues | Bounds repetition during sustained fights |
| Category | critical | Uses the critical channel (§4) and needs verified critical media (ALERT-007) |

When `health_danger` and `health_low` fire in the same tick, the critical channel admits `health_danger`. The ordinary `health_low` follows the priority matrix: it is dropped with reason `critical_active`.

**M2 latency evidence.** QUALITY-001 acceptance in M2 uses admitted `health_danger` events, with T0 at the arrival of the GSI payload that triggered them. `latency_probe` stays available in test builds for preemption and engineering fixtures. Its measurements are **engineering evidence only** and never count toward QUALITY-001.

## 4. Audio priority and preemption (MP-02, MEDIA-005, VOICE-006, QUALITY-007)

Native audio owns one output path with five classes, highest first:

| Class | Members | Source of ordering |
| --- | --- | --- |
| C1 critical | `risk_alert`, `health_danger` | MP-02, ALERT-001 |
| C2 correction | `risk_revision` | Critical policy (FEATURE-CATALOG) |
| C3 notice | `enemy_missing` | ALERT-006: dropped while critical audio is active |
| C4 ordinary | Voiced ordinary events (§2) | MP-02 |
| C5 voice | Voice answers and advice speech (Normal) | VOICE-006: preemptible by the announcer and by critical audio |

Each row is the class of a new request (incoming); each column is the class already playing (active):

| Incoming \ active | idle | C1 | C2 | C3 | C4 | C5 |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | play | preempt if a different episode; otherwise drop `cooldown` | preempt | preempt | preempt | preempt |
| C2 | play | queue (latest only, TTL 3,000 ms) then play; drop `critical_active` on expiry | preempt (newer revision) | preempt | preempt | preempt |
| C3 | play | drop `critical_active` | drop `critical_active` | drop `channel_busy` | preempt | preempt |
| C4 | play | drop `critical_active` | drop `critical_active` | drop `channel_busy` | drop `channel_busy` | preempt |
| C5 | play | wait* | wait* | wait* | wait* | supersede (VOICE-003) |

\* **Wait** means the voice answer starts after the active output ends. The voice turn's own deadline and cancellation still apply. A voice answer never preempts event audio.

**Preemption** cancels both the audio and its paired visual tail under the same playback generation (MEDIA-005). It records `preempted` for the old playback and fences any late completion. C1 admission never takes a Voice, renderer, disk or network lock (ALERT-001).

**Prewarm.** All C1 and C2 media of the active revision are decoded to PCM in memory when the revision activates. Critical readiness is `ready` only when every **available** C1/C2 event has verified, prewarmed media (MP-08, ALERT-007). In M2 that is no events while `health_danger` is unapproved, so readiness reports `not_ready` with reason `MEDIA_UNMAPPED`. It never reports a vacuous "ready".

**Mute (acknowledged by the product owner on 2026-11-11).** The global mute, a single mute action, silences C1 through C5. Each silenced request records `muted`. While mute is on, `runtime-status.media` reports critical readiness as `unavailable` with reason `AUDIO_MUTED`, so the UI cannot show safety audio as ready. Acoustic acceptance runs unmuted. A separate "keep safety alerts audible while muted" option would be a new product feature and is not proposed here.

## 5. Outcome reasons (MP-02)

Detection is recorded as an event ([event.schema.json](../../contracts/event.schema.json) 2.0.0). Audio admission is recorded separately as an outcome ([audio-outcome.schema.json](../../contracts/audio-outcome.schema.json)), so history distinguishes "detected" from "heard".

| Outcome | Meaning |
| --- | --- |
| `admitted` | Playback started (`playback_id` set) |
| `preempted` | Playback started and was cancelled by a higher class (`playback_id` set) |
| `unmapped` | No mapping in the active revision |
| `invalid_media` | Mapping exists but its media failed verification or decode |
| `inert` | The event ID is unknown to this runtime's taxonomy (MP-05) |
| `not_voiced` | The catalog marks the event unvoiced (for example `hero_respawn`) |
| `detector_unavailable` | The event is registered but its detector is not available in this runtime |
| `stale_session` | The session or source generation changed before admission |
| `superseded_revision` | The active package revision changed before admission |
| `critical_active` | Dropped because C1/C2 audio is active |
| `channel_busy` | Dropped per the priority matrix |
| `lower_rank_same_tick` | Lost ordinary same-tick arbitration |
| `cooldown` | An episode or global cooldown is active |
| `muted` | Global mute is on |

## 6. Media format and decode checks (MP-03, MP-04, MEDIA-004)

| Asset | Accepted profile | Verified by |
| --- | --- | --- |
| Mixed audio | RIFF/WAVE, `WAVE_FORMAT_PCM` (1), mono, 48,000 Hz, 16-bit, a single `data` chunk, duration ≤15,000 ms | Native: full decode to PCM at install; samples × 1000 / 48,000 must equal the binding `duration_ms` (floor) |
| Identity (original) audio | Same profile | Native. When the binding names `identity_audio_path`, its sample count MUST equal the mixed audio's. It is never played in place of mixed audio |
| Silent visual | WebM (EBML/Matroska), exactly one video track with codec `V_VP9` or `V_VP8`, **no** audio track, ≤1920×1080, ≤60 fps | Native: container/track parse with no codec decode. Renderer: decode probe (below) |

**Video duration.** The video duration must lie between `duration_ms` and `duration_ms + visual_tail_ms`, with a tolerance of one frame.

**Renderer decode probe.** The renderer plays video (MP-04), so video decodability is proven by the component that decodes it, under native orchestration:

1. After the native checks pass for a staged revision, native hands each visual's bytes to the renderer's probe context. This context is not the gameplay overlay.
2. The probe context decodes the first frame and reads the duration. It returns `{install_id, revision, path_sha256, ok, duration_ms}`.
3. A missing, failed or timed-out probe (5,000 ms) leaves that binding `invalid_media`. The rest of the revision is unaffected.
4. The renderer never chooses or replays media; the probe has no playback role.

Activation does not wait for probes of mappings that are not critical. Critical readiness requires that probes of the C1/C2 visuals succeeded.

## 7. Package descriptor, registry and installation (MP-03, MP-06, MP-07, MEDIA-006..008)

**Package layout (format profile 1).**

- **Descriptor.** The descriptor is the first ZIP entry, `descriptor.json`. It is validated by [media-manifest.schema.json](../../contracts/media-manifest.schema.json) 1.2.0.
- **Inventory.** Its `files` array is the payload inventory.
- **Event registry.** Its `bindings` array is the event registry. The MP-03 elements are therefore sections of one bounded (≤2 MiB) document, not separate files.
- **Field naming.** `bindings[].event_id` names a catalog ID. It is the event *type*; the per-occurrence identifier in events is `instance_id`.
- **Tiers.** `bindings[].tier` (manifest 1.2.0) is optional and allowed only for an event with a catalog `tier_range`, inside that range. A descriptor holds at most one binding per `event_id` and tier, counting "no tier" as one. Violations reject the package before installation.
- **Container digest.** The SHA-256 of the whole container is computed by the installer. It is never self-declared inside the package.

**Authority is installer-bound.** The package carries no authority field. A `publisher_key_id` is only a claim. The installer records `authority_class` in the registry:

- Until DEC-PACK-CRYPTO closes, the only grantable value is `local_import`.
- Publisher and store authority remain unavailable.
- A package that claims a publisher is installed as `local_import` with its claim recorded as `unverified`.

Verified `local_import` revisions **may** be selected and played. Integrity is sufficient for local use (Decision Log, reconciled ambiguities) but never grants publisher or purchase authority (MEDIA-008).

**Install contracts.**

- [media-install-request.schema.json](../../contracts/media-install-request.schema.json) is the MP-06 request: request ID, profile ID, source path, expected container digest, optional project binding, and activation policy `never`, `now_if_idle` or `after_match`.
- [media-install-result.schema.json](../../contracts/media-install-result.schema.json) reports the transaction:
  - status: `installed`, `active`, `deferred`, `activation_error` or `rejected`;
  - the registry record: install ID, package ID, revision, container digest, authority class and publisher-claim state;
  - a reason code.
- Retrying the same `request_id` with the same digest returns the first result unchanged (idempotent).

The Windows transport framing (named pipe, 16 KiB frames, handshake) stays with DEC-INTERFACES.

## 8. Closure

- **DEC-EVENT-AUDIO** closes when:
  - E-1 and E-2 are confirmed from the capture corpus, or the affected events stay unavailable;
  - the catalog, contracts and fixtures are reviewed;
  - the product owner acknowledges the mute semantics in §4 (done: acknowledged 2026-11-11; recorded in contracts/decisions.json);
  - the product owner confirms the ordinary ranks not fixed by MP-02 (done: confirmed 2026-11-11).
- The tier dimension of `multi_kill` and `streak` was added on the owner's decision of 2026-11-11 (recorded in contracts/decisions.json). Its detector and lookup still need fixtures in M2.
- **DEC-HEALTH-DANGER** is **decided**: the owner approved §3 as proposed on 2026-11-11. The catalog status is `active`, and the golden traces (burst, slow drain, stale sample, death/cooldown, inclusive boundary) are in fixtures. Audible use still depends on verified critical media (DEC-CRITICAL-MEDIA).
