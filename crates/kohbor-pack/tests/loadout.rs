mod common;

use common::*;
use kohbor_pack::{
    limits::Limits,
    store::{
        ActivationPolicy, ActivationState, Hook, Injection, LoadoutDesired, LoadoutStatus,
        MAX_EQUIPPED_PACKS, Step, Store, StoreError,
    },
};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

fn install<H: Hook>(
    store: &mut Store<H>,
    temp: &TempDir,
    package_id: &str,
    revision: u64,
    request_id: &str,
) -> String {
    let (path, bytes) = store_package(temp.0.as_path(), package_id, revision, false);
    store
        .install(&request(path, &bytes, request_id))
        .unwrap()
        .record
        .unwrap()
        .install_id
}

fn desired(ids: &[&str]) -> LoadoutDesired {
    LoadoutDesired {
        install_ids: ids.iter().map(|id| (*id).to_owned()).collect(),
        event_locks: Default::default(),
        match_consistency: false,
    }
}

#[test]
fn active_json_migrates_in_memory_without_writing_loadout() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let install_id = install(&mut store, &temp, "voice.main", 1, "request_0001");
    store
        .activate(
            "activate_0001",
            &install_id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    let active_before = std::fs::read(root.join("active.json")).unwrap();
    std::fs::remove_file(root.join("loadout.json")).unwrap();

    let migrated = Store::open(&root, catalog(), Limits::default()).unwrap();
    assert_eq!(migrated.loadout().equipped.len(), 1);
    assert_eq!(migrated.loadout().equipped[0].install_id, install_id);
    assert_eq!(migrated.loadout().generation, 1);
    assert!(migrated.loadout().event_locks.is_empty());
    assert!(!migrated.loadout().match_consistency);
    assert!(
        !root.join("loadout.json").exists(),
        "migration writes on demand only"
    );
    assert_eq!(
        std::fs::read(root.join("active.json")).unwrap(),
        active_before
    );
}

#[test]
fn two_pack_commit_round_trips_and_mirrors_slot_one() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let first = install(&mut store, &temp, "voice.main", 1, "request_0001");
    let second = install(&mut store, &temp, "voice.special", 1, "request_0002");
    let mut wanted = desired(&[&first, &second]);
    wanted
        .event_locks
        .insert("hero_death".to_owned(), "voice.main".to_owned());
    wanted.match_consistency = true;
    let result = store
        .set_loadout(
            "loadout_0001",
            &wanted,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    assert_eq!(result.status, LoadoutStatus::Active);
    assert_eq!(
        result
            .loadout
            .equipped
            .iter()
            .map(|entry| entry.install_id.as_str())
            .collect::<Vec<_>>(),
        [first.as_str(), second.as_str()]
    );
    assert_eq!(result.loadout.event_locks, wanted.event_locks);
    assert!(result.loadout.match_consistency);
    let active: Value =
        serde_json::from_slice(&std::fs::read(root.join("active.json")).unwrap()).unwrap();
    assert_eq!(active["install_id"], first);

    let reopened = Store::open(&root, catalog(), Limits::default()).unwrap();
    assert_eq!(reopened.loadout(), &result.loadout);
    assert_eq!(reopened.active().unwrap().install_id, first);
}

#[test]
fn invalid_loadouts_have_distinct_reasons_and_leave_state_unchanged() {
    let temp = TempDir::new();
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    let mut ids = Vec::new();
    for (index, package_id, revision) in [
        (1, "voice.a", 1),
        (2, "voice.a", 2),
        (3, "voice.b", 1),
        (4, "voice.c", 1),
        (5, "voice.d", 1),
        (6, "voice.e", 1),
    ] {
        ids.push(install(
            &mut store,
            &temp,
            package_id,
            revision,
            &format!("request_{index:04}"),
        ));
    }
    let before = store.loadout().clone();
    let cases = [
        (
            desired(&[&ids[0], &ids[2], &ids[3], &ids[4], &ids[5]]),
            "LOADOUT_TOO_MANY",
        ),
        (desired(&[&ids[0], &ids[0]]), "LOADOUT_DUPLICATE_INSTALL"),
        (desired(&[&ids[0], &ids[1]]), "LOADOUT_DUPLICATE_PACKAGE"),
        (desired(&["ins_9999999999"]), "UNKNOWN_INSTALL"),
    ];
    for (index, (request, expected)) in cases.into_iter().enumerate() {
        let result = store
            .set_loadout(
                &format!("badloadout_{index:03}"),
                &request,
                ActivationPolicy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap();
        assert_eq!(result.status, LoadoutStatus::Rejected);
        assert_eq!(result.reason.as_deref(), Some(expected));
        assert_eq!(store.loadout(), &before);
    }
    assert_eq!(MAX_EQUIPPED_PACKS, 4);
}

#[test]
fn pending_loadout_is_last_request_wins_and_resumes_after_match_or_lease() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let first = install(&mut store, &temp, "voice.main", 1, "request_0001");
    let second = install(&mut store, &temp, "voice.special", 1, "request_0002");
    store
        .set_loadout(
            "loadout_0001",
            &desired(&[&first]),
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    let match_active = ActivationState {
        match_active: true,
        ..ActivationState::default()
    };
    let deferred = store
        .set_loadout(
            "loadout_0002",
            &desired(&[&second]),
            ActivationPolicy::NowIfIdle,
            &match_active,
        )
        .unwrap();
    assert_eq!(deferred.status, LoadoutStatus::Deferred);
    assert_eq!(
        deferred.pending.as_ref().unwrap().equipped[0].install_id,
        second
    );
    assert!(store.resume_deferred(&match_active).unwrap().is_empty());

    let replacement = store
        .set_loadout(
            "loadout_0003",
            &desired(&[&first, &second]),
            ActivationPolicy::NowIfIdle,
            &match_active,
        )
        .unwrap();
    assert_eq!(replacement.status, LoadoutStatus::Deferred);
    assert_eq!(store.pending_loadout_request_id(), Some("loadout_0003"));
    let resumed = store.resume_deferred(&ActivationState::default()).unwrap();
    assert!(resumed.is_empty());
    assert_eq!(store.loadout().equipped.len(), 2);
    assert_eq!(store.loadout().equipped[0].install_id, first);
    assert_eq!(store.loadout().equipped[1].install_id, second);
    assert!(store.pending_loadout().is_none());
    assert!(!root.join("pending_loadout.json").exists());

    let leased = ActivationState {
        leased_install_ids: vec![second.clone()],
        ..ActivationState::default()
    };
    let deferred = store
        .set_loadout(
            "loadout_0004",
            &desired(&[&first]),
            ActivationPolicy::NowIfIdle,
            &leased,
        )
        .unwrap();
    assert_eq!(
        deferred.status,
        LoadoutStatus::Deferred,
        "a leased secondary pack protects the whole loadout"
    );
    store
        .resume_pending_loadout(&ActivationState::default())
        .unwrap();
    assert_eq!(store.loadout().equipped.len(), 1);
}

#[derive(Clone)]
struct CrashSwitch(Rc<Cell<Option<Step>>>);
impl Hook for CrashSwitch {
    fn before(&mut self, step: Step) -> Injection {
        if self.0.get() == Some(step) {
            Injection::Crash
        } else {
            Injection::Proceed
        }
    }
}

#[test]
fn crashes_around_loadout_and_mirror_renames_reopen_to_one_consistent_state() {
    for point in [
        Step::RenameLoadout,
        Step::AfterRenameLoadout,
        Step::RenameActive,
        Step::AfterRenameActive,
    ] {
        let temp = TempDir::new();
        let root = temp.0.join("store");
        let crash = Rc::new(Cell::new(None));
        let mut store = Store::open_with_hook(
            &root,
            catalog(),
            Limits::default(),
            CrashSwitch(crash.clone()),
        )
        .unwrap();
        let first = install(&mut store, &temp, "voice.main", 1, "request_0001");
        let second = install(&mut store, &temp, "voice.special", 1, "request_0002");
        store
            .set_loadout(
                "loadout_0001",
                &desired(&[&first]),
                ActivationPolicy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap();
        crash.set(Some(point));
        assert!(
            matches!(
                store.set_loadout(
                    "loadout_0002",
                    &desired(&[&second]),
                    ActivationPolicy::NowIfIdle,
                    &ActivationState::default()
                ),
                Err(StoreError::Interrupted(_))
            ),
            "{point:?}"
        );
        drop(store);
        crash.set(None);
        let reopened = Store::open(&root, catalog(), Limits::default()).unwrap();
        let primary = reopened
            .loadout()
            .equipped
            .first()
            .map(|item| item.install_id.as_str());
        assert!(
            primary == Some(first.as_str()) || primary == Some(second.as_str()),
            "{point:?}: {primary:?}"
        );
        let active: Value =
            serde_json::from_slice(&std::fs::read(root.join("active.json")).unwrap()).unwrap();
        assert_eq!(
            active["install_id"].as_str(),
            primary,
            "mirror repaired after {point:?}"
        );
    }
}

#[test]
fn corrupt_loadout_is_not_reset_and_a_disagreeing_mirror_is_repaired() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let first = install(&mut store, &temp, "voice.main", 1, "request_0001");
    let second = install(&mut store, &temp, "voice.special", 1, "request_0002");
    store
        .set_loadout(
            "loadout_0001",
            &desired(&[&first, &second]),
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    let mut mirror: Value =
        serde_json::from_slice(&std::fs::read(root.join("active.json")).unwrap()).unwrap();
    mirror["package_id"] = json!("voice.wrong");
    std::fs::write(
        root.join("active.json"),
        serde_json::to_vec(&mirror).unwrap(),
    )
    .unwrap();
    let repaired = Store::open(&root, catalog(), Limits::default()).unwrap();
    assert_eq!(repaired.active().unwrap().install_id, first);
    let mirror: Value =
        serde_json::from_slice(&std::fs::read(root.join("active.json")).unwrap()).unwrap();
    assert_eq!(mirror["package_id"], "voice.main");

    std::fs::write(root.join("loadout.json"), b"{broken").unwrap();
    assert!(matches!(
        Store::open(&root, catalog(), Limits::default()),
        Err(StoreError::Corrupt(_))
    ));
}
