use crate::account_host::{
    AccountHost, AccountStorage, ConfirmedDeleteStatus, DeleteResolution, DeleteStatusBroker,
};
use kohbor_account::{BrokerError, PersistedSession, RecoveryMarker, SecretStore};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "kohbor-host-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn open(&self) -> AccountHost {
        AccountHost::open(&self.0).expect("provision private child")
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn session(id: &str) -> PersistedSession {
    PersistedSession {
        account_id: id.into(),
        refresh_token: format!("token-{id}"),
        device_key: format!("key-{id}"),
        friends: vec!["friend".into()],
        pairing_ids: vec!["pair".into()],
        private_item_labels: vec!["private".into()],
    }
}
fn pending() -> RecoveryMarker {
    RecoveryMarker::DeletePending {
        account_id: "acct_a".into(),
    }
}
fn prepare(h: &AccountHost) {
    h.with_operation(|g| {
        g.write_session(&session("acct_a")).unwrap();
        g.write_recovery_marker(&pending()).unwrap();
    })
    .unwrap();
}
fn hidden(h: &AccountHost) {
    h.with_operation(|g| {
        assert_eq!(g.load_recovery_marker(), Ok(pending()));
        assert!(matches!(g.load_session(), Ok(None) | Err(_)));
        assert!(g.write_session(&session("acct_b")).is_err());
        assert!(g.write_friends("acct_b", &["intruder".into()]).is_err());
    })
    .unwrap();
}
struct Status {
    account: &'static str,
    resolution: DeleteResolution,
    unavailable: bool,
}
impl DeleteStatusBroker for Status {
    fn confirmed_delete_status(&mut self, id: &str) -> Result<ConfirmedDeleteStatus, BrokerError> {
        assert_eq!(id, "acct_a");
        if self.unavailable {
            return Err(BrokerError::Unavailable);
        }
        Ok(ConfirmedDeleteStatus {
            account_id: self.account.into(),
            resolution: self.resolution,
        })
    }
}
#[test]
fn provisions_private_fixed_child_under_broad_parent() {
    let root = Root::new();
    let h = root.open();
    let child = root.0.join("account");
    assert!(child.is_dir());
    assert!(
        AccountStorage::open(&child).is_ok(),
        "protected private DACL required"
    );
    h.with_operation(|g| g.write_session(&session("acct_a")).unwrap())
        .unwrap();
    drop(h);
    assert_eq!(
        root.open().with_operation(|g| g.load_session()).unwrap(),
        Ok(Some(session("acct_a")))
    );
}
#[test]
fn rejects_existing_broad_child_without_acl_repair() {
    let root = Root::new();
    let child = root.0.join("account");
    std::fs::create_dir(&child).unwrap();
    std::fs::write(child.join("sentinel"), b"unchanged").unwrap();
    assert!(
        AccountStorage::open(&child).is_err(),
        "fixture must be broad"
    );
    assert!(AccountHost::open(&root.0).is_err());
    assert!(AccountStorage::open(&child).is_err(), "must not repair ACL");
    assert_eq!(std::fs::read(child.join("sentinel")).unwrap(), b"unchanged");
}
#[test]
fn fresh_operation_cannot_use_earlier_pending_permit() {
    let root = Root::new();
    let h = root.open();
    prepare(&h);
    h.with_operation(|g| {
        assert!(g.write_recovery_marker(&RecoveryMarker::Clear).is_err());
        assert!(g
            .write_recovery_marker(&RecoveryMarker::WipePending {
                account_id: "acct_a".into()
            })
            .is_err());
    })
    .unwrap();
    hidden(&h);
}
#[test]
fn unknown_unavailable_and_wrong_account_preserve_hidden_data() {
    let root = Root::new();
    let h = root.open();
    prepare(&h);
    for mut s in [
        Status {
            account: "acct_a",
            resolution: DeleteResolution::Unknown,
            unavailable: false,
        },
        Status {
            account: "acct_a",
            resolution: DeleteResolution::Committed,
            unavailable: true,
        },
        Status {
            account: "acct_b",
            resolution: DeleteResolution::Committed,
            unavailable: false,
        },
        Status {
            account: "acct_b",
            resolution: DeleteResolution::NotCommitted,
            unavailable: false,
        },
    ] {
        let _ = h.reconcile_pending_delete(&mut s);
        hidden(&h);
    }
    let mut s = Status {
        account: "acct_a",
        resolution: DeleteResolution::NotCommitted,
        unavailable: false,
    };
    h.reconcile_pending_delete(&mut s).unwrap();
    assert_eq!(
        h.with_operation(|g| g.load_session()).unwrap(),
        Ok(Some(session("acct_a")))
    );
}
#[test]
fn same_account_committed_status_wipes_across_operations() {
    let root = Root::new();
    let h = root.open();
    prepare(&h);
    let mut s = Status {
        account: "acct_a",
        resolution: DeleteResolution::Committed,
        unavailable: false,
    };
    h.reconcile_pending_delete(&mut s).unwrap();
    h.with_operation(|g| {
        assert_eq!(g.load_recovery_marker(), Ok(RecoveryMarker::Clear));
        assert_eq!(g.load_session(), Ok(None));
    })
    .unwrap();
}
#[test]
fn broker_verification_holds_storage_lock() {
    struct Checking<'a>(&'a AccountHost);
    impl DeleteStatusBroker for Checking<'_> {
        fn confirmed_delete_status(
            &mut self,
            id: &str,
        ) -> Result<ConfirmedDeleteStatus, BrokerError> {
            assert!(
                self.0.with_operation(|_| ()).is_err(),
                "guard must remain held during broker verification"
            );
            Ok(ConfirmedDeleteStatus {
                account_id: id.into(),
                resolution: DeleteResolution::Unknown,
            })
        }
    }
    let root = Root::new();
    let h = root.open();
    prepare(&h);
    h.reconcile_pending_delete(&mut Checking(&h)).unwrap();
    hidden(&h);
}
