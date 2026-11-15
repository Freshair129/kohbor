use crate::account_host::{AccountStorage, DeleteResolution};
use kohbor_account::{PersistedSession, RecoveryMarker, SecretStore};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "kohbor-lock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        private_acl(&path);
        Self(path)
    }
    fn storage(&self) -> AccountStorage {
        AccountStorage::open(&self.0).expect("open private local root")
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// Apply a protected DACL to the newly created leaf only, never its parents.
#[cfg(windows)]
fn private_acl(path: &Path) {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, ptr};
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
        fn GetTokenInformation(
            token: *mut c_void,
            class: u32,
            info: *mut c_void,
            len: u32,
            needed: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: *mut c_void, text: *mut *mut u16) -> i32;
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text: *const u16,
            rev: u32,
            sd: *mut *mut c_void,
            size: *mut u32,
        ) -> i32;
        fn GetFileSecurityW(
            path: *const u16,
            info: u32,
            sd: *mut c_void,
            len: u32,
            needed: *mut u32,
        ) -> i32;
        fn GetSecurityDescriptorControl(
            sd: *mut c_void,
            control: *mut u16,
            revision: *mut u32,
        ) -> i32;
        fn GetSecurityDescriptorDacl(
            sd: *mut c_void,
            present: *mut i32,
            dacl: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetAclInformation(acl: *mut c_void, info: *mut c_void, len: u32, class: u32) -> i32;
        fn GetAce(acl: *mut c_void, index: u32, ace: *mut *mut c_void) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn LocalFree(mem: *mut c_void) -> *mut c_void;
        fn CreateDirectoryW(path: *const u16, attributes: *mut c_void) -> i32;
    }
    #[repr(C)]
    struct SecurityAttributes {
        length: u32,
        descriptor: *mut c_void,
        inherit_handle: i32,
    }
    #[repr(C)]
    struct AclSize {
        count: u32,
        used: u32,
        free: u32,
    }
    unsafe {
        let mut token = ptr::null_mut();
        assert_ne!(
            OpenProcessToken(GetCurrentProcess(), 8, &mut token),
            0,
            "open process token"
        );
        let mut needed = 0;
        GetTokenInformation(token, 1, ptr::null_mut(), 0, &mut needed);
        let mut buf = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        assert_ne!(
            GetTokenInformation(token, 1, buf.as_mut_ptr().cast(), needed, &mut needed),
            0,
            "query token user"
        );
        let sid = *(buf.as_ptr() as *const *mut c_void);
        let mut sid_text = ptr::null_mut();
        assert_ne!(ConvertSidToStringSidW(sid, &mut sid_text), 0);
        let mut len = 0;
        while *sid_text.add(len) != 0 {
            len += 1;
        }
        let sid_string = String::from_utf16(std::slice::from_raw_parts(sid_text, len)).unwrap();
        LocalFree(sid_text.cast());
        CloseHandle(token);
        let sddl = format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{sid_string})");
        let wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut sd = ptr::null_mut();
        assert_ne!(
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                1,
                &mut sd,
                ptr::null_mut()
            ),
            0
        );
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut attributes = SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            descriptor: sd,
            inherit_handle: 0,
        };
        let created = CreateDirectoryW(
            name.as_ptr(),
            (&mut attributes as *mut SecurityAttributes).cast(),
        );
        let create_error = std::io::Error::last_os_error();
        LocalFree(sd);
        assert_ne!(created, 0, "create private fixture leaf: {create_error}");
        let mut needed = 0;
        GetFileSecurityW(name.as_ptr(), 4, ptr::null_mut(), 0, &mut needed);
        assert_ne!(
            needed,
            0,
            "query fixture descriptor size: {}",
            std::io::Error::last_os_error()
        );
        let mut descriptor = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        assert_ne!(
            GetFileSecurityW(
                name.as_ptr(),
                4,
                descriptor.as_mut_ptr().cast(),
                needed,
                &mut needed
            ),
            0,
            "query fixture descriptor: {}",
            std::io::Error::last_os_error()
        );
        let mut control = 0u16;
        let mut revision = 0;
        assert_ne!(
            GetSecurityDescriptorControl(
                descriptor.as_mut_ptr().cast(),
                &mut control,
                &mut revision
            ),
            0
        );
        if control & 0x1000 == 0 {
            let mut present = 0;
            let mut defaulted = 0;
            let mut dacl = ptr::null_mut();
            assert_ne!(
                GetSecurityDescriptorDacl(
                    descriptor.as_mut_ptr().cast(),
                    &mut present,
                    &mut dacl,
                    &mut defaulted
                ),
                0
            );
            if present != 0 && !dacl.is_null() {
                let mut info = AclSize {
                    count: 0,
                    used: 0,
                    free: 0,
                };
                assert_ne!(
                    GetAclInformation(
                        dacl,
                        (&mut info as *mut AclSize).cast(),
                        std::mem::size_of::<AclSize>() as u32,
                        2
                    ),
                    0
                );
                for index in 0..info.count {
                    let mut ace: *mut c_void = ptr::null_mut();
                    assert_ne!(GetAce(dacl, index, &mut ace), 0);
                    eprintln!(
                        "fixture ACE type={} inherited={}",
                        *(ace as *const u8),
                        *((ace as *const u8).add(1)) & 0x10 != 0
                    );
                }
            }
        }
        assert_ne!(control & 0x1000, 0, "fixture DACL must be protected");
    }
}
#[cfg(not(windows))]
fn private_acl(path: &Path) {
    std::fs::create_dir(path).expect("create owned test leaf");
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
fn prepare(g: &mut impl SecretStore) {
    g.write_session(&session("acct_a")).unwrap();
    g.write_recovery_marker(&pending()).unwrap();
}

#[test]
fn pending_hides_old_session_and_rejects_other_account_writes() {
    let root = Root::new();
    let storage = root.storage();
    let mut g = storage.try_lock().unwrap();
    prepare(&mut g);
    assert!(
        matches!(g.load_session(), Ok(None) | Err(_)),
        "pending session must not be exposed"
    );
    assert!(g.write_session(&session("acct_b")).is_err());
    assert!(g.write_friends("acct_b", &["intruder".into()]).is_err());
    assert_eq!(g.load_recovery_marker(), Ok(pending()));
}
#[test]
fn unknown_and_wrong_account_preserve_pending_and_original_session() {
    let root = Root::new();
    let storage = root.storage();
    let mut g = storage.try_lock().unwrap();
    prepare(&mut g);
    g.resolve_pending_delete("acct_a", DeleteResolution::Unknown)
        .unwrap();
    assert!(g
        .resolve_pending_delete("acct_b", DeleteResolution::Committed)
        .is_err());
    assert!(g
        .resolve_pending_delete("acct_b", DeleteResolution::NotCommitted)
        .is_err());
    assert_eq!(g.load_recovery_marker(), Ok(pending()));
    assert!(matches!(g.load_session(), Ok(None) | Err(_)));
    g.resolve_pending_delete("acct_a", DeleteResolution::NotCommitted)
        .unwrap();
    assert_eq!(g.load_recovery_marker(), Ok(RecoveryMarker::Clear));
    assert_eq!(g.load_session(), Ok(Some(session("acct_a"))));
}
#[test]
fn committed_resolution_wipes_and_stays_wiped_across_fresh_guard() {
    let root = Root::new();
    let storage = root.storage();
    let mut g = storage.try_lock().unwrap();
    prepare(&mut g);
    g.resolve_pending_delete("acct_a", DeleteResolution::Committed)
        .unwrap();
    assert_eq!(g.load_recovery_marker(), Ok(RecoveryMarker::Clear));
    assert_eq!(g.load_session(), Ok(None));
    g.wipe_account("acct_a").unwrap();
    drop(g);
    let g = root.storage().try_lock().unwrap();
    assert_eq!(g.load_recovery_marker(), Ok(RecoveryMarker::Clear));
    assert_eq!(g.load_session(), Ok(None));
}
#[test]
fn rejects_relative_missing_and_file_roots() {
    assert!(AccountStorage::open(Path::new("relative-root")).is_err());
    let root = Root::new();
    assert!(AccountStorage::open(&root.0.join("missing")).is_err());
    let file = root.0.join("file");
    std::fs::write(&file, b"x").unwrap();
    assert!(AccountStorage::open(&file).is_err());
}
#[test]
fn child_lock_helper() {
    let Ok(path) = std::env::var("KOHBOR_LOCK_CHILD_ROOT") else {
        return;
    };
    let storage = AccountStorage::open(Path::new(&path)).expect("child open");
    match std::env::var("KOHBOR_LOCK_CHILD_MODE").unwrap().as_str() {
        "contended" => assert!(
            storage.try_lock().is_err(),
            "child must not acquire guard while parent holds it"
        ),
        "released" => {
            let mut g = storage.try_lock().expect("child acquires released lock");
            assert_eq!(g.load_recovery_marker(), Ok(RecoveryMarker::Clear));
            assert_eq!(g.load_session(), Ok(Some(session("acct_a"))));
            g.write_friends("acct_a", &["child".into()]).unwrap();
        }
        _ => panic!("invalid child mode"),
    }
}
fn child(root: &Path, mode: &str) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "host_storage_lock::child_lock_helper",
            "--nocapture",
        ])
        .env("KOHBOR_LOCK_CHILD_ROOT", root)
        .env("KOHBOR_LOCK_CHILD_MODE", mode)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "child {mode} failed: {}",
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("1 passed"),
        "child helper was not run"
    );
}
#[test]
fn exclusive_lock_blocks_other_process_until_guard_drops() {
    let root = Root::new();
    let storage = root.storage();
    let mut g = storage.try_lock().unwrap();
    g.write_session(&session("acct_a")).unwrap();
    child(&root.0, "contended");
    assert_eq!(g.load_recovery_marker(), Ok(RecoveryMarker::Clear));
    assert_eq!(g.load_session(), Ok(Some(session("acct_a"))));
    drop(g);
    child(&root.0, "released");
}

#[test]
fn callback_started_before_pending_cannot_create_account_b() {
    use crate::support::fakes::{new_trace, FakeBroker, FakeClock, FakeProvider};
    use kohbor_account::{AuthMaterial, CallbackOutcome, Client, ServiceConfig};
    let root = Root::new();
    let storage = root.storage();
    let mut g = storage.try_lock().unwrap();
    g.write_session(&session("acct_a")).unwrap();
    let clock = FakeClock::new(1_000_000);
    let trace = new_trace();
    let mut broker = FakeBroker::new(trace);
    let mut provider = FakeProvider;
    let mut client = Client::new();
    let material = AuthMaterial {
        state: "state-b".into(),
        nonce: "nonce-b".into(),
        code_verifier: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_".into(),
    };
    let request = client.begin_handoff(material, &clock, &g).unwrap();
    g.write_recovery_marker(&pending()).unwrap();
    let service = ServiceConfig {
        issuer: "https://provider.invalid".into(),
        audience: "kohbor-synthetic".into(),
    };
    assert_ne!(
        client.deliver_callback(
            (&request.state, "code_b"),
            &service,
            &clock,
            &mut provider,
            &mut broker,
            &mut g
        ),
        CallbackOutcome::Accepted
    );
    assert_eq!(g.load_recovery_marker(), Ok(pending()));
    g.resolve_pending_delete("acct_a", DeleteResolution::NotCommitted)
        .unwrap();
    assert_eq!(g.load_session(), Ok(Some(session("acct_a"))));
}
#[cfg(windows)]
#[test]
fn rejects_inherited_broad_write_leaf() {
    // The owned test lane currently grants inherited Modify to Everyone and
    // Authenticated Users. Do not change the ACL on this leaf or its parent.
    let path = std::env::temp_dir().join(format!(
        "kohbor-broad-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    let denied = AccountStorage::open(&path).is_err();
    std::fs::remove_dir(&path).unwrap();
    assert!(denied, "a broad-write root is not trusted private storage");
}

#[test]
fn fresh_guard_cannot_resolve_existing_pending_via_secret_store() {
    let root = Root::new();
    let storage = root.storage();
    let mut first = storage.try_lock().unwrap();
    prepare(&mut first);
    drop(first);

    let mut fresh = storage.try_lock().unwrap();
    assert!(fresh.write_recovery_marker(&RecoveryMarker::Clear).is_err());
    assert!(fresh
        .write_recovery_marker(&RecoveryMarker::WipePending {
            account_id: "acct_a".into(),
        })
        .is_err());
    assert_eq!(fresh.load_recovery_marker(), Ok(pending()));
    assert!(matches!(fresh.load_session(), Ok(None) | Err(_)));

    fresh
        .resolve_pending_delete("acct_a", DeleteResolution::NotCommitted)
        .unwrap();
    assert_eq!(fresh.load_recovery_marker(), Ok(RecoveryMarker::Clear));
    assert_eq!(fresh.load_session(), Ok(Some(session("acct_a"))));
}
