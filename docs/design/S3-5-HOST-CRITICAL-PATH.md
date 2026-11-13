---
title: "S3-5 Host Critical Warning Path (design and task breakdown)"
doc_id: "S3-5-HOST-CRITICAL-PATH"
status: draft
version: "0.1.1"
updated: "2026-11-13"
authority: "Lead architect decision under owner delegation for SPRINT-ROADMAP S3-5/S3-6; elaborates ALERT-001, ALERT-007, MEDIA-004..008, QUALITY-001 and EVENT-AUDIO-SPEC sections 4 to 7.1. Changes no requirement statement."
approval_status: "agent_decision_not_owner_approved"
---

# S3-5 Host Critical Warning Path

Labels: **FROM-REPO** = read in this checkout (file and symbol or line). **UNVERIFIED** = not checked.
**DECIDED** = decision of this document. Section 12 lists what the owner must still see.

## 0. Facts the design rests on

| # | Fact | Label |
| --- | --- | --- |
| F1 | Intake stamps arrival with `Instant::now()` once the body is read (`intake.rs` `serve`, line 618) and derives `at_ms = duration_ms(arrival - self.started_at)` (`Shared::accept`, line 557). `started_at` is created in `Shared::new` (line 339). | FROM-REPO |
| F2 | Detection runs inside `Shared::accept` under the `Shared` mutex (`lock(&handle).accept`, line 627). The same mutex is taken by `runtime_get_status` and `own_snapshot` (`lib.rs` 53, 59). `status::build` does memory work only. | FROM-REPO |
| F3 | `platform_steamid64()` reads the registry on every request before `accept` (`intake.rs` 116-125, 626). `persist_runtime()` writes a file inside `accept` on `Started`/`OwnerChanged` (line 521). | FROM-REPO |
| F4 | `AudioOwner::request` decides and calls `sink.submit` in one call; `pump_sink_events` handles `DeviceLost` by `set_device_available(false)`; notices are capped at 256 (`owner.rs` 357, 520, 986). The owner has no getter for mute or device state, no `sink_mut`, and always starts playback ids at 1 (`AudioOwner::new`). | FROM-REPO |
| F5 | `WasapiSink::start()` blocks until its render thread reports ready; `Drop` joins that thread; it never reopens after `DeviceLost` and keeps the old endpoint after `DeviceChanged` (`wasapi_backend.rs` 147-204, 247). Hand-off queues: 8 commands, 16 events (`engine.rs` 6-7). | FROM-REPO |
| F6 | `Store::resolve(event_id, tier)` is exact-tier, re-reads and re-hashes descriptor and both files on every call, and returns absolute paths (`store.rs` 890-975). The store has no lock and assumes one writer (EVENT-AUDIO-SPEC 7.1). There is no "list bindings" call. | FROM-REPO |
| F7 | The owner decision on tiers is "highest bound tier not above the occurrence; a tier-less binding covers every tier" (EVENT-AUDIO-SPEC section 2; CHANGELOG line 20). | FROM-REPO |
| F8 | Job object hard cap: 10% of total system CPU for the whole process tree; working set limit 900,000,000 bytes when this was read (`containment.rs` 8-11; no memory limit since DEC-RESOURCE-BUDGET). SDD line 359 requires intake/alert/audio threads at `THREAD_PRIORITY_ABOVE_NORMAL`; no code sets a thread priority today (no `SetThreadPriority` in `apps/` or `crates/`). | FROM-REPO |
| F9 | The instance guard runs before `run()` (`main.rs` 9). The data directory is `app.path().app_data_dir()` (`lib.rs` 90), frozen as `%APPDATA%\io.github.freshair129.kohbor` (`contracts/product-identity.json`). | FROM-REPO |
| F10 | Renderer I1: event `playback://event`; `started` carries `playback_id, class, event_type, tier, visual_url, audio_duration_ms, visual_tail_ms, at_ms`; `ended` carries `playback_id, reason, at_ms`. The parser accepts only `event_type` `"multi_kill"` or null and requires `visual_url` to start with `kohbor-media://` (`apps/desktop/src/playback.ts`). I2: `invoke("set_muted", {muted})` returns `{muted: boolean}` (`AudioPanel.tsx` 24). | FROM-REPO |
| F11 | Tauri 2.12.5 has `Builder::register_asynchronous_uri_scheme_protocol(scheme, Fn(UriSchemeContext, http::Request<Vec<u8>>, UriSchemeResponder))`. On Windows the webview URL is `http://<scheme>.localhost/<path>` (local registry source `tauri-2.12.5/src/app.rs` 2264, 2335). | FROM-REPO (dependency source) |
| F12 | CSP is `default-src 'self'; ... connect-src ipc: http://ipc.localhost` with no `media-src` (`tauri.conf.json`). | FROM-REPO |
| F13 | `user-settings.schema.json` has only `schema_version, revision, locale, gsi`; no audio field. | FROM-REPO |
| F14 | runtime-status `media.reason` already allows `AUDIO_MUTED`, `MEDIA_PROBE_FAILED`, `MEDIA_INVALID`, `MEDIA_UNMAPPED`, `CAPABILITY_UNAVAILABLE`. `status::build` hard-codes `not_ready/MEDIA_UNMAPPED` (`status.rs` 75). 1.0.0 to 1.1.0 was an additive domain recorded in CHANGELOG line 28 and DOCUMENT-CONTROL line 36. | FROM-REPO |
| F15 | Session epochs start at 1 and only increase (`session.rs` 125-145). Catalog events with `critical_media_required: true`: `health_danger`, `risk_alert`, `risk_revision`; only `health_danger` has a detector (`EventId::is_critical`). | FROM-REPO |
| F16 | Whether `AppHandle::emit` can block when the webview is stalled; whether `std::sync::mpsc::SyncSender::try_send` takes an internal waker mutex; whether the WASAPI render thread uses MMCSS. | UNVERIFIED |

## 1. Process and thread model

One process (the guard of F9 stays the only writer guarantee). New long-lived threads, all created in `run()` setup:

```
gsi-intake (existing, ABOVE_NORMAL)      audio-engine (new, ABOVE_NORMAL)        wasapi render (inside WasapiSink)
 serve -> accept: admit -> detect  --try_send+unpark-->  Engine::step:               ^ bounded hand-off (F5)
   T0 arrival, T1 after observe      critical lane (8)     resolve -> AudioOwner ----+ sink.submit / cancel
                                     ordinary lane (64)    owns AudioOwner<SlotSink>, Arc<PrewarmTable>
 Tauri command threads --control lane (32)-->              |  try_send, never blocks
   set_muted, media_*, probe                               v  out lane (256)
 media-service (new, BELOW_NORMAL) --SwapTable-->      audio-publisher (new, NORMAL)
   owns Store, prewarm loader, probe timer             history ring, app.emit("playback://event"), latency samples
 audio-device (new, NORMAL) --SinkReady-->             kohbor-media protocol: handler -> media-serve worker (new)
   opens / reopens WasapiSink with backoff
```

**Ownership (DECIDED).** `AudioOwner<SlotSink>` and the current `Arc<PrewarmTable>` are owned by the `audio-engine` thread only. `SlotSink` is a host type holding `Option<Box<dyn Sink + Send>>`; with an empty slot `submit` returns `Err(SinkError)`. The engine is not generic, so tests inject a boxed fake sink. `Store` is owned by `media-service` only. The history ring is owned by `audio-publisher`.

**Cadence (DECIDED).** The engine is event-driven with a timed fallback: the loop ends with `std::thread::park_timeout(wait)`; every sender calls `unpark` after `try_send`. `wait` is 10 ms while `owner.active(now)` or `owner.visual(now)` is `Some`, otherwise 250 ms (only to pick up device events, which have no wake). One iteration (`Engine::step(now_ms)`):

1. drain the critical lane completely;
2. drain up to 32 control messages;
3. drain up to 16 ordinary ticks;
4. `owner.pump_sink_events(now)`, `owner.tick(now)`, convert `drain_notices()`;
5. store the atomics of `EngineShared`.

`step` is a plain method taking `now_ms`, so tests drive it with no thread and no sleep.

**Intake side (DECIDED).** In `Shared::accept`, directly after `self.detector.observe(&tick)`: take `detected_at = Instant::now()` (T1); when `detected` is non-empty call `EngineTx::offer_tick`. The intake thread may only: build the message, `try_send`, `unpark`, bump an atomic counter. It never waits, never takes an engine lock, never touches the store. A session change (`Started`, `OwnerChanged`, `Resumed`) sends `Control::Session { epoch }` the same way.

```rust
// apps/desktop/src-tauri/src/clock.rs
#[derive(Clone, Copy)] pub struct MonoClock { origin: Instant }
impl MonoClock {
    pub fn from_origin(origin: Instant) -> Self;
    pub fn ms_at(&self, t: Instant) -> u64;   // saturating, capped at 9_007_199_254_740_991 like intake::duration_ms
    pub fn now_ms(&self) -> u64;
}
// intake.rs addition: impl Shared { pub fn clock(&self) -> MonoClock /* origin = started_at */
//                                   pub fn attach_engine(&mut self, tx: EngineTx) }

// apps/desktop/src-tauri/src/audio/lanes.rs
pub const CRITICAL_LANE: usize = 8; pub const ORDINARY_LANE: usize = 64;
pub const CONTROL_LANE: usize = 32; pub const OUT_LANE: usize = 256;
pub struct TickMsg {
    pub arrival: Instant,        // T0 (F1)
    pub detected: Instant,       // T1
    pub at_ms: u64,              // intake at_ms of the tick
    pub session_epoch: u64, pub generation: u64, pub sequence: u64,   // sequence = Shared.sequence + 1
    pub events: Vec<kohbor_events::Event>,
}
pub enum Control {
    Session { epoch: u64 },
    SetMuted { muted: bool, ack: SyncSender<bool> },
    SwapTable { table: Arc<PrewarmTable> },
    SinkReady(Box<dyn kohbor_audio::Sink + Send>),
    Shutdown,
}
pub enum Offer { Sent, DroppedOrdinary, CriticalOverflow }
#[derive(Clone)] pub struct EngineTx { /* 3 SyncSenders, std::thread::Thread, Arc<EngineShared> */ }
impl EngineTx {
    pub fn offer_tick(&self, msg: TickMsg) -> Offer;          // critical lane iff any event.id.is_critical()
    pub fn control(&self, c: Control) -> Result<(), Control>; // Err = lane full, caller reports CAPABILITY_UNAVAILABLE
}
pub struct EngineShared {   // atomics only; read by status, media-service, tests
    pub heartbeat_ms: AtomicU64, pub muted: AtomicBool, pub device_available: AtomicBool,
    pub lease_active: AtomicBool, pub fence: AtomicU64, pub dropped_ordinary: AtomicU64,
    pub critical_overflow: AtomicU64, pub out_dropped: AtomicU64, pub rebuilds: AtomicU32, pub degraded: AtomicBool,
}
```

**Drop policy (DECIDED).** A tick with a critical event goes whole (critical and ordinary events of that tick together, so same-tick order is kept) to the critical lane. Other ticks go to the ordinary lane; when it is full the **new** tick is dropped and `dropped_ordinary` is incremented (no outcome record can be written for it; the counter is the trace). The critical lane cannot fill while the engine lives: `health_danger` has a 20,000 ms detector cooldown (FROM-REPO `Config::danger_cooldown_ms`), so 8 slots cover at least 140 s of a dead engine. If `try_send` on the critical lane still fails, `critical_overflow` is incremented and status reports `unavailable / CAPABILITY_UNAVAILABLE`. A bounded queue cannot promise more; this is stated in section 9.

**Time base (DECIDED).** One `MonoClock` whose origin is `Shared.started_at` (F1). `TickMsg.at_ms` is the intake value. Every `at_ms` passed to the owner (`request`, `tick`, `pump_sink_events`, `set_*`) is `clock.now_ms()` read on the engine thread at the call, because the owner computes audio deadlines from the admission instant (F4). `Decision.decided_at_ms` and `Notice.at_ms` are therefore in the same base as intake `at_ms`, and I1 `at_ms` is that value.

**Panic containment (DECIDED).** Engine code must not panic (no `unwrap`, `expect`, indexing or unchecked arithmetic in the admission files; source guard in section 10). The thread body is `loop { catch_unwind(AssertUnwindSafe(|| self.run())) ... }` with the lane receivers held outside the closure. After a caught panic the engine: increments `rebuilds`; sends the old sink to `audio-device` for dropping; builds `AudioOwner::with_first_playback_id(SlotSink::empty(), config, last_issued_id + 1)`; re-applies mute, session and table; asks `audio-device` for a new sink; emits `ended / cancelled_device_lost` for the playback that was live. More than 3 rebuilds in 60 s sets `degraded`: the engine keeps draining lanes and records every request as `channel_busy` without touching a sink, and status reports `unavailable / CAPABILITY_UNAVAILABLE` until restart. FROM-REPO: no `panic = "abort"` in either `Cargo.toml`, so unwinding is available.

**Owner additions needed (DECIDED, small, pure, F4).** In `crates/kohbor-audio/src/owner.rs`:

```rust
pub fn with_first_playback_id(sink: S, config: Config, first_playback_id: u64) -> AudioOwner<S>; // 0 is treated as 1
pub fn sink_mut(&mut self) -> &mut S;        // host device recovery only
pub fn is_muted(&self) -> bool;
pub fn device_available(&self) -> bool;
```

**Shutdown order (DECIDED).** `run()` switches to `build(...)?.run(|_, event| ...)`. On `RunEvent::Exit`: (1) set `shutting_down`, commands answer `CANCELLED`; (2) `Control::Shutdown`: the engine drops the owner, the sink goes to `audio-device`, which drops it (joins the render thread, F5); (3) `media-service` finishes the store call in progress and is joined for at most 5,000 ms; (4) `audio-publisher` ends when its channel disconnects; (5) the intake thread stays detached as today. Nothing is deleted.

**CPU cap and priorities (DECIDED, F8).** The hard cap suspends the whole job when the budget is spent, so background work can freeze the critical threads. Therefore: `audio-engine` and `gsi-intake` set `THREAD_PRIORITY_ABOVE_NORMAL` (the `windows` crate feature `Win32_System_Threading` is already enabled, FROM-REPO `Cargo.toml`); `media-service` sets `THREAD_PRIORITY_BELOW_NORMAL` and the prewarm loader keeps a 25% duty cycle (after each `resolve` it sleeps three times the elapsed time) and pauses while the existing `Throttle` reports throttled (FROM-REPO `lib.rs` 107-111). Idle cost of the engine is four wake-ups per second.

**Instance guard.** No step of this design or of its verification starts the application. All tests use a fake sink and a temporary store root. Native-silence tests live in `kohbor-audio-native` and open only the audio endpoint, never the data directory.

## 2. Event to audio mapping

`audio/resolve.rs`, pure:

```rust
pub fn class_of(id: EventId) -> AudioClass;                       // Critical if id.is_critical(), else Ordinary
pub fn instance_id(generation: u64, sequence: u64, index: usize) -> String;   // "ev_g{generation}_s{sequence}_{index}"
pub enum Lookup<'a> { Ready(&'a Clip), Unmapped, Invalid }
impl PrewarmTable { pub fn lookup(&self, id: EventId, tier: Option<u8>) -> Lookup<'_>; }
```

`Engine::handle_tick(msg, now)` order (DECIDED):

1. If `msg.session_epoch` is greater than the engine's epoch: `owner.set_session(now, epoch)`. A lower epoch is not applied (the owner then answers `stale_session`).
2. Assign `instance_id` by position in `msg.events`.
3. Events in `intake::EVENT_UNAVAILABLE` (today `first_blood`) are removed **before** arbitration and recorded `detector_unavailable`. Otherwise an unconfirmed detector could silence a confirmed lower-ranked event.
4. Events with `!id.voiced()` (`hero_respawn`) are recorded `not_voiced`.
5. Critical events first, in order: `lookup`; `Unmapped` records `unmapped`, `Invalid` records `invalid_media`, `Ready` builds a `Request` and calls `owner.request(now, request)`.
6. `kohbor_events::arbitrate(&rest)`: every `dropped` entry records `lower_rank_same_tick`; the `winner` goes through the same lookup and request. A winner that is unmapped or invalid leaves the slot silent (EVENT-AUDIO-SPEC section 2 rule 4).

`Request` fields: `instance_id` as above; `event_type = Some(id.as_str())`; `session_epoch = msg.session_epoch`; `class = class_of(id)`; `episode = Some(instance_id.clone())` for critical, `None` otherwise; `package_revision = Some(table.fence)`; `pcm = clip.pcm.clone()`; `visual_tail_ms = clip.visual_tail_ms`.

- **Episode.** The detector emits `health_danger` once per danger episode (re-arm at 40%, 20 s cooldown; FROM-REPO `lib.rs` 166-170 of kohbor-events), so each event is its own episode. The owner's same-episode `cooldown` branch stays unused for `health_danger`; it exists for `risk_alert`.
- **Same tick.** If `health_danger` is admitted, the ordinary winner of that tick gets `critical_active` from the owner. If `health_danger` is unmapped (today), the ordinary winner plays normally; it is its own event, not a substitute (see P4 in section 12).
- **Not wired.** `enemy_missing`, `risk_alert`, `risk_revision` are not `EventId` variants and have no detector (F15). C2, C3 and C5 are reachable only in owner tests. No code path may submit them from the host in S3-5.
- **`inert`.** The detector can only emit catalog IDs, so the engine never produces `inert`; the value stays reserved for package bindings (reported by the store) and future sources.
- **Tier (F6, F7).** `lookup(id, Some(t))` returns the clip with the highest bound tier not above `t`, else the tier-less clip, else `Unmapped`. `lookup(id, None)` returns only the tier-less clip. The store's exact resolve is the primitive used to fill the table; the fallback rule lives in the table.

## 3. Media availability without disk in the admission path

```rust
// audio/resolve.rs
pub struct Clip { pub pcm: Pcm, pub bound_tier: Option<u8>, pub audio_ms: u32 /* ceil */, pub visual_tail_ms: u32,
                  pub visual_url: Option<Arc<str>> }
pub enum Entry { Ready(Clip), Invalid }
pub struct PrewarmTable { pub fence: u64 /* install number; 0 = none */, pub install_id: Option<Arc<str>>,
                          entries: Vec<(EventId, Option<u8>, Entry)> }
impl PrewarmTable { pub fn empty() -> Arc<PrewarmTable>; }
// media/prewarm.rs  (media-service thread only)
pub fn build_table<H: Hook>(store: &Store<H>, pause: &dyn Fn()) -> (Arc<PrewarmTable>, VisualIndex, Vec<CriticalMedia>);
```

- **Who decodes (DECIDED).** `media-service`, off the critical path, when the store opens with an active revision and after every successful activation. Key space: every `EventId` that is voiced, plus each tier of its `tier_range`, plus the tier-less key: 23 keys today. For each key: `store.resolve(id.as_str(), tier)` (which re-verifies size and SHA-256, F6), `std::fs::read` of `mixed_audio`, `kohbor_audio::pcm::decode_wav`. `ResolveError::Unmapped` leaves no entry; `Invalid(_)`, a read error or a decode failure stores `Entry::Invalid`; `Inert` cannot occur for these keys.
- **Fence number (DECIDED).** `fence` is the decimal number of `install_id` (`ins_0000000007` gives 7), as the owner's `Request::package_revision` doc already says (FROM-REPO `owner.rs` 143). The descriptor revision is not unique across packages.
- **Swap (DECIDED).** `Control::SwapTable { table }`. The engine, in one step: `owner.set_revision(now, table.fence)`, replace `self.table`, store `EngineShared.fence`, send the old `Arc` to the publisher lane so the large free happens off the engine thread. No request can see a mixed state because requests and swaps run on one thread. `media-service` swaps the `VisualIndex` (section 5) when it sends the message.
- **Gap after activation.** Between `Store::activate` and the swap the engine still plays the previous table from memory. That is the immutable lease of MEDIA-PACK-SPEC line 92; readiness describes the table the engine holds.
- **Memory bound.** One clip is at most 15 s x 48,000 Hz x 2 bytes = 1,440,000 bytes. The descriptor limit of 128 bindings would be 184,320,000 bytes, but at most one binding per (event, tier) is legal and only catalog-mappable keys can play: 23 keys today (33,120,000 bytes), 26 when `enemy_missing`, `risk_alert` and `risk_revision` gain detectors (37,440,000 bytes, 35.7 MiB). During a swap two tables exist: at most 74,880,000 bytes, 8.3% of the 900,000,000-byte budget.
- **What is prewarmed (DECIDED): everything playable, eagerly.** Consequence for ALERT-001: no admission, critical or ordinary, ever reads a disk, so a slow or failing disk cannot delay a C1 request queued behind a C4 request. The rejected alternative (critical eager, ordinary lazy) would save at most 33 MB and would put file reads either on the engine thread (violates ALERT-001) or behind an asynchronous load that makes first plays late and adds a second code path. Visual bytes are never held in memory (up to 32 MiB each).
- **Loader cost.** Worst case 23 resolves, each hashing up to about 33.4 MiB and re-reading a descriptor of up to 2 MiB; throttled as in section 1. A store call that lists bindings in one pass is a later optimisation and is not part of S3-5.

## 4. Package store in the host

- **Location (DECIDED).** `<app_data_dir>/media` (F9), created by `Store::open`. Catalog: `Catalog::from_json` over `include_str!` of `contracts/event-catalog.json`; `Limits::default()`.
- **Owner.** `media-service` opens it after startup, never on the UI or engine thread. If `open` fails the service stays in `StoreUnavailable`: commands answer `Err("MEDIA_INVALID")`, the engine keeps the empty table, and status reports `not_ready / MEDIA_INVALID`.
- **Commands (DECIDED; `#[tauri::command(async)]`, like `gsi_setup`).** Each sends a job on a bounded channel (4) with a one-shot reply and waits; a full channel answers `Err("CAPABILITY_UNAVAILABLE")`.

| Command | Argument | Result | Store call |
| --- | --- | --- | --- |
| `media_inspect` | `{ source_path: string }` | `{ container_sha256: string, size_bytes: number }` or `Err(code)` | none (bounded read, at most 512 MiB) |
| `media_install` | `request`: media-install-request 1.0.0, parsed by `InstallRequest::from_json` | media-install-result 1.0.0 (`InstallResult::to_json`) | `Store::install` only; never activates (7.1) |
| `media_activate` | `{ request_id, install_id, policy: "never" \| "now_if_idle" \| "after_match" }` | media-install-result 1.0.0 | `Store::activate`, then prewarm and swap on `active` |
| `media_list` | none | `{ active: {install_id, package_id, revision} \| null, records: Record[], pending: string[] }` (host-local shape, not a contract) | `active`, `records`, `pending_activations` |

- **ActivationState (DECIDED).** `match_active` = the last admitted phase is `Draft`, `Pregame` or `Live` (read from `Shared` under its lock on the service thread) and `intake::game_running()` is not `Some(false)` (FROM-REPO `intake.rs` 136; a closed game ends the barrier, an unreadable process list keeps it). `leased_install_ids` = the engine table's `install_id` when `EngineShared.lease_active` is true (a non-voice playback or its tail exists), else empty.
- **Deferred.** While `pending_activations()` is non-empty the service calls `resume_deferred(state)` every 2,000 ms and prewarms on `active`.
- **Not in this task.** The external authoring-app transport (named pipe, 16 KiB frames, handshake) stays with DEC-INTERFACES. No file-picker plugin is added; install UI is a later task. The commands exist so the path can be exercised by tests and by a later UI.

## 5. Renderer interfaces

**I1 emission (DECIDED).** The engine keeps a `live` map of at most two entries (`PlaybackId` to `{Decision, audio_ends_at_ms}`: the active playback and one tail). It sends `EngineOut::Playback(json)` and `EngineOut::Outcome(record)` on the out lane with `try_send`; a full lane increments `out_dropped` and never blocks. `audio-publisher` calls `app.emit("playback://event", payload)` (F16 is why emit is not on the engine thread). Notices drained after a call are emitted **before** that call's own decision, so the old playback ends before the new one starts.

| Owner output | I1 payload | Outcome record |
| --- | --- | --- |
| `Decision{Admitted}` or `Notice::Decided(Decision{Admitted})` | `started {playback_id: id.to_contract_id(), class, event_type, tier: clip.bound_tier, visual_url, audio_duration_ms: clip.audio_ms, visual_tail_ms: min(tail, 3200), at_ms: decided_at_ms}` | `admitted` |
| `Notice::Finished{id, at_ms}` | `ended {reason: "finished"}` | none |
| `Cancelled{Preempted}` | `ended cancelled_preempted` | `preempted` for the old playback, **only if** `at_ms < audio_ends_at_ms`; a tail-only cancel writes no record |
| `Cancelled{SessionFence}` | `ended cancelled_session` | none |
| `Cancelled{RevisionFence}` | `ended cancelled_revision` | none |
| `Cancelled{Muted}` | `ended cancelled_muted` | none |
| `Cancelled{DeviceLost}` | `ended cancelled_device_lost` | none |
| any other `Decision` (direct or `Notice::Decided`) | none | its `outcome` |

**Renderer changes required (F10, F11, F12).** (a) `event_type` must accept every catalog ID, not only `multi_kill`; otherwise a `health_danger` start is counted malformed. (b) `visual_url` stays the logical form `kohbor-media://localhost/v1/<install_id>/<path_sha256>`; `EventBanner` converts it to the platform URL (`http://kohbor-media.localhost/v1/...` on Windows) with a pure function. (c) CSP gains `media-src http://kohbor-media.localhost kohbor-media:`. This scheme is internal to the webview; it is not the reserved OS scheme `kohbor` of the product identity.

**`kohbor-media` protocol (DECIDED).** Registered with `register_asynchronous_uri_scheme_protocol("kohbor-media", handler)`.

- `VisualIndex` = `HashMap<token, {path, size_bytes, modified}>` for the engine-held revision, behind an `Arc<Mutex<Arc<VisualIndex>>>` that only the handler and `media-service` touch. `token` = lowercase hex SHA-256 of the binding's descriptor-relative visual path with `/` separators (this is the `path_sha256` of the probe; the spec does not define its input, so this is a proposal).
- The handler accepts only `GET` or `HEAD` on `/v1/<ins_ + 10 digits>/<64 hex>` with no query. It clones the index `Arc` and passes the responder to the `media-serve` worker through a bounded channel (8); a full channel answers 503. It does nothing else on the calling thread.
- The worker: `install_id` must equal the index's; token must be present; the file must be a regular file whose length and modification time equal the values recorded at prewarm (the SHA-256 was verified by `resolve` at prewarm; a full re-hash per range request is rejected as too slow); else 404. It serves `Content-Type: video/webm`, `Accept-Ranges: bytes`, `Cache-Control: no-store`; one `Range: bytes=a-b` gives 206 with at most 4 MiB; no range gives 200 with the whole file (at most 32 MiB by `MAX_PAYLOAD_BYTES`); malformed range gives 416.
- Never served: any path derived from the URL, a directory listing, audio, identity audio, a file of another revision, anything after a failed check. The URL never contains a file name.
- Residual risk (stated, not solved): a file replaced with equal length and time between prewarm and read would be served as video. It cannot reach the audio path.

**Decode probe (DECIDED; EVENT-AUDIO-SPEC section 6).**

- After a swap, `media-service` forms the probe set: bindings of class C1/C2 that have a visual (today at most `health_danger`). It emits (through the publisher) `media://probe-request` `{install_id, revision, items: [{path_sha256, url, duration_ms, visual_tail_ms}]}` and starts a 5,000 ms timer per item.
- The renderer's probe context (a hidden element, not the banner) decodes the first frame, reads the duration and calls `media_probe_report({install_id, revision, path_sha256, ok, duration_ms}) -> {accepted: boolean}`.
- Native accepts a report only for the engine-held `install_id`, a token in the probe set and a running timer. `probe_ok = Some(true)` needs `ok` and `duration_ms` in `[binding.duration_ms - 42, binding.duration_ms + visual_tail_ms + 42]` (42 ms = one frame at 24 fps; proposal P5). Timeout or a failed report gives `Some(false)`.
- `media_probe_pending() -> items[]` lets a reloaded renderer fetch the items that are pending or failed; calling it restarts their timers once per renderer mount.
- **Effect: critical readiness only** (`MEDIA_PROBE_FAILED`). A failed critical probe also removes that clip's `visual_url` (the service sends a table copy with the same fence and shared PCM). Audio admission never waits for, or depends on, a probe. This narrows section 6 step 3 of the spec (P2). Ordinary visuals are not probed in S3-5.

**I2 `set_muted` (DECIDED).** `set_muted(muted: bool) -> Result<{muted: bool}, String>` sends `Control::SetMuted` and waits at most 250 ms for the ack on the command thread; timeout or full lane gives `Err("CAPABILITY_UNAVAILABLE")`. The owner is the only authority; `EngineShared.muted` mirrors `owner.is_muted()`. **Mute is not persisted**: the process starts unmuted. Reasons: no settings contract change (F13); a restart must not leave safety audio silently off; status always shows the truth. Readiness: `assess` rule 1 gives `unavailable / AUDIO_MUTED` (owner acknowledgement of 2026-11-11, FROM-REPO `contracts/decisions.json`).

## 6. Readiness and status

`media/inventory.rs` (DECIDED):

```rust
pub const ALL_EVENTS: [EventId; 12];                 // guarded by an exhaustive match so a new variant fails to compile
pub fn available_critical() -> Vec<EventId>;         // ALL_EVENTS where is_critical() and not in EVENT_UNAVAILABLE
pub struct MediaFacts { pub store_ok: bool, pub critical: Vec<CriticalMedia> }   // published by media-service
pub fn media_status(facts: &MediaFacts, shared: &EngineShared, now_ms: u64) -> (&'static str, Option<&'static str>);
```

- The inventory is built by the host from the detector's own enum; no caller, package or renderer can shorten it. A test loads `contracts/event-catalog.json` and asserts: every catalog event with `critical_media_required: true` and `source: gsi` is in `available_critical()` or in `EVENT_UNAVAILABLE`, and no listed event lacks the flag.
- For each inventory event `CriticalMedia` = `{mapped: an entry exists for the tier-less key, verified and prewarmed: the entry is Ready, probe_ok}` taken from the table the engine holds.
- `media_status`: engine heartbeat older than 2,000 ms, `degraded`, or `critical_overflow > 0` gives `unavailable / CAPABILITY_UNAVAILABLE`; `!store_ok` gives `not_ready / MEDIA_INVALID` unless mute or device already apply; otherwise `kohbor_audio::assess(&ReadinessInput{available_critical, muted, device_available})`.
- `status::build` gains one parameter `media: (&str, Option<&str>)`; `MediaFacts` sits behind a mutex that only the status command and `media-service` take.
- **Result today:** no authorized critical media exists, so `health_danger` is unmapped: `not_ready / MEDIA_UNMAPPED` on a machine with an output device, `unavailable / CAPABILITY_UNAVAILABLE` without one, `unavailable / AUDIO_MUTED` when muted. The product never reports `ready` and never claims audible safety alerts.
- **When critical media is installed and activated:** unverified or undecodable gives `MEDIA_INVALID`; probe pending, failed or timed out gives `MEDIA_PROBE_FAILED`; all passing gives `ready`.
- **Schema (DECIDED): no change, stay on 1.1.0.** History has its own command. If a field is needed later (for example `media.active_install_id`), it is additive and follows the 1.1.0 precedent (F14): bump the `schema_version` const, `status.rs`, `types.ts`, fixtures, CHANGELOG and DOCUMENT-CONTROL in one change.

## 7. Outcome history

- `audio/outcome.rs`: `pub fn record(d: &Decision) -> serde_json::Value` produces audio-outcome 1.0.0: `instance_id`, `event_type`, `session_epoch`, `audio_class: class.as_str()`, `outcome: outcome.as_str()`, `playback_id: playback_id.map(to_contract_id)` (`pb_0000000001`), `package_revision`, `decided_monotonic_ms: decided_at_ms`.
- Outcomes decided before the owner (`unmapped`, `invalid_media`, `not_voiced`, `detector_unavailable`, `lower_rank_same_tick`) use the same builder with `playback_id: null`, `package_revision: Some(fence)` or null when `fence == 0`, and the engine's `now`.
- `preempted` is a second record for the old playback: identity fields from its stored `Decision`, `outcome: "preempted"`, its `playback_id`, `decided_monotonic_ms` = the notice `at_ms` (FROM-REPO `Notice` doc, `owner.rs` 195-199). History is append-only: `admitted` then `preempted`.
- Voice records (none are produced in S3-5) have `event_type` and `package_revision` null, as the owner already normalises.
- A record that would break the contract (for example `session_epoch` 0) is not stored; a counter is incremented.
- Ring: `VecDeque` of 256 records in `audio-publisher`, behind a mutex that only the publisher and the command take. `audio_history(limit: Option<u32>) -> Value[]`, oldest first, at most 256. **Never written to disk, never sent anywhere** (AGENTS.md).

## 8. Device loss and recovery

- **Detection.** `pump_sink_events` marks the device unavailable (F4). The engine compares `owner.device_available()` before and after and, on a fall, sends `EngineOut::DeviceLost`. The active playback gets `Cancelled{DeviceLost}`; a queued C2 gets `Notice::Decided(channel_busy, DeviceUnavailable)` from the owner (FROM-REPO `owner.rs` 612-619) and is **not** replayed after recovery: a correction older than the outage is stale.
- **Reopen (DECIDED).** On the `audio-device` thread: `WasapiSink::start()` after 250, 500, 1,000, 2,000, 4,000 ms, then every 5,000 ms without limit. Success sends `Control::SinkReady`. The engine installs it with `owner.sink_mut()`, sends the old sink back for dropping, then `owner.set_device_available(now, true)`. The same path opens the first sink at startup; until then the owner is marked unavailable.
- **Default device changed.** The engine forwards `DeviceChanged`; `audio-device` opens a sink on the new default and sends `SinkReady`; the engine holds it and swaps only when `owner.active(now)` is `None` (at most one clip and tail later). The old stream keeps playing meanwhile. Because `pump_sink_events` consumes the event, `SlotSink::poll_event` records "changed" in a flag the engine reads.
- **UI.** `media` shows `unavailable / CAPABILITY_UNAVAILABLE`; the banner ends with `cancelled_device_lost`. Requests during the outage are recorded `channel_busy`.

## 9. Failure containment

| Stalled or dead | Keeps working | Degrades |
| --- | --- | --- |
| Renderer / webview | Intake, detection, admission, audio, outcome history (ring keeps the newest 256) | No banner; `playback://event` is lost once the 256-slot out lane fills; critical probes time out, so readiness is `MEDIA_PROBE_FAILED` for a revision activated meanwhile |
| Network | Everything. No component of this design opens a socket other than the existing loopback intake | Nothing |
| Package store I/O | Admission and audio from the table already in memory; mute; status | Install, activate and prewarm of a new revision fail or wait; commands time out; a store that fails to open leaves the empty table (`MEDIA_INVALID`) |
| Prewarm loader | Playback of the previous table | New revision not audible until it completes; if it never completes the old one stays |
| Probe | Audio of every class | Critical readiness `MEDIA_PROBE_FAILED`; that clip has no visual |
| Output device | Detection, outcomes (`channel_busy`), status, reopen attempts | No audio until `SinkReady` |
| Engine panic | Lanes keep accepting; rebuild as in section 1 | The live playback is cut; after 3 rebuilds in 60 s audio is off until restart |
| Publisher | Admission and audio | No history, no banner; out lane drops are counted |

**Not guaranteed:** audible output (it needs a device, unmuted state and installed authorized media); acoustic latency (not measured, section 10); delivery of a critical event when the engine has been dead for 8 cues; ordinary events when the ordinary lane is full; latency while a prewarm runs with the job at its CPU cap; the F2/F3 items below.

**Existing items on the T0 to T1 path, outside S3-5 scope but reported for S3-7 (FROM-REPO F2, F3):** a registry read per payload; a file write on session start (not on a tick that can emit `health_danger`, because the first tick of a session only sets baselines); the `Shared` mutex shared with two status commands that hold it for memory work only. New rule for this sprint: no reader of `Shared` may do I/O under the lock, and the engine never takes it.

## 10. Verification plan

| Check | What it proves | Evidence class | State |
| --- | --- | --- | --- |
| `audio/` unit tests: resolve, tier fallback, outcome records validated against `audio-outcome.schema.json`, I1 payloads against the renderer's closed shape | Mapping and contracts | synthetic | to write |
| Whole-path test with a fake boxed sink: build `Shared`, `attach_engine`, feed payloads through `accept` (as `intake.rs` tests do), call `Engine::step`, assert fake-sink submits, records and I1 order | intake tick, detector, arbiter, request, owner, sink | synthetic | to write |
| Same test with a publisher whose emitter blocks forever | Admission continues with a stalled renderer; `out_dropped` rises; nothing blocks | synthetic | to write |
| Same test with `Store::open_with_hook` injecting `IoError`/`Crash`, and with the service thread never answering | Admission continues from the held table; failed activation leaves the old table | synthetic | to write |
| Lane tests: ordinary overflow drops the newest and counts; a critical tick is delivered when the ordinary lane is full; critical overflow sets the status | Drop policy | synthetic | to write |
| Device tests with a fake sink emitting `DeviceLost`/`DeviceChanged`, then `SinkReady` | Section 8, queued C2 outcome | synthetic | to write |
| Panic test: a fake sink that panics in `submit` | Rebuild, id continuity, degraded mode | synthetic | to write |
| Protocol tests on the worker function (no webview): traversal, wrong install, unknown token, changed length, ranges | Section 5 | synthetic | to write |
| **Source guard** (pattern of `intake.rs` 1507-1511): a test reads the admission files (`clock.rs`, `audio/lanes.rs`, `audio/resolve.rs`, `audio/outcome.rs`, `audio/engine.rs`, `audio/slot_sink.rs`; not `publisher.rs` or `device.rs`) with `include_str!` and fails on `std::fs`, `std::net`, `File::`, `Mutex`, `RwLock`, `.lock(`, `.recv()` (blocking), `.send(` on a sync channel, `thread::sleep`, `unwrap(`, `expect(`, `panic!`, `kohbor_pack`, `tauri` | No file, network, lock, blocking call or panic source in admission code | source | to write |
| Existing `kohbor-audio-native` tests | Hand-off and silent rendering | native (silence only) | exists, not rerun here |
| Device loss on real hardware; default-device change | Section 8 on Windows | native | NOT_RUN |
| QUALITY-001 loopback measurement | Acoustic latency | acoustic | NOT_RUN; owner hardware and authorized media needed |

**S3-6 latency harness (DECIDED).**

- Timestamps, all `Instant` converted by the one `MonoClock`: **T0** = `arrival` in `serve`, after the body is read (F1); **T1** = `detected`, after `Detector::observe` returns; **T2** = on the engine thread immediately before `owner.request`; **T3'** = immediately after `owner.request` returns `Admitted`, which is when `sink.submit` has accepted the buffer into the hand-off. T3' is **not** the first rendered frame and not sound.
- The engine emits `EngineOut::Latency { instance_id, event_type, t0_ms, t1_us, t2_us, t3p_us }` (offsets from T0 in microseconds) only when a `latency` flag is set at construction; production leaves it off.
- Stage A (synthetic): a test binary runs at least 300 `health_danger` golden traces (`fixtures/health-danger-golden.json`) through real threads (intake `accept`, engine thread, fake sink, a temporary table built from a generated WAV) and writes a receipt.
- Stage B (native, silence only, optional): the same with `WasapiSink` and an all-zero clip, adding **T3''** = first render service that copied the buffer. This needs a small test-only hook in `kohbor-audio-native`; the S3-1 spike saw 8.6 to 11.2 ms from producer marker to render service over 200 runs (FROM-REPO `S3-1-AUDIO-SPIKE.md` line 49-50, silence only).
- Receipt: JSON under `target/evidence/s3-6/` (not tracked) that follows `contracts/evidence.schema.json`: `evidence_class` `synthetic` (stage A) or `native` (stage B), `source_revision` = `git rev-parse HEAD`, the harness binary SHA-256, sample count, p50, max, every sample, and the literal note "engineering fixture; not QUALITY-001".
- QUALITY-001 itself needs loopback-audible T3 over at least 300 samples per scenario, authorized critical media and the owner's hardware. It stays **NOT_RUN**. No threshold of this harness is a pass criterion for it.

## 11. Task breakdown

Rules for every task: branch `s3/native-audio-critical-path`; one writer per file scope below; no task starts the application or opens the real data directory; `cargo test -p <crate> -j 4`; report commands and results; R2 means the lead reads the whole diff. Luna = general, Terra = tests and review.

| ID | Goal | May write | Must match | Acceptance | Model | Risk | Evidence | Needs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| S3-5a | Owner hooks | `crates/kohbor-audio/src/owner.rs`, `crates/kohbor-audio/tests/host_hooks.rs` | The four signatures in section 1 | New tests: id continuity from a given first id; `sink_mut` swap while unavailable; getters follow `set_*` and `pump_sink_events`. Existing tests unchanged and green | Luna | R2 | synthetic | none |
| S3-5b | Clock, lanes, message types, `EngineShared` | `src-tauri/src/clock.rs`, `src-tauri/src/audio/mod.rs`, `audio/lanes.rs` | Section 1 code block | Lane tests of section 10 (overflow, critical priority) | Luna | R2 | synthetic | none |
| S3-5c | Tests first: resolve, tier, outcome records, I1 payloads | `src-tauri/src/audio/tests_resolve.rs`, `audio/tests_outcome.rs` | Sections 2, 5 (table), 7; schemas in `contracts/` | Tests compile against stubs and fail only for missing behaviour | Terra | R1 | synthetic | 5b |
| S3-5d | `resolve.rs` (class, instance id, `PrewarmTable`, lookup) and `outcome.rs` (records, I1 builders) | `audio/resolve.rs`, `audio/outcome.rs` | Sections 2, 3, 5, 7 | S3-5c tests green | Luna | R2 | synthetic | 5c |
| S3-5e | Tests first: engine step (whole path with fake sink, same-tick order, session fence, swap, mute ack, device loss and change, panic rebuild, degraded) | `audio/tests_engine.rs`, `audio/fake_sink.rs` | Sections 1, 2, 5, 8 | Tests written against the `Engine` API below | Terra | R1 | synthetic | 5a, 5d |
| S3-5f | `Engine` and `SlotSink` | `audio/engine.rs`, `audio/slot_sink.rs` | `Engine::new(clock, shared, sink: Option<Box<dyn Sink + Send>>, out: SyncSender<EngineOut>, latency: bool)`; `Engine::step(&mut self, now_ms: u64) -> Duration`; `Engine::spawn(self, rx: EngineRx) -> EngineTx`; section 1 loop order | S3-5e green; no thread needed by tests except `spawn` smoke test | Luna | R2 | synthetic | 5e |
| S3-5g | Publisher (history ring, emitter trait) and device supervisor | `audio/publisher.rs`, `audio/device.rs` | `trait Emit { fn emit(&self, event: &str, payload: &Value); }`; `audio_history`; backoff of section 8 with an injectable opener `Fn() -> Result<Box<dyn Sink + Send>, ()>` and injectable sleep | Blocking-emitter test; ring bound; backoff sequence test without real time | Luna | R1 | synthetic | 5b, 5d |
| S3-5h | Inventory and readiness | `src-tauri/src/media/mod.rs`, `media/inventory.rs` | Section 6 signatures | Catalog cross-check test; table of states including today's `MEDIA_UNMAPPED`; never `ready` with an empty or partial inventory | Terra then Luna | R2 | synthetic | 5d |
| S3-5i | Prewarm loader and media service (store owner, jobs, ActivationState, deferred resume) | `media/prewarm.rs`, `media/service.rs`, `src-tauri/tests/media_service.rs` | Sections 3, 4; `build_table` signature | Temp-store tests using packages built as `crates/kohbor-pack/tests/common` does: valid, corrupt WAV (`Invalid`), failing store hook, activation deferred during a match and resumed, throttle pause honoured | Luna | R2 | synthetic | 5d, 5h |
| S3-5j | `kohbor-media` worker and probe state | `media/protocol.rs`, `media/probe.rs` | Section 5 | Protocol and probe tests of section 10 (timer injectable) | Luna | R2 | synthetic | 5i |
| S3-5k | Renderer: widen `event_type`, URL conversion, probe context, `media_probe_report`/`pending` calls, mock updates | `apps/desktop/src/playback.ts`, `EventBanner.tsx`, `runtime.ts`, new `probe.ts`, `mock/harness.tsx`, `tests/playback.test.ts` | F10; section 5 | Renderer tests green; a `health_danger` start is accepted; logical URL converts to the Windows form | Luna | R1 | synthetic | none (contract from this document) |
| S3-5l | Integration: wire threads, commands, protocol registration, status, intake hook, thread priorities, shutdown, CSP | `src-tauri/src/lib.rs`, `intake.rs`, `status.rs`, `Cargo.toml` (add `kohbor-pack`, `kohbor-audio`, `kohbor-audio-native`), `tauri.conf.json` | Sections 1, 4, 5, 6 | All host tests green; status tests validate the contract for mute, no device, unmapped; intake tests unchanged; source guard of section 10 added and green | Luna | R2 | synthetic + source | 5f, 5g, 5i, 5j |
| S3-6a | Latency harness stage A and receipt writer | `src-tauri/tests/latency_harness.rs`, `src-tauri/src/latency.rs` | Section 10 | At least 300 samples; receipt validates against `evidence.schema.json`; labelled engineering fixture | Terra | R1 | synthetic | 5l |
| S3-6b | Stage B hook (optional) | `crates/kohbor-audio-native/src/*` (test-only first-fill timestamp) | Section 10 | Silence only; skipped and reported NOT_RUN when no endpoint | Luna | R1 | native (silence) | 6a |
| S3-7 | Review of S3-3 and S3-5: full diff of every R2 task, the F2/F3 items, RTM and `requirements.json` `implementation_refs` update | review notes; `contracts/requirements.json`, `docs/requirements/RTM.csv` via its generator | AGENTS.md evidence rules | Findings listed; no PASS claimed for native, live or acoustic classes | Terra, Lead | R2 | source | 5l, 6a |

`src-tauri/src/...` means `apps/desktop/src-tauri/src/...`. Test modules under `audio/` are declared in `audio/mod.rs` by S3-5b as `#[cfg(test)] mod` lines so later tasks do not edit that file.

```
5a ──────────────┐
5b ─> 5c ─> 5d ─┬┴> 5e ─> 5f ──────────┐
                ├─> 5g ────────────────┤
                └─> 5h ─> 5i ─> 5j ────┼─> 5l ─> 6a ─> 6b
5k (parallel, renderer only) ──────────┘          └──> S3-7
Parallel sets: {5a, 5b, 5k}; after 5d: {5e, 5g, 5h}; 5f alongside 5i and 5j.
```

## 12. Decisions for the owner, and open decisions

**Made here; the owner must see them (none is recorded as owner approval):**

| # | Decision | Why it needs the owner's eye |
| --- | --- | --- |
| P1 | Mute is not persisted; every start is unmuted | Product behaviour; persisting later means an additive `user-settings` field |
| P2 | A failed or missing critical probe blocks readiness and the visual, but not the audio | EVENT-AUDIO-SPEC section 6 step 3 says the binding becomes `invalid_media`; this design narrows that so a stalled renderer cannot silence a safety cue |
| P3 | All playable clips are decoded into memory (at most 37.4 MB, 74.9 MB during a swap) | Uses part of RAM (no limit is set; DEC-RESOURCE-BUDGET) |
| P4 | When `health_danger` is unmapped, a same-tick ordinary event (for example `health_low`) still plays | Could be read as a weaker cue standing in for the safety cue; ALERT-007 forbids substitution, and this is the event's own clip |
| P5 | Probe duration tolerance 42 ms; `path_sha256` hashes the descriptor-relative path; activation barrier ends when the game process is gone | Thresholds and definitions the spec leaves open |
| P6 | Tier lookup follows the owner's 2026-11-11 rule (highest bound tier not above) in the host table, while the store stays exact | EVENT-AUDIO-SPEC 7.1 text says "no fallback between tiers"; section 2 and the CHANGELOG say the opposite. The spec text needs one sentence reconciling them |
| P7 | New dependencies of the desktop host: the three workspace crates (and through `kohbor-audio-native`, `wasapi` 0.26.0, already in the workspace lock). No new third-party crate, no plugin, no network use | Dependency rule of AGENTS.md |
| P8 | Four additive methods in the reviewed `kohbor-audio` owner; CSP gains a `media-src` entry | Touches reviewed code and a security setting |

**Touching DEC-CRITICAL-MEDIA:** nothing here creates, selects or substitutes critical media. With no authorized clip the product reports `not_ready / MEDIA_UNMAPPED` and `health_danger` is recorded `unmapped`.

**Open decisions and what they block:**

| Decision | Blocks | Proceeds anyway |
| --- | --- | --- |
| DEC-CRITICAL-MEDIA (open) | Any `ready` state on real media; QUALITY-001 acceptance; UAT-03 | All of S3-5 and S3-6 with generated test clips in temporary stores |
| DEC-INTERFACES (open) | External authoring-app transport; freezing the command names of sections 4 and 5 as a contract | The Tauri commands as host-local proposals |
| DEC-PACK-CRYPTO (open) | Publisher or store authority | `local_import` packages |
| DEC-EVENT-AUDIO (proposed) | Freezing audio-outcome 1.0.0 and the outcome set | Implementation against the proposal |
| E-1 (first blood) | Voicing `first_blood` | It is recorded `detector_unavailable` |
| Owner hardware session | Device loss, default-device change, acoustic latency evidence | Synthetic and native-silence evidence, reported as such |

## 13. S3-6a result and S3-7 review dispositions (2026-11-13)

Written by the lead after the independent read-only reviews S3-7a (S3-5l) and S3-7c (S3-6a and the S3-7 fixes). Agent reviews only; no owner approval, no native, live or acoustic evidence.

**S3-6a stage A (synthetic, engineering fixture).** `apps/desktop/src-tauri/src/latency.rs` and `tests/latency_harness.rs`. The test is `#[ignore]` because the burst trace is replayed in real time (about 2 s per sample, about 10 minutes for 300): `cargo test -p kohbor-desktop --test latency_harness --offline --locked -j 4 -- --ignored`. It drives `Shared::accept`, the engine thread and the publisher with a silent fake sink and a generated clip, collects 300 admitted `health_danger` samples (one new match id per sample), writes `latency-stage-a.receipt.json` and `latency-stage-a.samples.json` under `<target>/evidence/s3-6/` (untracked) and validates the receipt against `contracts/evidence.schema.json`. The receipt is `synthetic`, status `INDETERMINATE`, requirement `QUALITY-001`, with the literal limitation "engineering fixture; not QUALITY-001". **QUALITY-001 stays NOT_RUN**: T0 here is the `arrival` passed to `accept`, T3' is the sink hand-off, nothing is audible, and the clip is generated.

| Run | Source revision | T0 to T3' p50 / p95 / max (µs) | Note |
| --- | --- | --- | --- |
| 1a | `8ab8d94`, dirty tree (worker run) | 257 / 4,182 / 266,293 | Other builds may have run on the machine |
| 1b | `8ab8d94`, dirty tree | 223 / 4,040 / 63,783 | Same |
| 2 | `77277b2`, clean | 192 / 1,241 / 4,450 | No stall above 5 ms; not kept |
| final | `1f41c67`, clean | 198 / 1,781 / 92,488 | Receipt and samples kept in `docs/delivery/evidence/s3-6a/` (the receipt's `raw_evidence_paths` is the lane path of the run; the sample file was copied unchanged) |

Every run is a distribution on one developer machine under uncontrolled load. Single stalls of 60 to 266 ms appeared in three of the four runs, including the final run on a clean tree with no build of this session running (other processes, such as the banner lab session or a game, were not controlled). They show that scheduler or I/O stalls of tens of milliseconds occur on this machine and that nothing about the QUALITY-001 hard maximum of 300 ms can be inferred from p50 or p95 of this harness; the real measurement needs loopback audio on the owner's hardware. They are not explained by the code (the session-start write is on the baseline tick, not the trigger tick), and no outlier is removed. The harness does not include the HTTP read, JSON parse, registry read or the `Shared` lock held by status commands, so it does not quantify F2/F3.

**Dispositions of review findings.**

| # | Finding | Disposition |
| --- | --- | --- |
| S3-7a blocker | Unbounded `stop_engine` and joins at shutdown | Fixed (bounded attempts, `join_with_deadline`), tested |
| 1 | `detected.clone()` in `Shared::accept` | Accepted. It runs only on a tick that detected an event, after T1 and before the non-blocking offer; the vector holds a handful of events. Independent review S3-7c agreed |
| 2 | `media_inspect` size check then read to EOF | Fixed: `kohbor_pack::container::sha256_file_limited` reads at most limit + 1 bytes from the opened handle, rejects non-regular files by handle metadata, and `inspect_with_limit` requires the hashed length to equal both metadata snapshots. A symlink to a regular file is followed on purpose (user-selected path; install re-verifies the hash). Content changed at equal length between inspect and install is caught by the install-time hash, not by inspect |
| 3a | Protocol mutex held while the server stops | Fixed: dispatch clones a sender under the lock and releases it; `stop_protocol` takes the server out first |
| 3b | Protocol server join unbounded | Fixed in the same pass: `ProtocolServer::stop` uses `join_with_deadline` (5 s, detach) |
| 3c | `kohbor-media` ignores origin/authority | Accepted risk, **not proven closed**. The scheme is served inside the app's own webview only, the CSP is `default-src 'self'` with `media-src` limited to the scheme, there is no remote navigation, and the worker re-validates install id, token, length and modification time. The token is a SHA-256 of a descriptor-relative path, so it is a handle, not a secret. Review S3-7c disagrees that CSP proves the caller; the open question (what Tauri/WebView2 guarantees about who can request a registered scheme on Windows) needs the dependency's documentation or native verification on the owner's machine |
| 4 | Legacy commands have no shutdown gate | Narrowed: `gsi_setup` and `settings_patch` (the mutators) now answer `CANCELLED` once shutdown starts, before and after taking the settings gate. Read-only commands stay ungated. Writes were already atomic |
| 5 | F2/F3: registry read per payload, file write at session start, `Shared` mutex held by status commands | Inherited and still open. The stage A harness does not measure them; a harness variant that goes through `serve` is needed before any claim |
| S3-7c nit | `latency` is an always-compiled public module | Accepted: no production call site; smallest fix, if wanted, is a harness-only cargo feature that gates the module and the integration test |
| flaky | `host::tests::stop_engine_is_bounded_when_the_engine_is_gone_or_the_lane_stays_full` asserts under 3 s and failed once in a loaded full-suite run, then passed in every other run | Recorded; the bound is 100 attempts of a 10 ms sleep. Not weakened. Re-examine if it fails again |

**S3-6b stage B (native, digital silence only; owner approved 2026-11-13).** Hook: cargo feature `latency-probe` in `kohbor-audio-native`, off by default and enabled only through a `[dev-dependencies]` entry of the desktop crate (resolver 2: a normal app build does not enable it). The render thread records a monotonic timestamp at the first `fill` that copies a submitted playback, published through a bounded seqlock (no allocation, lock or I/O). Harness: `tests/latency_harness_native.rs` (`#[ignore]`, about 10 minutes): `-- --ignored`; smoke mode (`KOHBOR_STAGE_B_SAMPLES` below 300) writes no file. With no render endpoint it writes a `NOT_RUN` receipt and passes.

| Run | Source revision | T0 to T3'' min / p50 / p95 / max (µs) | Note |
| --- | --- | --- | --- |
| final | `6333241`, clean | 206 / 7,995 / 10,101 / 13,649 | 300 samples, default render endpoint, all-zero clip; receipt and samples in `docs/delivery/evidence/s3-6b/`; no endpoint name or hardware recorded |

The spread of about 0 to 13 ms is the phase of the roughly 10 ms event-driven render period relative to the trigger, and is consistent with the S3-1 spike (8.6 to 11.2 ms producer-to-render-service). T3'' is the first render service that copied the buffer: not audible, not loopback, not QUALITY-001. The review S3-6b (read-only, BLOCK) found a pairing race in the probe (fixed), smoke mode writing a NOT_RUN receipt (fixed), T0 trigger identification (now verified within 2 ms) and revision-to-binary binding (accepted: the run builds and executes in one invocation on a clean tree; the receipt records HEAD and the executable hash, but a stale binary run later would not be detected). The 300-sample result is one run on one machine; no warm-up or outlier was removed.
