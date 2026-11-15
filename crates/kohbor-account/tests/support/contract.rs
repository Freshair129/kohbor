//! Candidate contract tests, not yet wired to a crate. See ADAPTER.md.
//! All identities and credentials here are synthetic. Proposed ACCOUNT IDs
//! label draft requirements; they do not claim approval.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    SignedOut,
    BrowserHandoff,
    SignedInOffline,
    SignedInOnline,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Account {
    A,
    B,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Accepted,
    Rejected,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Generation(u64),
    Leave(&'static str),
    Revoke(Account),
    Wipe(&'static str),
}

/// Test-only instrumentation; operations must call the real account client.
/// Independent fakes observe its boundaries; they must not enforce the tested rules.
pub trait ContractHarness {
    fn now_ms(&self) -> u64;
    fn advance_ms(&mut self, elapsed: u64);
    fn begin_handoff(&mut self);
    fn cancel_handoff(&mut self);
    fn deliver_callback(&mut self, state: &'static str, code: &'static str) -> Outcome;
    fn expected_state(&self) -> &'static str;
    fn state(&self) -> State;
    fn secret_writes(&self) -> usize;
    fn no_service_configured(&mut self);
    fn seed_offline_session(&mut self);
    fn set_broker_available(&mut self, available: bool);
    fn start(&mut self);
    fn local_core_probe(&self) -> bool;
    fn broker_requests(&self) -> usize;
    fn friends(&self) -> Vec<&'static str>;
    fn request_friend_change(&mut self) -> Outcome;
    fn seed_signed_in_a(&mut self);
    fn generation(&self) -> u64;
    fn events(&self) -> Vec<Event>;
    fn fail_revoke(&mut self);
    fn hold_friend_reply(&mut self, account: Account, value: &'static str);
    fn release_friend_reply(&mut self);
    fn sign_out(&mut self);
    fn switch_to_b(&mut self);
    fn local_core_snapshot(&self) -> Vec<&'static str>;
    fn remaining_a_local_items(&self) -> Vec<&'static str>;
    fn seed_delete_graph(&mut self);
    fn broker_rows(&self, account: Account) -> Vec<&'static str>;
    fn local_account_items(&self) -> Vec<&'static str>;
    fn reauthenticate_at(&mut self, timestamp_ms: u64);
    fn fail_delete(&mut self);
    fn delete_account(&mut self) -> Outcome;
}

/// The future integration calls `s1t_contract_tests!(make_harness)` once.
/// Every invocation of the zero-argument factory must yield an isolated harness.
#[macro_export]
macro_rules! s1t_contract_tests {
    ($make:expr) => {
        use $crate::{Account, ContractHarness as _, Event, Outcome, State};

        #[test]
        fn account_001_cancel_then_late_callback_cannot_store_session() {
            let mut h = ($make)();
            h.begin_handoff();
            assert_eq!(h.state(), State::BrowserHandoff);
            let state = h.expected_state();
            let writes = h.secret_writes();
            h.cancel_handoff();
            assert_eq!(h.state(), State::SignedOut);
            assert_eq!(
                h.deliver_callback(state, "code_cancelled"),
                Outcome::Rejected
            );
            assert_eq!(h.state(), State::SignedOut);
            assert_eq!(h.secret_writes(), writes);
        }

        #[test]
        fn account_001_wrong_state_cannot_store_session() {
            let mut h = ($make)();
            h.begin_handoff();
            let writes = h.secret_writes();
            assert_ne!(h.expected_state(), "wrong_state");
            assert_eq!(
                h.deliver_callback("wrong_state", "code_wrong_state"),
                Outcome::Rejected
            );
            assert_eq!(h.state(), State::SignedOut);
            assert_eq!(h.secret_writes(), writes);
        }

        #[test]
        fn account_001_handoff_times_out_after_proposed_five_minutes() {
            let mut h = ($make)();
            h.begin_handoff();
            let state = h.expected_state();
            let writes = h.secret_writes();
            let before = h.now_ms();
            // Well after the proposed limit; exact equality is deliberately unspecified here.
            h.advance_ms(300_001);
            assert_eq!(h.now_ms(), before + 300_001);
            assert_eq!(h.deliver_callback(state, "code_expired"), Outcome::Rejected);
            assert_eq!(h.state(), State::SignedOut);
            assert_eq!(h.secret_writes(), writes);
        }

        #[test]
        fn core_001_no_service_start_preserves_local_core_without_requests() {
            let mut h = ($make)();
            h.no_service_configured();
            h.start();
            assert!(h.local_core_probe());
            assert_eq!(h.broker_requests(), 0);
        }

        #[test]
        fn account_006_offline_start_exposes_cache_without_sending_mutations() {
            let mut h = ($make)();
            h.seed_offline_session();
            h.set_broker_available(false);
            h.start();
            assert_eq!(h.state(), State::SignedInOffline);
            assert!(h.local_core_probe());
            assert_eq!(h.friends(), vec!["cached_friend"]);
            let requests = h.broker_requests();
            assert_ne!(h.request_friend_change(), Outcome::Accepted);
            assert_eq!(h.broker_requests(), requests);
            assert_eq!(h.friends(), vec!["cached_friend"]);
        }

        #[test]
        fn core_003_signout_fences_late_reply_even_when_revoke_fails() {
            let mut h = ($make)();
            h.seed_signed_in_a();
            let original_core = h.local_core_snapshot();
            let generation = h.generation();
            h.hold_friend_reply(Account::A, "late_a");
            h.fail_revoke();
            h.sign_out();
            assert!(h.generation() > generation);
            let events = h.events();
            assert_eq!(events.first(), Some(&Event::Generation(h.generation())));
            assert!(events.contains(&Event::Leave("pair_a_1")));
            assert!(events.contains(&Event::Leave("pair_a_2")));
            assert!(events.contains(&Event::Revoke(Account::A)));
            for item in ["refresh_a", "device_a", "cache_a", "pair_secret_a"] {
                assert!(
                    events.contains(&Event::Wipe(item)),
                    "missing wipe for {item}"
                );
            }
            assert!(h.remaining_a_local_items().is_empty());
            h.release_friend_reply();
            assert_eq!(h.state(), State::SignedOut);
            assert!(!h.friends().contains(&"late_a"));
            assert_eq!(h.local_core_snapshot(), original_core);
        }

        #[test]
        fn core_003_switch_fences_old_reply_and_preserves_new_account() {
            let mut h = ($make)();
            h.seed_signed_in_a();
            let original_core = h.local_core_snapshot();
            let generation = h.generation();
            h.hold_friend_reply(Account::A, "late_a");
            h.switch_to_b();
            assert!(h.generation() > generation);
            let events = h.events();
            assert_eq!(events.first(), Some(&Event::Generation(h.generation())));
            assert!(events.contains(&Event::Leave("pair_a_1")));
            assert!(events.contains(&Event::Leave("pair_a_2")));
            assert!(events.contains(&Event::Revoke(Account::A)));
            assert!(h.remaining_a_local_items().is_empty());
            assert_eq!(h.state(), State::SignedInOnline);
            assert_eq!(h.friends(), vec!["friend_b"]);
            h.release_friend_reply();
            assert_eq!(h.friends(), vec!["friend_b"]);
            assert_eq!(h.local_core_snapshot(), original_core);
        }

        #[test]
        fn account_008_delete_failure_preserves_local_and_remote_data() {
            let mut h = ($make)();
            h.seed_delete_graph();
            h.reauthenticate_at(h.now_ms());
            let local = h.local_account_items();
            let rows_a = h.broker_rows(Account::A);
            let rows_b = h.broker_rows(Account::B);
            assert!(!local.is_empty() && !rows_a.is_empty() && !rows_b.is_empty());
            h.fail_delete();
            assert_eq!(h.delete_account(), Outcome::Failed);
            assert_eq!(h.local_account_items(), local);
            assert_eq!(h.broker_rows(Account::A), rows_a);
            assert_eq!(h.broker_rows(Account::B), rows_b);
        }

        #[test]
        fn account_008_stale_auth_cannot_delete_or_wipe() {
            let mut h = ($make)();
            h.seed_delete_graph();
            h.reauthenticate_at(h.now_ms());
            h.advance_ms(300_001); // Proposed age; no claim at the equality boundary.
            let local = h.local_account_items();
            let rows_a = h.broker_rows(Account::A);
            let rows_b = h.broker_rows(Account::B);
            assert!(!local.is_empty() && !rows_a.is_empty() && !rows_b.is_empty());
            assert_ne!(h.delete_account(), Outcome::Accepted);
            assert_eq!(h.local_account_items(), local);
            assert_eq!(h.broker_rows(Account::A), rows_a);
            assert_eq!(h.broker_rows(Account::B), rows_b);
        }

        #[test]
        fn account_008_delete_removes_a_graph_without_deleting_b_rows() {
            let mut h = ($make)();
            h.seed_delete_graph();
            h.reauthenticate_at(h.now_ms());
            let b_before = h.broker_rows(Account::B);
            assert!(!h.broker_rows(Account::A).is_empty());
            assert!(!b_before.is_empty());
            assert!(!h.local_account_items().is_empty());
            assert_eq!(h.delete_account(), Outcome::Accepted);
            assert!(h.broker_rows(Account::A).is_empty());
            assert_eq!(h.broker_rows(Account::B), b_before);
            assert!(h.local_account_items().is_empty());
        }
    };
}
