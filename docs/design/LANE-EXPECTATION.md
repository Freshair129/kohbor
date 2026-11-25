# Lane expectation and review slices

Status: implemented source candidate; owner decision recorded 2026-11-25. The owner-approved baseline is DEC-LANE-EXPECTATION, with Analytics v2 from DEC-ANALYTICS-V2. Source and synthetic checks do not establish native or live-match acceptance. The shipped coach table starts empty; the Huskar illustration in the source proposal is hypothetical and is not data.

## Behavior and sources

The existing raw lane result remains unchanged. Analytics adds a distinct lane score and, when supported by reviewed evidence, an expected score with uncertainty, named sources and a verdict. Unknown values remain null with a reason. When no reviewed expectation is usable, the lane shows “ยังไม่มีค่าคาดหมาย” and the raw result only. The player's own average is shown separately as “ค่าเฉลี่ยของคุณ” and never contributes to the expectation.

Expectations use a structural role baseline and owner-reviewed expert rows. The coach authors rows; the owner reviews every patch. Only rows with an owner review are usable. Attribution contains role labels (“coach”, “owner”), never real names. There is no public fetch and no AI-generated expectation. A future source study is L6 and requires the owner's word. If that study leads to a fetch, it uses the already-confirmed DEC-PICK-STATS consent and privacy rules.

The single shipped constants file is `apps/desktop/src-tauri/src/analytics/data/lane-expectation-constants.json`. Rust includes the exact bytes and reports a SHA-256 hash with its version. Runtime calculation reads constants from that file; calibration writes a separate report and proposed v1 hash, never the constants file. Version 0 remains provisional. After the single calibration on at least 30 lanes, a v1 can be frozen; later changes require a recorded decision. All readings keep “ทดลอง · เกณฑ์ชั่วคราว” until R1, R2 and R3 pass. This implementation does not pass them.

## Score and unknown states

The score combines relative net worth, XP, last hits plus weighted denies, hero-attributed lane deaths and tier-one tower health at the approved boundary. Net worth and XP are required. Missing optional components are omitted and the remaining weights are renormalized, with the used components listed. Deaths before the horn are excluded; only deaths with a hero killer are counted. Tower health is zero only when the source reports zero. Standard modes use the approved ten-minute boundary. Turbo and unlisted modes have no boundary until measured and confirmed.

Supported expectation shapes are restricted to the implemented lane assignment and reviewed roles. Unsupported shapes, unavailable score components, unknown roles, an invalid table, stale rows and excessive uncertainty produce null expectation and a reason. The matcher does not guess.

## Match result and AI context

Match result is separate from lane analysis. Sources resolve in this order: replay engine, local GSI history joined by the keyed match hash, the user's own OpenDota history only when enabled, then unknown. A disagreement returns unknown and logs one warning. Abandoned or unjoined history stays unknown. Lane output is independent of match result.

The host accepts the replay-engine winner only from bundle schema 3.1; schema 3.0 stays eligible for the existing local-history fallback and otherwise reports `re-parse with engine 0.3.0 to get this`. A 3.1 null winner keeps the engine's `winner_reason` when no history source resolves it. The K0-K2 digest records each result source and the engine source for draft, true GPM and attacker-side hero damage; schema 3.0 keeps draft, true GPM and hero damage unknown with the same re-parse reason.

The digest sent to Analytics AI includes the raw result, score and available components, expectation and uncertainty when known, verdict, source, and match-result source/state. Consent names these added fields and has text version 3. The assistant cites the expectation source, never invents an expectation, and never states a match result when the digest marks it unknown.

## Persistence and tools

The local lane store holds at most 1,500 hero-level lane rows across all three lanes. It stores no player names or IDs, match identifiers or paths. Writes are atomic; corrupt data is unavailable rather than fabricated. Clear-history deletes the store and the calibration report. Own-lane records support only the separate personal average and are excluded from expectation baselines.

L4 reads local replay bundles, refuses fewer than 30 eligible lanes, records bundle hashes in its calibration report and emits a proposed constants v1 hash. It never edits the shipped constants table. L5 presents a blind coach tier, then raw values without the expectation, then reveals the model result. It exports the constants version/hash and bundle hashes only after every selected lane is reviewed, without personal data.

## Frozen exit criteria

- **R1:** Calibration on at least 30 lanes.
- **R2:** At least 15 blind coach reviews; at least 70% same-verdict agreement and no opposite-direction pair.
- **R3:** No unexplained case where score sign contradicts the coach's reading of the raw values.

These criteria are not reduced after results are seen. The experimental label remains until all three pass under a reviewed evidence record.

## Slice and test coverage

| Slice | Implementation | Required test focus |
| --- | --- | --- |
| L0 | Engine/GSI/OpenDota/unknown result resolution and conflict warning | Priority, win/loss, abandoned, mismatch, conflicts, no raw match ID |
| L1 | Score, one constants table/hash, boundary and null semantics | Synthetic golden score, optional renormalization, required nulls, Turbo unknown, pre-horn deaths, result invariance; opt-in sanitized bundle values +12.3, -7.3 and -19.9 |
| L2 | Reviewed expert table, expectation, verdicts, lane card, matchup strip and AI rules | Unreviewed row rejection, validation, precedence, role weighting, clamp, patch staleness, shape and uncertainty gates, verdict cuts, hostile reason text, no-expectation fallback, AI source snapshot |
| L3 | Bounded local history and clear-history deletion | Three lanes, 1,500 cap, no person/match identifiers, atomic writes, corrupt file, own-lane exclusion, sample minimum and clear |
| L4 | Local calibration report and proposed constants | Deterministic output, fewer than 30 refused, bundle hashes present, constants file unchanged |
| L5 | Blind coach review and export | Stage order enforced, all rows required, constants version/hash and bundle hashes included, no personal data |

Public matchup-source work (L6) and the later overview tile (L7) are outside this implementation.
