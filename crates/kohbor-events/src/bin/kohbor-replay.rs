use base64::{engine::general_purpose::STANDARD, Engine as _};
use kohbor_events::{adapter::tick_from_admitted, arbitrate, Detector, Event, EventId, Phase};
use kohbor_gsi::{
    admission::{Admitted, Field, Outcome, Ownership},
    admit,
    session::{SessionChange, SessionTracker},
};
use serde_json::{json, Map, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, ErrorKind},
    path::{Path, PathBuf},
    process::ExitCode,
};

const SYNTHETIC_TOKEN: &str = "kohbor-replay-synthetic-token";
const SYNTHETIC_STEAMID: u64 = 76_561_197_960_265_729;
const SESSION_KEY: [u8; 32] = [0x52; 32];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("kohbor-replay: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let mut report = Report::new();
    let mut detector = Detector::new();
    let mut sessions = SessionTracker::new(SESSION_KEY);
    let mut diagnostic = StreakDiagnostic::default();

    if args.path.is_dir() && is_gsi_session(&args.path) {
        replay_gsi_session(
            &args.path,
            &mut report,
            &mut detector,
            &mut sessions,
            &mut diagnostic,
        )?;
    } else if args.path.is_dir() {
        replay_directory(
            &args.path,
            &mut report,
            &mut detector,
            &mut sessions,
            &mut diagnostic,
        )?;
    } else {
        replay_jsonl(
            &args.path,
            &mut report,
            &mut detector,
            &mut sessions,
            &mut diagnostic,
        )?;
    }

    let ground_truth = args
        .expect_kills
        .as_deref()
        .map(read_expected_kills)
        .transpose()?
        .map(|expected| compare_ground_truth(&report, expected));
    let value = report.to_json(&diagnostic, ground_truth.as_ref());
    if args.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
        );
    } else {
        print_text(&report, &diagnostic, ground_truth.as_ref());
    }
    Ok(())
}

struct ReplayArgs {
    path: PathBuf,
    json_output: bool,
    expect_kills: Option<PathBuf>,
}

fn parse_args() -> Result<ReplayArgs, String> {
    let mut args = env::args_os().skip(1);
    let Some(path) = args.next() else {
        return Err(usage());
    };
    let mut json_output = false;
    let mut expect_kills = None;
    while let Some(arg) = args.next() {
        if arg == "--json" && !json_output {
            json_output = true;
        } else if arg == "--expect-kills" && expect_kills.is_none() {
            let Some(path) = args.next() else {
                return Err(usage());
            };
            expect_kills = Some(PathBuf::from(path));
        } else {
            return Err(usage());
        }
    }
    Ok(ReplayArgs {
        path: PathBuf::from(path),
        json_output,
        expect_kills,
    })
}

fn usage() -> String {
    "usage: kohbor-replay <corpus.jsonl | dir-of-json | gsi-session-dir> [--expect-kills file.json] [--json]".into()
}

fn replay_jsonl(
    path: &Path,
    report: &mut Report,
    detector: &mut Detector,
    sessions: &mut SessionTracker,
    diagnostic: &mut StreakDiagnostic,
) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut sequence = 0_u64;
    for (line_number, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        sequence = sequence
            .checked_add(1)
            .ok_or_else(|| "JSONL input has too many records".to_owned())?;
        let record: Value = serde_json::from_str(&line)
            .map_err(|e| format!("{} line {}: {e}", path.display(), line_number + 1))?;
        let at_ms = record.get("t_ms").and_then(Value::as_u64).ok_or_else(|| {
            format!(
                "{} line {} has no unsigned t_ms",
                path.display(),
                line_number + 1
            )
        })?;
        let body = record
            .get("body")
            .cloned()
            .ok_or_else(|| format!("{} line {} has no body", path.display(), line_number + 1))?;
        process_payload(
            sequence, at_ms, body, report, detector, sessions, diagnostic,
        );
    }
    Ok(())
}

fn replay_directory(
    directory: &Path,
    report: &mut Report,
    detector: &mut Detector,
    sessions: &mut SessionTracker,
    diagnostic: &mut StreakDiagnostic,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|e| format!("cannot read directory {}: {e}", directory.display()))?;
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("cannot read directory {}: {e}", directory.display()))?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            && fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
    });
    paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for (index, path) in paths.iter().enumerate() {
        let zero_based_index = u64::try_from(index)
            .ok()
            .ok_or_else(|| "directory input has too many files to assign timestamps".to_owned())?;
        let sequence = zero_based_index
            .checked_add(1)
            .ok_or_else(|| "directory input has too many files to assign sequences".to_owned())?;
        let at_ms = zero_based_index
            .checked_mul(1_000)
            .ok_or_else(|| "directory input has too many files to assign timestamps".to_owned())?;
        let text =
            fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let body: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        process_payload(
            sequence, at_ms, body, report, detector, sessions, diagnostic,
        );
    }
    Ok(())
}

fn is_gsi_session(directory: &Path) -> bool {
    directory.join("manifest.json").is_file() && directory.join("raw").is_dir()
}

fn replay_gsi_session(
    directory: &Path,
    report: &mut Report,
    detector: &mut Detector,
    sessions: &mut SessionTracker,
    diagnostic: &mut StreakDiagnostic,
) -> Result<(), String> {
    let raw_directory = directory.join("raw");
    let entries = fs::read_dir(&raw_directory).map_err(|_| {
        format!(
            "cannot read raw session directory {}",
            raw_directory.display()
        )
    })?;
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            format!(
                "cannot read raw session directory {}",
                raw_directory.display()
            )
        })?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
            && fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
    });
    paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    let mut first_complete_ns = None;
    for path in paths {
        let file = fs::File::open(&path)
            .map_err(|_| format!("cannot read raw session file {}", path.display()))?;
        for line in BufReader::new(file).lines() {
            let line = match line {
                Ok(line) => line,
                Err(error) if error.kind() == ErrorKind::InvalidData => {
                    report.undecodable = report.undecodable.saturating_add(1);
                    continue;
                }
                Err(_) => return Err(format!("cannot read raw session file {}", path.display())),
            };
            let record: Value = match serde_json::from_str(&line) {
                Ok(record) => record,
                Err(_) => {
                    report.undecodable = report.undecodable.saturating_add(1);
                    continue;
                }
            };
            let sequence = record.get("sequence").and_then(Value::as_u64);
            let complete_ns = record
                .get("body_complete_monotonic_ns")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<u128>().ok());
            let encoded_body = record.get("raw_body_base64").and_then(Value::as_str);
            let (Some(sequence), Some(complete_ns), Some(encoded_body)) =
                (sequence, complete_ns, encoded_body)
            else {
                report.undecodable = report.undecodable.saturating_add(1);
                continue;
            };
            let first_ns = *first_complete_ns.get_or_insert(complete_ns);
            if encoded_body.len() > kohbor_gsi::config::BODY_LIMIT_BYTES.div_ceil(3) * 4 {
                report.undecodable = report.undecodable.saturating_add(1);
                continue;
            }
            let decoded = match STANDARD.decode(encoded_body) {
                Ok(decoded) if decoded.len() <= kohbor_gsi::config::BODY_LIMIT_BYTES => decoded,
                _ => {
                    report.undecodable = report.undecodable.saturating_add(1);
                    continue;
                }
            };
            let body = match kohbor_gsi::strict_json::parse(&decoded) {
                Ok(body) => body,
                Err(_) => {
                    report.undecodable = report.undecodable.saturating_add(1);
                    continue;
                }
            };
            let at_ms = complete_ns
                .saturating_sub(first_ns)
                .checked_div(1_000_000)
                .and_then(|elapsed_ms| u64::try_from(elapsed_ms).ok())
                .unwrap_or(u64::MAX);
            process_payload(
                sequence, at_ms, body, report, detector, sessions, diagnostic,
            );
        }
    }
    Ok(())
}

fn process_payload(
    sequence: u64,
    at_ms: u64,
    mut body: Value,
    report: &mut Report,
    detector: &mut Detector,
    sessions: &mut SessionTracker,
    diagnostic: &mut StreakDiagnostic,
) {
    let platform = inject_synthetic_identity(&mut body);
    match admit(&body, SYNTHETIC_TOKEN, platform, None) {
        Outcome::Admitted(admitted) => {
            report.admitted += 1;
            report.first_admitted_t_ms.get_or_insert(at_ms);
            report.last_admitted_t_ms = Some(at_ms);

            let session_epoch = if admitted.ownership == Ownership::Verified {
                match (admitted.match_id.as_deref(), admitted.steamid64) {
                    (Some(match_id), Some(owner)) => {
                        match sessions
                            .observe(Some(match_id), owner, || format!("replay-session-{at_ms}"))
                        {
                            SessionChange::Started { epoch }
                            | SessionChange::OwnerChanged { epoch }
                            | SessionChange::Resumed { epoch }
                            | SessionChange::Unchanged { epoch } => epoch,
                            SessionChange::NoEvidence => {
                                sessions.current().map_or(0, |current| current.epoch)
                            }
                        }
                    }
                    _ => sessions.current().map_or(0, |current| current.epoch),
                }
            } else {
                sessions.current().map_or(0, |current| current.epoch)
            };
            diagnostic.observe(at_ms, session_epoch, &admitted);

            let tick = tick_from_admitted(
                &admitted,
                at_ms,
                session_epoch,
                admitted.ownership == Ownership::Verified,
            );
            let events = detector.observe(&tick);
            for event in &events {
                report.event_counts[event_index(event.id)] += 1;
            }
            let arbitration = arbitrate(&events);
            match arbitration.winner {
                Some(winner) => report.winner_counts[event_index(winner.id)] += 1,
                None => report.no_winner_count += 1,
            }
            if !events.is_empty() {
                report.timeline.push(TimelineEntry {
                    sequence,
                    at_ms,
                    phase: phase_name(tick.phase).to_owned(),
                    kills: tick.kills,
                    events,
                    winner: arbitration.winner,
                });
            }
        }
        Outcome::NotPlaying => report.skipped.not_playing += 1,
        Outcome::Unsupported(_) => report.skipped.unsupported += 1,
        Outcome::Rejected(_) => report.skipped.rejected += 1,
    }
}

fn inject_synthetic_identity(body: &mut Value) -> Option<u64> {
    let root = body.as_object_mut()?;
    let auth = root
        .entry("auth".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !auth.is_object() {
        *auth = Value::Object(Map::new());
    }
    auth.as_object_mut()?.insert(
        "token".to_owned(),
        Value::String(SYNTHETIC_TOKEN.to_owned()),
    );

    let player = root.get_mut("player")?.as_object_mut()?;
    let existing = player
        .get("steamid")
        .and_then(Value::as_str)
        .filter(|steamid| steamid.len() == 17 && steamid.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|steamid| steamid.parse::<u64>().ok());
    let identity = existing.unwrap_or(SYNTHETIC_STEAMID);
    if existing.is_none() {
        player.insert(
            "steamid".to_owned(),
            Value::String(SYNTHETIC_STEAMID.to_string()),
        );
    }
    Some(identity)
}

#[derive(Default)]
struct SkippedCounts {
    not_playing: u64,
    unsupported: u64,
    rejected: u64,
}

struct TimelineEntry {
    sequence: u64,
    at_ms: u64,
    phase: String,
    kills: Option<u32>,
    events: Vec<Event>,
    winner: Option<Event>,
}

struct Report {
    event_counts: [u64; 12],
    winner_counts: [u64; 12],
    no_winner_count: u64,
    admitted: u64,
    undecodable: u64,
    skipped: SkippedCounts,
    first_admitted_t_ms: Option<u64>,
    last_admitted_t_ms: Option<u64>,
    timeline: Vec<TimelineEntry>,
}

impl Report {
    fn new() -> Self {
        Self {
            event_counts: [0; 12],
            winner_counts: [0; 12],
            no_winner_count: 0,
            admitted: 0,
            undecodable: 0,
            skipped: SkippedCounts::default(),
            first_admitted_t_ms: None,
            last_admitted_t_ms: None,
            timeline: Vec::new(),
        }
    }

    fn to_json(
        &self,
        diagnostic: &StreakDiagnostic,
        ground_truth: Option<&GroundTruthComparison>,
    ) -> Value {
        let mut events = Map::new();
        let mut winners = Map::new();
        for id in EVENT_IDS {
            events.insert(
                id.as_str().to_owned(),
                json!(self.event_counts[event_index(id)]),
            );
            winners.insert(
                id.as_str().to_owned(),
                json!(self.winner_counts[event_index(id)]),
            );
        }
        winners.insert("none".to_owned(), json!(self.no_winner_count));
        let tier_summary = self.tier_summary();

        json!({
            "ownership": "synthetic in replay",
            "ownership_note": "When a valid 17-digit SteamID is present, replay uses it as the platform identity, so ownership is verified by construction.",
            "header": "Kohbor replay event timeline",
            "columns": ["t_ms", "seq", "phase", "kills", "events"],
            "timeline": self.timeline.iter().map(timeline_json).collect::<Vec<_>>(),
            "tier_events": self.tier_events().iter().map(tier_event_json).collect::<Vec<_>>(),
            "tier_summary": tier_summary.to_json(),
            "events": events,
            "ticks": {
                "admitted": self.admitted,
                "undecodable": self.undecodable,
                "skipped": {
                    "NotPlaying": self.skipped.not_playing,
                    "Unsupported": self.skipped.unsupported,
                    "Rejected": self.skipped.rejected
                }
            },
            "first_admitted_t_ms": self.first_admitted_t_ms,
            "last_admitted_t_ms": self.last_admitted_t_ms,
            "arbitration_winners": winners,
            "diagnostic": diagnostic.to_json(),
            "ground_truth": ground_truth.map(GroundTruthComparison::to_json)
        })
    }

    fn tier_events(&self) -> Vec<TierEvent> {
        self.timeline
            .iter()
            .flat_map(|entry| {
                entry
                    .events
                    .iter()
                    .copied()
                    .filter(|event| matches!(event.id, EventId::MultiKill | EventId::Streak))
                    .map(|event| TierEvent {
                        event,
                        sequence: entry.sequence,
                        at_ms: entry.at_ms,
                    })
            })
            .collect()
    }

    fn tier_summary(&self) -> TierSummary {
        let mut summary = TierSummary::default();
        for tier_event in self.tier_events() {
            if let Some(tier) = tier_event.event.tier {
                match tier_event.event.id {
                    EventId::MultiKill if (2..=5).contains(&tier) => {
                        summary.multi_kill[usize::from(tier - 2)] += 1;
                    }
                    EventId::Streak if (3..=10).contains(&tier) => {
                        summary.streak[usize::from(tier - 3)] += 1;
                    }
                    _ => {}
                }
            }
        }
        summary
    }
}

struct TierEvent {
    event: Event,
    sequence: u64,
    at_ms: u64,
}

#[derive(Default)]
struct TierSummary {
    multi_kill: [u64; 4],
    streak: [u64; 8],
}

impl TierSummary {
    fn to_json(&self) -> Value {
        let multi_kill = (2_u8..=5)
            .map(|tier| {
                (
                    tier.to_string(),
                    json!(self.multi_kill[usize::from(tier - 2)]),
                )
            })
            .collect::<Map<_, _>>();
        let streak = (3_u8..=10)
            .map(|tier| (tier.to_string(), json!(self.streak[usize::from(tier - 3)])))
            .collect::<Map<_, _>>();
        json!({"multi_kill": multi_kill, "streak": streak})
    }

    fn text(&self) -> String {
        (2_u8..=5)
            .map(|tier| {
                format!(
                    "multi_kill_tier_{tier}={}",
                    self.multi_kill[usize::from(tier - 2)]
                )
            })
            .chain(
                (3_u8..=10).map(|tier| {
                    format!("streak_tier_{tier}={}", self.streak[usize::from(tier - 3)])
                }),
            )
            .collect::<Vec<_>>()
            .join(" ")
    }
}

struct ExpectedKill {
    kill_number: u64,
    sequence: u64,
    observed_clock_seconds: f64,
}

struct GroundTruthMatch {
    kill_number: u64,
    expected_sequence: u64,
    observed_clock_seconds: f64,
    detector_sequence: Option<u64>,
    delta: Option<i64>,
}

struct GroundTruthComparison {
    matches: Vec<GroundTruthMatch>,
    expected_count: u64,
    matched_count: u64,
    unmatched_detector_hero_kills: u64,
}

impl GroundTruthComparison {
    fn to_json(&self) -> Value {
        json!({
            "expected": self.expected_count,
            "matched": self.matched_count,
            "unmatched": self.expected_count.saturating_sub(self.matched_count),
            "unmatched_detector_hero_kills": self.unmatched_detector_hero_kills,
            "matches": self.matches.iter().map(|matched| json!({
                "kill_number": matched.kill_number,
                "event": "hero_kill",
                "expected_sequence": matched.expected_sequence,
                "observed_clock_seconds": matched.observed_clock_seconds,
                "detector_sequence": matched.detector_sequence,
                "delta": matched.delta
            })).collect::<Vec<_>>()
        })
    }
}

const EVENT_IDS: [EventId; 12] = [
    EventId::MultiKill,
    EventId::Streak,
    EventId::FirstBlood,
    EventId::HeroKill,
    EventId::HeroDeath,
    EventId::LevelMilestone,
    EventId::HealthLow,
    EventId::ManaLow,
    EventId::MatchEnded,
    EventId::MatchStarted,
    EventId::HeroRespawn,
    EventId::HealthDanger,
];

fn event_index(id: EventId) -> usize {
    EVENT_IDS
        .iter()
        .position(|candidate| *candidate == id)
        .expect("every event id is in the replay catalog")
}

fn timeline_json(entry: &TimelineEntry) -> Value {
    json!({
        "t_ms": entry.at_ms,
        "at_ms": entry.at_ms,
        "seq": entry.sequence,
        "phase": entry.phase,
        "kills": entry.kills,
        "events": entry.events.iter().map(|event| event_json(*event)).collect::<Vec<_>>(),
        "arbitration_winner": entry.winner.map(event_json)
    })
}

fn event_json(event: Event) -> Value {
    json!({"id": event.id.as_str(), "tier": event.tier})
}

fn tier_event_json(tier_event: &TierEvent) -> Value {
    json!({
        "id": tier_event.event.id.as_str(),
        "tier": tier_event.event.tier,
        "seq": tier_event.sequence,
        "t_ms": tier_event.at_ms
    })
}

fn print_text(
    report: &Report,
    diagnostic: &StreakDiagnostic,
    ground_truth: Option<&GroundTruthComparison>,
) {
    println!(
        "Ownership is synthetic in replay; a valid 17-digit SteamID is verified by construction"
    );
    println!("Kohbor replay event timeline");
    println!("t_ms  seq  phase  kills  events");
    for entry in &report.timeline {
        let events = entry
            .events
            .iter()
            .map(event_label)
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "{}  {}  {}  {}  {}",
            entry.at_ms,
            entry.sequence,
            entry.phase,
            entry
                .kills
                .map_or_else(|| "?".to_owned(), |kills| kills.to_string()),
            events
        );
    }

    if let Some(ground_truth) = ground_truth {
        println!("Ground truth comparison");
        for matched in &ground_truth.matches {
            println!(
                "kill_number={} expected_seq={} observed_clock_seconds={} hero_kill_seq={} delta={}",
                matched.kill_number,
                matched.expected_sequence,
                matched.observed_clock_seconds,
                display_optional_number(matched.detector_sequence),
                display_optional_number(matched.delta)
            );
        }
        println!(
            "Counts: expected={} matched={} unmatched={} unmatched_detector_hero_kills={}",
            ground_truth.expected_count,
            ground_truth.matched_count,
            ground_truth
                .expected_count
                .saturating_sub(ground_truth.matched_count),
            ground_truth.unmatched_detector_hero_kills
        );
    }

    println!("Detector tier events");
    for tier_event in report.tier_events() {
        let tier = tier_event
            .event
            .tier
            .expect("multi_kill and streak events have tiers");
        println!(
            "{} tier={} seq={} t_ms={}",
            tier_event.event.id.as_str(),
            tier,
            tier_event.sequence,
            tier_event.at_ms
        );
    }
    println!("Per-tier summary: {}", report.tier_summary().text());

    let event_counts = EVENT_IDS
        .iter()
        .map(|id| format!("{}={}", id.as_str(), report.event_counts[event_index(*id)]))
        .collect::<Vec<_>>()
        .join(" ");
    let winner_counts = EVENT_IDS
        .iter()
        .map(|id| format!("{}={}", id.as_str(), report.winner_counts[event_index(*id)]))
        .chain(std::iter::once(format!("none={}", report.no_winner_count)))
        .collect::<Vec<_>>()
        .join(" ");
    println!("Events: {event_counts}");
    println!(
        "Ticks: admitted={} undecodable={} skipped: NotPlaying={} Unsupported={} Rejected={}",
        report.admitted,
        report.undecodable,
        report.skipped.not_playing,
        report.skipped.unsupported,
        report.skipped.rejected
    );
    println!(
        "First and last admitted t_ms: {} {}",
        display_time(report.first_admitted_t_ms),
        display_time(report.last_admitted_t_ms)
    );
    println!("Per-tick arbitration winners: {winner_counts}");
    println!(
        "E-3 diagnostic (not detector logic): compared={} mismatches={}",
        diagnostic.compared, diagnostic.mismatches
    );
    for mismatch in &diagnostic.first_mismatches {
        println!(
            "  t_ms={} computed={} player.kill_streak={}",
            mismatch.at_ms, mismatch.computed, mismatch.observed
        );
    }
}

fn display_optional_number<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}

fn event_label(event: &Event) -> String {
    match event.tier {
        Some(tier) => format!("{}:{tier}", event.id.as_str()),
        None => event.id.as_str().to_owned(),
    }
}

fn display_time(time: Option<u64>) -> String {
    time.map_or_else(|| "unknown".to_owned(), |time| time.to_string())
}

fn read_expected_kills(path: &Path) -> Result<Vec<ExpectedKill>, String> {
    let text = fs::read_to_string(path)
        .map_err(|_| format!("cannot read expected kills file {}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| "expected kills file is not valid JSON".to_owned())?;
    let events = value
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| "expected kills file has no events array".to_owned())?;
    events
        .iter()
        .map(|event| {
            let kill_number = event
                .get("kill_number")
                .and_then(Value::as_u64)
                .ok_or_else(|| "expected kill has no unsigned kill_number".to_owned())?;
            let sequence = event
                .get("sequence")
                .and_then(Value::as_u64)
                .ok_or_else(|| "expected kill has no unsigned sequence".to_owned())?;
            let observed_clock_seconds = event
                .get("observed_clock_seconds")
                .and_then(Value::as_f64)
                .filter(|seconds| seconds.is_finite())
                .ok_or_else(|| "expected kill has no numeric observed_clock_seconds".to_owned())?;
            Ok(ExpectedKill {
                kill_number,
                sequence,
                observed_clock_seconds,
            })
        })
        .collect()
}

fn compare_ground_truth(report: &Report, expected: Vec<ExpectedKill>) -> GroundTruthComparison {
    let detector_kills = report
        .timeline
        .iter()
        .flat_map(|entry| {
            entry
                .events
                .iter()
                .filter(|event| event.id == EventId::HeroKill)
                .map(|_| (entry.sequence, entry.at_ms))
        })
        .collect::<Vec<_>>();
    let mut used_detector_kills = vec![false; detector_kills.len()];
    let mut matches = Vec::with_capacity(expected.len());
    let mut previous_expected_sequence = None;

    for expected_kill in expected {
        let detected = detector_kills
            .iter()
            .enumerate()
            .find(|(index, (sequence, _))| {
                !used_detector_kills[*index]
                    && previous_expected_sequence.is_none_or(|previous| *sequence >= previous)
                    && sequence.abs_diff(expected_kill.sequence) <= 5
            });
        let (detector_sequence, delta) = if let Some((index, (sequence, _))) = detected {
            used_detector_kills[index] = true;
            (
                Some(*sequence),
                Some(sequence_delta(*sequence, expected_kill.sequence)),
            )
        } else {
            (None, None)
        };
        matches.push(GroundTruthMatch {
            kill_number: expected_kill.kill_number,
            expected_sequence: expected_kill.sequence,
            observed_clock_seconds: expected_kill.observed_clock_seconds,
            detector_sequence,
            delta,
        });
        previous_expected_sequence = Some(expected_kill.sequence);
    }

    let expected_count = u64::try_from(matches.len()).unwrap_or(u64::MAX);
    let matched_count = u64::try_from(
        used_detector_kills
            .iter()
            .filter(|matched| **matched)
            .count(),
    )
    .unwrap_or(u64::MAX);
    let unmatched_detector_hero_kills = u64::try_from(
        used_detector_kills
            .iter()
            .filter(|matched| !**matched)
            .count(),
    )
    .unwrap_or(u64::MAX);

    GroundTruthComparison {
        matches,
        expected_count,
        matched_count,
        unmatched_detector_hero_kills,
    }
}

fn sequence_delta(detected: u64, expected: u64) -> i64 {
    if detected >= expected {
        i64::try_from(detected - expected).unwrap_or(i64::MAX)
    } else {
        let difference = expected - detected;
        i64::try_from(difference)
            .map(|difference| -difference)
            .unwrap_or(i64::MIN)
    }
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Draft => "Draft",
        Phase::Pregame => "Pregame",
        Phase::Live => "Live",
        Phase::PostGame => "PostGame",
    }
}

#[derive(Default)]
struct StreakDiagnostic {
    previous: Option<DiagnosticSample>,
    computed: Option<u32>,
    compared: u64,
    mismatches: u64,
    first_mismatches: Vec<DiagnosticMismatch>,
}

#[derive(Clone, Copy)]
struct DiagnosticSample {
    session_epoch: u64,
    kills: Option<u32>,
    alive: Option<bool>,
}

struct DiagnosticMismatch {
    at_ms: u64,
    computed: u32,
    observed: u32,
}

impl StreakDiagnostic {
    fn observe(&mut self, at_ms: u64, session_epoch: u64, admitted: &Admitted) {
        let current = DiagnosticSample {
            session_epoch,
            kills: field_u32(&admitted.counters.kills),
            alive: field_bool(&admitted.hero.alive),
        };
        let new_session = self
            .previous
            .is_none_or(|previous| previous.session_epoch != session_epoch);

        if new_session {
            self.computed = known_baseline(current);
        } else if let Some(previous) = self.previous {
            self.computed = next_computed(previous, current, self.computed);
        }
        self.previous = Some(current);

        if let (Some(computed), Field::Value(observed)) =
            (self.computed, &admitted.counters.kill_streak)
        {
            if let Ok(observed) = u32::try_from(*observed) {
                self.compared += 1;
                if computed != observed {
                    self.mismatches += 1;
                    if self.first_mismatches.len() < 5 {
                        self.first_mismatches.push(DiagnosticMismatch {
                            at_ms,
                            computed,
                            observed,
                        });
                    }
                }
            }
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "label": "E-3 diagnostic (not detector logic)",
            "compared": self.compared,
            "mismatches": self.mismatches,
            "first_mismatches": self.first_mismatches.iter().map(|mismatch| json!({
                "at_ms": mismatch.at_ms,
                "computed": mismatch.computed,
                "player_kill_streak": mismatch.observed
            })).collect::<Vec<_>>()
        })
    }
}

fn known_baseline(sample: DiagnosticSample) -> Option<u32> {
    if sample.alive == Some(false) || sample.kills == Some(0) {
        Some(0)
    } else {
        None
    }
}

fn next_computed(
    previous: DiagnosticSample,
    current: DiagnosticSample,
    computed: Option<u32>,
) -> Option<u32> {
    if current.alive == Some(false) {
        return Some(0);
    }
    if current.alive.is_none() || previous.alive.is_none() {
        return None;
    }

    let kill_delta = match (previous.kills, current.kills) {
        (Some(previous), Some(current)) if current >= previous => Some(current - previous),
        (Some(_), Some(_)) => return known_baseline(current),
        _ => return None,
    };

    computed?.checked_add(kill_delta?)
}

fn field_u32(field: &kohbor_gsi::admission::Field<i64>) -> Option<u32> {
    match field {
        kohbor_gsi::admission::Field::Value(value) => u32::try_from(*value).ok(),
        kohbor_gsi::admission::Field::Missing | kohbor_gsi::admission::Field::Invalid => None,
    }
}

fn field_bool(field: &kohbor_gsi::admission::Field<bool>) -> Option<bool> {
    match field {
        kohbor_gsi::admission::Field::Value(value) => Some(*value),
        kohbor_gsi::admission::Field::Missing | kohbor_gsi::admission::Field::Invalid => None,
    }
}
