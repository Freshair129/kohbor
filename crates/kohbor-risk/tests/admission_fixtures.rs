mod support;

use kohbor_capture::Coverage;
use kohbor_risk::admission::{
    self, AudibleDecision, Gate, MapPos, OwnState, ParamValue, Params, ProposedEvaluator, Sighting,
    Tick,
};
use std::collections::BTreeMap;
use support::{load, Json};

fn close(actual: Option<f64>, expected: Option<f64>, context: &str) {
    match (actual, expected) {
        (None, None) => (),
        (Some(a), Some(b)) if (a - b).abs() <= 1e-6 => (),
        _ => panic!("{context}: actual {actual:?}, expected {expected:?}"),
    }
}
fn pos(v: &Json) -> MapPos {
    MapPos {
        x: v.get("x").number(),
        y: v.get("y").number(),
    }
}

#[test]
fn tables() {
    let p = Params::default();
    for row in load("score-table.json").get("rows").array() {
        let s = match row.get("missing_seconds") {
            Json::String(s) if s == "NaN" => f64::NAN,
            v => v.number(),
        };
        close(
            admission::base_risk(s, &p),
            row.get("expected_score").optional_number(),
            &format!("score-table {s}"),
        );
    }
    for row in load("heading-table.json").get("rows").array() {
        let id = row.get("id").string();
        let history: Vec<_> = row
            .get("history")
            .array()
            .iter()
            .map(|v| (v.get("t_ms").number() as u64, pos(v)))
            .collect();
        let h = admission::heading(&history, &p);
        assert_eq!(
            h.is_some(),
            row.get("valid").boolean(),
            "heading-table {id} valid"
        );
        close(
            Some(admission::heading_multiplier(
                h,
                history.last().unwrap().1,
                &p,
            )),
            Some(row.get("multiplier").number()),
            &format!("heading-table {id} multiplier"),
        );
    }
    for row in load("aggregate-table.json").get("rows").array() {
        let scores: Vec<_> = row.get("scores").array().iter().map(Json::number).collect();
        close(
            Some(admission::aggregate(&scores, &p)),
            Some(row.get("expected_score").number()),
            &format!("aggregate-table {}", row.get("id").string()),
        );
    }
    for row in load("eta-table.json").get("rows").array() {
        let actual: Vec<_> = row
            .get("missing_seconds")
            .array()
            .iter()
            .map(|v| admission::eta_s(v.number()))
            .collect();
        let expected = row.get("per_hero_eta_s").array();
        assert_eq!(
            actual.len(),
            expected.len(),
            "eta-table {} length",
            row.get("id").string()
        );
        for (a, e) in actual.iter().zip(expected) {
            close(Some(*a), Some(e.number()), row.get("id").string());
        }
        close(
            actual.iter().copied().reduce(f64::min),
            Some(row.get("group_eta_s").number()),
            row.get("id").string(),
        );
    }
}

// DEC-RISK-DETAIL is open: the build fails if the audible cue is switched on.
const _: () = assert!(!admission::AUDIBLE_MISSING_GANK_APPROVED);

#[test]
fn traces_and_audible_off() {
    for entry in load("index.json").get("fixtures").array() {
        if entry.get("kind").string() != "trace" {
            continue;
        }
        let id = entry.get("id").string();
        let trace = load(entry.get("file").string());
        let mut params = Params::default();
        if let Json::Object(root) = &trace {
            if let Some(Json::Object(overrides)) = root.get("params_override") {
                for (name, v) in overrides {
                    let v = match v {
                        Json::Number(n) => ParamValue::Number(*n),
                        Json::Bool(b) => ParamValue::Bool(*b),
                        _ => panic!("{id}: invalid override {name}"),
                    };
                    params
                        .set(name, v)
                        .unwrap_or_else(|e| panic!("{id}: {name}: {e}"));
                }
            }
        }
        // Preserve lexical order and equality of the fixture's string track IDs.
        let mut tracks = BTreeMap::new();
        for tick in trace.get("ticks").array() {
            for s in tick.get("sightings").array() {
                tracks.insert(s.get("track").string().to_owned(), 0_u64);
            }
        }
        for (i, value) in tracks.values_mut().enumerate() {
            *value = i as u64;
        }
        let mut evaluator = ProposedEvaluator::new(params);
        let mut events = Vec::new();
        let probes = trace.get("expected").get("probes").array();
        let mut seen = 0;
        for t in trace.get("ticks").array() {
            let now = t.get("t_ms").number() as u64;
            let coverage = match t.get("coverage").string() {
                "valid" => Coverage::Valid,
                "stale" => Coverage::Stale,
                "unavailable" => Coverage::Unavailable,
                v => panic!("{id} {now}: coverage {v}"),
            };
            let own = match t.get("own") {
                Json::Null => None,
                v => Some(OwnState {
                    fresh: v.get("fresh").boolean(),
                    in_match: v.get("in_match").boolean(),
                    alive: v.get("alive").boolean(),
                    paused: match v.get("paused") {
                        Json::Null => None,
                        v => Some(v.boolean()),
                    },
                    kills: v.get("kills").optional_number().map(|n| n as u32),
                    assists: v.get("assists").optional_number().map(|n| n as u32),
                    map_pos: match v.get("map_pos") {
                        Json::Null => None,
                        v => Some(pos(v)),
                    },
                }),
            };
            let sightings = t
                .get("sightings")
                .array()
                .iter()
                .map(|s| Sighting {
                    track: tracks[s.get("track").string()],
                    hero: match s.get("hero") {
                        Json::Null => None,
                        v => Some(v.string().to_owned()),
                    },
                    pos: pos(s),
                })
                .collect();
            let out = evaluator.tick(&Tick {
                t_ms: now,
                coverage,
                own,
                media_ready: t.get("media_ready").boolean(),
                sightings,
            });
            assert_eq!(
                admission::audible_decision(&out),
                AudibleDecision::UnavailableDecisionOpen,
                "{id} {now} audible"
            );
            for probe in probes
                .iter()
                .filter(|p| p.get("t_ms").number() as u64 == now)
            {
                seen += 1;
                let reason = match out.gate {
                    Gate::Admit => "admit",
                    Gate::Blocked(r) => r.as_str(),
                };
                assert_eq!(reason, probe.get("reason").string(), "{id} {now} gate");
                close(
                    out.score_all,
                    probe.get("score_all").optional_number(),
                    &format!("{id} {now} score_all"),
                );
            }
            events.extend(out.events);
        }
        assert_eq!(seen, probes.len(), "{id} unmatched probes");
        let expected = trace.get("expected").get("events").array();
        assert_eq!(events.len(), expected.len(), "{id} event count");
        for (a, e) in events.iter().zip(expected) {
            let at = e.get("t_ms").number() as u64;
            assert_eq!(a.t_ms, at, "{id} {at} event t_ms");
            assert_eq!(
                a.kind.as_str(),
                e.get("kind").string(),
                "{id} {at} event kind"
            );
            let members: Vec<_> = e
                .get("members")
                .array()
                .iter()
                .map(|v| v.string())
                .collect();
            assert_eq!(
                a.members.iter().map(String::as_str).collect::<Vec<_>>(),
                members,
                "{id} {at} members"
            );
            close(
                a.score,
                e.get("score").optional_number(),
                &format!("{id} {at} event score"),
            );
            close(
                a.eta_s,
                e.get("eta_s").optional_number(),
                &format!("{id} {at} event eta_s"),
            );
        }
    }
}

#[test]
fn params_reject_table() {
    for row in load("params-reject-table.json").get("rows").array() {
        let name = row.get("name").string();
        let value = row.get("value").number();
        let mut p = Params::default();
        let accepted = p.set(name, ParamValue::Number(value)).is_ok();
        assert_eq!(
            accepted,
            row.get("accepted").boolean(),
            "params-reject-table {name}={value}"
        );
        if !accepted {
            assert_eq!(
                p,
                Params::default(),
                "rejected {name} must not change Params"
            );
        }
    }
}
