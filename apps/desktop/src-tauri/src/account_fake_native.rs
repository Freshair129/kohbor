use super::{
    is_plain_directory, is_plain_file, BrowserSignInError, BrowserSignInRequest, DesktopAccountHost,
};
use kohbor_account::{
    AuthMaterial, Broker, BrokerError, BrokerReply, CallbackOutcome, Client, ClientObserver, Clock,
    OperationOutcome, PersistedSession, Provider, ProviderError, ProviderIdentity, RecoveryMarker,
    SecretStore, ServerSession, ServiceConfig,
};
use serde::Serialize;
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Manager, State};

const PROFILE_MARKER: &str = ".kohbor-s1c-native-fake-profile";
const PROFILE_MARKER_CONTENT: &str = "KOHBOR_S1C_NATIVE_FAKE_PROFILE_V1";
const AUTH_STATE: &str = "s1c-native-fake-state";
const AUTH_NONCE: &str = "s1c-native-fake-nonce";
const CODE_VERIFIER: &str = "s1c-native-fake-code-verifier-abcdefghijklmnopqrstuvwxyz-0123456789";
const SYNTHETIC_CODE: &str = "code_a";
const TEST_ACCOUNT_ID: &str = "acct_a";
const SYNTHETIC_REFRESH_TOKEN: &str = "synthetic-refresh-token";
const SYNTHETIC_DEVICE_KEY: &str = "synthetic-device-key";
const SYNTHETIC_FRIEND_ID: &str = "friend_a";
const SYNTHETIC_PAIRING_ID: &str = "pair_a_1";
const SYNTHETIC_CACHE_LABEL: &str = "synthetic-cache";
const TEST_DISPLAY_NAME_LABEL_PREFIX: &str = "s1c-native-demo-display-name:v1:";

/// A receipt from one actual local `DesktopAccountHost` browser-callback flow.
/// It deliberately contains no credential, account identifier, or filesystem path.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountFakeNativeReceipt {
    operation: &'static str,
    outcome: &'static str,
    host_state: &'static str,
    provider_exchanges: usize,
    broker_sign_ins: usize,
    broker_pairing_leaves: Option<usize>,
    broker_session_revocations: Option<usize>,
    broker_account_deletions: Option<usize>,
    recovery_marker_clear: Option<bool>,
    session_persisted: Option<bool>,
    broker_name_acceptances: Option<usize>,
    display_name_persisted: Option<bool>,
    display_name_read_back: Option<bool>,
    display_name: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AccountFakeNativeAction {
    Check,
    SignIn,
    SaveDisplayName,
    ReadDisplayName,
    SignOut,
    DeleteAccount,
}

/// Test-only account command. Only the display-name save accepts renderer
/// text; the callback, provider, broker, and profile root are fixed or managed.
#[tauri::command]
pub(crate) fn account_fake_native(
    action: AccountFakeNativeAction,
    display_name: Option<String>,
    confirmed: Option<bool>,
    host: State<'_, DesktopAccountHost>,
    app: State<'_, crate::AppState>,
    window: tauri::WebviewWindow<tauri::Wry>,
) -> Result<AccountFakeNativeReceipt, &'static str> {
    if window.label() != "control" {
        return Err("ACCOUNT_FAKE_CONTROL_WINDOW_REQUIRED");
    }
    fake_profile_is_armed(&host, &window)?;

    match action {
        AccountFakeNativeAction::Check => {
            if display_name.is_some() || confirmed.is_some() {
                return Err("ACCOUNT_FAKE_UNEXPECTED_DISPLAY_NAME");
            }
            // Keep the initial availability probe account-storage-free.
            Ok(AccountFakeNativeReceipt {
                operation: "profile_check",
                outcome: "ready",
                host_state: "fake_gate_ready",
                provider_exchanges: 0,
                broker_sign_ins: 0,
                broker_pairing_leaves: None,
                broker_session_revocations: None,
                broker_account_deletions: None,
                recovery_marker_clear: None,
                session_persisted: None,
                broker_name_acceptances: None,
                display_name_persisted: None,
                display_name_read_back: None,
                display_name: None,
            })
        }
        AccountFakeNativeAction::SignIn => {
            if display_name.is_some() || confirmed.is_some() {
                return Err("ACCOUNT_FAKE_UNEXPECTED_DISPLAY_NAME");
            }
            sign_in_fake_account(&host, &app)
        }
        AccountFakeNativeAction::SaveDisplayName => {
            if confirmed.is_some() {
                return Err("ACCOUNT_FAKE_UNEXPECTED_CONFIRMATION");
            }
            let Some(display_name) = display_name else {
                return Err("ACCOUNT_FAKE_DISPLAY_NAME_REQUIRED");
            };
            save_fake_display_name(&host, &app, &window, &display_name)
        }
        AccountFakeNativeAction::ReadDisplayName => {
            if display_name.is_some() || confirmed.is_some() {
                return Err("ACCOUNT_FAKE_UNEXPECTED_DISPLAY_NAME");
            }
            read_fake_display_name(&host, &app, &window)
        }
        AccountFakeNativeAction::SignOut => {
            if display_name.is_some() || confirmed.is_some() {
                return Err("ACCOUNT_FAKE_UNEXPECTED_SIGN_OUT_INPUT");
            }
            sign_out_fake_account(&host, &app)
        }
        AccountFakeNativeAction::DeleteAccount => {
            if display_name.is_some() || confirmed != Some(true) {
                return Err("ACCOUNT_FAKE_DELETE_CONFIRMATION_REQUIRED");
            }
            delete_fake_account(&host, &app)
        }
    }
}

fn sign_in_fake_account(
    host: &DesktopAccountHost,
    app: &crate::AppState,
) -> Result<AccountFakeNativeReceipt, &'static str> {
    let in_live_match = {
        let runtime = crate::intake::lock(&app.runtime.shared);
        runtime.phase == Some(kohbor_gsi::Phase::Live)
    };
    if in_live_match {
        return Err("ACCOUNT_FAKE_SIGN_IN_BLOCKED_IN_MATCH");
    }

    let profile_state = host
        .with_operation(|storage| (storage.load_recovery_marker(), storage.load_session()))
        .map_err(|_| "ACCOUNT_FAKE_PROFILE_UNAVAILABLE")?;
    match profile_state {
        (Ok(RecoveryMarker::Clear), Ok(None)) => {}
        _ => return Err("ACCOUNT_FAKE_PROFILE_NOT_EMPTY"),
    }

    let service = ServiceConfig {
        issuer: "https://provider.invalid".to_owned(),
        audience: "kohbor-synthetic".to_owned(),
    };
    let material = AuthMaterial {
        state: AUTH_STATE.to_owned(),
        nonce: AUTH_NONCE.to_owned(),
        code_verifier: CODE_VERIFIER.to_owned(),
    };
    let cancelled = AtomicBool::new(false);
    let request = BrowserSignInRequest {
        material,
        service: &service,
        in_live_match,
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
    };
    let clock = SyntheticClock;
    let mut provider = SyntheticProvider::default();
    let mut broker = SyntheticBroker::default();
    let callback_worker = Arc::new(Mutex::new(None));
    let worker_slot = Arc::clone(&callback_worker);

    let outcome = host.sign_in_via_browser(
        &mut Client::new(),
        request,
        &clock,
        &mut provider,
        &mut broker,
        move |authorization, address| {
            if authorization.state != AUTH_STATE
                || authorization.scope != "openid"
                || authorization.code_challenge_method != "S256"
                || authorization.code_challenge.is_empty()
            {
                return Err(BrowserSignInError::BrowserLaunch);
            }

            let worker = thread::Builder::new()
                .name("account-fake-callback".to_owned())
                .spawn(move || send_synthetic_callback(address))
                .map_err(|_| BrowserSignInError::BrowserLaunch)?;
            *worker_slot
                .lock()
                .map_err(|_| BrowserSignInError::BrowserLaunch)? = Some(worker);
            Ok(())
        },
    );

    let callback_finished = callback_worker
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .and_then(|worker| worker.join().ok())
        .unwrap_or(false);
    let stored_state = host
        .with_operation(|storage| (storage.load_recovery_marker(), storage.load_session()))
        .map_err(|_| "ACCOUNT_FAKE_HOST_UNAVAILABLE")?;
    let session_persisted = matches!(
        stored_state,
        (Ok(RecoveryMarker::Clear), Ok(Some(ref session)))
            if is_expected_synthetic_session(session)
    );

    if outcome != Ok(CallbackOutcome::Accepted)
        || !callback_finished
        || provider.exchanges != 1
        || broker.sign_ins != 1
        || !session_persisted
    {
        return Err("ACCOUNT_FAKE_SIGN_IN_FAILED");
    }

    crate::log_info!(
        "account",
        "fake_native_sign_in",
        "operation=fake_sign_in outcome=accepted"
    );

    Ok(AccountFakeNativeReceipt {
        operation: "fake_sign_in",
        outcome: "accepted",
        host_state: "signed_in_online",
        provider_exchanges: provider.exchanges,
        broker_sign_ins: broker.sign_ins,
        broker_pairing_leaves: None,
        broker_session_revocations: None,
        broker_account_deletions: None,
        recovery_marker_clear: Some(true),
        session_persisted: Some(session_persisted),
        broker_name_acceptances: None,
        display_name_persisted: None,
        display_name_read_back: None,
        display_name: None,
    })
}

fn save_fake_display_name(
    host: &DesktopAccountHost,
    app: &crate::AppState,
    window: &tauri::WebviewWindow<tauri::Wry>,
    raw_display_name: &str,
) -> Result<AccountFakeNativeReceipt, &'static str> {
    let display_name =
        normalize_demo_display_name(raw_display_name).ok_or("ACCOUNT_FAKE_DISPLAY_NAME_INVALID")?;
    require_fake_gsi_idle(app)?;

    let mut broker = SyntheticBroker::default();
    host.with_operation(|storage| {
        let marker = storage
            .load_recovery_marker()
            .map_err(|_| "ACCOUNT_FAKE_PROFILE_UNAVAILABLE")?;
        if marker != RecoveryMarker::Clear {
            return Err("ACCOUNT_FAKE_PROFILE_NOT_EMPTY");
        }
        let mut session = storage
            .load_session()
            .map_err(|_| "ACCOUNT_FAKE_PROFILE_UNAVAILABLE")?
            .ok_or("ACCOUNT_FAKE_EXPECTED_SESSION_REQUIRED")?;
        if !is_expected_synthetic_session(&session) {
            return Err("ACCOUNT_FAKE_EXPECTED_SESSION_REQUIRED");
        }
        broker.accept_display_name(&mut session, &display_name)?;
        storage
            .write_session(&session)
            .map_err(|_| "ACCOUNT_FAKE_DISPLAY_NAME_PERSIST_FAILED")
    })
    .map_err(|_| "ACCOUNT_FAKE_HOST_UNAVAILABLE")??;

    if broker.name_acceptances != 1 {
        return Err("ACCOUNT_FAKE_DISPLAY_NAME_REJECTED");
    }

    // Recheck all fake gates and reopen account storage through a new guard
    // before reporting that the user's name was accepted.
    fake_profile_is_armed(host, window)?;
    require_fake_gsi_idle(app)?;
    let readback = load_expected_synthetic_session(host)?;
    let readback_name =
        persisted_display_name(&readback).ok_or("ACCOUNT_FAKE_DISPLAY_NAME_READBACK_FAILED")?;
    if readback_name != display_name {
        return Err("ACCOUNT_FAKE_DISPLAY_NAME_READBACK_FAILED");
    }

    crate::log_info!(
        "account",
        "fake_native_display_name_saved",
        "operation=save_display_name outcome=accepted broker=1 persisted=true readback=true"
    );

    Ok(AccountFakeNativeReceipt {
        operation: "save_display_name",
        outcome: "accepted",
        host_state: "signed_in_online",
        provider_exchanges: 0,
        broker_sign_ins: 0,
        broker_pairing_leaves: None,
        broker_session_revocations: None,
        broker_account_deletions: None,
        recovery_marker_clear: Some(true),
        session_persisted: Some(true),
        broker_name_acceptances: Some(broker.name_acceptances),
        display_name_persisted: Some(true),
        display_name_read_back: Some(true),
        display_name: Some(readback_name.to_owned()),
    })
}

fn read_fake_display_name(
    host: &DesktopAccountHost,
    app: &crate::AppState,
    window: &tauri::WebviewWindow<tauri::Wry>,
) -> Result<AccountFakeNativeReceipt, &'static str> {
    require_fake_gsi_idle(app)?;
    fake_profile_is_armed(host, window)?;
    let session = load_expected_synthetic_session(host)?;
    let display_name =
        persisted_display_name(&session).ok_or("ACCOUNT_FAKE_DISPLAY_NAME_NOT_FOUND")?;

    crate::log_info!(
        "account",
        "fake_native_display_name_read",
        "operation=read_display_name outcome=accepted persisted=true readback=true"
    );

    Ok(AccountFakeNativeReceipt {
        operation: "read_display_name",
        outcome: "accepted",
        host_state: "signed_in_online",
        provider_exchanges: 0,
        broker_sign_ins: 0,
        broker_pairing_leaves: None,
        broker_session_revocations: None,
        broker_account_deletions: None,
        recovery_marker_clear: Some(true),
        session_persisted: Some(true),
        broker_name_acceptances: Some(0),
        display_name_persisted: Some(true),
        display_name_read_back: Some(true),
        display_name: Some(display_name.to_owned()),
    })
}

fn sign_out_fake_account(
    host: &DesktopAccountHost,
    app: &crate::AppState,
) -> Result<AccountFakeNativeReceipt, &'static str> {
    require_fake_gsi_idle(app)?;

    let (pairing_leaves, session_revocations) = host
        .with_operation(|storage| -> Result<(usize, usize), &'static str> {
            let marker = storage
                .load_recovery_marker()
                .map_err(|_| "ACCOUNT_FAKE_PROFILE_UNAVAILABLE")?;
            if marker != RecoveryMarker::Clear {
                return Err("ACCOUNT_FAKE_PROFILE_NOT_READY");
            }
            let session = storage
                .load_session()
                .map_err(|_| "ACCOUNT_FAKE_PROFILE_UNAVAILABLE")?
                .ok_or("ACCOUNT_FAKE_EXPECTED_SESSION_REQUIRED")?;
            if !is_expected_synthetic_session(&session) {
                return Err("ACCOUNT_FAKE_EXPECTED_SESSION_REQUIRED");
            }

            let expected_pairing_leaves = session.pairing_ids.len();
            let mut client = Client::new();
            let mut broker = SyntheticBroker::default();
            let mut observer = SyntheticClientObserver;
            let outcome = client.sign_out(&mut broker, storage, &mut observer);
            let marker_after = storage
                .load_recovery_marker()
                .map_err(|_| "ACCOUNT_FAKE_POSTCONDITION_UNAVAILABLE")?;
            let session_after = storage
                .load_session()
                .map_err(|_| "ACCOUNT_FAKE_POSTCONDITION_UNAVAILABLE")?;
            let recovery_marker_clear = marker_after == RecoveryMarker::Clear;
            let session_absent = session_after.is_none();

            if outcome != OperationOutcome::Accepted
                || !recovery_marker_clear
                || !session_absent
                || broker.sign_ins != 0
                || broker.pairing_leaves != expected_pairing_leaves
                || broker.session_revocations != 1
                || broker.account_deletions != 0
            {
                crate::log_info!(
                    "account",
                    "fake_native_sign_out",
                    format!(
                        "operation=sign_out outcome=failed provider_exchanges=0 broker_sign_ins=0 broker_pairing_leaves={} broker_session_revocations={} broker_account_deletions={} recovery_marker_clear={}",
                        broker.pairing_leaves,
                        broker.session_revocations,
                        broker.account_deletions,
                        recovery_marker_clear
                    )
                );
                return Err("ACCOUNT_FAKE_SIGN_OUT_FAILED");
            }

            Ok((broker.pairing_leaves, broker.session_revocations))
        })
        .map_err(|_| "ACCOUNT_FAKE_HOST_UNAVAILABLE")??;

    crate::log_info!(
        "account",
        "fake_native_sign_out",
        format!(
            "operation=sign_out outcome=accepted provider_exchanges=0 broker_sign_ins=0 broker_pairing_leaves={} broker_session_revocations={} broker_account_deletions=0 recovery_marker_clear=true session_present=false",
            pairing_leaves, session_revocations
        )
    );

    Ok(AccountFakeNativeReceipt {
        operation: "sign_out",
        outcome: "accepted",
        host_state: "signed_out",
        provider_exchanges: 0,
        broker_sign_ins: 0,
        broker_pairing_leaves: Some(pairing_leaves),
        broker_session_revocations: Some(session_revocations),
        broker_account_deletions: Some(0),
        recovery_marker_clear: Some(true),
        session_persisted: Some(false),
        broker_name_acceptances: Some(0),
        display_name_persisted: Some(false),
        display_name_read_back: Some(false),
        display_name: None,
    })
}

fn delete_fake_account(
    host: &DesktopAccountHost,
    app: &crate::AppState,
) -> Result<AccountFakeNativeReceipt, &'static str> {
    require_fake_gsi_idle(app)?;

    let service = ServiceConfig {
        issuer: "https://provider.invalid".to_owned(),
        audience: "kohbor-synthetic".to_owned(),
    };
    let clock = SyntheticClock;
    let (provider_exchanges, broker_sign_ins, account_deletions) = host
        .with_operation(|storage| -> Result<(usize, usize, usize), &'static str> {
            let marker = storage
                .load_recovery_marker()
                .map_err(|_| "ACCOUNT_FAKE_PROFILE_UNAVAILABLE")?;
            if marker != RecoveryMarker::Clear {
                return Err("ACCOUNT_FAKE_PROFILE_NOT_READY");
            }
            let persisted_session = storage
                .load_session()
                .map_err(|_| "ACCOUNT_FAKE_PROFILE_UNAVAILABLE")?
                .ok_or("ACCOUNT_FAKE_EXPECTED_SESSION_REQUIRED")?;
            if !is_expected_synthetic_session(&persisted_session) {
                return Err("ACCOUNT_FAKE_EXPECTED_SESSION_REQUIRED");
            }

            // A restored renderer has no live Client freshness state. Perform a
            // fresh synthetic provider callback through Client while holding
            // this same storage guard, preserving the exact current demo labels.
            let mut client = Client::new();
            let mut provider = SyntheticProvider::default();
            let mut broker =
                SyntheticBroker::with_private_item_labels(persisted_session.private_item_labels.clone());
            let material = AuthMaterial {
                state: AUTH_STATE.to_owned(),
                nonce: AUTH_NONCE.to_owned(),
                code_verifier: CODE_VERIFIER.to_owned(),
            };
            if client.begin_handoff(material, &clock, storage).is_err() {
                return Err("ACCOUNT_FAKE_DELETE_REAUTH_FAILED");
            }
            let callback = client.deliver_callback(
                (AUTH_STATE, SYNTHETIC_CODE),
                &service,
                &clock,
                &mut provider,
                &mut broker,
                storage,
            );
            let reauthenticated_marker = storage
                .load_recovery_marker()
                .map_err(|_| "ACCOUNT_FAKE_DELETE_REAUTH_FAILED")?;
            let reauthenticated_session = storage
                .load_session()
                .map_err(|_| "ACCOUNT_FAKE_DELETE_REAUTH_FAILED")?;
            if callback != CallbackOutcome::Accepted
                || provider.exchanges != 1
                || broker.sign_ins != 1
                || reauthenticated_marker != RecoveryMarker::Clear
                || reauthenticated_session.as_ref() != Some(&persisted_session)
            {
                return Err("ACCOUNT_FAKE_DELETE_REAUTH_FAILED");
            }

            let outcome = client.delete_account(&clock, &mut broker, storage);
            let marker_after = storage.load_recovery_marker();
            let session_after = storage.load_session();
            let recovery_marker_clear = matches!(marker_after, Ok(RecoveryMarker::Clear));
            let session_absent = matches!(session_after, Ok(None));
            if outcome != OperationOutcome::Accepted
                || !recovery_marker_clear
                || !session_absent
                || provider.exchanges != 1
                || broker.sign_ins != 1
                || broker.pairing_leaves != 0
                || broker.session_revocations != 0
                || broker.account_deletions != 1
            {
                crate::log_info!(
                    "account",
                    "fake_native_account_delete",
                    format!(
                        "operation=delete_account outcome=failed provider_exchanges={} broker_sign_ins={} broker_pairing_leaves={} broker_session_revocations={} broker_account_deletions={} recovery_marker_clear={}",
                        provider.exchanges,
                        broker.sign_ins,
                        broker.pairing_leaves,
                        broker.session_revocations,
                        broker.account_deletions,
                        recovery_marker_clear
                    )
                );
                return Err("ACCOUNT_FAKE_DELETE_FAILED");
            }

            Ok((provider.exchanges, broker.sign_ins, broker.account_deletions))
        })
        .map_err(|_| "ACCOUNT_FAKE_HOST_UNAVAILABLE")??;

    crate::log_info!(
        "account",
        "fake_native_account_delete",
        format!(
            "operation=delete_account outcome=accepted provider_exchanges={} broker_sign_ins={} broker_pairing_leaves=0 broker_session_revocations=0 broker_account_deletions={} recovery_marker_clear=true session_present=false",
            provider_exchanges, broker_sign_ins, account_deletions
        )
    );

    Ok(AccountFakeNativeReceipt {
        operation: "delete_account",
        outcome: "accepted",
        host_state: "signed_out",
        provider_exchanges,
        broker_sign_ins,
        broker_pairing_leaves: Some(0),
        broker_session_revocations: Some(0),
        broker_account_deletions: Some(account_deletions),
        recovery_marker_clear: Some(true),
        session_persisted: Some(false),
        broker_name_acceptances: Some(0),
        display_name_persisted: Some(false),
        display_name_read_back: Some(false),
        display_name: None,
    })
}

struct SyntheticClientObserver;

impl ClientObserver for SyntheticClientObserver {
    fn generation_advanced(&mut self, _generation: u64) {}
}

fn require_fake_gsi_idle(app: &crate::AppState) -> Result<(), &'static str> {
    let in_live_match = {
        let runtime = crate::intake::lock(&app.runtime.shared);
        runtime.phase == Some(kohbor_gsi::Phase::Live)
    };
    if in_live_match {
        Err("ACCOUNT_FAKE_SIGN_IN_BLOCKED_IN_MATCH")
    } else {
        Ok(())
    }
}

fn normalize_demo_display_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let character_count = trimmed.chars().count();
    if character_count == 0 || character_count > 32 || trimmed.chars().any(char::is_control) {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn is_expected_synthetic_session(session: &PersistedSession) -> bool {
    if session.account_id != TEST_ACCOUNT_ID
        || session.refresh_token != SYNTHETIC_REFRESH_TOKEN
        || session.device_key != SYNTHETIC_DEVICE_KEY
        || session.friends.len() != 1
        || session.friends[0] != SYNTHETIC_FRIEND_ID
        || session.pairing_ids.len() != 1
        || session.pairing_ids[0] != SYNTHETIC_PAIRING_ID
    {
        return false;
    }

    let mut has_cache_label = false;
    let mut display_name_count = 0;
    for label in &session.private_item_labels {
        if label == SYNTHETIC_CACHE_LABEL {
            if has_cache_label {
                return false;
            }
            has_cache_label = true;
        } else if label.starts_with(TEST_DISPLAY_NAME_LABEL_PREFIX) {
            display_name_count += 1;
            let name = &label[TEST_DISPLAY_NAME_LABEL_PREFIX.len()..];
            if normalize_demo_display_name(name).as_deref() != Some(name) {
                return false;
            }
        } else {
            return false;
        }
    }
    has_cache_label && display_name_count <= 1
}

fn persisted_display_name(session: &PersistedSession) -> Option<&str> {
    if !is_expected_synthetic_session(session) {
        return None;
    }
    session.private_item_labels.iter().find_map(|label| {
        label
            .strip_prefix(TEST_DISPLAY_NAME_LABEL_PREFIX)
            .filter(|name| normalize_demo_display_name(name).as_deref() == Some(name))
    })
}

fn load_expected_synthetic_session(
    host: &DesktopAccountHost,
) -> Result<PersistedSession, &'static str> {
    host.with_operation(|storage| {
        let marker = storage
            .load_recovery_marker()
            .map_err(|_| "ACCOUNT_FAKE_PROFILE_UNAVAILABLE")?;
        if marker != RecoveryMarker::Clear {
            return Err("ACCOUNT_FAKE_PROFILE_NOT_EMPTY");
        }
        let session = storage
            .load_session()
            .map_err(|_| "ACCOUNT_FAKE_PROFILE_UNAVAILABLE")?
            .ok_or("ACCOUNT_FAKE_EXPECTED_SESSION_REQUIRED")?;
        if !is_expected_synthetic_session(&session) {
            return Err("ACCOUNT_FAKE_EXPECTED_SESSION_REQUIRED");
        }
        Ok(session)
    })
    .map_err(|_| "ACCOUNT_FAKE_HOST_UNAVAILABLE")?
}

fn fake_profile_is_armed(
    host: &DesktopAccountHost,
    window: &tauri::WebviewWindow<tauri::Wry>,
) -> Result<(), &'static str> {
    if std::env::var("KOHBOR_DEMO").ok().as_deref() != Some("1")
        || std::env::var("KOHBOR_ACCOUNT_FAKE_TEST").ok().as_deref() != Some("1")
    {
        return Err("ACCOUNT_FAKE_TEST_DISABLED");
    }

    let Some(configured_root) = std::env::var_os("KOHBOR_DATA_DIR") else {
        return Err("ACCOUNT_FAKE_DATA_PROFILE_REQUIRED");
    };
    let configured_root = std::path::PathBuf::from(configured_root);
    let Some(configured_root) = canonical_plain_directory(&configured_root) else {
        return Err("ACCOUNT_FAKE_DATA_PROFILE_REQUIRED");
    };
    if is_default_app_data_root(&configured_root) {
        return Err("ACCOUNT_FAKE_DATA_PROFILE_REQUIRED");
    }
    let Some(managed_root) = canonical_plain_directory(&host.app_data_root) else {
        return Err("ACCOUNT_FAKE_DATA_PROFILE_REQUIRED");
    };
    if configured_root != managed_root {
        return Err("ACCOUNT_FAKE_DATA_PROFILE_REQUIRED");
    }

    let Some(requested_webview_root) = std::env::var_os("WEBVIEW2_USER_DATA_FOLDER") else {
        return Err("ACCOUNT_FAKE_WEBVIEW_PROFILE_REQUIRED");
    };
    let Some(webview_root) = canonical_plain_directory(&PathBuf::from(requested_webview_root))
    else {
        return Err("ACCOUNT_FAKE_WEBVIEW_PROFILE_REQUIRED");
    };
    let default_webview_root = match window.app_handle().path().app_local_data_dir() {
        Ok(path) => path,
        Err(_) => return Err("ACCOUNT_FAKE_WEBVIEW_PROFILE_REQUIRED"),
    };
    let Some(default_webview_root) = canonical_directory_or_missing_leaf(&default_webview_root)
    else {
        return Err("ACCOUNT_FAKE_WEBVIEW_PROFILE_REQUIRED");
    };
    if webview_root == configured_root || webview_root == default_webview_root {
        return Err("ACCOUNT_FAKE_WEBVIEW_PROFILE_REQUIRED");
    }

    let marker = configured_root.join(PROFILE_MARKER);
    match fs::symlink_metadata(&marker) {
        Ok(metadata)
            if is_plain_file(&metadata)
                && metadata.len() == PROFILE_MARKER_CONTENT.len() as u64 => {}
        _ => return Err("ACCOUNT_FAKE_TEST_PROFILE_MARKER_REQUIRED"),
    }
    if fs::read_to_string(marker)
        .map(|content| content == PROFILE_MARKER_CONTENT)
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err("ACCOUNT_FAKE_TEST_PROFILE_MARKER_REQUIRED")
    }
}

fn canonical_plain_directory(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_plain_directory(&metadata) => fs::canonicalize(path).ok(),
        _ => None,
    }
}

fn canonical_directory_or_missing_leaf(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_plain_directory(&metadata) => fs::canonicalize(path).ok(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent()?;
            let leaf = path.file_name()?;
            let metadata = fs::symlink_metadata(parent).ok()?;
            if !is_plain_directory(&metadata) {
                return None;
            }
            Some(fs::canonicalize(parent).ok()?.join(leaf))
        }
        _ => None,
    }
}

fn is_default_app_data_root(configured_root: &std::path::Path) -> bool {
    let Some(roaming_root) = std::env::var_os("APPDATA") else {
        return true;
    };
    let roaming_root = match fs::canonicalize(roaming_root) {
        Ok(path) => path,
        Err(_) => return true,
    };
    let default_root = roaming_root.join("io.github.freshair129.kohbor");
    if default_root == configured_root {
        return true;
    }
    match fs::canonicalize(default_root) {
        Ok(path) => path == configured_root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        // If the default cannot be resolved, this check cannot prove that the
        // supplied profile is separate, so keep the fake bridge disabled.
        Err(_) => true,
    }
}

fn send_synthetic_callback(address: SocketAddrV4) -> bool {
    if *address.ip() != Ipv4Addr::LOCALHOST {
        return false;
    }
    let Ok(mut stream) = TcpStream::connect(address) else {
        return false;
    };
    if stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .is_err()
    {
        return false;
    }
    let request = format!(
        "GET /callback?code={SYNTHETIC_CODE}&state={AUTH_STATE} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    // The host only sends a fixed, sanitized acknowledgement. Reading it keeps
    // the local loopback exchange complete without retaining response contents.
    let mut response = Vec::new();
    let read_ok = stream.read_to_end(&mut response).is_ok();
    read_ok
}

struct SyntheticClock;

impl Clock for SyntheticClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
            .unwrap_or(u64::MAX)
    }
}

#[derive(Default)]
struct SyntheticProvider {
    exchanges: usize,
}

impl Provider for SyntheticProvider {
    fn exchange_code(
        &mut self,
        code: &str,
        code_verifier: &str,
    ) -> Result<ProviderIdentity, ProviderError> {
        self.exchanges += 1;
        if code != SYNTHETIC_CODE || code_verifier.len() < 43 {
            return Err(ProviderError::InvalidCode);
        }
        Ok(ProviderIdentity {
            subject_hash: "subject-hash-a".to_owned(),
            issuer: "https://provider.invalid".to_owned(),
            audiences: vec!["kohbor-synthetic".to_owned()],
            nonce: AUTH_NONCE.to_owned(),
            expires_at_ms: u64::MAX,
            signature_valid: true,
        })
    }
}

#[derive(Default)]
struct SyntheticBroker {
    sign_ins: usize,
    name_acceptances: usize,
    pairing_leaves: usize,
    session_revocations: usize,
    account_deletions: usize,
    sign_in_private_item_labels: Option<Vec<String>>,
}

impl Broker for SyntheticBroker {
    fn sign_in(
        &mut self,
        identity: &ProviderIdentity,
        _now_ms: u64,
    ) -> Result<ServerSession, BrokerError> {
        self.sign_ins += 1;
        if identity.subject_hash != "subject-hash-a" {
            return Err(BrokerError::Rejected);
        }
        Ok(ServerSession {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            refresh_token: SYNTHETIC_REFRESH_TOKEN.to_owned(),
            device_key: SYNTHETIC_DEVICE_KEY.to_owned(),
            friends: vec![SYNTHETIC_FRIEND_ID.to_owned()],
            pairing_ids: vec![SYNTHETIC_PAIRING_ID.to_owned()],
            private_item_labels: self
                .sign_in_private_item_labels
                .clone()
                .unwrap_or_else(|| vec![SYNTHETIC_CACHE_LABEL.to_owned()]),
        })
    }

    fn validate_session(
        &mut self,
        _account_id: &str,
        _refresh_token: &str,
        _device_key: &str,
    ) -> Result<(), BrokerError> {
        Err(BrokerError::Unavailable)
    }

    fn begin_friend_list(&mut self, _account_id: &str) -> BrokerReply<Vec<String>> {
        BrokerReply::Ready(Err(BrokerError::Unavailable))
    }

    fn take_friend_reply(&mut self, _request_id: u64) -> Option<Result<Vec<String>, BrokerError>> {
        None
    }

    fn mutate_friend(
        &mut self,
        _account_id: &str,
        _operation: &str,
        _friend_code: &str,
    ) -> Result<(), BrokerError> {
        Err(BrokerError::Unavailable)
    }

    fn leave_pairing(&mut self, account_id: &str, pairing_id: &str) -> Result<(), BrokerError> {
        if account_id != TEST_ACCOUNT_ID || pairing_id != SYNTHETIC_PAIRING_ID {
            return Err(BrokerError::Rejected);
        }
        self.pairing_leaves += 1;
        Ok(())
    }

    fn revoke_session(&mut self, account_id: &str) -> Result<(), BrokerError> {
        if account_id != TEST_ACCOUNT_ID {
            return Err(BrokerError::Rejected);
        }
        self.session_revocations += 1;
        Ok(())
    }

    fn delete_account(&mut self, account_id: &str) -> Result<(), BrokerError> {
        if account_id != TEST_ACCOUNT_ID {
            return Err(BrokerError::Rejected);
        }
        self.account_deletions += 1;
        Ok(())
    }
}

impl SyntheticBroker {
    fn with_private_item_labels(labels: Vec<String>) -> Self {
        Self {
            sign_in_private_item_labels: Some(labels),
            ..Self::default()
        }
    }

    fn accept_display_name(
        &mut self,
        session: &mut PersistedSession,
        display_name: &str,
    ) -> Result<(), &'static str> {
        if normalize_demo_display_name(display_name).as_deref() != Some(display_name)
            || !is_expected_synthetic_session(session)
        {
            return Err("ACCOUNT_FAKE_DISPLAY_NAME_REJECTED");
        }

        let persisted_label = format!("{TEST_DISPLAY_NAME_LABEL_PREFIX}{display_name}");
        if let Some(label) = session
            .private_item_labels
            .iter_mut()
            .find(|label| label.starts_with(TEST_DISPLAY_NAME_LABEL_PREFIX))
        {
            *label = persisted_label;
        } else {
            session.private_item_labels.push(persisted_label);
        }
        self.name_acceptances += 1;
        Ok(())
    }
}
