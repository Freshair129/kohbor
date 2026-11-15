//! Production must ignore any override file (KOHBOR-LAB.md section 4).
//!
//! Source scan in the style of the demo gate: the override reader exists in exactly one
//! file, that module is compiled only with the `lab` feature or debug assertions, and
//! the public resolver has a separate release variant that never mentions it.

use std::fs;
use std::path::PathBuf;

fn src(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(name);
    fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

const GATE: &str = r#"#[cfg(any(feature = "lab", debug_assertions))]"#;
const NOT_GATE: &str = r#"#[cfg(not(any(feature = "lab", debug_assertions)))]"#;

#[test]
fn override_env_names_live_only_in_the_gated_module() {
    for name in ["lib.rs", "profile.rs"] {
        let text = src(name);
        assert!(
            !text.contains("KOHBOR_LAB"),
            "{name} must not name the override variables"
        );
        assert!(
            !text.contains("read_to_string"),
            "{name} must not read files"
        );
        assert!(
            !text.contains("std::env"),
            "{name} must not read the environment"
        );
    }
    let gated = src("dev_override.rs");
    assert!(gated.contains("KOHBOR_LAB"));
}

#[test]
fn override_module_is_compiled_out_of_release() {
    let lib = src("lib.rs");
    let declaration = format!("{GATE}\npub mod dev_override;");
    assert!(
        lib.contains(&declaration),
        "dev_override must carry the gate cfg"
    );
    assert_eq!(lib.matches("mod dev_override").count(), 1);
}

#[test]
fn release_resolver_never_calls_the_override_reader() {
    let lib = src("lib.rs");
    let release_start = lib
        .find(NOT_GATE)
        .expect("a release variant of resolve_effective");
    let release_fn: String = lib[release_start..]
        .lines()
        .take(6)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(release_fn.contains("fn resolve_effective"), "{release_fn}");
    assert!(!release_fn.contains("dev_override"), "{release_fn}");
    let dev_start = lib
        .find(&format!("{GATE}\npub fn resolve_effective"))
        .expect("dev variant");
    let dev_fn: String = lib[dev_start..]
        .lines()
        .take(6)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(dev_fn.contains("dev_override::"), "{dev_fn}");
}

#[cfg(any(feature = "lab", debug_assertions))]
mod behaviour {
    use kohbor_lab::dev_override::resolve_from;
    use kohbor_lab::Source;
    use std::path::{Path, PathBuf};

    const GOOD: &str = r#"{"profile_version":"1.0","taxonomy_version":"1.1","events":{"health_low":{"params":{"threshold_percent":25}}}}"#;

    fn env_with(
        lab: Option<&'static str>,
        path: Option<&'static str>,
    ) -> impl Fn(&str) -> Option<String> {
        move |name| match name {
            "KOHBOR_LAB" => lab.map(str::to_owned),
            "KOHBOR_LAB_PROFILE" => path.map(str::to_owned),
            _ => None,
        }
    }

    fn reader(text: &'static str) -> impl Fn(&Path) -> std::io::Result<String> {
        move |_| Ok(text.to_owned())
    }

    fn absolute() -> &'static str {
        if cfg!(windows) {
            r"C:\lab\profile.json"
        } else {
            "/lab/profile.json"
        }
    }

    #[test]
    fn needs_both_the_flag_and_an_absolute_path() {
        let off = resolve_from(&env_with(None, Some(absolute())), &reader(GOOD));
        assert_eq!(off.source, Source::Embedded);
        assert!(off.rejected.is_none());
        let wrong = resolve_from(&env_with(Some("yes"), Some(absolute())), &reader(GOOD));
        assert_eq!(wrong.source, Source::Embedded);
        let relative = resolve_from(&env_with(Some("1"), Some("profile.json")), &reader(GOOD));
        assert_eq!(relative.source, Source::Embedded);
        assert_eq!(relative.rejected.unwrap().code, "PATH_NOT_ABSOLUTE");
        let missing = resolve_from(&env_with(Some("1"), None), &reader(GOOD));
        assert_eq!(missing.source, Source::Embedded);
        assert!(missing.rejected.is_none(), "an unset path is not an error");
    }

    #[test]
    fn valid_override_wins_and_invalid_falls_back_whole() {
        let good = resolve_from(&env_with(Some("1"), Some(absolute())), &reader(GOOD));
        assert_eq!(good.source, Source::Override(PathBuf::from(absolute())));
        assert_eq!(good.effective.config().health_low_threshold_percent, 25);
        assert!(good.rejected.is_none());

        let bad = resolve_from(
            &env_with(Some("1"), Some(absolute())),
            &reader(
                r#"{"profile_version":"1.0","taxonomy_version":"1.1","events":{"health_low":{"params":{"threshold_percent":99}}}}"#,
            ),
        );
        assert_eq!(bad.source, Source::Embedded);
        assert_eq!(bad.effective.config(), kohbor_events::Config::default());
        assert_eq!(bad.rejected.unwrap().code, "PARAM_OUT_OF_RANGE");

        let unreadable = resolve_from(&env_with(Some("1"), Some(absolute())), &|_: &Path| {
            Err(std::io::Error::other("denied"))
        });
        assert_eq!(unreadable.source, Source::Embedded);
        assert_eq!(unreadable.rejected.unwrap().code, "READ_FAILED");

        let huge = resolve_from(&env_with(Some("1"), Some(absolute())), &|_: &Path| {
            Ok(" ".repeat(70_000))
        });
        assert_eq!(huge.rejected.unwrap().code, "TOO_LARGE");
    }
}
