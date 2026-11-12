//! Filesystem helpers shared by the package reader and store.

use std::fs::Metadata;
use std::io;
use std::path::Path;

/// True when metadata describes a symbolic link or a Windows reparse point.
pub(crate) fn is_link_or_reparse_point(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// `Metadata::is_dir` does not classify every Windows junction as a directory.
pub(crate) fn is_directory_reparse_point(metadata: &Metadata) -> bool {
    if metadata.is_dir() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0010;
        metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Windows treats trailing dots/spaces and DOS device stems specially. Reject
/// those names before they can be interpreted differently by the host OS.
pub(crate) fn has_unsafe_windows_segment(path: &str) -> bool {
    path.split('/').any(|segment| {
        if segment.len() > 255
            || segment.ends_with('.')
            || segment.ends_with(' ')
            || segment.contains(':')
        {
            return true;
        }

        let stem = segment.split('.').next().unwrap_or_default();
        let stem = stem.to_ascii_uppercase();
        matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit()
                && stem.as_bytes()[3] != b'0')
    })
}

/// Canonical key used for Windows-safe duplicate detection.
pub(crate) fn windows_path_key(path: &str) -> String {
    path.split('/')
        .map(|segment| segment.trim_end_matches('.').to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}

/// Flush directory metadata to stable storage where the platform supports it.
/// A filesystem that explicitly reports this operation as unsupported is the
/// only case that may be ignored; other open or flush errors are returned.
pub(crate) fn flush_dir(path: &Path) -> io::Result<()> {
    let result = platform_flush_dir(path);
    match result {
        Err(error) if error.kind() == io::ErrorKind::Unsupported => {
            // Some filesystems do not implement directory handles or directory
            // fsync. File contents are still synced separately by the caller.
            Ok(())
        }
        result => result,
    }
}

#[cfg(windows)]
fn platform_flush_dir(path: &Path) -> io::Result<()> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?
        .sync_all()
}

#[cfg(unix)]
fn platform_flush_dir(path: &Path) -> io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(any(unix, windows)))]
fn platform_flush_dir(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "directory flush is unsupported on this platform",
    ))
}
