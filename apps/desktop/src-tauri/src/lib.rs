//! Kohbor desktop host (M1 slice): containment, game-state intake, runtime status.

pub mod atomic;
mod audio;
mod capture_host;
mod clock;
mod commands;
pub mod containment;
pub mod host;
pub mod instance;
pub mod intake;
pub mod latency;
pub mod logging;
mod media;
mod portraits;
pub mod secrets;
pub mod settings;
pub mod status;
mod steam;
mod version;

/// Runs the application only after containment succeeds.
///
/// On failure, reports a sanitized error containing the failed containment step
/// and returns exit code 1 without launching the UI or sidecar path.
pub fn start_with(
    establish: impl FnOnce() -> Result<(), &'static str>,
    launch: impl FnOnce(),
    report: impl FnOnce(&str),
) -> i32 {
    match establish() {
        Ok(()) => {
            launch();
            0
        }
        Err(step) => {
            report(&format!("CPU_CONTAINMENT_FAILED ({step})"));
            1
        }
    }
}

use containment::{Sample, Sampler, Throttle};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter as TauriEmitter, Listener, Manager};

pub(crate) struct AppState {
    pub(crate) runtime: Arc<intake::Runtime>,
    pub(crate) host: host::Host,
    pub(crate) settings_dir: PathBuf,
    /// Serializes all commands that may replace user-settings.json.
    pub(crate) settings_gate: Mutex<()>,
    pub(crate) sample: Arc<Mutex<(Sample, bool)>>,
    pub(crate) revision: AtomicU64,
}

impl host::Emitter for tauri::AppHandle<tauri::Wry> {
    fn emit(&self, event: &str, payload: Value) {
        let _ = TauriEmitter::emit(self, event, payload);
    }
}

/// The application data directory. Demo builds may redirect it with `KOHBOR_DATA_DIR` (only together
/// with `KOHBOR_DEMO=1`) so the demo never touches the real packs; release builds ignore the variable.
fn data_dir(app: &tauri::App<tauri::Wry>) -> tauri::Result<std::path::PathBuf> {
    #[cfg(any(feature = "demo", debug_assertions))]
    {
        if let Some(dir) = demo_data_dir_override() {
            std::fs::create_dir_all(&dir)?;
            return Ok(dir);
        }
    }
    app.path().app_data_dir()
}

#[cfg(any(feature = "demo", debug_assertions))]
fn demo_data_dir_override() -> Option<std::path::PathBuf> {
    let demo = std::env::var("KOHBOR_DEMO").ok();
    let requested = std::env::var("KOHBOR_DATA_DIR").ok();
    demo::data_dir_override(demo.as_deref(), requested.as_deref())
}

#[cfg(not(any(feature = "demo", debug_assertions)))]
fn demo_data_dir_override() -> Option<std::path::PathBuf> {
    None
}

/// Initialise logging before the instance guard or Tauri setup can fail. On Windows,
/// `app.path().app_data_dir()` resolves the roaming data root plus the configured bundle
/// identifier; the demo override follows the same rule as `data_dir()` below.
pub fn initialize_logging() {
    let data_dir = demo_data_dir_override().or_else(|| {
        let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let identifier = context.config().identifier.clone();
        #[cfg(windows)]
        let root = std::env::var_os("APPDATA").map(std::path::PathBuf::from);
        #[cfg(not(windows))]
        let root = std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .map(|home| home.join(".local").join("share"))
            });
        root.map(|path| path.join(identifier))
    });
    logging::initialize(data_dir.as_deref());
}

/// Place the overlay on the primary monitor. Returns false (and the caller keeps the window hidden)
/// when the monitor or the geometry cannot be applied. Called again before every show so a changed
/// display layout is followed.
fn place_overlay_window(window: &tauri::WebviewWindow<tauri::Wry>) -> bool {
    let monitor = match window.primary_monitor() {
        Ok(Some(monitor)) => monitor,
        Ok(None) => {
            crate::log_warn!("overlay", "placement_unavailable", "no primary monitor");
            return false;
        }
        Err(error) => {
            crate::log_warn!("overlay", "placement_unavailable", format!("{error}"));
            return false;
        }
    };
    let position = monitor.position();
    let size = monitor.size();
    let rect = overlay::place_overlay(&overlay::MonitorRect {
        x: position.x,
        y: position.y,
        w: size.width,
        h: size.height,
    });
    let moved = window.set_position(tauri::PhysicalPosition::new(rect.x, rect.y));
    let sized = window.set_size(tauri::PhysicalSize::new(rect.w, rect.h));
    if let Err(error) = moved.as_ref().and(sized.as_ref()) {
        crate::log_warn!("overlay", "placement_failed", format!("{error}"));
        return false;
    }
    true
}

/// WebView2 arguments shared by every window (WebView2 requires equal arguments for windows that
/// share one data directory). They keep Chromium from throttling or pausing the Live minimap when the
/// window is behind the borderless game. Must equal `additionalBrowserArgs` in tauri.conf.json.
const WEBVIEW2_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows";

fn build_overlay_window(
    app: &mut tauri::App<tauri::Wry>,
) -> Option<tauri::WebviewWindow<tauri::Wry>> {
    let spec = overlay::overlay_window_spec();
    let window = match tauri::WebviewWindowBuilder::new(
        app,
        spec.label,
        tauri::WebviewUrl::App(spec.url.into()),
    )
    .transparent(spec.transparent)
    .decorations(spec.decorations)
    .always_on_top(spec.always_on_top)
    .skip_taskbar(spec.skip_taskbar)
    .focusable(spec.focusable)
    .focused(spec.focused)
    .shadow(spec.shadow)
    .resizable(spec.resizable)
    .visible(spec.visible)
    .additional_browser_args(WEBVIEW2_BROWSER_ARGS)
    .inner_size(
        f64::from(overlay::WINDOW_LOGICAL.0),
        f64::from(overlay::WINDOW_LOGICAL.1),
    )
    .build()
    {
        Ok(window) => window,
        Err(error) => {
            crate::log_warn!("overlay", "window_unavailable", format!("{error}"));
            return None;
        }
    };

    // A failed placement leaves the window hidden for good: it is placed again before every show.
    let _ = place_overlay_window(&window);

    if let Err(error) = window.set_ignore_cursor_events(spec.ignore_cursor_events) {
        crate::log_warn!("overlay", "click_through_unavailable", format!("{error}"));
    }
    Some(window)
}

fn register_overlay_events(
    app: tauri::AppHandle<tauri::Wry>,
    window: Option<tauri::WebviewWindow<tauri::Wry>>,
    last_status: Arc<Mutex<Value>>,
) {
    let visible_window = window.clone();
    app.listen("overlay://visible", move |event| {
        let Ok(payload) = serde_json::from_str::<Value>(event.payload()) else {
            return;
        };
        let Some(values) = payload.as_object() else {
            return;
        };
        if values.len() != 1 {
            return;
        }
        let Some(visible) = values.get("visible").and_then(Value::as_bool) else {
            return;
        };
        let Some(window) = visible_window.as_ref() else {
            return;
        };
        if !visible {
            if let Err(error) = window.hide() {
                crate::log_warn!("overlay", "hide_failed", format!("{error}"));
            }
            return;
        }
        // Fail closed: never show an unplaced window, and never leave a shown window that can
        // intercept game input (click-through is applied before and again after the show).
        if !place_overlay_window(window) || window.set_ignore_cursor_events(true).is_err() {
            let _ = window.hide();
            return;
        }
        if let Err(error) = window.show() {
            crate::log_warn!("overlay", "show_failed", format!("{error}"));
            let _ = window.hide();
            return;
        }
        if window.set_ignore_cursor_events(true).is_err() {
            let _ = window.hide();
            crate::log_warn!(
                "overlay",
                "click_through_failed",
                "window hidden after failure"
            );
        }
    });

    let ready_handle = app.clone();
    app.listen("overlay://ready", move |event| {
        let Ok(payload) = serde_json::from_str::<Value>(event.payload()) else {
            return;
        };
        if !payload.as_object().is_some_and(|values| values.is_empty()) {
            return;
        }
        let payload = intake::lock(&last_status).clone();
        if let Err(error) = ready_handle.emit("overlay://status", payload) {
            crate::log_warn!("overlay", "status_reply_failed", format!("{error}"));
        }
    });
}

fn start_overlay_status_mirror(
    app: tauri::AppHandle<tauri::Wry>,
    last_status: Arc<Mutex<Value>>,
    initial_status: Value,
) {
    let app_for_thread = app.clone();
    let last_for_thread = last_status.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("overlay-status-mirror".into())
        .spawn(move || {
            let mut previous = initial_status;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let state = app_for_thread.state::<AppState>();
                if state.host.shutdown_started() {
                    break;
                }
                let (media_state, reason) = state.host.media_status();
                let current = overlay::status_payload(media_state, reason);

                if current != previous {
                    previous = current.clone();
                    *intake::lock(&last_for_thread) = current.clone();
                    if let Err(error) = app_for_thread.emit("overlay://status", current) {
                        crate::log_warn!("overlay", "status_update_failed", format!("{error}"));
                    }
                }
            }
        })
    {
        crate::log_warn!("overlay", "status_mirror_unavailable", format!("{error}"));
    }
    // Deliver the current mirror once at startup. `overlay://ready` replays it
    // after the overlay webview has installed its listener.
    if let Err(error) = app.emit("overlay://status", intake::lock(&last_status).clone()) {
        crate::log_warn!(
            "overlay",
            "initial_status_update_failed",
            format!("{error}")
        );
    }
}

pub fn run() {
    initialize_logging();
    let app_version = version::AppVersion::current();
    let commit = app_version
        .git_commit
        .as_deref()
        .map(|value| value.chars().take(7).collect::<String>());
    crate::log_info!(
        "app",
        "started",
        "Kohbor started",
        version = app_version.version,
        commit = commit,
        profile = app_version.profile,
        demo = app_version.demo,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH
    );
    let builder = tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("kohbor-media", commands::media_protocol_handler)
        .setup(|app| {
            // The control window title stays "Kohbor — Command Deck": instance.rs finds the
            // existing window by that exact title for second-instance focus. The version is shown
            // in the header and the Status tab instead.
            let dir = data_dir(app).inspect_err(|_error| {
                crate::log_error!("app", "setup_failed", "APP_DATA_DIR_UNAVAILABLE");
            })?;
            let settings_dir = dir.clone();
            let runtime = intake::Runtime::new(dir.clone());
            let sample = Arc::new(Mutex::new((
                Sample {
                    cpu_percent: None,
                    working_set_bytes: None,
                    process_count: None,
                },
                true,
            )));
            let emitter: Arc<dyn host::Emitter> = Arc::new(app.handle().clone());
            let host = host::Host::start(dir, runtime.clone(), sample.clone(), emitter)
                .inspect_err(|_error| {
                    crate::log_error!("app", "setup_failed", "HOST_SETUP_FAILED");
                })?;
            let overlay_window = build_overlay_window(app);
            let (initial_media_state, initial_reason) = host.media_status();
            let initial_overlay_status =
                overlay::status_payload(initial_media_state, initial_reason);
            let sink = sample.clone();
            if let Err(error) = std::thread::Builder::new()
                .name("resource-sampler".into())
                .spawn(move || {
                    let mut sampler = Sampler::new();
                    let mut throttle = Throttle::default();
                    loop {
                        let s = sampler.sample();
                        let t = throttle.update(&s);
                        *intake::lock(&sink) = (s, t);
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                })
            {
                crate::log_warn!("resources", "sampler_unavailable", format!("{error}"));
            }
            let gsi_runtime = runtime.clone();
            let gsi = capture_host::GsiView::new(move || {
                let shared = intake::lock(&gsi_runtime.shared);
                let mut view = capture_host::GsiSnapshot::from_snapshot(&shared.snapshot());
                // In match only while the intake phase is Live (pregame also exposes own data).
                view.in_match = shared.phase == Some(kohbor_gsi::Phase::Live);
                view.game_phase = shared.phase.map(capture_host::GamePhase::from);
                view.recent_events = shared
                    .recent_health_events(std::time::Instant::now(), 5000)
                    .into_iter()
                    .map(|(id, at_ms)| capture_host::GsiEvent { id, at_ms })
                    .collect();
                view
            });
            let resource_sample = sample.clone();
            let resources = capture_host::ResourceView::new(move || {
                let (s, _) = *intake::lock(&resource_sample);
                capture_host::ResourceSample {
                    cpu_percent: s.cpu_percent,
                    working_set_bytes: s.working_set_bytes,
                }
            });
            capture_host::start(app.handle().clone(), gsi, resources, settings_dir.clone());
            let steam_runtime = runtime.clone();
            app.manage(AppState {
                runtime,
                host,
                settings_dir,
                settings_gate: Mutex::new(()),
                sample,
                revision: AtomicU64::new(0),
            });
            steam::start_status_probe(app.handle().clone(), steam_runtime);
            let app_handle = app.handle().clone();
            let last_overlay_status = Arc::new(Mutex::new(initial_overlay_status.clone()));
            register_overlay_events(
                app_handle.clone(),
                overlay_window,
                last_overlay_status.clone(),
            );
            start_overlay_status_mirror(app_handle, last_overlay_status, initial_overlay_status);
            crate::log_info!("app", "setup_complete", "application setup completed");
            Ok(())
        });
    #[cfg(any(feature = "demo", debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::app_version,
        portraits::hero_portrait,
        steam::launch_dota,
        steam::steam_status,
        commands::log_tail,
        commands::log_dir,
        commands::log_open_folder,
        commands::log_frontend,
        commands::log_set_level,
        commands::runtime_get_status,
        commands::own_snapshot,
        commands::gsi_setup,
        commands::settings_get,
        commands::settings_patch,
        commands::media_inspect,
        commands::media_install,
        commands::media_activate,
        commands::media_list,
        commands::media_probe_report,
        commands::media_probe_pending,
        commands::audio_history,
        commands::set_muted,
        capture_host::minimap_subscribe,
        capture_host::capture_status,
        capture_host::capture_invalidate,
        capture_host::capture_retry,
        capture_host::capture_get_config,
        capture_host::capture_set_config,
        capture_host::capture_reset_crop,
        capture_host::capture_propose_crop,
        capture_host::capture_accept_proposal,
        capture_host::capture_confirm_crop,
        commands::demo_available,
        commands::demo_trigger
    ]);
    #[cfg(not(any(feature = "demo", debug_assertions)))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::app_version,
        portraits::hero_portrait,
        steam::launch_dota,
        steam::steam_status,
        commands::log_tail,
        commands::log_dir,
        commands::log_open_folder,
        commands::log_frontend,
        commands::log_set_level,
        commands::runtime_get_status,
        commands::own_snapshot,
        commands::gsi_setup,
        commands::settings_get,
        commands::settings_patch,
        commands::media_inspect,
        commands::media_install,
        commands::media_activate,
        commands::media_list,
        commands::media_probe_report,
        commands::media_probe_pending,
        commands::audio_history,
        commands::set_muted,
        capture_host::minimap_subscribe,
        capture_host::capture_status,
        capture_host::capture_invalidate,
        capture_host::capture_retry,
        capture_host::capture_get_config,
        capture_host::capture_set_config,
        capture_host::capture_reset_crop,
        capture_host::capture_propose_crop,
        capture_host::capture_accept_proposal,
        capture_host::capture_confirm_crop,
        commands::demo_available
    ]);
    let app = builder.build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                crate::log_info!("app", "exit", "Kohbor is exiting");
                handle.state::<AppState>().host.shutdown();
                logging::flush();
            }
        }),
        Err(_error) => crate::log_error!("app", "build_failed", "TAURI_BUILD_FAILED"),
    }
    logging::flush();
}

#[cfg(test)]
mod containment_failure_tests {
    use super::{containment, start_with};
    use std::cell::{Cell, RefCell};

    const STEPS: [&str; 4] = ["create", "cpu-rate", "limits", "assign"];

    struct FakeBackend {
        fail_at: Option<&'static str>,
        calls: Vec<&'static str>,
    }

    impl FakeBackend {
        fn step(&mut self, name: &'static str) -> Result<(), ()> {
            self.calls.push(name);
            if self.fail_at == Some(name) {
                Err(())
            } else {
                Ok(())
            }
        }
    }

    impl containment::JobBackend for FakeBackend {
        fn create(&mut self) -> Result<(), ()> {
            self.step("create")
        }
        fn configure_cpu_rate(&mut self) -> Result<(), ()> {
            self.step("cpu-rate")
        }
        fn configure_limits(&mut self) -> Result<(), ()> {
            self.step("limits")
        }
        fn assign(&mut self) -> Result<(), ()> {
            self.step("assign")
        }
    }

    #[test]
    fn establishment_stops_at_each_failure() {
        for (index, stage) in STEPS.iter().copied().enumerate() {
            let mut backend = FakeBackend {
                fail_at: Some(stage),
                calls: vec![],
            };
            assert_eq!(containment::establish_with(&mut backend), Err(stage));
            assert_eq!(backend.calls, STEPS[..=index], "failure at {stage}");
        }
    }

    #[test]
    fn establishment_succeeds_in_order() {
        let mut backend = FakeBackend {
            fail_at: None,
            calls: vec![],
        };
        assert_eq!(containment::establish_with(&mut backend), Ok(()));
        assert_eq!(backend.calls, STEPS);
    }

    #[test]
    fn startup_reports_only_the_safe_failure_and_never_launches() {
        for stage in STEPS {
            let launches = Cell::new(0);
            let reports = RefCell::new(Vec::new());
            let code = start_with(
                || Err(stage),
                || launches.set(launches.get() + 1),
                |message| reports.borrow_mut().push(message.to_owned()),
            );
            assert_eq!(code, 1);
            assert_eq!(launches.get(), 0);
            assert_eq!(
                *reports.borrow(),
                [format!("CPU_CONTAINMENT_FAILED ({stage})")]
            );
        }
    }

    #[test]
    fn startup_launches_once_after_success_without_reporting() {
        let launches = Cell::new(0);
        let reports = Cell::new(0);
        let code = start_with(
            || Ok(()),
            || launches.set(launches.get() + 1),
            |_| reports.set(reports.get() + 1),
        );
        assert_eq!(code, 0);
        assert_eq!(launches.get(), 1);
        assert_eq!(reports.get(), 0);
    }

    #[test]
    fn backend_failures_prevent_startup_launch() {
        for stage in STEPS {
            let mut backend = FakeBackend {
                fail_at: Some(stage),
                calls: vec![],
            };
            let launches = Cell::new(0);
            let reports = RefCell::new(Vec::new());
            let code = start_with(
                || containment::establish_with(&mut backend),
                || launches.set(launches.get() + 1),
                |message| reports.borrow_mut().push(message.to_owned()),
            );
            assert_eq!(code, 1);
            assert_eq!(launches.get(), 0);
            assert_eq!(
                *reports.borrow(),
                [format!("CPU_CONTAINMENT_FAILED ({stage})")]
            );
        }
    }
}

mod demo;
mod overlay;

#[cfg(test)]
mod webview_args_tests {
    #[test]
    fn overlay_and_control_windows_use_the_same_browser_args() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(
            conf["app"]["windows"][0]["additionalBrowserArgs"].as_str(),
            Some(super::WEBVIEW2_BROWSER_ARGS)
        );
    }
}
