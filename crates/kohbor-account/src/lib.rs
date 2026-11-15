//! Pure account-client seam for the S1-T synthetic contract.
//!
//! This module has no network, OS, logging, or external-crate dependency.
//! Callers provide the clock, OAuth code exchanger, broker, secret store,
//! handoff randomness, and an observer for boundary evidence.
//!
//! The recovery marker is a typed seam, not a complete durability guarantee.
//! Host stores must make its record independently readable and atomically
//! durable, retain the previous value on failed writes, and serialize session
//! writes against marker reads/updates across processes.

use std::collections::BTreeMap;

pub const HANDOFF_TTL_MS: u64 = 300_000;
pub const DELETE_REAUTH_TTL_MS: u64 = 300_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountState {
    SignedOut,
    BrowserHandoff,
    SigningIn,
    NeedsDisplayName,
    SignedInOnline,
    SignedInOffline,
    ServiceUnavailable,
    SessionRevoked,
    PausedNewDevice,
    Deleting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackOutcome {
    Accepted,
    Rejected,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationOutcome {
    Accepted,
    Rejected,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderError {
    InvalidCode,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrokerError {
    Unavailable,
    Rejected,
    InjectedFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    Unreadable,
    Unwritable,
}

/// Durable intent that gates loading or creating local account sessions.
///
/// `DeletePending` is deliberately unresolved: a process may have stopped
/// before, during, or after the broker request, so recovery must neither
/// reissue the delete nor erase local data without an authoritative result.
/// `WipePending` means local erasure is authorized and may be retried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryMarker {
    Clear,
    DeletePending { account_id: String },
    WipePending { account_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceConfig {
    pub issuer: String,
    pub audience: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthMaterial {
    /// Caller-supplied cryptographically random state value.
    pub state: String,
    /// Caller-supplied nonce sent to the provider and checked on return.
    pub nonce: String,
    /// High-entropy PKCE verifier. Only the derived challenge is returned.
    pub code_verifier: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub state: String,
    pub nonce: String,
    pub scope: &'static str,
    pub code_challenge: String,
    pub code_challenge_method: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderIdentity {
    /// A provider-verified, keyed/opaque subject representation; never a raw
    /// provider subject or email.
    pub subject_hash: String,
    pub issuer: String,
    pub audiences: Vec<String>,
    pub nonce: String,
    pub expires_at_ms: u64,
    pub signature_valid: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerSession {
    pub account_id: String,
    pub refresh_token: String,
    pub device_key: String,
    pub friends: Vec<String>,
    pub pairing_ids: Vec<String>,
    /// Labels for additional account-scoped blobs in the injected store. The
    /// production client does not inspect or log their values.
    pub private_item_labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistedSession {
    pub account_id: String,
    pub refresh_token: String,
    pub device_key: String,
    pub friends: Vec<String>,
    pub pairing_ids: Vec<String>,
    pub private_item_labels: Vec<String>,
}

impl From<ServerSession> for PersistedSession {
    fn from(value: ServerSession) -> Self {
        Self {
            account_id: value.account_id,
            refresh_token: value.refresh_token,
            device_key: value.device_key,
            friends: value.friends,
            pairing_ids: value.pairing_ids,
            private_item_labels: value.private_item_labels,
        }
    }
}

pub trait Clock {
    fn now_ms(&self) -> u64;
}

pub trait Provider {
    fn exchange_code(
        &mut self,
        code: &str,
        code_verifier: &str,
    ) -> Result<ProviderIdentity, ProviderError>;
}

pub enum BrokerReply<T> {
    Ready(Result<T, BrokerError>),
    Pending(u64),
}

pub trait Broker {
    fn sign_in(
        &mut self,
        identity: &ProviderIdentity,
        now_ms: u64,
    ) -> Result<ServerSession, BrokerError>;

    fn validate_session(
        &mut self,
        account_id: &str,
        refresh_token: &str,
        device_key: &str,
    ) -> Result<(), BrokerError>;

    fn begin_friend_list(&mut self, account_id: &str) -> BrokerReply<Vec<String>>;

    fn take_friend_reply(&mut self, request_id: u64) -> Option<Result<Vec<String>, BrokerError>>;

    fn mutate_friend(
        &mut self,
        account_id: &str,
        operation: &str,
        friend_code: &str,
    ) -> Result<(), BrokerError>;

    fn leave_pairing(&mut self, account_id: &str, pairing_id: &str) -> Result<(), BrokerError>;

    fn revoke_session(&mut self, account_id: &str) -> Result<(), BrokerError>;

    /// Delete one account. `Rejected` and `InjectedFailure` are authoritative
    /// no-commit outcomes. `Unavailable` is ambiguous, including a transport
    /// interruption after the broker may have committed; callers must retain
    /// `DeletePending`, neither retry the delete nor wipe local data, and wait
    /// for authoritative reconciliation.
    fn delete_account(&mut self, account_id: &str) -> Result<(), BrokerError>;
}

pub trait SecretStore {
    /// Read a durable recovery record independently of the cached session.
    /// A missing record means `Clear`; an unreadable record must be an error.
    fn load_recovery_marker(&self) -> Result<RecoveryMarker, StoreError>;

    /// Atomically replace and durably commit the recovery record before the
    /// client relies on it. Host stores must preserve the previous committed
    /// value on error and serialize marker updates with session writes.
    fn write_recovery_marker(&mut self, marker: &RecoveryMarker) -> Result<(), StoreError>;

    fn load_session(&self) -> Result<Option<PersistedSession>, StoreError>;

    fn write_session(&mut self, session: &PersistedSession) -> Result<(), StoreError>;

    fn write_friends(&mut self, account_id: &str, friends: &[String]) -> Result<(), StoreError>;

    /// Idempotently and durably remove only this account's local session and
    /// account-owned data. A committed `WipePending` marker stays in place
    /// until this returns success and the client commits `Clear`. Host stores
    /// must serialize marker checks/updates with session persistence across
    /// concurrent clients; the pure client can only recheck before writing.
    fn wipe_account(&mut self, account_id: &str) -> Result<(), StoreError>;
}

pub trait ClientObserver {
    /// Called after the generation changes and before any teardown boundary
    /// operation. Test observers capture this actual client hook in order.
    fn generation_advanced(&mut self, generation: u64);
}

#[derive(Clone, Debug)]
struct PendingHandoff {
    state: String,
    nonce: String,
    code_verifier: String,
    started_at_ms: u64,
    generation: u64,
}

#[derive(Clone, Debug)]
struct PendingFriendReply {
    generation: u64,
    account_id: String,
}

#[derive(Debug)]
pub struct Client {
    state: AccountState,
    generation: u64,
    pending_handoff: Option<PendingHandoff>,
    pending_friend_replies: BTreeMap<u64, PendingFriendReply>,
    session: Option<PersistedSession>,
    friends: Vec<String>,
    last_fresh_signin_ms: Option<u64>,
    recovery_unresolved: bool,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self {
            state: AccountState::SignedOut,
            generation: 0,
            pending_handoff: None,
            pending_friend_replies: BTreeMap::new(),
            session: None,
            friends: Vec::new(),
            last_fresh_signin_ms: None,
            recovery_unresolved: false,
        }
    }

    pub fn state(&self) -> AccountState {
        self.state
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn friends(&self) -> &[String] {
        &self.friends
    }

    /// Start a provider handoff with caller-generated random state, nonce, and
    /// verifier material. The pure client derives the PKCE S256 challenge and
    /// returns only the protocol parameters needed by the host.
    pub fn begin_handoff<C: Clock, S: SecretStore>(
        &mut self,
        material: AuthMaterial,
        clock: &C,
        store: &S,
    ) -> Result<AuthorizationRequest, CallbackOutcome> {
        if self.pending_handoff.is_some()
            || material.state.is_empty()
            || material.nonce.is_empty()
            || material.code_verifier.is_empty()
        {
            return Err(CallbackOutcome::Rejected);
        }
        if self.state == AccountState::Deleting || self.recovery_unresolved {
            self.fence_and_hide_account(AccountState::ServiceUnavailable);
            return Err(CallbackOutcome::Failed);
        }
        match store.load_recovery_marker() {
            Ok(RecoveryMarker::Clear) => {}
            Ok(marker) => {
                self.fence_and_hide_account(marker_blocked_state(&marker));
                return Err(CallbackOutcome::Rejected);
            }
            Err(_) => {
                self.recovery_unresolved = true;
                self.fence_and_hide_account(AccountState::ServiceUnavailable);
                return Err(CallbackOutcome::Failed);
            }
        }
        let request = AuthorizationRequest {
            state: material.state.clone(),
            nonce: material.nonce.clone(),
            scope: "openid",
            code_challenge: pkce_s256(&material.code_verifier),
            code_challenge_method: "S256",
        };
        self.pending_handoff = Some(PendingHandoff {
            state: material.state,
            nonce: material.nonce,
            code_verifier: material.code_verifier,
            started_at_ms: clock.now_ms(),
            generation: self.generation,
        });
        self.state = AccountState::BrowserHandoff;
        Ok(request)
    }

    pub fn cancel_handoff(&mut self) {
        if self.pending_handoff.take().is_some() {
            self.state = AccountState::SignedOut;
        }
    }

    pub fn deliver_callback<C, P, B, S>(
        &mut self,
        (state, code): (&str, &str),
        service: &ServiceConfig,
        clock: &C,
        provider: &mut P,
        broker: &mut B,
        store: &mut S,
    ) -> CallbackOutcome
    where
        C: Clock,
        P: Provider,
        B: Broker,
        S: SecretStore,
    {
        let Some(pending) = self.pending_handoff.take() else {
            return CallbackOutcome::Rejected;
        };
        if self.state != AccountState::BrowserHandoff
            || state != pending.state
            || pending.generation != self.generation
        {
            self.state = AccountState::SignedOut;
            return CallbackOutcome::Rejected;
        }
        if self.recovery_unresolved {
            self.fence_and_hide_account(AccountState::ServiceUnavailable);
            return CallbackOutcome::Failed;
        }
        match store.load_recovery_marker() {
            Ok(RecoveryMarker::Clear) => {}
            Ok(marker) => {
                self.fence_and_hide_account(marker_blocked_state(&marker));
                return CallbackOutcome::Rejected;
            }
            Err(_) => {
                self.recovery_unresolved = true;
                self.fence_and_hide_account(AccountState::ServiceUnavailable);
                return CallbackOutcome::Failed;
            }
        }
        let now_ms = clock.now_ms();
        if now_ms.saturating_sub(pending.started_at_ms) > HANDOFF_TTL_MS {
            self.state = AccountState::SignedOut;
            return CallbackOutcome::Rejected;
        }

        self.state = AccountState::SigningIn;
        let identity = match provider.exchange_code(code, &pending.code_verifier) {
            Ok(identity) => identity,
            Err(_) => {
                self.state = AccountState::SignedOut;
                return CallbackOutcome::Failed;
            }
        };
        let valid_identity = identity.signature_valid
            && identity.issuer == service.issuer
            && identity.audiences.len() == 1
            && identity.audiences.first().map(String::as_str) == Some(service.audience.as_str())
            && identity.nonce == pending.nonce
            && identity.expires_at_ms > now_ms;
        if !valid_identity {
            self.state = AccountState::SignedOut;
            return CallbackOutcome::Rejected;
        }

        let server_session = match broker.sign_in(&identity, now_ms) {
            Ok(session) => session,
            Err(_) => {
                self.state = AccountState::SignedOut;
                return CallbackOutcome::Failed;
            }
        };
        let session = PersistedSession::from(server_session);

        // Re-read after provider/broker work so a handoff started before a
        // teardown marker appeared cannot persist a stale account session.
        // A real host must serialize this check with session writes across
        // processes; this pure client cannot provide that atomic boundary.
        if pending.generation != self.generation || self.recovery_unresolved {
            self.fence_and_hide_account(AccountState::ServiceUnavailable);
            return CallbackOutcome::Failed;
        }
        match store.load_recovery_marker() {
            Ok(RecoveryMarker::Clear) => {}
            Ok(marker) => {
                self.fence_and_hide_account(marker_blocked_state(&marker));
                return CallbackOutcome::Failed;
            }
            Err(_) => {
                self.recovery_unresolved = true;
                self.fence_and_hide_account(AccountState::ServiceUnavailable);
                return CallbackOutcome::Failed;
            }
        }
        if store.write_session(&session).is_err() {
            self.state = AccountState::SignedOut;
            return CallbackOutcome::Failed;
        }

        self.friends = session.friends.clone();
        self.session = Some(session);
        self.last_fresh_signin_ms = Some(now_ms);
        self.state = AccountState::SignedInOnline;
        CallbackOutcome::Accepted
    }

    /// Offline-capable startup. Missing service configuration is an immediate
    /// local return and makes no broker call.
    pub fn start<C, B, S>(
        &mut self,
        service: Option<&ServiceConfig>,
        _clock: &C,
        broker: &mut B,
        store: &mut S,
    ) where
        C: Clock,
        B: Broker,
        S: SecretStore,
    {
        self.fence_and_hide_account(AccountState::SignedOut);
        let marker = match store.load_recovery_marker() {
            Ok(marker) => marker,
            Err(_) => {
                self.recovery_unresolved = true;
                self.state = AccountState::ServiceUnavailable;
                return;
            }
        };
        match marker {
            RecoveryMarker::Clear if self.recovery_unresolved => {
                self.state = AccountState::ServiceUnavailable;
                return;
            }
            RecoveryMarker::Clear => {}
            RecoveryMarker::DeletePending { .. } => {
                self.recovery_unresolved = true;
                self.state = AccountState::Deleting;
                return;
            }
            RecoveryMarker::WipePending { account_id } => {
                if !retry_authorized_wipe(store, &account_id) {
                    self.state = AccountState::SignedOut;
                    return;
                }
                self.recovery_unresolved = false;
                self.state = AccountState::SignedOut;
                return;
            }
        }
        let Some(_service) = service else {
            return;
        };
        let session = match store.load_session() {
            Ok(Some(session)) => session,
            Ok(None) => return,
            Err(_) => {
                self.recovery_unresolved = true;
                self.state = AccountState::ServiceUnavailable;
                return;
            }
        };
        self.friends = session.friends.clone();
        let validation = broker.validate_session(
            &session.account_id,
            &session.refresh_token,
            &session.device_key,
        );
        self.session = Some(session);
        self.state = match validation {
            Ok(()) => AccountState::SignedInOnline,
            Err(BrokerError::Unavailable) => AccountState::SignedInOffline,
            Err(_) => AccountState::ServiceUnavailable,
        };
    }

    pub fn request_friend_change<B: Broker>(
        &mut self,
        broker: &mut B,
        operation: &str,
        friend_code: &str,
    ) -> OperationOutcome {
        let Some(session) = self.session.as_ref() else {
            return OperationOutcome::Rejected;
        };
        if self.state != AccountState::SignedInOnline {
            return OperationOutcome::Rejected;
        }
        match broker.mutate_friend(&session.account_id, operation, friend_code) {
            Ok(()) => OperationOutcome::Accepted,
            Err(BrokerError::Unavailable | BrokerError::Rejected) => OperationOutcome::Rejected,
            Err(BrokerError::InjectedFailure) => OperationOutcome::Failed,
        }
    }

    /// Initiate a real client request. The broker may return a ready result or
    /// a pending request id; pending replies retain the generation in force at
    /// dispatch and are discarded after sign-out or account switch.
    pub fn request_friend_list<B: Broker, S: SecretStore>(
        &mut self,
        broker: &mut B,
        store: &mut S,
    ) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if self.state != AccountState::SignedInOnline {
            return;
        }
        let account_id = session.account_id.clone();
        match broker.begin_friend_list(&account_id) {
            BrokerReply::Ready(Ok(friends)) => {
                self.apply_friends(account_id, friends, store);
            }
            BrokerReply::Ready(Err(_)) => {}
            BrokerReply::Pending(request_id) => {
                self.pending_friend_replies.insert(
                    request_id,
                    PendingFriendReply {
                        generation: self.generation,
                        account_id,
                    },
                );
            }
        }
    }

    pub fn deliver_friend_reply<B: Broker, S: SecretStore>(
        &mut self,
        request_id: u64,
        broker: &mut B,
        store: &mut S,
    ) -> bool {
        let Some(reply) = broker.take_friend_reply(request_id) else {
            return false;
        };
        let Some(pending) = self.pending_friend_replies.remove(&request_id) else {
            return false;
        };
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        if self.state != AccountState::SignedInOnline
            || pending.generation != self.generation
            || pending.account_id != session.account_id
        {
            return false;
        }
        match reply {
            Ok(friends) => {
                self.apply_friends(pending.account_id, friends, store);
                true
            }
            Err(_) => false,
        }
    }

    fn apply_friends<S: SecretStore>(
        &mut self,
        account_id: String,
        friends: Vec<String>,
        store: &mut S,
    ) {
        if store.write_friends(&account_id, &friends).is_ok() {
            self.friends = friends.clone();
            if let Some(session) = self.session.as_mut() {
                if session.account_id == account_id {
                    session.friends = friends;
                }
            }
        }
    }

    /// Fence old account work before attempting remote teardown, then remove
    /// only that account's local secrets and cache. Remote teardown is best
    /// effort; local wipe failures are reported to the caller.
    pub fn sign_out<B, S, O>(
        &mut self,
        broker: &mut B,
        store: &mut S,
        observer: &mut O,
    ) -> OperationOutcome
    where
        B: Broker,
        S: SecretStore,
        O: ClientObserver,
    {
        let current_session = self.session.clone();
        self.generation = self.generation.saturating_add(1);
        self.clear_account_material();
        observer.generation_advanced(self.generation);
        self.state = AccountState::SignedOut;

        let marker = match store.load_recovery_marker() {
            Ok(marker) => marker,
            Err(_) => {
                self.recovery_unresolved = true;
                self.state = AccountState::ServiceUnavailable;
                return OperationOutcome::Failed;
            }
        };
        match marker {
            RecoveryMarker::Clear => {}
            RecoveryMarker::DeletePending { .. } => {
                self.recovery_unresolved = true;
                self.state = AccountState::Deleting;
                return OperationOutcome::Failed;
            }
            RecoveryMarker::WipePending { account_id } => {
                if retry_authorized_wipe(store, &account_id) {
                    self.recovery_unresolved = false;
                    return OperationOutcome::Accepted;
                }
                self.state = AccountState::SignedOut;
                return OperationOutcome::Failed;
            }
        }

        let session = match current_session {
            Some(session) => Some(session),
            None => match store.load_session() {
                Ok(session) => session,
                Err(_) => {
                    self.recovery_unresolved = true;
                    self.state = AccountState::ServiceUnavailable;
                    return OperationOutcome::Failed;
                }
            },
        };

        let Some(session) = session else {
            if self.recovery_unresolved {
                self.state = AccountState::ServiceUnavailable;
                return OperationOutcome::Failed;
            }
            return OperationOutcome::Accepted;
        };

        let pending = RecoveryMarker::WipePending {
            account_id: session.account_id.clone(),
        };
        if store.write_recovery_marker(&pending).is_err() {
            // Do not revoke remotely or wipe locally when the durable intent
            // could not be committed. This process stays blocked; a new
            // process cannot be protected if the host store cannot persist it.
            self.recovery_unresolved = true;
            self.state = AccountState::ServiceUnavailable;
            return OperationOutcome::Failed;
        }
        self.recovery_unresolved = false;

        for pairing_id in &session.pairing_ids {
            let _ = broker.leave_pairing(&session.account_id, pairing_id);
        }
        let _ = broker.revoke_session(&session.account_id);

        if !retry_authorized_wipe(store, &session.account_id) {
            self.state = AccountState::SignedOut;
            return OperationOutcome::Failed;
        }
        OperationOutcome::Accepted
    }

    /// Record a successful fresh provider sign-in through deliver_callback.
    /// The test driver establishes this by running a new synthetic handoff.
    pub fn delete_account<C, B, S>(
        &mut self,
        clock: &C,
        broker: &mut B,
        store: &mut S,
    ) -> OperationOutcome
    where
        C: Clock,
        B: Broker,
        S: SecretStore,
    {
        if self.recovery_unresolved {
            self.fence_and_hide_account(AccountState::ServiceUnavailable);
            return OperationOutcome::Failed;
        }
        match store.load_recovery_marker() {
            Ok(RecoveryMarker::Clear) => {}
            Ok(marker) => {
                self.fence_and_hide_account(marker_blocked_state(&marker));
                return OperationOutcome::Failed;
            }
            Err(_) => {
                self.recovery_unresolved = true;
                self.fence_and_hide_account(AccountState::ServiceUnavailable);
                return OperationOutcome::Failed;
            }
        }
        let Some(session) = self.session.as_ref() else {
            return OperationOutcome::Rejected;
        };
        let Some(recent_auth_ms) = self.last_fresh_signin_ms else {
            return OperationOutcome::Rejected;
        };
        let now_ms = clock.now_ms();
        if now_ms < recent_auth_ms || now_ms.saturating_sub(recent_auth_ms) > DELETE_REAUTH_TTL_MS {
            return OperationOutcome::Rejected;
        }
        let account_id = session.account_id.clone();
        let old_state = self.state;
        let intent = RecoveryMarker::DeletePending {
            account_id: account_id.clone(),
        };
        if store.write_recovery_marker(&intent).is_err() {
            // No broker call was made; retain the old local and in-memory
            // session because deletion is still definitely uncommitted.
            return OperationOutcome::Failed;
        }
        self.state = AccountState::Deleting;
        match broker.delete_account(&account_id) {
            Ok(()) => {
                self.generation = self.generation.saturating_add(1);
                self.clear_account_material();
                self.state = AccountState::Deleting;
                let wipe = RecoveryMarker::WipePending {
                    account_id: account_id.clone(),
                };
                if store.write_recovery_marker(&wipe).is_err() {
                    // The committed delete must not be repeated. Under the
                    // host's atomic-write contract, DeletePending remains and
                    // recovery blocks pending authoritative reconciliation.
                    self.recovery_unresolved = true;
                    return OperationOutcome::Failed;
                }
                self.recovery_unresolved = false;
                if retry_authorized_wipe(store, &account_id) {
                    self.state = AccountState::SignedOut;
                    OperationOutcome::Accepted
                } else {
                    self.state = AccountState::Deleting;
                    OperationOutcome::Failed
                }
            }
            Err(BrokerError::Rejected | BrokerError::InjectedFailure) => {
                if store.write_recovery_marker(&RecoveryMarker::Clear).is_ok() {
                    self.state = old_state;
                } else {
                    self.recovery_unresolved = true;
                    self.generation = self.generation.saturating_add(1);
                    self.clear_account_material();
                    self.state = AccountState::Deleting;
                }
                OperationOutcome::Failed
            }
            Err(BrokerError::Unavailable) => {
                // The broker result is ambiguous. Keep DeletePending durable,
                // hide A in memory, and require future authoritative
                // reconciliation; never retry or wipe from this state.
                self.recovery_unresolved = true;
                self.generation = self.generation.saturating_add(1);
                self.clear_account_material();
                self.state = AccountState::Deleting;
                OperationOutcome::Failed
            }
        }
    }

    fn clear_account_material(&mut self) {
        self.pending_handoff = None;
        self.pending_friend_replies.clear();
        self.session = None;
        self.friends.clear();
        self.last_fresh_signin_ms = None;
    }

    fn fence_and_hide_account(&mut self, blocked_state: AccountState) {
        self.generation = self.generation.saturating_add(1);
        self.clear_account_material();
        self.state = blocked_state;
    }
}

fn marker_blocked_state(marker: &RecoveryMarker) -> AccountState {
    match marker {
        RecoveryMarker::Clear => AccountState::SignedOut,
        RecoveryMarker::DeletePending { .. } => AccountState::Deleting,
        RecoveryMarker::WipePending { .. } => AccountState::SignedOut,
    }
}

/// Retry only local erasure that a committed `WipePending` marker authorizes.
/// The marker stays present if either wipe or marker clear fails.
fn retry_authorized_wipe<S: SecretStore>(store: &mut S, account_id: &str) -> bool {
    if store.wipe_account(account_id).is_err() {
        return false;
    }
    store.write_recovery_marker(&RecoveryMarker::Clear).is_ok()
}

/// OAuth PKCE S256 challenge, encoded as unpadded base64url.
pub fn pkce_s256(verifier: &str) -> String {
    base64url(&sha256(verifier.as_bytes()))
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(c & 0x3f) as usize] as char);
        }
    }
    output
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut padded = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (index, word) in block.chunks_exact(4).take(16).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let x = w[index - 15];
            let y = w[index - 2];
            let sigma0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
            let sigma1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
            w[index] = w[index - 16]
                .wrapping_add(sigma0)
                .wrapping_add(w[index - 7])
                .wrapping_add(sigma1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for index in 0..64 {
            let big1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(big1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let big0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = big0.wrapping_add(majority);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut digest = [0u8; 32];
    for (index, word) in h.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}
