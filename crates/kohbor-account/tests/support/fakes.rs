use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use kohbor_account::{
    Broker, BrokerError, BrokerReply, ClientObserver, Clock, PersistedSession, Provider,
    ProviderError, ProviderIdentity, RecoveryMarker, SecretStore, ServerSession, StoreError,
};

use super::contract::{Account, Event};

pub type SharedTrace = Rc<RefCell<Vec<Event>>>;

pub fn new_trace() -> SharedTrace {
    Rc::new(RefCell::new(Vec::new()))
}

pub fn clear_trace(trace: &SharedTrace) {
    trace.borrow_mut().clear();
}

pub struct FakeClock {
    now_ms: u64,
}

impl FakeClock {
    pub fn new(now_ms: u64) -> Self {
        Self { now_ms }
    }

    pub fn advance_ms(&mut self, elapsed: u64) {
        self.now_ms = self.now_ms.saturating_add(elapsed);
    }

    pub fn set_ms(&mut self, timestamp_ms: u64) {
        self.now_ms = timestamp_ms;
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now_ms
    }
}

pub struct FakeProvider;

impl Provider for FakeProvider {
    fn exchange_code(
        &mut self,
        code: &str,
        code_verifier: &str,
    ) -> Result<ProviderIdentity, ProviderError> {
        if code_verifier.len() < 43 {
            return Err(ProviderError::InvalidCode);
        }
        let (subject_hash, nonce) = match code {
            "code_a" => ("subject-hash-a", "nonce-a"),
            "code_b" => ("subject-hash-b", "nonce-b"),
            _ => return Err(ProviderError::InvalidCode),
        };
        Ok(ProviderIdentity {
            subject_hash: subject_hash.to_owned(),
            issuer: "https://provider.invalid".to_owned(),
            audiences: vec!["kohbor-synthetic".to_owned()],
            nonce: nonce.to_owned(),
            expires_at_ms: u64::MAX,
            signature_valid: true,
        })
    }
}

#[derive(Clone)]
struct BrokerRow {
    value: String,
    owners: Vec<String>,
}

pub struct FakeBroker {
    trace: SharedTrace,
    rows: Vec<BrokerRow>,
    available: bool,
    fail_revoke: bool,
    fail_delete: bool,
    delete_count: usize,
    request_count: usize,
    next_request_id: u64,
    pending_replies: BTreeMap<u64, Result<Vec<String>, BrokerError>>,
    held_next_reply: Option<String>,
    last_pending_request: Option<u64>,
    last_friend_account: Option<String>,
    friends: BTreeMap<String, Vec<String>>,
}

impl FakeBroker {
    pub fn new(trace: SharedTrace) -> Self {
        Self {
            trace,
            rows: Vec::new(),
            available: true,
            fail_revoke: false,
            fail_delete: false,
            delete_count: 0,
            request_count: 0,
            next_request_id: 1,
            pending_replies: BTreeMap::new(),
            held_next_reply: None,
            last_pending_request: None,
            last_friend_account: None,
            friends: BTreeMap::new(),
        }
    }

    pub fn delete_count(&self) -> usize {
        self.delete_count
    }

    pub fn set_available(&mut self, available: bool) {
        self.available = available;
    }

    pub fn set_fail_revoke(&mut self) {
        self.fail_revoke = true;
    }

    pub fn set_fail_delete(&mut self) {
        self.fail_delete = true;
    }

    pub fn hold_next_friend_reply(&mut self, value: &str) {
        self.held_next_reply = Some(value.to_owned());
        self.last_pending_request = None;
    }

    pub fn last_pending_request(&self) -> Option<u64> {
        self.last_pending_request
    }

    pub fn last_friend_account(&self) -> Option<&str> {
        self.last_friend_account.as_deref()
    }

    pub fn request_count(&self) -> usize {
        self.request_count
    }

    pub fn reset_request_count(&mut self) {
        self.request_count = 0;
    }

    pub fn seed_delete_graph(&mut self) {
        self.add_row("account:acct_a", &["acct_a"]);
        self.add_row("identity:acct_a", &["acct_a"]);
        self.add_row("session:acct_a", &["acct_a"]);
        self.add_row("request:acct_a->acct_b", &["acct_a", "acct_b"]);
        self.add_row("request:acct_b->acct_a", &["acct_a", "acct_b"]);
        self.add_row("friendship:acct_b<->acct_a", &["acct_a", "acct_b"]);
        self.add_row("block:acct_b->acct_a", &["acct_a", "acct_b"]);
        self.add_row("share:acct_a", &["acct_a"]);
        self.add_row("pairing:acct_a", &["acct_a"]);
        self.add_row("rate-limit:acct_a", &["acct_a"]);
        self.add_row("tombstone:acct_a", &["acct_a"]);
        self.add_row("account:acct_b", &["acct_b"]);
        self.add_row("share:acct_b", &["acct_b"]);
        self.friends
            .insert("acct_a".to_owned(), vec!["friend_a".to_owned()]);
        self.friends
            .insert("acct_b".to_owned(), vec!["friend_b".to_owned()]);
    }

    pub fn rows_for(&self, account: Account) -> Vec<String> {
        let id = match account {
            Account::A => "acct_a",
            Account::B => "acct_b",
        };
        self.rows
            .iter()
            .filter(|row| {
                row.owners.iter().any(|owner| owner == id)
                    && (account != Account::B || !row.owners.iter().any(|owner| owner == "acct_a"))
            })
            .map(|row| row.value.clone())
            .collect()
    }

    fn add_row(&mut self, value: &str, owners: &[&str]) {
        if self.rows.iter().any(|row| row.value == value) {
            return;
        }
        self.rows.push(BrokerRow {
            value: value.to_owned(),
            owners: owners.iter().map(|owner| (*owner).to_owned()).collect(),
        });
    }

    fn append_session_rows(&mut self, account_id: &str) {
        self.add_row(&format!("account:{account_id}"), &[account_id]);
        self.add_row(&format!("session:{account_id}"), &[account_id]);
    }
}

impl Broker for FakeBroker {
    fn sign_in(
        &mut self,
        identity: &ProviderIdentity,
        _now_ms: u64,
    ) -> Result<ServerSession, BrokerError> {
        self.request_count += 1;
        if !self.available {
            return Err(BrokerError::Unavailable);
        }
        let session = match identity.subject_hash.as_str() {
            "subject-hash-a" => ServerSession {
                account_id: "acct_a".to_owned(),
                refresh_token: "refresh_a".to_owned(),
                device_key: "device_a".to_owned(),
                friends: self
                    .friends
                    .get("acct_a")
                    .cloned()
                    .unwrap_or_else(|| vec!["friend_a".to_owned()]),
                pairing_ids: vec!["pair_a_1".to_owned(), "pair_a_2".to_owned()],
                private_item_labels: vec!["cache_a".to_owned(), "pair_secret_a".to_owned()],
            },
            "subject-hash-b" => ServerSession {
                account_id: "acct_b".to_owned(),
                refresh_token: "refresh_b".to_owned(),
                device_key: "device_b".to_owned(),
                friends: self
                    .friends
                    .get("acct_b")
                    .cloned()
                    .unwrap_or_else(|| vec!["friend_b".to_owned()]),
                pairing_ids: vec!["pair_b_1".to_owned()],
                private_item_labels: vec!["cache_b".to_owned(), "pair_secret_b".to_owned()],
            },
            _ => return Err(BrokerError::Rejected),
        };
        self.append_session_rows(&session.account_id);
        self.friends
            .insert(session.account_id.clone(), session.friends.clone());
        Ok(session)
    }

    fn validate_session(
        &mut self,
        _account_id: &str,
        _refresh_token: &str,
        _device_key: &str,
    ) -> Result<(), BrokerError> {
        self.request_count += 1;
        if self.available {
            Ok(())
        } else {
            Err(BrokerError::Unavailable)
        }
    }

    fn begin_friend_list(&mut self, account_id: &str) -> BrokerReply<Vec<String>> {
        self.request_count += 1;
        self.last_friend_account = Some(account_id.to_owned());
        if !self.available {
            return BrokerReply::Ready(Err(BrokerError::Unavailable));
        }
        if let Some(value) = self.held_next_reply.take() {
            let request_id = self.next_request_id;
            self.next_request_id = self.next_request_id.saturating_add(1);
            self.pending_replies.insert(request_id, Ok(vec![value]));
            self.last_pending_request = Some(request_id);
            return BrokerReply::Pending(request_id);
        }
        BrokerReply::Ready(Ok(self
            .friends
            .get(account_id)
            .cloned()
            .unwrap_or_default()))
    }

    fn take_friend_reply(&mut self, request_id: u64) -> Option<Result<Vec<String>, BrokerError>> {
        self.pending_replies.remove(&request_id)
    }

    fn mutate_friend(
        &mut self,
        _account_id: &str,
        _operation: &str,
        _friend_code: &str,
    ) -> Result<(), BrokerError> {
        self.request_count += 1;
        if self.available {
            Ok(())
        } else {
            Err(BrokerError::Unavailable)
        }
    }

    fn leave_pairing(&mut self, account_id: &str, pairing_id: &str) -> Result<(), BrokerError> {
        self.request_count += 1;
        self.trace
            .borrow_mut()
            .push(Event::Leave(static_id(pairing_id)));
        if self.available {
            Ok(())
        } else {
            let _ = account_id;
            Err(BrokerError::Unavailable)
        }
    }

    fn revoke_session(&mut self, account_id: &str) -> Result<(), BrokerError> {
        self.request_count += 1;
        self.trace
            .borrow_mut()
            .push(Event::Revoke(contract_account(account_id)));
        if self.fail_revoke {
            Err(BrokerError::InjectedFailure)
        } else if self.available {
            Ok(())
        } else {
            Err(BrokerError::Unavailable)
        }
    }

    fn delete_account(&mut self, account_id: &str) -> Result<(), BrokerError> {
        self.delete_count += 1;
        self.request_count += 1;
        if self.fail_delete {
            return Err(BrokerError::InjectedFailure);
        }
        if !self.available {
            return Err(BrokerError::Unavailable);
        }
        self.rows
            .retain(|row| !row.owners.iter().any(|owner| owner == account_id));
        self.friends.remove(account_id);
        Ok(())
    }
}

pub struct FakeSecretStore {
    sessions: BTreeMap<String, PersistedSession>,
    active_account: Option<String>,
    trace: SharedTrace,
    writes: usize,
    unreadable: bool,
    recovery_marker: RecoveryMarker,
    fail_wipe: bool,
}

impl FakeSecretStore {
    pub fn new(trace: SharedTrace) -> Self {
        Self {
            sessions: BTreeMap::new(),
            active_account: None,
            trace,
            writes: 0,
            unreadable: false,
            recovery_marker: RecoveryMarker::Clear,
            fail_wipe: false,
        }
    }

    pub fn set_fail_wipe(&mut self, fail: bool) {
        self.fail_wipe = fail;
    }

    pub fn seed_session(&mut self, session: PersistedSession) {
        self.active_account = Some(session.account_id.clone());
        self.sessions.insert(session.account_id.clone(), session);
    }

    pub fn reset_write_count(&mut self) {
        self.writes = 0;
    }

    pub fn write_count(&self) -> usize {
        self.writes
    }

    pub fn account_items(&self, account_id: &str) -> Vec<String> {
        let Some(session) = self.sessions.get(account_id) else {
            return Vec::new();
        };
        let mut items = vec![session.refresh_token.clone(), session.device_key.clone()];
        items.extend(session.private_item_labels.iter().cloned());
        items.sort();
        items
    }

    pub fn set_unreadable(&mut self, unreadable: bool) {
        self.unreadable = unreadable;
    }
}

impl SecretStore for FakeSecretStore {
    fn load_recovery_marker(&self) -> Result<RecoveryMarker, StoreError> {
        Ok(self.recovery_marker.clone())
    }

    fn write_recovery_marker(&mut self, marker: &RecoveryMarker) -> Result<(), StoreError> {
        // Fake replacements are atomic: an error leaves the previous durable
        // marker intact, matching the contract required of a host adapter.
        self.recovery_marker = marker.clone();
        Ok(())
    }

    fn load_session(&self) -> Result<Option<PersistedSession>, StoreError> {
        if self.unreadable {
            return Err(StoreError::Unreadable);
        }
        Ok(self
            .active_account
            .as_ref()
            .and_then(|account_id| self.sessions.get(account_id))
            .cloned())
    }

    fn write_session(&mut self, session: &PersistedSession) -> Result<(), StoreError> {
        self.writes += 1;
        self.active_account = Some(session.account_id.clone());
        self.sessions
            .insert(session.account_id.clone(), session.clone());
        Ok(())
    }

    fn write_friends(&mut self, account_id: &str, friends: &[String]) -> Result<(), StoreError> {
        self.writes += 1;
        let Some(session) = self.sessions.get_mut(account_id) else {
            return Err(StoreError::Unwritable);
        };
        session.friends = friends.to_vec();
        Ok(())
    }

    fn wipe_account(&mut self, account_id: &str) -> Result<(), StoreError> {
        if self.fail_wipe {
            return Err(StoreError::Unwritable);
        }
        let session = self.sessions.remove(account_id);
        if self.active_account.as_deref() == Some(account_id) {
            self.active_account = None;
        }
        if let Some(session) = session {
            for item in self.items_from(&session) {
                self.trace.borrow_mut().push(Event::Wipe(static_id(&item)));
            }
        }
        Ok(())
    }
}

impl FakeSecretStore {
    fn items_from(&self, session: &PersistedSession) -> Vec<String> {
        let mut items = vec![session.refresh_token.clone(), session.device_key.clone()];
        items.extend(session.private_item_labels.iter().cloned());
        items
    }
}

pub struct TraceObserver {
    trace: SharedTrace,
}

impl TraceObserver {
    pub fn new(trace: SharedTrace) -> Self {
        Self { trace }
    }
}

impl ClientObserver for TraceObserver {
    fn generation_advanced(&mut self, generation: u64) {
        self.trace.borrow_mut().push(Event::Generation(generation));
    }
}

fn contract_account(account_id: &str) -> Account {
    match account_id {
        "acct_b" => Account::B,
        _ => Account::A,
    }
}

fn static_id(value: &str) -> &'static str {
    match value {
        "pair_a_1" => "pair_a_1",
        "pair_a_2" => "pair_a_2",
        "pair_b_1" => "pair_b_1",
        "refresh_a" => "refresh_a",
        "device_a" => "device_a",
        "cache_a" => "cache_a",
        "pair_secret_a" => "pair_secret_a",
        "refresh_b" => "refresh_b",
        "device_b" => "device_b",
        "cache_b" => "cache_b",
        "pair_secret_b" => "pair_secret_b",
        _ => "<unrecognized-boundary-item>",
    }
}
