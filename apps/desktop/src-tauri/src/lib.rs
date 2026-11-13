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
use tauri::{Emitter as TauriEmitter, Manager};

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

pub fn run() {
    let app = tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("kohbor-media", commands::media_protocol_handler)
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
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
            let gsi_runtime = runtime.clone();
            let gsi = capture_host::GsiView::new(move || {
                capture_host::GsiSnapshot::from_snapshot(
                    &intake::lock(&gsi_runtime.shared).snapshot(),
                )
            });
            let resource_sample = sample.clone();
            let resources = capture_host::ResourceView::new(move || {
                let (s, _) = *intake::lock(&resource_sample);
                capture_host::ResourceSample {
                    cpu_percent: s.cpu_percent,
                    working_set_bytes: s.working_set_bytes,
                }
            });
            capture_host::start(app.handle().clone(), gsi, resources);
            app.manage(AppState {
                runtime,
                host,
                settings_dir,
                settings_gate: Mutex::new(()),
                sample,
                revision: AtomicU64::new(0),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            capture_host::capture_invalidate
        ])
        .build(tauri::generate_context!());
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
