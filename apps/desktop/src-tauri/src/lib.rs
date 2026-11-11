//! Kohbor desktop host (M1 slice): containment, game-state intake, runtime status.

pub mod atomic;
pub mod containment;
pub mod instance;
pub mod intake;
pub mod secrets;
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
use intake::lock;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Manager;

struct AppState {
    runtime: Arc<intake::Runtime>,
    sample: Arc<Mutex<(Sample, bool)>>,
    revision: AtomicU64,
}

#[tauri::command]
fn runtime_get_status(state: tauri::State<'_, AppState>) -> Value {
    let rev = state.revision.fetch_add(1, Ordering::Relaxed) + 1;
    let (sample, throttled) = *lock(&state.sample);
    let shared = lock(&state.runtime.shared);
    status::build(&shared, &sample, throttled, rev)
}

#[tauri::command]
fn own_snapshot(state: tauri::State<'_, AppState>) -> Value {
    lock(&state.runtime.shared).snapshot()
}

/// User action: write the owned game configuration and start listening.
///
/// `replace` is the separate, explicit confirmation to replace an owned file that
/// was edited outside the app. Runs off the main thread: it does file and port work.
#[tauri::command(async)]
fn gsi_setup(state: tauri::State<'_, AppState>, replace: Option<bool>) -> Result<(), String> {
    state
        .runtime
        .setup(replace.unwrap_or(false))
        .map_err(str::to_owned)
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let runtime = intake::Runtime::new(dir);
            runtime.start();
            let sample = Arc::new(Mutex::new((
                Sample {
                    cpu_percent: None,
                    working_set_bytes: None,
                    process_count: None,
                },
                true,
            )));
            let sink = sample.clone();
            std::thread::Builder::new()
                .name("resource-sampler".into())
                .spawn(move || {
                    let mut sampler = Sampler::new();
                    let mut throttle = Throttle::default();
                    loop {
                        let s = sampler.sample();
                        let t = throttle.update(&s);
                        *lock(&sink) = (s, t);
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                })?;
            app.manage(AppState {
                runtime,
                sample,
                revision: AtomicU64::new(0),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            runtime_get_status,
            own_snapshot,
            gsi_setup
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kohbor");
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
