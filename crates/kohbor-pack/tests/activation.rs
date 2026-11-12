mod common;
use common::*;
use kohbor_pack::{
    error::RejectReason,
    limits::Limits,
    store::{ActivationPolicy as Policy, ActivationState, Status, Store},
};

#[test]
fn policy_and_barriers_control_activation() {
    let temp = TempDir::new();
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    let installed = store.install(&request(path, &bytes, "request_1")).unwrap();
    assert_eq!(installed.status, Status::Installed);
    let id = installed.record.unwrap().install_id;
    let idle = ActivationState::default();
    assert_eq!(
        store
            .activate("request_1", &id, Policy::Never, &idle)
            .unwrap()
            .status,
        Status::Installed
    );
    assert_eq!(
        store
            .activate("request_1", &id, Policy::NowIfIdle, &idle)
            .unwrap()
            .status,
        Status::Active
    );
    assert_eq!(store.active().unwrap().install_id, id);
    assert_eq!(
        store
            .activate("request_1", &id, Policy::NowIfIdle, &idle)
            .unwrap()
            .status,
        Status::Active
    );
    let unknown = store
        .activate("request_1", "ins_9999999999", Policy::NowIfIdle, &idle)
        .unwrap();
    assert_eq!(unknown.reject, Some(RejectReason::UnknownInstall));
}

#[test]
fn deferred_activation_resumes_after_match_and_lease() {
    for leased in [false, true] {
        let temp = TempDir::new();
        let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
        let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
        let old = store
            .install(&request(path, &bytes, "request_1"))
            .unwrap()
            .record
            .unwrap();
        store
            .activate(
                "request_1",
                &old.install_id,
                Policy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap();
        let (path, bytes) = store_package(&temp.0, "test-pack", 2, false);
        let new = store
            .install(&request(path, &bytes, "request_2"))
            .unwrap()
            .record
            .unwrap();
        let blocked = ActivationState {
            match_active: !leased,
            leased_install_ids: if leased {
                vec![old.install_id.clone()]
            } else {
                vec![]
            },
        };
        let result = store
            .activate("request_2", &new.install_id, Policy::AfterMatch, &blocked)
            .unwrap();
        assert_eq!(result.status, Status::Deferred);
        assert_eq!(result.reason.as_deref(), Some("ACTIVATION_DEFERRED"));
        assert_eq!(result.record, Some(new.clone()));
        assert_eq!(store.active().unwrap().install_id, old.install_id);
        assert!(store.resolve("first_blood", None).is_ok());
        assert_eq!(store.pending_activations(), vec![new.install_id.clone()]);
        assert!(store.resume_deferred(&blocked).unwrap().is_empty());
        let resumed = store.resume_deferred(&ActivationState::default()).unwrap();
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].status, Status::Active);
        assert_eq!(store.active().unwrap().install_id, new.install_id);
        assert!(store.pending_activations().is_empty());
    }
}

#[test]
fn activation_io_error_preserves_pointer_and_retry_succeeds() {
    use kohbor_pack::store::{Hook, Injection, Step};
    use std::{cell::Cell, rc::Rc};
    struct Switch(Rc<Cell<Option<Step>>>);
    impl Hook for Switch {
        fn before(&mut self, step: Step) -> Injection {
            if self.0.get() == Some(step) {
                Injection::IoError
            } else {
                Injection::Proceed
            }
        }
    }
    for step in [Step::WriteActiveTemp, Step::RenameActive] {
        let temp = TempDir::new();
        let root = temp.0.join("store");
        let injection = Rc::new(Cell::new(None));
        let mut store = Store::open_with_hook(
            &root,
            catalog(),
            Limits::default(),
            Switch(injection.clone()),
        )
        .unwrap();
        let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
        let old = store
            .install(&request(path, &bytes, "request_1"))
            .unwrap()
            .record
            .unwrap();
        store
            .activate(
                "request_1",
                &old.install_id,
                Policy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap();
        let (path, bytes) = store_package(&temp.0, "test-pack", 2, false);
        let new = store
            .install(&request(path, &bytes, "request_2"))
            .unwrap()
            .record
            .unwrap();
        injection.set(Some(step));
        let failure = store
            .activate(
                "request_2",
                &new.install_id,
                Policy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap();
        assert_eq!(failure.status, Status::ActivationError);
        // DISK_PRESSURE for activation I/O errors is a lead proposal, not an owner decision.
        assert_eq!(failure.reason.as_deref(), Some("DISK_PRESSURE"));
        assert_eq!(store.active().unwrap().install_id, old.install_id);
        assert_eq!(store.records().len(), 2);
        assert!(!root.join("active.json.tmp").exists());
        injection.set(None);
        assert_eq!(
            store
                .activate(
                    "request_2",
                    &new.install_id,
                    Policy::NowIfIdle,
                    &ActivationState::default()
                )
                .unwrap()
                .status,
            Status::Active
        );
        assert_eq!(store.active().unwrap().install_id, new.install_id);
    }
}

#[test]
fn pending_activations_resume_oldest_first() {
    let temp = TempDir::new();
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    let blocked = ActivationState {
        match_active: true,
        ..ActivationState::default()
    };
    let mut ids = Vec::new();
    for revision in 1..=3 {
        let (path, bytes) = store_package(&temp.0, "test-pack", revision, false);
        let request_id = format!("request_{revision}");
        let record = store
            .install(&request(path, &bytes, &request_id))
            .unwrap()
            .record
            .unwrap();
        let result = store
            .activate(&request_id, &record.install_id, Policy::NowIfIdle, &blocked)
            .unwrap();
        assert_eq!(
            result.status,
            Status::Deferred,
            "revision {revision}: deferred"
        );
        ids.push(record.install_id);
    }
    assert_eq!(store.pending_activations(), ids, "pending order");
    assert!(
        store.resume_deferred(&blocked).unwrap().is_empty(),
        "blocked resume changes nothing"
    );
    assert_eq!(store.pending_activations(), ids, "blocked pending order");
    // The documented oldest-first rule is read as applying each pending request in
    // order; the newer activation supersedes the older only after the latter runs.
    let results = store.resume_deferred(&ActivationState::default()).unwrap();
    assert_eq!(
        results.len(),
        ids.len(),
        "one result per changed pending request"
    );
    for (index, result) in results.iter().enumerate() {
        assert_eq!(
            result.status,
            Status::Active,
            "pending index {index}: status"
        );
        assert_eq!(
            result.record.as_ref().unwrap().install_id,
            ids[index],
            "pending index {index}: order"
        );
    }
    assert_eq!(
        store.active().unwrap().install_id,
        ids[2],
        "newest activation wins after older ones"
    );
    assert!(
        store.pending_activations().is_empty(),
        "completed pending queue"
    );
}

#[test]
fn now_if_idle_defers_for_match_and_lease() {
    for leased in [false, true] {
        let temp = TempDir::new();
        let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
        let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
        let old = store
            .install(&request(path, &bytes, "request_1"))
            .unwrap()
            .record
            .unwrap();
        store
            .activate(
                "request_1",
                &old.install_id,
                Policy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap();
        let (path, bytes) = store_package(&temp.0, "test-pack", 2, false);
        let new = store
            .install(&request(path, &bytes, "request_2"))
            .unwrap()
            .record
            .unwrap();
        let blocked = ActivationState {
            match_active: !leased,
            leased_install_ids: if leased {
                vec![old.install_id.clone()]
            } else {
                vec![]
            },
        };
        let result = store
            .activate("request_2", &new.install_id, Policy::NowIfIdle, &blocked)
            .unwrap();
        assert_eq!(result.status, Status::Deferred, "leased={leased}: deferred");
        assert_eq!(
            result.reason.as_deref(),
            Some("ACTIVATION_DEFERRED"),
            "leased={leased}: reason"
        );
        assert_eq!(result.record, Some(new.clone()), "leased={leased}: record");
        assert_eq!(
            store.active().unwrap().install_id,
            old.install_id,
            "leased={leased}: pointer"
        );
        assert_eq!(
            store.resolve("first_blood", None).unwrap().install_id,
            old.install_id,
            "leased={leased}: resolution"
        );
        assert_eq!(
            store.pending_activations(),
            vec![new.install_id],
            "leased={leased}: pending"
        );
    }
}

#[test]
fn after_match_idle_returns_active_with_no_pending() {
    let temp = TempDir::new();
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    let id = store
        .install(&request(path, &bytes, "request_1"))
        .unwrap()
        .record
        .unwrap()
        .install_id;
    let result = store
        .activate(
            "request_1",
            &id,
            Policy::AfterMatch,
            &ActivationState::default(),
        )
        .unwrap();
    assert_eq!(result.status, Status::Active, "after_match idle status");
    assert_eq!(
        store.active().unwrap().install_id,
        id,
        "after_match idle pointer"
    );
    assert!(
        store.pending_activations().is_empty(),
        "after_match idle pending"
    );
}
