use kohbor_events::{adapter::tick_from_admitted, arbitrate, Detector, Event, EventId, Phase};
use kohbor_gsi::{
    admission::{Admitted, Field, Outcome, Ownership},
    admit,
    session::{SessionChange, SessionTracker},
};
use serde_json::{json, Map, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader},
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
    let (path, json_output) = parse_args()?;
    let mut report = Report::new();
    let mut detector = Detector::new();
    let mut sessions = SessionTracker::new(SESSION_KEY);
    let mut diagnostic = StreakDiagnostic::default();

    if path.is_dir() {
        replay_directory(
            &path,
            &mut report,
            &mut detector,
            &mut sessions,
            &mut diagnostic,
        )?;
    } else {
        replay_jsonl(
            &path,
            &mut report,
            &mut detector,
            &mut sessions,
            &mut diagnostic,
        )?;
    }

    let value = report.to_json(&diagnostic);
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
        );
    } else {
        print_text(&report, &diagnostic);
    }
    Ok(())
}

fn parse_args() -> Result<(PathBuf, bool), String> {
    let mut args = env::args_os().skip(1);
    let Some(path) = args.next() else {
        return Err("usage: kohbor-replay <corpus.jsonl | dir-of-json> [--json]".into());
    };
    let mut json_output = false;
    for arg in args {
        if arg == "--json" && !json_output {
            json_output = true;
        } else {
            return Err("usage: kohbor-replay <corpus.jsonl | dir-of-json> [--json]".into());
        }
    }
    Ok((PathBuf::from(path), json_output))
}

fn replay_jsonl(
    path: &Path,
    report: &mut Report,
    detector: &mut Detector,
    sessions: &mut SessionTracker,
    diagnostic: &mut StreakDiagnostic,
) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    for (line_number, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
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
        process_payload(at_ms, body, report, detector, sessions, diagnostic);
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
        let at_ms = u64::try_from(index)
            .ok()
            .and_then(|index| index.checked_mul(1_000))
            .ok_or_else(|| "directory input has too many files to assign timestamps".to_owned())?;
        let text =
            fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let body: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        process_payload(at_ms, body, report, detector, sessions, diagnostic);
    }
    Ok(())
}

fn process_payload(
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
            skipped: SkippedCounts::default(),
            first_admitted_t_ms: None,
            last_admitted_t_ms: None,
            timeline: Vec::new(),
        }
    }

    fn to_json(&self, diagnostic: &StreakDiagnostic) -> Value {
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

        json!({
            "ownership": "synthetic in replay",
            "header": "Kohbor replay event timeline",
            "columns": ["t_ms", "phase", "kills", "events"],
            "timeline": self.timeline.iter().map(timeline_json).collect::<Vec<_>>(),
            "events": events,
            "ticks": {
                "admitted": self.admitted,
                "skipped": {
                    "NotPlaying": self.skipped.not_playing,
                    "Unsupported": self.skipped.unsupported,
                    "Rejected": self.skipped.rejected
                }
            },
            "first_admitted_t_ms": self.first_admitted_t_ms,
            "last_admitted_t_ms": self.last_admitted_t_ms,
            "arbitration_winners": winners,
            "diagnostic": diagnostic.to_json()
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
        "at_ms": entry.at_ms,
        "phase": entry.phase,
        "kills": entry.kills,
        "events": entry.events.iter().map(|event| event_json(*event)).collect::<Vec<_>>(),
        "arbitration_winner": entry.winner.map(event_json)
    })
}

fn event_json(event: Event) -> Value {
    json!({"id": event.id.as_str(), "tier": event.tier})
}

fn print_text(report: &Report, diagnostic: &StreakDiagnostic) {
    println!("Ownership is synthetic in replay");
    println!("Kohbor replay event timeline");
    println!("t_ms  phase  kills  events");
    for entry in &report.timeline {
        let events = entry
            .events
            .iter()
            .map(event_label)
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "{}  {}  {}  {}",
            entry.at_ms,
            entry.phase,
            entry
                .kills
                .map_or_else(|| "?".to_owned(), |kills| kills.to_string()),
            events
        );
    }

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
        "Ticks: admitted={} skipped: NotPlaying={} Unsupported={} Rejected={}",
        report.admitted,
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

fn event_label(event: &Event) -> String {
    match event.tier {
        Some(tier) => format!("{}:{tier}", event.id.as_str()),
        None => event.id.as_str().to_owned(),
    }
}

fn display_time(time: Option<u64>) -> String {
    time.map_or_else(|| "unknown".to_owned(), |time| time.to_string())
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
