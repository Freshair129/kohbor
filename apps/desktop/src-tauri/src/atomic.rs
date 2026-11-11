//! Atomic file replacement: write a sibling temporary file, flush it to disk,
//! then rename over the target (GSI-INTAKE-SPEC §2.2). A failed write leaves the
//! previous file intact and removes the temporary file.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn tmp_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

pub fn write(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_path(target);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_content_and_leaves_no_temporary_file() {
        let dir = std::env::temp_dir().join(format!("kohbor-atomic-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("owned.cfg");
        write(&target, b"one").unwrap();
        write(&target, b"two").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"two");
        assert!(!tmp_path(&target).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_write_keeps_previous_file_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("kohbor-atomic-fail-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // The target is a directory, so the final rename cannot succeed.
        let target = dir.join("occupied");
        fs::create_dir_all(target.join("child")).unwrap();
        assert!(write(&target, b"x").is_err());
        assert!(target.join("child").is_dir());
        assert!(!tmp_path(&target).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
