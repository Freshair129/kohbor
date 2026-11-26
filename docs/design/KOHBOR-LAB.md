---
title: "Kohbor Lab: dev-stage event-logic studio (design)"
doc_id: "KOHBOR-LAB"
status: draft
version: "0.1.1"
updated: "2026-11-24"
authority: "Agent design proposal for owner review; LAB-001 follows the owner instruction recorded 2026-11-24. Changes no unrelated requirement, contract or threshold."
approval_status: "agent_decision_not_owner_approved"
---

# Kohbor Lab: dev-stage event-logic studio

## Owner decision record (2026-11-15, relayed by the coordinator session)

Owner statement (Thai, as relayed): "เปิดให้ใช้เฉพาะเดฟก่อน แล้วเริ่มเลย".

- **DEC-LAB-GATE: decided.** Kohbor Lab is dev-only (demo/debug gating, absent from release builds), and building starts now with stage L1, then L2 if L1 is green.
- **DEC-LAB-LOGIC: not decided.** User-composed conditions (tier 2) stay out; implementation is tier 1 (parameters) only.
- This is recorded here only. `contracts/decisions.json` and the DECISION-LOG are owner-reserved and are **not** edited; adding DEC-LAB-GATE there is a proposal for the owner (§7.1, item 5).
- v1 profile scope narrowed by implementation (see §10): `events.<id>.params` only. The `enabled`, `condition`, `arming` and `bindings` fields of §2.2 are deferred (bindings to L3, others to tier 2).

## 0. Purpose, scope, evidence

The owner cannot currently see how admitted GSI values become events (for example why `health_danger` fires), cannot change that logic without editing Rust, and cannot map events to announcer clips in one place. Kohbor Lab is a **development-stage view inside the desktop app** that makes the mapping visible, lets the owner edit bounded parameters live, pairs events with clips, and exports the whole configuration. The same defaults ship embedded in production, where no external file is read.

Evidence classes used in this document: **CODE** (read in this checkout on 2026-11-15), **DOC** (stated in a spec, not re-verified in a run), **PROPOSED**, **UNVERIFIED**. Nothing here has been built or run; no Lab behaviour is claimed to work.

Constraints inherited (AGENTS.md, MEDIA-PACK-SPEC): owner-reserved files (`contracts/**`, `AGENTS.md`, `docs/delivery/AGENT-WORKFLOW.md`) are not edited here; no cloud/telemetry; announcer clips are never downloaded; no fps/performance work in MVP; no G-Maiden code is reused. From G-AnnStudio (`EventTestGrid`, `PackEditor`, `EventPicker`) only UX concepts are used (a grid of event cards with a per-card test button, a per-event clip picker, an editor/test split). No code or data structures were copied.

## 1. What exists today (CODE)

| Fact | Where |
|---|---|
| The detector is pure logic: one `Tick` (kills, deaths, kill_streak, combined_score, level, hp, max_hp, mana_percent, alive, phase, trusted, epoch) in, `Vec<Event>` out. It reads no clock or file. | `crates/kohbor-events/src/lib.rs` |
| 12 event ids exist in `EventId`; `contracts/event-catalog.json` registers 18 (the other 6: `enemy_missing`, `risk_alert`, `risk_revision`, `risk_unavailable`, `clip_requested`, `latency_probe`). | lib.rs, event-catalog.json |
| `Config` has 13 tunables with catalog defaults and `Detector::with_config(Config)` exists. **No production caller passes a non-default config**: the host uses `Detector::new()` (intake.rs:381, :514) and so does `kohbor-replay`. So today every threshold is effectively compiled in. | lib.rs:139, intake.rs |
| Catalog `tunables` carry `default/min/max/unit` for 9 events' parameters and map 1:1 onto `Config` fields (e.g. `health_low.threshold_percent` ↔ `health_low_threshold_percent`). Nothing in code enforces min/max on `Config`; UNVERIFIED whether a test checks the mapping. | event-catalog.json |
| Event rank, tier range, voiced, critical flags are `match` arms on `EventId`, not data. | lib.rs:49-82 |
| `kohbor-risk` rules (`MIN_ABSENCE_MS`, `ALERT_COOLDOWN_MS`, HP meter 50/30, median N=5, …) are `pub const`s tagged PROPOSED; not configurable. | kohbor-risk/src/lib.rs:7-48 |
| MEDIA-PACK-SPEC MP-01: "Detector references identify reviewed native logic; they are not executable expressions or a runtime rule engine." | MEDIA-PACK-SPEC.md |
| Demo gating: `#[cfg(any(feature="demo", debug_assertions))]` plus env `KOHBOR_DEMO=1`; data-dir override honoured only together with the demo flag; release builds ignore it. | lib.rs:70-88, demo.rs, S6-OVERLAY-SLICE §9 |
| GSI keys actually read into the own snapshot: `player.{activity,steamid,team_name,kills,deaths,kill_streak,assists,…}`, `map.{radiant_score,dire_score,game_state}`, `hero.{name,level,health,max_health,mana_percent,alive,…}`, `items`, `abilities` (data blocks `provider,map,player,hero,abilities,items`). The detector consumes a subset, listed in §2.1. | gsi config.rs:11, admission.rs, own-snapshot.schema.json |
| Offline replay exists: `tools/gsi_replay.py` replays a recorded session from `F:\gsi-log` into a demo instance; `kohbor-replay` (bin) feeds a recording through admission + detector + arbitrate. | tools/, kohbor-events/src/bin |

**Tension to resolve (owner decision DEC-LAB-LOGIC):** the owner wants to "define which GSI keys combine, with which operators". MP-01 says the runtime is not a rule engine. This design therefore keeps expressiveness in three tiers (§2.3); stages 1-4 stay in tier 1, which needs no change to MP-01.

## 2. Logic configuration data model (PROPOSED)

The file is called a **Lab profile**. It is separate from `contracts/` (it is not a cross-boundary contract); its schema lives at `crates/kohbor-lab/schema/lab-profile.schema.json` (proposed new crate, see §3) and the embedded default at `crates/kohbor-lab/default-profile.json`.

### 2.1 Signals (read-only, defined by the code, not by the file)

A signal is a named, typed view of one admitted field, with unknown as a first-class state (never zero).

| Signal id | Source | Type | Used by (CODE) |
|---|---|---|---|
| `gsi.hero.hp` / `max_hp` | `hero.health`, `hero.max_health` | u32 \| unknown | health_low, health_danger |
| `derived.hp_percent` | hp·100/max_hp, valid only if max_hp>0 and hp≤max_hp | % \| unknown | health_low, health_danger |
| `gsi.hero.mana_percent` | `hero.mana_percent` | % \| unknown | mana_low |
| `gsi.hero.alive` | `hero.alive` | bool \| unknown | health_*, mana_low, respawn |
| `gsi.hero.level` | `hero.level` | u32 | level_milestone |
| `gsi.player.kills/deaths/kill_streak` | `player.*` | u32 | kill, death, streak, multi_kill |
| `gsi.map.combined_score` | radiant+dire score | u32 | first_blood |
| `gsi.map.phase` | `map.game_state` | enum | match_started/ended |
| `vision.*`, `capture.*` (later) | CV/minimap (kohbor-vision, kohbor-minimap-cv) | counts | enemy_missing, risk_alert; **not in Lab stages 1-4** |

The signal list ships in code and in the exported file (`signals[]`, informational) so the file documents itself; the loader ignores edits to it.

### 2.2 Profile schema (draft 2020-12, abridged)

```json
{
  "$id": "kohbor-lab-profile/1",
  "type": "object",
  "required": ["profile_version", "taxonomy_version", "events"],
  "additionalProperties": false,
  "properties": {
    "profile_version": { "const": "1.0" },
    "taxonomy_version": { "type": "string", "pattern": "^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$" },
    "base": { "enum": ["embedded"], "description": "overrides are deltas on the embedded default" },
    "events": {
      "type": "object",
      "propertyNames": { "pattern": "^[a-z][a-z0-9_]{1,40}$" },
      "additionalProperties": { "$ref": "#/$defs/event" }
    },
    "bindings": { "$ref": "#/$defs/bindings" }
  },
  "$defs": {
    "event": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "enabled": { "type": "boolean" },
        "detector": { "enum": ["native"] },
        "params": { "type": "object", "description": "tier 1: keys exactly the catalog tunables of this event; each number inside catalog min..max" },
        "condition": { "$ref": "#/$defs/condition", "description": "tier 2 only; absent in v1" },
        "arming": { "$ref": "#/$defs/arming" },
        "cooldown_ms": { "type": "integer", "minimum": 0 },
        "tier": { "type": "object", "properties": { "min": {"type":"integer"}, "max": {"type":"integer"} } }
      }
    },
    "condition": {
      "description": "Tier 2 (not in v1). Bounded tree over whitelisted signals; no scripting.",
      "oneOf": [
        { "type": "object", "required": ["all"], "properties": { "all": { "type": "array", "items": { "$ref": "#/$defs/condition" } } } },
        { "type": "object", "required": ["any"], "properties": { "any": { "type": "array", "items": { "$ref": "#/$defs/condition" } } } },
        { "type": "object", "required": ["signal", "op", "value"], "properties": {
            "signal": { "type": "string" },
            "op": { "enum": ["<=", ">=", "<", ">", "==", "!=", "drop_ge", "rise_ge", "crossed_down", "crossed_up"] },
            "value": { "type": "number" },
            "window_ms": { "type": "integer", "minimum": 100, "maximum": 30000 }
        } }
      ]
    },
    "arming": {
      "type": "object", "additionalProperties": false,
      "properties": { "rearm": { "type": "number" }, "rearm_signal": { "type": "string" }, "rearm_on": { "enum": ["respawn", "threshold", "none"] } }
    },
    "bindings": {
      "type": "object",
      "additionalProperties": { "type": "object", "properties": {
        "tier": { "type": ["integer", "null"] },
        "clip": { "type": "object", "properties": { "pack": {"type":"string"}, "mapping_key": {"type":"string"} } },
        "banner": { "type": "object", "properties": { "pack": {"type":"string"}, "mapping_key": {"type":"string"} } }
      } }
    }
  }
}
```

Rules enforced by the loader, not just the schema (PROPOSED):

1. **Delta over embedded default.** The file lists only what differs; absent = embedded value. An exported file is a full snapshot (all effective values) so it can be read back or diffed.
2. **Bounds from the catalog.** Every `params` number must lie in the catalog `min..max`; out-of-range = whole profile rejected (not clamped silently).
3. **Critical events cannot be disabled or silenced by the profile.** `health_danger` (and any `critical` catalog event): `enabled` must be absent or `true`; `cooldown_ms` and params stay within catalog bounds; a binding to no clip is allowed (fail-silent is MP-08) but Lab shows it as a red readiness gap.
4. **Unknown keys are errors** (`additionalProperties:false`, strict JSON, duplicate keys rejected, same posture as `strict_json.rs`).
5. **Bindings hold references, never bytes.** Clip/banner are `pack + mapping_key` pairs resolved through the existing package reader, so MP-03/MP-04 admission (digest, decode probe) still applies; Lab never reads loose files or downloads anything.

### 2.3 Expressiveness tiers

| Tier | What the owner can change | Needs |
|---|---|---|
| **1 Parameters** (stages 1-4) | The catalog tunables of existing native detectors, plus cooldown, enabled (non-critical), clip/banner binding | Wire `Config` from the profile. No contract change. |
| **2 Condition templates** (stage 6, optional) | Re-express threshold-edge (health_low, mana_low) and burst-drop (health_danger) as a bounded `condition` tree over whitelisted signals, interpreted by one reviewed generic evaluator. Still no scripting. | Owner reading of MP-01 ("not a runtime rule engine") and catalog change `detector_ref` semantics. **DEC-LAB-LOGIC.** |
| **3 Custom events** | A new event id built from a condition | Catalog minor bump, rank/arbitration, media mapping, EVENT-AUDIO-SPEC. Out of scope for this design. |

Minimal tier-2 illustration of `health_danger` as data (equivalent to lib.rs `observe_health_danger`, UNVERIFIED until a parity test exists):
`all[ hp_percent <= 25 , hp_percent drop_ge 15 within 2000ms ]`, arming `rearm_signal=hp_percent rearm=40`, `cooldown_ms=20000`, dead ticks excluded by the evaluator's implicit `alive==true` guard.

## 3. Mapping onto the existing detector (PROPOSED)

| Aspect | Configurable today | Hard-coded today | Lab approach |
|---|---|---|---|
| Thresholds, windows, cooldown, re-arm (13 `Config` fields) | Yes, via `Config` / `with_config` (unused by host) | Defaults in `Default for Config` | Profile `params` → `Config`; default JSON ↔ `Config::default()` kept equal by a test |
| Level milestones list | Yes (`levels`) | default `[6,12,18,25]` | `params.levels` |
| Rank, tier range, voiced, critical | No | `match` on `EventId` | Stay in code/catalog; Lab shows them read-only |
| Alive/valid-hp guards, baseline/epoch/regression handling, first-blood rule (E-1), chain rule | No | Inline in `observe` | Stay native. Lab shows them as "guards" text per event (explain view), not editable |
| Risk rules (`kohbor-risk`) | No (`pub const`) | PROPOSED consts | Out of Lab v1; shown read-only in explain view only if `RiskEngine` can expose trace (UNVERIFIED) |
| Clip/banner mapping | By installed pack (kohbor-pack) | n/a | Profile `bindings` is a **dev preview overlay**; production mapping stays the pack's registry (MP-03/06) |

**Explain trace (new, read-only):** to answer "why did health_danger fire", `observe` needs a side-output of intermediate values per tick. PROPOSED: a new `Detector::observe_traced(&Tick) -> (Vec<Event>, Trace)` where `Trace` lists per event: the input signals and values used, each sub-condition `{name, lhs, op, rhs, result}`, armed/cooldown state, and the reason it did not fire. `observe` stays byte-for-byte as is; a test asserts `observe_traced(..).0 == observe(..)` across all existing golden and `death_counter` fixtures. This is the one invasive piece: it touches `kohbor-events/src/lib.rs`, which is an owned crate, so it needs a claim in `F:\kohbor-coord` and its own PR.

**Migration path (no existing test breaks):**

1. New crate `kohbor-lab` (pure, no I/O): profile types, schema validation, `merge(default, override) -> Effective`, `effective_to_config() -> kohbor_events::Config`, export. `kohbor-events` gains no dependency on it.
2. Host builds `Detector::with_config(effective.config)`; with no override, effective == `Config::default()`, so every current test, the health-danger golden trace (`fixtures/health-danger-golden.json`) and recorded-session replay results are unchanged. A parity test pins this.
3. Evidence classes stay separate: unit/golden (synthetic) are re-run unchanged; replay results are labelled "recorded GSI, offline"; nothing from Lab counts as live-match, acoustic or FPS evidence.
4. Tier 2 (if approved) is added behind a second evaluator, run **in parallel with the native detector in tests** until parity over all golden and recorded sessions is shown; the native path remains the default for critical events.

## 4. Configuration precedence and production gating (PROPOSED)

```
effective profile = (dev override file, if allowed and valid)  >  embedded default
```

- **Embedded default**: `include_str!` of `default-profile.json`, parsed and validated at startup and in CI; a `profile_version` and a content SHA-256 are part of the build. Production reads nothing else.
- **Dev override**: honoured only when **both** `cfg(any(feature="lab", debug_assertions))` is compiled in **and** `KOHBOR_LAB=1` with `KOHBOR_LAB_PROFILE=<absolute path>` is set, mirroring `KOHBOR_DEMO`. The code that reads the env var and file is compiled out of release builds (cfg), and a source-scan test (like `demo.rs:187`'s "release source does not contain KOHBOR_DEMO") asserts the release path has no reference to the override. UNVERIFIED: the exact scan technique; reuse of the demo test is assumed.
- **Invalid override** → fall back to the embedded default, show a persistent "Lab profile rejected: <reason>" banner, and log it. Never partially apply.
- **Hot reload (dev)**: Lab edits write the override file atomically (existing `atomic.rs` pattern) and apply at the next session boundary, never mid-tick (detector state would otherwise mix configs). "Live" in the UI means "applies from the next replayed/real session epoch", plus instant re-evaluation in the replay sandbox (§5.3).
- **Export** always writes the full effective profile plus a header (`profile_version`, `taxonomy_version`, `embedded_default_sha256`, source revision). Promoting an export to the embedded default is a source change (replace `default-profile.json` in a PR), never a runtime action.

## 5. Kohbor Lab UI (PROPOSED)

A route in the control window, `Lab`, present only in lab/debug builds with `KOHBOR_LAB=1`, labelled **DEV** (same honesty rule as the demo chip: it can never be mistaken for a production feature). Not in UX-SPEC's destination inventory today; adding it there is a doc change for the owner.

### 5.1 Layout

```
+-------------------------------------------------------------------------------+
| Kohbor Lab  [DEV]  profile: embedded | override (valid, sha 3f9a..)   [Export]  |
+---------------+-----------------------------------+---------------------------+
| EVENTS        | EVENT DETAIL: health_danger        | SIGNAL INSPECTOR (live)   |
| (cards)       |  category critical  rank -         |  gsi.hero.hp        412   |
| multi_kill    |  Why it fires (guards)             |  gsi.hero.max_hp   2000   |
| streak        |   [x] alive == true                |  derived.hp_%      20.6%  |
| ...           |   [x] hp_% <= 25     now 20.6 OK   |  ---- vs threshold ----   |
| health_low    |   [ ] drop >= 15pt/2s now 3 NO     |  hp_% <= 25   ####|--   |
| health_danger |   [x] armed (rearm >= 40%)         |  drop  15pt/2s  #----|    |
| mana_low      |   [x] cooldown 20s ok              |  ring buffer 2s: sparkline|
| ...           |  Params  max_hp% [25]  drop [15]    +---------------------------+
|  status dots: |          window [2000] rearm [40]  | CLIP / BANNER BINDING     |
|  ok/unmapped/ |          cooldown [20000] (bounds) |  pack: [..]  key: [..]    |
|  gap          |  Last fired: 12:03:11 (replay)     |  [Play clip] [Show banner]|
+---------------+-----------------------------------+---------------------------+
| REPLAY / TEST:  [session: 01-mid-2026110... v] [Run] [step] timeline w/ event  |
| markers; click marker -> detail shows that tick's values and which guard passed |
+-------------------------------------------------------------------------------+
```

### 5.2 Regions

1. **Event cards** (concept from G-AnnStudio EventTestGrid): one card per catalog event the detector can emit (12), showing label (en/th from catalog), category, a status dot (bound / unmapped / safety-gap / disabled), last-fired time, and a **Test** button that triggers the demo path (`demo_trigger`, S6 §9) so the real arbiter, audio and overlay run. Non-detector catalog events appear greyed with "vision/native risk — not Lab v1".
2. **Event detail / "why"**: per-event list of guard rows `{name, lhs value, operator, rhs threshold, pass/fail}` from the explain trace (§3). Unknown shows "—", never 0 (UX-SPEC §6). Parameter editors are numeric steppers with catalog min/max, unit, default, and a reset-to-default per field; changed fields are visibly marked and counted ("3 overrides").
3. **Signal inspector (live)**: current admitted value of each signal next to the threshold it is compared against, with age/freshness (fresh ≤ 2000 ms per own-snapshot contract) and a short ring-buffer sparkline. Source selectable: live GSI, or the replay cursor.
4. **Clip pairing** (concept from PackEditor/EventPicker): per event (and tier), choose a clip and optional banner from installed packs via the existing media listing; **Play** uses the same package reader and native audio owner as production (MP-04: preview and production share admission). A clip that fails admission/decode is shown with its reason code, never substituted.
5. **Replay sandbox**: pick a recording in `F:\gsi-log` (read-only), run it through admission → `observe_traced` with the *edited, unsaved* params and show events on a timeline; compare "embedded vs edited" side by side (events gained/lost). This is offline and instantaneous because the detector is pure; it does not need `gsi_replay.py` or a demo instance. For an end-to-end check of the real host (audio, overlay), the owner still uses `tools/gsi_replay.py` into a demo instance as documented in `docs/operations/GSI-REPLAY.md`.
6. **Export**: writes the full effective profile (JSON) and a short human summary (markdown: changed fields vs embedded default, safety-gap list). Import of a profile = pointing `KOHBOR_LAB_PROFILE` at it.

## 6. Out of scope (intentional)

- Arbitrary scripting, expressions strings, plugins or WASM rules. (Condition trees in tier 2 are data over a fixed whitelist of signals and operators.)
- Custom new event ids (tier 3), editing rank/tier range/voiced/critical, editing arbitration or the ordinary/critical class matrix (EVENT-AUDIO-SPEC §4-§5).
- Editing `kohbor-risk` rules and any CV/minimap/capture signals in stages 1-4 (listed as a later signal source only).
- Cloud sync, telemetry, remote profile distribution, account-bound profiles, paid features, a profile store.
- Any performance or fps tuning (owner: after core features). The inspector's sparkline is a fixed-size ring buffer; no measurement is claimed.
- Rebuilding a pack authoring editor, importing or downloading clips, changing pack trust/authority. Lab only references already-installed packs.
- Shipping the Lab in production builds.

## 7. Stage plan and acceptance checks

| Stage | Deliverable | Acceptance (evidence class) | Owner decision? |
|---|---|---|---|
| **L0** | This doc reviewed; owner picks DEC items below | PR review | Yes: DEC-LAB-LOGIC, DEC-LAB-GATE |
| **L1 Read-only explain + export** (smallest useful) | `kohbor-lab` crate: embedded default, schema, `effective_to_config`, export; `observe_traced`; Lab route with event cards, "why" view, live signal inspector, **Export current effective profile**. No editing. | Unit: embedded default == `Config::default()` and == catalog defaults (synthetic). Parity: `observe_traced` events == `observe` over all existing fixtures + health-danger golden (synthetic). Replay: explain trace on one recorded session shows the sample that fires `health_danger` (recorded, offline). Release build: override code absent (source scan). | No (once L0 done) |
| **L2 Editable thresholds** | Param editors with bounds; override file write; replay sandbox with edited params; host uses `with_config` from effective profile | Unit: out-of-bounds, unknown key, duplicate key, critical-disable are rejected, fall back to embedded + banner. Unit: edited health_low threshold changes replay output as predicted. Precedence test: override > embedded, release ignores override. | No |
| **L3 Clip pairing + test** | Bindings editor, Play/Show banner via real reader and audio owner; safety-gap indicator for critical events | Integration (native): bound clip plays through production path (Windows native evidence, manual first); invalid media shows reason, plays nothing. Rights of clips remain the owner's rights team. | No |
| **L4 Export/import polish** | Diff vs embedded, markdown summary, "promote to default" documented as a PR recipe | Round-trip: export → set as override → effective equal (synthetic). | No |
| **L5 Live tuning against a real match** | Lab open during a live match with a dev build | Live-match evidence is its own class; may not be claimed from L1-L4 | Owner supplies match |
| **L6 (optional) Tier-2 condition evaluator** | Generic evaluator + parity suite vs native for health_low, mana_low, health_danger | Parity on all goldens and recorded sessions; native remains default for critical until the owner signs off | **Yes: DEC-LAB-LOGIC** |

Owner decisions required (PROPOSED ids, not yet in DECISION-LOG):

- **DEC-LAB-LOGIC**: stay at tier 1 only, or approve tier 2 (reinterpret MP-01 for a bounded, reviewed evaluator). Recommendation: tier 1 now; revisit after L2 shows what the owner actually wants to change.
- **DEC-LAB-GATE**: gating mechanism (new cargo feature `lab` vs reusing `demo`) and whether Lab may exist in a release-profile dev build. Recommendation: reuse the demo gating pattern with a separate `lab` feature.
- **DEC-LAB-CONTRACTS**: the owner-reserved changes below.

### 7.1 Proposed owner-reserved contract changes (not made here)

1. `contracts/event-catalog.json`: add, per tunable, an optional `safety` flag (`floor`/`ceiling` the Lab may never cross, narrower than `min/max` for critical events) so the bounds live in one place instead of being duplicated in the Lab loader. Optional; the loader may initially read `min/max` only.
2. A new `contracts/lab-profile.schema.json` **only if** profiles ever cross a process or machine boundary; while the file is dev-local, the crate-local schema in §2 suffices (recommended).
3. `docs/design/UX-SPEC.md` §2: one row for the dev-only Lab destination, stating it is absent in production.
4. `docs/domains/` registry: a domain card for Lab (steward, files owned) before L1 begins.
5. Decision log: record DEC-LAB-GATE (decided 2026-11-15) and DEC-LAB-LOGIC (open, tier 2) in `contracts/decisions.json`.

## 8. Risks

| Risk | Consequence | Mitigation |
|---|---|---|
| A mis-edit silently disables or dulls a safety event (e.g. `health_danger` window 500 ms, max_hp 10%, or clip unbound) | Safety cue never fires or is silent; owner believes it is working | Critical events not disable-able; catalog bounds enforced as errors; Lab shows an always-visible **Safety readiness** strip (event, effective params vs default, bound clip, last verified); "N overrides on critical events" badge; export summary lists them first; golden trace for `health_danger` re-run in the replay sandbox on every save |
| Version skew: profile written for taxonomy/profile version X, app at Y | Unknown keys ignored or misapplied | `profile_version` and `taxonomy_version` required; MP-05 grammar and rules (older/equal OK, newer unknown inert but reported, different major rejected); `embedded_default_sha256` recorded so drift from the embedded base is shown |
| Dev override leaks into production | Behaviour differs from tested defaults; unreviewed config in shipped app | cfg-compiled-out override path, env + feature both required, release source scan test, effective-profile hash shown in About/readiness; CI check that the release artifact contains no override symbol (UNVERIFIED how to assert on a binary) |
| Embedded default and `Config::default()`/catalog drift apart | Tests pass but production uses different numbers | Parity tests (§7 L1) in the gate; single source of truth is `default-profile.json`, `Config::default()` derives from it or is asserted equal |
| `observe_traced` diverges from `observe` | Explain view lies about why an event fired | Trace produced by the same code path (one function, two outputs), not a second implementation; parity test on all fixtures |
| Mid-session reconfiguration corrupts detector state | Spurious or missed events | Apply only at session-epoch boundary or in a fresh sandbox detector |
| Replay results mistaken for live evidence | False confidence | Every Lab result carries an evidence-class label (synthetic / recorded-offline / live) and the recording id; no PASS crosses classes (AGENTS.md) |
| Recorded sessions in `F:\gsi-log` contain identifiers | Privacy leak via export | Export contains profile and aggregates only, never raw GSI bodies, steam ids or tokens |
| Scope creep into a rule engine | MP-01 violated, maintenance cost | Tiers + out-of-scope list; tier 2 gated behind an owner decision |

## 9. Open points / UNVERIFIED

- Whether any test already asserts catalog tunables ↔ `Config` fields (UNVERIFIED).
- Exact shape of an existing event/overlay path that a Lab Test button would call beyond `demo_trigger` (UNVERIFIED; read from S6 doc, not code).
- Whether `RiskEngine` can expose a trace without changing its behaviour (UNVERIFIED; excluded from v1).
- Source-scan and binary-symbol techniques for the release-ignores-override check (UNVERIFIED).
- Frontend component choices for the UI (React 19 is in the repo; no Lab component exists).

## 10. Implementation status (L1/L2, 2026-11-15)

Owner decisions applied: DEC-LAB-GATE (dev-only, start now), tier 1 only. Evidence classes are kept apart.

| Piece | Where | Evidence (this checkout) |
|---|---|---|
| Explain trace (`Detector::observe_traced`, guard rows for health_danger, health_low, mana_low, level_milestone, hero_kill, hero_death; other events listed without guards) | `crates/kohbor-events/src/{lib,trace}.rs` | CODE + synthetic tests: `tests/trace_parity.rs` (events identical to `observe`; `fired == all guards pass` on every tick of the 6 golden traces and synthetic walks). Not a second implementation: `observe` fills a scratch record at the same statements that decide. |
| Profile crate: embedded default, bounded override, whole-profile validation, batch edits, export, describe | `crates/kohbor-lab` | Synthetic tests `tests/{profile,edit}.rs`: default == `Config::default()` == catalog defaults, bounds/unknown/duplicate/version/re-arm/critical-lock rejections, export round trip. |
| Production ignores overrides | `dev_override.rs` behind `cfg(any(feature="lab", debug_assertions))` | `tests/release_gate.rs` source scan plus `cargo test --release -p kohbor-lab` run (release variant compiled and passing). Binary-level assertion: UNVERIFIED. |
| Replay sandbox (offline, edited vs base) | `crates/kohbor-lab/src/sandbox.rs` | Synthetic: reproduces golden cue times `[2000, 25000]`; shows the cue lost when `max_hp_percent` is lowered to 10. Recorded `F:\gsi-log` sessions are NOT yet fed in (needs the admission pipeline of `kohbor-replay`): UNVERIFIED. |
| Lab view model and view | `apps/desktop/src/lab/*`, `tests/lab-model.test.ts` | Model: 6 `node --test` tests pass. `Lab.tsx`: written, NOT typechecked or run (`node_modules` missing, host not wired): UNVERIFIED. |
| Host adapter (`lab_status/describe/last_trace/export/apply_edits/replay`) | `apps/desktop/src-tauri/src/lab.rs` | Wired 2026-11-15 after the MVP lead's release (base: `mvp3/volume` merged in). `cargo check`/clippy `-D warnings` (debug, all targets) pass; unit tests `lab::tests` pass (host `observe` equals plain `observe` on the golden replay; default host config equals `Config::default()`). Release `clippy -D warnings` fails only on 3 pre-existing dead-code items in `demo.rs`, none in Lab. Not launched; commands not invoked from the UI: UNVERIFIED. |

v1 profile scope narrowed to `events.<id>.params` (13 tunables of the existing `Config`). Unchanged from the plan: critical events cannot be disabled; override file applies at the next session boundary; `KOHBOR_LAB=1` and `KOHBOR_LAB_PROFILE=<absolute path>` are both required, and only in `lab`/debug builds.

### 10.1 Host wiring conditions (MVP lead ACK, 2026-11-15, conditional)

Host wiring (`lib.rs`, `intake.rs`, `App.tsx`, desktop `Cargo.toml`) was released by the MVP lead on 2026-11-15 and is applied on top of `mvp3/volume`; `commands.rs` and `main.tsx` are untouched (Lab commands live in `lab.rs`; `<Lab />` is mounted in `App.tsx` beside the demo panel, lazily and only when `import.meta.env.DEV`). Conditions from the host lead: release parity is the bar (feature `lab` off and no debug assertions behaves exactly as today; `lab::observe` and `lab::current_config` are plain passthroughs there, no IO, no extra field or lock); `lab` not a default feature; `mod lab;` private; the six `lab_*` handlers only in the debug/demo handler list; `<Lab />` loaded with `React.lazy` behind the availability check so release bundles omit it; apply **after** the `mvp3/volume` branch lands (merge order decided by the coordinator) to avoid conflicts in `lib.rs`, `commands.rs` and `App.tsx`; Lab output stays synthetic/replay evidence, never live.

Known deviations to settle at wiring time: (1) `kohbor-lab` cannot be an optional dependency because debug builds (debug assertions) need it without the feature; release builds do not call it (the linker drops it), which is a weaker guarantee than "not linked" (UNVERIFIED on the binary); (2) RESOLVED (coordinator decision): `lab_apply_edits` writes only `<app data dir>/lab/profile.override.json` (fixed name; no renderer- or environment-supplied path; `KOHBOR_LAB_PROFILE` only selects what is read at startup). `save_under` refuses relative data dirs, `.`/`..` components, a missing data dir, and links (symlink or junction) at the lab directory, the target or the temporary file; synthetic tests in `crates/kohbor-lab/tests/edit.rs` cover each (the link tests skip with a message where links cannot be created); (3) `lab_export` contains parameters only (no steam id or names).

## 11. Stage L3: overlay editor (banner), 2026-11-15

Owner order (relayed): use the G-Maiden reference picture and "profile 1", and put an overlay editor into our program. Same gate as the rest of Lab (DEC-LAB-GATE: dev/test builds only). Clean-room: only the UX concept (a 16:9 canvas with a screenshot background, a draggable module, numeric fields, HUD zones with an overlap warning, reset) was taken; no G-Maiden code was read or copied.

- **Layout model:** profile field `layout.banner {x_percent, y_percent, scale, enabled}` (percent of the primary monitor; the anchor is the CENTRE of the art stage; bounds x/y 0..100, scale 0.2..3.0), validated like any profile field (partial overrides allowed, unknown module/field rejected). The default is the host's `overlay::DEFAULT_BANNER_LAYOUT` (50 / 10.5 / 0.45 / true, the owner's profile 1); `kohbor-lab`'s embedded default repeats it and a host test (`lab_layout_default_equals_the_overlay_default`) pins equality.
- **Geometry is not duplicated:** `lab_layout_preview` calls the real `overlay::place_overlay_with`, so the editor draws the clamped rectangle the host will use. HUD zones come from the host's `overlay::PROTECTED_REGIONS`.
- **Writes:** only through `save_under` (`<data dir>/lab/profile.override.json`); no renderer path. The host hook for show time is `lab::banner_layout()` (dev/test only); wiring it into `overlay::effective_banner_layout()` is the host lead's one line.
- **Reference screenshot:** never committed or bundled. Loaded at runtime from `<data dir>/lab/assets/dota-overlay-reference.jpg` (fixed name, JPEG only, 8 MiB max, no links); absent means a plain 16:9 grid. Rights stay unverified (AGENTS.md); the owner authorised its local use. Update 2026-11-22: the Settings overlay layout editor (UI-015) reads the same file through `lab_reference_image`; separately, by owner decision of 2026-11-22 (DECISION-LOG), a redacted copy of this owner screenshot (one chat-feed line with a player name pixelated, no metadata) is bundled as `apps/desktop/src/assets/editor/editor-reference-1080p.jpg`, the Settings editor's default background only. Provenance: the owner's own screenshot taken with G-Maiden; rights unverified; the owner accepted the risk; a rights-team review is required before any public release (THIRD-PARTY-DATA). The unredacted original stays local and is still never committed.
- **Test banner:** the existing dev `demo_trigger` (needs `KOHBOR_DEMO=1`).
- **Evidence (this checkout):** Rust unit tests for layout bounds, merge, export round trip, reference-image rules; node tests for clamp, drag, nudge, overlap, parsing; `tsc` and both frontend builds pass; the normal bundle contains no Lab code. **UNVERIFIED:** the view was not launched, drag and fullscreen were not exercised in a webview, the real overlay window was not moved by an applied layout (hook not wired yet), the reference image was not loaded in the app. The default banner overlaps the host's `announcement` protected region, so the editor shows an overlap warning for the owner's own profile 1 (as drawn on a 16:9 monitor; expected from the region numbers, not observed).

### 11.1 Build matrix (corrects earlier statements, 2026-11-15)

Found by the host lead's binary scan: Lab's commands are registered only in the debug/demo handler list, so a release exe built with `--features lab` alone had no `lab_*` commands. Fixed: **`lab` implies `demo`** (`lab = ["demo", "kohbor-lab/lab"]` in the desktop `Cargo.toml`), and a stub `lab::dev` module (compiled only for `demo` without `lab` and without debug assertions) keeps the same handler names compiling and answers "unavailable". Earlier text that says "only the `lab` feature enables Lab in release" means `lab` (which pulls in `demo`).

| Build | Result |
|---|---|
| shipping: `--release` (no features) | Lab absent; `lab::observe` is a plain passthrough |
| demo only: `--release --features demo` | Compiles (was broken); `lab_*` stubs answer unavailable; no override reader |
| Lab test build: `--release --features lab,tauri/custom-protocol` with `VITE_KOHBOR_LAB=1` | Lab commands real; needs `KOHBOR_LAB=1` (and `KOHBOR_DEMO=1` for Test banner) at run time |
| debug builds | Lab real; same run-time switches |

Verified by `cargo check -p kohbor-desktop --offline` for: `--release --features demo`, `--release --features lab`, `--release`, debug, debug `--features lab` (all finish without error). **Regression guard:** `cargo check -p kohbor-desktop --release --features demo` is not part of `tools/gate.py`; adding it is a proposal for the gate owner (about 1.5 minutes in a warm lane).

### 11.2 Usability fixes after the build 10 screenshot (2026-11-15)

- **Layout:** the Lab is no longer a fifth cell of the status grid (where it fell outside the visible, non-scrolling area). It is a fixed full-window studio panel opened from a small "Kohbor Lab (DEV)" launcher; inside, header (title, profile line, tabs `Events` | `Overlay editor`, Export, Close) plus ONE bounded scroll container (`overflow-y: auto`, `min-height: 0` chain, keyboard focusable, visible focus ring). Escape closes it unless the canvas is fullscreen. The shell's no-page-scroll rule is untouched (a test asserts the Lab CSS does not style `html/body/#root/.app`). The canvas width is `min(100%, (100dvh - 170px) * 16/9)` so it should fit at 1280x720 to 1920x1080 (computed, not yet seen on screen: UNVERIFIED).
- **Neutral profile line:** an unset `KOHBOR_LAB_PROFILE` is no longer a rejection; the view says "embedded profile (no override file)". Real rejections (bad file) still show.
- **Saved edits survive a restart:** when no profile file is selected by the environment, the first Lab command loads `<data dir>/lab/profile.override.json` if present (before this, saved edits were written but never re-read). Limitation: this happens when the Lab launcher first asks the host, so the overlay hook uses the compiled default until then.

### 11.3 HUD layout editor

The dev-only editor stores the six HUD module layouts under `layout.hud.<id>` in `<data dir>/lab/profile.override.json`; each module has a screen-centre `x_percent`, `y_percent`, `scale` and `enabled`. It merges with the existing profile, uses the profile parser's 0..100 percent and 0.2..3.0 scale bounds, and reset writes only the embedded HUD defaults while preserving banner and detector edits. No settings, runtime-status or shared contract schema changes.

With `KOHBOR_LAB=1` in a `lab` or debug build, `lab_hud_layout_get`, `lab_hud_layout_apply`, `lab_hud_layout_reset` and `lab_hud_preview` are available. The HUD window receives `hud://layout` (schema 1, six modules and `strip_height_percent`) at thread start, after a layout revision and every two seconds. Preview accepts 1..60 seconds and emits fixed synthetic stats through `hud://state` until its deadline, then resumes the real snapshot; preview data is **synthetic evidence** only.

Source and test coverage do not verify the rendered editor, native window placement, live event delivery, a Windows display or a real match. No app was started for this change.

### 11.4 Lab minimap frame recorder (LAB-001)

The owner instruction to build a Lab tool for minimap test-data collection is recorded in the [Decision Log](../governance/DECISION-LOG.md). The recorder is available through the Lab consent switch only when `lab` is compiled and `KOHBOR_LAB=1`; consent defaults off. It records only the already admitted minimap crop while both capture and GSI phase are live and fresh, the crop is confirmed, and candidate session epoch matches the fresh GSI session. It does not open another capture source, identify heroes or alter UI-016.

One background worker receives `Arc`-backed crop samples through a bounded eight-frame queue. The capture thread uses `try_send` and never encodes or writes. A full queue increments the drop count; later successful frame writes include the current counter in the match manifest. PNG encoding uses the existing `flate2` dependency. Sampling is 1–5 fps (default 2 fps).

The local data root is `<app-data>/lab/minimap-recordings/<match-hash>/`, with configuration at `<app-data>/lab/minimap-recorder.json`. The total cap defaults to 5 GB (5,000,000,000 bytes) and is configurable; one match can store at most 150 MB (150,000,000 bytes) of PNG frames. A cap stops recording for that match and reports the reason in Lab status. Older recordings are never automatically deleted; Lab asks before deleting only the selected match. The manifest stores only an HMAC-SHA256 derived from the install key and session epoch, safe match metadata, capture settings and frame/drop counts. Each JSONL frame entry stores filename, game clock, epoch, phase, capture age, source crop rectangle, source capture size and stale state. PNGs contain the minimap crop only.

Only the current read model's team side and session epoch are available at the capture hook. Game clock, build and full team lineups stay null until an approved read-only intake view supplies them. Dota's hero-icon toggle and hero-size values are read without writes from the active Steam user's convars file; missing or invalid values stay null. The host status payload exposes `minimap_recording` for the Live status chip, but its UI consumer remains outside this change's authorized file scope. The evaluator adapter reads `manifest.json`, `frames.jsonl` and PNG crops; human marker annotations remain a separate input and are never inferred.
