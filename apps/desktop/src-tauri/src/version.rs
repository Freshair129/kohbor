use serde_json::{json, Value};

/// Control window title without a version; also the `tauri.conf.json` fallback title.
pub const CONTROL_TITLE_BASE: &str = "Kohbor — Command Deck";

/// Control window title shown in the title bar: base title, then the version (including any
/// pre-release string), then ` · LAB` for builds that carry the Lab.
pub fn control_window_title(version: &str, lab: bool) -> String {
    if lab {
        format!("{CONTROL_TITLE_BASE} {version} · LAB")
    } else {
        format!("{CONTROL_TITLE_BASE} {version}")
    }
}

/// True for the control window of ANY version (single-instance focus matches on the prefix).
pub fn is_control_window_title(title: &str) -> bool {
    title == CONTROL_TITLE_BASE
        || title
            .strip_prefix(CONTROL_TITLE_BASE)
            .is_some_and(|rest| rest.starts_with(' '))
}

pub struct AppVersion {
    pub version: &'static str,
    pub git_commit: Option<String>,
    pub build_date: Option<String>,
    pub profile: &'static str,
    pub demo: bool,
}

impl AppVersion {
    pub fn current() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            git_commit: non_empty(env!("KOHBOR_GIT_SHA")),
            build_date: non_empty(env!("KOHBOR_BUILD_DATE")),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            demo: cfg!(feature = "demo"),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "version": self.version,
            "git_commit": self.git_commit.as_deref(),
            "build_date": self.build_date.as_deref(),
            "profile": self.profile,
            "demo": self.demo,
        })
    }
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{control_window_title, is_control_window_title, non_empty, AppVersion};

    #[test]
    fn control_title_carries_the_version_and_the_lab_marker() {
        assert_eq!(
            control_window_title("0.3.0-dev.14", false),
            "Kohbor — Command Deck 0.3.0-dev.14"
        );
        assert_eq!(
            control_window_title("0.3.0-dev.14", true),
            "Kohbor — Command Deck 0.3.0-dev.14 · LAB"
        );
    }

    #[test]
    fn control_title_recognition_covers_every_version_and_nothing_else() {
        assert!(is_control_window_title("Kohbor — Command Deck"));
        assert!(is_control_window_title(
            "Kohbor — Command Deck 0.2.0-mvp2.1"
        ));
        assert!(is_control_window_title(&control_window_title(
            "0.3.0-dev.15",
            true
        )));
        for other in [
            "",
            "Kohbor",
            "Kohbor — Command DeckX",
            "Kohbor Lab",
            "kohbor — command deck 1",
        ] {
            assert!(!is_control_window_title(other), "{other}");
        }
    }

    #[test]
    fn empty_build_metadata_is_unknown() {
        assert_eq!(non_empty(""), None);
        assert_eq!(non_empty("abc123def"), Some("abc123def".to_owned()));
    }

    #[test]
    fn json_shape_keeps_unknown_metadata_null() {
        let version = AppVersion {
            version: "0.2.0-mvp2.1",
            git_commit: None,
            build_date: None,
            profile: "debug",
            demo: false,
        };

        assert_eq!(
            version.to_json(),
            serde_json::json!({
                "version": "0.2.0-mvp2.1",
                "git_commit": null,
                "build_date": null,
                "profile": "debug",
                "demo": false,
            })
        );
    }
}
