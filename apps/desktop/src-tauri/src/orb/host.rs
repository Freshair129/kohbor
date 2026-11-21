//! Orb runtime: the separate `orb` window, the host cursor poll, the fail-closed foreground guard and the commands
//! of the orb and its deck twins (`mixer_get`, `mixer_set`, `mixer_focus_toggle`, plus the orb-only `orb_click` and
//! `orb_panel_height`, which cannot reach audio or processes). The renderer never names a process: the host resolves
//! Dota and the music application itself.

use super::{
    kept_focus, panel_rect, patch_prefs, Foreground, Geometry, Machine, OrbPrefs, PrefsStore,
    CLICK_SETTLE, ORB_LABEL,
};
use crate::intake::lock;
use crate::mixer::AppChannel;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const STATE_EVENT: &str = "orb://state";
const MIXER_EVENT: &str = "orb://mixer";
const KEEPALIVE: Duration = Duration::from_secs(1);
const PLACEMENT_REFRESH: Duration = Duration::from_secs(2);
/// Margin added to a critical cue's audio duration before the orb unlocks.
const CRITICAL_MARGIN: Duration = Duration::from_millis(500);

struct Pending {
    n: u64,
    target: &'static str,
    before: Option<Foreground>,
    due: Instant,
}

pub(crate) struct Shared {
    machine: Machine,
    geometry: Option<Geometry>,
    orb_hwnd: isize,
    last_hover: Option<Foreground>,
    pending: Vec<Pending>,
    interactions: u64,
    activations: u64,
    match_interactions: u64,
    match_activations: u64,
    in_match_prev: bool,
    click_through: bool,
    shown: bool,
    last_payload: Option<Value>,
    last_emit: Option<Instant>,
    /// Last logged visibility diagnosis, so a change is logged once (diagnostics for owner testing).
    last_diag: Option<String>,
    last_zone: Option<crate::orb::Zone>,
}

impl Shared {
    fn new(dwell_ms: u16, orb_hwnd: isize) -> Self {
        Self {
            machine: Machine::new(dwell_ms),
            geometry: None,
            orb_hwnd,
            last_hover: None,
            pending: Vec::new(),
            interactions: 0,
            activations: 0,
            match_interactions: 0,
            match_activations: 0,
            in_match_prev: false,
            click_through: true,
            shown: false,
            last_payload: None,
            last_emit: None,
            last_diag: None,
            last_zone: None,
        }
    }

    fn activation(
        &mut self,
        source: &'static str,
        before: Option<&Foreground>,
        after: &Foreground,
    ) {
        self.machine.fail_closed();
        self.activations = self.activations.saturating_add(1);
        self.match_activations = self.match_activations.saturating_add(1);
        crate::log_warn!(
            "orb",
            "activation_detected",
            "orb click changed the foreground; panel closed, orb hidden until the match ends",
            source = source,
            fg_before = before.map_or("unknown", Foreground::exe_or_unknown),
            fg_after = after.exe_or_unknown(),
            orb_foreground = after.hwnd == self.orb_hwnd
        );
    }
}

struct Running {
    window: tauri::WebviewWindow<tauri::Wry>,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

pub(crate) struct OrbHost {
    store: PrefsStore,
    prefs: Mutex<OrbPrefs>,
    running: Mutex<Option<Running>>,
    start_error: Mutex<Option<String>>,
}

impl OrbHost {
    pub(crate) fn open(dir: &Path) -> Self {
        let store = PrefsStore::in_data_dir(dir);
        let prefs = store.load();
        Self {
            store,
            prefs: Mutex::new(prefs),
            running: Mutex::new(None),
            start_error: Mutex::new(None),
        }
    }

    pub(crate) fn prefs(&self) -> OrbPrefs {
        lock(&self.prefs).clone()
    }

    fn shared(&self) -> Option<Arc<Mutex<Shared>>> {
        lock(&self.running)
            .as_ref()
            .map(|running| running.shared.clone())
    }

    fn window(&self) -> Option<tauri::WebviewWindow<tauri::Wry>> {
        lock(&self.running)
            .as_ref()
            .map(|running| running.window.clone())
    }

    /// Start the orb when it is enabled. A failure leaves it off and is reported in the status, never retried blindly.
    pub(crate) fn start_if_enabled(&self, app: &tauri::AppHandle<tauri::Wry>) {
        if !self.prefs().enabled {
            return;
        }
        let result = self.start(app);
        if let Err(code) = &result {
            crate::log_warn!(
                "orb",
                "start_failed",
                "orb could not start; it stays off",
                code = code.as_str()
            );
        }
        *lock(&self.start_error) = result.err();
    }

    #[cfg(windows)]
    fn start(&self, app: &tauri::AppHandle<tauri::Wry>) -> Result<(), String> {
        let mut running = lock(&self.running);
        if running.is_some() {
            return Ok(());
        }
        let window = native::create_window(app)?;
        let Some(hwnd) = crate::overlay_hwnd(&window).map(|hwnd| hwnd.0 as isize) else {
            let _ = window.destroy();
            return Err("ORB_HWND_UNAVAILABLE".to_owned());
        };
        let shared = Arc::new(Mutex::new(Shared::new(self.prefs().dwell_ms, hwnd)));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (app, window, shared, stop) =
                (app.clone(), window.clone(), shared.clone(), stop.clone());
            std::thread::Builder::new()
                .name("orb-cursor".into())
                .spawn(move || native::poll_loop(app, window, shared, stop))
        };
        let join = match thread {
            Ok(join) => join,
            Err(_) => {
                let _ = window.destroy();
                return Err("ORB_THREAD_UNAVAILABLE".to_owned());
            }
        };
        *running = Some(Running {
            window,
            shared,
            stop,
            join: Some(join),
        });
        crate::log_info!(
            "orb",
            "started",
            "edge orb started",
            dwell_ms = self.prefs().dwell_ms
        );
        Ok(())
    }

    #[cfg(not(windows))]
    fn start(&self, _app: &tauri::AppHandle<tauri::Wry>) -> Result<(), String> {
        Err("ORB_WINDOWS_ONLY".to_owned())
    }

    /// Settings off switch: stop the poll, then remove the window (the event loop is running).
    pub(crate) fn stop(&self) {
        let Some(mut running) = lock(&self.running).take() else {
            return;
        };
        running.stop.store(true, Ordering::SeqCst);
        if let Some(join) = running.join.take() {
            let _ = join.join();
        }
        let _ = running.window.set_ignore_cursor_events(true);
        let _ = running.window.destroy();
        Self::log_stopped(&running);
    }

    /// Application exit: only signal the poll. Joining could wait on a window call that the exiting event loop
    /// never answers; the process ends right after and takes the window with it.
    pub(crate) fn stop_for_exit(&self) {
        let Some(running) = lock(&self.running).take() else {
            return;
        };
        running.stop.store(true, Ordering::SeqCst);
        Self::log_stopped(&running);
    }

    fn log_stopped(running: &Running) {
        let shared = lock(&running.shared);
        crate::log_info!(
            "orb",
            "stopped",
            "edge orb stopped",
            interactions = shared.interactions,
            activations = shared.activations
        );
    }

    fn status(&self) -> Value {
        let shared = self.shared();
        let (phase, reason, failed_closed, interactions, activations) = match shared {
            Some(shared) => {
                let shared = lock(&shared);
                let payload = shared.machine.payload(Instant::now(), None);
                (
                    payload["phase"].clone(),
                    payload["reason"].clone(),
                    shared.machine.failed_closed,
                    shared.interactions,
                    shared.activations,
                )
            }
            None => (Value::Null, Value::Null, false, 0, 0),
        };
        json!({
            "running": self.shared().is_some(),
            "phase": phase,
            "reason": reason,
            "failed_closed": failed_closed,
            "interactions": interactions,
            "activations": activations,
            "start_error": *lock(&self.start_error),
            "windows_only": !cfg!(windows),
        })
    }

    fn prefs_json(&self) -> Value {
        let prefs = self.prefs();
        json!({
            "enabled": prefs.enabled,
            "dwell_ms": prefs.dwell_ms,
            "dwell_choices_ms": super::DWELL_CHOICES_MS,
            "focus_percent": prefs.focus_percent,
            "music_exe": prefs.music_exe,
            "focus_hotkey": prefs.focus_hotkey,
            "focus_hotkey_available": false,
            "status": self.status(),
        })
    }

    /// A qualified critical alert was admitted: close the panel and lock the orb at once, click-through first.
    pub(crate) fn critical_admitted(&self, app: &tauri::AppHandle<tauri::Wry>, duration: Duration) {
        let (Some(shared), Some(window)) = (self.shared(), self.window()) else {
            return;
        };
        {
            let mut shared = lock(&shared);
            shared
                .machine
                .lock_for_critical(Instant::now(), duration + CRITICAL_MARGIN);
            shared.click_through = true;
        }
        if window.set_ignore_cursor_events(true).is_err() {
            let _ = window.hide();
        }
        crate::log_info!(
            "orb",
            "critical_lock",
            "critical alert admitted; orb panel closed and locked"
        );
        emit_state(app, &shared, true);
    }

    /// Guard an orb-window interaction. The pre-click foreground is the poll's last sample while the pointer was over
    /// the orb; the post-click sample is taken now and again after 100 ms by the poll.
    fn guard(
        &self,
        app: &tauri::AppHandle<tauri::Wry>,
        target: &'static str,
        needs_panel: bool,
    ) -> Result<(), String> {
        let (Some(shared), Some(window)) = (self.shared(), self.window()) else {
            return Err("ORB_NOT_RUNNING".to_owned());
        };
        let after = sample_foreground();
        let failed = {
            let mut guard = lock(&shared);
            if guard.machine.failed_closed || !guard.machine.visible() {
                return Err("ORB_HIDDEN".to_owned());
            }
            if needs_panel && !guard.machine.panel_open() {
                return Err("ORB_PANEL_CLOSED".to_owned());
            }
            guard.interactions = guard.interactions.saturating_add(1);
            guard.match_interactions = guard.match_interactions.saturating_add(1);
            let n = guard.interactions;
            let before = guard.last_hover.clone();
            let orb_hwnd = guard.orb_hwnd;
            if kept_focus(before.as_ref(), &after, orb_hwnd) {
                guard.pending.push(Pending {
                    n,
                    target,
                    before,
                    due: Instant::now() + CLICK_SETTLE,
                });
                false
            } else {
                crate::log_info!(
                    "orb",
                    "interaction",
                    "orb interaction",
                    n = n,
                    target = target,
                    kept_focus = false
                );
                guard.activation("click", before.as_ref(), &after);
                guard.click_through = true;
                guard.shown = false;
                true
            }
        };
        if failed {
            let _ = window.set_ignore_cursor_events(true);
            let _ = window.hide();
            emit_state(app, &shared, true);
            return Err("ORB_FAILED_CLOSED".to_owned());
        }
        Ok(())
    }
}

fn sample_foreground() -> Foreground {
    #[cfg(windows)]
    {
        native::foreground()
    }
    #[cfg(not(windows))]
    {
        Foreground { hwnd: 0, exe: None }
    }
}

fn emit_state(app: &tauri::AppHandle<tauri::Wry>, shared: &Arc<Mutex<Shared>>, force: bool) {
    let now = Instant::now();
    let payload = {
        let mut shared = lock(shared);
        let payload = shared.machine.payload(now, shared.geometry.as_ref());
        // Charge progress moves every poll; the renderer animates it, so it does not force an emission.
        let mut comparable = payload.clone();
        comparable["charge_ms"] = Value::Null;
        let due = shared
            .last_emit
            .is_none_or(|at| now.saturating_duration_since(at) >= KEEPALIVE);
        if !force && !due && shared.last_payload.as_ref() == Some(&comparable) {
            return;
        }
        shared.last_payload = Some(comparable);
        shared.last_emit = Some(now);
        payload
    };
    let _ = app.emit_to(ORB_LABEL, STATE_EVENT, payload);
}

fn require_window(
    window: &tauri::WebviewWindow<tauri::Wry>,
    allowed: &[&str],
) -> Result<(), String> {
    if allowed.contains(&window.label()) {
        Ok(())
    } else {
        Err("ORB_WRONG_WINDOW".to_owned())
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Mixer composition (Game and Music sessions from the mixer thread; Kohbor from the existing announcer path)

fn kohbor_channel(state: &crate::AppState) -> Value {
    let (_, reason) = state.host.media_status();
    // Same rule as the deck's deriveMuted: only these reasons say anything about mute.
    let muted = match reason {
        Some("AUDIO_MUTED") => Value::Bool(true),
        None | Some("MEDIA_UNMAPPED" | "MEDIA_INVALID" | "MEDIA_PROBE_FAILED") => {
            Value::Bool(false)
        }
        Some(_) => Value::Null,
    };
    json!({
        "channel": "kohbor",
        "state": "present",
        "exe": null,
        "percent": state.audio_prefs.current().master_volume_percent,
        "muted": muted,
    })
}

fn compose(state: &crate::AppState, mut mixer: Value) -> Value {
    mixer["kohbor"] = kohbor_channel(state);
    mixer
}

fn reply(state: &crate::AppState, result: Result<Value, (String, Value)>) -> Value {
    match result {
        Ok(mixer) => json!({"ok": true, "code": null, "mixer": compose(state, mixer)}),
        Err((code, mixer)) => json!({"ok": false, "code": code, "mixer": compose(state, mixer)}),
    }
}

fn mixer_snapshot(state: &crate::AppState) -> Result<Value, String> {
    state.mixer.call(|mixer| mixer.snapshot())
}

fn broadcast(app: &tauri::AppHandle<tauri::Wry>, payload: &Value) {
    // Keep the orb and the deck twins on the same read.
    let _ = app.emit(MIXER_EVENT, payload);
}

fn set_kohbor(
    app: &tauri::AppHandle<tauri::Wry>,
    state: &crate::AppState,
    request: &Value,
) -> Result<Value, String> {
    match request["op"].as_str() {
        Some("level") => {
            let percent = request["percent"]
                .as_u64()
                .filter(|percent| *percent <= 100)
                .ok_or("MIXER_REQUEST_INVALID")? as u8;
            let (_, changed) = state.audio_prefs.set_volume_percent(percent)?;
            if changed {
                crate::log_info!(
                    "audio",
                    "volume_changed",
                    "master volume changed",
                    percent = percent,
                    source = "orb"
                );
                let _ = app.emit(
                    "audio://volume",
                    json!({"percent": percent, "source": "ui"}),
                );
            }
            Ok(json!({}))
        }
        Some("mute") => {
            let muted = request["muted"].as_bool().ok_or("MIXER_REQUEST_INVALID")?;
            state.host.commands.set_muted(muted)
        }
        _ => Err("MIXER_REQUEST_INVALID".to_owned()),
    }
}

fn persist_music(state: &crate::AppState, exe: Option<String>) {
    let mut prefs = lock(&state.orb.prefs);
    if prefs.music_exe == exe {
        return;
    }
    let mut next = prefs.clone();
    next.music_exe = exe;
    match state.orb.store.save(&next) {
        Ok(()) => *prefs = next,
        Err(code) => crate::log_warn!(
            "orb",
            "prefs_write_failed",
            "orb preferences not saved",
            code = code.as_str()
        ),
    }
}

fn mixer_set_inner(
    app: &tauri::AppHandle<tauri::Wry>,
    state: &crate::AppState,
    request: &Value,
) -> Result<Value, String> {
    let op = request["op"].as_str().ok_or("MIXER_REQUEST_INVALID")?;
    if request["channel"].as_str() == Some("kohbor") {
        let failure = set_kohbor(app, state, request).err();
        let snapshot = mixer_snapshot(state)
            .unwrap_or_else(|code| json!({"available": false, "reason": code}));
        return Ok(match failure {
            None => reply(state, Ok(snapshot)),
            Some(code) => reply(state, Err((code, snapshot))),
        });
    }
    let channel = |request: &Value| match request["channel"].as_str() {
        Some("game") => Ok(AppChannel::Game),
        Some("music") => Ok(AppChannel::Music),
        _ => Err("MIXER_REQUEST_INVALID".to_owned()),
    };
    let result = match op {
        "level" => {
            let channel = channel(request)?;
            let percent = request["percent"]
                .as_u64()
                .filter(|percent| *percent <= 100)
                .ok_or("MIXER_REQUEST_INVALID")? as u8;
            state
                .mixer
                .call(move |mixer| mixer.set_level(channel, percent))?
        }
        "mute" => {
            let channel = channel(request)?;
            let muted = request["muted"].as_bool().ok_or("MIXER_REQUEST_INVALID")?;
            state
                .mixer
                .call(move |mixer| mixer.set_muted(channel, muted))?
        }
        "app" => {
            let exe = request["exe"]
                .as_str()
                .map(str::to_ascii_lowercase)
                .filter(|exe| super::valid_exe(exe))
                .ok_or("MIXER_REQUEST_INVALID")?;
            let chosen = exe.clone();
            let result = state.mixer.call(move |mixer| mixer.select_music(&chosen))?;
            if result.is_ok() {
                persist_music(state, Some(exe));
            }
            result
        }
        "reset" => {
            let summary = state.mixer.call(|mixer| {
                let summary = mixer.restore_all("reset");
                (summary, mixer.snapshot())
            })?;
            let (summary, snapshot) = summary;
            if summary.pending > 0 {
                Err(("MIXER_RESTORE_PENDING".to_owned(), snapshot))
            } else {
                Ok(snapshot)
            }
        }
        _ => return Err("MIXER_REQUEST_INVALID".to_owned()),
    };
    Ok(reply(state, result))
}

// ---------------------------------------------------------------------------------------------------------------
// Commands

#[tauri::command]
pub(crate) async fn orb_prefs_get(
    state: tauri::State<'_, crate::AppState>,
) -> Result<Value, String> {
    Ok(state.orb.prefs_json())
}

/// Settings (control window only): `enabled`, `dwell_ms`, `focus_percent`. Disabling restores original levels.
#[tauri::command]
pub(crate) async fn orb_prefs_set(
    app: tauri::AppHandle<tauri::Wry>,
    window: tauri::WebviewWindow<tauri::Wry>,
    state: tauri::State<'_, crate::AppState>,
    patch: Value,
) -> Result<Value, String> {
    require_window(&window, &["control"])?;
    let (previous, next) = {
        let mut prefs = lock(&state.orb.prefs);
        let next = patch_prefs(&prefs, &patch)?;
        state.orb.store.save(&next)?;
        let previous = std::mem::replace(&mut *prefs, next.clone());
        (previous, next)
    };
    let music = next.music_exe.clone();
    let focus = next.focus_percent;
    let _ = state.mixer.call(move |mixer| mixer.configure(music, focus));
    if previous.dwell_ms != next.dwell_ms {
        if let Some(shared) = state.orb.shared() {
            lock(&shared).machine.dwell = Duration::from_millis(u64::from(next.dwell_ms));
        }
    }
    if previous.enabled != next.enabled {
        crate::log_info!(
            "orb",
            "enabled_changed",
            "edge orb setting changed",
            enabled = next.enabled
        );
        if next.enabled {
            state.orb.start_if_enabled(&app);
        } else {
            state.orb.stop();
            *lock(&state.orb.start_error) = None;
            let _ = state.mixer.call(|mixer| mixer.restore_all("disable"));
        }
    }
    Ok(state.orb.prefs_json())
}

/// Orb window only: a click on the revealed disc toggles the panel; `close` closes it.
#[tauri::command]
pub(crate) async fn orb_click(
    app: tauri::AppHandle<tauri::Wry>,
    window: tauri::WebviewWindow<tauri::Wry>,
    state: tauri::State<'_, crate::AppState>,
    target: String,
) -> Result<Value, String> {
    require_window(&window, &[ORB_LABEL])?;
    let target: &'static str = match target.as_str() {
        "disc" => "disc",
        "close" => "close",
        _ => return Err("ORB_TARGET_INVALID".to_owned()),
    };
    if let Err(code) = state.orb.guard(&app, target, target == "close") {
        crate::log_info!(
            "orb",
            "click_refused",
            "orb click refused by the guard",
            target = target,
            code = code.clone()
        );
        return Err(code);
    }
    let shared = state.orb.shared().ok_or("ORB_NOT_RUNNING")?;
    let open = {
        let mut shared = lock(&shared);
        match target {
            "disc" => match shared.machine.click_disc() {
                Ok(open) => {
                    crate::log_info!("orb", "click_accepted", "orb disc click", open = open);
                    open
                }
                Err(code) => {
                    crate::log_info!(
                        "orb",
                        "click_refused",
                        "orb disc click refused",
                        target = target,
                        code = code,
                        zone = format!("{:?}", shared.machine.zone)
                    );
                    return Err(code.to_owned());
                }
            },
            _ => {
                shared.machine.close();
                false
            }
        }
    };
    emit_state(&app, &shared, true);
    Ok(json!({"open": open}))
}

/// Orb window only: the measured panel height. The host places the panel outside the protected regions or keeps
/// it closed.
#[tauri::command]
pub(crate) async fn orb_panel_height(
    app: tauri::AppHandle<tauri::Wry>,
    window: tauri::WebviewWindow<tauri::Wry>,
    state: tauri::State<'_, crate::AppState>,
    height: f64,
) -> Result<Value, String> {
    require_window(&window, &[ORB_LABEL])?;
    let shared = state.orb.shared().ok_or("ORB_NOT_RUNNING")?;
    let result = {
        let mut shared = lock(&shared);
        if !shared.machine.panel_open() {
            return Err("ORB_PANEL_CLOSED".to_owned());
        }
        let rect = shared
            .geometry
            .as_ref()
            .and_then(|geometry| panel_rect(&geometry.layout, height));
        match rect {
            Some(rect) => {
                shared.machine.panel = Some(rect);
                Ok(json!({"x": rect.x, "y": rect.y, "w": rect.w, "h": rect.h}))
            }
            None => {
                shared.machine.close();
                crate::log_warn!(
                    "orb",
                    "panel_unplaceable",
                    "panel would cover a protected region; kept closed"
                );
                Err("ORB_PANEL_UNPLACEABLE".to_owned())
            }
        }
    };
    emit_state(&app, &shared, true);
    result
}

#[tauri::command]
pub(crate) async fn mixer_get(
    window: tauri::WebviewWindow<tauri::Wry>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Value, String> {
    require_window(&window, &["control", ORB_LABEL])?;
    let snapshot = mixer_snapshot(&state)?;
    Ok(reply(&state, Ok(snapshot)))
}

/// `{op: "level", channel, percent}`, `{op: "mute", channel, muted}`, `{op: "app", exe}` or `{op: "reset"}`.
/// Channels are `game`, `kohbor` and `music`; `exe` must be a music application with a session right now.
#[tauri::command]
pub(crate) async fn mixer_set(
    app: tauri::AppHandle<tauri::Wry>,
    window: tauri::WebviewWindow<tauri::Wry>,
    state: tauri::State<'_, crate::AppState>,
    request: Value,
) -> Result<Value, String> {
    require_window(&window, &["control", ORB_LABEL])?;
    if window.label() == ORB_LABEL {
        state.orb.guard(&app, "panel", true)?;
    }
    let result = mixer_set_inner(&app, &state, &request)?;
    broadcast(&app, &result);
    Ok(result)
}

#[tauri::command]
pub(crate) async fn mixer_focus_toggle(
    app: tauri::AppHandle<tauri::Wry>,
    window: tauri::WebviewWindow<tauri::Wry>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Value, String> {
    require_window(&window, &["control", ORB_LABEL])?;
    if window.label() == ORB_LABEL {
        state.orb.guard(&app, "focus", true)?;
    }
    let result = state.mixer.call(|mixer| mixer.focus_toggle())?;
    let result = reply(&state, result);
    broadcast(&app, &result);
    Ok(result)
}

/// Listen for qualified critical admissions on the playback stream (`class: critical`, `kind: started`).
pub(crate) fn register_critical_listener(app: &tauri::AppHandle<tauri::Wry>) {
    use tauri::Listener;
    let handle = app.clone();
    app.listen("playback://event", move |event| {
        let Ok(payload) = serde_json::from_str::<Value>(event.payload()) else {
            return;
        };
        if payload["kind"] != "started" || payload["class"] != "critical" {
            return;
        }
        let duration = Duration::from_millis(
            payload["audio_duration_ms"]
                .as_u64()
                .unwrap_or(1000)
                .min(10_000),
        );
        handle
            .state::<crate::AppState>()
            .orb
            .critical_admitted(&handle, duration);
    });
}

// ---------------------------------------------------------------------------------------------------------------
// Windows: window, cursor poll, foreground and fullscreen

#[cfg(windows)]
mod native {
    use super::{emit_state, Foreground, Geometry, Shared, PLACEMENT_REFRESH};
    use crate::intake::lock;
    use crate::orb::{kept_focus, HideReason, Zone, ORB_LABEL, ORB_URL};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use tauri::Manager;
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        GetWindowThreadProcessId, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    };

    pub(super) fn create_window(
        app: &tauri::AppHandle<tauri::Wry>,
    ) -> Result<tauri::WebviewWindow<tauri::Wry>, String> {
        let window = tauri::WebviewWindowBuilder::new(
            app,
            ORB_LABEL,
            tauri::WebviewUrl::App(ORB_URL.into()),
        )
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .shadow(false)
        .resizable(false)
        .visible(false)
        .additional_browser_args(crate::WEBVIEW2_BROWSER_ARGS)
        .inner_size(320.0, 720.0)
        .build()
        .map_err(|_| "ORB_WINDOW_CREATE_FAILED".to_owned())?;
        let configured = (|| {
            window
                .set_ignore_cursor_events(true)
                .map_err(|_| "ORB_CLICK_THROUGH_UNAVAILABLE")?;
            let hwnd = crate::overlay_hwnd(&window).ok_or("ORB_HWND_UNAVAILABLE")?;
            if !apply_no_activate_styles(hwnd) {
                return Err("ORB_NO_ACTIVATE_UNAVAILABLE");
            }
            Ok(())
        })();
        if let Err(code) = configured {
            let _ = window.destroy();
            return Err(code.to_owned());
        }
        Ok(window)
    }

    fn apply_no_activate_styles(hwnd: HWND) -> bool {
        let required = WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0;
        unsafe {
            let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (current | required) as isize);
            GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & required == required
        }
    }

    fn has_no_activate_style(hwnd: HWND) -> bool {
        unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_NOACTIVATE.0 != 0 }
    }

    /// Switch click-through by toggling only `WS_EX_TRANSPARENT`.
    ///
    /// `WebviewWindow::set_ignore_cursor_events` adds and removes `WS_EX_LAYERED` together with
    /// `WS_EX_TRANSPARENT`. Re-adding `WS_EX_LAYERED` to a window that never had layered attributes set leaves it
    /// undrawn, so after the first hover the disc vanished (owner test 2026-11-21). The layered bit stays as the
    /// window was created (click-through) and only the transparent bit changes.
    fn set_click_through(hwnd: HWND, click_through: bool) -> bool {
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 | WS_EX_LAYERED.0;
            let next = if click_through {
                style | WS_EX_TRANSPARENT.0
            } else {
                style & !WS_EX_TRANSPARENT.0
            };
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next as isize);
            let applied = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            (applied & WS_EX_TRANSPARENT.0 != 0) == click_through
        }
    }

    fn raise(hwnd: HWND) -> bool {
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            )
            .is_ok()
        }
    }

    pub(super) fn foreground() -> Foreground {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return Foreground { hwnd: 0, exe: None };
        }
        let mut process_id = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(std::ptr::addr_of_mut!(process_id)));
        }
        Foreground {
            hwnd: hwnd.0 as isize,
            exe: crate::mixer::native::process_base_name(process_id),
        }
    }

    fn exclusive_fullscreen() -> bool {
        unsafe { SHQueryUserNotificationState() }
            .is_ok_and(|state| state == QUNS_RUNNING_D3D_FULL_SCREEN)
    }

    fn cursor() -> Option<(i32, i32)> {
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point) }.ok()?;
        Some((point.x, point.y))
    }

    /// The monitor that holds the centre of the Dota window (the foreground window when this is called).
    fn placement(window: &tauri::WebviewWindow<tauri::Wry>, dota: &Foreground) -> Option<Geometry> {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(HWND(dota.hwnd as *mut core::ffi::c_void), &mut rect) }.ok()?;
        let center = (
            ((i64::from(rect.left) + i64::from(rect.right)) / 2) as i32,
            ((i64::from(rect.top) + i64::from(rect.bottom)) / 2) as i32,
        );
        let monitors = window.available_monitors().ok()?;
        let monitor = monitors.iter().find(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            center.0 >= position.x
                && center.1 >= position.y
                && i64::from(center.0) < i64::from(position.x) + i64::from(size.width)
                && i64::from(center.1) < i64::from(position.y) + i64::from(size.height)
        })?;
        let position = monitor.position();
        let size = monitor.size();
        Geometry::new(
            (
                position.x,
                position.y,
                i32::try_from(size.width).ok()?,
                i32::try_from(size.height).ok()?,
            ),
            monitor.scale_factor(),
        )
    }

    fn apply_geometry(window: &tauri::WebviewWindow<tauri::Wry>, geometry: &Geometry) -> bool {
        let (w, h) = geometry.window_physical();
        window
            .set_position(tauri::PhysicalPosition::new(
                geometry.monitor.0,
                geometry.monitor.1,
            ))
            .is_ok()
            && window.set_size(tauri::PhysicalSize::new(w, h)).is_ok()
    }

    fn match_live(app: &tauri::AppHandle<tauri::Wry>) -> bool {
        let state = app.state::<crate::AppState>();
        let phase = lock(&state.runtime.shared).phase;
        matches!(
            phase,
            Some(kohbor_gsi::admission::Phase::Pregame | kohbor_gsi::admission::Phase::Live)
        )
    }

    enum Visibility {
        Keep,
        Show,
        Hide,
    }

    pub(super) fn poll_loop(
        app: tauri::AppHandle<tauri::Wry>,
        window: tauri::WebviewWindow<tauri::Wry>,
        shared: Arc<Mutex<Shared>>,
        stop: Arc<AtomicBool>,
    ) {
        let mut next_placement = Instant::now();
        let mut placed_for: Option<Geometry> = None;
        while !stop.load(Ordering::SeqCst) {
            let now = Instant::now();
            let fg = foreground();
            let fullscreen = exclusive_fullscreen();
            let in_match = match_live(&app);
            if fg.is_dota() && (now >= next_placement || placed_for.is_none()) {
                let geometry = placement(&window, &fg);
                if geometry != placed_for {
                    let placed = geometry
                        .as_ref()
                        .is_some_and(|geometry| apply_geometry(&window, geometry));
                    placed_for = if placed { geometry } else { None };
                    lock(&shared).geometry = placed_for;
                    if !placed {
                        crate::log_warn!(
                            "orb",
                            "placement_unavailable",
                            "orb placement failed; orb hidden"
                        );
                    }
                }
                next_placement = now + PLACEMENT_REFRESH;
            }
            let cursor = cursor();
            let (wants_click_through, visibility, interval) = {
                let mut s = lock(&shared);
                if s.in_match_prev && !in_match {
                    crate::log_info!(
                        "orb",
                        "match_summary",
                        "orb interactions in the match that ended",
                        interactions = s.match_interactions,
                        activations = s.match_activations
                    );
                    s.machine.match_ended();
                    s.match_interactions = 0;
                    s.match_activations = 0;
                }
                s.in_match_prev = in_match;
                if fg.hwnd == s.orb_hwnd && !s.machine.failed_closed {
                    let before = s.last_hover.clone();
                    s.activation("poll", before.as_ref(), &fg);
                }
                // The 100 ms post-click sample.
                let due: Vec<_> = {
                    let (due, waiting) = std::mem::take(&mut s.pending)
                        .into_iter()
                        .partition(|p| p.due <= now);
                    s.pending = waiting;
                    due
                };
                for pending in due {
                    let kept = kept_focus(pending.before.as_ref(), &fg, s.orb_hwnd);
                    crate::log_info!(
                        "orb",
                        "interaction",
                        "orb interaction",
                        n = pending.n,
                        target = pending.target,
                        kept_focus = kept
                    );
                    if !kept && !s.machine.failed_closed {
                        s.activation("click_settle", pending.before.as_ref(), &fg);
                    }
                }
                let eligible = if !in_match {
                    Err(HideReason::NotInMatch)
                } else if !fg.is_dota() {
                    Err(HideReason::DotaNotForeground)
                } else if fullscreen {
                    Err(HideReason::ExclusiveFullscreen)
                } else if s.geometry.is_none() {
                    Err(HideReason::PlacementUnavailable)
                } else {
                    Ok(())
                };
                let zone = match (cursor, s.geometry.as_ref()) {
                    (Some(cursor), Some(geometry)) => {
                        crate::orb::zone(cursor, geometry, s.machine.revealed(), s.machine.panel)
                    }
                    _ => Zone::Outside,
                };
                if zone != Zone::Outside && fg.is_dota() {
                    s.last_hover = Some(fg.clone());
                }
                s.machine.tick(now, eligible, zone);
                if s.last_zone != Some(zone) && s.machine.visible() {
                    let logical = match (cursor, s.geometry.as_ref()) {
                        (Some(c), Some(g)) => format!(
                            "{:.0},{:.0}",
                            f64::from(c.0 - g.monitor.0) / g.scale,
                            f64::from(c.1 - g.monitor.1) / g.scale
                        ),
                        _ => "-".to_owned(),
                    };
                    crate::log_info!(
                        "orb",
                        "zone_changed",
                        "orb pointer zone changed",
                        zone = format!("{zone:?}"),
                        cursor_logical = logical,
                        click_through = s.click_through
                    );
                }
                s.last_zone = Some(zone);
                // Diagnostics: log each change of phase kind with the eligibility inputs (no values, no ids).
                let phase_kind = match s.machine.phase {
                    crate::orb::Phase::Hidden(reason) => format!("hidden:{}", reason.as_str()),
                    crate::orb::Phase::Tucked => "tucked".to_owned(),
                    crate::orb::Phase::Charging { .. } => "charging".to_owned(),
                    crate::orb::Phase::Sliding { .. } => "sliding".to_owned(),
                    crate::orb::Phase::Out { .. } => "out".to_owned(),
                    crate::orb::Phase::Open { .. } => "open".to_owned(),
                };
                if s.last_diag.as_deref() != Some(phase_kind.as_str()) {
                    crate::log_info!(
                        "orb",
                        "phase_changed",
                        "orb phase changed",
                        phase = phase_kind.clone(),
                        in_match = in_match,
                        dota_foreground = fg.is_dota(),
                        foreground_exe = fg.exe.clone().unwrap_or_default(),
                        fullscreen = fullscreen,
                        placed = s.geometry.is_some(),
                        zone = format!("{zone:?}")
                    );
                    s.last_diag = Some(phase_kind);
                }
                let visibility = match (s.machine.visible(), s.shown) {
                    (true, false) => Visibility::Show,
                    (false, true) => Visibility::Hide,
                    _ => Visibility::Keep,
                };
                (
                    !s.machine.wants_input(),
                    visibility,
                    s.machine.poll_interval(),
                )
            };

            // Click-through is restored before anything else; failure hides the orb (fail closed).
            let previous = lock(&shared).click_through;
            if wants_click_through != previous || matches!(visibility, Visibility::Hide) {
                let target = wants_click_through || matches!(visibility, Visibility::Hide);
                let applied = crate::overlay_hwnd(&window)
                    .is_some_and(|hwnd| set_click_through(hwnd, target));
                if !applied {
                    if let Some(hwnd) = crate::overlay_hwnd(&window) {
                        let _ = set_click_through(hwnd, true);
                    }
                    let _ = window.hide();
                    let mut s = lock(&shared);
                    s.shown = false;
                    s.click_through = true;
                    s.machine.fail_closed();
                    crate::log_warn!(
                        "orb",
                        "click_through_failed",
                        "orb hidden: click-through not guaranteed"
                    );
                } else {
                    lock(&shared).click_through = target;
                }
            }
            match visibility {
                Visibility::Show => {
                    let shown = crate::overlay_hwnd(&window).is_some_and(|hwnd| {
                        has_no_activate_style(hwnd)
                            && lock(&shared).click_through
                            && crate::show_without_activation(&window)
                            && raise(hwnd)
                    });
                    lock(&shared).shown = shown;
                    if !shown {
                        let _ = window.hide();
                        crate::log_warn!(
                            "orb",
                            "show_failed",
                            "orb could not be shown without activation"
                        );
                    }
                }
                Visibility::Hide => {
                    if window.hide().is_err() {
                        crate::log_warn!("orb", "hide_failed", "orb hide failed");
                    }
                    lock(&shared).shown = false;
                }
                Visibility::Keep => {}
            }
            emit_state(&app, &shared, false);
            std::thread::sleep(interval);
        }
    }
}
