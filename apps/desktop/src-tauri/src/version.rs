use serde_json::{json, Value};

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
    use super::{non_empty, AppVersion};

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
