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
    // No path set is the normal case (no override file), not an error.
    let Some(raw) = env(PATH) else {
        return Resolved::embedded();
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

/// The override a previous Lab save left at the fixed location, if any. `None` when there is no
/// file; a refused file (link, not a plain file, too large, invalid profile) leaves the embedded
/// default in force and says why. Used when `KOHBOR_LAB_PROFILE` selects nothing.
pub fn load_saved(data_dir: &Path) -> Option<Resolved> {
    let path = override_path(data_dir).ok()?;
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(_) => return None,
    };
    if is_link(&meta) || !meta.is_file() {
        return Some(refuse(
            "READ_FAILED",
            "the saved override is not a plain file",
        ));
    }
    if meta.len() as usize > MAX_BYTES {
        return Some(refuse(
            "TOO_LARGE",
            "the saved override is larger than 64 KiB",
        ));
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Some(refuse(
            "READ_FAILED",
            "the saved override could not be read",
        ));
    };
    Some(match merge_override(&text) {
        Ok(effective) => Resolved {
            effective: effective.with_source(Source::Override(path.clone())),
            source: Source::Override(path),
            rejected: None,
        },
        Err(error) => Resolved {
            rejected: Some(error),
            ..Resolved::embedded()
        },
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
    write_fixed(data_dir, OVERRIDE_FILE, &effective.export_json())
}

pub const MAPPING_FILE: &str = "pack-mapping.override.json";

/// Write the dev pack-mapping draft (already validated by `mapping::parse`) to its fixed file
/// next to the profile override, with the same confinement rules.
pub fn save_mapping_under(data_dir: &Path, text: &str) -> io::Result<PathBuf> {
    write_fixed(data_dir, MAPPING_FILE, text)
}

/// The saved mapping draft text, if any (plain file only, 64 KiB at most).
pub fn load_mapping(data_dir: &Path) -> io::Result<Option<String>> {
    let path = override_path(data_dir)?.with_file_name(MAPPING_FILE);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if is_link(&meta) || !meta.is_file() || meta.len() as usize > MAX_BYTES {
        return Err(refuse_input(
            "the mapping draft is not an acceptable plain file",
        ));
    }
    std::fs::read_to_string(&path).map(Some)
}

fn write_fixed(data_dir: &Path, file: &str, text: &str) -> io::Result<PathBuf> {
    let target = override_path(data_dir)?.with_file_name(file);
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
    let temporary = dir.join(format!("{file}.tmp"));
    for path in [&target, &temporary] {
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            if is_link(&meta) || !meta.is_file() {
                return Err(refuse_input("the override file is not a plain file"));
            }
        }
    }
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, &target)?;
    Ok(target)
}

pub const ASSET_DIR: &str = "assets";
pub const ASSET_FILE: &str = "dota-overlay-reference.jpg";
const MAX_REFERENCE_BYTES: u64 = 8 * 1024 * 1024;

/// The owner's Dota reference screenshot, loaded at runtime from the fixed dev location
/// `<data_dir>/lab/assets/dota-overlay-reference.jpg`. It is never part of the repository or
/// the installer. `Ok(None)` when it is not there; an error when it is a link, not a plain
/// file, larger than 8 MiB, or not a JPEG.
pub fn read_reference_image(data_dir: &Path) -> io::Result<Option<Vec<u8>>> {
    let file = override_path(data_dir)?
        .with_file_name(ASSET_DIR)
        .join(ASSET_FILE);
    let folders = [
        data_dir.join(OVERRIDE_DIR),
        data_dir.join(OVERRIDE_DIR).join(ASSET_DIR),
    ];
    for folder in &folders {
        match std::fs::symlink_metadata(folder) {
            Ok(meta) if is_link(&meta) || !meta.is_dir() => {
                return Err(refuse_input("the asset folder is not a plain directory"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    let meta = match std::fs::symlink_metadata(&file) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if is_link(&meta) || !meta.is_file() || meta.len() > MAX_REFERENCE_BYTES {
        return Err(refuse_input(
            "the reference image is not an acceptable plain file",
        ));
    }
    let bytes = std::fs::read(&file)?;
    if !bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err(refuse_input("the reference image is not a JPEG"));
    }
    Ok(Some(bytes))
}
