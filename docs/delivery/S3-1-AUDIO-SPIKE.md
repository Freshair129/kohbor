# S3-1 Windows Native Audio Spike

## Result and evidence key

Recommendation: use `wasapi` 0.26.0 for the first Windows output adapter in `kohbor-audio`. Keep one shared-mode, event-driven stream open on one audio-owner thread; publish predecoded buffers and cancellation state through a bounded lock-free handoff. This is an engineering recommendation from native silence-only observations, not QUALITY-001 acceptance.

- **[NATIVE-SILENCE]** Windows endpoint/stream observations on this machine. Every stream rendered only zero PCM or used WASAPI's silent-buffer flag.
- **[SOURCE/CARGO]** Cached crate source, package graph, source/build checks, or spike code inspection.
- **[TASK INPUT]** Required input format: mono 48,000 Hz signed 16-bit PCM.
- **[NOT_RUN]** Requires owner hardware action or acoustic output; no result is inferred.

The standalone workspace is `spikes/audio/`; it has its own `[workspace]` and `Cargo.lock`. Cargo ran offline with the process-scoped target directory in the owner-authorized external lane. No source-brand media, clips, tones, files, or nonzero samples were used. No audio setting, device, volume, or default was changed. The main workspace manifest and lockfile were untouched.

Toolchain [SOURCE/CARGO]: `cargo 1.96.0`, `rustc 1.96.0`.

Artifact identity at review:

- Checkout HEAD at final review: `4961048f9398720e6af342baf61f3ed88a053af2` [SOURCE/CARGO]. A concurrent commit updated `crates/kohbor-audio` outside this spike workspace; the spike files are uncommitted and independently bound by the source-manifest hash below.
- SHA-256 of sorted relative-path/file-hash manifest for all files under `spikes/audio/`: `c375048a9ca8e57285fb77c0a832a1df3f15c9825bc71a176d90b6d751f7c20b` [SOURCE/CARGO].
- SHA-256 of the standalone `Cargo.lock`: `48864c8cf0d306a34992a3889650469a435a06d20333c4745af308e497b66221` [SOURCE/CARGO].

## 1. API, format, period, and dependency comparison

All four candidates built offline and opened the current default render device in shared mode. Dependency counts are unique normal dependency package IDs (direct and transitive, excluding the spike package), from `cargo tree`; development dependencies are excluded. Counts describe each isolated candidate closure, not the marginal additions after merging into a future Kohbor workspace lockfile. Each per-candidate result row is tagged with its evidence class.

| Candidate | Offline build / dependency count | Native open and format result | Period / latency information |
|---|---|---|---|
| `cpal` 0.16.0 | Build exit 0; 7 packages [SOURCE/CARGO]. | Opened CPAL's WASAPI host in shared mode. Default config was 48,000 Hz, 2-channel F32. The supported-config scan found no exact mono/48,000/I16 config; the callback converted I16 samples to the default sample format and duplicated mono into output channels. [NATIVE-SILENCE] | CPAL reported `buffer=Default`; the first callback block was 1,056 frames = 22.000 ms at 48,000 Hz. CPAL exposes neither `GetDevicePeriod` nor output latency here. [NATIVE-SILENCE] |
| `rodio` 0.19.0 with `default-features=false` (uses `cpal` 0.15.3) | Build exit 0; 14 packages [SOURCE/CARGO]. | Opened the default output in shared mode at 48,000 Hz, 2-channel F32. Rodio's `Sink` takes an F32 `Source`; the spike converted I16 to F32 in its source. This API does not accept an `Arc<[i16]>` directly. [NATIVE-SILENCE] | The exposed default stream config has no period or output-latency value. `Sink` does not expose the underlying device period. [NATIVE-SILENCE/SOURCE] |
| `wasapi` 0.26.0 | Build exit 0; 22 packages [SOURCE/CARGO]. | Shared event-driven initialization accepted the required mono/48,000/I16 format with `autoconvert=true`. The stream format reported 1 channel/16-bit integer; the endpoint mix format was 48,000 Hz/2-channel/32-bit float. WASAPI performed shared-mode format conversion. [NATIVE-SILENCE] | `GetDevicePeriod`: default 10.000 ms, minimum 3.000 ms. `GetBufferSize`: 1,056 frames = 22.000 ms. `GetStreamLatency`: 0.000 ms as returned by this endpoint/API call; this is not acoustic latency. [NATIVE-SILENCE] |
| `windows` 0.62.2 direct COM | Build exit 0; 15 packages [SOURCE/CARGO]. | `IsFormatSupported` returned false for exact mono/48,000/I16, but shared `Initialize` with `EVENTCALLBACK | AUTOCONVERTPCM | SRC_DEFAULT_QUALITY` succeeded. Active stream format was 1 channel/16-bit PCM; mix format was 48,000 Hz/2-channel/32-bit extensible float. [NATIVE-SILENCE] | `GetDevicePeriod`: default 10.000 ms, minimum 3.000 ms. `GetBufferSize`: 1,056 frames = 22.000 ms. `GetStreamLatency`: 0.000 ms as returned by the API; this is not acoustic latency. [NATIVE-SILENCE] |

`wasapi` therefore gives the adapter the requested PCM format and lets WASAPI convert it to this endpoint's mix format. With CPAL, the engine must convert I16 to the configured output sample type/channel layout; if a future endpoint reports a different sample rate, the engine also needs a real resampler. Rodio needs an F32 source adapter and its public Sink path adds queue/mixer behavior. `hound` 3.5.1 was not evaluated: WAV decoding is outside this output-API comparison, and the spike intentionally read no file. [SOURCE/CARGO]

## 2. Render endpoint inventory

The device-list binary used `IMMDeviceEnumerator` for render endpoints and printed only friendly name, endpoint ID, default flag, and state. `[NATIVE-SILENCE]` applies to every row. `default=true` means the endpoint was the default for at least one of Console, Multimedia, or Communications roles; the flag is the union of those roles, not a single-role claim. The inventory contained 18 endpoints: 4 Active, 2 Unplugged, and 12 NotPresent.

The per-endpoint table (friendly names and endpoint IDs of the owner's hardware) is deliberately NOT committed: device names and IDs identify the owner's equipment and are not needed for the recommendation. The spike binary prints them locally. Observed on this machine [NATIVE-SILENCE]: 18 render endpoints; 4 Active (2 of them default for at least one role), 2 Unplugged, 12 NotPresent; the Active set included USB audio interfaces and monitor/HDMI outputs, so a device change between active endpoints is a realistic case for the owner.

## 3. Submit-to-consumer timing and preemption

Each metric below is a native silence-only fixture, measured with `Instant` over 200 iterations per candidate. These are control-to-consumer or event-service intervals, not T0-to-acoustic-T3 latency. The APIs do not all have the same callback boundary: CPAL invokes a data callback, Rodio consumes `Source::next` on its mixer thread, and the two WASAPI implementations service a render event on the audio-owner thread.

| Candidate / measured interval | Samples | Min (ms) | p50 (ms) | p95 (ms) | Max (ms) | Evidence |
|---|---:|---:|---:|---:|---:|---|
| CPAL submit marker to first data callback selecting/reading the submitted prebuilt zero-PCM slot | 200 | 4.034100 | 10.004900 | 10.580700 | 19.987300 | [NATIVE-SILENCE] |
| Rodio `Sink::append` to first `Source::next` consuming the submitted zero sample | 200 | 0.001300 | 0.001700 | 10.599100 | 20.775800 | [NATIVE-SILENCE] |
| `wasapi` producer marker to next event render service | 200 | 8.750100 | 9.980800 | 10.657200 | 11.142100 | [NATIVE-SILENCE] |
| Direct Windows producer marker to next event render service | 200 | 8.597300 | 10.034800 | 10.642200 | 11.230200 | [NATIVE-SILENCE] |

Rodio reported `source_next_on_submit_thread n=0`: every measured source read occurred on a different thread from the submitting thread. Its low p50 is time to the first mixer-source read only; it is not evidence that the system audio engine or hardware has rendered that sample. For the event-driven APIs, the marker is sent from a producer thread while the MTA owner waits for an event; the measure ends when that owner next services the render event. Their event loop used a preallocated zero buffer and `AUDCLNT_BUFFERFLAGS_SILENT`/the wrapper's equivalent.

The following is API-call-pair time only; it does not wait for the first post-restart render. All rows are native silence-only observations over 200 iterations.

| Candidate / calls | Samples | Min (ms) | p50 (ms) | p95 (ms) | Max (ms) | Evidence |
|---|---:|---:|---:|---:|---:|---|
| CPAL `pause()` + `play()` | 200 | 0.001200 | 0.001300 | 0.002100 | 0.005100 | [NATIVE-SILENCE] |
| Rodio `Sink::pause()` + `play()` (Sink state only) | 200 | 0.000000 | 0.000100 | 0.000100 | 0.000500 | [NATIVE-SILENCE] |
| `wasapi` `Stop()` + `Start()` | 200 | 0.238700 | 0.253700 | 0.340900 | 0.469600 | [NATIVE-SILENCE] |
| Direct Windows `Stop()` + `Start()` | 200 | 0.243400 | 0.255900 | 0.376800 | 0.415600 | [NATIVE-SILENCE] |

Keep one stream open. A cancel should atomically mark a playback ID/generation inactive; the next callback/event writes silence and retires that buffer. The submit-to-consumer numbers above estimate how long a buffer/state swap waits for that consumer boundary. They do not establish cancellation-to-acoustic-silence time. The WASAPI stop/start timings are API round trips, and Rodio's pair does not stop its device stream.

## 4. Device change and loss watch

All four `--watch` modes were started with silence; each printed `event=WatchStarted t_ms=0` and was stopped by terminating only its own exact process [NATIVE-SILENCE]. No physical endpoint was unplugged, disabled, or changed. Physical loss and default-change results are **[NOT_RUN]**.

| API | Default-device change | Removal/state change | Stream invalidation |
|---|---|---|---|
| CPAL | No `IMMNotificationClient` surfaced by CPAL here; watcher polls the default friendly name every 250 ms and timestamps name changes. | No endpoint identity/state callback in this adapter; the WASAPI stream error callback can report an error. | CPAL `StreamError` callback is available; the spike forwards it to the watcher. All source facts [SOURCE/CARGO]; polling configuration [SOURCE/CARGO]. |
| Rodio | Watcher polls default friendly name every 250 ms; no endpoint ID. | No `Sink` event for endpoint state. Rodio's internal CPAL error callback writes to stderr; the `Sink` user API has no runtime error event callback. | Build/play errors return during setup; ongoing output errors are logged internally, not returned as `SinkEvent`. [SOURCE/CARGO] |
| `wasapi` crate | `DeviceEventCallbacks` surfaces default-device changes with direction, role, and ID. | Callbacks surface device added, removed, and state changed notifications. | Event wait/render service returns errors; watch reports `StreamInvalidated` and stops using the invalid stream. [SOURCE/CARGO] |
| Direct Windows | `IMMNotificationClient::OnDefaultDeviceChanged` provides flow, role, and endpoint ID. | `OnDeviceAdded`, `OnDeviceRemoved`, and `OnDeviceStateChanged` provide endpoint notifications. | `WaitForSingleObject` failure or render-client errors are reported as `StreamInvalidated`. [SOURCE/CARGO] |

The direct Windows handler received `DevicePropertyChanged` callbacks at `t_ms=1102` during a normal stream run and `t_ms=1096` during watcher startup, both for the default USB audio endpoint [NATIVE-SILENCE]. No device removal, state change, or default-device change was induced. These property callbacks do not prove a loss/recovery path.

**Owner action:** run one watcher at a time, with `$lane` set to the authorized external target directory: `& (Join-Path $lane 'debug\audio-cpal-spike.exe') --watch`, `& (Join-Path $lane 'debug\audio-rodio-spike.exe') --watch`, `& (Join-Path $lane 'debug\audio-wasapi-spike.exe') --watch`, or `& (Join-Path $lane 'debug\audio-windows-spike.exe') --watch`. For loss, unplug or disable the active endpoint the watcher reports as the default (the watcher prints its name locally) and expect a timestamped removal/state event and, if the existing stream is invalidated, `StreamInvalidated`. For a default change, use Windows Sound Settings to choose another active endpoint and expect `DefaultDeviceChanged`; CPAL/Rodio should report a friendly-name change on their next poll if that name differs. Event order is OS-dependent. Re-enable/reconnect before rerunning any native stream test. The agent did not perform these actions.

Expected output forms for a future owner run (forms from source, not observations of a physical loss/default change) [SOURCE/CARGO, NOT_RUN]:

```text
event=DeviceStateChanged t_ms=<monotonic-ms> id=<endpoint-id> state=<state>
event=DeviceRemoved t_ms=<monotonic-ms> id=<endpoint-id>
event=StreamInvalidated t_ms=<monotonic-ms> detail=<native-error>
event=DefaultDeviceChanged t_ms=<monotonic-ms> direction=Render role=<role> id=<endpoint-id>
event=DefaultDeviceChanged t_ms=<monotonic-ms> name=<friendly-name>
```

## 5. Threading and critical path

- CPAL creates a WASAPI worker thread and invokes the data callback there. The default cached Windows backend path sets `THREAD_PRIORITY_TIME_CRITICAL` when its optional `audio_thread_priority` feature is not enabled. CPAL owns its COM/thread details; the application callback must still avoid blocking work. [SOURCE/CARGO]
- Rodio's mixer runs through CPAL's callback thread. Source inspection found mutex-protected controls/queues, `Sink::append` queue locking, and boxed source construction on append. This is a poor fit for a critical submit path. [SOURCE/CARGO]
- The `wasapi` crate does not require an application callback thread; this spike waited on the render event and wrote from the calling thread after `initialize_mta()`. The direct Windows version does the same with explicit `CoInitializeEx(COINIT_MULTITHREADED)`. The event-driven owner should be a dedicated MTA thread that owns `IAudioClient`, `IAudioRenderClient`, and the event handle. Neither implementation in the spike set MMCSS/real-time priority; priority behavior was not measured. [SOURCE/CARGO; priority NOT_RUN]
- Use one bounded SPSC command queue from a single coordinator to the audio owner. If multiple callers exist, serialize them before this boundary. `submit` should publish an already-decoded buffer/slot and playback ID without filesystem, network, allocation, or a lock shared with the renderer. The audio owner consumes commands at render events and fills output or marks the WASAPI buffer silent. Use a reverse bounded event/retirement queue for `Finished`, device events, and buffer reclamation.
- If passing `Arc<[i16]>`, move/clone it before the real-time boundary and ensure the callback cannot perform the last `Arc` drop. Return retired buffers to the coordinator or use preallocated slots/generation IDs. An atomic raw buffer pointer needs an explicit lifetime/reclamation scheme; an atomic pointer swap alone is not sufficient.

## 6. Recommendation and proposed `Sink`

Pin `wasapi` 0.26.0 as the initial Windows backend. It opened the requested PCM in shared event mode with auto-conversion, reported period/buffer details, and exposes `IMMNotificationClient`-backed device notifications in one adapter. Its 22-package graph is larger than direct `windows` at 15, but it avoids putting the COM initialization, unsafe buffer/service calls, and notification plumbing directly into the Kohbor adapter. Keep direct `windows` as a fallback if future requirements need an unsupported WASAPI control. This is a source/native-spike judgment; it does not resolve `DEC-CRITICAL-MEDIA` or validate a real alert.

```rust
pub type PlaybackId = u64;

pub enum SinkEvent {
    Finished { playback_id: PlaybackId },
    DeviceLost { endpoint_id: Option<String> },
    DeviceChanged { endpoint_id: Option<String> },
}

pub trait Sink {
    fn submit(
        &mut self,
        playback_id: PlaybackId,
        pcm: std::sync::Arc<[i16]>, // mono, 48,000 Hz, signed 16-bit PCM
    ) -> Result<(), SubmitError>;
    fn cancel(&mut self, playback_id: PlaybackId);
    fn poll_event(&mut self) -> Option<SinkEvent>;
}
```

The public `Sink` is called on a non-real-time coordinator thread. Its implementation owns a bounded queue; one dedicated MTA audio-owner thread owns the WASAPI client, render client, and event handle. The callback/event service never takes a renderer-held lock. It reads the active playback/generation, mixes or copies PCM, emits silence after cancel, and sends completion/device events through a bounded queue. Device notification callbacks should only copy a compact event and enqueue it; reopen/recovery and buffer destruction belong on the owner/coordinator path, not in the render callback.

## 7. Commands, build results, and hashes

Commands were run from `spikes/audio/`; `CARGO_TARGET_DIR` pointed to the authorized external lane. Every Cargo operation used `--offline`; no network fetch occurred.

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all` | 0 | Formatted spike workspace. [SOURCE/CARGO] |
| `cargo build --offline` | 0 | Built all five spike packages. [SOURCE/CARGO] |
| `cargo build --offline --locked -p audio-device-list` | 0 | Device inventory binary built. [SOURCE/CARGO] |
| `cargo build --offline --locked -p audio-cpal-spike` | 0 | CPAL candidate built. [SOURCE/CARGO] |
| `cargo build --offline --locked -p audio-rodio-spike` | 0 | Rodio candidate built. [SOURCE/CARGO] |
| `cargo build --offline --locked -p audio-wasapi-spike` | 0 | WASAPI crate candidate built. [SOURCE/CARGO] |
| `cargo build --offline --locked -p audio-windows-spike` | 101 | Initial isolated build exposed missing package-local Windows features. [SOURCE/CARGO] |
| `cargo build --offline --locked -p audio-windows-spike` | 0 | Direct Windows candidate built in isolation. [SOURCE/CARGO] |
| `cargo fmt --all --check` | 0 | Formatting check passed. [SOURCE/CARGO] |
| `& (Join-Path $lane 'debug\audio-device-list.exe')` | 0 | Printed the endpoint inventory above. [NATIVE-SILENCE] |
| `& (Join-Path $lane 'debug\audio-cpal-spike.exe')` | 0 | Opened shared stream; 200 timing samples. [NATIVE-SILENCE] |
| `& (Join-Path $lane 'debug\audio-rodio-spike.exe')` | 0 | Opened shared stream; 200 timing samples. [NATIVE-SILENCE] |
| `& (Join-Path $lane 'debug\audio-wasapi-spike.exe')` | 0 | Opened shared stream; 200 timing samples. [NATIVE-SILENCE] |
| `& (Join-Path $lane 'debug\audio-windows-spike.exe')` | 0 | Opened shared stream; 200 timing samples. [NATIVE-SILENCE] |
| `cargo tree --offline --locked -p audio-cpal-spike -e normal --prefix none` | 0 | 7 unique non-root packages. [SOURCE/CARGO] |
| `cargo tree --offline --locked -p audio-rodio-spike -e normal --prefix none` | 0 | 14 unique non-root packages. [SOURCE/CARGO] |
| `cargo tree --offline --locked -p audio-wasapi-spike -e normal --prefix none` | 0 | 22 unique non-root packages. [SOURCE/CARGO] |
| `cargo tree --offline --locked -p audio-windows-spike -e normal --prefix none` | 0 | 15 unique non-root packages. [SOURCE/CARGO] |

The first direct-Windows isolated build failed because the spike manifest lacked `Win32_Security`, `Win32_System_Com_StructuredStorage`, and `Win32_System_Variant`. These package-local features were added to `windows-output/Cargo.toml`; the isolated rebuild then passed. No main-workspace manifest was changed. [SOURCE/CARGO]

For each `--watch` mode, PowerShell used `$exe = Join-Path $lane "debug\$name.exe"`, then `Start-Process -FilePath $exe -ArgumentList '--watch' -NoNewWindow -PassThru`; it observed `WatchStarted`, waited 1,200 ms, then terminated that exact PID. These processes were intentionally stopped and have no natural exit code. [NATIVE-SILENCE]

After the final CPAL output-label source edit, the final pass repeated `cargo fmt --all --check`, `cargo build --offline`, `cargo build --offline --locked -p audio-cpal-spike`, and the CPAL runtime; all exited 0. The CPAL figures in section 3 are from that last runtime. [SOURCE/CARGO; NATIVE-SILENCE]

SHA-256 identities of native debug artifacts [SOURCE/CARGO]:

| Artifact | SHA-256 |
|---|---|
| `audio-device-list.exe` | `f5627b852d7aeb9c32dadb61b97a5022a75a45fdb689a95a2848e7ce000108c4` |
| `audio-cpal-spike.exe` | `597c31932768c95a4649386f26986395d1ab9e3fb87602a0881b5a33f0fc7269` |
| `audio-rodio-spike.exe` | `c1f1208c438ab9ef5eb752f5d84bf707ba8b6299af7cbd139962ea701c86801a` |
| `audio-wasapi-spike.exe` | `2d8c241890dce027cc178e1666e95ff4fd567196c8b4edff4b626100e4a8439f` |
| `audio-windows-spike.exe` | `dcb418c4f2f9a7f0ff7362bbc96c33d0b6c5fc969c719469b5c037dc9b42d05e` |

## Not measured / remaining owner work

- **[NOT_RUN] Acoustic latency:** no loopback capture, audible T3, or QUALITY-001 acceptance sample set. `GetStreamLatency=0.000 ms` is only the API value observed here; it is not physical output evidence.
- **[NOT_RUN] Device loss/change:** no unplug, disable, re-enable, or default-device switch. Owner should run a single `--watch` candidate and perform the actions in section 4.
- **[NOT_RUN] Recovery:** no stream reopen/rebind/recovery behavior was exercised after endpoint loss.
- **[NOT_RUN] Real media:** no Kohbor pack, decoded nonzero PCM, real alert, or MEDIA-005 paired visual lifecycle was exercised. `DEC-CRITICAL-MEDIA` remains open and no authorized critical media was available.
- **[NOT_RUN] Real-time admission:** no MMCSS priority, CPU/underrun/xrun profile, allocation profiler, lock-contention probe, or cross-machine/device compatibility matrix.
- **[NOT_RUN] Product integration:** no changes to `crates/kohbor-audio`, desktop app, RTM, installer, or release path; no native product acceptance.

Source checks and native silence observations above do not substitute for acoustic, live-device-loss, or product acceptance evidence.
