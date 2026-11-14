//! Tauri command and protocol adapters. The behavior behind each command lives
//! in the Tauri-independent host and media-service layers.

use crate::intake::lock;
use crate::{settings, status, AppState};
use serde_json::{Map, Value};
use std::sync::atomic::Ordering;
use tauri::Manager;

#[tauri::command]
pub(crate) fn app_version() -> Value {
    crate::version::AppVersion::current().to_json()
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
    lock(&state.runtime.shared).snapshot()
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
