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

pub const OVERRIDE_DIR: &str = "lab";
pub const OVERRIDE_FILE: &str = "profile.override.json";

fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT: symlinks and junctions.
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

fn refuse_input(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// The only place Lab ever writes: `<data_dir>/lab/profile.override.json`. The file name is
/// fixed; nothing a renderer sends can change it. `data_dir` must be absolute and free of
/// `.` and `..` components.
pub fn override_path(data_dir: &Path) -> io::Result<PathBuf> {
    use std::path::Component;
    if !data_dir.is_absolute() {
        return Err(refuse_input("the data directory must be absolute"));
    }
    if data_dir
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(refuse_input("the data directory must not contain . or .."));
    }
    Ok(data_dir.join(OVERRIDE_DIR).join(OVERRIDE_FILE))
}

/// Write the profile to the fixed override location through a temporary file and a rename,
/// so a crash never leaves a half-written override. Refuses links (symlinks, junctions) at
/// the directory, the target and the temporary file, and any result that resolves outside
/// `data_dir`. Returns the path written.
pub fn save_under(data_dir: &Path, effective: &Effective) -> io::Result<PathBuf> {
    let target = override_path(data_dir)?;
    let dir = data_dir.join(OVERRIDE_DIR);
    if !std::fs::metadata(data_dir)?.is_dir() {
        return Err(refuse_input("the data directory is not a directory"));
    }
    match std::fs::symlink_metadata(&dir) {
        Ok(meta) if is_link(&meta) || !meta.is_dir() => {
            return Err(refuse_input("the lab directory is not a plain directory"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::create_dir(&dir)?,
        Err(error) => return Err(error),
    }
    if !std::fs::canonicalize(&dir)?.starts_with(std::fs::canonicalize(data_dir)?) {
        return Err(refuse_input(
            "the lab directory resolves outside the data directory",
        ));
    }
    let temporary = dir.join(format!("{OVERRIDE_FILE}.tmp"));
    for path in [&target, &temporary] {
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            if is_link(&meta) || !meta.is_file() {
                return Err(refuse_input("the override file is not a plain file"));
            }
        }
    }
    std::fs::write(&temporary, effective.export_json())?;
    std::fs::rename(&temporary, &target)?;
    Ok(target)
}
