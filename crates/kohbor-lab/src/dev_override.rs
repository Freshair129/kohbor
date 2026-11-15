//! Dev-only override reader. Compiled only with feature `lab` or debug assertions.
//!
//! Needs `KOHBOR_LAB=1` and `KOHBOR_LAB_PROFILE=<absolute path>`. Any problem leaves the
//! embedded default in force and reports why; a profile is never applied partially.

use crate::{merge_override, Effective, ProfileError, Resolved, Source};
use std::io;
use std::path::{Path, PathBuf};

const FLAG: &str = "KOHBOR_LAB";
const PATH: &str = "KOHBOR_LAB_PROFILE";
const MAX_BYTES: usize = 64 * 1024;

fn refuse(code: &'static str, detail: &str) -> Resolved {
    Resolved {
        rejected: Some(ProfileError {
            code,
            detail: detail.to_owned(),
        }),
        ..Resolved::embedded()
    }
}

pub fn resolve_from(
    env: &dyn Fn(&str) -> Option<String>,
    read: &dyn Fn(&Path) -> io::Result<String>,
) -> Resolved {
    if env(FLAG).as_deref() != Some("1") {
        return Resolved::embedded();
    }
    let Some(raw) = env(PATH) else {
        return refuse("PATH_MISSING", "KOHBOR_LAB_PROFILE is not set");
    };
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return refuse(
            "PATH_NOT_ABSOLUTE",
            "KOHBOR_LAB_PROFILE must be an absolute path",
        );
    }
    let Ok(text) = read(&path) else {
        return refuse("READ_FAILED", "the profile file could not be read");
    };
    if text.len() > MAX_BYTES {
        return refuse("TOO_LARGE", "the profile file is larger than 64 KiB");
    }
    match merge_override(&text) {
        Ok(effective) => Resolved {
            effective: effective.with_source(Source::Override(path.clone())),
            source: Source::Override(path),
            rejected: None,
        },
        Err(error) => Resolved {
            rejected: Some(error),
            ..Resolved::embedded()
        },
    }
}

pub fn resolve_from_env() -> Resolved {
    resolve_from(&|name| std::env::var(name).ok(), &|path| {
        std::fs::read_to_string(path)
    })
}

/// Write the profile to `path` (absolute) through a temporary file and a rename, so a crash
/// never leaves a half-written override behind.
pub fn save(path: &Path, effective: &Effective) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the profile path must be absolute",
        ));
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, effective.export_json())?;
    std::fs::rename(&temporary, path)
}
