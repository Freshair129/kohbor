//! Kohbor desktop host (M1 slice): containment, game-state intake, runtime status.

pub mod containment;
pub mod intake;
pub mod secrets;
pub mod status;

use containment::{Sample, Sampler, Throttle};
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
    let (sample, throttled) = *state.sample.lock().unwrap();
    let shared = state.runtime.shared.lock().unwrap();
    status::build(&shared, &sample, throttled, rev)
}

#[tauri::command]
fn own_snapshot(state: tauri::State<'_, AppState>) -> Value {
    state.runtime.shared.lock().unwrap().snapshot()
}

/// User action: write the owned game configuration and start listening.
#[tauri::command]
fn gsi_setup(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.runtime.setup().map_err(str::to_owned)
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
                        if let Ok(mut slot) = sink.lock() {
                            *slot = (s, t);
                        }
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
