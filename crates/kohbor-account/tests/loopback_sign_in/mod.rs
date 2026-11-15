use crate::account_host::{AccountHost, BrowserSignInError as Error, BrowserSignInRequest};
use crate::support::fakes::{new_trace, FakeBroker, FakeClock, FakeProvider};
use kohbor_account::{
    AuthMaterial, CallbackOutcome, Client, Provider, ProviderError, ProviderIdentity, SecretStore,
    ServiceConfig,
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
