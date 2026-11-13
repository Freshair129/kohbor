//! Tauri command and protocol adapters. The behavior behind each command lives
//! in the Tauri-independent host and media-service layers.

use crate::intake::lock;
use crate::{settings, status, AppState};
use serde_json::Value;
use std::sync::atomic::Ordering;
use tauri::Manager;

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
    let _settings_guard = lock(&state.settings_gate);
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
    let _settings_guard = lock(&state.settings_gate);
    settings::patch(&state.settings_dir, &request)
}

#[tauri::command(async)]
pub(crate) fn media_inspect(
    state: tauri::State<'_, AppState>,
    source_path: String,
) -> Result<Value, String> {
    state.host.commands.media_inspect(&source_path)
}

#[tauri::command(async)]
pub(crate) fn media_install(
    state: tauri::State<'_, AppState>,
    request: Value,
) -> Result<Value, String> {
    state.host.commands.media_install(&request)
}

#[tauri::command(async)]
pub(crate) fn media_activate(
    state: tauri::State<'_, AppState>,
    request_id: String,
    install_id: String,
    policy: String,
) -> Result<Value, String> {
    state
        .host
        .commands
        .media_activate(&request_id, &install_id, &policy)
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
