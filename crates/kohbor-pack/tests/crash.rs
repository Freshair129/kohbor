mod common;
use common::*;
use kohbor_pack::{
    limits::Limits,
    store::{ActivationPolicy, ActivationState, Hook, Injection, Step, Store, StoreError},
};
use std::{cell::RefCell, rc::Rc};

struct Recorder {
    stop: Option<Step>,
    injection: Injection,
    seen: Rc<RefCell<Vec<Step>>>,
}
impl Hook for Recorder {
    fn before(&mut self, step: Step) -> Injection {
        self.seen.borrow_mut().push(step);
        if self.stop == Some(step) {
            self.injection
        } else {
            Injection::Proceed
        }
    }
}

#[test]
fn uninterrupted_transactions_visit_every_step_in_order() {
    let temp = TempDir::new();
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let hook = Recorder {
        stop: None,
        injection: Injection::Proceed,
        seen: seen.clone(),
    };
    let mut store =
        Store::open_with_hook(&temp.0.join("store"), catalog(), Limits::default(), hook).unwrap();
    let record = store
        .install(&request(path, &bytes, "request_1"))
        .unwrap()
        .record
        .unwrap();
    store
        .activate(
            "request_1",
            &record.install_id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    assert_eq!(*seen.borrow(), Step::ALL);
}

fn crash_case(step: Step, has_old: bool) {
    let temp = TempDir::new();
    let root = temp.0.join("crashed");
    let reference = temp.0.join("reference");
    let (old_path, old_bytes) = store_package(&temp.0, "test-pack", 1, false);
    let (path, bytes) = store_package(&temp.0, "test-pack", 2, false);
    let old_req = request(old_path, &old_bytes, "request_old");
    let req = request(path, &bytes, "request_new");
    let idle = ActivationState::default();
    for dir in [&root, &reference] {
        let mut s = Store::open(dir, catalog(), Limits::default()).unwrap();
        if has_old {
            let old = s.install(&old_req).unwrap().record.unwrap();
            s.activate(
                &old_req.request_id,
                &old.install_id,
                ActivationPolicy::NowIfIdle,
                &idle,
            )
            .unwrap();
        }
    }
    let mut baseline = Store::open(&reference, catalog(), Limits::default()).unwrap();
    let previous_records = baseline.records();
    let previous_active = baseline.active();
    let expected = baseline.install(&req).unwrap();
    let expected_record = expected.record.clone().unwrap();
    let activation_step = matches!(step, Step::WriteActiveTemp | Step::RenameActive);
    if activation_step {
        baseline
            .activate(
                &req.request_id,
                &expected_record.install_id,
                ActivationPolicy::NowIfIdle,
                &idle,
            )
            .unwrap();
    }
    drop(baseline);
    let hook = Recorder {
        stop: Some(step),
        injection: Injection::Crash,
        seen: Rc::new(RefCell::new(Vec::new())),
    };
    let mut store = Store::open_with_hook(&root, catalog(), Limits::default(), hook).unwrap();
    let outcome = store.install(&req);
    if activation_step {
        let record = outcome.unwrap().record.unwrap();
        assert_eq!(
            store.activate(
                &req.request_id,
                &record.install_id,
                ActivationPolicy::NowIfIdle,
                &idle
            ),
            Err(StoreError::Interrupted(step)),
            "{step:?}, old={has_old}: activation interruption"
        );
    } else {
        assert_eq!(
            outcome,
            Err(StoreError::Interrupted(step)),
            "{step:?}, old={has_old}: install interruption"
        );
    }
    drop(store);
    let mut recovered = Store::open(&root, catalog(), Limits::default()).unwrap();
    // Hooks fire BEFORE each step: RenameRegistry has not committed. Only the
    // two activation crash points occur after the registry commit.
    let mut expected_records = previous_records.clone();
    if activation_step {
        expected_records.push(expected_record.clone());
    }
    assert_eq!(
        recovered.records(),
        expected_records,
        "{step:?}, old={has_old}: committed records"
    );
    assert_eq!(
        recovered.active(),
        previous_active,
        "{step:?}, old={has_old}: pointer must remain exactly old (no mixed pointer)"
    );
    if has_old {
        assert_eq!(
            recovered.resolve("first_blood", None).unwrap().install_id,
            previous_records[0].install_id,
            "{step:?}: old pair remains resolvable"
        );
    }
    assert!(
        tree(&root.join("staging")).is_empty(),
        "{step:?}, old={has_old}: staging cleanup"
    );
    let revisions = root.join("revisions");
    if revisions.exists() {
        for entry in std::fs::read_dir(revisions).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                assert!(
                    expected_records
                        .iter()
                        .any(|r| r.install_id == entry.file_name().to_string_lossy()),
                    "{step:?}: unlisted revision {:?}",
                    entry.path()
                );
            }
        }
    }
    assert!(
        !tree(&root)
            .iter()
            .any(|p| p.to_string_lossy().ends_with(".tmp")),
        "{step:?}: temporary files"
    );
    for name in ["registry.json", "active.json"] {
        let p = root.join(name);
        if p.exists() {
            let _: serde_json::Value = serde_json::from_slice(&std::fs::read(p).unwrap())
                .unwrap_or_else(|e| panic!("{step:?}: {name} invalid: {e}"));
        }
    }
    let retry = recovered.install(&req).unwrap();
    assert_eq!(
        retry.to_json(),
        expected.to_json(),
        "{step:?}: retry result"
    );
    if activation_step {
        recovered
            .activate(
                &req.request_id,
                &expected_record.install_id,
                ActivationPolicy::NowIfIdle,
                &idle,
            )
            .unwrap();
    }
    assert_eq!(
        tree(&root),
        tree(&reference),
        "{step:?}, old={has_old}: final tree"
    );
    let (third_path, third_bytes) = store_package(&temp.0, "third-pack", 1, false);
    let next = recovered
        .install(&request(third_path, &third_bytes, "request_next"))
        .unwrap()
        .record
        .unwrap();
    let expected_next = if has_old {
        "ins_0000000003"
    } else {
        "ins_0000000002"
    };
    assert_eq!(
        next.install_id, expected_next,
        "{step:?}, old={has_old}: counter skips or reuses id"
    );
}

macro_rules! crash_tests {
    ($(($first:ident, $old:ident, $step:ident)),+ $(,)?) => { $(
        #[test] fn $first() { crash_case(Step::$step, false); }
        #[test] fn $old() { crash_case(Step::$step, true); }
    )+ };
}
crash_tests!(
    (crash_copy_first, crash_copy_old, CopySource),
    (crash_validate_first, crash_validate_old, ValidateContainer),
    (crash_extract_first, crash_extract_old, ExtractToStaging),
    (crash_media_first, crash_media_old, VerifyMedia),
    (crash_publish_first, crash_publish_old, PublishRevision),
    (
        crash_registry_temp_first,
        crash_registry_temp_old,
        WriteRegistryTemp
    ),
    (
        crash_registry_rename_first,
        crash_registry_rename_old,
        RenameRegistry
    ),
    (
        crash_active_temp_first,
        crash_active_temp_old,
        WriteActiveTemp
    ),
    (
        crash_active_rename_first,
        crash_active_rename_old,
        RenameActive
    ),
);

#[test]
fn corrupt_metadata_is_not_reset() {
    for name in ["registry.json", "active.json"] {
        let temp = TempDir::new();
        let root = temp.0.join("store");
        drop(Store::open(&root, catalog(), Limits::default()).unwrap());
        std::fs::write(root.join(name), b"{").unwrap();
        assert!(matches!(
            Store::open(&root, catalog(), Limits::default()),
            Err(StoreError::Corrupt(_))
        ));
    }
}

#[test]
fn recovery_removes_orphans_and_temporary_files() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    drop(Store::open(&root, catalog(), Limits::default()).unwrap());
    for dir in ["staging/garbage", "revisions/ins_9999999999", "nested"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "staging/garbage/data",
        "revisions/ins_9999999999/data",
        "active.json.tmp",
        "nested/leftover.tmp",
    ] {
        std::fs::write(root.join(file), b"garbage").unwrap();
    }
    drop(Store::open(&root, catalog(), Limits::default()).unwrap());
    assert!(tree(&root.join("staging")).is_empty(), "staging garbage");
    assert!(
        !root.join("revisions/ins_9999999999").exists(),
        "unlisted revision"
    );
    // S2-4 F3: only the store's own transaction temporaries are removed. A foreign
    // `*.tmp` file elsewhere under the root is not ours and must survive open.
    assert!(
        !root.join("active.json.tmp").exists(),
        "owned temporary file"
    );
    assert!(
        root.join("nested/leftover.tmp").exists(),
        "foreign temporary file is preserved"
    );
}

#[test]
fn missing_active_revision_is_corrupt() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let id = store
        .install(&request(path, &bytes, "request_1"))
        .unwrap()
        .record
        .unwrap()
        .install_id;
    store
        .activate(
            "request_1",
            &id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    drop(store);
    std::fs::remove_dir_all(root.join("revisions").join(id)).unwrap();
    assert!(
        matches!(
            Store::open(&root, catalog(), Limits::default()),
            Err(StoreError::Corrupt(_))
        ),
        "active revision directory missing"
    );
}

#[test]
fn active_pointer_to_unknown_install_is_corrupt() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let id = store
        .install(&request(path, &bytes, "request_1"))
        .unwrap()
        .record
        .unwrap()
        .install_id;
    store
        .activate(
            "request_1",
            &id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    drop(store);
    let active_path = root.join("active.json");
    let text = std::fs::read_to_string(&active_path).unwrap();
    assert!(
        text.contains(&id),
        "active pointer must name its installation"
    );
    std::fs::write(active_path, text.replace(&id, "ins_9999999999")).unwrap();
    assert!(
        matches!(
            Store::open(&root, catalog(), Limits::default()),
            Err(StoreError::Corrupt(_))
        ),
        "unknown active installation"
    );
}

#[test]
fn install_io_errors_leave_no_committed_record() {
    for step in Step::ALL.into_iter().take(7) {
        let temp = TempDir::new();
        let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
        let root = temp.0.join("store");
        let hook = Recorder {
            stop: Some(step),
            injection: Injection::IoError,
            seen: Rc::new(RefCell::new(Vec::new())),
        };
        let mut store = Store::open_with_hook(&root, catalog(), Limits::default(), hook).unwrap();
        let req = request(path, &bytes, "request_1");
        let result = store.install(&req).unwrap();
        assert_eq!(
            result.status,
            kohbor_pack::store::Status::Rejected,
            "{step:?}: status"
        );
        assert_eq!(
            result.reason.as_deref(),
            Some("DISK_PRESSURE"),
            "{step:?}: reason"
        );
        assert_eq!(result.reject, None, "{step:?}: rejection detail");
        assert!(store.records().is_empty(), "{step:?}: records");
        assert!(tree(&root.join("staging")).is_empty(), "{step:?}: staging");
        assert!(
            !tree(&root)
                .iter()
                .any(|p| p.to_string_lossy().ends_with(".tmp")),
            "{step:?}: temporary files"
        );
        assert!(
            tree(&root.join("revisions")).is_empty(),
            "{step:?}: uncommitted revision"
        );
        drop(store);
        let mut retry = Store::open(&root, catalog(), Limits::default()).unwrap();
        assert_eq!(
            retry.install(&req).unwrap().record.unwrap().install_id,
            "ins_0000000001",
            "{step:?}: failed install must not consume id"
        );
    }
}
