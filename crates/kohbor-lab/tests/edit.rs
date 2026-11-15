use kohbor_lab::embedded;
use serde_json::json;

#[test]
fn batch_edit_validates_as_a_whole() {
    let base = embedded();
    // Raising the trigger above the default re-arm alone is invalid...
    assert_eq!(
        base.with_edits(&[("health_low", "threshold_percent", json!(40))])
            .unwrap_err()
            .code,
        "REARM_NOT_ABOVE_TRIGGER"
    );
    // ...but moving both together is a valid profile.
    let edited = base
        .with_edits(&[
            ("health_low", "threshold_percent", json!(40)),
            ("health_low", "rearm_percent", json!(55)),
        ])
        .unwrap();
    assert_eq!(edited.config().health_low_threshold_percent, 40);
    assert_eq!(
        edited.overrides(),
        vec![
            "health_low.threshold_percent".to_string(),
            "health_low.rearm_percent".to_string()
        ]
    );
    // The original is untouched.
    assert!(base.overrides().is_empty());
}

#[test]
fn edits_are_bounded_and_cannot_invent_parameters() {
    let base = embedded();
    assert_eq!(
        base.with_edits(&[("health_danger", "window_ms", json!(499))])
            .unwrap_err()
            .code,
        "PARAM_OUT_OF_RANGE"
    );
    assert_eq!(
        base.with_edits(&[("health_danger", "enabled", json!(false))])
            .unwrap_err()
            .code,
        "UNKNOWN_PARAM"
    );
    assert_eq!(
        base.with_edits(&[("risk_alert", "global_cooldown_ms", json!(30000))])
            .unwrap_err()
            .code,
        "UNKNOWN_EVENT"
    );
}

#[test]
fn reset_returns_a_value_to_the_embedded_default() {
    let edited = embedded()
        .with_edits(&[("health_danger", "cooldown_ms", json!(60000))])
        .unwrap();
    assert_eq!(edited.overrides().len(), 1);
    let back = edited
        .with_edits(&[("health_danger", "cooldown_ms", json!(20000))])
        .unwrap();
    assert!(back.overrides().is_empty());
}

#[cfg(any(feature = "lab", debug_assertions))]
mod writes {
    use super::*;
    use kohbor_lab::dev_override::{
        override_path, resolve_from, save_under, OVERRIDE_DIR, OVERRIDE_FILE,
    };
    use std::path::{Path, PathBuf};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kohbor-lab-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn edited() -> kohbor_lab::Effective {
        embedded()
            .with_edits(&[("health_danger", "cooldown_ms", json!(60000))])
            .unwrap()
    }

    /// Directory link without elevated rights: a junction on Windows, a symlink elsewhere.
    fn link_dir(link: &Path, target: &Path) -> bool {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .is_ok_and(|o| o.status.success())
        }
        #[cfg(not(windows))]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }

    fn link_file(link: &Path, target: &Path) -> bool {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link).is_ok()
        }
        #[cfg(not(windows))]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }

    #[test]
    fn writes_only_the_fixed_file_under_the_data_dir_and_reloads() {
        let dir = temp_dir("fixed");
        let written = save_under(&dir, &edited()).unwrap();
        assert_eq!(written, dir.join(OVERRIDE_DIR).join(OVERRIDE_FILE));
        assert_eq!(written, override_path(&dir).unwrap());
        assert!(!dir
            .join(OVERRIDE_DIR)
            .join(format!("{OVERRIDE_FILE}.tmp"))
            .exists());
        let path_text = written.to_string_lossy().into_owned();
        let resolved = resolve_from(
            &|name| match name {
                "KOHBOR_LAB" => Some("1".to_owned()),
                "KOHBOR_LAB_PROFILE" => Some(path_text.clone()),
                _ => None,
            },
            &|p| std::fs::read_to_string(p),
        );
        assert!(resolved.rejected.is_none());
        assert_eq!(resolved.effective.config().danger_cooldown_ms, 60000);
        // A second save replaces the file.
        save_under(&dir, &embedded()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_relative_and_dotdot_and_missing_data_dirs() {
        let kind = |p: &Path| save_under(p, &edited()).unwrap_err().kind();
        assert_eq!(
            kind(Path::new("relative")),
            std::io::ErrorKind::InvalidInput
        );
        let dir = temp_dir("dotdot");
        let sneaky = dir.join("sub").join("..").join("x");
        assert_eq!(kind(&sneaky), std::io::ErrorKind::InvalidInput);
        assert!(save_under(&dir.join("missing"), &edited()).is_err());
        assert!(!dir.join("missing").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_a_lab_directory_that_links_outside() {
        let dir = temp_dir("dirlink");
        let outside = temp_dir("dirlink-outside");
        if !link_dir(&dir.join(OVERRIDE_DIR), &outside) {
            eprintln!("skipped: cannot create a directory link here");
            return;
        }
        let error = save_under(&dir, &edited()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(
            std::fs::read_dir(&outside).unwrap().next().is_none(),
            "nothing written outside"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn refuses_an_override_file_that_links_elsewhere() {
        let dir = temp_dir("filelink");
        let outside = temp_dir("filelink-outside");
        std::fs::create_dir(dir.join(OVERRIDE_DIR)).unwrap();
        let victim = outside.join("victim.json");
        std::fs::write(&victim, "keep").unwrap();
        if !link_file(&dir.join(OVERRIDE_DIR).join(OVERRIDE_FILE), &victim) {
            eprintln!("skipped: cannot create a file link here");
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::remove_dir_all(&outside);
            return;
        }
        let error = save_under(&dir, &edited()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
