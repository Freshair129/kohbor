---
title: "Event and Audio Specification (M2)"
doc_id: "EVENT-AUDIO-SPEC"
status: draft
version: "0.3.7"
updated: "2026-11-20"
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
| `streak` | ordinary | gsi | M2 | 80 | yes | The game's kill streak (own kills since the last own death) reaches 3 or more; each further kill re-emits. Tier 3–10. Uses `player.kill_streak` when present (E-3 confirmed to tier 4), otherwise a count from `player.kills` and own deaths |
| `first_blood` | ordinary | gsi | M2 | 70 | yes | First hero kill of the match, credited to self (E-1) |
| `hero_kill` | ordinary | gsi | M2 | 60 | yes | Own kill counter increments |
| `hero_death` | ordinary | gsi | M2 | 50 | yes | Own `player.deaths` counter increases. If the counter is absent, `alive` going from true to false is the fallback. `alive` alone is not a death (E-5) |
| `level_milestone` | ordinary | gsi | M2 | 40 | yes | Level reaches 6, 12, 18 or 25 |
| `health_low` | ordinary | gsi | M2 | 30 | yes | HP falls to ≤20% of max HP; re-arms at ≥30% or after respawn |
| `mana_low` | ordinary | gsi | M2 | 20 | yes | Mana falls to ≤15%; re-arms at ≥25% or after respawn |
| `match_ended` | ordinary | gsi | M2 | 15 | yes | Game state enters post-game (E-2) |
| `match_started` | ordinary | gsi | M2 | 10 | yes | Game state enters live play for the current session (E-2) |
| `hero_respawn` | ordinary | gsi | M2 | — | **no** | Own `alive` goes from false to true after a counted death. Detected and recorded; not voiced and not mappable in taxonomy 1.x (MEDIA-003) |
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

`Store::resolve_variants` uses the same exact event and tier lookup as `Store::resolve` (§7.1) and reports each variant's verification outcome in descriptor order. The host applies the tier fallback above and selects only among verified, prewarmed variants of the resolved `(event_id, bound tier)` binding. `random` uses the declared weight (default 1); `shuffle` chooses the least-recently played ready variant and randomizes ties, without an immediate repeat when at least two are ready; `sequence` follows descriptor order and wraps. A single variant always repeats. History is keyed by the event and resolved bound tier, so one tier-less binding shares history across requested tiers, and resets when the session epoch advances. The engine's small PRNG is seeded from the existing OS RNG; selector tests use fixed seeds.

Audition has no variant-id input. Until that API is extended, it plays the first ready variant in descriptor order and does not advance match selection history.

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
| E-5 | `player.deaths` increases once per own death, and `hero.alive` can be false without a death | Replay full matches and compare the counter with `alive` transitions. If the counter is absent or wrong, the `alive` fallback applies |


**Capture of 2026-11-11 (GSI-INTAKE-SPEC §10.1):**

- **E-2: confirmed.** Pre-game, live and post-game all appeared as `map.game_state` transitions. `map.win_team` changed from `none` to a team at post-game.
- **E-3, E-4: not tested.** `player.kill_streak` and `player.kills` are present in the captured payloads, but the minimized fixtures show a streak of 0 only and no multi-kill.
- **E-1: partly.** The negative case was observed: the combined score went 0→1 while the own kill count stayed 0, so the kill was not credited to self. The positive case (first blood by self) was not captured, so `first_blood` stays unavailable until it is.

**Owner collector sessions of 2026-11-02 and 2026-11-03 (six Mid-lane matches, replayed 2026-11-12).** The owner's loopback collector recorded each payload body with a monotonic arrival time (116,960 admitted payloads in total). The replay tool `kohbor-replay` ran them through the production admission and the detector. The raw files hold a token and real identifiers, stay on the owner's machine and are not part of this package. Evidence class: synthetic replay of real recorded payloads; it is not a live run of the application.

- **Kill timing, E-4 (partly confirmed).** In the one match that has a replay-file (.dem) analysis by the owner, the detector's 11 own kills matched the 11 analysed kills at the same payload sequence, with no extra and no missing kill. Four double kills occurred across the sessions and each was reported as `multi_kill` tier 2. The two pairs that the analysis places six seconds apart are among them. The game's own announcements were not recorded, so the match is by game clock and kill count. Not covered: a triple kill, tiers 3–5, and a gap near the 18,000 ms boundary.
- **Streak, E-3 (confirmed to tier 4).** `player.kill_streak` equalled the count of own kills since the last death at every compared payload (116,960 comparisons, 0 differences) once deaths were taken from `player.deaths`. The largest streak seen was 4. Tiers 5–10 are not covered. The detector therefore uses the field.
- **Deaths, E-5 (new).** `player.deaths` increased 37 times and each increase is one death. `hero.alive` also went false three times without a death (one of them for about six seconds), and the counter sometimes rose one payload before `alive` turned false. An alive-based death rule produced one false `hero_death` and reset a live streak. The detector now counts deaths from the counter and uses `alive` only as a fallback and for respawn.
- **First blood, E-1 (still open).** No session shows first blood credited to self. `first_blood` stays unavailable and the host reports it so in the runtime status (`events.unavailable`, contract 1.1.0).

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
| C4 | play | queue (one slot, higher rank wins, TTL 3,000 ms) then play; drop `critical_active` on expiry or replacement | queue (one slot, higher rank wins, TTL 3,000 ms) then play; drop `critical_active` on expiry or replacement | drop `channel_busy` | strictly higher rank: preempt with fade†; equal or lower rank: drop `channel_busy` | preempt |
| C5 | play | wait* | wait* | wait* | wait* | supersede (VOICE-003) |

† **Fade (owner decision 2026-11-23).** When the active output is an ordinary (C4) clip and a request preempts it (a strictly higher-rank C4, or C1, C2 or C3), the new clip starts at once and the old clip fades out under it: a linear gain ramp from 1 to 0 over 100 ms (4,800 mono samples at 48 kHz, or what remains of the clip), summed with i16 saturation. This is the only mix the sink performs; it never delays the new clip, so a critical cue keeps its deadline. The faded clip ends `preempted` (never `finished`). Each fade preemption writes one `audio/clip_preempted {from_event, to_event, fade_ms}` info line; drops still write `audio/clip_dropped`. Every other preemption is a hard cut as before.

\* **Wait** means the voice answer starts after the active output ends. The voice turn's own deadline and cancellation still apply. A voice answer never preempts event audio.

**Preemption** cancels both the audio and its paired visual tail under the same playback generation (MEDIA-005). It records `preempted` for the old playback and fences any late completion. C1 admission never takes a Voice, renderer, disk or network lock (ALERT-001).

If both queues wait, C2 starts before C4. C4 keeps its original enqueue time and TTL while C2 plays. Equal C4 ranks keep the newer request. The owner checks TTL at the time it processes the queue, even if the previous audio ended earlier. C1 admission never starts a queued request. Mute, session or revision changes, device loss and stop clear waiting requests with the applicable outcome without submitting them.

**Prewarm.** The media worker prewarms critical clips first, then the first variant of every other binding, then additional variants in descriptor order as they fit the 40-million-sample PCM budget. Shared audio paths reuse one PCM allocation. A variant that exceeds the remaining budget is excluded individually and recorded once per table build as `audio/clip_variant_skipped` with reason `prewarm_cap`. File reads and decoding stay on the media worker. The engine skips a variant when its payload fails verification, reading, decoding or visual indexing. It reports `invalid_media` if no variant in the binding remains playable. Critical readiness is `ready` only when every **available** C1/C2 event has verified, prewarmed media (MP-08, ALERT-007). V1 admission requires exactly one critical variant, so critical selection and probe behavior remain unchanged. In M2 that is no events while `health_danger` is unapproved, so readiness reports `not_ready` with reason `MEDIA_UNMAPPED`. It never reports a vacuous "ready".

**Local variant diagnostics.** `audio/clip_played` records `variant_id`, `clip_id` when present, `selection` (null for a single or legacy binding), and `has_visual` for the selected clip. `audio/clip_queued` and `audio/clip_dropped` record variant provenance when selection had already identified a variant. `audio/clip_variant_invalid` records the event, bound tier, variant ID, optional clip ID and reason once per skipped variant per session when the binding is requested. These fields contain no media paths or clip contents.

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
| `critical_active` | Dropped by C1/C2 policy, or because a queued C2/C4 request expired or was replaced |
| `channel_busy` | Dropped per the priority matrix |
| `lower_rank_same_tick` | Lost ordinary same-tick arbitration |
| `cooldown` | An episode or global cooldown is active |
| `muted` | Global mute is on |

### Local match recap (H12, D1)

`audio-outcome.schema.json` 1.1.0 adds `tier`, `variant_id`, `clip_id`, `selection`, `game_clock_seconds`, `match_id` and `playback_end`. These fields are present on announcer event outcomes, including dropped outcomes; unavailable provenance and game time are `null`. `game_clock_seconds` is copied only from a non-negative local GSI `map.clock_time`. The local match key is `local_match_<session_epoch>`: it is derived from the persisted local session epoch, never from the raw GSI `match_id`, and no network service is used.

An admitted outcome starts with `playback_end: null`; the host updates the same outcome by `playback_id` to `finished`, `cut` or `failed` when audio actually finishes, is interrupted, or loses its device. A dropped outcome has no playback and keeps `playback_end: null`. `preempted` is a played outcome whose end is `cut`. Voice and audition records are not part of match recap.

The host persists a schema-versioned `match-recap.json` in the application data directory. It retains at most five match groups, at most 128 outcomes per match and at most 1 MiB on disk. Writes use a same-directory temporary file and atomic replacement. A missing file starts empty; a corrupt or unsupported file starts empty and produces one warning at load. `match_recap` reads the local recap and `clear_match_recap` replaces it with an empty document. Neither command sends data outside this machine. D1 authorizes the five-match local retention and clear action.

The Packs recap shows admitted/preempted announcer events with their tier/variant provenance and playback end. Dropped outcomes remain available in the local outcome record but are not listed as played. The page uses the owner wording “เล่นแล้ว” and one footnote explaining that admission does not prove the player heard it, and that a cut or failed playback did not finish normally.

### Audition lifecycle result (H5)

`media_audition` returns a caller-visible result with `audio_started`, `audio_ended`, and `audio_end_reason`, plus `visual_status`, `visual_reason`, and `video_error_code`. `media_audition_variant(event_id, tier, variant_id)` uses the same lifecycle result while selecting the named variant exactly. These active-pack commands accept `overlay: boolean`, default false; callers opt in to the existing in-game overlay behavior only for the active pack.

**Drawer preview (owner decision 2026-11-20; D10 superseded for this case).** `media_audition_installed_variant(install_id, event_id, tier, variant_id)` auditions the exact variant from an installed but non-active pack. The host resolves and verifies that installed revision, fully decodes its audio, sends the audio through the app preview path, and always removes its visual URL and visual tail before submitting to the audio engine. It never routes drawer-preview media through the in-game overlay. The command returns a reason code while a match is active and the host rate-limits audition requests. `media_variant_visual_url(install_id, event_id, tier, variant_id)` separately returns `{"visual_url": null}` or the exact `kohbor-media://localhost/v1/<install_id>/<sha256>` route after the host verifies the same variant and publishes it in the existing protocol allowlist; no path is returned. A match blocks this command too.

The existing `playback://event` lifecycle emits `started` and `ended` records with the playback ID. The drawer calls the visual URL command first, starts its muted panel video when it observes the matching audition `started` event, and stops it on the corresponding `ended` event. The event contains the host monotonic `at_ms`, exact `playback_id`, and audio duration. The panel's visual lifecycle is separate from overlay diagnostics: it does not report a drawer video as shown based on the in-game overlay.

`visual_status` is `shown` only after the overlay video emits `playing`, `failed` on a video error, unusable visual URL, or five-second start timeout, `none` for an audio-only active-pack variant, and `unknown` when the host cannot confirm the expected overlay lifecycle. The host logs the same bounded result as `media/audition_result`; it does not claim that audio was physically heard. Owner decision 2026-11-20 reverses D10 only for drawer previews of non-active packs; it does not authorize non-active playback on the game overlay.

## 6. Media format and decode checks (MP-03, MP-04, MEDIA-004)

| Asset | Accepted profile | Verified by |
| --- | --- | --- |
| Mixed audio | RIFF/WAVE, `WAVE_FORMAT_PCM` (1), mono, 48,000 Hz, 16-bit, a single `data` chunk, duration ≤15,000 ms | Native: full decode to PCM at install; samples × 1000 / 48,000 must equal the binding `duration_ms` (floor) |
| Identity (original) audio | Same profile | Native. When the binding names `identity_audio_path`, its sample count MUST equal the mixed audio's. It is never played in place of mixed audio |
| Silent visual (optional per binding) | WebM (EBML/Matroska), exactly one video track with codec `V_VP9` or `V_VP8`, **no** audio track, ≤1920×1080, ≤60 fps | Native: container/track parse with no codec decode. Renderer: decode probe (below) |

**Video duration.** For a binding that has a video, the video duration must lie between `duration_ms` and `duration_ms + visual_tail_ms`, with a tolerance of one frame.

**Renderer decode probe.** The renderer plays video (MP-04), so video decodability is proven by the component that decodes it, under native orchestration:

1. After the native checks pass for a staged revision, native hands each visual's bytes to the renderer's probe context. This context is not the gameplay overlay.
2. The probe context decodes the first frame and reads the duration. It returns `{install_id, revision, path_sha256, ok, duration_ms}`.
3. A missing, failed or timed-out probe (5,000 ms) leaves that binding `invalid_media`. The rest of the revision is unaffected.
4. The renderer never chooses or replays media; the probe has no playback role.

Activation does not wait for probes of mappings that are not critical. Critical readiness requires that probes of the C1/C2 visuals succeeded.

## 7. Package descriptor, registry and installation (MP-03, MP-06, MP-07, MEDIA-006..008)

**Package layout (format profile 1).**

- **Descriptor.** The descriptor is the first ZIP entry, `descriptor.json`. The established reader accepts 1.3.0 and 1.4.0; 1.3.0 makes `silent_visual_path` optional (an audio-only binding plays audio and shows no banner; DEC-PACK-AUDIO-OPTIONAL-VISUAL). Proposed 1.4.1 adds optional per-variant `caption` (trimmed plain text of 1 to 200 Unicode scalar values with no control characters or line breaks), preserves caption-free 1.3.0 and 1.4.0 packs, and requires owner approval before integration. Authors should supply NFC text. Captions are display text only and are not proof of spoken content.
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

### 7.1 Implementation notes (Sprint 2, proposed, not owner-approved)

`crates/kohbor-pack` implements §7 for hash-verified local packages. The choices below are the lead's proposals and stay open until the owner acknowledges them; the code and tests follow them.

- **Install and activation are two calls.** `Store::install` returns `installed` or `rejected` and never `active`; `Store::activate` and `Store::resume_deferred` return `active`, `deferred` (reason `ACTIVATION_DEFERRED`) or `activation_error`. A retry of an install never re-runs activation.
- **First activation is never deferred (owner decision, 2026-11-19).** A match or a playback lease defers only a switch away from an existing active revision, which stays usable (AC-MEDIA-007). With no active revision there is nothing to keep usable. So `NowIfIdle` and `AfterMatch` activate at once even while a match is active, and the host swaps the new table into the engine immediately. Before this rule, a pack installed mid-match stayed silent for the whole match.
- **Reasons.** `DISK_PRESSURE` is used for any recoverable write failure (install or activation); `SOURCE_INVALID` for a digest mismatch and for a reused request ID with different content (`RequestConflict`); `MEDIA_INVALID` for every other rejection. The specific local reason (`RejectReason`) is kept apart from the contract code.
- **Idempotency window.** Request results are kept for the last 512 requests; an evicted request ID is treated as new (an already-installed identity still returns `installed` with its record). Records are never evicted.
- **Tier lookup.** The store resolves exactly: `resolve(event, Some(n))` returns only the binding with tier `n` and `resolve(event, None)` only the tier-less binding. The host applies the owner's rule of section 2 in its prewarm table (S3-5-HOST-CRITICAL-PATH, design P6): the highest bound tier not above the occurrence, else the tier-less binding; a chosen clip that is invalid is reported as `invalid_media` and never replaced by a weaker tier.
- **Unknown or non-mappable event IDs are inert** for packages of the same major taxonomy (older, equal or newer minor); they are verified like the rest and never played. A different major is rejected (`VERSION_UNSUPPORTED`).
- **Names.** A container or descriptor path is `UnsafePath` for a `..`/`.` segment, a segment ending in a dot or space, a `:`, a Windows device stem (CON, PRN, AUX, NUL, COM1–9, LPT1–9) or a segment over 255 bytes; duplicates are compared case-insensitively.
- **Durability.** Extracted files, the staged directory tree, the revision directory, the temporary registry/pointer files and their parent directories are flushed before each commit rename. Crash tests inject at step boundaries only; power loss and storage-cache failure are **not** demonstrated (native evidence needed).
- **No store lock.** The store assumes one writer. The host's single-instance guard provides it; an authoring tool must not write into a live store.
- **Parked.** Actual frame rate, frame size and duration of the video are proven by the renderer decode probe (§6) and the critical-readiness gate, not by the native WebM parse; the parse checks declared dimensions, track list and, when present, `DefaultDuration` and segment duration.

## 8. Closure

- **DEC-EVENT-AUDIO** closes when:
  - E-1 and E-2 are confirmed from the capture corpus, or the affected events stay unavailable (state on 2026-11-12: E-2 confirmed, E-3 confirmed to tier 4, E-4 confirmed for single and double kills, E-5 confirmed; E-1 open, so `first_blood` is reported unavailable);
  - the catalog, contracts and fixtures are reviewed;
  - the product owner acknowledges the mute semantics in §4 (done: acknowledged 2026-11-11; recorded in contracts/decisions.json);
  - the product owner confirms the ordinary ranks not fixed by MP-02 (done: confirmed 2026-11-11).
- The tier dimension of `multi_kill` and `streak` was added on the owner's decision of 2026-11-11 (recorded in contracts/decisions.json). Its detector and lookup still need fixtures in M2.
- **DEC-HEALTH-DANGER** is **decided**: the owner approved §3 as proposed on 2026-11-11. The catalog status is `active`, and the golden traces (burst, slow drain, stale sample, death/cooldown, inclusive boundary) are in fixtures. Audible use still depends on verified critical media (DEC-CRITICAL-MEDIA).

## Change history

- **2026-11-20 (P5, owner approved audio-outcome 1.1.0):** local last-five-match recap, local epoch-derived match keys, game-clock provenance and final playback status. The integration lead relayed the contract approval.
- **2026-11-19 (owner decision, “Queue and log”):** C4 requests during C1/C2 use one rank-prioritized queue slot with a 3,000 ms TTL; expiry or replacement records `critical_active`. Event drop and queue logging are specified without adding an outcome reason.
