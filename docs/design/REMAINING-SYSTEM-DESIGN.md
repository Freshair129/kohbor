---
title: "Remaining System Design"
doc_id: "REMAINING-SYSTEM-DESIGN"
status: draft
updated: "2026-11-12"
authority: "Lead architect output under owner delegation; PROPOSED. Changes no baseline until recorded under DOCUMENT-CONTROL"
approval_status: "not_approved"
---

# Remaining system design (Sprint 4 onward)

This is an agent design, not human, security or legal approval. It designs what Sprints 0 to 3 did not build and decides what can be decided now. It does not duplicate `docs/design/S3-5-HOST-CRITICAL-PATH.md` (in preparation); alert integration below plugs into that path.

**Labels.** FROM-REPO = file and section. VERIFIED = read on the web on 2026-11-12 through a fetch tool (URL in section 8; re-read before citing externally). UNVERIFIED = not confirmed; the confirming step is named. PROPOSED = my decision or value, inside delegated engineering freedom unless marked OWNER.

**Baseline check.** A live 30 fps picture is a design target, not a requirement: UI-002 asks for "an enlarged minimap as the dominant region" and DESIGN-NOTES §9 says 30 fps is "drawn as the target ... the budget has not been measured" (FROM-REPO). Nothing here adds a feature to PRD/SRS. Items outside the baseline are marked **PROPOSAL-OWNER**.

## 0. Decisions in brief

| # | Decision | Status |
| --- | --- | --- |
| D1 | Capture with DXGI Desktop Duplication on the game's output; copy only the minimap rectangle off the GPU | Decided; spike V1 must confirm cost |
| D2 | Windows.Graphics.Capture is a second backend behind the same trait, Windows 11 only, built only if V1 fails on FPS | Decided |
| D3 | The displayed picture needs no CV. Display path and analysis path are separate consumers of one frame slot, with separate rates and budgets | Decided |
| D4 | Frame transport is behind one seam; T0 raw IPC channel first, T1 WebView2 shared buffer if T0 misses its allocation, T2 native child window only as an escape | Decided; spike V2 selects by a rule fixed here |
| D5 | Calibration = profile candidate, structural check, one user confirmation; a crop is shown only when validated; every frame carries a calibration generation | Decided |
| D6 | Markers ship in stages: picture (M3a), anonymous presence (M3b), identity per certified class (M3c). Identity never blocks the picture | Decided; DEC-MODELS gates M3b/M3c |
| D7 | Share list engine is a pure crate; pair secret by X25519 + HKDF on the two clients; blind binding by HMAC; announce + sync over one sequential worker | Decided; contracts to freeze in S10-0 |
| D8 | Server = two deployables (account broker with friend-code module; stateless relay), Ed25519 tokens with one audience each | Decided at architecture level; hosting is OWNER |
| D9 | A loopback fake (broker + relay + fake identity provider) lets the client be built and tested with no hosted service | Decided |
| D10 | Peer, account and memory work run as parallel tracks beside the M3 line; one cargo writer at a time stays the rule | Decided |

## 1. ADR-V: minimap capture, transport and vision

### 1.1 Context

- The Command Deck shows the real in-game minimap enlarged on a second monitor (DEC-SESSION-MONITOR, UI-002, UI-008; FROM-REPO DECISION-LOG, UX-SPEC §4). GSI carries no enemy position (DATA-005).
- Certified cell: Windows 10 build 19045 and Windows 11 build 26100, 1920×1080, 100% DPI, borderless fullscreen, default minimap bottom-left (FROM-REPO NFR "Certified starting matrix").
- Budgets: every 1 s CPU sample ≤10% of total CPU for the whole job; peak Working Set and VRAM measured and reported with no limit until the owner sets one from the first baseline (900,000,000 bytes until DEC-RESOURCE-BUDGET, withdrawn); FPS loss ≤3% in each pair (FROM-REPO NFR). The job is a hard cap created before the WebView (FROM-REPO SDD §14).
- SAD §4 already says "Retain DXGI read-only vision as the baseline" (FROM-REPO).

### 1.2 Platform facts

| Fact | Label |
| --- | --- |
| Desktop Duplication returns the whole output image as a DXGI surface, always `B8G8R8A8_UNORM`, with dirty and move rectangles; failures to handle include `DXGI_ERROR_ACCESS_LOST` and `DXGI_ERROR_WAIT_TIMEOUT`; rotation must be handled by the client | VERIFIED [W1] |
| Windows.Graphics.Capture captures a display or a window (no sub-region), and the system draws a yellow border around the captured item | VERIFIED [W2] |
| `GraphicsCaptureSession.IsBorderRequired` was introduced in build 20348. It does not exist on build 19045, so the border cannot be turned off there. Turning it off needs user consent through `GraphicsCaptureAccess.RequestAccessAsync(Borderless)` and the `graphicsCaptureWithoutBorder` capability | VERIFIED [W3] |
| `MinUpdateInterval` (system-side rate limit) exists only from build 26100 | VERIFIED [W4] |
| A hard-capped job: "After the job reaches its CPU cycle limit for the current scheduling interval, no threads associated with the job will run until the next interval." `CpuRate` is percent × 100 | VERIFIED [W5] |
| WebView2 `CreateSharedBuffer` gives memory visible to both the host and page script as an `ArrayBuffer` (SDK 1.0.1661.34 and later, under 2 GB) | VERIFIED [W6] |
| Tauri recommends `tauri::ipc::Response` for array buffers and `Channel` for streaming; the page gives no transport or performance numbers | VERIFIED [W7] |
| Whether duplicating the output removes independent flip for a borderless game, and what that costs in FPS | UNVERIFIED: spike V1 FPS pairs |
| Whether an unpackaged Win32 app can disable the WGC border on Windows 11 without a package manifest | UNVERIFIED: spike V1 (only if D2 is triggered) |
| Whether Tauri 2.12.5 / wry exposes the WebView2 environment well enough to call `CreateSharedBuffer` and `PostSharedBufferToScript` | UNVERIFIED: spike V2 |
| Side length of the default minimap at 1080p, and how "extra large minimap" and "minimap on the right" change it | UNVERIFIED: spike V3 (owner stills) |
| How Dota 2 draws heroes on the minimap under each option (icons, arrows, colours) | UNVERIFIED: spike V3/V4 |

### 1.3 Decision D1/D2: capture API

**Decision.** One capture worker in the native host uses Desktop Duplication on the output that contains the game window. Each cycle: `AcquireNextFrame(timeout 0–15 ms)` → `CopySubresourceRegion` of the calibrated rectangle into a small staging texture → `ReleaseFrame` at once → `Map` the staging texture → write BGRA into the frame slot. The full desktop image never enters CPU memory and is never stored (SECURITY-PRIVACY route matrix: screen images are local only; FROM-REPO).

| Alternative | Why rejected |
| --- | --- |
| WGC as the primary | Yellow border on build 19045 cannot be removed [W3]: a visible mark on the game screen in one of two certified OS cells. Consent prompt and capability on Windows 11 |
| WGC window capture for both | Same border problem; no sub-region either [W2] |
| GDI BitBlt / PrintWindow | CPU copies of the whole screen; no frame timing |
| OBS virtual camera or game-capture hook | VISION-004 forbids OBS replacing the critical source; a hook is injection, which is a non-goal (PRD §4) |
| Reading the minimap from game memory or files | Non-goal (PRD §4); Valve-policy exposure (FROM-REPO SHARING-MODEL-REVIEW §1.2) |

**Consequences.**
- Exclusive fullscreen stays unsupported (matrix says borderless). A mode switch, UAC desktop or display change returns `ACCESS_LOST`: state `lost`, calibration generation bump, re-create with backoff 0.5–5 s.
- Duplication yields a new image only when the desktop changes. A paused or static game gives `WAIT_TIMEOUT` on a healthy session. **Freshness is the age of the last successful acquire cycle (new frame or healthy timeout), not the age of the last changed frame.** Otherwise a paused game would be reported as stale capture.
- The picture is whatever is on that output. If another window covers the minimap area, the structural check (1.5) fails and coverage becomes unavailable; the crop is withheld.
- Multi-adapter machines must create the duplication on the adapter that owns the output. Laptops with hybrid graphics are outside the reference tier and stay "unverified" (CORE-005).
- D2: if V1 shows an FPS failure caused by duplication on a cell, a WGC monitor backend is built for build 26100 only (`MinUpdateInterval` available [W4]); build 19045 then ships Lite or telemetry-only and says so.

### 1.4 Decision D3/D4: frame path to the second monitor

Two consumers read one latest-value slot (capacity 1, overwrite; SAD §6 bounded queues):

```
capture worker (BELOW_NORMAL) ──► frame slot ──► display transport ──► WebGL2 quad in the map box   (no inference)
                                      └────────► analysis worker (rate ≤ analysis Hz) ──► sightings ──► tracker ──► read model / alert path
```

**A smooth picture needs zero CV inference.** Markers are a separate feature with a separate rate, budget and evidence class (VISION-003, QUALITY-004). A slow or absent detector never slows or hides the picture.

Frame message (PROPOSED, to freeze in ICD): 32-byte header `{magic, version, width, height, stride, session_epoch, source_generation, calibration_generation, frame_seq, capture_time_us, flags}` + BGRA rows. The renderer uploads it as one texture and swizzles BGRA in the fragment shader (no CPU colour conversion). It draws a frame only if all three generations equal the current read-model generations (UX-SPEC §4 "Live markers require matching frame, session, source generation").

Size: side S pixels gives S²×4 bytes. If S ≈ 280 (UNVERIFIED) a frame is about 314 KB and 30 fps is about 9.4 MB/s. The box on a 1080p second monitor is 1024 px (DESIGN-NOTES §2), so the picture is magnified about 3.7 times and will look soft. That is a property of the source, not of the transport; "extra large minimap" in the game reduces it. Sharpening or super-resolution is not in scope.

| Transport | CPU copies per frame | New native surface | HTML markers and banner over the picture | Risk | Verdict |
| --- | --- | --- | --- | --- | --- |
| T0 Tauri `Channel` with raw bytes | 2 to 4 inside WebView2 IPC (UNVERIFIED) + 1 GPU upload | None | Yes | Lowest build risk; cost unknown | **Build first** |
| T1 WebView2 shared buffer + seqlock, read on `requestAnimationFrame` | 1 host write + 1 GPU upload | COM calls through the wry webview handle | Yes | Tauri access UNVERIFIED | **Use if T0 misses its allocation** |
| T2 Native child window with its own swap chain (GPU to GPU) | 0 | A second HWND tracked to the map box | **No**: airspace; markers, banner and badge would need native drawing; the 1 CSS px rule and focus order get harder | Highest | Escape only; needs a new decision |
| Custom-protocol MJPEG or WebRTC/WebCodecs stream | Encode + decode | None | Yes | Encoder CPU inside a 10% cap; added latency | Rejected |
| JSON or base64 events | Several, plus parsing | None | Yes | Far over budget | Rejected |

**CPU allocation inside the 10% cap (PROPOSED engineering allocation, fixed before any measurement; percent of total CPU, 12 logical processors).** These are not acceptance thresholds and do not replace QUALITY-002.

| Consumer | Allocation | Note |
| --- | --- | --- |
| Intake, events, alert, audio | 1.0 | Never throttled (SDD §14) |
| Capture worker (acquire, region copy, readback) | 1.0 | About 4 ms of one core per frame at 30 fps |
| Transport + WebView processes drawing the map | 2.5 | Renderer and GPU process are in the job |
| Rest of the deck UI | 1.0 | Routine updates never animate (DESIGN-NOTES §5) |
| Analysis (detector + tracker), when enabled | 2.5 | At the analysis rate, not 30 fps |
| Peer and account worker | 0.2 | |
| Margin below the 9.0% throttle entry | 0.8 | Sum = 9.0 |

RAM allocation (PROPOSED): frame path ≤40 MB including textures; detector ≤150 MB Working Set. The WebView dominates RAM use (the earlier 900 MB ceiling is withdrawn, DEC-RESOURCE-BUDGET); that is measured, not assumed.

**Selection rule for V2 (fixed now).** Choose the first of T0, T1 whose transport + WebView cost at 30 fps stays within 2.5% with capture-to-present age p95 ≤100 ms on the reference cell. If neither does at 30 fps, the default display rate becomes the highest of 15 or 10 fps that passes, the badge shows the real rate, and the owner is told that the 30 fps target was not met. T2 is opened only by a new decision.

### 1.5 Decision D5: calibration (VISION-001)

| Step | What happens | Output |
| --- | --- | --- |
| 1 Locate | Find the game's top-level window (process name and window class through Win32; no game memory), its monitor, output size and DPI | `output_id`, size, DPI |
| 2 Candidate | Look up a rectangle in a profile table keyed by output size, aspect and minimap option. The table is measured in spike V3 and stored with provenance (source stills, game patch, date) | Candidate rect, `profile_id` |
| 3 Structural check | On ≥30 frames: square region at the expected screen edge; stable border; background changes far less than the 3D scene; passes without any game asset | pass / fail |
| 4 Confirm once | In detail mode, outside a live fight, the user sees the dimmed candidate with corner brackets and confirms, or drags the corners (manual path) | `method = profile \| manual` |
| 5 Record | `{output_id, output_size, dpi, rect, profile_id, method, calibration_generation, validated_at}` in settings (atomic write) | `validated` |

- **Invalidation** (each bumps `calibration_generation`, rejects frames in flight, state `recalibration_needed`): output size, DPI or monitor change; game window rect or monitor change; `ACCESS_LOST`; structural check failing for 60 consecutive analysis frames; the user's action. Old frames and old coordinates are never drawn (AC-VISION-001).
- Steps 1 to 3 rerun automatically. If the stored record still matches and step 3 passes, no new confirmation is asked.
- **A crop is displayed only in state `validated`.** During calibration the candidate appears only inside the calibration step. This closes DESIGN-NOTES §11 item 9 (PROPOSED; the owner may ask for a different flow).
- Outside the certified cell: manual calibration is allowed and the picture is labelled "unverified layout" (CORE-005). Markers, missing-enemy state, risk and vision audio stay off there.
- The structural check uses no Valve artwork. Any template or reference image derived from game assets is DEC-GAME-DATA provenance and OWNER (rights).

### 1.6 States, rates and the governor

Runtime status needs capture sub-states (DESIGN-NOTES §11 item 12). PROPOSED for `runtime-status` 1.2.0, frozen in S4-0:

| `capture.phase` | Entry condition | Deck presentation (DESIGN-NOTES §3.1) |
| --- | --- | --- |
| `live` | Validated calibration; last acquire cycle ≤2,000 ms old; rate = target | Solid frame; badge shows measured rate and frame age |
| `lite` | Same, but rate below target: user setting (5 fps) or governor | Dashed frame; badge shows measured rate |
| `stale` | Last acquire cycle >2,000 ms old | Dimmed, hatched, markers removed, age shown |
| `coverage_unavailable` | >5,000 ms without a valid frame (VISION-002) | "No visibility data — not safe"; tracker stops asserting |
| `calibrating` / `recalibration_needed` | Section 1.5 | Brackets or plate; no markers |
| `lost` | Duplication cannot be created or is lost | Plate; retry with backoff |
| `unsupported` | Exclusive fullscreen, unsupported OS, no game output | Labelled decorative diagram |

- 2,000 ms reuses the product's single freshness boundary (tests at 2,000/2,001 ms, as DATA-001 and PEER-005). 5,000 ms is the baseline (VISION-002).
- **Governor interaction.** Exhausting the hard cap stops every thread in the job, including audio and alert threads [W5]. So the video path must never push the job to the cap. Rules: (a) the capture worker asks the governor before each cycle and skips the cycle instead of queueing; (b) on `throttled` (SDD §14: one sample ≥9.0%) the display rate halves 30 → 15 → 7.5 → 3.75 → 2 fps floor and returns step by step after three samples ≤8.0%; (c) the analysis rate halves before the display rate does, down to the minimum in 1.7; (d) critical threads keep `ABOVE_NORMAL`. The badge always states the measured rate; "Live 30 fps" is never printed from a constant.
- The scheduling interval of the cap is UNVERIFIED; spike V1 records the worst audio-thread stall seen while the video path runs at 30 fps.

### 1.7 Decision D6: marker and CV scope

| Stage | Capability | Requirements | Model needed | Earliest |
| --- | --- | --- | --- | --- |
| M3a | Live picture, calibration, honest states | VISION-001, VISION-004, UI-002 (map), UI-008 (stale) | None | Sprint 4 |
| M3b | Hero-marker **presence** with position; tracker; seen / missing / never seen; "seen, class not certified" | DATA-005, VISION-002, ALERT-006, QUALITY-004 presence metric | Detector configuration from V4 | Sprint 5 |
| M3c | **Identity** for the certified class list; risk score; audible gank path | VISION-003, QUALITY-004 identity, ALERT-002 to 005, QUALITY-005 | Classifier per certified class | Sprints 5–6 |
| Later | More classes; other layouts and resolutions | CORE-005, QUALITY-008 | Retraining and new evidence | After M4a |

- Ally rings are drawn only for sharing teammates found in the frame; a peer never supplies a position (DESIGN-NOTES §9; PEER-003).
- **Slot assignment problem.** Five stable enemy slots and "once per hero per episode" (ALERT-006) need a stable key per enemy. Without certified identity the tracker can only hold anonymous tracks. Options for V4 to evaluate: identity classes; or session-local re-identification (same icon seen again in this match, no hero name asserted). Decide after V4; until then enemy slots show counts and unassigned sightings.
- **Honest sizing of certification.** QUALITY-004 needs ≥50 independent appearance episodes per class across ≥5 held-out matches and a holdout of ≥20 whole matches (FROM-REPO NFR). With well over a hundred heroes this is hundreds of labelled matches. The first certified list will be small, and every other hero stays "seen · unverified". This is the baseline's rule, not a new limit.
- **Latency link.** For a vision-originated critical alert T0 is the time the acquisition cycle becomes due (NFR). With the 300 ms hard maximum, analysis period + inference + decision + audio must fit: analysis ≥10 Hz with inference ≤50 ms is the working constraint (PROPOSED; S3 latency receipts give the audio share). The governor may not drop analysis below 10 Hz while the audible gank path is advertised as available; below that the path reports unavailable.

**DEC-MODELS gate for vision (owner role: engineering lead; decided here as the gate, not the model).** A detector configuration is admitted only with all of: licence compatible with distribution; reproducible artifact and label hashes; at least two configurations benchmarked on the same owner-recorded holdout (A: classical colour/shape plus template; B: small CNN on CPU); presence P/R ≥95%/90%; per-class identity P/R ≥95%/85% with 95% intervals for each class it claims; analysis within 2.5% CPU at ≥10 Hz and ≤150 MB; whole-match split. Failing any item keeps the capability unavailable. No threshold moves after a failed run (AGENTS.md).

**Update 2026-11-13 (DEC-RESOURCE-BUDGET, owner statement relayed by the coordinating session).** The owner allows GPU processing for the vision path itself, capture post-processing and local model inference, and prefers it to CPU because it is faster and keeps CPU free. DEC-MODELS is therefore free to choose a GPU-based inference runtime (for example DirectML or a similar one); it remains open and every admission item above applies unchanged. The CPU cap of 10 percent of normalized CPU with one-second samples, the reporting rules and no RAM or VRAM limit exists (the owner will set one from the first measured baseline); RAM and VRAM are measured and reported per process. A configuration that runs on the GPU is measured on both: GPU use does not excuse CPU or RAM time, and unmeasured VRAM is unknown. Capture stays DXGI Desktop Duplication (D1): the GPU is used after capture, not instead of it. No new requirement follows from this note; the `analysis within 2.5% CPU` item keeps its meaning as a CPU measurement, and any GPU-time or VRAM threshold for a detector needs its own owner decision before a run can pass or fail against it.

### 1.8 Evidence and spikes before commitment

| Spike | Question | Method | Commit rule | Class |
| --- | --- | --- | --- | --- |
| V1 capture | Cost of region-only duplication; FPS loss; `ACCESS_LOST` and timeout behaviour; audio stall under cap | Stand-alone binary in `spikes/capture`; owner runs ≥3 pairs of ≥60 s, capture off/on, same replay segment | Proceed with D1 if every pair ≤3% and capture ≤1.0% CPU; else D2 or Lite default | Native observation + owner FPS runs |
| V2 transport | T0 versus T1 at 30 fps for S = 256 and 512 | Synthetic frame source inside the real app shell; job CPU, Working Set, present age | Rule in 1.4 | Native observation |
| V3 geometry | Rect table for the certified cell and the two minimap options; marker rendering styles | Owner stills at each setting; measured by hand, stored with provenance | Table has ≥1 validated profile | Native observation |
| V4 detector | Section 1.7 gate | Recorder tool, labelling protocol, scorer | Gate in 1.7 | Synthetic scorer + live recordings |
| V5 overlay | Transparent, topmost, click-through, no-activate window; FPS loss with overlay on | Section 4 | Every pair ≤3% | Native observation + owner FPS runs |

**Owner-only parts of ADR-V.** FPS, resource and acoustic runs on the reference hardware (DEC-NATIVE-EVIDENCE); recording own matches for the dataset and the labelling effort; certified class list (DEC-GAME-DATA); rights for any game-derived template; accepting a default rate below 30 fps; any layout beyond the certified cell.

## 2. ADR-P: peer sharing runtime

### 2.1 Context

DEC-SHARE-LIST and DEC-ACCOUNT-IDENTITY are decided by delegation; their requirement amendments (PEER-001/004/008/009, PEER-011 to 016, SOCIAL-001/002/004/005, ACCOUNT-001 to 010) are queued and **not yet in the SRS** (FROM-REPO DECISION-LOG). Existing contracts still describe rooms: `contracts/peer-sync.schema.json` has `room_generation` and a raw `team` (FROM-REPO). Nothing is hosted until DEC-PRIVACY-LEGAL, DEC-VALVE-POLICY and DEC-ACCOUNT-ORIGIN close.

### 2.2 Decision D7: client engine

New pure crate `crates/kohbor-peer` (injected clock, injected transport, no I/O), driven by one host worker. It never sees the full snapshot: the host builds the two closed DTOs and passes them in (SDD §13: "CMP-PEER performs its own narrower publication gate").

| Concern | Design |
| --- | --- |
| Device key | X25519 key pair per installation and account, created at sign-in; private half under DPAPI current-user (same method as GSI secrets, FROM-REPO `secrets.rs`); public half registered with the broker |
| Pair secret (review rule 24) | `HKDF-SHA256(ikm = X25519(my_private, friend_public), salt = pairing_id, info = "kohbor-pair-secret-v1")`, 32 bytes. Derived on both clients; never sent; not stored (re-derived from the pinned friend key). New label `pair_secret_domain` for `product-identity.json` |
| Friend key trust | The broker distributes friend public keys. The client pins the key at the first-time sheet. A key change (friend's new device) re-derives only after the friend's side has confirmed (PEER-016). Residual: an operator who substitutes keys can learn bindings; stated in the privacy notice |
| Blind binding (PEER-012) | `bind = HMAC-SHA256(pair_secret, "kohbor-peer-hash-v1" ‖ 0x00 ‖ decimal(match_id) ‖ 0x00 ‖ team)`. The label already exists in `product-identity.json` (FROM-REPO). The relay compares for equality; it never receives match id, team or SteamID |
| Admission per sample (PEER-002) | App 570, playing, owner verified, positive match id, exact team, pregame or in-progress, every DTO field present. Any failure withdraws publication at once and announces a null binding. Re-evaluated on every admitted GSI tick |
| Vitals DTO (PEER-003) | Exactly `contracts/peer-sample.schema.json`. No other field, ever |
| Abilities DTO (PEER-010) | Exactly `contracts/peer-abilities.schema.json`, only for a pairing whose `abilities` scope the sender granted |
| Per-recipient scope | The sender chooses the DTO set per pairing; the relay enforces the same from the ticket. Two independent checks |
| Announce | When the binding changes, and every 20 s: `{(pairing_id, pairing_generation, bind)}` for every mutual, unpaused entry. Request ≤4,096 bytes, so the mutual list is capped at 32 entries (PROPOSED value for DEC-SOCIAL). The reply lists pairings whose counterpart holds an equal binding. An announce expires on the relay after 30 s (PEER-004) |
| Sync (PEER-006) | One sequential request in flight; 500 ms cadence; 1,500 ms timeout; reply ≤32 KiB; retry backoff 0.5–5 s; after a failure a successful null probe precedes any sample. Carries my sample once with the recipient pairings and returns theirs. No task per game tick |
| Receiving age (PEER-005) | `age = source_age + sender hold + relay dwell + full round trip + local elapsed`, all monotonic durations. ≤2,000 ms is live; 2,001 ms is null. The UI timer expires data even if events stop. Polling never rejuvenates a sample |
| Leave (PEER-008 extended) | Global pause, per-friend pause, scope withdrawal, removal, block, sign-out, account switch, match end, team or match change, new-device pause: each bumps `pairing_generation`, clears received state locally at once, stops sending at once, fences late replies. The other side clears by the 2,000 ms rule; remote erasure is never promised |
| Tripwires | More than four active pairings in one binding; one pairing presenting two different bindings inside one match. Either suspends the affected pairings for that match. A tripwire is not team verification (G1 stays open) |
| Custody (PEER-007) | Tokens, tickets, keys and pairing ids live only in the host. One pinned HTTPS origin from build configuration; no redirects, URL credentials, query or fragment. Literal loopback HTTP only in a build with the `dev-social` feature. The renderer gets display state only |
| No persistence (PEER-009) | Samples live in memory in the engine and the read model only. Not written to journal, log, memory, advice or crash output. A route test asserts denial; an egress test compares outbound bytes with the allow-list |

Contract work for the integration owner (S10-0, R2): `peer-sync` 2.0.0 (drop `room_generation` and raw `team`; add `pairing_generation`), new `peer-announce`, `relay-ticket`, `share-list` schemas, labels `pair_secret_domain`, `account_token_domain`, `relay_ticket_domain`, `friend_code_prefix`.

| Alternative | Why rejected |
| --- | --- |
| Server-generated pair secret | The operator could test public match ids and recover who plays which match (review rule 24). Kept only as the documented fallback |
| Rooms and codes | Replaced by DEC-SHARE-LIST; guest codes are not in the first release (DEC-GUEST-PEER) |
| Peer-to-peer transport | NAT traversal, IP disclosure between players, no single point for scope enforcement |
| WebSocket push | PEER-006 fixes one sequential request and a 500 ms cadence. It would cut request cost about twenty-fold on one priced platform (section 2.4) but needs a requirement change: **PROPOSAL-OWNER**, not adopted |
| End-to-end encryption of samples | Possible with the pair secret; the baseline says "do not claim end-to-end encryption" (SECURITY-PRIVACY). **PROPOSAL-OWNER** for later; it would let the notice say the operator cannot read samples |

### 2.3 Decision D8: server architecture (design only)

```
Google (OIDC) ──ID token──► client ──ID token + device key──► ACCOUNT BROKER ──signed relay ticket──► client ──ticket + bindings + samples──► RELAY
                                                   (database)                                                  (memory only)
```

| Component | Does | Stores | Never stores or receives |
| --- | --- | --- | --- |
| Account broker (one deployable; friend-code service is a module of it) | Verifies the Google ID token (`aud` = Kohbor client id, `iss`, signature, `exp`, nonce); issues sessions; friend codes; friendships; blocks; Share list entries; device keys; relay tickets; export; delete | Review rule 26 list: login-identity hash, account id, friend code, display name, settings, device public keys, session records (hashed refresh tokens), friendships, pending requests, blocks, Share list entries (scopes, pause, last confirmed), pairing ids, rate counters | Email, raw Google subject, Google tokens, Steam id, IP history, presence history, match ids, bindings, pairing activity times, samples |
| Relay (one deployable, stateless across restarts) | Verifies tickets offline with the broker's public key; holds announces and latest samples per pairing side for ≤30 s; releases a sample only on equal bindings and granted scope; rate limits | Memory only: `pairing_id → {side, bind, generation, latest DTOs, monotonic receipt time}` | Account token, account id, friend code, display name, match id, team; nothing on disk; no sample in logs |
| Google | Identity only, scope `openid` | n/a | Nothing from Kohbor |

| Token | Issuer label | Audience | Lifetime (PROPOSED) | Holder |
| --- | --- | --- | --- | --- |
| Google ID token | Google | Kohbor OAuth client id | One use, then discarded | Client → broker once |
| Access token | `account_token_domain` label | `broker` | 15 min (review rule 12) | Host only |
| Refresh token | opaque, rotating, device-bound | `broker` | 30 days idle | Host only, DPAPI |
| Relay ticket | `relay_ticket_domain` label | `relay` | 10 min | Host only; contains pairing ids, scopes each way, device-key thumbprint, random subject |

- Signatures are Ed25519; the relay holds only the public key (review rule 25). Issuer strings are labels, not domains: no origin is inferred from the product name (AGENTS.md "Identity").
- ACCOUNT-007 (proposed): each token has exactly one audience; broker and relay reject any other audience or issuer.
- Trust boundaries: renderer ↔ host (IPC, typed, no tokens); host ↔ broker (TLS, pinned origin); host ↔ relay (TLS, pinned origin, ticket); broker ↔ relay (no runtime call; public key only).
- Revocation speed: the client side acts at once (stops sending, discards receipts). The broker stops issuing tickets for the pairing; an outstanding ticket lives ≤10 min, but the counterpart's announce no longer matches, so the relay releases nothing.

### 2.4 Operating cost options (choice is OWNER: DEC-ACCOUNT-ORIGIN)

Load fact: the baseline cadence is 2 sync requests per second per player in a match, which is 7,200 requests per player-hour (derived from PEER-006).

| Option | Unit prices | What it means | Label |
| --- | --- | --- | --- |
| A. Serverless functions + in-memory objects (priced example: Cloudflare Workers + Durable Objects) | Free: 100,000 requests/day; paid: US$5/month minimum with 10M requests, then US$0.30/M; Durable Object requests US$0.15/M after 1M | Free tier covers about 13.9 player-hours per day in total. At 5,000 player-hours/month (36M requests) about US$18/month by my arithmetic. Needs a port of the relay to that runtime | Prices VERIFIED [W8]; arithmetic mine; fit UNVERIFIED |
| B. One small virtual machine running broker + relay as one Rust binary with SQLite | Flat monthly price | Predictable cost; the owner patches, renews TLS, monitors and backs up | Price UNVERIFIED |
| C. Managed backend-as-a-service for the broker + option A or B for the relay | Free and paid tiers | Less code; ACCOUNT-REVIEW found one such service cannot separate token audiences per application (its item 4, UNVERIFIED) | UNVERIFIED |
| D. No hosted service in the first release | Zero | Client ships with the account surface "service pending"; everything local works | Matches every safe-interim in `decisions.json` |

Recommended default: **D now, B when the gates open.** Money is small at this scale; the real cost is operations and the legal gate. Any paid service is an owner decision (AGENTS.md).

### 2.5 Decision D9: local fake for development and tests

- `crates/kohbor-social-core` (pure state machine + storage trait): broker and relay logic, written once.
- `crates/kohbor-social-fake` (dev-only binary): `social-core` with in-memory storage, on literal loopback HTTP, plus a **fake identity provider** that signs ID tokens with a test key generated at start. No real credential is used or needed.
- The client accepts a loopback origin and the fake issuer only when built with the `dev-social` feature. A release build has no such path; a test asserts that.
- Tests: two `kohbor-peer` engines against the fake in one process (synthetic); then over real loopback sockets (socket-integration class). Two-device native evidence stays an owner run (SDP M6 exit).
- A production server built from `social-core` is **PROPOSAL-OWNER**: PRD §4 lists optional account services as an external dependency that is "not automatically desktop MVP scope".

**Evidence needed for ADR-P.** Known-answer vectors for HKDF and HMAC; 2,000/2,001 ms age tests; leave-race tests for every leave trigger; allow-list egress capture; fake-relay restart; ticket audience and expiry negatives; later, two-device runs. **Owner-only:** DEC-PRIVACY-LEGAL, DEC-VALVE-POLICY, DEC-ACCOUNT-ORIGIN, DEC-SHARE-RECONFIRM (90 days proposed), the first-time sheet wording after a comprehension test, the G1 spike (DEC-TEAM-VERIFY), confirmation of the queued requirement amendments.

## 3. ADR-A: account and session client (design only until a service exists)

**Decision.** New pure crate `crates/kohbor-account` (state machine, PKCE, token rotation, injected clock and HTTP) plus host glue for the browser hand-off, the loopback listener and DPAPI. The account worker is separate from intake, alert and audio and is never awaited by them (CORE-001, ALERT-001).

| Flow | Steps | Failure behaviour |
| --- | --- | --- |
| Sign-in (ACCOUNT-001) | Not offered during a live match. Host asks the broker for a nonce bound to the device key; binds `127.0.0.1` on an ephemeral port; opens the system browser to Google with PKCE S256, `state`, nonce, scope `openid`; accepts exactly one request with a matching `state`; closes the listener; exchanges the code; sends the ID token and device public key to the broker; discards Google tokens; stores the session | 5 min timeout (PROPOSED) or Cancel returns to `signed_out`; nothing stored; the game is never focused away |
| Custody (ACCOUNT-005) | Refresh token, device private key and cached account data as DPAPI current-user blobs under the app data directory, written atomically. Access token in memory only. Never in the renderer, a URL or a log | Unreadable blob → `signed_out` with a sanitized reason; local core unaffected. Honest limit: DPAPI does not stop malware running as the same Windows user |
| Offline (ACCOUNT-006) | Start never waits for the network. With a stored session and no network: `signed_in_offline`; cached friend list read-only; no sending or receiving; no dialog; refresh retried with backoff 5 s to 5 min off the critical path | Service error → `service_unavailable` chip only |
| Rotation | Refresh rotates on use; a reused old token revokes the session (`session_revoked`) | The user signs in again; Share list entries stay paused until confirmed (PEER-016) |
| Sign-out / switch (CORE-003) | Bump account generation first; fence late replies; engine leave for every pairing; best-effort revoke call; wipe every blob and cache | Local gameplay, history and settings untouched; an old account's reply can never populate the new one |
| Delete (ACCOUNT-008) | Fresh Google sign-in within 5 min; broker deletes every row; client wipes local cache | If the call fails nothing is wiped; the user is told it did not complete |
| Export | Own profile, friend list, Share list settings, block list as JSON to a path the user picks | n/a |

UI states to draw and bind to the fake (extends the `account` domain of runtime status): `signed_out`, `browser_handoff`, `signing_in`, `needs_display_name`, `signed_in_online`, `signed_in_offline`, `service_unavailable`, `session_revoked`, `paused_new_device`, `deleting`. Copy rules from ACCOUNT-IDENTITY-REVIEW §5 apply ("Signing in shares nothing"; one wording for every failed friend request).

| Alternative | Why rejected |
| --- | --- |
| Embedded webview sign-in | Google requires the system browser for desktop apps (ACCOUNT-IDENTITY-REVIEW rule 1, VERIFIED there) |
| Custom URI scheme redirect | No longer supported by Google for this flow (same source); the reserved `kohbor` scheme stays unregistered |
| Windows Credential Manager | Workable; DPAPI is already implemented and reviewed in this repo. One mechanism is simpler |
| Tokens in the renderer | PEER-007 pattern forbids it |

**Evidence needed.** State-machine tests against the fake provider (state mismatch, replayed code, timeout, cancel, rotation reuse, offline start, wipe on sign-out, switch fences); a native check that the listener is closed outside the flow; one real Google sign-in in a Kohbor-owned project (UNVERIFIED items 1 to 3 of ACCOUNT-IDENTITY-REVIEW §6). **Owner-only:** DEC-GOOGLE-PROJECT (project, consent screen, verification), DEC-ACCOUNT-ORIGIN, session and limit values, the legal items.

## 4. Remaining baseline modules

| Module (component) | Placement | Depends on | Risk | Evidence class | Blocking decisions | Sprint |
| --- | --- | --- | --- | --- | --- | --- |
| Capture and calibration (CMP-VISION) | `crates/kohbor-capture` (pure state machine, trait) + `crates/kohbor-capture-native` (Desktop Duplication), same split as audio | S3-5 host path; spikes V1 to V3 | R2 (writes settings, new native surface) | Synthetic + native + owner FPS | DEC-NATIVE-EVIDENCE | 4 |
| Tracker and missing detector (CMP-VISION; VISION-002, DATA-005, ALERT-006) | `crates/kohbor-vision` (pure; consumes sightings, emits episodes) | Capture; detector from V4 | R2 (observation boundary) | Synthetic; live recordings | DEC-MODELS, DEC-GAME-DATA | 5 |
| Risk and alert policy (CMP-RISK, CMP-ALERT) | `crates/kohbor-risk` (pure formula and latch); admission wired into the S3-5 critical path | Tracker output type only (can be tested on synthetic sightings now) | R1 pure, R2 at the critical path | Synthetic golden; acoustic later | DEC-RISK-DETAIL (ALERT-003), DEC-CRITICAL-MEDIA | 6 |
| Session Monitor (CMP-READMODEL; UI-002 to 006, 008) | `apps/desktop/src/**`; native read model in host | Own snapshot (exists), capture phase, tracker, peer state | R1 | Browser interaction; native geometry | DEC-VISUALS measurements; deck scaling (section 6, O-12) | 4–7 |
| Overlay window (UI-001) | Second Tauri window: transparent, topmost, click-through, never activated; banner stage 720×220 driven by the audio owner (MEDIA-005) | Audio owner (exists), event banner (exists on mocks) | R2 (capability files, window flags) | Native + owner FPS pairs | DEC-VISUALS, DEC-INTERFACES | 7 |
| Settings and overlay profiles | Existing `settings.rs` + `settings-patch` contract; new categories capture, overlay modules, audio, sharing | Contract change (R2) | R1 | Synthetic | DEC-INTERFACES | 4, 7 |
| History, memory, insights (CMP-MEMORY; MEM-001 to 005) | `crates/kohbor-memory`: JSONL journal + atomic versioned snapshot (SAD §7); no database | Events (exist), audio outcomes (exist) | R2 (persistence, forget) | Synthetic incl. crash and restart | DEC-SESSION, DEC-RETENTION | 13a |
| Store and entitlement (CMP-ENTITLEMENT) | Destination stays visible with an honest "not in this build" state (UX-SPEC §2). No code beyond that | n/a | R0 | Browser | DEC-PREMIUM (deferred) | Not scheduled (M7) |
| Voice, STT, answer (CMP-VOICE) | **Placement only:** an owned sidecar process inside the job (counted, killable on cancel), fed by a bounded PCM pipe; output through the one audio owner at Normal priority | Model feasibility spike | R2 | Benchmarks, native, acoustic | DEC-MODELS, DEC-VOICE-BUFFER | 13b |
| Manual advice (CMP-ADVICE) | Local first; cloud projection is a separate closed DTO | Voice sidecar or local backend | R2 for the cloud route | Synthetic + egress capture | DEC-MODELS | 13b / M6 |
| OBS highlights (CMP-RECORDER) | As SPRINT-ROADMAP Sprint 8, unchanged | Events; OBS | R1 | Native OBS | DEC-OBS-BUDGET | 9 |
| Peer engine, account client (CMP-PEER, CMP-SOCIAL) | Sections 2 and 3 | Fake server | R2 | Synthetic, socket integration | Section 2 list | 10, 11 |
| Installer and update (CMP-UPDATER) | Tauri bundle for a local unsigned reviewable artifact; updater disabled while `update_manifest_origin` is null (FROM-REPO `product-identity.json`) | Release environment | R2 | Native install, update, rollback | DEC-NAME (origin part), DEC-PACK-CRYPTO, signing authority (OWNER) | 14 |
| Diagnostics | Runtime status + sanitized local log with reason codes only. No upload, no automatic export (SECURITY-PRIVACY). A user-started export bundle is **PROPOSAL-OWNER** | Runtime status | R1 | Synthetic | None | Ongoing |
| Resource governance (CMP-STARTUP, CMP-PERFORMANCE) | Exists (SDD §14); add capture and analysis rate control (1.6) and per-consumer counters | Capture | R2 | Native resource runs | DEC-NATIVE-EVIDENCE | 4 |
| Native evidence runs | Owner machine; receipts per `contracts/evidence.schema.json`, bound to revision and artifact hash | Each slice | n/a | Native, live, acoustic, FPS | DEC-NATIVE-EVIDENCE | Every sprint exit |

Overlay notes: Tauri has `set_ignore_cursor_events`; one public issue reports it not passing clicks through on Windows 10 (VERIFIED that the report exists [W9]; cause UNVERIFIED), so V5 tests the window flags on both OS cells before any commitment. A topmost transparent window over a borderless game may cost FPS (UNVERIFIED); that is what QUALITY-003's overlay-off/on pairs measure. Duplication captures the overlay too, so no overlay module may cover the minimap rectangle (already required by UX-SPEC §8).

## 5. Revised delivery plan

Rules kept from AGENT-WORKFLOW and SPRINT-ROADMAP: sprints have no calendar length; Luna = general worker, Terra = tests and reviews, Lead reads every R2 diff; tests first where a contract exists (T-L-R); one writer per file area; **one worker holds the cargo lane at a time** (AGENT-WORKFLOW §7.3), so the parallelism below is logical: frontend, read-only reviews, documents and owner runs overlap with one cargo writer. Sprints 0 to 3 stay as recorded. The roadmap's old Sprints 4 to 8 map to 4 to 9 here.

### Sprint 4 — Live minimap picture (M3a)

**Goal.** The real minimap appears on the Command Deck at a measured rate with honest states. No detection. Requirements: VISION-001, VISION-004, UI-002 (map region), UI-008 (stale part), CORE-005.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S4-0 | Contracts: runtime-status 1.2.0 capture phases and reason codes; frame header and calibration record in ICD; settings categories | Lead | L | R2 | source |
| S4-1 | Spike V1 in `spikes/capture`; written result with recommendation | Luna | L | R0 | native observation |
| S4-2 | Spike V2 transports T0 and T1 with a synthetic source | Luna | L | R0 | native observation |
| S4-3 | Spike V3: measure the rect table from owner stills; provenance file | Luna | L | R0 | native observation |
| S4-4 | Tests with a fake frame source: generation fences, healthy timeout is not stale, 2,000/2,001 ms, 5,000 ms coverage, invalidation on size, DPI, monitor and layout change, crop withheld unless validated | Terra | T-L-R 1 | R2 | synthetic |
| S4-5 | `kohbor-capture`: state machine, calibration, governor hooks | Luna | T-L-R 2 | R2 | synthetic |
| S4-6 | `kohbor-capture-native`: duplication backend, region copy | Luna | L-R | R1 | native observation |
| S4-7 | Host: capture worker, frame slot, chosen transport, status | Luna | L-R | R2 | synthetic |
| S4-8 | Frontend: Live deck on mocks: map canvas, ten map states, columns, 1 CSS px invariance test (can start before S4-1) | Luna, Terra tests | L-R | R1 | browser |
| S4-9 | Frontend: bind the real frame transport and the calibration step | Luna | L-R | R1 | browser + native observation |
| S4-10 | Review of S4-5, S4-7 | Terra, Lead | T-L-R 3 | R2 | source |

- **Owner input.** FPS pairs and resource runs for V1 and the final build; stills for V3; acceptance of the default rate if 30 fps is not met; O-12.
- **Exit gate.** State tests pass. Each FPS pair is recorded PASS or FAIL per pair. The badge shows the measured rate. QUALITY-003 stays NOT_RUN for the overlay until Sprint 7.

### Sprint 5 — Sightings and the observation boundary (M3b)

Requirements: DATA-005, DATA-006, VISION-002, VISION-003, ALERT-006, QUALITY-004.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S5-1 | Dev recorder: user-started saving of validated crops with frame headers to a folder outside the repository (dev tool, like `kohbor-gsi-capture`; not shipped) | Luna | L-R | R1 | native observation |
| S5-2 | Labelling protocol, annotation format, scorer with whole-match split, per-class P/R and 95% intervals | Terra | L-R | R1 | synthetic |
| S5-3 | Spike V4: two detector configurations on the owner dataset; DEC-MODELS record | Luna | L | R0 | live recordings |
| S5-4 | Tracker tests on synthetic sightings: prior presence, continuing coverage, gap gives no missing and no all-clear, strict >5 s, re-arm on continuous reappearance, uncertified class stays unverified | Terra | T-L-R 1 | R2 | synthetic |
| S5-5 | `kohbor-vision` tracker and missing-notice policy | Luna | T-L-R 2 | R2 | synthetic |
| S5-6 | Detector behind a `Detector` trait, analysis worker, rate control (only if V4 passes the gate) | Luna | L-R | R2 | synthetic + recordings |
| S5-7 | Read model and frontend: markers as outlines, enemy column states | Luna | L-R | R1 | browser |
| S5-8 | Review of S5-5, S5-6; the lead reads the observation boundary | Terra, Lead | T-L-R 3 | R2 | source |

- **Owner input.** Recorded matches and labelling; DEC-MODELS acceptance; DEC-GAME-DATA class list and asset provenance.
- **Exit gate.** No enemy field is ever filled without an admitted sighting. If V4 fails the gate, S5-6 and S5-7 do not ship; the deck keeps the picture and reports markers unavailable.

### Sprint 6 — Risk and alert policy (M3c)

Requirements: ALERT-002 to 005, QUALITY-005. S6-1 and S6-2 depend only on a sighting type and may start during Sprint 4.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S6-1 | Golden tests from FEATURE-CATALOG ("Missing-time heuristic", "Hysteresis") and SDD §6: formula, thresholds, episode deduplication, 30 s and 60 s intervals, correction never from time decay | Terra | T-L-R 1 | R1 | synthetic |
| S6-2 | `kohbor-risk` | Luna | T-L-R 2 | R1 | synthetic |
| S6-3 | Draft coordinated-motion admission parameters and positive/negative traces for DEC-RISK-DETAIL | Lead | L | R2 | source |
| S6-4 | Wire admission into the critical path per S3-5-HOST-CRITICAL-PATH | Luna | L-R | R2 | synthetic (engineering fixture) |
| S6-5 | Warning-quality harness over recorded sessions (±8 s window) | Terra | L-R | R1 | synthetic + recordings |
| S6-6 | Review | Terra, Lead | T-L-R 3 | R2 | source |

- **Owner input.** DEC-RISK-DETAIL; DEC-CRITICAL-MEDIA and authorized warning media; labelled sessions; acoustic runs.
- **Exit gate.** Until DEC-RISK-DETAIL closes, the score is informational and audible gank reports unavailable. QUALITY-005 is measured or NOT_RUN.

### Sprint 7 — Overlay, profiles, Session Monitor completion

Requirements: UI-001, UI-003 to 006, UI-008, QUALITY-003.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S7-1 | Spike V5 overlay window flags on both OS cells | Luna | L | R0 | native observation |
| S7-2 | State tests: honest chips, protected regions (minimap rect, skills, shop), focus and Escape order | Terra | T-L-R 1 | R1 | synthetic + browser |
| S7-3 | Overlay window, banner stage, companion chip | Luna | T-L-R 2 | R2 | synthetic |
| S7-4 | Settings: per-module enabled, position, scale, opacity; saved profiles | Luna | L-R | R1 | synthetic |
| S7-5 | Inspector drawer, keyboard paths, glance/detail | Luna | L-R | R1 | browser |
| S7-6 | FPS measurement harness and receipt validator | Terra | L-R | R1 | synthetic |
| S7-7 | Review | Terra, Lead | T-L-R 3 | R2 | source |

- **Owner input.** FPS pairs with overlay on; safe-area measurements (DEC-VISUALS); DEC-INTERFACES for commands.
- **Exit gate (SDP M3).** Geometry, freshness and unknown tests pass; initial native vision and acoustic evidence exists or is NOT_RUN by name.

### Sprints 8 and 9 — First demonstration (M4a) and OBS highlights (M4b)

Unchanged from SPRINT-ROADMAP Sprints 7 and 8. Added to the demo checklist: the capture phase and the measured rate are visible in the evidence recording.

### Sprint 10 — Peer engine on the fake relay (parallel track P)

Requirements: PEER-002, 003, 005 to 010 and the queued PEER-011 to 016.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S10-0 | Apply the queued amendments to the registry as proposed entries; freeze the schemas and labels of 2.2 | Lead | L | R2 | source |
| S10-1 | Tests: HKDF and HMAC known answers; per-sample admission; allow-list DTOs; age 2,000/2,001 ms; every leave trigger with a late reply; null probe; tripwires; persistence route denied | Terra | T-L-R 1 | R2 | synthetic |
| S10-2 | `kohbor-peer` | Luna | T-L-R 2 | R2 | synthetic |
| S10-3 | `kohbor-social-core` relay part + `kohbor-social-fake` | Luna | L-R | R1 | synthetic |
| S10-4 | Two engines over real loopback sockets; outbound byte capture against the allow-list | Terra | L-R | R2 | socket integration |
| S10-5 | Host worker, native custody, read-model projection | Luna | L-R | R2 | synthetic |
| S10-6 | Frontend: ally cards, Share indicator, pause, bound to the engine (mock screens exist in `visuals/command-deck`) | Luna | L-R | R1 | browser |
| S10-7 | Review | Terra, Lead | T-L-R 3 | R2 | source |

- **Owner input.** Confirmation of the queued amendments; DEC-SHARE-RECONFIRM; first-time sheet wording.
- **Exit gate.** No release build can reach a relay: no origin is configured, and a test asserts it.

### Sprint 11 — Account client on the fake broker (parallel track P)

Requirements: CORE-003, SOCIAL-001 to 003 and the queued ACCOUNT-001 to 010.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S11-1 | State-machine tests of section 3 against the fake provider | Terra | T-L-R 1 | R2 | synthetic |
| S11-2 | `kohbor-account`; broker part of `social-core`; fake provider | Luna | T-L-R 2 | R2 | synthetic |
| S11-3 | Host: browser hand-off, loopback listener, DPAPI custody | Luna | L-R | R2 | synthetic + native observation |
| S11-4 | Frontend: account, friends, Share list screens bound to the fake | Luna | L-R | R1 | browser |
| S11-5 | Sign-out, switch, export, delete; row-count deletion test on the fake | Terra, then Luna | T-L-R | R2 | synthetic |
| S11-6 | Review | Terra, Lead | T-L-R 3 | R2 | source |

- **Owner input.** DEC-GOOGLE-PROJECT and one real test sign-in.
- **Exit gate.** With no service configured the account surface says "service pending" and CORE-001 cold-offline tests still pass.

### Sprints 12 to 14 — slice level only

| Sprint | Scope | Entry gate |
| --- | --- | --- |
| 12 | G1 spike (DEC-TEAM-VERIFY); hosted bring-up of broker and relay; two-device evidence | OWNER: DEC-ACCOUNT-ORIGIN, DEC-PRIVACY-LEGAL, DEC-VALVE-POLICY, DEC-GOOGLE-PROJECT. Not planned further until they close |
| 13a | Memory and history (MEM-001 to 005): tests first on journal, snapshot, forget cutoff | DEC-SESSION, DEC-RETENTION. No model dependency; may run beside Sprints 5 to 7 |
| 13b | Voice, STT, answer, manual advice (M5) | Model feasibility spike and DEC-MODELS, DEC-VOICE-BUFFER; replanned after the spike |
| 14 | Installer, updater, release candidate (M8) | Update origin, signing authority and DEC-PACK-CRYPTO: OWNER |

### Execution DAG

```
MAIN (cargo lane, critical path marked ═)
 S3-5 host critical path ═► S4-1 V1 ═► S4-5/6/7 capture ═► S5-1/3 dataset + V4 ═► S5-5/6 tracker+detector ═► S6-4 alert wiring ═► S7-3 overlay ═► S8 demo ─► S9 OBS
                             S4-2 V2 ─┘                      ▲ owner matches,                                  ▲ DEC-RISK-DETAIL,
                             S4-3 V3 ─┘ (owner stills)       │ DEC-MODELS, DEC-GAME-DATA                       │ DEC-CRITICAL-MEDIA

PARALLEL, no dependency on capture
 F  frontend:   S4-8 deck on mocks ─► S4-9 bind ─► S5-7 markers ─► S7-5 inspector          (one frontend writer)
 R  pure logic: S6-1 ─► S6-2 risk                                                           (any time after S4-0)
 P  social:     S10-0 ─► S10-1..7 peer on fake ─► S11-1..6 account on fake ─► [OWNER gates] ─► S12
 M  memory:     S13a                                                                        (after DEC-SESSION, DEC-RETENTION)
 K  spikes:     V5 overlay (before S7-3) · G1 (owner key) · voice model feasibility (before S13b)
 T  reviews:    read-only Terra reviews run beside any writer
```

Critical path: S3-5 → V1 → capture → dataset and V4 → tracker and detector → alert wiring → overlay → demo. The longest poles are owner-side: recorded and labelled matches for V4, DEC-RISK-DETAIL traces, and authorized critical media. Tracks P, R and M shorten nothing on that path; they use otherwise idle worker time.

## 6. Owner-only decisions

| # | Decision | Recommended default | Blocks |
| --- | --- | --- | --- |
| O-1 | DEC-ACCOUNT-ORIGIN: where broker and relay run, who operates them, any paid service | None hosted now; one small VM later (2.4 B) | Sprint 12; every hosted feature |
| O-2 | DEC-PRIVACY-LEGAL: legal review | Engage a lawyer before any hosting | Any data leaving a device |
| O-3 | DEC-VALVE-POLICY: written position on relaying GSI-derived state | Ask before hosting; stop condition if the answer is no | All sharing |
| O-4 | DEC-TEAM-VERIFY: fund the G1 spike; Steam Web API key | Run it once; close DEC-TEAM-SHARE as rejected if no source exists | Team tier only |
| O-5 | DEC-GOOGLE-PROJECT: Kohbor-owned Google project and verification | Create a test project; one test sign-in | Real sign-in |
| O-6 | Confirm the queued PEER/SOCIAL/ACCOUNT amendments | Confirm as proposed | S10-0 |
| O-7 | DEC-SHARE-RECONFIRM, DEC-GUEST-PEER, DEC-PEER-ECONOMY | 90 days; no guest codes; no economy scope | Share list details |
| O-8 | DEC-CRITICAL-MEDIA: authorized warning media | Commission original lines | Audible critical alerts; M4a |
| O-9 | DEC-GAME-DATA: certified classes, patch catalog, rights for game-derived templates | Start with the classes the owner's dataset can certify | Identity, M3c |
| O-10 | DEC-RISK-DETAIL: coordinated-motion parameters and traces (lead drafts in S6-3) | Approve the draft after replay on recordings | Audible gank |
| O-11 | DEC-NATIVE-EVIDENCE: hardware, testers, time for FPS, resource, acoustic and live runs | The owner's machine as the reference cell, recorded exactly | Every acceptance claim |
| O-12 | Viewport-scaled deck versus the 1420×760 stage (DESIGN-NOTES §11 item 13) | Scaled deck for the Live destination only | S4-8 layout |
| O-13 | Default display rate if 30 fps fails, and the Lite rate | Highest passing of 15 or 10; Lite 5 | Sprint 4 exit |
| O-14 | Recording own matches for the detector dataset (other players' markers are in the crops; local only) | Allow, local, outside the repository | V4 |
| O-15 | PEER-006 transport change to a push connection, if cost ever requires | Keep the baseline | Nothing now |
| O-16 | DEC-SESSION, DEC-RETENTION | Conservative defaults already in `decisions.json` | Sprint 13a |
| O-17 | DEC-PACK-CRYPTO, update origin, signing | Decide before Sprint 14 | Release |
| O-18 | DEC-OBS-BUDGET | Report separately; decide after first measurements | M4b acceptance claim |
| O-19 | Commit authority per sprint; push, merge, release | As today | Integration |

## 7. The three risks most likely to sink the plan

1. **Live picture with markers under the FPS and CPU limits.** Three costs stack on the game: duplication (possible loss of independent flip, UNVERIFIED), a second monitor redrawing a 1024 px canvas 30 times a second, and a topmost overlay. All of it shares a 10% hard cap with the audio and alert threads, and hitting the cap stalls those threads [W5]. Per-class certification needs hundreds of labelled matches. Likely outcome if it goes badly: a picture at 10 to 15 fps, anonymous presence markers, a short certified class list, and build 19045 in Lite. The design keeps each of those as an honest shippable state; it cannot make them unnecessary. Earliest test: V1 and V2 in Sprint 4.
2. **Team verification (G1).** Match and team are self-asserted. No verifiable source is known (FROM-REPO SHARING-MODEL-REVIEW §1.1). The Share list ships with a stated residual risk; team-wide sharing may never ship. A negative written answer from Valve on relaying GSI-derived state would stop even the Share list. Nothing in engineering removes this risk.
3. **Hosted social service.** The money is small at first-release scale (2.4). The load is on one owner: operating two services, key rotation, deletion and backup duties, abuse handling, Google verification and the legal gate. Until those are staffed and decided, sharing exists only against the loopback fake. The plan therefore builds the whole client without the service and lets the first release ship with the account surface marked "service pending".

Also material: DEC-CRITICAL-MEDIA (no authorized warning lines exist; FROM-REPO SPRINT-ROADMAP Sprint 3 owner input) blocks the demo's audible warning whatever the code does.

## 8. Web sources (read 2026-11-12)

| Id | Source |
| --- | --- |
| W1 | https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api |
| W2 | https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture |
| W3 | https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.isborderrequired |
| W4 | https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.minupdateinterval |
| W5 | https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_cpu_rate_control_information |
| W6 | https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2environment12 |
| W7 | https://v2.tauri.app/develop/calling-rust/ |
| W8 | https://developers.cloudflare.com/workers/platform/pricing/ |
| W9 | https://github.com/tauri-apps/tauri/issues/11461 (search-result summary only; the issue page itself was not opened) |

## 9. What I did not verify

- Nothing was built or run: no cargo, no app, no capture, no measurement. Every cost number in section 1 is an allocation or an estimate.
- Independent-flip behaviour under duplication; the cap's scheduling interval; Tauri access to the WebView2 shared buffer; the WGC border for unpackaged apps; minimap size and marker styles in the current game patch.
- The copy count of Tauri's raw IPC path on Windows.
- Prices other than the one platform in W8; W8 itself was read through a summarising tool.
- Whether the public Tauri click-through report (W9) applies to version 2.12.5.
- `docs/design/S3-5-HOST-CRITICAL-PATH.md` appeared while this was being written and was not read; section 5 assumes it defines where vision-originated admission attaches. The lead must check S6-4 and the thread-priority rules in 1.6 against it.
- No legal conclusion is drawn anywhere in this document.
