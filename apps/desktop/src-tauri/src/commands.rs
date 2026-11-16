//! Tauri command and protocol adapters. The behavior behind each command lives
//! in the Tauri-independent host and media-service layers.

use crate::audio_prefs::{AudioPrefs, AudioPrefsService};
use crate::hotkeys::{StartFailure, VolumeStep};
use crate::intake::lock;
use crate::{settings, status, AppState};
use serde_json::{Map, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{Emitter as TauriEmitter, Manager};

#[tauri::command]
pub(crate) fn app_version() -> Value {
    crate::version::AppVersion::current().to_json()
}

#[tauri::command]
pub(crate) fn update_status(state: tauri::State<'_, AppState>) -> Value {
    let match_active = (state.match_active)();
    lock(&state.updater).status(match_active)
}

/// Manual "check for updates": opens the GitHub latest-release page in the default browser. No
/// download, no install, no request from the app; refused while a match is pregame/live.
#[tauri::command]
pub(crate) fn update_open_release_page(state: tauri::State<'_, AppState>) -> Value {
    let match_active = (state.match_active)();
    let reply = crate::update::open_release_page_reply(match_active, open_in_default_browser);
    crate::log_info!(
        "updater",
        "release_page",
        "release page request completed",
        opened = reply["opened"].as_bool().unwrap_or(false),
        reason = reply["reason"].as_str().unwrap_or("NONE")
    );
    reply
}

/// Opens ONLY the fixed release page URL (the argument is checked against the constant), with
/// explorer.exe and no shell parsing.
fn open_in_default_browser(url: &str) -> bool {
    if url != crate::update::RELEASE_PAGE_URL {
        return false;
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(url)
            .spawn()
            .is_ok()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(feature = "updater")]
#[tauri::command]
pub(crate) fn update_check(state: tauri::State<'_, AppState>) -> Value {
    let match_active = (state.match_active)();
    let reply = lock(&state.updater).check(match_active);
    log_update_action("update_check", &reply);
    reply
}

#[cfg(feature = "updater")]
#[tauri::command]
pub(crate) fn update_install(state: tauri::State<'_, AppState>, confirm: bool) -> Value {
    let match_active = (state.match_active)();
    let reply = lock(&state.updater).install(confirm, match_active);
    log_update_action("update_install", &reply);
    reply
}

#[cfg(feature = "updater")]
fn log_update_action(action: &'static str, reply: &Value) {
    let state_after = reply
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let reason = reply
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("NONE");
    crate::log_info!(
        "updater",
        action,
        "updater action completed",
        state_after = state_after,
        reason = reason
    );
}

fn is_reason_code(value: &str) -> bool {
    value.len() >= 3
        && value.len() <= 64
        && value.contains('_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn log_media_result(operation: &'static str, result: &Result<Value, String>) {
    match result {
        Ok(value) => {
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .filter(|status| {
                    status.len() <= 32
                        && status
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                })
                .unwrap_or("ok");
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .filter(|reason| is_reason_code(reason))
                .unwrap_or("NONE");
            if matches!(status, "rejected" | "activation_error") {
                crate::log_warn!(
                    "media",
                    "operation_result",
                    "media operation returned a failure status",
                    operation = operation,
                    status = status,
                    reason = reason
                );
            } else {
                crate::log_info!(
                    "media",
                    "operation_result",
                    "media operation completed",
                    operation = operation,
                    status = status,
                    reason = reason
                );
            }
        }
        Err(error) => {
            let code = error
                .split(|ch: char| !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'))
                .find(|part| is_reason_code(part))
                .unwrap_or("MEDIA_OPERATION_FAILED");
            crate::log_warn!(
                "media",
                "operation_result",
                "media operation failed",
                operation = operation,
                code = code
            );
        }
    }
}

#[tauri::command]
pub(crate) fn log_tail(since_seq: Option<u64>, limit: Option<u32>) -> Value {
    crate::logging::tail(since_seq, limit)
}

#[tauri::command]
pub(crate) fn log_dir() -> String {
    crate::logging::log_dir()
}

#[tauri::command]
pub(crate) fn log_open_folder() -> Result<(), String> {
    crate::logging::open_log_dir()
}

#[tauri::command]
pub(crate) fn log_frontend(
    level: String,
    module: String,
    msg: String,
    fields: Option<Value>,
) -> Result<bool, String> {
    crate::logging::validate_frontend_input(&level, &module, &msg, fields.as_ref())
        .map_err(str::to_owned)?;
    let Ok(level) = crate::logging::Level::parse(&level) else {
        // Invalid requests produce no line, so they cannot bypass the renderer
        // limiter by flooding warnings before the rate-limited facade.
        return Ok(false);
    };
    let fields = match fields {
        Some(Value::Object(fields)) => fields,
        Some(Value::Null) | None => Map::new(),
        Some(value) => Map::from_iter([("value".to_owned(), value)]),
    };
    Ok(crate::logging::emit_frontend(
        level, &module, "frontend", &msg, fields,
    ))
}

#[tauri::command]
pub(crate) fn log_set_level(level: String) -> Result<(), String> {
    crate::logging::set_level(&level).map_err(str::to_owned)
}

#[tauri::command(async)]
pub(crate) fn runtime_get_status(state: tauri::State<'_, AppState>) -> Value {
    let revision = state.revision.fetch_add(1, Ordering::Relaxed) + 1;
    let (sample, throttled) = *lock(&state.sample);
    // Taken before `Shared` so the facts lock is never nested under it.
    let media = state.host.media_status();
    let shared = lock(&state.runtime.shared);
    status::build(&shared, &sample, throttled, revision, media)
}

#[tauri::command(async)]
pub(crate) fn own_snapshot(state: tauri::State<'_, AppState>) -> Value {
    own_snapshot_value(state.runtime.as_ref())
}

/// Shared read-model builder used by the command and the passive HUD.
pub(crate) fn own_snapshot_value(runtime: &crate::intake::Runtime) -> Value {
    lock(&runtime.shared).snapshot()
}

/// User action: write the owned game configuration and start listening.
/// `replace` is the separate, explicit confirmation to replace an owned file
/// that was edited outside the app.
#[tauri::command(async)]
pub(crate) fn gsi_setup(
    state: tauri::State<'_, AppState>,
    replace: Option<bool>,
) -> Result<(), String> {
    if state.host.shutdown_started() {
        return Err("CANCELLED".to_owned());
    }
    let _settings_guard = lock(&state.settings_gate);
    if state.host.shutdown_started() {
        return Err("CANCELLED".to_owned());
    }
    state
        .runtime
        .setup(replace.unwrap_or(false))
        .map_err(str::to_owned)
}

#[tauri::command(async)]
pub(crate) fn settings_get(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let _settings_guard = lock(&state.settings_gate);
    settings::get(&state.settings_dir)
}

#[tauri::command(async)]
pub(crate) fn settings_patch(
    state: tauri::State<'_, AppState>,
    request: Value,
) -> Result<Value, String> {
    if state.host.shutdown_started() {
        return Err("CANCELLED".to_owned());
    }
    let _settings_guard = lock(&state.settings_gate);
    if state.host.shutdown_started() {
        return Err("CANCELLED".to_owned());
    }
    settings::patch(&state.settings_dir, &request)
}

#[tauri::command(async)]
pub(crate) fn media_inspect(
    state: tauri::State<'_, AppState>,
    source_path: String,
) -> Result<Value, String> {
    let result = state.host.commands.media_inspect(&source_path);
    log_media_result("inspect", &result);
    result
}

#[tauri::command(async)]
pub(crate) fn media_install(
    state: tauri::State<'_, AppState>,
    request: Value,
) -> Result<Value, String> {
    let result = state.host.commands.media_install(&request);
    log_media_result("install", &result);
    result
}

#[tauri::command(async)]
pub(crate) fn media_activate(
    state: tauri::State<'_, AppState>,
    request_id: String,
    install_id: String,
    policy: String,
) -> Result<Value, String> {
    let result = state
        .host
        .commands
        .media_activate(&request_id, &install_id, &policy);
    log_media_result("activate", &result);
    result
}

#[tauri::command(async)]
pub(crate) fn media_list(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.commands.media_list()
}

#[tauri::command(async)]
pub(crate) fn media_probe_report(
    state: tauri::State<'_, AppState>,
    install_id: String,
    revision: u64,
    path_sha256: String,
    ok: bool,
    duration_ms: u64,
) -> Result<Value, String> {
    state
        .host
        .commands
        .media_probe_report(&install_id, revision, &path_sha256, ok, duration_ms)
}

#[tauri::command(async)]
pub(crate) fn media_probe_pending(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.commands.media_probe_pending()
}

#[tauri::command(async)]
pub(crate) fn audio_history(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<Value>, String> {
    state.host.commands.audio_history(limit)
}

#[tauri::command(async)]
pub(crate) fn set_muted(state: tauri::State<'_, AppState>, muted: bool) -> Result<Value, String> {
    state.host.commands.set_muted(muted)
}

fn parse_volume_percent(value: &Value) -> Result<u8, String> {
    value
        .as_i64()
        .filter(|percent| (0..=100).contains(percent))
        .map(|percent| percent as u8)
        .ok_or_else(|| "AUDIO_VOLUME_INVALID".to_owned())
}

fn audio_prefs_payload(prefs: &AudioPrefs, hotkeys_registered: bool, muted: Option<bool>) -> Value {
    serde_json::json!({
        "master_volume_percent": prefs.master_volume_percent,
        "hotkeys_enabled": prefs.hotkeys_enabled,
        "hotkeys_registered": hotkeys_registered,
        "muted": muted,
    })
}

fn apply_volume_value(
    prefs: &AudioPrefsService,
    value: &Value,
    source: &'static str,
    emit: impl FnOnce(Value),
) -> Result<AudioPrefs, String> {
    if !matches!(source, "ui" | "hotkey") {
        return Err("AUDIO_VOLUME_INVALID".to_owned());
    }
    let percent = parse_volume_percent(value)?;
    let (updated, changed) = prefs.set_volume_percent(percent)?;
    if changed {
        emit(serde_json::json!({"percent": percent, "source": source}));
    }
    Ok(updated)
}

fn apply_hotkey_step(
    prefs: &AudioPrefsService,
    step: VolumeStep,
    emit: impl FnOnce(Value),
) -> Result<AudioPrefs, String> {
    let (updated, changed) = prefs.step_volume(step)?;
    if changed {
        emit(serde_json::json!({
            "percent": updated.master_volume_percent,
            "source": "hotkey"
        }));
    }
    Ok(updated)
}

fn apply_hotkeys_setting(
    prefs: &AudioPrefsService,
    enabled: bool,
    set_registration: impl FnOnce(bool) -> bool,
) -> Result<(AudioPrefs, bool), String> {
    let (updated, _) = prefs.set_hotkeys_enabled(enabled)?;
    let registered = set_registration(enabled);
    Ok((updated, registered))
}

fn emit_volume_change(app: &tauri::AppHandle, payload: Value) {
    let percent = payload["percent"].as_u64().unwrap_or_default();
    let source = payload["source"].as_str().unwrap_or("ui");
    crate::log_info!(
        "audio",
        "volume_changed",
        "master volume changed",
        percent = percent,
        source = source
    );
    let _ = TauriEmitter::emit(app, "audio://volume", payload);
}

fn log_hotkeys_state(enabled: bool, registered: bool) {
    crate::log_info!(
        "audio",
        "hotkeys_state",
        "global volume hotkeys state",
        enabled = enabled,
        registered = registered
    );
}

fn log_hotkey_start_failure(failure: StartFailure) {
    match failure {
        StartFailure::Registration(error_code) => crate::log_warn!(
            "audio",
            "hotkey_register_failed",
            "global hotkey registration failed",
            error_code = error_code
        ),
        StartFailure::Thread(_error) => crate::log_warn!(
            "audio",
            "hotkey_thread_start_failed",
            "global hotkey thread could not start",
            code = "HOTKEY_THREAD_START_FAILED"
        ),
    }
}

fn volume_callback(app: tauri::AppHandle) -> Arc<dyn Fn(VolumeStep) + Send + Sync + 'static> {
    Arc::new(move |step| {
        let state = app.state::<AppState>();
        if state.host.shutdown_started() {
            return;
        }
        let result = apply_hotkey_step(&state.audio_prefs, step, |payload| {
            emit_volume_change(&app, payload);
        });
        if result.is_err() {
            crate::log_warn!(
                "audio",
                "audio_prefs_write_failed",
                "audio volume preference could not be saved",
                code = "AUDIO_PREFS_WRITE_FAILED"
            );
        }
    })
}

pub(crate) fn start_hotkeys(app: &tauri::AppHandle, state: &AppState) {
    let enabled = state.audio_prefs.current().hotkeys_enabled;
    let callback = volume_callback(app.clone());
    let registered = match lock(&state.hotkeys).set_enabled(enabled, callback) {
        Ok(registered) => registered,
        Err(error) => {
            log_hotkey_start_failure(error);
            false
        }
    };
    log_hotkeys_state(enabled, registered);
}

#[tauri::command(async)]
pub(crate) fn audio_prefs_get(state: tauri::State<'_, AppState>) -> Value {
    let registered = lock(&state.hotkeys).is_registered();
    // Host does not currently expose a read-only mute snapshot to commands.
    audio_prefs_payload(&state.audio_prefs.current(), registered, None)
}

#[tauri::command(async)]
pub(crate) fn audio_set_volume(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    percent: Value,
) -> Result<AudioPrefs, String> {
    apply_volume_value(&state.audio_prefs, &percent, "ui", |payload| {
        emit_volume_change(&app, payload);
    })
}

#[tauri::command(async)]
pub(crate) fn audio_set_hotkeys(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<AudioPrefs, String> {
    let mut hotkeys = lock(&state.hotkeys);
    let previously_enabled = state.audio_prefs.current().hotkeys_enabled;
    let callback = volume_callback(app.clone());
    let (prefs, registered) = apply_hotkeys_setting(&state.audio_prefs, enabled, |enabled| {
        match hotkeys.set_enabled(enabled, callback) {
            Ok(registered) => registered,
            Err(error) => {
                log_hotkey_start_failure(error);
                false
            }
        }
    })?;
    drop(hotkeys);
    if previously_enabled != enabled {
        log_hotkeys_state(enabled, registered);
    }
    Ok(prefs)
}

#[tauri::command(async)]
pub(crate) fn demo_available() -> bool {
    #[cfg(any(feature = "demo", debug_assertions))]
    {
        let enabled = std::env::var("KOHBOR_DEMO").ok();
        crate::demo::demo_enabled(enabled.as_deref())
    }
    #[cfg(not(any(feature = "demo", debug_assertions)))]
    {
        false
    }
}

#[cfg(any(feature = "demo", debug_assertions))]
#[tauri::command(async)]
pub(crate) fn demo_trigger(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    event_id: String,
    tier: Option<u8>,
) -> Result<(), String> {
    if state.host.shutdown_started() {
        return Err("CANCELLED".to_owned());
    }
    let enabled = std::env::var("KOHBOR_DEMO").ok();
    if !crate::demo::demo_enabled(enabled.as_deref()) {
        return Err("CAPABILITY_UNAVAILABLE".to_owned());
    }
    let event = crate::demo::demo_event(&event_id, tier).map_err(str::to_owned)?;

    let _ = tauri::Emitter::emit(
        &app,
        "overlay://demo",
        serde_json::json!({"source": "demo"}),
    );
    state.host.demo_offer(event)
}

/// Called on Tauri's protocol thread. It only snapshots request fields and
/// enqueues the response continuation into the bounded `media-serve` lane.
pub(crate) fn media_protocol_handler(
    context: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    let protocol_request = crate::media::protocol::ProtocolRequest {
        method: request.method().as_str().to_owned(),
        path: request.uri().path().to_owned(),
        query: request.uri().query().map(str::to_owned),
        range: request
            .headers()
            .get(tauri::http::header::RANGE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
    };
    let state = context.app_handle().state::<AppState>();
    state
        .host
        .dispatch_protocol(protocol_request, move |response| {
            let mut output = tauri::http::Response::new(response.body);
            *output.status_mut() = tauri::http::StatusCode::from_u16(response.status)
                .unwrap_or(tauri::http::StatusCode::SERVICE_UNAVAILABLE);
            for (name, value) in response.headers {
                if let (Ok(name), Ok(value)) = (
                    tauri::http::header::HeaderName::from_bytes(name.as_bytes()),
                    tauri::http::header::HeaderValue::from_str(&value),
                ) {
                    output.headers_mut().insert(name, value);
                }
            }
            responder.respond(output);
        });
}

#[cfg(test)]
mod audio_volume_tests {
    use super::{
        apply_hotkey_step, apply_hotkeys_setting, apply_volume_value, audio_prefs_payload,
        parse_volume_percent,
    };
    use crate::audio_prefs::{AudioPrefs, AudioPrefsService, AudioPrefsStore};
    use crate::hotkeys::VolumeStep;
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    struct FakeStore(Mutex<AudioPrefs>);

    impl AudioPrefsStore for FakeStore {
        fn load(&self) -> AudioPrefs {
            *self.0.lock().unwrap()
        }

        fn save(&self, prefs: &AudioPrefs) -> Result<(), String> {
            *self.0.lock().unwrap() = *prefs;
            Ok(())
        }
    }

    fn service(initial: AudioPrefs) -> (AudioPrefsService, Arc<FakeStore>) {
        let store = Arc::new(FakeStore(Mutex::new(initial)));
        let service = AudioPrefsService::with_store(store.clone());
        (service, store)
    }

    #[test]
    fn volume_parser_accepts_both_bounds() {
        assert_eq!(parse_volume_percent(&json!(0)), Ok(0));
        assert_eq!(parse_volume_percent(&json!(100)), Ok(100));
    }

    #[test]
    fn volume_parser_accepts_an_integer() {
        assert_eq!(parse_volume_percent(&json!(55)), Ok(55));
    }

    #[test]
    fn volume_parser_rejects_non_integer_json_numbers() {
        assert!(parse_volume_percent(&json!(55.0)).is_err());
    }

    #[test]
    fn volume_parser_rejects_strings_and_booleans() {
        assert!(parse_volume_percent(&json!("55")).is_err());
        assert!(parse_volume_percent(&json!(true)).is_err());
    }

    #[test]
    fn volume_parser_rejects_values_outside_zero_to_one_hundred() {
        assert!(parse_volume_percent(&json!(-1)).is_err());
        assert!(parse_volume_percent(&json!(101)).is_err());
    }

    #[test]
    fn ui_volume_handler_persists_and_emits_the_named_event_payload() {
        let (service, store) = service(AudioPrefs::default());
        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let emitted = events.clone();

        apply_volume_value(&service, &json!(65), "ui", move |payload| {
            emitted.lock().unwrap().push(payload);
        })
        .unwrap();

        assert_eq!(store.load().master_volume_percent, 65);
        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[json!({"percent": 65, "source": "ui"})]
        );
    }

    #[test]
    fn hotkey_volume_handler_steps_from_the_current_persisted_value() {
        let initial = AudioPrefs {
            master_volume_percent: 55,
            hotkeys_enabled: true,
        };
        let (service, store) = service(initial);
        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let emitted = events.clone();

        apply_hotkey_step(&service, VolumeStep::Increase, move |payload| {
            emitted.lock().unwrap().push(payload);
        })
        .unwrap();

        assert_eq!(store.load().master_volume_percent, 65);
        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[json!({"percent": 65, "source": "hotkey"})]
        );
    }

    #[test]
    fn hotkey_setting_handler_persists_and_calls_the_registration_boundary() {
        let (service, store) = service(AudioPrefs::default());
        let mut requested = None;

        let (prefs, registered) = apply_hotkeys_setting(&service, false, |enabled| {
            requested = Some(enabled);
            false
        })
        .unwrap();

        assert_eq!(requested, Some(false));
        assert!(!store.load().hotkeys_enabled);
        assert_eq!(prefs, store.load());
        assert!(!registered);
    }

    #[test]
    fn prefs_get_payload_carries_registration_and_unknown_mute_state() {
        let payload = audio_prefs_payload(
            &AudioPrefs {
                master_volume_percent: 37,
                hotkeys_enabled: true,
            },
            true,
            None,
        );

        assert_eq!(
            payload,
            json!({
                "master_volume_percent": 37,
                "hotkeys_enabled": true,
                "hotkeys_registered": true,
                "muted": null
            })
        );
    }
}
