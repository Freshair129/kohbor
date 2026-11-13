#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_summary_rejects_empty_and_preserves_extremes() {
        assert!(summarize(&[]).is_err());
        let s = summarize(&(1..=100).rev().collect::<Vec<_>>()).unwrap();
        assert_eq!(
            (s.count, s.min, s.p50, s.p95, s.p99, s.max),
            (100, 1, 50, 95, 99, 100)
        );
        assert_eq!(summarize(&[42]).unwrap().p99, 42);
    }

    #[test]
    fn stage_a_receipt_records_invocation_and_measurement_limitations() {
        let report = StageAReport {
            samples: vec![crate::audio::publisher::LatencySample {
                instance_id: "synthetic".into(),
                event_type: Some("health_danger".into()),
                t0_ms: 0,
                t1_us: 3,
                t2_us: 7,
                t3p_us: 12,
            }],
        };
        let receipt = build_receipt(&report, Path::new("synthetic.samples.json")).unwrap();

        assert_eq!(receipt["status"], "INDETERMINATE");
        assert_eq!(
            receipt["commands"][0],
            "cargo test -p kohbor-desktop --test latency_harness --offline --locked -j 4 -- --ignored"
        );
        let limitations = receipt["limitations"].as_array().unwrap();
        for expected in [
            "engineering fixture; not QUALITY-001",
            "T0 is the arrival Instant passed to Shared::accept: HTTP read, JSON parse, the registry read and the serve-side Shared lock are not in the measurement; F2 and F3 are not quantified",
            "no warm-up discarded and no outlier removed; samples include scheduler stalls of this machine",
            "one session epoch per burst trace (new match id), measured on a developer machine under no controlled load",
        ] {
            assert!(
                limitations.iter().any(|value| value == expected),
                "missing receipt limitation: {expected}"
            );
        }
    }

    #[test]
    fn stage_b_receipt_records_native_fixture_and_keeps_device_identity_unknown() {
        let report = StageBReport {
            samples: vec![StageBSample {
                sample: crate::audio::publisher::LatencySample {
                    instance_id: "synthetic".into(),
                    event_type: Some("health_danger".into()),
                    t0_ms: 0,
                    t1_us: 3,
                    t2_us: 7,
                    t3p_us: 12,
                },
                t3pp_us: 20,
            }],
        };
        let receipt = build_stage_b_receipt(&report, Path::new("stage-b.samples.json")).unwrap();

        assert_eq!(receipt["evidence_class"], "native");
        assert_eq!(receipt["status"], "INDETERMINATE");
        assert_eq!(receipt["scenario_id"], "S3-6b-stage-b-engineering-fixture");
        assert_eq!(receipt["requirement_ids"][0], "QUALITY-001");
        assert_eq!(
            receipt["commands"][0],
            "cargo test -p kohbor-desktop --test latency_harness_native --offline --locked -j 4 -- --ignored"
        );
        assert_eq!(
            receipt["observed_result"],
            "T0 to T3'' microseconds: count=1, p50=20, p95=20, max=20"
        );
        assert_eq!(receipt["environment"]["os_build"], Value::Null);
        assert_eq!(receipt["environment"]["hardware"], Value::Null);
        assert_eq!(receipt["environment"]["game_patch"], Value::Null);
        assert_eq!(receipt["environment"]["profile"], Value::Null);
        assert_eq!(receipt["raw_evidence_paths"][0], "stage-b.samples.json");
        assert_eq!(receipt["artifact_sha256"], artifact_sha256().unwrap());

        fn assert_no_device_identity_fields(value: &Value) {
            match value {
                Value::Object(fields) => {
                    for (key, child) in fields {
                        let normalized = key.to_ascii_lowercase();
                        assert!(!normalized.contains("endpoint"));
                        assert!(!normalized.contains("device_id"));
                        assert!(!normalized.contains("friendly"));
                        if normalized == "hardware" {
                            assert_eq!(child, &Value::Null);
                        }
                        assert_no_device_identity_fields(child);
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        assert_no_device_identity_fields(item);
                    }
                }
                _ => {}
            }
        }
        assert_no_device_identity_fields(&receipt);

        let limitations = receipt["limitations"].as_array().unwrap();
        for expected in [
            "engineering fixture; not QUALITY-001",
            "native silence only: default render endpoint, shared-mode event-driven stream, all-zero generated clip, no audible output",
            "T3'' is the first render service that copied the buffer, not loopback-audible sound",
            "generated test clip, not authorized critical media",
            "DEC-CRITICAL-MEDIA open",
            "T0 is the arrival Instant passed to Shared::accept: HTTP read, JSON parse, the registry read and the serve-side Shared lock are not in the measurement; F2 and F3 are not quantified",
            "no warm-up discarded and no outlier removed; samples include scheduler stalls of this machine",
            "QUALITY-001 remains NOT_RUN",
        ] {
            assert!(
                limitations.iter().any(|value| value == expected),
                "missing receipt limitation: {expected}"
            );
        }
    }

    #[test]
    fn not_run_receipt_records_reason_and_execution_time() {
        let receipt = not_run_receipt("no active render endpoint").unwrap();

        assert_eq!(receipt["status"], "NOT_RUN");
        assert_eq!(receipt["observed_result"], Value::Null);
        assert!(receipt["executed_at_utc"].as_str().is_some());
        assert!(receipt["limitations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "no active render endpoint"));
    }

    #[test]
    fn missing_first_fill_is_an_error() {
        let reader = |_id: kohbor_audio::PlaybackId| None;
        assert!(wait_for_first_fill(&reader, kohbor_audio::PlaybackId(9), Duration::ZERO).is_err());
    }

    #[test]
    fn stage_b_t0_matches_trigger_arrival_with_two_millisecond_tolerance() {
        let origin = Instant::now();
        let clock = MonoClock::from_origin(origin);
        let arrival = origin + Duration::from_millis(10);

        assert!(stage_b_t0_matches(12, arrival, clock));
        assert!(!stage_b_t0_matches(13, arrival, clock));
    }

    #[test]
    fn stages_reject_out_of_order_timestamps() {
        let mut sample = crate::audio::publisher::LatencySample {
            instance_id: "fixture".into(),
            event_type: None,
            t0_ms: 8,
            t1_us: 3,
            t2_us: 7,
            t3p_us: 12,
        };
        assert_eq!(deltas(&sample).unwrap(), [3, 4, 5, 12]);
        sample.t2_us = 2;
        assert!(deltas(&sample).is_err());
    }

    #[test]
    fn production_engine_disables_latency() {
        let host = include_str!("host.rs");
        assert!(host.contains("Engine::spawn(clock, engine_shared.clone(), out_tx.clone(), false)"));
    }

    #[test]
    fn harness_source_has_no_network_endpoint_or_ui_access() {
        let source = include_str!("latency.rs");
        for forbidden in [
            ["std::", "net"].concat(),
            ["Tcp", "Listener"].concat(),
            ["Tcp", "Stream"].concat(),
            ["Wasapi", "Sink"].concat(),
            ["kohbor_audio_", "native"].concat(),
            ["App", "Handle"].concat(),
            ["tauri", "::"].concat(),
        ] {
            assert!(
                !source.contains(&forbidden),
                "forbidden source token: {forbidden}"
            );
        }
    }
}
use crate::audio::publisher::LatencySample;

#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub count: usize,
    pub min: u64,
    pub p50: u64,
    pub p95: u64,
    pub p99: u64,
    pub max: u64,
}

/// Nearest-rank percentile: sorted element at ceil(percent * count / 100),
/// with ranks starting at one. Empty samples are an error.
pub fn summarize(values: &[u64]) -> Result<Summary, String> {
    if values.is_empty() {
        return Err("empty sample set".into());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = |percent: usize| -> u64 {
        let index =
            (sorted.len() / 100) * percent + ((sorted.len() % 100) * percent).div_ceil(100) - 1;
        sorted[index]
    };
    Ok(Summary {
        count: sorted.len(),
        min: sorted[0],
        p50: rank(50),
        p95: rank(95),
        p99: rank(99),
        max: sorted[sorted.len() - 1],
    })
}

/// T0 is zero; the remaining timestamps are microsecond offsets from T0.
pub fn deltas(sample: &LatencySample) -> Result<[u64; 4], String> {
    let t1_t2 = sample
        .t2_us
        .checked_sub(sample.t1_us)
        .ok_or("T2 precedes T1")?;
    let t2_t3 = sample
        .t3p_us
        .checked_sub(sample.t2_us)
        .ok_or("T3' precedes T2")?;
    Ok([sample.t1_us, t1_t2, t2_t3, sample.t3p_us])
}

use crate::audio::{
    engine::Engine,
    lanes::{Control, EngineShared, OUT_LANE},
    publisher::{self, Emit, HistoryHandle},
    resolve::{Clip, Entry, PrewarmTable},
};
use crate::clock::MonoClock;
use crate::intake::{self, Shared};
use kohbor_audio::{
    pcm::{decode_wav, Pcm},
    PlaybackId, Sink, SinkError, SinkEvent,
};
use kohbor_events::EventId;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct StageAReport {
    pub samples: Vec<LatencySample>,
}

pub struct StageBSample {
    pub sample: LatencySample,
    pub t3pp_us: u64,
}

pub struct StageBReport {
    pub samples: Vec<StageBSample>,
}

pub trait FirstFillReader: Send + Sync {
    fn first_fill(&self, playback_id: PlaybackId) -> Option<Instant>;
}

impl<F> FirstFillReader for F
where
    F: Fn(PlaybackId) -> Option<Instant> + Send + Sync,
{
    fn first_fill(&self, playback_id: PlaybackId) -> Option<Instant> {
        self(playback_id)
    }
}

struct TimedSample {
    sample: LatencySample,
    t3pp_us: Option<u64>,
}

struct SubmitTrackingSink {
    inner: Box<dyn Sink + Send>,
    last_submitted_id: Arc<AtomicU64>,
}

impl Sink for SubmitTrackingSink {
    fn submit(&mut self, id: PlaybackId, pcm: Pcm) -> Result<(), SinkError> {
        let result = self.inner.submit(id, pcm);
        if result.is_ok() {
            self.last_submitted_id.store(id.0, Ordering::Release);
        }
        result
    }

    fn cancel(&mut self, id: PlaybackId) {
        self.inner.cancel(id);
    }

    fn poll_event(&mut self) -> Option<SinkEvent> {
        self.inner.poll_event()
    }
}

struct SilentSink;
impl Sink for SilentSink {
    fn submit(&mut self, _: PlaybackId, _: Pcm) -> Result<(), SinkError> {
        Ok(())
    }
    fn cancel(&mut self, _: PlaybackId) {}
    fn poll_event(&mut self) -> Option<SinkEvent> {
        None
    }
}
struct SilentEmitter;
impl Emit for SilentEmitter {
    fn emit(&self, _: &str, _: &Value) {}
}

fn target_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    exe.ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .ok_or_else(|| "executable is not beneath a Cargo target profile".into())
}
fn random_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn generated_wav() -> Vec<u8> {
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&132_u32.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&48_000_u32.to_le_bytes());
    wav.extend_from_slice(&96_000_u32.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&96_u32.to_le_bytes());
    wav.resize(140, 0);
    wav
}
fn join_bounded(handle: JoinHandle<()>) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return Err("thread join timed out after 5 seconds".into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    handle.join().map_err(|_| "thread panicked".into())
}

fn wait_for_first_fill(
    reader: &dyn FirstFillReader,
    playback_id: PlaybackId,
    timeout: Duration,
) -> Result<Instant, String> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| "first-fill timeout deadline overflowed".to_owned())?;
    loop {
        if let Some(instant) = reader.first_fill(playback_id) {
            return Ok(instant);
        }
        let now = Instant::now();
        let Some(remaining) = deadline.checked_duration_since(now) else {
            return Err(format!(
                "first fill for playback {} did not arrive before timeout",
                playback_id.0
            ));
        };
        if remaining.is_zero() {
            return Err(format!(
                "first fill for playback {} did not arrive before timeout",
                playback_id.0
            ));
        }
        thread::sleep(remaining.min(Duration::from_millis(1)));
    }
}

const FIRST_FILL_TIMEOUT: Duration = Duration::from_secs(5);
const STAGE_B_T0_TOLERANCE_MS: u64 = 2;

fn stage_b_t0_matches(sample_t0_ms: u64, arrival: Instant, clock: MonoClock) -> bool {
    sample_t0_ms.abs_diff(clock.ms_at(arrival)) <= STAGE_B_T0_TOLERANCE_MS
}

/// Run real intake, engine and publisher threads using the burst trace and a generated clip.
pub fn run_stage_a(count: usize) -> Result<StageAReport, String> {
    if count < 300 {
        return Err("stage A requires at least 300 samples".into());
    }
    let samples = run_with_sink(count, Box::new(SilentSink), None)?;
    Ok(StageAReport {
        samples: samples.into_iter().map(|timed| timed.sample).collect(),
    })
}

/// Run the same fixture while recording the first render fill for each admitted playback.
pub fn run_stage_b(
    count: usize,
    sink: Box<dyn Sink + Send>,
    first_fill_reader: &dyn FirstFillReader,
) -> Result<StageBReport, String> {
    if count == 0 {
        return Err("stage B requires at least one sample".into());
    }
    let timed_samples = run_with_sink(count, sink, Some(first_fill_reader))?;
    let mut samples = Vec::with_capacity(timed_samples.len());
    for timed in timed_samples {
        let t3pp_us = timed
            .t3pp_us
            .ok_or_else(|| "stage B sample has no first-fill timestamp".to_owned())?;
        samples.push(StageBSample {
            sample: timed.sample,
            t3pp_us,
        });
    }
    Ok(StageBReport { samples })
}

fn run_with_sink(
    count: usize,
    sink: Box<dyn Sink + Send>,
    first_fill_reader: Option<&dyn FirstFillReader>,
) -> Result<Vec<TimedSample>, String> {
    let submitted_id = first_fill_reader.map(|_| Arc::new(AtomicU64::new(0)));
    let sink = match submitted_id.as_ref() {
        Some(last_submitted_id) => Box::new(SubmitTrackingSink {
            inner: sink,
            last_submitted_id: Arc::clone(last_submitted_id),
        }) as Box<dyn Sink + Send>,
        None => sink,
    };
    let root = target_dir()?
        .join("tmp")
        .join(format!("kohbor-latency-{}", random_id()?));
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let wav_path = root.join("fixture.wav");
    fs::write(&wav_path, generated_wav()).map_err(|e| e.to_string())?;
    let wav = fs::read(&wav_path).map_err(|e| e.to_string())?;
    let pcm = decode_wav(&wav).ok_or("generated WAV failed to decode")?;
    let table = Arc::new(PrewarmTable::new(
        1,
        None,
        vec![(
            EventId::HealthDanger,
            None,
            Entry::Ready(Clip {
                pcm,
                bound_tier: None,
                audio_ms: 1,
                visual_tail_ms: 0,
                visual_url: None,
            }),
        )],
    ));
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/health-danger-golden.json"
    ))
    .map_err(|e| e.to_string())?;
    let trace = fixture["traces"]
        .as_array()
        .and_then(|v| v.iter().find(|t| t["id"] == "burst"))
        .and_then(|t| t["samples"].as_array())
        .ok_or("missing burst trace")?;
    let mut intake = Shared::new(root.clone());
    let token = intake::token_from(&root).ok_or("temporary intake token unavailable")?;
    let clock = intake.clock();
    let shared = Arc::new(EngineShared::new());
    let (out_tx, out_rx) = mpsc::sync_channel(OUT_LANE);
    let (sample_tx, sample_rx) = mpsc::channel();
    let publisher = publisher::spawn_publisher(
        out_rx,
        Arc::new(SilentEmitter),
        HistoryHandle::new(),
        None,
        Some(sample_tx),
    )
    .map_err(|e| e.to_string())?;
    let spawned = Engine::spawn(clock, shared.clone(), out_tx, true);
    let (tx, engine) = match spawned {
        Ok(v) => v,
        Err(e) => {
            let _ = join_bounded(publisher);
            return Err(e.to_string());
        }
    };
    intake.attach_engine(tx.clone());
    let run = (|| -> Result<Vec<TimedSample>, String> {
        tx.control(Control::SwapTable { table })
            .map_err(|_| "table control rejected")?;
        tx.control(Control::SinkReady(sink))
            .map_err(|_| "sink control rejected")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while shared.fence.load(Ordering::Relaxed) != 1
            || !shared.device_available.load(Ordering::Relaxed)
        {
            if Instant::now() >= deadline {
                return Err("engine setup timed out".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        let owner = kohbor_gsi::steamid64_from_account_id(12_345);
        let mut samples = Vec::with_capacity(count);
        for index in 0..count {
            let start = Instant::now();
            let mut body = json!({"provider":{"name":"Dota 2","appid":570}, "map":{"matchid":format!("{}", 8_000_000_001_u64 + index as u64),"game_state":"DOTA_GAMERULES_STATE_GAME_IN_PROGRESS","customgamename":""}, "player":{"steamid":owner.to_string(),"activity":"playing","team_name":"radiant"}, "hero":{"name":"npc_dota_hero_axe","level":12,"mana_percent":43,"alive":true,"respawn_seconds":0}, "auth":{"token":token}});
            let mut trigger_arrival = None;
            for (point_index, point) in trace.iter().enumerate() {
                let offset = point["t_ms"].as_u64().ok_or("trace time missing")?;
                let deadline = start + Duration::from_millis(offset);
                if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                    thread::sleep(wait);
                }
                body["hero"]["health"] = point["hp"].clone();
                body["hero"]["max_health"] = point["max_hp"].clone();
                body["hero"]["alive"] = point["alive"].clone();
                let arrival = Instant::now();
                if intake.accept(&body, arrival, Some(owner)) != 200 {
                    return Err("trace intake rejected".into());
                }
                // The burst trace triggers on its last point: T0 is the arrival of the
                // accept call whose response produced this publisher sample.
                if point_index == trace.len().saturating_sub(1) {
                    trigger_arrival = Some(arrival);
                }
            }
            let arrival = trigger_arrival.ok_or("burst trace had no trigger arrival")?;
            let sample = sample_rx
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| format!("sample {index}: {e}"))?;
            if sample.event_type.as_deref() != Some("health_danger") {
                return Err(format!("sample {index}: unexpected event"));
            }
            let trigger_t0_ms = clock.ms_at(arrival);
            if !stage_b_t0_matches(sample.t0_ms, arrival, clock) {
                return Err(format!(
                    "sample {index}: publisher T0 {} ms does not match trigger arrival {} ms within {} ms",
                    sample.t0_ms, trigger_t0_ms, STAGE_B_T0_TOLERANCE_MS
                ));
            }
            deltas(&sample)?;
            let t3pp_us = match (first_fill_reader, submitted_id.as_ref()) {
                (Some(reader), Some(last_submitted_id)) => {
                    let id = PlaybackId(last_submitted_id.load(Ordering::Acquire));
                    if id.0 == 0 {
                        return Err(format!(
                            "sample {index}: no playback id was submitted for the latency event"
                        ));
                    }
                    let first_fill = wait_for_first_fill(reader, id, FIRST_FILL_TIMEOUT)
                        .map_err(|error| format!("sample {index}, playback {}: {error}", id.0))?;
                    let elapsed = first_fill
                        .checked_duration_since(arrival)
                        .ok_or_else(|| format!("sample {index}: first fill precedes T0"))?;
                    Some(
                        u64::try_from(elapsed.as_micros())
                            .map_err(|_| format!("sample {index}: T0 to T3'' overflowed"))?,
                    )
                }
                (None, None) => None,
                _ => return Err("first-fill tracking was not initialized".into()),
            };
            samples.push(TimedSample { sample, t3pp_us });
        }
        Ok(samples)
    })();
    let shutdown = tx
        .control(Control::Shutdown)
        .map_err(|_| "shutdown control rejected".to_string());
    drop(intake);
    drop(tx);
    let engine_result = join_bounded(engine);
    let publisher_result = join_bounded(publisher);
    shutdown?;
    engine_result?;
    publisher_result?;
    run
}

pub fn artifact_sha256() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let data = fs::read(exe).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(data)))
}
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_owned())
}
fn utc_now() -> Result<String, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let days = i64::try_from(seconds / 86_400).map_err(|e| e.to_string())?;
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    let time = seconds % 86_400;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time % 3600 / 60,
        time % 60
    ))
}
pub fn build_receipt(report: &StageAReport, sample_path: &Path) -> Result<Value, String> {
    let values: Vec<u64> = report.samples.iter().map(|s| s.t3p_us).collect();
    let summary = summarize(&values)?;
    let mut limitations = vec![
        "engineering fixture; not QUALITY-001",
        "fake sink",
        "T3' is sink.submit accepted into the hand-off and not a rendered frame or sound",
        "no acoustic measurement",
        "generated test clip, not authorized critical media",
        "DEC-CRITICAL-MEDIA open",
        "T0 is the arrival Instant passed to Shared::accept: HTTP read, JSON parse, the registry read and the serve-side Shared lock are not in the measurement; F2 and F3 are not quantified",
        "no warm-up discarded and no outlier removed; samples include scheduler stalls of this machine",
        "one session epoch per burst trace (new match id), measured on a developer machine under no controlled load",
        "QUALITY-001 remains NOT_RUN",
    ];
    match git(&["status", "--porcelain"]) {
        Some(s) if !s.is_empty() => limitations.push("working tree dirty at run time"),
        None => limitations.push("working tree status unavailable"),
        _ => {}
    }
    Ok(json!({
        "schema_version":"1.0.0", "spec_version":"0.3.6", "receipt_id":format!("s3-6a-{}", random_id()?),
        "requirement_ids":["QUALITY-001"], "scenario_id":"S3-6a-stage-a-engineering-fixture", "evidence_class":"synthetic", "status":"INDETERMINATE",
        "source_revision":git(&["rev-parse", "HEAD"]), "artifact_sha256":artifact_sha256()?, "executed_at_utc":utc_now()?, "operator":"kohbor latency harness (stage A)",
        "environment":{"os_build":null,"hardware":null,"game_patch":null,"profile":null},
        "commands":["cargo test -p kohbor-desktop --test latency_harness --offline --locked -j 4 -- --ignored"],
        "raw_evidence_paths":[sample_path.to_string_lossy()],
        "observed_result":format!("T0 to T3' microseconds: count={}, p50={}, p95={}, max={}",summary.count,summary.p50,summary.p95,summary.max),
        "limitations":limitations
    }))
}

fn stage_b_limitations() -> Vec<String> {
    let mut limitations = vec![
        "engineering fixture; not QUALITY-001".to_owned(),
        "native silence only: default render endpoint, shared-mode event-driven stream, all-zero generated clip, no audible output".to_owned(),
        "T3'' is the first render service that copied the buffer, not loopback-audible sound".to_owned(),
        "generated test clip, not authorized critical media".to_owned(),
        "DEC-CRITICAL-MEDIA open".to_owned(),
        "T0 is the arrival Instant passed to Shared::accept: HTTP read, JSON parse, the registry read and the serve-side Shared lock are not in the measurement; F2 and F3 are not quantified".to_owned(),
        "no warm-up discarded and no outlier removed; samples include scheduler stalls of this machine".to_owned(),
        "one session epoch per burst trace (new match id), measured on a developer machine under no controlled load".to_owned(),
        "QUALITY-001 remains NOT_RUN".to_owned(),
    ];
    match git(&["status", "--porcelain"]) {
        Some(status) if !status.is_empty() => {
            limitations.push("working tree dirty at run time".to_owned());
        }
        None => limitations.push("working tree status unavailable".to_owned()),
        _ => {}
    }
    limitations
}

const STAGE_B_COMMAND: &str = "cargo test -p kohbor-desktop --test latency_harness_native --offline --locked -j 4 -- --ignored";

pub fn build_stage_b_receipt(report: &StageBReport, sample_path: &Path) -> Result<Value, String> {
    let values: Vec<u64> = report.samples.iter().map(|sample| sample.t3pp_us).collect();
    let summary = summarize(&values)?;
    Ok(json!({
        "schema_version":"1.0.0", "spec_version":"0.3.6", "receipt_id":format!("s3-6b-{}", random_id()?),
        "requirement_ids":["QUALITY-001"], "scenario_id":"S3-6b-stage-b-engineering-fixture", "evidence_class":"native", "status":"INDETERMINATE",
        "source_revision":git(&["rev-parse", "HEAD"]), "artifact_sha256":artifact_sha256()?, "executed_at_utc":utc_now()?, "operator":"kohbor latency harness (stage B)",
        "environment":{"os_build":null,"hardware":null,"game_patch":null,"profile":null},
        "commands":[STAGE_B_COMMAND],
        "raw_evidence_paths":[sample_path.to_string_lossy()],
        "observed_result":format!("T0 to T3'' microseconds: count={}, p50={}, p95={}, max={}",summary.count,summary.p50,summary.p95,summary.max),
        "limitations":stage_b_limitations()
    }))
}

pub fn not_run_receipt(reason: &str) -> Result<Value, String> {
    let mut limitations = stage_b_limitations();
    limitations.push(reason.to_owned());
    Ok(json!({
        "schema_version":"1.0.0", "spec_version":"0.3.6", "receipt_id":format!("s3-6b-{}", random_id()?),
        "requirement_ids":["QUALITY-001"], "scenario_id":"S3-6b-stage-b-engineering-fixture", "evidence_class":"native", "status":"NOT_RUN",
        "source_revision":git(&["rev-parse", "HEAD"]), "artifact_sha256":artifact_sha256()?, "executed_at_utc":utc_now()?, "operator":"kohbor latency harness (stage B)",
        "environment":{"os_build":null,"hardware":null,"game_patch":null,"profile":null},
        "commands":[STAGE_B_COMMAND],
        "raw_evidence_paths":[],
        "observed_result":null,
        "limitations":limitations
    }))
}

pub fn write_stage_b_evidence(report: &StageBReport) -> Result<(PathBuf, PathBuf), String> {
    let dir = target_dir()?.join("evidence").join("s3-6");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let samples = dir.join("latency-stage-b.samples.json");
    let receipt = dir.join("latency-stage-b.receipt.json");
    let raw: Vec<Value> = report
        .samples
        .iter()
        .map(|sample| {
            json!({
                "instance_id":sample.sample.instance_id,
                "event_type":sample.sample.event_type,
                "t0_ms":sample.sample.t0_ms,
                "t1_us":sample.sample.t1_us,
                "t2_us":sample.sample.t2_us,
                "t3p_us":sample.sample.t3p_us,
                "t3pp_us":sample.t3pp_us
            })
        })
        .collect();
    fs::write(
        &samples,
        serde_json::to_vec_pretty(&raw).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let value = build_stage_b_receipt(report, &samples)?;
    fs::write(
        &receipt,
        serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok((receipt, samples))
}

pub fn write_stage_b_not_run(reason: &str) -> Result<PathBuf, String> {
    let dir = target_dir()?.join("evidence").join("s3-6");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let receipt = dir.join("latency-stage-b.receipt.json");
    let value = not_run_receipt(reason)?;
    fs::write(
        &receipt,
        serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(receipt)
}

pub fn write_evidence(report: &StageAReport) -> Result<(PathBuf, PathBuf), String> {
    let dir = target_dir()?.join("evidence").join("s3-6");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let samples = dir.join("latency-stage-a.samples.json");
    let receipt = dir.join("latency-stage-a.receipt.json");
    let raw: Vec<Value> = report.samples.iter().map(|s| json!({"instance_id":s.instance_id,"event_type":s.event_type,"t0_ms":s.t0_ms,"t1_us":s.t1_us,"t2_us":s.t2_us,"t3p_us":s.t3p_us})).collect();
    fs::write(
        &samples,
        serde_json::to_vec_pretty(&raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let value = build_receipt(report, &samples)?;
    fs::write(
        &receipt,
        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((receipt, samples))
}
