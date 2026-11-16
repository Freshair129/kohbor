use kohbor_account::{
    AccountState, AuthMaterial, AuthorizationRequest, CallbackOutcome, Client, OperationOutcome,
    ServiceConfig,
};

use super::contract::{Account, ContractHarness, Event, Outcome, State};
use super::fakes::{
    clear_trace, new_trace, FakeBroker, FakeClock, FakeProvider, FakeSecretStore, TraceObserver,
};

const INITIAL_TIME_MS: u64 = 1_000_000;
const STATE_A: &str = "expected_state_a";
const STATE_B: &str = "expected_state_b";
const VERIFIER: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";

pub struct Harness {
    client: Client,
    provider: FakeProvider,
    broker: FakeBroker,
    store: FakeSecretStore,
    clock: FakeClock,
    service: Option<ServiceConfig>,
    expected_state: &'static str,
    pending_friend_request: Option<u64>,
    trace: super::fakes::SharedTrace,
    local_core: Vec<&'static str>,
}

impl Harness {
    pub fn new_isolated() -> Self {
        let trace = new_trace();
        Self {
            client: Client::new(),
            provider: FakeProvider,
            broker: FakeBroker::new(trace.clone()),
            store: FakeSecretStore::new(trace.clone()),
            clock: FakeClock::new(INITIAL_TIME_MS),
            service: Some(ServiceConfig {
                issuer: "https://provider.invalid".to_owned(),
                audience: "kohbor-synthetic".to_owned(),
            }),
            expected_state: STATE_A,
            pending_friend_request: None,
            trace,
            local_core: vec![
                "gameplay:local-sentinel",
                "history:local-sentinel",
                "settings:local-sentinel",
            ],
        }
    }

    fn start_handoff(&mut self, account: Account) -> Option<AuthorizationRequest> {
        let (state, nonce) = match account {
            Account::A => (STATE_A, "nonce-a"),
            Account::B => (STATE_B, "nonce-b"),
        };
        let request = self
            .client
            .begin_handoff(
                AuthMaterial {
                    state: state.to_owned(),
                    nonce: nonce.to_owned(),
                    code_verifier: VERIFIER.to_owned(),
                },
                &self.clock,
                &self.store,
            )
            .ok()?;
        self.expected_state = observed_static_state(&request);
        Some(request)
    }

    fn synthetic_sign_in(&mut self, account: Account) -> CallbackOutcome {
        let code = match account {
            Account::A => "code_a",
            Account::B => "code_b",
        };
        let Some(request) = self.start_handoff(account) else {
            return CallbackOutcome::Rejected;
        };
        let Some(service) = self.service.as_ref() else {
            return CallbackOutcome::Rejected;
        };
        self.client.deliver_callback(
            (&request.state, code),
            service,
            &self.clock,
            &mut self.provider,
            &mut self.broker,
            &mut self.store,
        )
    }

    fn fixture_account_id(account: Account) -> &'static str {
        match account {
            Account::A => "acct_a",
            Account::B => "acct_b",
        }
    }

    fn outcome(value: OperationOutcome) -> Outcome {
        match value {
            OperationOutcome::Accepted => Outcome::Accepted,
            OperationOutcome::Rejected => Outcome::Rejected,
            OperationOutcome::Failed => Outcome::Failed,
        }
    }

    fn callback_outcome(value: CallbackOutcome) -> Outcome {
        match value {
            CallbackOutcome::Accepted => Outcome::Accepted,
            CallbackOutcome::Rejected => Outcome::Rejected,
            CallbackOutcome::Failed => Outcome::Failed,
        }
    }
}

impl ContractHarness for Harness {
    fn now_ms(&self) -> u64 {
        kohbor_account::Clock::now_ms(&self.clock)
    }

    fn advance_ms(&mut self, elapsed: u64) {
        self.clock.advance_ms(elapsed);
    }

    fn begin_handoff(&mut self) {
        let _ = self.start_handoff(Account::A);
    }

    fn cancel_handoff(&mut self) {
        self.client.cancel_handoff();
    }

    fn deliver_callback(&mut self, state: &'static str, code: &'static str) -> Outcome {
        let Some(service) = self.service.as_ref() else {
            return Outcome::Rejected;
        };
        Self::callback_outcome(self.client.deliver_callback(
            (state, code),
            service,
            &self.clock,
            &mut self.provider,
            &mut self.broker,
            &mut self.store,
        ))
    }

    fn expected_state(&self) -> &'static str {
        self.expected_state
    }

    fn state(&self) -> State {
        match self.client.state() {
            AccountState::SignedOut
            | AccountState::ServiceUnavailable
            | AccountState::SessionRevoked
            | AccountState::Deleting => State::SignedOut,
            AccountState::BrowserHandoff | AccountState::SigningIn => State::BrowserHandoff,
            AccountState::SignedInOffline => State::SignedInOffline,
            AccountState::SignedInOnline
            | AccountState::NeedsDisplayName
            | AccountState::PausedNewDevice => State::SignedInOnline,
        }
    }

    fn secret_writes(&self) -> usize {
        self.store.write_count()
    }

    fn no_service_configured(&mut self) {
        self.service = None;
    }

    fn seed_offline_session(&mut self) {
        self.store.seed_session(kohbor_account::PersistedSession {
            account_id: "acct_a".to_owned(),
            refresh_token: "refresh_a".to_owned(),
            device_key: "device_a".to_owned(),
            friends: vec!["cached_friend".to_owned()],
            pairing_ids: vec!["pair_a_1".to_owned(), "pair_a_2".to_owned()],
            private_item_labels: vec!["cache_a".to_owned(), "pair_secret_a".to_owned()],
        });
        self.store.reset_write_count();
        self.broker.reset_request_count();
    }

    fn set_broker_available(&mut self, available: bool) {
        self.broker.set_available(available);
    }

    fn start(&mut self) {
        self.client.start(
            self.service.as_ref(),
            &self.clock,
            &mut self.broker,
            &mut self.store,
        );
    }

    fn local_core_probe(&self) -> bool {
        self.local_core.contains(&"gameplay:local-sentinel")
    }

    fn broker_requests(&self) -> usize {
        self.broker.request_count()
    }

    fn friends(&self) -> Vec<&'static str> {
        self.client
            .friends()
            .iter()
            .map(|friend| static_friend(friend))
            .collect()
    }

    fn request_friend_change(&mut self) -> Outcome {
        Self::outcome(
            self.client
                .request_friend_change(&mut self.broker, "add", "KB-SYNTHETIC"),
        )
    }

    fn seed_signed_in_a(&mut self) {
        let _ = self.synthetic_sign_in(Account::A);
        clear_trace(&self.trace);
    }

    fn generation(&self) -> u64 {
        self.client.generation()
    }

    fn events(&self) -> Vec<Event> {
        self.trace.borrow().clone()
    }

    fn fail_revoke(&mut self) {
        self.broker.set_fail_revoke();
    }

    fn hold_friend_reply(&mut self, account: Account, value: &'static str) {
        self.broker.hold_next_friend_reply(value);
        self.client
            .request_friend_list(&mut self.broker, &mut self.store);
        let expected_account = Self::fixture_account_id(account);
        self.pending_friend_request = if self.broker.last_friend_account() == Some(expected_account)
        {
            self.broker.last_pending_request()
        } else {
            None
        };
    }

    fn release_friend_reply(&mut self) {
        if let Some(request_id) = self.pending_friend_request.take() {
            let _ = self
                .client
                .deliver_friend_reply(request_id, &mut self.broker, &mut self.store);
        }
    }

    fn sign_out(&mut self) {
        let mut observer = TraceObserver::new(self.trace.clone());
        self.client
            .sign_out(&mut self.broker, &mut self.store, &mut observer);
    }

    fn switch_to_b(&mut self) {
        self.sign_out();
        let _ = self.synthetic_sign_in(Account::B);
    }

    fn local_core_snapshot(&self) -> Vec<&'static str> {
        self.local_core.clone()
    }

    fn remaining_a_local_items(&self) -> Vec<&'static str> {
        self.store
            .account_items("acct_a")
            .iter()
            .map(|item| static_item(item))
            .collect()
    }

    fn seed_delete_graph(&mut self) {
        self.broker.seed_delete_graph();
        let _ = self.synthetic_sign_in(Account::A);
        clear_trace(&self.trace);
        self.broker.reset_request_count();
    }

    fn broker_rows(&self, account: Account) -> Vec<&'static str> {
        self.broker
            .rows_for(account)
            .iter()
            .map(|row| static_row(row))
            .collect()
    }

    fn local_account_items(&self) -> Vec<&'static str> {
        self.remaining_a_local_items()
    }

    fn reauthenticate_at(&mut self, timestamp_ms: u64) {
        self.clock.set_ms(timestamp_ms);
        let _ = self.synthetic_sign_in(Account::A);
    }

    fn fail_delete(&mut self) {
        self.broker.set_fail_delete();
    }

    fn delete_account(&mut self) -> Outcome {
        Self::outcome(
            self.client
                .delete_account(&self.clock, &mut self.broker, &mut self.store),
        )
    }
}

fn observed_static_state(request: &AuthorizationRequest) -> &'static str {
    match request.state.as_str() {
        STATE_A => STATE_A,
        STATE_B => STATE_B,
        _ => "<unrecognized-client-state>",
    }
}

fn static_friend(friend: &str) -> &'static str {
    match friend {
        "cached_friend" => "cached_friend",
        "friend_a" => "friend_a",
        "friend_b" => "friend_b",
        "late_a" => "late_a",
        _ => "<unrecognized-friend>",
    }
}

fn static_item(item: &str) -> &'static str {
    match item {
        "refresh_a" => "refresh_a",
        "device_a" => "device_a",
        "cache_a" => "cache_a",
        "pair_secret_a" => "pair_secret_a",
        "refresh_b" => "refresh_b",
        "device_b" => "device_b",
        "cache_b" => "cache_b",
        "pair_secret_b" => "pair_secret_b",
        _ => "<unrecognized-account-item>",
    }
}

fn static_row(row: &str) -> &'static str {
    match row {
        "account:acct_a" => "account:acct_a",
        "identity:acct_a" => "identity:acct_a",
        "session:acct_a" => "session:acct_a",
        "request:acct_a->acct_b" => "request:acct_a->acct_b",
        "request:acct_b->acct_a" => "request:acct_b->acct_a",
        "friendship:acct_b<->acct_a" => "friendship:acct_b<->acct_a",
        "block:acct_b->acct_a" => "block:acct_b->acct_a",
        "share:acct_a" => "share:acct_a",
        "pairing:acct_a" => "pairing:acct_a",
        "rate-limit:acct_a" => "rate-limit:acct_a",
        "tombstone:acct_a" => "tombstone:acct_a",
        "account:acct_b" => "account:acct_b",
        "session:acct_b" => "session:acct_b",
        "share:acct_b" => "share:acct_b",
        _ => "<unrecognized-broker-row>",
    }
}
