//! Tauri command and protocol adapters. The behavior behind each command lives
//! in the Tauri-independent host and media-service layers.

use crate::audio_prefs::{AudioPrefs, AudioPrefsService};
use crate::hotkeys::{StartFailure, VolumeStep};
use crate::intake::lock;
use crate::{settings, status, AppState};
use serde_json::{json, Map, Value};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{Emitter as TauriEmitter, Manager};

#[tauri::command]
pub(crate) fn app_version() -> Value {
    crate::version::AppVersion::current().to_json()
}

#[tauri::command]
pub(crate) fn draft_stats_prefs_get(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
) -> Result<crate::hero_stats::DraftStatsPrefs, String> {
    state.hero_stats.prefs_get(webview.label())
}

#[tauri::command]
pub(crate) fn draft_stats_prefs_set(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
    patch: Value,
) -> Result<crate::hero_stats::DraftStatsPrefs, String> {
    state.hero_stats.prefs_set(webview.label(), patch)
}

#[tauri::command]
pub(crate) fn draft_stats_retry(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.hero_stats.retry(webview.label())
}

#[tauri::command]
pub(crate) fn draft_column_view(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
) -> Result<crate::hero_stats::DraftColumnView, String> {
    state.hero_stats.view(webview.label())
}

#[tauri::command]
pub(crate) fn update_status(state: tauri::State<'_, AppState>) -> Value {
    let match_active = (state.match_active)();
    state.updater.status(match_active)
}

/// Manual "check for updates": opens the GitHub latest-release page in the default browser. No
/// download, no install, no request from the app; refused while a match is pregame/live.
#[tauri::command]
pub(crate) fn update_open_release_page(state: tauri::State<'_, AppState>) -> Value {
    let match_active = (state.match_active)();
    crate::update::open_release_page_reply(match_active, open_in_default_browser)
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
pub(crate) async fn update_check(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let match_active = (state.match_active)();
    let mut reply = state.updater.check(match_active).await;
    if !match_active {
        let now_active = (state.match_active)();
        if now_active {
            reply = state.updater.status(true);
        }
    }
    log_update_action("update_check", &reply);
    Ok(reply)
}

#[cfg(feature = "updater")]
#[tauri::command]
pub(crate) async fn update_download(
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    let match_active = (state.match_active)();
    let mut reply = state.updater.download(match_active).await;
    if !match_active && (state.match_active)() {
        reply = state.updater.status(true);
    }
    log_update_action("update_download", &reply);
    Ok(reply)
}

#[cfg(feature = "updater")]
#[tauri::command]
pub(crate) fn update_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    confirm: bool,
) -> Result<Value, String> {
    let match_active = (state.match_active)();
    let reply = state.updater.install(confirm, match_active, || {
        // Recheck immediately before stopping services in case the match began after the IPC
        // call was admitted.
        if (state.match_active)() {
            return Err(crate::update::MATCH_ACTIVE);
        }
        stop_update_services(&app, &state);
        Ok(())
    });
    log_update_action("update_install", &reply);
    Ok(reply)
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
    let fields = Map::from_iter([
        ("state".to_owned(), Value::String(state_after.to_owned())),
        ("reason".to_owned(), Value::String(reason.to_owned())),
    ]);
    crate::logging::emit(
        crate::logging::Level::Info,
        "update",
        action,
        "updater action completed",
        fields,
    );
}

#[cfg(feature = "updater")]
fn stop_update_services(app: &tauri::AppHandle, state: &AppState) {
    crate::capture_host::stop();
    app.state::<crate::account_supabase::SupabaseAuth>()
        .shutdown();
    // Keep the Exit handler's shutdown order so every touched audio session is restored.
    state.orb.stop_for_exit();
    state.mixer.shutdown();
    lock(&state.hud).stop();
    lock(&state.hotkeys).stop();
    state.host.shutdown();
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

#[tauri::command]
pub(crate) fn objective_prefs_get(state: tauri::State<'_, AppState>) -> Value {
    state.objectives.prefs.current().to_value()
}

#[tauri::command]
pub(crate) fn objective_prefs_patch(
    state: tauri::State<'_, AppState>,
    request: Value,
) -> Result<Value, String> {
    if state.host.shutdown_started() {
        return Err("CANCELLED".to_owned());
    }
    state.objectives.prefs.patch(&request)
}

fn layout_viewport(app: &tauri::AppHandle<tauri::Wry>) -> crate::layout::LayoutViewport {
    app.get_webview_window("hud")
        .map(|window| crate::layout::LayoutViewport::from_window(&window))
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) fn layout_get(
    app: tauri::AppHandle<tauri::Wry>,
    state: tauri::State<'_, AppState>,
) -> Value {
    state.layout.get(layout_viewport(&app))
}

#[tauri::command]
pub(crate) fn layout_patch(
    app: tauri::AppHandle<tauri::Wry>,
    state: tauri::State<'_, AppState>,
    request: Value,
) -> Result<Value, String> {
    if state.host.shutdown_started() {
        return Err("CANCELLED".to_owned());
    }
    state.layout.patch(&request, layout_viewport(&app))
}

#[tauri::command]
pub(crate) fn layout_reset(
    app: tauri::AppHandle<tauri::Wry>,
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    if state.host.shutdown_started() {
        return Err("CANCELLED".to_owned());
    }
    state.layout.reset(layout_viewport(&app))
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
pub(crate) fn media_package_summary(
    state: tauri::State<'_, AppState>,
    source_path: String,
) -> Result<Value, String> {
    let result = state.host.commands.media_package_summary(&source_path);
    log_media_result("package_summary", &result);
    result
}

#[tauri::command]
pub(crate) fn media_package_size_limit() -> u64 {
    kohbor_pack::Limits::default().container_bytes
}

#[tauri::command(async)]
pub(crate) fn media_package_summary_bytes(
    state: tauri::State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<Value, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("SOURCE_INVALID".to_owned());
    };
    let result = state.host.commands.media_package_bytes(bytes, None);
    log_media_result("package_summary_bytes", &result);
    result
}

#[tauri::command(async)]
pub(crate) fn media_install_bytes(
    state: tauri::State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<Value, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("SOURCE_INVALID".to_owned());
    };
    let header = |name: &'static str| {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| "SOURCE_INVALID".to_owned())
    };
    let identity = (
        header("x-kohbor-request-id")?,
        header("x-kohbor-container-sha256")?,
    );
    let result = state
        .host
        .commands
        .media_package_bytes(bytes, Some(identity));
    log_media_result("install_bytes", &result);
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
pub(crate) fn media_cancel_pending_activation(
    state: tauri::State<'_, AppState>,
    install_id: String,
) -> Result<Value, String> {
    let result = state
        .host
        .commands
        .media_cancel_pending_activation(&install_id);
    log_media_result("cancel_pending_activation", &result);
    result
}

#[tauri::command(async)]
pub(crate) fn media_list(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.commands.media_list()
}

#[tauri::command(async)]
pub(crate) fn media_pack_summaries(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.commands.media_pack_summaries()
}

#[tauri::command(async)]
pub(crate) fn media_cabinet(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.commands.media_cabinet()
}

#[tauri::command(async)]
pub(crate) fn media_event_map(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.commands.media_event_map()
}

#[tauri::command(async)]
pub(crate) fn media_installed_event_map(
    state: tauri::State<'_, AppState>,
    install_id: String,
) -> Result<Value, String> {
    state.host.commands.media_installed_event_map(&install_id)
}

#[derive(Clone, Default)]
pub(crate) struct AuditionTracker {
    sequence: Arc<std::sync::atomic::AtomicU64>,
    active: Arc<std::sync::Mutex<Option<PendingAudition>>>,
    last_request: Arc<std::sync::Mutex<Option<Instant>>>,
}

struct PendingAudition {
    ticket: u64,
    playback_id: Option<String>,
    early_visual: Option<crate::overlay::VideoDiagnostic>,
    sender: Sender<AuditionSignal>,
}

#[derive(Debug, Clone)]
enum AuditionSignal {
    Started {
        playback_id: String,
        has_visual: bool,
    },
    Ended {
        playback_id: String,
        reason: String,
    },
    Visual(crate::overlay::VideoDiagnostic),
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub(crate) struct MediaAuditionResult {
    audio_started: bool,
    audio_ended: bool,
    audio_end_reason: Option<String>,
    visual_status: &'static str,
    visual_reason: Option<String>,
    video_error_code: Option<u8>,
}

struct AuditionWait {
    ticket: u64,
    receiver: Receiver<AuditionSignal>,
}

impl AuditionTracker {
    fn begin(&self) -> Result<AuditionWait, String> {
        const MIN_AUDITION_INTERVAL: Duration = Duration::from_millis(750);
        let (sender, receiver) = mpsc::channel();
        let mut active = crate::intake::lock(&self.active);
        if active.is_some() {
            return Err("AUDITION_BUSY".to_owned());
        }
        let now = Instant::now();
        let mut last_request = crate::intake::lock(&self.last_request);
        if last_request
            .is_some_and(|last| now.saturating_duration_since(last) < MIN_AUDITION_INTERVAL)
        {
            return Err("AUDITION_RATE_LIMITED".to_owned());
        }
        *last_request = Some(now);
        drop(last_request);
        let ticket = self
            .sequence
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        *active = Some(PendingAudition {
            ticket,
            playback_id: None,
            early_visual: None,
            sender,
        });
        Ok(AuditionWait { ticket, receiver })
    }

    fn finish(&self, ticket: u64) {
        let mut active = crate::intake::lock(&self.active);
        if active
            .as_ref()
            .is_some_and(|pending| pending.ticket == ticket)
        {
            *active = None;
        }
    }

    pub(crate) fn observe_playback(&self, payload: &Value) {
        let Some(object) = payload.as_object() else {
            return;
        };
        let kind = object.get("kind").and_then(Value::as_str);
        if kind == Some("started")
            && object.get("event_type").and_then(Value::as_str) != Some("audition")
        {
            return;
        }
        let Some(playback_id) = object.get("playback_id").and_then(Value::as_str) else {
            return;
        };
        if !valid_audition_playback_id(playback_id) {
            return;
        }

        let mut active = crate::intake::lock(&self.active);
        let Some(pending) = active.as_mut() else {
            return;
        };
        match kind {
            Some("started") => {
                if pending.playback_id.is_some() {
                    return;
                }
                let Some(has_visual) = object.get("visual_url").map(|url| !url.is_null()) else {
                    return;
                };
                pending.playback_id = Some(playback_id.to_owned());
                let _ = pending.sender.send(AuditionSignal::Started {
                    playback_id: playback_id.to_owned(),
                    has_visual,
                });
                if let Some(visual) = pending.early_visual.take() {
                    if visual.playback_id == playback_id {
                        let _ = pending.sender.send(AuditionSignal::Visual(visual));
                    }
                }
            }
            Some("ended") if pending.playback_id.as_deref() == Some(playback_id) => {
                let Some(reason) = object.get("reason").and_then(Value::as_str) else {
                    return;
                };
                if !valid_audition_reason(reason) {
                    return;
                }
                let _ = pending.sender.send(AuditionSignal::Ended {
                    playback_id: playback_id.to_owned(),
                    reason: reason.to_owned(),
                });
            }
            _ => {}
        }
    }

    pub(crate) fn observe_visual(&self, visual: crate::overlay::VideoDiagnostic) {
        let mut active = crate::intake::lock(&self.active);
        let Some(pending) = active.as_mut() else {
            return;
        };
        match pending.playback_id.as_deref() {
            Some(playback_id) if playback_id == visual.playback_id => {
                let _ = pending.sender.send(AuditionSignal::Visual(visual));
            }
            None => pending.early_visual = Some(visual),
            _ => {}
        }
    }
}

fn valid_audition_playback_id(value: &str) -> bool {
    let Some(digits) = value.strip_prefix("pb_") else {
        return false;
    };
    (10..=20).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_audition_reason(value: &str) -> bool {
    matches!(
        value,
        "finished"
            | "cancelled_preempted"
            | "cancelled_session"
            | "cancelled_revision"
            | "cancelled_muted"
            | "cancelled_device_lost"
    )
}

fn audition_result(
    audio_started: bool,
    audio_ended: bool,
    audio_end_reason: Option<String>,
    visual_status: &'static str,
    visual_reason: Option<String>,
    video_error_code: Option<u8>,
) -> MediaAuditionResult {
    MediaAuditionResult {
        audio_started,
        audio_ended,
        audio_end_reason,
        visual_status,
        visual_reason,
        video_error_code,
    }
}

fn wait_for_audition_result(receiver: &Receiver<AuditionSignal>) -> MediaAuditionResult {
    const START_TIMEOUT: Duration = Duration::from_secs(5);
    const END_TIMEOUT: Duration = Duration::from_secs(30);
    const VISUAL_GRACE: Duration = Duration::from_secs(6);

    let started = Instant::now();
    let (playback_id, has_visual) = loop {
        match receiver.recv_timeout(START_TIMEOUT.saturating_sub(started.elapsed())) {
            Ok(AuditionSignal::Started {
                playback_id,
                has_visual,
            }) => break (playback_id, has_visual),
            Ok(_) => continue,
            Err(_) => {
                return audition_result(
                    false,
                    false,
                    Some("AUDIO_START_UNCONFIRMED".to_owned()),
                    "unknown",
                    Some("AUDITION_RESULT_UNAVAILABLE".to_owned()),
                    None,
                );
            }
        }
    };

    let mut audio_ended = false;
    let mut audio_timed_out = false;
    let mut audio_end_reason = None;
    let mut visual_status = (!has_visual).then_some("none");
    let mut visual_reason = None;
    let mut video_error_code = None;

    let end_deadline = Instant::now() + END_TIMEOUT;
    let mut visual_deadline = None;
    loop {
        if (audio_ended || audio_timed_out) && visual_status.is_some() {
            break;
        }
        let now = Instant::now();
        if !audio_ended && !audio_timed_out && now >= end_deadline {
            audio_end_reason = Some("AUDIO_END_UNCONFIRMED".to_owned());
            audio_timed_out = true;
            visual_deadline.get_or_insert(now + VISUAL_GRACE);
        }
        if has_visual && visual_status.is_none() {
            if visual_deadline.is_none() && audio_ended {
                visual_deadline = Some(now + VISUAL_GRACE);
            }
            if visual_deadline.is_some_and(|deadline| now >= deadline) {
                visual_status = Some("unknown");
                visual_reason = Some("VIDEO_DIAGNOSTIC_UNAVAILABLE".to_owned());
            }
        }
        if audio_end_reason.is_some() && visual_status.is_some() {
            break;
        }

        let mut wait = if audio_ended || audio_timed_out {
            visual_deadline
                .map(|deadline| deadline.saturating_duration_since(now))
                .unwrap_or(VISUAL_GRACE)
        } else {
            end_deadline.saturating_duration_since(now)
        };
        if has_visual && visual_status.is_none() {
            if let Some(deadline) = visual_deadline {
                wait = wait.min(deadline.saturating_duration_since(now));
            }
        }
        match receiver.recv_timeout(wait) {
            Ok(AuditionSignal::Ended {
                playback_id: id,
                reason,
            }) if id == playback_id => {
                audio_ended = true;
                audio_end_reason = Some(reason);
                visual_deadline.get_or_insert(Instant::now() + VISUAL_GRACE);
            }
            Ok(AuditionSignal::Visual(visual)) if visual.playback_id == playback_id => {
                match visual.outcome {
                    crate::overlay::VideoDiagnosticOutcome::Started => {
                        visual_status = Some("shown");
                    }
                    crate::overlay::VideoDiagnosticOutcome::Failed => {
                        visual_status = Some("failed");
                        visual_reason = Some(visual.reason_code.to_owned());
                        video_error_code = visual.error_code;
                    }
                }
            }
            Ok(AuditionSignal::Started { .. })
            | Ok(AuditionSignal::Ended { .. })
            | Ok(AuditionSignal::Visual(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !audio_ended && !audio_timed_out {
                    audio_end_reason = Some("AUDIO_END_UNCONFIRMED".to_owned());
                    audio_timed_out = true;
                    visual_deadline.get_or_insert(Instant::now() + VISUAL_GRACE);
                    if !has_visual {
                        break;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                audio_end_reason = Some("AUDITION_RESULT_UNAVAILABLE".to_owned());
                if has_visual && visual_status.is_none() {
                    visual_status = Some("unknown");
                    visual_reason = Some("AUDITION_RESULT_UNAVAILABLE".to_owned());
                }
                break;
            }
        }
    }

    audition_result(
        true,
        audio_ended,
        audio_end_reason,
        visual_status.unwrap_or("unknown"),
        visual_reason,
        video_error_code,
    )
}

fn log_audition_result(result: &MediaAuditionResult) {
    crate::log_info!(
        "media",
        "audition_result",
        "pack clip audition lifecycle reported",
        audio_started = result.audio_started,
        audio_ended = result.audio_ended,
        audio_end_reason = result.audio_end_reason.as_deref().unwrap_or("NONE"),
        visual_status = result.visual_status,
        visual_reason = result.visual_reason.as_deref().unwrap_or("NONE"),
        video_error_code_known = result.video_error_code.is_some(),
        video_error_code = result.video_error_code.unwrap_or_default()
    );
}

#[tauri::command(async)]
pub(crate) fn media_audition(
    state: tauri::State<'_, AppState>,
    event_id: String,
    tier: Option<u8>,
    overlay: Option<bool>,
) -> Result<MediaAuditionResult, String> {
    run_media_audition(
        &state,
        &event_id,
        tier,
        None,
        overlay.unwrap_or(false),
        None,
    )
}

#[tauri::command(async)]
pub(crate) fn media_audition_variant(
    state: tauri::State<'_, AppState>,
    event_id: String,
    tier: Option<u8>,
    variant_id: String,
    overlay: Option<bool>,
) -> Result<MediaAuditionResult, String> {
    run_media_audition(
        &state,
        &event_id,
        tier,
        Some(&variant_id),
        overlay.unwrap_or(false),
        None,
    )
}

#[tauri::command(async)]
pub(crate) fn media_audition_installed_variant(
    state: tauri::State<'_, AppState>,
    install_id: String,
    event_id: String,
    tier: Option<u8>,
    variant_id: String,
) -> Result<MediaAuditionResult, String> {
    run_media_audition(
        &state,
        &event_id,
        tier,
        Some(&variant_id),
        false,
        Some(&install_id),
    )
}

fn run_media_audition(
    state: &AppState,
    event_id: &str,
    tier: Option<u8>,
    variant_id: Option<&str>,
    overlay: bool,
    installed_pack: Option<&str>,
) -> Result<MediaAuditionResult, String> {
    let pending = state.audition.begin()?;
    let session_epoch = crate::intake::lock(&state.runtime.shared)
        .epoch
        .unwrap_or(1);
    let audition = match installed_pack {
        Some(install_id) => state.host.commands.media_audition_installed_variant(
            install_id,
            event_id,
            tier,
            variant_id.unwrap_or_default(),
            session_epoch,
        ),
        None => match variant_id {
            Some(variant_id) => state.host.commands.media_audition_variant(
                event_id,
                tier,
                variant_id,
                session_epoch,
                overlay,
            ),
            None => state
                .host
                .commands
                .media_audition(event_id, tier, session_epoch, overlay),
        },
    };
    if let Err(error) = audition {
        state.audition.finish(pending.ticket);
        return Err(error);
    }
    let result = wait_for_audition_result(&pending.receiver);
    state.audition.finish(pending.ticket);
    log_audition_result(&result);
    Ok(result)
}

#[tauri::command(async)]
pub(crate) fn media_variant_visual_url(
    state: tauri::State<'_, AppState>,
    install_id: String,
    event_id: String,
    tier: Option<u8>,
    variant_id: String,
) -> Result<Value, String> {
    state
        .host
        .commands
        .media_variant_visual_url(&install_id, &event_id, tier, &variant_id)
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
pub(crate) fn match_recap(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.commands.match_recap()
}

#[tauri::command(async)]
pub(crate) fn clear_match_recap(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.commands.clear_match_recap()
}

/// OVERVIEW-002: own local match history with its computed summary.
#[tauri::command(async)]
pub(crate) fn match_history_get(
    state: tauri::State<'_, AppState>,
    month_start: Option<u64>,
) -> Value {
    state.match_history.view(month_start)
}

/// Deletes the local match history file contents (user action on the Overview).
#[tauri::command(async)]
pub(crate) fn match_history_clear(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.match_history.clear()?;
    crate::log_info!(
        "history",
        "match_history_cleared",
        "local match history cleared"
    );
    Ok(json!({"cleared": true}))
}

/// OVERVIEW-003: the cached or freshly fetched news view (images as data: URLs).
#[tauri::command(async)]
pub(crate) fn news_feed_get(state: tauri::State<'_, AppState>) -> Value {
    state.news.view()
}

/// The user's "retry" on the news column. Fetches only the fixed feed file.
#[tauri::command]
pub(crate) fn news_refresh(state: tauri::State<'_, AppState>) -> Value {
    state.news.refresh()
}

#[tauri::command]
pub(crate) fn news_prefs_get(state: tauri::State<'_, AppState>) -> crate::news::NewsPrefs {
    state.news.prefs()
}

#[tauri::command]
pub(crate) fn news_prefs_set(
    state: tauri::State<'_, AppState>,
    patch: Value,
) -> Result<crate::news::NewsPrefs, String> {
    state.news.set_prefs(&patch)
}

/// Opens a news link in the system browser. The renderer names a shown post and link index;
/// the URL comes only from the validated feed. Refused while a match is running.
#[tauri::command]
pub(crate) fn news_open_link(
    state: tauri::State<'_, AppState>,
    post_id: String,
    index: usize,
) -> Value {
    if (state.match_active)() {
        return json!({"opened": false, "reason": "MATCH_ACTIVE"});
    }
    let Some(url) = state.news.link_url(&post_id, index) else {
        return json!({"opened": false, "reason": "LINK_UNKNOWN"});
    };
    #[cfg(windows)]
    let opened = std::process::Command::new("explorer.exe")
        .arg(&url)
        .spawn()
        .is_ok();
    #[cfg(not(windows))]
    let opened = {
        let _ = url;
        false
    };
    crate::log_info!(
        "news",
        "link_opened",
        "news link open requested",
        opened = opened
    );
    json!({"opened": opened, "reason": if opened { Value::Null } else { json!("OPEN_FAILED") }})
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

#[derive(serde::Serialize)]
pub(crate) struct AudioPrefsReply {
    master_volume_percent: u8,
    hotkeys_enabled: bool,
    hotkeys_registered: bool,
    muted: Option<bool>,
}

fn audio_prefs_reply(prefs: &AudioPrefs, hotkeys_registered: bool) -> AudioPrefsReply {
    AudioPrefsReply {
        master_volume_percent: prefs.master_volume_percent,
        hotkeys_enabled: prefs.hotkeys_enabled,
        hotkeys_registered,
        // The host has no read-only mute snapshot.
        muted: None,
    }
}

fn audio_prefs_snapshot(state: &AppState) -> AudioPrefsReply {
    let hotkeys = lock(&state.hotkeys);
    audio_prefs_reply(&state.audio_prefs.current(), hotkeys.is_registered())
}

fn apply_volume_value(
    prefs: &AudioPrefsService,
    value: &Value,
    source: &'static str,
    emit: impl FnOnce(Value),
) -> Result<(), String> {
    if !matches!(source, "ui" | "hotkey") {
        return Err("AUDIO_VOLUME_INVALID".to_owned());
    }
    let percent = parse_volume_percent(value)?;
    let (_, changed) = prefs.set_volume_percent(percent)?;
    if changed {
        emit(serde_json::json!({"percent": percent, "source": source}));
    }
    Ok(())
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
pub(crate) fn audio_prefs_get(state: tauri::State<'_, AppState>) -> AudioPrefsReply {
    audio_prefs_snapshot(&state)
}

#[tauri::command(async)]
pub(crate) fn audio_set_volume(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    percent: Value,
) -> Result<AudioPrefsReply, String> {
    apply_volume_value(&state.audio_prefs, &percent, "ui", |payload| {
        emit_volume_change(&app, payload);
    })?;
    Ok(audio_prefs_snapshot(&state))
}

#[tauri::command(async)]
pub(crate) fn audio_set_hotkeys(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<AudioPrefsReply, String> {
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
    Ok(audio_prefs_reply(&prefs, registered))
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
            if !(200..300).contains(&response.status) {
                crate::log_info!(
                    "media",
                    "protocol_response",
                    "media protocol request returned a non-success response",
                    status = response.status,
                    reason_code = response.reason_code.unwrap_or("MEDIA_RESPONSE_UNKNOWN")
                );
            }
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
        apply_hotkey_step, apply_hotkeys_setting, apply_volume_value, audio_prefs_reply,
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
    fn shared_prefs_reply_serializes_registration_and_unknown_mute_state() {
        for (enabled, registered) in [(true, true), (true, false), (false, false)] {
            let reply = audio_prefs_reply(
                &AudioPrefs {
                    master_volume_percent: 37,
                    hotkeys_enabled: enabled,
                },
                registered,
            );
            assert_eq!(
                json!(reply),
                json!({
                    "master_volume_percent": 37,
                    "hotkeys_enabled": enabled,
                    "hotkeys_registered": registered,
                    "muted": null
                })
            );
        }
    }
}

#[cfg(test)]
mod audition_result_tests {
    use super::{wait_for_audition_result, AuditionSignal};
    use crate::overlay::{VideoDiagnostic, VideoDiagnosticOutcome};
    use std::sync::mpsc;

    fn result_with_visual(outcome: Option<VideoDiagnosticOutcome>) -> super::MediaAuditionResult {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(AuditionSignal::Started {
                playback_id: "pb_0000000001".to_owned(),
                has_visual: outcome.is_some(),
            })
            .expect("send started lifecycle");
        if let Some(outcome) = outcome {
            sender
                .send(AuditionSignal::Visual(VideoDiagnostic {
                    playback_id: "pb_0000000001".to_owned(),
                    event_type: Some("audition".to_owned()),
                    outcome,
                    error_code: (outcome == VideoDiagnosticOutcome::Failed).then_some(4),
                    reason_code: if outcome == VideoDiagnosticOutcome::Failed {
                        "VIDEO_ERROR"
                    } else {
                        "NONE"
                    },
                }))
                .expect("send video lifecycle");
        }
        sender
            .send(AuditionSignal::Ended {
                playback_id: "pb_0000000001".to_owned(),
                reason: "finished".to_owned(),
            })
            .expect("send ended lifecycle");
        wait_for_audition_result(&receiver)
    }

    #[test]
    fn audition_result_distinguishes_visual_started_failed_and_none() {
        let shown = result_with_visual(Some(VideoDiagnosticOutcome::Started));
        assert!(shown.audio_started && shown.audio_ended);
        assert_eq!(shown.audio_end_reason.as_deref(), Some("finished"));
        assert_eq!(shown.visual_status, "shown");
        assert_eq!(shown.visual_reason, None);

        let failed = result_with_visual(Some(VideoDiagnosticOutcome::Failed));
        assert_eq!(failed.visual_status, "failed");
        assert_eq!(failed.visual_reason.as_deref(), Some("VIDEO_ERROR"));
        assert_eq!(failed.video_error_code, Some(4));

        let none = result_with_visual(None);
        assert_eq!(none.visual_status, "none");
        assert_eq!(none.visual_reason, None);
        assert_eq!(none.video_error_code, None);
    }
}
