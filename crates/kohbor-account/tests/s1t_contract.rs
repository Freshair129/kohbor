#[path = "../../../apps/desktop/src-tauri/src/account_host.rs"]
mod account_host;
#[path = "support/mod.rs"]
mod support;

pub use support::contract::{Account, ContractHarness, Event, Outcome, State};

fn make_harness() -> impl ContractHarness {
    support::driver::Harness::new_isolated()
}

mod terra_contract_cases {
    use super::make_harness;

    crate::s1t_contract_tests!(make_harness);
}

#[path = "wipe_failure/mod.rs"]
mod wipe_failure;

#[path = "host_recovery/mod.rs"]
mod host_recovery;

#[path = "host_storage_lock/mod.rs"]
mod host_storage_lock;

#[path = "desktop_integration.rs"]
mod desktop_integration;

#[path = "loopback_sign_in/mod.rs"]
mod loopback_sign_in;
