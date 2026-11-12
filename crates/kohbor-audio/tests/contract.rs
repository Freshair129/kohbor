mod common;
use kohbor_audio::{AudioClass, Outcome, PlaybackId};
#[test]
fn outcome_values_match_schema() {
    let mut expected = common::enum_values(&common::schema("audio-outcome.schema.json"), "outcome");
    let variants = [
        Outcome::Admitted,
        Outcome::Preempted,
        Outcome::Unmapped,
        Outcome::InvalidMedia,
        Outcome::Inert,
        Outcome::NotVoiced,
        Outcome::DetectorUnavailable,
        Outcome::StaleSession,
        Outcome::SupersededRevision,
        Outcome::CriticalActive,
        Outcome::ChannelBusy,
        Outcome::LowerRankSameTick,
        Outcome::Cooldown,
        Outcome::Muted,
    ];
    let mut actual: Vec<_> = variants.iter().map(|v| v.as_str().to_string()).collect();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}
#[test]
fn class_values_match_schema() {
    let mut expected =
        common::enum_values(&common::schema("audio-outcome.schema.json"), "audio_class");
    let mut actual: Vec<_> = [
        AudioClass::Critical,
        AudioClass::Correction,
        AudioClass::Notice,
        AudioClass::Ordinary,
        AudioClass::Voice,
    ]
    .iter()
    .map(|v| v.as_str().to_string())
    .collect();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}
#[test]
fn admitted_decisions_have_contract_fields_for_each_class() {
    for class in [
        AudioClass::Critical,
        AudioClass::Correction,
        AudioClass::Notice,
        AudioClass::Ordinary,
        AudioClass::Voice,
    ] {
        let mut owner = common::owner();
        let decision = owner.request(0, common::request(class)).unwrap();
        assert_eq!(decision.outcome, Outcome::Admitted, "{class:?}");
        if class == AudioClass::Voice {
            assert_eq!(decision.package_revision, None, "{class:?}");
            assert_eq!(decision.event_type, None, "{class:?}");
        } else {
            assert_eq!(decision.package_revision, Some(1), "{class:?}");
            let event = decision
                .event_type
                .as_deref()
                .expect("non-voice event_type");
            let mut bytes = event.bytes();
            assert!(
                (1..=80).contains(&event.len())
                    && bytes.next().is_some_and(|b| b.is_ascii_lowercase())
                    && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "{class:?}: {event}"
            );
        }
    }
}
// Ten digits is a minimum width; larger u64 identities must still match the schema pattern.
#[test]
fn playback_ids_match_pattern() {
    for id in [1, 9_999_999_999, u64::MAX] {
        let s = PlaybackId(id).to_contract_id();
        assert!(
            (8..=64).contains(&s.len())
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "id {id}: {s}"
        );
        if id == 1 {
            assert_eq!(s, "pb_0000000001");
        }
    }
}
#[test]
fn production_source_has_no_forbidden_dependencies_or_operations() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let mut in_dependencies = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap().trim();
        if line.starts_with('[') {
            in_dependencies = line.contains("dependencies");
            assert!(
                !in_dependencies || line == "[dependencies]",
                "dependency section: {line}"
            );
        } else if in_dependencies {
            assert!(line.is_empty(), "dependency: {line}");
        }
    }
    for entry in std::fs::read_dir(root.join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        // Strip comments, including nested block comments, before checking tokens.
        let mut code = String::new();
        let mut chars = src.chars().peekable();
        let mut depth = 0usize;
        let mut line = false;
        while let Some(c) = chars.next() {
            if line {
                if c == '\n' {
                    line = false;
                    code.push('\n');
                }
                continue;
            }
            if depth > 0 {
                if c == '/' && chars.peek() == Some(&'*') {
                    chars.next();
                    depth += 1;
                } else if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    depth -= 1;
                }
                continue;
            }
            if c == '/' && chars.peek() == Some(&'/') {
                chars.next();
                line = true;
            } else if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                depth = 1;
            } else {
                code.push(c);
            }
        }
        let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        for forbidden in [
            "std::fs",
            "std::net",
            "std::thread",
            "std::time::Instant",
            "SystemTime",
            "std::process",
            "unsafe",
            "Mutex",
            "RwLock",
            "mpsc",
            "channel(",
            "sync_channel(",
        ] {
            assert!(
                !compact.contains(forbidden),
                "{} contains {forbidden}",
                path.display()
            );
        }
    }
}
