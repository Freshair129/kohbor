mod diagnostics;
mod prefs;
mod tone;

use kohbor_events::{ObjectiveAlertBatch, ObjectiveId, ObjectivePhase};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use tauri::{Emitter, Manager};

pub(crate) use diagnostics::MapDiagnostic;
pub(crate) use prefs::{ObjectivePrefs, ObjectivePrefsService, ObjectiveRuntimeFlags};

const ALERT_QUEUE_CAPACITY: usize = 32;

pub(crate) struct ObjectiveIntakeWiring {
    pub(crate) flags: std::sync::Arc<ObjectiveRuntimeFlags>,
    pub(crate) diagnostics: SyncSender<MapDiagnostic>,
}

pub(crate) struct ObjectiveHost {
    pub(crate) prefs: ObjectivePrefsService,
    diagnostic_sender: SyncSender<MapDiagnostic>,
}

impl ObjectiveHost {
    pub(crate) fn open(data_dir: &Path, app_logs_dir: &Path) -> Self {
        let prefs = ObjectivePrefsService::open(data_dir);
        let (diagnostic_sender, diagnostic_receiver) = mpsc::sync_channel(32);
        diagnostics::start_writer(
            diagnostic_receiver,
            app_logs_dir.to_path_buf(),
            prefs.flags().diagnostic_enabled(),
        );
        Self {
            prefs,
            diagnostic_sender,
        }
    }

    pub(crate) fn intake_wiring(&self) -> ObjectiveIntakeWiring {
        ObjectiveIntakeWiring {
            flags: self.prefs.flags(),
            diagnostics: self.diagnostic_sender.clone(),
        }
    }

    pub(crate) fn new_alert_channel() -> (
        SyncSender<ObjectiveAlertBatch>,
        Receiver<ObjectiveAlertBatch>,
    ) {
        mpsc::sync_channel(ALERT_QUEUE_CAPACITY)
    }
}

pub(crate) fn start_alert_worker(
    app: tauri::AppHandle<tauri::Wry>,
    receiver: Receiver<ObjectiveAlertBatch>,
) {
    if let Err(error) = thread::Builder::new()
        .name("objective-alerts".into())
        .spawn(move || {
            while let Ok(batch) = receiver.recv() {
                let state = app.state::<crate::AppState>();
                if state.host.shutdown_started() {
                    break;
                }
                let payload = alert_payload(&batch);
                let _ = app.emit("objectives://alert", payload);
                let _ = state
                    .host
                    .try_play_objective_tone(tone::synthesize_soft_beep_beep());
            }
        })
    {
        crate::log_warn!("objectives", "alert_worker_unavailable", format!("{error}"));
    }
}

fn alert_payload(batch: &ObjectiveAlertBatch) -> Value {
    let alerts: Vec<Value> = batch
        .alerts
        .iter()
        .map(|alert| {
            json!({
                "objective": alert.objective.as_str(),
                "phase": alert.phase.as_str(),
                "occurrence_seconds": alert.occurrence_seconds,
                "due_seconds": alert.due_seconds,
            })
        })
        .collect();
    json!({
        "schema_version": 1,
        "observed_clock_seconds": batch.observed_clock_seconds,
        "alerts": alerts,
    })
}

#[cfg(test)]
mod tests {
    use super::{alert_payload, ObjectiveAlertBatch};
    use kohbor_events::{ObjectiveAlert, ObjectiveId, ObjectivePhase};

    #[test]
    fn alert_payload_is_a_small_closed_host_event() {
        let payload = alert_payload(&ObjectiveAlertBatch {
            observed_clock_seconds: 120,
            alerts: vec![ObjectiveAlert {
                objective: ObjectiveId::WaterRune,
                phase: ObjectivePhase::Spawn,
                occurrence_seconds: 120,
                due_seconds: 120,
            }],
        });
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["alerts"][0]["objective"], "water_rune");
        assert_eq!(payload["alerts"][0]["phase"], "spawn");
        assert!(payload.get("player").is_none());
    }
}
