mod common;

use common::{catalog, request, store_package, TempDir};
use kohbor_pack::store::{Store, StoreError};
use kohbor_pack::Limits;
use std::fs;
use std::path::Path;
use std::process::Command;

fn create_junction(link: &Path, target: &Path) -> bool {
    let command = format!("mklink /J {} {}", link.display(), target.display());
    match Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
    {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            println!(
                "junction test skipped: command {command:?}; mklink /J failed ({}): {}{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            false
        }
        Err(error) => {
            println!("junction test skipped: could not run mklink /J: {error}");
            false
        }
    }
}

#[cfg(windows)]
#[test]
fn recovery_removes_orphan_junction_without_touching_target_and_open_rejects_listed_junction() {
    let temp = TempDir::new();
    let target = temp.0.join("sibling-target");
    fs::create_dir(&target).unwrap();
    let sentinel = target.join("sentinel");
    fs::write(&sentinel, b"must survive").unwrap();

    let root = temp.0.join("store");
    let orphan_revision = root.join("revisions").join("ins_9999999999");
    fs::create_dir_all(&orphan_revision).unwrap();
    let orphan_link = orphan_revision.join("linked");
    if !create_junction(&orphan_link, &target) {
        return;
    }
    Store::open(&root, catalog(), Limits::default()).unwrap();
    assert!(sentinel.is_file());
    assert_eq!(fs::read(&sentinel).unwrap(), b"must survive");
    assert!(!orphan_revision.exists());

    let (package, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let record = store
        .install(&request(package, &bytes, "request_listed"))
        .unwrap()
        .record
        .unwrap();
    drop(store);

    let listed_link = root
        .join("revisions")
        .join(&record.install_id)
        .join("unreferenced-link");
    if !create_junction(&listed_link, &target) {
        return;
    }
    let reopened = Store::open(&root, catalog(), Limits::default());
    assert!(matches!(reopened, Err(StoreError::Corrupt(_))));
    assert!(sentinel.is_file());
    assert_eq!(fs::read(&sentinel).unwrap(), b"must survive");
}

#[cfg(not(windows))]
#[test]
fn junction_checks_are_windows_only() {
    // This target is intentionally a no-op outside Windows.
}
