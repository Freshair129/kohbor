use crate::account_host::{AccountHost, BrowserSignInError as Error, BrowserSignInRequest};
use crate::support::fakes::{new_trace, FakeBroker, FakeClock, FakeProvider, TraceObserver};
use kohbor_account::{
    AuthMaterial, CallbackOutcome, Client, OperationOutcome, Provider, ProviderError,
    ProviderIdentity, RecoveryMarker, SecretStore, ServiceConfig,
};
use std::{
    io::{Read, Write},
    net::{SocketAddrV4, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "kohbor-loopback-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn open(&self) -> AccountHost {
        AccountHost::open(&self.0).unwrap()
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct CountingProvider(usize);
impl Provider for CountingProvider {
    fn exchange_code(
        &mut self,
        code: &str,
        verifier: &str,
    ) -> Result<ProviderIdentity, ProviderError> {
        self.0 += 1;
        FakeProvider.exchange_code(code, verifier)
    }
}
fn material() -> AuthMaterial {
    AuthMaterial {
        state: "state-a".into(),
        nonce: "nonce-a".into(),
        code_verifier: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_".into(),
    }
}
fn service() -> ServiceConfig {
    ServiceConfig {
        issuer: "https://provider.invalid".into(),
        audience: "kohbor-synthetic".into(),
    }
}
fn request<'a>(s: &'a ServiceConfig, cancel: &'a AtomicBool) -> BrowserSignInRequest<'a> {
    BrowserSignInRequest {
        material: material(),
        service: s,
        in_live_match: false,
        timeout: Duration::from_secs(2),
        cancelled: cancel,
    }
}
fn send(addr: SocketAddrV4, wire: &'static str) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut socket = TcpStream::connect(addr).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket.write_all(wire.as_bytes()).unwrap();
        let mut response = String::new();
        let _ = socket.read_to_string(&mut response);
        response
    })
}
fn absent(host: &AccountHost, provider: &CountingProvider, broker: &FakeBroker) {
    assert_eq!(provider.0, 0);
    assert_eq!(broker.request_count(), 0);
    assert_eq!(host.with_operation(|g| g.load_session()).unwrap(), Ok(None));
}

fn run_same_code_handoff(
    host: &AccountHost,
    client: &mut Client,
    provider: &mut CountingProvider,
    broker: &mut FakeBroker,
    before_callback: impl FnOnce(),
) -> Result<CallbackOutcome, Error> {
    let service = service();
    let cancelled = AtomicBool::new(false);
    let mut worker = None;
    let mut req = request(&service, &cancelled);
    req.timeout = Duration::from_secs(5);
    let result = host.sign_in_via_browser(
        client,
        req,
        &FakeClock::new(1_000_000),
        provider,
        broker,
        |_, addr| {
            before_callback();
            worker = Some(send(
                addr,
                "GET /callback?code=code_a&state=state-a HTTP/1.1\r\nHost: localhost\r\n\r\n",
            ));
            Ok(())
        },
    );
    worker.unwrap().join().unwrap();
    result
}

fn signed_in_fixture(host: &AccountHost) -> (Client, FakeBroker) {
    let mut client = Client::new();
    let mut provider = CountingProvider(0);
    let mut broker = FakeBroker::new(new_trace());
    assert_eq!(
        run_same_code_handoff(host, &mut client, &mut provider, &mut broker, || {}),
        Ok(CallbackOutcome::Accepted)
    );
    assert_eq!(provider.0, 1);
    (client, broker)
}

fn complete_sign_out(host: &AccountHost, client: &mut Client, broker: &mut FakeBroker) {
    let result = host
        .with_operation(|guard| {
            let mut observer = TraceObserver::new(new_trace());
            client.sign_out(broker, guard, &mut observer)
        })
        .unwrap();
    assert_eq!(result, OperationOutcome::Accepted);
}

fn assert_marker_clear(host: &AccountHost) {
    assert_eq!(
        host.with_operation(|guard| guard.load_recovery_marker())
            .unwrap(),
        Ok(RecoveryMarker::Clear)
    );
}

fn assert_clear_and_absent(host: &AccountHost) {
    assert_marker_clear(host);
    assert_eq!(
        host.with_operation(|guard| guard.load_session()).unwrap(),
        Ok(None)
    );
}

#[test]
fn valid_callback_uses_pkce_persists_session_and_closes_listener() {
    let root = Root::new();
    let host = root.open();
    let mut client = Client::new();
    let s = service();
    let cancel = AtomicBool::new(false);
    let clock = FakeClock::new(1_000_000);
    let mut provider = CountingProvider(0);
    let mut broker = FakeBroker::new(new_trace());
    let mut worker = None;
    let mut address = None;
    let outcome = host.sign_in_via_browser(
        &mut client,
        request(&s, &cancel),
        &clock,
        &mut provider,
        &mut broker,
        |auth, addr| {
            assert_eq!(*addr.ip(), std::net::Ipv4Addr::LOCALHOST);
            assert_ne!(addr.port(), 0);
            assert_eq!(auth.state, "state-a");
            assert_eq!(auth.nonce, "nonce-a");
            assert_eq!(auth.scope, "openid");
            assert_eq!(auth.code_challenge_method, "S256");
            assert_ne!(auth.code_challenge, material().code_verifier);
            assert!(
                host.with_operation(|_| ()).is_ok(),
                "storage guard must not span browser wait"
            );
            address = Some(addr);
            worker = Some(send(
                addr,
                "GET /callback?code=code_a&state=state-a HTTP/1.1\r\nHost: localhost\r\n\r\n",
            ));
            Ok(())
        },
    );
    assert_eq!(outcome, Ok(CallbackOutcome::Accepted));
    let response = worker.unwrap().join().unwrap();
    for secret in ["code_a", "state-a", "refresh_a"] {
        assert!(!response.contains(secret));
    }
    assert_eq!(provider.0, 1);
    assert_eq!(broker.request_count(), 1);
    assert_eq!(
        host.with_operation(|g| g.load_session())
            .unwrap()
            .unwrap()
            .unwrap()
            .account_id,
        "acct_a"
    );
    let bytes = std::fs::read(root.0.join("account").join("account-session.bin")).unwrap();
    assert!(!bytes.windows(9).any(|w| w == b"refresh_a"));
    assert!(
        TcpStream::connect_timeout(&address.unwrap().into(), Duration::from_millis(250)).is_err()
    );
}

#[test]
fn stale_callback_after_other_host_sign_out_preserves_tombstone_and_clear_marker() {
    for seed_tombstone in [false, true] {
        let root = Root::new();
        let host_a = root.open();
        let host_b = root.open();
        let session_path = root.0.join("account").join("account-session.bin");

        if seed_tombstone {
            let (mut seed_client, mut seed_broker) = signed_in_fixture(&host_b);
            complete_sign_out(&host_b, &mut seed_client, &mut seed_broker);
            assert_clear_and_absent(&host_b);
        } else {
            assert!(!session_path.exists());
        }
        let previous_tombstone = seed_tombstone.then(|| std::fs::read(&session_path).unwrap());

        let mut client_a = Client::new();
        let mut provider_a = CountingProvider(0);
        let mut broker_a = FakeBroker::new(new_trace());
        let mut new_tombstone = None;
        let result = run_same_code_handoff(
            &host_a,
            &mut client_a,
            &mut provider_a,
            &mut broker_a,
            || {
                assert!(host_b.with_operation(|_| ()).is_ok());
                let (mut client_b, mut broker_b) = signed_in_fixture(&host_b);
                complete_sign_out(&host_b, &mut client_b, &mut broker_b);
                assert_clear_and_absent(&host_b);

                let tombstone = std::fs::read(&session_path).unwrap();
                if let Some(previous) = previous_tombstone.as_ref() {
                    assert_ne!(previous, &tombstone);
                }
                new_tombstone = Some(tombstone);
            },
        );

        assert_eq!(result, Err(Error::Cancelled));
        assert_eq!(client_a.state(), kohbor_account::AccountState::SignedOut);
        absent(&host_a, &provider_a, &broker_a);
        assert_marker_clear(&host_a);
        assert_eq!(
            std::fs::read(&session_path).unwrap(),
            new_tombstone.unwrap()
        );
    }
}

#[test]
fn stale_callback_preserves_same_account_session_replacement() {
    let root = Root::new();
    let host_a = root.open();
    let host_b = root.open();
    let session_path = root.0.join("account").join("account-session.bin");

    let (_seed_client, _seed_broker) = signed_in_fixture(&host_b);
    let previous_session = std::fs::read(&session_path).unwrap();

    let mut client_a = Client::new();
    let mut provider_a = CountingProvider(0);
    let mut broker_a = FakeBroker::new(new_trace());
    let mut replacement = None;
    let result = run_same_code_handoff(
        &host_a,
        &mut client_a,
        &mut provider_a,
        &mut broker_a,
        || {
            assert!(host_b.with_operation(|_| ()).is_ok());
            let (_client_b, _broker_b) = signed_in_fixture(&host_b);
            let bytes = std::fs::read(&session_path).unwrap();
            assert_ne!(previous_session, bytes);
            assert_marker_clear(&host_b);
            assert_eq!(
                host_b
                    .with_operation(|guard| guard.load_session())
                    .unwrap()
                    .unwrap()
                    .unwrap()
                    .account_id,
                "acct_a"
            );
            replacement = Some(bytes);
        },
    );

    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(client_a.state(), kohbor_account::AccountState::SignedOut);
    assert_eq!(provider_a.0, 0);
    assert_eq!(broker_a.request_count(), 0);
    assert_marker_clear(&host_a);
    assert_eq!(
        host_a
            .with_operation(|guard| guard.load_session())
            .unwrap()
            .unwrap()
            .unwrap()
            .account_id,
        "acct_a"
    );
    assert_eq!(std::fs::read(&session_path).unwrap(), replacement.unwrap());
}

#[test]
fn invalid_callbacks_do_not_exchange_or_persist() {
    for wire in [
        "GET /callback?code=code_a&state=wrong HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /callback?code=code_a&state=state-a&state=state-a HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "POST /callback?code=code_a&state=state-a HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /other?code=code_a&state=state-a HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /callback?code=&state=state-a HTTP/1.1\r\nHost: localhost\r\n\r\n",
    ] {
        let root = Root::new();
        let host = root.open();
        let mut client = Client::new();
        let s = service();
        let cancel = AtomicBool::new(false);
        let mut provider = CountingProvider(0);
        let mut broker = FakeBroker::new(new_trace());
        let mut worker = None;
        let result = host.sign_in_via_browser(
            &mut client,
            request(&s, &cancel),
            &FakeClock::new(1_000_000),
            &mut provider,
            &mut broker,
            |_, addr| {
                worker = Some(send(addr, wire));
                Ok(())
            },
        );
        assert_eq!(result, Err(Error::InvalidCallback));
        worker.unwrap().join().unwrap();
        absent(&host, &provider, &broker);
    }
}

#[test]
fn live_match_prevents_launch_and_side_effects() {
    let root = Root::new();
    let host = root.open();
    let s = service();
    let cancel = AtomicBool::new(false);
    let mut req = request(&s, &cancel);
    req.in_live_match = true;
    let mut provider = CountingProvider(0);
    let mut broker = FakeBroker::new(new_trace());
    let mut launched = false;
    assert_eq!(
        host.sign_in_via_browser(
            &mut Client::new(),
            req,
            &FakeClock::new(1),
            &mut provider,
            &mut broker,
            |_, _| {
                launched = true;
                Ok(())
            }
        ),
        Err(Error::LiveMatch)
    );
    assert!(!launched);
    absent(&host, &provider, &broker);
}

#[test]
fn timeout_and_launch_failure_have_no_side_effects() {
    for fail in [false, true] {
        let root = Root::new();
        let host = root.open();
        let s = service();
        let cancel = AtomicBool::new(false);
        let mut provider = CountingProvider(0);
        let mut broker = FakeBroker::new(new_trace());
        let mut req = request(&s, &cancel);
        req.timeout = Duration::from_millis(250);
        let result = host.sign_in_via_browser(
            &mut Client::new(),
            req,
            &FakeClock::new(1),
            &mut provider,
            &mut broker,
            |_, _| {
                if fail {
                    Err(Error::BrowserLaunch)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            result,
            Err(if fail {
                Error::BrowserLaunch
            } else {
                Error::Timeout
            })
        );
        absent(&host, &provider, &broker);
    }
}

#[test]
fn cancellation_during_partial_request_interrupts_wait_without_side_effects() {
    let root = Root::new();
    let host = root.open();
    let s = service();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut provider = CountingProvider(0);
    let mut broker = FakeBroker::new(new_trace());
    let mut worker = None;
    let result = host.sign_in_via_browser(
        &mut Client::new(),
        request(&s, &cancel),
        &FakeClock::new(1),
        &mut provider,
        &mut broker,
        |_, addr| {
            let flag = cancel.clone();
            worker = Some(thread::spawn(move || {
                let mut stream = TcpStream::connect(addr).unwrap();
                stream.write_all(b"GET /callback?").unwrap();
                flag.store(true, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(400));
            }));
            Ok(())
        },
    );
    assert_eq!(result, Err(Error::Cancelled));
    worker.unwrap().join().unwrap();
    absent(&host, &provider, &broker);
}

#[test]
fn partial_header_times_out_without_exchange() {
    let root = Root::new();
    let host = root.open();
    let s = service();
    let cancel = AtomicBool::new(false);
    let mut provider = CountingProvider(0);
    let mut broker = FakeBroker::new(new_trace());
    let mut worker = None;
    let mut req = request(&s, &cancel);
    req.timeout = Duration::from_millis(250);
    let result = host.sign_in_via_browser(
        &mut Client::new(),
        req,
        &FakeClock::new(1),
        &mut provider,
        &mut broker,
        |_, addr| {
            worker = Some(thread::spawn(move || {
                let mut stream = TcpStream::connect(addr).unwrap();
                stream.write_all(b"GET /callback?").unwrap();
                thread::sleep(Duration::from_millis(400));
            }));
            Ok(())
        },
    );
    assert_eq!(result, Err(Error::Timeout));
    worker.unwrap().join().unwrap();
    absent(&host, &provider, &broker);
}
