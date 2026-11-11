---
title: "Feature Catalog and Reference Algorithms"
doc_id: "FEATURE-CATALOG"
status: draft
version: "0.3.4"
updated: "2026-11-11"
authority: "DEC-BASELINE; see DOCUMENT-CONTROL"
owner_role: "Product and engineering owners; people not assigned"
approval_status: "not_approved"
---

# Feature Catalog and Reference Algorithms


## Capability inventory

| Functional module | Phase | Main dependency / exclusion |
| --- | --- | --- |
| Telemetry intake and session guard | MVP | Valid own-player source, no enemy feed |
| Vision/visibility monitor | MVP | Calibrated visible minimap and certified classes |
| Missing-time risk | MVP | Explicit heuristic; no hidden path prediction |
| Urgent alerts/correction | MVP | Native deterministic/audio path |
| Event announcer/media | MVP | Verified selected package and one audio owner |
| Overlay/resource governor | MVP | Native containment, focus and safe area |
| Local event history/summary | MVP | Source-complete evidence |
| Local voice conversation | MVP | PTT/mic consent, measured local STT/answer |
| Personal memory | MVP | Bounded local aggregates and durable forget |
| Strategic advice | MVP | Local by default; narrow separately consented cloud |
| Live match canvas (shared base field) | MVP | Honest read models and fixed map topology; premium inspector/timer details stay deferred |
| Peer party vitals | Explicit extension | Fresh consent, room/match/team and narrow DTO |
| Friends/invitations | Optional extension | Separate private account services |
| OBS local highlights | Explicit MVP slice | User-started external recorder, local clips |
| Local action damage | Tracked feature | Exact patch data/formulas/coverage not frozen here |
| Premium inspector/timers | Deferred authority integration | No widening of data/consent; native timer clock |
| Deep match review / recovery coach | Later | Separate requirements beyond ≤3-point summary |
| Multi-provider router / tone presets | Later | Never on alert path |
| Broadcast co-host / dynamic soundtrack | Later | Distinct privacy and media requirements |
| Store/wallet/payments/security recovery | Later service | Separate design/operational authorization |

## Missing-time heuristic

This is a transparent risk score, not a proven probability that an enemy will gank and not a trajectory estimate through fog. Only calculate it when its input coverage is valid. Track at most the last five minutes of admitted sightings.

Default parameters: ramp_start_s=5, peak_s=12, peak_risk=0.70, decay_per_s=0.03, floor=0.10, multi_boost=1.15, heading_amp=0.22.

For missing duration s seconds:

- s < 5: r=0.
- 5 ≤ s ≤12: r=((s−5)/7)×0.70.
- s>12: r=max(0.70−(s−12)×0.03,0.10).

If a valid pre-vanish heading exists, multiply by 1+clamp(cos(heading,toward_map_center),−1,1)×0.22, then clamp to [0,1]. If heading is unavailable use a neutral multiplier 1.0, not an invented vector.

Combine admitted contributors as 1−product(1−r_i). When at least two contributing missing heroes remain, multiply by 1.15 and cap at 1.0. Reference ETA for a contributing hero is max(12−s,1) seconds and must be labeled estimated. Aggregation of ETA, death discount and coordinated movement require the additional DEC-RISK-DETAIL fixtures before exact parity is claimed.

Golden examples with neutral heading/no death discount: s=4→0, s=5→0, s=8.5→0.35, s=12→0.70, s=20→0.46, s=40→0.10. Two heroes at s=12 yield min((1−0.3²)×1.15,1)=1.0. Score 1.0 is a heuristic saturation value, not certainty.

## Hysteresis

| Sensitivity | Necessary danger score | Necessary revision score |
| --- | --- | --- |
| Low | score≥0.85 | score<0.50 |
| Medium default | score≥0.65 | score<0.40 |
| High | score≥0.50 | score<0.30 |

These thresholds do not by themselves authorize audio. The audible missing-gank gate additionally requires admitted identity/roster and continuing coverage, supported live gameplay with the local hero alive and clock advancing, a fresh baseline after observation gaps, and verified critical-media readiness. Its movement evidence must establish multiple enemies moving and disappearing near one another in time/space, or a separately specified independently verified positional-evidence route. Missing count and the numeric risk score alone are insufficient; neither establishes movement toward the player.

Use one alert per admitted absence episode, a 30-second global cooldown and a 60-second cooldown for every contributing hero. Any contributor already notified for its current absence suppresses a new alert. Cooldown expiry does not re-arm an episode; confirmed continuous reappearance does. Gaps and toggles do not reset cooldowns. The aggregate risk multiplier above is not a replacement for coordinated-motion admission.

A spoken revision requires all previously alerted heroes to have left the admitted missing set through confirmed reappearance and the risk to be below the revision threshold. Natural time decay below that threshold does not produce a revision. Loss of coverage/session suspends silently and displays unavailable, never all-clear. See the full state guards in docs/design/SDD.md.

The exact motion-admission parameters and traces remain in DEC-RISK-DETAIL; until frozen, expose only the reference score and keep audible missing-gank admission unavailable. Evaluation is event-driven. A 500ms watchdog can detect stale data, but it cannot define the start of acoustic latency or justify polling that misses the deadline.

## Visibility missing notification

Require a previously observed enemy, valid continuing coverage and absence strictly longer than 5 seconds. Emit once per hero per absence interval; re-arm on admitted reappearance. Global missing-notification audio cooldown ≥10 seconds. Drop that audio when critical audio is active. Optional missing-event media absence leaves a truthful UI chip/local reason, not substituted speech.

## Event semantics

One catalog owns event ID, label, category, detector reference, tunables and ordinary voice rank. Detector references describe native code ownership; they are not arbitrary executable rules. Multi-kill and since-death streak are distinct counters. Reconnection/baseline establishment must not synthesize match-start/kill/death/level events.

Default tunables: multi-kill window 18 seconds, low health 20%, low mana 15%, level milestones 6/12/18/25. Respawn remains a detected transition and is intentionally unvoiced in this target. Ordinary same-tick arbitration selects the highest emitted ordinary rank before media lookup. If that event lacks a valid mapping, it stays silent; do not select a lower-ranked mapped event as fallback. Critical/correction/missing notifications have separate policy.

The full approved event catalog and golden multi-event traces must be frozen during DEC-INTERFACES. Do not derive missing event semantics merely from a UI label.

## Optional next-session tuning

Any local tuning profile must be validated, bounded and linked to evidence. It may become active only at a new match boundary, never mutate the current match's heuristic. If adopted, require at least 3 fully evidenced matches, medium-sensitivity F1 improvement ≥0.01 against the baseline, finite/range/order checks, immutable inputs and backup/default fallback. No automatic cloud learning or wider data collection is implied.
