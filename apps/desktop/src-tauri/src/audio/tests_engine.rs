use super::fake_sink::{table, tick, Call, FakeSink, Harness};
use super::lanes::{Control, EngineOut, Offer};
use super::resolve::{Entry, PrewarmTable};
use super::slot_sink::SlotSink;
use kohbor_audio::{PlaybackId, Sink, SinkEvent};
use kohbor_events::EventId;
use serde_json::Value;
use std::sync::{atomic::Ordering, mpsc, Arc};
use std::time::Duration;

const SAFE: u64 = 9_007_199_254_740_991;
fn valid_id(s: &str, max: usize) -> bool {
    (8..=max).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn valid_event(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .skip(1)
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn check_record(v: &Value) {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../../contracts/audio-outcome.schema.json"
    ))
    .unwrap();
    assert_eq!(
        schema["additionalProperties"], false,
        "schema remains closed"
    );
    let obj = v.as_object().expect("outcome object");
    let required = schema["required"].as_array().unwrap();
    assert_eq!(obj.len(), required.len(), "outcome closed schema");
    for key in required {
        assert!(
            obj.contains_key(key.as_str().unwrap()),
            "outcome required {key}"
        );
    }
    assert_eq!(
        v["schema_version"], schema["properties"]["schema_version"]["const"],
        "schema version"
    );
    assert!(
        v["instance_id"].as_str().is_some_and(|s| valid_id(s, 96)),
        "outcome instance id"
    );
    for field in ["audio_class", "outcome"] {
        assert!(
            schema["properties"][field]["enum"]
                .as_array()
                .unwrap()
                .contains(&v[field]),
            "outcome enum {field}"
        );
    }
    assert!(
        v["session_epoch"]
            .as_u64()
            .is_some_and(|n| (1..=SAFE).contains(&n)),
        "outcome session"
    );
    assert!(
        v["decided_monotonic_ms"]
            .as_u64()
            .is_some_and(|n| n <= SAFE),
        "outcome time"
    );
    assert!(
        v["package_revision"].is_null()
            || v["package_revision"]
                .as_u64()
                .is_some_and(|n| (1..=SAFE).contains(&n)),
        "outcome revision"
    );
    if matches!(v["outcome"].as_str(), Some("admitted" | "preempted")) {
        assert!(
            v["playback_id"].as_str().is_some_and(|s| valid_id(s, 64)),
            "outcome playback id"
        );
    } else {
        assert!(v["playback_id"].is_null(), "non-admitted playback id null");
    }
    if v["audio_class"] == "voice" {
        assert!(
            v["event_type"].is_null() && v["package_revision"].is_null(),
            "voice fields null"
        );
    } else {
        assert!(
            v["event_type"].as_str().is_some_and(valid_event),
            "outcome event type"
        );
        if v["outcome"] == "admitted" {
            assert!(v["package_revision"].is_u64(), "admitted revision");
        }
    }
    assert_ne!(
        v["outcome"], "superseded_revision",
        "host request always uses held fence"
    );
}
fn check_playback(v: &Value) {
    let obj = v.as_object().expect("playback object");
    assert!(
        v["playback_id"].as_str().is_some_and(|s| valid_id(s, 64)),
        "I1 id"
    );
    assert!(v["at_ms"].as_u64().is_some(), "I1 time");
    let fields: &[&str] = if v["kind"] == "ended" {
        assert!(
            [
                "finished",
                "cancelled_preempted",
                "cancelled_session",
                "cancelled_revision",
                "cancelled_muted",
                "cancelled_device_lost"
            ]
            .contains(&v["reason"].as_str().unwrap_or("")),
            "I1 reason"
        );
        &["kind", "playback_id", "reason", "at_ms"]
    } else {
        assert_eq!(v["kind"], "started", "I1 kind");
        assert!(
            ["critical", "correction", "notice", "ordinary", "voice"]
                .contains(&v["class"].as_str().unwrap_or("")),
            "I1 class"
        );
        assert!(
            v["event_type"].is_null() || v["event_type"].as_str().is_some_and(valid_event),
            "I1 event"
        );
        assert!(
            v["tier"].is_null() || v["tier"].as_u64().is_some(),
            "I1 tier"
        );
        assert!(
            v["visual_url"].is_null()
                || v["visual_url"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("kohbor-media://")),
            "I1 URL"
        );
        assert!(
            v["audio_duration_ms"].as_u64().is_some() && v["visual_tail_ms"].as_u64().is_some(),
            "I1 durations"
        );
        &[
            "kind",
            "playback_id",
            "class",
            "event_type",
            "tier",
            "visual_url",
            "audio_duration_ms",
            "visual_tail_ms",
            "at_ms",
        ]
    };
    assert_eq!(obj.len(), fields.len(), "I1 closed shape");
    for f in fields {
        assert!(obj.contains_key(*f), "I1 field {f}");
    }
}
pub(super) fn check_outputs(out: &[EngineOut]) {
    for item in out {
        match item {
            EngineOut::Outcome(v) => check_record(v),
            EngineOut::Playback(v) => check_playback(v),
            _ => {}
        }
    }
}

fn setup() -> (Harness, super::fake_sink::Log) {
    let (sink, log) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), false, 256);
    assert!(h.tx.control(Control::SwapTable { table: table(7, &[(EventId::HealthDanger,None,4801,0,Some("kohbor-media://localhost/v1/ins_0000000007/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")), (EventId::HeroKill,None,4800,0,None)]) }).is_ok());
    h.engine.step(0);
    h.drain();
    (h, log)
}
fn offer(h: &Harness, seq: u64, events: &[(EventId, Option<u8>)]) {
    assert_eq!(
        h.tx.offer_tick(tick(h.origin, 1, 3, seq, events)),
        Offer::Sent
    );
}
fn outcomes(out: &[EngineOut]) -> Vec<&Value> {
    out.iter()
        .filter_map(|x| {
            if let EngineOut::Outcome(v) = x {
                Some(v)
            } else {
                None
            }
        })
        .collect()
}
fn playbacks(out: &[EngineOut]) -> Vec<&Value> {
    out.iter()
        .filter_map(|x| {
            if let EngineOut::Playback(v) = x {
                Some(v)
            } else {
                None
            }
        })
        .collect()
}
fn has_outcome(out: &[EngineOut], value: &str) -> bool {
    outcomes(out).iter().any(|v| v["outcome"] == value)
}
fn has_end(out: &[EngineOut], reason: &str) -> bool {
    playbacks(out)
        .iter()
        .any(|v| v["kind"] == "ended" && v["reason"] == reason)
}

#[test]
fn critical_whole_path() {
    let (mut h, log) = setup();
    offer(&h, 120, &[(EventId::HealthDanger, None)]);
    h.engine.step(10);
    let out = h.drain();
    assert_eq!(
        log.lock().unwrap().calls,
        vec![Call::Submit(PlaybackId(1), 4801)],
        "critical clip samples submitted once"
    );
    let p = playbacks(&out);
    assert_eq!(p.len(), 1, "one I1 start");
    assert_eq!(p[0]["kind"], "started");
    assert_eq!(p[0]["class"], "critical");
    assert_eq!(p[0]["event_type"], "health_danger");
    assert_eq!(p[0]["audio_duration_ms"], 101);
    assert!(p[0]["visual_url"]
        .as_str()
        .unwrap()
        .contains("ins_0000000007"));
    assert!(has_outcome(&out, "admitted"), "admitted outcome");
    h.engine.step(112);
    let out = h.drain();
    assert!(has_end(&out, "finished"), "natural end");
    assert!(outcomes(&out).is_empty(), "natural end creates no outcome");
}
#[test]
fn ordinary_hero_kill_plays() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    assert_eq!(
        log.lock().unwrap().calls,
        vec![Call::Submit(PlaybackId(1), 4800)],
        "ordinary clip"
    );
    assert_eq!(playbacks(&h.drain())[0]["class"], "ordinary");
}
#[test]
fn initial_missing_sink_requests_once() {
    let mut h = Harness::new(None, false, 256);
    h.engine.step(0);
    assert_eq!(
        h.drain()
            .iter()
            .filter(|x| matches!(x, EngineOut::NeedSink))
            .count(),
        1,
        "first step requests sink"
    );
    h.engine.step(1);
    assert!(
        !h.drain().iter().any(|x| matches!(x, EngineOut::NeedSink)),
        "no repeated request"
    );
}
#[test]
fn unavailable_removed_before_arbitration() {
    let (mut h, log) = setup();
    offer(
        &h,
        1,
        &[(EventId::FirstBlood, None), (EventId::HeroKill, None)],
    );
    h.engine.step(10);
    let o = h.drain();
    assert!(
        has_outcome(&o, "detector_unavailable"),
        "unavailable first blood"
    );
    assert!(
        has_outcome(&o, "admitted"),
        "lower ranked available event plays"
    );
    assert_eq!(log.lock().unwrap().calls.len(), 1);
}
#[test]
fn unvoiced_respawn_is_recorded() {
    let (mut h, _) = setup();
    offer(&h, 1, &[(EventId::HeroRespawn, None)]);
    h.engine.step(10);
    assert!(
        has_outcome(&h.drain(), "not_voiced"),
        "respawn is not voiced"
    );
}
#[test]
fn critical_blocks_same_tick_ordinary() {
    let (mut h, log) = setup();
    offer(
        &h,
        1,
        &[(EventId::HeroKill, None), (EventId::HealthDanger, None)],
    );
    h.engine.step(10);
    let o = h.drain();
    assert!(
        has_outcome(&o, "critical_active"),
        "ordinary sees critical active"
    );
    assert_eq!(
        log.lock().unwrap().calls.len(),
        1,
        "only critical submitted"
    );
}
#[test]
fn unmapped_critical_does_not_block_ordinary() {
    let (sink, log) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), false, 256);
    assert!(h
        .tx
        .control(Control::SwapTable {
            table: table(7, &[(EventId::HeroKill, None, 4800, 0, None)])
        })
        .is_ok());
    h.engine.step(0);
    h.drain();
    offer(
        &h,
        1,
        &[(EventId::HealthDanger, None), (EventId::HeroKill, None)],
    );
    h.engine.step(10);
    let o = h.drain();
    assert!(has_outcome(&o, "unmapped"), "critical unmapped");
    assert!(has_outcome(&o, "admitted"), "ordinary admitted under P4");
    assert_eq!(log.lock().unwrap().calls.len(), 1);
}
#[test]
fn invalid_mapping_records_invalid_media() {
    let (sink, log) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), false, 256);
    let t = Arc::new(PrewarmTable::new(
        7,
        Some(Arc::from("ins_0000000007")),
        vec![(EventId::HeroKill, None, Entry::Invalid)],
    ));
    assert!(h.tx.control(Control::SwapTable { table: t }).is_ok());
    h.engine.step(0);
    h.drain();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    assert!(
        has_outcome(&h.drain(), "invalid_media"),
        "invalid clip recorded"
    );
    assert!(log.lock().unwrap().calls.is_empty());
}
#[test]
fn instance_ids_use_original_position() {
    let (mut h, _) = setup();
    offer(
        &h,
        120,
        &[(EventId::FirstBlood, None), (EventId::HeroKill, None)],
    );
    h.engine.step(10);
    let o = h.drain();
    let records = outcomes(&o);
    assert!(records
        .iter()
        .any(|v| v["instance_id"] == "ev_g3_s120_0" && v["outcome"] == "detector_unavailable"));
    assert!(records
        .iter()
        .any(|v| v["instance_id"] == "ev_g3_s120_1" && v["outcome"] == "admitted"));
}
#[test]
fn mute_ack_cancels_and_blocks() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    let (tx, rx) = mpsc::sync_channel(1);
    assert!(h
        .tx
        .control(Control::SetMuted {
            muted: true,
            ack: tx
        })
        .is_ok());
    h.engine.step(20);
    assert_eq!(rx.try_recv(), Ok(true), "mute ack within step");
    assert!(h.shared.muted.load(Ordering::Relaxed));
    assert!(has_end(&h.drain(), "cancelled_muted"));
    offer(&h, 2, &[(EventId::HeroKill, None)]);
    h.engine.step(30);
    assert!(has_outcome(&h.drain(), "muted"));
    assert_eq!(
        log.lock()
            .unwrap()
            .calls
            .iter()
            .filter(|x| matches!(x, Call::Submit(..)))
            .count(),
        1
    );
}
#[test]
fn session_control_cancels_active() {
    let (mut h, _) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    assert!(h.tx.control(Control::Session { epoch: 2 }).is_ok());
    h.engine.step(20);
    assert!(
        has_end(&h.drain(), "cancelled_session"),
        "session fence ends playback"
    );
}
#[test]
fn swap_cancels_and_releases_table() {
    let (mut h, _) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    assert!(h
        .tx
        .control(Control::SwapTable {
            table: table(8, &[(EventId::HeroKill, None, 9600, 0, None)])
        })
        .is_ok());
    h.engine.step(20);
    let o = h.drain();
    assert!(has_end(&o, "cancelled_revision"));
    assert!(o
        .iter()
        .any(|x| matches!(x,EngineOut::ReleaseTable(t) if t.fence==7)));
    assert_eq!(h.shared.fence.load(Ordering::Relaxed), 8);
}
#[test]
fn heartbeat_and_idle_wait() {
    let (mut h, _) = setup();
    assert_eq!(h.engine.step(123), Duration::from_millis(250), "idle wait");
    assert_eq!(
        h.shared.heartbeat_ms.load(Ordering::Relaxed),
        123,
        "heartbeat"
    );
}
#[test]
fn active_wait_and_lease() {
    let (mut h, _) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    assert_eq!(h.engine.step(10), Duration::from_millis(10), "active wait");
    assert!(
        h.shared.lease_active.load(Ordering::Relaxed),
        "active lease"
    );
}
#[test]
fn device_loss_reports_and_fences() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    log.lock().unwrap().events.push_back(SinkEvent::DeviceLost);
    h.engine.step(20);
    let o = h.drain();
    assert!(has_end(&o, "cancelled_device_lost"));
    assert!(o.iter().any(|x| matches!(x, EngineOut::DeviceLost)));
    let lost = o
        .iter()
        .position(|x| matches!(x, EngineOut::DeviceLost))
        .expect("device lost emitted");
    let need = o
        .iter()
        .position(|x| matches!(x, EngineOut::NeedSink))
        .expect("replacement requested");
    assert!(lost < need, "device loss precedes replacement request");
    assert!(!h.shared.device_available.load(Ordering::Relaxed));
}
#[test]
fn panic_is_contained_and_sink_released() {
    let (mut h, log) = setup();
    log.lock().unwrap().panic_next = true;
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    let o = h.drain();
    assert_eq!(
        h.shared.rebuilds.load(Ordering::Relaxed),
        1,
        "panic triggers rebuild"
    );
    assert!(o.iter().any(|x| matches!(x, EngineOut::ReleaseSink(_))));
    assert!(o.iter().any(|x| matches!(x, EngineOut::NeedSink)));
}
#[test]
fn slot_empty_contract() {
    let mut s = SlotSink::empty();
    assert!(s.is_empty());
    assert!(s.submit(PlaybackId(1), Arc::from([0_i16])).is_err());
    s.cancel(PlaybackId(1));
    assert_eq!(s.poll_event(), None);
}
#[test]
fn slot_install_replace_take() {
    let mut s = SlotSink::empty();
    let (a, _) = FakeSink::new();
    let (b, _) = FakeSink::new();
    assert!(s.install(Box::new(a)).is_none());
    assert!(!s.is_empty());
    assert!(s.install(Box::new(b)).is_some());
    assert!(s.take().is_some());
    assert!(s.is_empty());
    assert!(s.take().is_none());
}
#[test]
fn slot_change_event_is_forwarded_and_consumed_once() {
    let mut s = SlotSink::empty();
    let (sink, log) = FakeSink::new();
    s.install(Box::new(sink));
    log.lock()
        .unwrap()
        .events
        .push_back(SinkEvent::DeviceChanged);
    assert_eq!(s.poll_event(), Some(SinkEvent::DeviceChanged));
    assert!(s.take_changed());
    assert!(!s.take_changed());
}
// Lexical scan only: keep string contents, strip nested Rust comments and the test suffix.
fn production_region(source: &str) -> String {
    let source = source.split("#[cfg(test)]").next().unwrap_or("");
    let mut out = String::new();
    let mut chars = source.chars().peekable();
    let mut block = 0_u32;
    let mut quoted = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if block > 0 {
            if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                block += 1;
            } else if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                block -= 1;
            } else if c == '\n' {
                out.push('\n');
            }
            continue;
        }
        if quoted {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quoted = false;
            }
            continue;
        }
        if c == '"' {
            quoted = true;
            out.push(c);
        } else if c == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for x in chars.by_ref() {
                if x == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            block = 1;
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}
#[test]
fn source_guard() {
    for (name, source) in [
        ("engine", include_str!("engine.rs")),
        ("slot_sink", include_str!("slot_sink.rs")),
    ] {
        let code = production_region(source);
        // catch_unwind, AssertUnwindSafe, JoinHandle and thread::Builder are allowed.
        for token in [
            "std::fs",
            "std::net",
            "Mutex",
            "RwLock",
            ".lock(",
            ".recv()",
            "thread::sleep",
            ".unwrap(",
            ".expect(",
            "panic!",
            "kohbor_pack",
            "tauri",
        ] {
            assert!(!code.contains(token), "{name} contains {token}");
        }
    }
}
#[test]
fn lexical_guard_strips_nested_comments_and_test_suffix() {
    let code = production_region(
        "let x = 1; /* Mutex /* std::fs */ panic! */ // RwLock\nlet y = 2;\n#[cfg(test)]\nstd::net",
    );
    assert!(
        code.contains("let x = 1") && code.contains("let y = 2"),
        "code is retained"
    );
    for token in ["Mutex", "std::fs", "panic!", "RwLock", "std::net"] {
        assert!(
            !code.contains(token),
            "comment or test code stripped: {token}"
        );
    }
}

#[test]
fn stale_session_is_not_applied() {
    let (mut h, _) = setup();
    assert!(h.tx.control(Control::Session { epoch: 2 }).is_ok());
    h.engine.step(1);
    h.drain();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    assert!(
        has_outcome(&h.drain(), "stale_session"),
        "lower epoch remains stale"
    );
}
#[test]
fn newer_tick_session_fences_old_playback() {
    let (mut h, _) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    assert_eq!(
        h.tx.offer_tick(tick(h.origin, 2, 3, 2, &[(EventId::HeroKill, None)])),
        Offer::Sent
    );
    h.engine.step(20);
    let o = h.drain();
    assert!(
        has_end(&o, "cancelled_session"),
        "newer tick fences old session"
    );
    assert!(has_outcome(&o, "admitted"), "new session request admitted");
}
#[test]
fn tier_fallback_selects_highest_bound_tier() {
    let (sink, log) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), false, 256);
    assert!(h
        .tx
        .control(Control::SwapTable {
            table: table(
                7,
                &[
                    (EventId::MultiKill, Some(2), 4800, 0, None),
                    (EventId::MultiKill, Some(3), 9600, 0, None)
                ]
            )
        })
        .is_ok());
    h.engine.step(0);
    h.drain();
    offer(&h, 1, &[(EventId::MultiKill, Some(4))]);
    h.engine.step(10);
    assert_eq!(
        log.lock().unwrap().calls,
        vec![Call::Submit(PlaybackId(1), 9600)],
        "tier 4 selects bound tier 3"
    );
    assert_eq!(playbacks(&h.drain())[0]["tier"], 3, "I1 exposes bound tier");
}
#[test]
fn missing_tier_never_selects_tiered_binding() {
    let (sink, log) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), false, 256);
    assert!(h
        .tx
        .control(Control::SwapTable {
            table: table(7, &[(EventId::MultiKill, Some(2), 4800, 0, None)])
        })
        .is_ok());
    h.engine.step(0);
    h.drain();
    offer(&h, 1, &[(EventId::MultiKill, None)]);
    h.engine.step(10);
    assert!(
        has_outcome(&h.drain(), "unmapped"),
        "tierless occurrence requires tierless binding"
    );
    assert!(log.lock().unwrap().calls.is_empty());
}
#[test]
fn latency_off_emits_no_timing() {
    let (mut h, _) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    assert!(
        !h.drain()
            .iter()
            .any(|o| matches!(o, EngineOut::Latency { .. })),
        "latency disabled"
    );
}
#[test]
fn latency_on_reports_arrival_relative_offsets() {
    let (sink, _) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), true, 256);
    assert_eq!(h.clock.ms_at(h.origin), 0, "fixed origin");
    assert!(h
        .tx
        .control(Control::SwapTable {
            table: table(7, &[(EventId::HeroKill, None, 4800, 0, None)])
        })
        .is_ok());
    h.engine.step(0);
    h.drain();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    let o = h.drain();
    let timing = o
        .iter()
        .find_map(|x| {
            if let EngineOut::Latency {
                instance_id,
                event_type,
                t0_ms,
                t1_us,
                t2_us,
                t3p_us,
            } = x
            {
                Some((instance_id, event_type, *t0_ms, *t1_us, *t2_us, *t3p_us))
            } else {
                None
            }
        })
        .expect("admitted timing");
    assert_eq!(timing.0, "ev_g3_s1_0");
    assert_eq!(timing.1.as_deref(), Some("hero_kill"));
    assert_eq!(timing.2, 0);
    assert_eq!(timing.3, 50);
    assert!(
        timing.4 >= timing.3 && timing.5 >= timing.4,
        "ordered offsets"
    );
}
#[test]
fn ordinary_lane_drains_at_most_sixteen() {
    let (mut h, _) = setup();
    for i in 0..40 {
        offer(&h, i, &[(EventId::HeroRespawn, None)]);
    }
    h.engine.step(10);
    assert_eq!(outcomes(&h.drain()).len(), 16, "first step drains 16");
    h.engine.step(20);
    assert_eq!(outcomes(&h.drain()).len(), 16, "second step drains 16");
    h.engine.step(30);
    assert_eq!(outcomes(&h.drain()).len(), 8, "third step drains remainder");
}

#[test]
fn preemption_ends_old_before_new_start() {
    let (mut h, _) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    offer(&h, 2, &[(EventId::HealthDanger, None)]);
    h.engine.step(20);
    let o = h.drain();
    let p = playbacks(&o);
    assert_eq!(p.len(), 2, "preemption produces two playback messages");
    assert_eq!(p[0]["reason"], "cancelled_preempted", "old ends first");
    assert_eq!(p[1]["kind"], "started", "new starts second");
    assert!(has_outcome(&o, "preempted"), "audio preemption recorded");
    assert!(has_outcome(&o, "admitted"), "new request admitted");
}
#[test]
fn unmute_does_not_restore_cancelled_playback() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    for muted in [true, false] {
        let (tx, rx) = mpsc::sync_channel(1);
        assert!(h.tx.control(Control::SetMuted { muted, ack: tx }).is_ok());
        h.engine.step(20);
        assert_eq!(rx.try_recv(), Ok(muted), "mute acknowledgement");
        h.drain();
    }
    assert!(
        !h.shared.muted.load(Ordering::Relaxed),
        "unmute reflected in shared state"
    );
    assert_eq!(
        log.lock()
            .unwrap()
            .calls
            .iter()
            .filter(|x| matches!(x, Call::Submit(..)))
            .count(),
        1,
        "unmute does not replay"
    );
}
#[test]
fn sink_ready_restores_service_after_loss() {
    let (mut h, old) = setup();
    old.lock().unwrap().events.push_back(SinkEvent::DeviceLost);
    h.engine.step(10);
    h.drain();
    let (new, new_log) = FakeSink::new();
    assert!(h.tx.control(Control::SinkReady(Box::new(new))).is_ok());
    h.engine.step(20);
    let o = h.drain();
    assert!(
        o.iter().any(|x| matches!(x, EngineOut::ReleaseSink(_))),
        "old sink released"
    );
    assert!(
        h.shared.device_available.load(Ordering::Relaxed),
        "device recovered"
    );
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(30);
    assert_eq!(
        new_log.lock().unwrap().calls.len(),
        1,
        "new sink receives playback"
    );
}
#[test]
fn critical_lane_runs_before_control_lane() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HealthDanger, None)]);
    let (tx, rx) = mpsc::sync_channel(1);
    assert!(h
        .tx
        .control(Control::SetMuted {
            muted: true,
            ack: tx
        })
        .is_ok());
    h.engine.step(10);
    assert_eq!(rx.try_recv(), Ok(true));
    assert_eq!(
        log.lock().unwrap().calls,
        vec![
            Call::Submit(PlaybackId(1), 4801),
            Call::Cancel(PlaybackId(1))
        ],
        "critical submit precedes mute cancel"
    );
}
#[test]
fn current_revision_table_is_used_after_swap() {
    let (mut h, log) = setup();
    assert!(h
        .tx
        .control(Control::SwapTable {
            table: table(8, &[(EventId::HeroKill, None, 9600, 0, None)])
        })
        .is_ok());
    h.engine.step(10);
    h.drain();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(20);
    assert_eq!(
        log.lock().unwrap().calls,
        vec![Call::Submit(PlaybackId(1), 9600)],
        "new table samples used"
    );
    assert!(
        outcomes(&h.drain())
            .iter()
            .any(|v| v["package_revision"] == 8),
        "new revision reported"
    );
}

fn start_with_tail() -> (Harness, super::fake_sink::Log) {
    let (sink, log) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), false, 256);
    assert!(
        h.tx.control(Control::SwapTable {
            table: table(
                7,
                &[
                    (EventId::HeroKill, None, 4800, 200, None),
                    (EventId::HealthDanger, None, 4800, 0, None)
                ]
            )
        })
        .is_ok(),
        "tail table queued"
    );
    h.engine.step(0);
    h.drain();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    (h, log)
}
#[test]
fn preemption_during_audio_records_cancel_time() {
    let (mut h, _) = start_with_tail();
    offer(&h, 2, &[(EventId::HealthDanger, None)]);
    h.engine.step(50);
    let o = h.drain();
    let old = outcomes(&o)
        .into_iter()
        .find(|v| v["outcome"] == "preempted")
        .expect("preempted record before audio end");
    assert_eq!(
        old["decided_monotonic_ms"], 50,
        "preemption records cancellation time"
    );
    assert_eq!(
        old["instance_id"], "ev_g3_s1_0",
        "preemption records old occurrence"
    );
}
#[test]
fn preemption_during_tail_has_no_preempted_record() {
    let (mut h, _) = start_with_tail();
    h.engine.step(120);
    h.drain();
    offer(&h, 2, &[(EventId::HealthDanger, None)]);
    h.engine.step(150);
    let o = h.drain();
    assert!(
        has_end(&o, "cancelled_preempted"),
        "tail end marked preempted"
    );
    assert!(
        !has_outcome(&o, "preempted"),
        "tail-only cut has no preempted outcome"
    );
}
#[test]
fn lease_stays_active_during_tail() {
    let (mut h, _) = start_with_tail();
    h.engine.step(120);
    assert!(
        h.shared.lease_active.load(Ordering::Relaxed),
        "visual tail retains lease"
    );
    assert_eq!(
        h.engine.step(130),
        Duration::from_millis(10),
        "tail retains short wait"
    );
}
#[test]
fn lease_ends_after_tail() {
    let (mut h, _) = start_with_tail();
    h.engine.step(311);
    assert!(
        !h.shared.lease_active.load(Ordering::Relaxed),
        "lease released after tail"
    );
    assert_eq!(
        h.engine.step(312),
        Duration::from_millis(250),
        "idle after tail"
    );
}
#[test]
fn lease_clears_after_mute() {
    let (mut h, _) = start_with_tail();
    let (tx, _) = mpsc::sync_channel(1);
    assert!(h
        .tx
        .control(Control::SetMuted {
            muted: true,
            ack: tx
        })
        .is_ok());
    h.engine.step(20);
    assert!(
        !h.shared.lease_active.load(Ordering::Relaxed),
        "cancel releases lease"
    );
}
#[test]
fn heartbeat_tracks_multiple_steps() {
    let (mut h, _) = setup();
    for time in [1, 12, 99, 1000] {
        h.engine.step(time);
        assert_eq!(
            h.shared.heartbeat_ms.load(Ordering::Relaxed),
            time,
            "heartbeat follows each monotonic step"
        );
    }
}
#[test]
fn control_capacity_and_single_step_drain() {
    let (mut h, _) = setup();
    let mut acks = Vec::new();
    for i in 0..32 {
        let (tx, rx) = mpsc::sync_channel(1);
        assert!(
            h.tx.control(Control::SetMuted {
                muted: i % 2 == 0,
                ack: tx
            })
            .is_ok(),
            "first 32 controls fit"
        );
        acks.push((rx, i % 2 == 0));
    }
    let (tx, _) = mpsc::sync_channel(1);
    assert!(
        h.tx.control(Control::SetMuted {
            muted: true,
            ack: tx
        })
        .is_err(),
        "33rd control rejected"
    );
    h.engine.step(10);
    for (rx, expected) in acks {
        assert_eq!(
            rx.try_recv(),
            Ok(expected),
            "each control acknowledged in same step"
        );
    }
    assert!(
        !h.shared.muted.load(Ordering::Relaxed),
        "last control atomics reflected"
    );
}
#[test]
fn respawn_never_calls_sink() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HeroRespawn, None)]);
    h.engine.step(10);
    assert!(has_outcome(&h.drain(), "not_voiced"), "respawn outcome");
    assert!(
        log.lock().unwrap().calls.is_empty(),
        "respawn makes no sink call"
    );
}
#[test]
fn arbitration_records_each_loser() {
    let (mut h, log) = setup();
    offer(
        &h,
        1,
        &[
            (EventId::HeroDeath, None),
            (EventId::HeroKill, None),
            (EventId::HealthLow, None),
        ],
    );
    h.engine.step(10);
    let o = h.drain();
    let losers: Vec<_> = outcomes(&o)
        .into_iter()
        .filter(|v| v["outcome"] == "lower_rank_same_tick")
        .collect();
    assert_eq!(losers.len(), 2, "one record per arbitration loser");
    assert!(losers.iter().any(|v| v["instance_id"] == "ev_g3_s1_0"));
    assert!(losers.iter().any(|v| v["instance_id"] == "ev_g3_s1_2"));
    assert_eq!(
        log.lock().unwrap().calls.len(),
        1,
        "winner alone reaches sink"
    );
}
#[test]
fn unmapped_winner_does_not_fall_back_to_loser() {
    let (mut h, log) = setup();
    offer(
        &h,
        1,
        &[(EventId::MultiKill, Some(4)), (EventId::HeroKill, None)],
    );
    h.engine.step(10);
    let o = h.drain();
    assert!(has_outcome(&o, "unmapped"), "winning event unmapped");
    assert!(has_outcome(&o, "lower_rank_same_tick"), "loser recorded");
    assert!(
        log.lock().unwrap().calls.is_empty(),
        "no loser substitution"
    );
}
#[test]
fn invalid_critical_allows_ordinary() {
    let (sink, log) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), false, 256);
    let base = table(7, &[(EventId::HeroKill, None, 4800, 0, None)]);
    let clip = match base.lookup(EventId::HeroKill, None) {
        super::resolve::Lookup::Ready(c) => c.clone(),
        _ => panic!("fixture"),
    };
    let t = Arc::new(PrewarmTable::new(
        7,
        Some(Arc::from("ins_0000000007")),
        vec![
            (EventId::HealthDanger, None, Entry::Invalid),
            (EventId::HeroKill, None, Entry::Ready(clip)),
        ],
    ));
    assert!(h.tx.control(Control::SwapTable { table: t }).is_ok());
    h.engine.step(0);
    h.drain();
    offer(
        &h,
        1,
        &[(EventId::HealthDanger, None), (EventId::HeroKill, None)],
    );
    h.engine.step(10);
    let o = h.drain();
    assert!(
        has_outcome(&o, "invalid_media"),
        "invalid critical recorded"
    );
    assert!(has_outcome(&o, "admitted"), "independent ordinary admitted");
    assert_eq!(
        log.lock().unwrap().calls.len(),
        1,
        "only ordinary submitted"
    );
}
#[test]
fn device_lost_requests_are_channel_busy() {
    let (mut h, log) = setup();
    log.lock().unwrap().events.push_back(SinkEvent::DeviceLost);
    h.engine.step(10);
    h.drain();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(20);
    assert!(
        has_outcome(&h.drain(), "channel_busy"),
        "outage reports busy"
    );
    assert!(log.lock().unwrap().calls.is_empty(), "outage cannot submit");
}
#[test]
fn sink_finished_event_emits_finished() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    log.lock()
        .unwrap()
        .events
        .push_back(SinkEvent::Finished { id: PlaybackId(1) });
    h.engine.step(20);
    let o = h.drain();
    assert!(has_end(&o, "finished"), "sink completion maps to finished");
    assert!(outcomes(&o).is_empty(), "finish does not create outcome");
}
#[test]
fn failed_submit_records_channel_busy() {
    let (mut h, log) = setup();
    log.lock().unwrap().fail_next = true;
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    let o = h.drain();
    assert!(
        has_outcome(&o, "channel_busy"),
        "failed submit reports busy"
    );
    assert!(playbacks(&o).is_empty(), "failed submit does not start");
}
#[test]
fn replacement_waits_for_current_audio() {
    let (mut h, old) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    h.drain();
    old.lock()
        .unwrap()
        .events
        .push_back(SinkEvent::DeviceChanged);
    h.engine.step(20);
    h.drain();
    let (new, newlog) = FakeSink::new();
    assert!(h.tx.control(Control::SinkReady(Box::new(new))).is_ok());
    h.engine.step(30);
    assert!(
        !h.drain()
            .iter()
            .any(|o| matches!(o, EngineOut::ReleaseSink(_))),
        "active sink retained during replacement"
    );
    assert!(
        newlog.lock().unwrap().calls.is_empty(),
        "new sink unused during old audio"
    );
    h.engine.step(111);
    let mut o = h.drain();
    h.engine.step(112);
    o.extend(h.drain());
    assert!(
        o.iter().any(|o| matches!(o, EngineOut::ReleaseSink(_))),
        "old sink released after audio ends"
    );
    offer(&h, 2, &[(EventId::HeroKill, None)]);
    h.engine.step(120);
    assert_eq!(
        newlog.lock().unwrap().calls.len(),
        1,
        "new sink serves subsequent request"
    );
}

fn panic_at(
    h: &mut Harness,
    time: u64,
    sequence: u64,
    initial: Option<super::fake_sink::Log>,
) -> super::fake_sink::Log {
    let log = if let Some(log) = initial {
        log
    } else {
        let (sink, log) = FakeSink::new();
        assert!(
            h.tx.control(Control::SinkReady(Box::new(sink))).is_ok(),
            "replacement enqueued"
        );
        h.engine.step(time.saturating_sub(1));
        h.drain();
        log
    };
    log.lock().unwrap().panic_next = true;
    offer(h, sequence, &[(EventId::HeroKill, None)]);
    h.engine.step(time);
    let o = h.drain();
    assert!(
        o.iter().any(|v| matches!(v, EngineOut::NeedSink)),
        "panic requests replacement"
    );
    log
}
fn panic_window(times: [u64; 4]) -> Harness {
    let (mut h, log) = setup();
    for (i, t) in times.into_iter().enumerate() {
        panic_at(
            &mut h,
            t,
            i as u64 + 1,
            if i == 0 { Some(log.clone()) } else { None },
        );
    }
    h
}
#[test]
fn fourth_panic_at_60000_is_degraded() {
    let h = panic_window([10, 20, 30, 60010]);
    assert_eq!(
        h.shared.rebuilds.load(Ordering::Relaxed),
        4,
        "four rebuilds counted"
    );
    assert!(
        h.shared.degraded.load(Ordering::Relaxed),
        "inclusive 60000ms window degrades"
    );
}
#[test]
fn fourth_panic_at_60001_is_not_degraded() {
    let h = panic_window([10, 20, 30, 60011]);
    assert_eq!(
        h.shared.rebuilds.load(Ordering::Relaxed),
        4,
        "four rebuilds counted"
    );
    assert!(
        !h.shared.degraded.load(Ordering::Relaxed),
        "60001ms window does not degrade"
    );
}
#[test]
fn degraded_never_calls_replacement_sink() {
    let mut h = panic_window([10, 20, 30, 40]);
    let (sink, log) = FakeSink::new();
    assert!(h.tx.control(Control::SinkReady(Box::new(sink))).is_ok());
    h.engine.step(41);
    h.drain();
    offer(&h, 10, &[(EventId::HeroKill, None)]);
    h.engine.step(42);
    assert!(
        has_outcome(&h.drain(), "channel_busy"),
        "degraded request recorded busy"
    );
    assert!(
        log.lock().unwrap().calls.is_empty(),
        "degraded does not submit"
    );
    assert!(
        h.shared.degraded.load(Ordering::Relaxed),
        "degraded remains published"
    );
}
#[test]
fn panic_cancels_live_and_preserves_id_sequence() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    let first = h.drain();
    let old = playbacks(&first)[0]["playback_id"]
        .as_str()
        .unwrap()
        .to_owned();
    log.lock().unwrap().panic_next = true;
    offer(&h, 2, &[(EventId::HealthDanger, None)]);
    h.engine.step(20);
    let o = h.drain();
    assert!(
        has_end(&o, "cancelled_device_lost"),
        "panic ends live playback"
    );
    let (sink, _) = FakeSink::new();
    assert!(h.tx.control(Control::SinkReady(Box::new(sink))).is_ok());
    h.engine.step(21);
    h.drain();
    offer(&h, 3, &[(EventId::HeroKill, None)]);
    h.engine.step(22);
    let o = h.drain();
    let id = playbacks(&o)[0]["playback_id"].as_str().unwrap();
    let number = |s: &str| s.strip_prefix("pb_").unwrap().parse::<u64>().unwrap();
    assert!(
        number(id) > number(&old),
        "rebuilt owner never reuses issued id"
    );
}
#[test]
fn repeated_rebuilds_preserve_playback_id_continuity() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    let o = h.drain();
    let first = playbacks(&o)[0]["playback_id"].as_str().unwrap().to_owned();
    log.lock().unwrap().panic_next = true;
    offer(&h, 2, &[(EventId::HealthDanger, None)]);
    h.engine.step(20);
    h.drain();
    let second = panic_at(&mut h, 30, 3, None);
    let third = panic_at(&mut h, 40, 4, None);
    let max_issued = [&log, &second, &third]
        .iter()
        .flat_map(|entry| entry.lock().unwrap().calls.clone())
        .filter_map(|call| match call {
            Call::Submit(id, _) => Some(id.0),
            Call::Cancel(_) => None,
        })
        .max()
        .expect("submitted ids observed");
    let (sink, _) = FakeSink::new();
    assert!(h.tx.control(Control::SinkReady(Box::new(sink))).is_ok());
    h.engine.step(41);
    h.drain();
    offer(&h, 5, &[(EventId::HeroKill, None)]);
    h.engine.step(42);
    let o = h.drain();
    let id = playbacks(&o)[0]["playback_id"].as_str().unwrap();
    let n = |s: &str| s.strip_prefix("pb_").unwrap().parse::<u64>().unwrap();
    assert!(
        n(id) > n(&first) && n(id) > max_issued,
        "multiple rebuilds retain every submitted id high-water mark"
    );
}
#[test]
fn rebuild_preserves_session_and_table() {
    let (mut h, log) = setup();
    assert!(h.tx.control(Control::Session { epoch: 2 }).is_ok());
    h.engine.step(1);
    h.drain();
    log.lock().unwrap().panic_next = true;
    assert_eq!(
        h.tx.offer_tick(tick(h.origin, 2, 3, 1, &[(EventId::HeroKill, None)])),
        Offer::Sent
    );
    h.engine.step(10);
    h.drain();
    let (sink, _) = FakeSink::new();
    assert!(h.tx.control(Control::SinkReady(Box::new(sink))).is_ok());
    h.engine.step(11);
    h.drain();
    assert_eq!(
        h.tx.offer_tick(tick(h.origin, 2, 3, 2, &[(EventId::HeroKill, None)])),
        Offer::Sent
    );
    h.engine.step(12);
    let o = h.drain();
    assert!(
        has_outcome(&o, "admitted"),
        "rebuilt session and mapping preserved"
    );
    assert_eq!(
        h.shared.fence.load(Ordering::Relaxed),
        7,
        "rebuilt table fence preserved"
    );
}
#[test]
fn full_out_lane_does_not_block_step() {
    let (sink, _) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), false, 1);
    let shared = h.shared.clone();
    assert!(h
        .tx
        .control(Control::SwapTable {
            table: table(7, &[(EventId::HeroKill, None, 4800, 0, None)])
        })
        .is_ok());
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        h.engine.step(10);
        done_tx.send(()).unwrap();
    });
    assert!(
        done_rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "full out lane never blocks engine step"
    );
    worker.join().unwrap();
    assert!(
        shared.out_dropped.load(Ordering::Relaxed) > 0,
        "full out lane counts drops"
    );
}
#[test]
fn spawn_smoke_shutdown_releases_sink() {
    let origin = std::time::Instant::now();
    let clock = crate::clock::MonoClock::from_origin(origin);
    let shared = Arc::new(super::lanes::EngineShared::new());
    let (out_tx, out_rx) = mpsc::sync_channel(256);
    let (tx, worker) =
        super::engine::Engine::spawn(clock, shared, out_tx, false).expect("engine thread starts");
    assert!(
        matches!(
            out_rx.recv_timeout(Duration::from_millis(500)),
            Ok(EngineOut::NeedSink)
        ),
        "spawn asks for sink"
    );
    let (sink, log) = FakeSink::new();
    assert!(tx.control(Control::SinkReady(Box::new(sink))).is_ok());
    assert!(tx
        .control(Control::SwapTable {
            table: table(7, &[(EventId::HeroKill, None, 4800, 0, None)])
        })
        .is_ok());
    assert_eq!(
        tx.offer_tick(tick(origin, 1, 3, 1, &[(EventId::HeroKill, None)])),
        Offer::Sent,
        "spawn returns usable sender"
    );
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    let mut admitted = false;
    while std::time::Instant::now() < deadline {
        match out_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(EngineOut::Outcome(v)) => {
                check_record(&v);
                if v["outcome"] == "admitted" {
                    admitted = true;
                    break;
                }
            }
            Ok(EngineOut::Playback(v)) => check_playback(&v),
            _ => {}
        }
    }
    assert!(admitted, "spawn processes tick");
    assert_eq!(log.lock().unwrap().calls.len(), 1, "spawn submits clip");
    assert!(tx.control(Control::Shutdown).is_ok());
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    let mut released = false;
    while std::time::Instant::now() < deadline {
        match out_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(EngineOut::ReleaseSink(_)) => {
                released = true;
                break;
            }
            Ok(EngineOut::Playback(v)) => check_playback(&v),
            Ok(EngineOut::Outcome(v)) => check_record(&v),
            _ => {}
        }
    }
    assert!(released, "shutdown releases sink");
    worker.join().expect("shutdown joins engine");
}

macro_rules! voiced_mapping {
    ($name:ident,$id:expr) => {
        #[test]
        fn $name() {
            let (sink, log) = FakeSink::new();
            let mut h = Harness::new(Some(Box::new(sink)), false, 256);
            assert!(
                h.tx.control(Control::SwapTable {
                    table: table(7, &[($id, None, 4800, 0, None)])
                })
                .is_ok(),
                "mapping table queued"
            );
            h.engine.step(0);
            h.drain();
            offer(&h, 1, &[($id, None)]);
            h.engine.step(10);
            let o = h.drain();
            assert!(has_outcome(&o, "admitted"), "mapped voiced event admitted");
            assert_eq!(
                playbacks(&o)[0]["event_type"],
                $id.as_str(),
                "I1 event matches mapped occurrence"
            );
            assert_eq!(
                log.lock().unwrap().calls.len(),
                1,
                "one sink submit for mapped event"
            );
        }
    };
}
voiced_mapping!(mapping_multi_kill, EventId::MultiKill);
voiced_mapping!(mapping_streak, EventId::Streak);
voiced_mapping!(mapping_hero_death, EventId::HeroDeath);
voiced_mapping!(mapping_level_milestone, EventId::LevelMilestone);
voiced_mapping!(mapping_health_low, EventId::HealthLow);
voiced_mapping!(mapping_mana_low, EventId::ManaLow);
voiced_mapping!(mapping_match_ended, EventId::MatchEnded);
voiced_mapping!(mapping_match_started, EventId::MatchStarted);
macro_rules! tier_case {
    ($name:ident,$request:expr,$expected:expr,$count:expr) => {
        #[test]
        fn $name() {
            let (sink, log) = FakeSink::new();
            let mut h = Harness::new(Some(Box::new(sink)), false, 256);
            assert!(h
                .tx
                .control(Control::SwapTable {
                    table: table(
                        7,
                        &[
                            (EventId::MultiKill, None, 1000, 0, None),
                            (EventId::MultiKill, Some(2), 2000, 0, None),
                            (EventId::MultiKill, Some(3), 3000, 0, None)
                        ]
                    )
                })
                .is_ok());
            h.engine.step(0);
            h.drain();
            offer(&h, 1, &[(EventId::MultiKill, $request)]);
            h.engine.step(10);
            let o = h.drain();
            assert_eq!(
                log.lock().unwrap().calls,
                vec![Call::Submit(PlaybackId(1), $count)],
                "tier rule selects expected samples"
            );
            assert_eq!(
                playbacks(&o)[0]["tier"],
                serde_json::json!($expected),
                "I1 contains selected bound tier"
            );
        }
    };
}
tier_case!(tier_none_selects_tierless, None, None::<u8>, 1000);
tier_case!(tier_below_bound_selects_tierless, Some(1), None::<u8>, 1000);
tier_case!(tier_exact_two, Some(2), Some(2), 2000);
tier_case!(tier_exact_three, Some(3), Some(3), 3000);
tier_case!(tier_above_range, Some(255), Some(3), 3000);
#[test]
fn swap_updates_fence_and_preserves_device_mute() {
    let (mut h, _) = setup();
    let (tx, _) = mpsc::sync_channel(1);
    assert!(h
        .tx
        .control(Control::SetMuted {
            muted: true,
            ack: tx
        })
        .is_ok());
    h.engine.step(1);
    h.drain();
    assert!(h
        .tx
        .control(Control::SwapTable {
            table: table(8, &[])
        })
        .is_ok());
    h.engine.step(2);
    assert_eq!(
        h.shared.fence.load(Ordering::Relaxed),
        8,
        "swap fence atomic"
    );
    assert!(
        h.shared.muted.load(Ordering::Relaxed),
        "swap retains mute atomic"
    );
    assert!(
        h.shared.device_available.load(Ordering::Relaxed),
        "swap retains device atomic"
    );
}
#[test]
fn session_preserves_other_atomics() {
    let (mut h, _) = setup();
    assert!(h.tx.control(Control::Session { epoch: 3 }).is_ok());
    h.engine.step(8);
    assert_eq!(
        h.shared.heartbeat_ms.load(Ordering::Relaxed),
        8,
        "session heartbeat"
    );
    assert_eq!(
        h.shared.fence.load(Ordering::Relaxed),
        7,
        "session retains fence"
    );
    assert!(
        h.shared.device_available.load(Ordering::Relaxed),
        "session retains device"
    );
    assert!(
        !h.shared.muted.load(Ordering::Relaxed),
        "session retains mute"
    );
}
#[test]
fn sink_ready_updates_device_atomic() {
    let mut h = Harness::new(None, false, 256);
    h.engine.step(0);
    h.drain();
    let (sink, _) = FakeSink::new();
    assert!(h.tx.control(Control::SinkReady(Box::new(sink))).is_ok());
    h.engine.step(1);
    assert!(
        h.shared.device_available.load(Ordering::Relaxed),
        "sink ready publishes device available"
    );
    assert!(
        !h.shared.muted.load(Ordering::Relaxed),
        "sink ready retains unmuted"
    );
    assert_eq!(
        h.shared.fence.load(Ordering::Relaxed),
        0,
        "sink ready retains initial fence"
    );
}
#[test]
fn mute_updates_atomic_in_same_step() {
    let (mut h, _) = setup();
    let (tx, rx) = mpsc::sync_channel(1);
    assert!(h
        .tx
        .control(Control::SetMuted {
            muted: true,
            ack: tx
        })
        .is_ok());
    h.engine.step(5);
    assert_eq!(rx.try_recv(), Ok(true), "mute ack");
    assert!(h.shared.muted.load(Ordering::Relaxed), "mute atomic");
    assert!(
        h.shared.device_available.load(Ordering::Relaxed),
        "mute retains device"
    );
    assert_eq!(
        h.shared.fence.load(Ordering::Relaxed),
        7,
        "mute retains fence"
    );
}
#[test]
fn critical_lane_is_fully_drained() {
    let (mut h, _) = setup();
    for i in 0..8 {
        offer(&h, i, &[(EventId::HealthDanger, None)]);
    }
    h.engine.step(10);
    let outputs = h.drain();
    assert_eq!(
        outcomes(&outputs)
            .iter()
            .filter(|v| v["outcome"] == "admitted")
            .count(),
        8,
        "all eight critical ticks admitted in one step"
    );
}
#[test]
fn critical_lane_precedes_ordinary_lane() {
    let (mut h, log) = setup();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    offer(&h, 2, &[(EventId::HealthDanger, None)]);
    h.engine.step(10);
    let o = h.drain();
    assert_eq!(
        log.lock().unwrap().calls,
        vec![Call::Submit(PlaybackId(1), 4801)],
        "critical submitted before earlier ordinary"
    );
    assert!(
        has_outcome(&o, "critical_active"),
        "ordinary sees active critical"
    );
}
#[test]
fn slot_empty_changed_flag_is_false() {
    let mut s = SlotSink::empty();
    assert!(!s.take_changed(), "empty slot has no device change");
    assert!(!s.take_changed(), "flag stays clear");
}
#[test]
fn slot_forwards_submit_and_cancel() {
    let mut s = SlotSink::empty();
    let (sink, log) = FakeSink::new();
    s.install(Box::new(sink));
    assert!(
        s.submit(PlaybackId(7), Arc::from([0_i16; 4])).is_ok(),
        "slot forwards submit"
    );
    s.cancel(PlaybackId(7));
    assert_eq!(
        log.lock().unwrap().calls,
        vec![Call::Submit(PlaybackId(7), 4), Call::Cancel(PlaybackId(7))],
        "slot preserves call order"
    );
}
#[test]
fn slot_forwards_device_lost_without_changed_flag() {
    let mut s = SlotSink::empty();
    let (sink, log) = FakeSink::new();
    s.install(Box::new(sink));
    log.lock().unwrap().events.push_back(SinkEvent::DeviceLost);
    assert_eq!(
        s.poll_event(),
        Some(SinkEvent::DeviceLost),
        "device loss forwarded"
    );
    assert!(!s.take_changed(), "loss is not default change");
}
#[test]
fn slot_forwards_finished_without_changed_flag() {
    let mut s = SlotSink::empty();
    let (sink, log) = FakeSink::new();
    s.install(Box::new(sink));
    log.lock()
        .unwrap()
        .events
        .push_back(SinkEvent::Finished { id: PlaybackId(9) });
    assert_eq!(
        s.poll_event(),
        Some(SinkEvent::Finished { id: PlaybackId(9) }),
        "finish forwarded"
    );
    assert!(!s.take_changed(), "finish is not default change");
}

#[test]
fn distinct_critical_occurrences_are_distinct_episodes() {
    let (mut h, log) = setup();
    offer(
        &h,
        1,
        &[(EventId::HealthDanger, None), (EventId::HealthDanger, None)],
    );
    h.engine.step(10);
    let o = h.drain();
    let p = playbacks(&o);
    assert_eq!(
        log.lock()
            .unwrap()
            .calls
            .iter()
            .filter(|x| matches!(x, Call::Submit(..)))
            .count(),
        2,
        "different occurrence positions are different episodes"
    );
    assert_eq!(
        p.iter().filter(|v| v["kind"] == "started").count(),
        2,
        "both critical occurrences admitted in order"
    );
    assert!(
        outcomes(&o)
            .iter()
            .any(|v| v["instance_id"] == "ev_g3_s1_1" && v["outcome"] == "admitted"),
        "second critical identity preserved"
    );
}
#[test]
fn latency_is_not_emitted_for_unmapped() {
    let (sink, _) = FakeSink::new();
    let mut h = Harness::new(Some(Box::new(sink)), true, 256);
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(10);
    let o = h.drain();
    assert!(has_outcome(&o, "unmapped"), "unmapped request outcome");
    assert!(
        !o.iter().any(|v| matches!(v, EngineOut::Latency { .. })),
        "only admitted requests emit latency"
    );
}
#[test]
fn record_checker_accepts_valid_outcomes() {
    for outcome in ["admitted", "preempted", "unmapped", "channel_busy"] {
        let id = if matches!(outcome, "admitted" | "preempted") {
            serde_json::json!("pb_0000000001")
        } else {
            Value::Null
        };
        let v = serde_json::json!({"schema_version":"1.0.0","instance_id":"ev_g3_s1_0","event_type":"hero_kill","session_epoch":1,"audio_class":"ordinary","outcome":outcome,"playback_id":id,"package_revision":7,"decided_monotonic_ms":10});
        check_record(&v);
    }
}
#[test]
fn record_checker_rejects_malformed_outcomes() {
    let base = serde_json::json!({"schema_version":"1.0.0","instance_id":"ev_g3_s1_0","event_type":"hero_kill","session_epoch":1,"audio_class":"ordinary","outcome":"admitted","playback_id":"pb_0000000001","package_revision":7,"decided_monotonic_ms":10});
    for (field, bad) in [
        ("instance_id", serde_json::json!("bad")),
        ("session_epoch", serde_json::json!(0)),
        ("audio_class", serde_json::json!("bad")),
        ("playback_id", Value::Null),
        ("event_type", serde_json::json!("Bad")),
        ("package_revision", Value::Null),
    ] {
        let mut v = base.clone();
        v[field] = bad;
        assert!(
            std::panic::catch_unwind(|| check_record(&v)).is_err(),
            "checker rejects malformed {field}"
        );
    }
}
#[test]
fn playback_checker_accepts_renderer_shapes() {
    let start = serde_json::json!({"kind":"started","playback_id":"pb_0000000001","class":"critical","event_type":"health_danger","tier":null,"visual_url":null,"audio_duration_ms":101,"visual_tail_ms":0,"at_ms":10});
    check_playback(&start);
    for reason in [
        "finished",
        "cancelled_preempted",
        "cancelled_session",
        "cancelled_revision",
        "cancelled_muted",
        "cancelled_device_lost",
    ] {
        check_playback(
            &serde_json::json!({"kind":"ended","playback_id":"pb_0000000001","reason":reason,"at_ms":20}),
        );
    }
}
#[test]
fn playback_checker_rejects_bad_shapes() {
    let base = serde_json::json!({"kind":"ended","playback_id":"pb_0000000001","reason":"finished","at_ms":20});
    for (field, bad) in [
        ("playback_id", serde_json::json!("bad")),
        ("reason", serde_json::json!("bogus")),
        ("at_ms", serde_json::json!(-1)),
        ("extra", serde_json::json!(true)),
    ] {
        let mut v = base.clone();
        v[field] = bad;
        assert!(
            std::panic::catch_unwind(|| check_playback(&v)).is_err(),
            "checker rejects bad {field}"
        );
    }
}

#[test]
fn rebuild_preserves_muted_state() {
    let (mut h, log) = setup();
    let (ack, rx) = mpsc::sync_channel(1);
    assert!(h.tx.control(Control::SetMuted { muted: true, ack }).is_ok());
    h.engine.step(1);
    assert_eq!(rx.try_recv(), Ok(true), "pre-panic mute acknowledged");
    h.drain();
    // A muted owner cannot reach submit, so inject a poll panic to observe restoration.
    log.lock().unwrap().panic_next_poll = true;
    h.engine.step(10);
    assert_eq!(
        h.shared.rebuilds.load(Ordering::Relaxed),
        1,
        "poll panic rebuilds owner"
    );
    h.drain();
    let (sink, new_log) = FakeSink::new();
    assert!(h.tx.control(Control::SinkReady(Box::new(sink))).is_ok());
    h.engine.step(11);
    h.drain();
    offer(&h, 1, &[(EventId::HeroKill, None)]);
    h.engine.step(12);
    assert!(
        h.shared.muted.load(Ordering::Relaxed),
        "mute retained across rebuild"
    );
    assert!(
        has_outcome(&h.drain(), "muted"),
        "rebuilt owner rejects while muted"
    );
    assert!(
        new_log.lock().unwrap().calls.is_empty(),
        "muted rebuilt owner never submits"
    );
}
