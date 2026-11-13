//! Kohbor desktop host (M1 slice): containment, game-state intake, runtime status.

pub mod atomic;
mod audio;
mod clock;
mod commands;
pub mod containment;
pub mod host;
pub mod instance;
pub mod intake;
pub mod latency;
mod media;
pub mod secrets;
pub mod settings;
pub mod status;

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
        let demo = std::env::var("KOHBOR_DEMO").ok();
        let requested = std::env::var("KOHBOR_DATA_DIR").ok();
        if let Some(dir) = demo::data_dir_override(demo.as_deref(), requested.as_deref()) {
            std::fs::create_dir_all(&dir)?;
            return Ok(dir);
        }
    }
    app.path().app_data_dir()
}

/// Place the overlay on the primary monitor. Returns false (and the caller keeps the window hidden)
/// when the monitor or the geometry cannot be applied. Called again before every show so a changed
/// display layout is followed.
fn place_overlay_window(window: &tauri::WebviewWindow<tauri::Wry>) -> bool {
    let monitor = match window.primary_monitor() {
        Ok(Some(monitor)) => monitor,
        Ok(None) => {
            eprintln!("overlay placement unavailable: no primary monitor");
            return false;
        }
        Err(error) => {
            eprintln!("overlay placement unavailable: {error}");
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
        eprintln!("overlay placement failed: {error}");
        return false;
    }
    true
}

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
    .inner_size(
        f64::from(overlay::WINDOW_LOGICAL.0),
        f64::from(overlay::WINDOW_LOGICAL.1),
    )
    .build()
    {
        Ok(window) => window,
        Err(error) => {
            eprintln!("overlay window unavailable: {error}");
            return None;
        }
    };

    // A failed placement leaves the window hidden for good: it is placed again before every show.
    let _ = place_overlay_window(&window);

    if let Err(error) = window.set_ignore_cursor_events(spec.ignore_cursor_events) {
        eprintln!("overlay click-through unavailable: {error}");
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
                eprintln!("overlay hide failed: {error}");
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
            eprintln!("overlay show failed: {error}");
            let _ = window.hide();
            return;
        }
        if window.set_ignore_cursor_events(true).is_err() {
            let _ = window.hide();
            eprintln!("overlay click-through failed after show; window hidden");
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
            eprintln!("overlay status reply failed: {error}");
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
                        eprintln!("overlay status update failed: {error}");
                    }
                }
            }
        })
    {
        eprintln!("overlay status mirror unavailable: {error}");
    }
    // Deliver the current mirror once at startup. `overlay://ready` replays it
    // after the overlay webview has installed its listener.
    if let Err(error) = app.emit("overlay://status", intake::lock(&last_status).clone()) {
        eprintln!("overlay status initial update failed: {error}");
    }
}

pub fn run() {
    let builder = tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("kohbor-media", commands::media_protocol_handler)
        .setup(|app| {
            let dir = data_dir(app)?;
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
            let host = host::Host::start(dir, runtime.clone(), sample.clone(), emitter)?;
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
                eprintln!("resource-sampler unavailable: {error}");
            }
            app.manage(AppState {
                runtime,
                host,
                settings_dir,
                settings_gate: Mutex::new(()),
                sample,
                revision: AtomicU64::new(0),
            });
            let app_handle = app.handle().clone();
            let last_overlay_status = Arc::new(Mutex::new(initial_overlay_status.clone()));
            register_overlay_events(
                app_handle.clone(),
                overlay_window,
                last_overlay_status.clone(),
            );
            start_overlay_status_mirror(app_handle, last_overlay_status, initial_overlay_status);
            Ok(())
        });
    #[cfg(any(feature = "demo", debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
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
        commands::demo_available,
        commands::demo_trigger
    ]);
    #[cfg(not(any(feature = "demo", debug_assertions)))]
    let builder = builder.invoke_handler(tauri::generate_handler![
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
        commands::demo_available
    ]);
    let app = builder.build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                handle.state::<AppState>().host.shutdown();
            }
        }),
        Err(error) => eprintln!("error while building Kohbor: {error}"),
    }
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
