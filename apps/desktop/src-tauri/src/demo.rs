use kohbor_events::{Event, EventId};

/// Reserved generation encoded in audio-history instance ids for DEMO rows.
/// It cannot collide with a live intake generation.
pub const DEMO_GENERATION: u64 = u64::MAX;

pub fn demo_enabled(env_value: Option<&str>) -> bool {
    env_value == Some("1")
}

pub fn demo_event(id: &str, tier: Option<u8>) -> Result<Event, &'static str> {
    let event_id = match id {
        "multi_kill" => EventId::MultiKill,
        "streak" => EventId::Streak,
        "first_blood" => EventId::FirstBlood,
        "hero_kill" => EventId::HeroKill,
        "hero_death" => EventId::HeroDeath,
        "level_milestone" => EventId::LevelMilestone,
        "health_low" => EventId::HealthLow,
        "mana_low" => EventId::ManaLow,
        "match_ended" => EventId::MatchEnded,
        "match_started" => EventId::MatchStarted,
        "hero_respawn" => EventId::HeroRespawn,
        "health_danger" => EventId::HealthDanger,
        _ => return Err("DEMO_EVENT_UNAVAILABLE"),
    };

    match (event_id.tier_range(), tier) {
        (Some((min, max)), Some(value)) if (min..=max).contains(&value) => {}
        (None, None) => {}
        _ => return Err("DEMO_EVENT_UNAVAILABLE"),
    }

    Ok(Event {
        id: event_id,
        tier,
        at_ms: 0,
    })
}

/// Demo-only data directory override: needs `KOHBOR_DEMO=1` and an absolute path.
#[cfg(any(feature = "demo", debug_assertions))]
pub fn data_dir_override(demo: Option<&str>, dir: Option<&str>) -> Option<std::path::PathBuf> {
    if !demo_enabled(demo) {
        return None;
    }
    let path = std::path::PathBuf::from(dir?);
    (path.is_absolute() && !path.as_os_str().is_empty()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_one_enables_demo() {
        assert!(demo_enabled(Some("1")));
        for value in [None, Some(""), Some("0"), Some("true"), Some("1 ")] {
            assert!(!demo_enabled(value));
        }
    }

    #[test]
    fn accepts_catalog_events_and_valid_tiers() {
        for id in [
            "first_blood",
            "hero_kill",
            "hero_death",
            "level_milestone",
            "health_low",
            "mana_low",
            "match_ended",
            "match_started",
            "hero_respawn",
            "health_danger",
        ] {
            assert!(demo_event(id, None).is_ok(), "{id}");
        }
        for (id, tier) in [
            ("multi_kill", 2),
            ("multi_kill", 5),
            ("streak", 3),
            ("streak", 10),
        ] {
            assert!(demo_event(id, Some(tier)).is_ok(), "{id} {tier}");
        }
    }

    #[test]
    fn all_active_catalog_ids_are_representable() {
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../../../contracts/event-catalog.json")).unwrap();
        for event in catalog["events"].as_array().unwrap() {
            if event["status"] != "active" {
                continue;
            }
            let id = event["id"].as_str().unwrap();
            let tier = event
                .get("tier_range")
                .and_then(|range| range["min"].as_u64())
                .map(|value| value as u8);
            let detectable = [
                "multi_kill",
                "streak",
                "first_blood",
                "hero_kill",
                "hero_death",
                "level_milestone",
                "health_low",
                "mana_low",
                "match_ended",
                "match_started",
                "hero_respawn",
                "health_danger",
            ]
            .contains(&id);
            // Only detector EventIds are representable; the rest must be refused.
            assert_eq!(
                demo_event(id, tier).is_ok(),
                detectable,
                "catalog event {id}"
            );
        }
    }

    #[test]
    fn rejects_unknown_and_invalid_tiers() {
        for (id, tier) in [
            ("unknown", None),
            ("hero_kill", Some(2)),
            ("multi_kill", Some(1)),
            ("multi_kill", Some(6)),
            ("streak", Some(2)),
            ("streak", Some(11)),
        ] {
            assert!(demo_event(id, tier).is_err(), "{id} {tier:?}");
        }
    }

    #[test]
    fn demo_never_enters_live_intake_or_ownership_admission() {
        let intake = include_str!("intake.rs");
        let admission = include_str!("../../../../crates/kohbor-gsi/src/admission.rs");
        for source in [intake, admission] {
            assert!(!source.to_ascii_lowercase().contains("demo"));
            assert!(!source.contains("KOHBOR_DEMO"));
            assert!(!source.contains("#[cfg(feature"));
        }

        assert!(intake.contains(
            "Outcome::Admitted(admitted) if admitted.ownership == Ownership::Verified => {"
        ));
        assert!(intake.contains("Ownership::Verified => None,"));
        assert!(admission.contains("Some(owner) if owner != id => Ownership::Mismatch,"));
        assert!(admission.contains("_ => Ownership::Verified,"));
    }

    #[test]
    fn demo_trigger_is_compiled_only_for_the_approved_build_modes() {
        let commands = include_str!("commands.rs");
        assert!(commands.contains(
            "#[cfg(any(feature = \"demo\", debug_assertions))]\n#[tauri::command(async)]\npub(crate) fn demo_trigger"
        ));
    }

    #[test]
    fn demo_marker_payload_matches_what_the_renderer_accepts() {
        // The overlay renderer accepts a demo marker only when `source` is "demo" (main.tsx).
        let commands = include_str!("commands.rs");
        assert!(
            commands.contains("\"overlay://demo\"")
                && commands.contains("serde_json::json!({\"source\": \"demo\"})")
        );
        let renderer = include_str!("../../src/main.tsx");
        assert!(renderer.contains(".source === \"demo\""));
    }

    #[test]
    fn data_dir_override_needs_demo_and_an_absolute_path() {
        let abs = if cfg!(windows) {
            r"C:\kohbor-demo"
        } else {
            "/tmp/kohbor-demo"
        };
        assert!(data_dir_override(Some("1"), Some(abs)).is_some());
        assert!(data_dir_override(None, Some(abs)).is_none());
        assert!(data_dir_override(Some("0"), Some(abs)).is_none());
        assert!(data_dir_override(Some("1"), Some("relative/dir")).is_none());
        assert!(data_dir_override(Some("1"), Some("")).is_none());
        assert!(data_dir_override(Some("1"), None).is_none());
    }
}
