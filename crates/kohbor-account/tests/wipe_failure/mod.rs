use crate::support::fakes::{
    new_trace, FakeBroker, FakeClock, FakeProvider, FakeSecretStore, TraceObserver,
};
use kohbor_account::{
    AccountState, AuthMaterial, CallbackOutcome, Client, OperationOutcome, ServiceConfig,
};

struct Fixture {
    client: Client,
    broker: FakeBroker,
    store: FakeSecretStore,
    clock: FakeClock,
    provider: FakeProvider,
    observer: TraceObserver,
    service: ServiceConfig,
}
impl Fixture {
    fn new() -> Self {
        let trace = new_trace();
        Self {
            client: Client::new(),
            broker: FakeBroker::new(trace.clone()),
            store: FakeSecretStore::new(trace.clone()),
            clock: FakeClock::new(1_000_000),
            provider: FakeProvider,
            observer: TraceObserver::new(trace),
            service: ServiceConfig {
                issuer: "https://provider.invalid".into(),
                audience: "kohbor-synthetic".into(),
            },
        }
    }
    fn signin(&mut self, b: bool) -> CallbackOutcome {
        let (state, nonce, code) = if b {
            ("state-b", "nonce-b", "code_b")
        } else {
            ("state-a", "nonce-a", "code_a")
        };
        let request = self.client.begin_handoff(
            AuthMaterial {
                state: state.into(),
                nonce: nonce.into(),
                code_verifier: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
                    .into(),
            },
            &self.clock,
            &self.store,
        );
        match request {
            Ok(request) => self.client.deliver_callback(
                (&request.state, code),
                &self.service,
                &self.clock,
                &mut self.provider,
                &mut self.broker,
                &mut self.store,
            ),
            Err(outcome) => outcome,
        }
    }
    fn fail_cleanup(&mut self, delete: bool) {
        assert_eq!(self.signin(false), CallbackOutcome::Accepted);
        self.store.set_fail_wipe(true);
        let result = if delete {
            self.client
                .delete_account(&self.clock, &mut self.broker, &mut self.store)
        } else {
            self.client
                .sign_out(&mut self.broker, &mut self.store, &mut self.observer)
        };
        assert_eq!(result, OperationOutcome::Failed);
        assert!(
            self.store
                .account_items("acct_a")
                .contains(&"refresh_a".to_string()),
            "fixture must retain A after failed wipe"
        );
        if delete {
            assert_eq!(self.broker.delete_count(), 1);
        }
    }
    fn reopen_offline(&mut self) {
        self.client = Client::new();
        self.broker.set_available(false);
        self.broker.reset_request_count();
        self.client.start(
            Some(&self.service),
            &self.clock,
            &mut self.broker,
            &mut self.store,
        );
        assert_eq!(
            self.client.state(),
            AccountState::SignedOut,
            "reopen must not restore A after failed cleanup"
        );
        assert!(
            self.client.friends().is_empty(),
            "reopen must not expose A friends"
        );
        assert_eq!(
            self.broker.request_count(),
            0,
            "reopen must not send A credentials to broker"
        );
    }
    fn blocked_switch(&mut self) {
        assert_eq!(
            self.client
                .sign_out(&mut self.broker, &mut self.store, &mut self.observer),
            OperationOutcome::Failed
        );
        let requests = self.broker.request_count();
        let writes = self.store.write_count();
        assert_ne!(
            self.signin(true),
            CallbackOutcome::Accepted,
            "B must be blocked while A wipe fails"
        );
        assert_eq!(
            self.broker.request_count(),
            requests,
            "blocked B must not contact broker"
        );
        assert_eq!(
            self.store.write_count(),
            writes,
            "blocked B must not write a session"
        );
        assert!(self.store.account_items("acct_b").is_empty());
    }
}

#[test]
fn signout_failed_wipe_reopen_offline_never_restores_a() {
    let mut f = Fixture::new();
    f.fail_cleanup(false);
    f.reopen_offline();
}
#[test]
fn deletion_failed_wipe_reopen_offline_never_restores_a() {
    let mut f = Fixture::new();
    f.fail_cleanup(true);
    f.reopen_offline();
}
#[test]
fn signout_repeated_failed_wipe_blocks_switch_to_b() {
    let mut f = Fixture::new();
    f.fail_cleanup(false);
    f.blocked_switch();
}
#[test]
fn deletion_repeated_failed_wipe_blocks_switch_to_b() {
    let mut f = Fixture::new();
    f.fail_cleanup(true);
    f.blocked_switch();
}
#[test]
fn deletion_cleanup_retry_allows_b_without_repeating_delete() {
    let mut f = Fixture::new();
    f.fail_cleanup(true);
    f.store.set_fail_wipe(false);
    assert_eq!(
        f.client
            .sign_out(&mut f.broker, &mut f.store, &mut f.observer),
        OperationOutcome::Accepted
    );
    assert!(f.store.account_items("acct_a").is_empty());
    assert_eq!(f.signin(true), CallbackOutcome::Accepted);
    assert_eq!(
        f.broker.delete_count(),
        1,
        "committed deletion must not be reissued"
    );
}
#[test]
fn signout_cleanup_retry_allows_b() {
    let mut f = Fixture::new();
    f.fail_cleanup(false);
    f.store.set_fail_wipe(false);
    assert_eq!(
        f.client
            .sign_out(&mut f.broker, &mut f.store, &mut f.observer),
        OperationOutcome::Accepted
    );
    assert!(f.store.account_items("acct_a").is_empty());
    assert_eq!(f.signin(true), CallbackOutcome::Accepted);
}
#[test]
fn reopened_client_blocks_b_while_failed_signout_cleanup_persists() {
    let mut f = Fixture::new();
    f.fail_cleanup(false);
    f.client = Client::new();
    f.broker.set_available(false);
    f.client
        .start(Some(&f.service), &f.clock, &mut f.broker, &mut f.store);
    f.broker.set_available(true);
    f.blocked_switch();
}
#[test]
fn reopened_client_blocks_b_while_failed_deletion_cleanup_persists() {
    let mut f = Fixture::new();
    f.fail_cleanup(true);
    f.client = Client::new();
    f.broker.set_available(false);
    f.client
        .start(Some(&f.service), &f.clock, &mut f.broker, &mut f.store);
    f.broker.set_available(true);
    f.blocked_switch();
}
