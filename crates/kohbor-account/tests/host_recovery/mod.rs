use crate::account_host::RecoveryMarkerFile;
use crate::support::fakes::{new_trace, FakeBroker, FakeClock, FakeProvider, FakeSecretStore};
use kohbor_account::{
    AuthMaterial, CallbackOutcome, Client, OperationOutcome, PersistedSession, RecoveryMarker,
    SecretStore, ServiceConfig, StoreError,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "kohbor-host-recovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("unique test root");
        Self(path)
    }
    fn file(&self) -> PathBuf {
        self.0.join("recovery-marker.bin")
    }
    fn marker(&self) -> RecoveryMarkerFile {
        RecoveryMarkerFile::new(&self.0)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn deletion() -> RecoveryMarker {
    RecoveryMarker::DeletePending {
        account_id: "acct_a".into(),
    }
}
fn wipe() -> RecoveryMarker {
    RecoveryMarker::WipePending {
        account_id: "acct_a".into(),
    }
}

#[test]
fn missing_marker_is_clear() {
    let root = Root::new();
    assert_eq!(root.marker().load(), Ok(RecoveryMarker::Clear));
}
#[test]
fn marker_roundtrips_across_fresh_handles_and_clear() {
    let root = Root::new();
    for marker in [deletion(), wipe(), RecoveryMarker::Clear] {
        root.marker().commit(&marker).expect("commit marker");
        assert_eq!(root.marker().load(), Ok(marker.clone()));
        let raw = std::fs::read(root.file()).expect("committed marker file");
        assert!(
            !raw.windows(b"acct_a".len()).any(|part| part == b"acct_a"),
            "account identifier must not be stored in plaintext"
        );
    }
}
#[test]
fn malformed_and_unreadable_marker_fail_closed() {
    let root = Root::new();
    std::fs::write(root.file(), b"not a recovery record").unwrap();
    assert_eq!(root.marker().load(), Err(StoreError::Unreadable));
    std::fs::remove_file(root.file()).unwrap();
    std::fs::create_dir(root.file()).unwrap();
    assert_eq!(root.marker().load(), Err(StoreError::Unreadable));
}
#[cfg(windows)]
#[test]
fn denied_destination_replacement_preserves_previous_record() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = Root::new();
    root.marker()
        .commit(&deletion())
        .expect("initial valid marker");
    // Allow reading and writing but deny FILE_SHARE_DELETE, which Windows
    // requires to replace an existing destination. Keep the handle alive.
    let old_bytes = std::fs::read(root.file()).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(root.file())
        .unwrap();
    assert_eq!(root.marker().commit(&wipe()), Err(StoreError::Unwritable));
    assert_eq!(root.marker().load(), Ok(deletion()));
    assert_eq!(std::fs::read(root.file()).unwrap(), old_bytes);
    drop(held);
    root.marker()
        .commit(&wipe())
        .expect("replacement succeeds once delete sharing is released");
    assert_eq!(root.marker().load(), Ok(wipe()));
}

struct FileStore {
    inner: FakeSecretStore,
    marker: RecoveryMarkerFile,
}
impl SecretStore for FileStore {
    fn load_recovery_marker(&self) -> Result<RecoveryMarker, StoreError> {
        self.marker.load()
    }
    fn write_recovery_marker(&mut self, marker: &RecoveryMarker) -> Result<(), StoreError> {
        self.marker.commit(marker)
    }
    fn load_session(&self) -> Result<Option<PersistedSession>, StoreError> {
        self.inner.load_session()
    }
    fn write_session(&mut self, s: &PersistedSession) -> Result<(), StoreError> {
        self.inner.write_session(s)
    }
    fn write_friends(&mut self, id: &str, friends: &[String]) -> Result<(), StoreError> {
        self.inner.write_friends(id, friends)
    }
    fn wipe_account(&mut self, id: &str) -> Result<(), StoreError> {
        self.inner.wipe_account(id)
    }
}
fn material(b: bool) -> AuthMaterial {
    let (state, nonce) = if b {
        ("state-b", "nonce-b")
    } else {
        ("state-a", "nonce-a")
    };
    AuthMaterial {
        state: state.into(),
        nonce: nonce.into(),
        code_verifier: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_".into(),
    }
}
#[test]
fn ambiguous_delete_survives_fresh_client_and_file_handle_without_restore_or_retry() {
    let root = Root::new();
    let trace = new_trace();
    let mut store = FileStore {
        inner: FakeSecretStore::new(trace.clone()),
        marker: root.marker(),
    };
    let mut broker = FakeBroker::new(trace);
    let clock = FakeClock::new(1_000_000);
    let service = ServiceConfig {
        issuer: "https://provider.invalid".into(),
        audience: "kohbor-synthetic".into(),
    };
    let mut provider = FakeProvider;
    let mut client = Client::new();
    let request = client
        .begin_handoff(material(false), &clock, &store)
        .unwrap();
    assert_eq!(
        client.deliver_callback(
            &request.state,
            "code_a",
            &service,
            &clock,
            &mut provider,
            &mut broker,
            &mut store
        ),
        CallbackOutcome::Accepted
    );
    assert!(!store.inner.account_items("acct_a").is_empty());
    broker.set_available(false);
    assert_eq!(
        client.delete_account(&clock, &mut broker, &mut store),
        OperationOutcome::Failed
    );
    assert_eq!(broker.delete_count(), 1);
    assert_eq!(root.marker().load(), Ok(deletion()));
    store.marker = root.marker();
    client = Client::new();
    broker.reset_request_count();
    broker.set_available(true);
    client.start(Some(&service), &clock, &mut broker, &mut store);
    assert!(client.friends().is_empty());
    assert!(client
        .begin_handoff(material(true), &clock, &store)
        .is_err());
    assert_eq!(
        client.delete_account(&clock, &mut broker, &mut store),
        OperationOutcome::Failed
    );
    assert_eq!(broker.request_count(), 0);
    assert_eq!(broker.delete_count(), 1);
    assert!(!store.inner.account_items("acct_a").is_empty());
    assert!(store.inner.account_items("acct_b").is_empty());
}
