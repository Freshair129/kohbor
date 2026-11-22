mod diagnostics;
mod prefs;
mod tone;

use kohbor_events::{ObjectiveAlertBatch, ObjectiveId};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;
use tauri::{Emitter, Manager};

pub(crate) use diagnostics::MapDiagnostic;
pub(crate) use prefs::{ObjectivePrefsService, ObjectiveRuntimeFlags};

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
                let locale = app_locale(&state);
                let payload = alert_payload(&batch, locale);
                let _ = app.emit_to("hud", "hud://objective-alert", payload);
                let objective = tone_priority(&batch);
                let result = state
                    .host
                    .try_play_objective_tone(tone::synthesize(objective));
                let log_batch = batch.clone();
                let spawned = thread::Builder::new()
                    .name("objective-tone-outcome".into())
                    .spawn(move || {
                        let outcome = result
                            .and_then(|receiver| receiver.recv_timeout(Duration::from_secs(6)).ok())
                            .unwrap_or(crate::audio::lanes::ObjectiveToneOutcome::Off);
                        log_alert_shown(&log_batch, outcome.as_str());
                    });
                if spawned.is_err() {
                    log_alert_shown(&batch, "off");
                }
            }
        })
    {
        crate::log_warn!("objectives", "alert_worker_unavailable", format!("{error}"));
    }
}

/// The HUD window is listen-only and may start after the control window published its
/// language, so every alert carries the saved app locale; unreadable settings send none.
fn app_locale(state: &crate::AppState) -> Option<&'static str> {
    let settings = {
        let _guard = crate::intake::lock(&state.settings_gate);
        crate::settings::get(&state.settings_dir).ok()?
    };
    locale_from_settings(&settings)
}

fn locale_from_settings(settings: &Value) -> Option<&'static str> {
    match settings.get("locale").and_then(Value::as_str) {
        Some("th") => Some("th"),
        Some("en") => Some("en"),
        _ => None,
    }
}

fn alert_payload(batch: &ObjectiveAlertBatch, locale: Option<&'static str>) -> Value {
    let alerts: Vec<Value> = batch
        .alerts
        .iter()
        .map(|alert| {
            json!({
                "objective": alert.objective.as_str(),
                "phase": alert.phase.as_str(),
                "lead_s": alert.phase.lead_s(),
                "occurrence_seconds": alert.occurrence_seconds,
                "due_seconds": alert.due_seconds,
            })
        })
        .collect();
    let mut payload = json!({
        "schema_version": 1,
        "observed_clock_seconds": batch.observed_clock_seconds,
        "alerts": alerts,
    });
    if let Some(locale) = locale {
        payload["locale"] = json!(locale);
    }
    payload
}

fn tone_priority(batch: &ObjectiveAlertBatch) -> ObjectiveId {
    [
        ObjectiveId::Tormentor,
        ObjectiveId::PowerRune,
        ObjectiveId::WaterRune,
        ObjectiveId::WisdomRune,
        ObjectiveId::BountyRune,
    ]
    .into_iter()
    .find(|objective| {
        batch
            .alerts
            .iter()
            .any(|alert| alert.objective == *objective)
    })
    .unwrap_or(ObjectiveId::BountyRune)
}

fn log_fields(batch: &ObjectiveAlertBatch, tone: &str) -> Value {
    let kinds: Vec<&str> = batch
        .alerts
        .iter()
        .map(|alert| alert.objective.as_str())
        .collect();
    let lead_s: Vec<String> = batch
        .alerts
        .iter()
        .map(|alert| {
            alert
                .phase
                .lead_s()
                .map_or_else(|| "spawn".to_owned(), |lead| lead.to_string())
        })
        .collect();
    json!({
        "kind": kinds.first().copied().unwrap_or("unknown"),
        "kinds": kinds.join(","),
        "lead_s": lead_s.join(","),
        "clock": batch.observed_clock_seconds,
        "tone": tone,
    })
}

fn log_alert_shown(batch: &ObjectiveAlertBatch, tone: &str) {
    let fields = log_fields(batch, tone);
    crate::log_info!(
        "objectives",
        "objective_alert_shown",
        "objective alert shown",
        kind = fields["kind"].clone(),
        kinds = fields["kinds"].clone(),
        lead_s = fields["lead_s"].clone(),
        clock = fields["clock"].clone(),
        tone = fields["tone"].clone()
    );
}

#[cfg(test)]
mod tests {
    use super::{
        alert_payload, locale_from_settings, log_fields, tone_priority, ObjectiveAlertBatch,
    };
    use kohbor_events::{ObjectiveAlert, ObjectiveId, ObjectivePhase};
    use serde_json::json;

    #[test]
    fn alert_payload_is_a_small_closed_host_event() {
        let payload = alert_payload(
            &ObjectiveAlertBatch {
                observed_clock_seconds: 120,
                alerts: vec![ObjectiveAlert {
                    objective: ObjectiveId::WaterRune,
                    phase: ObjectivePhase::Spawn,
                    occurrence_seconds: 120,
                    due_seconds: 120,
                }],
            },
            Some("th"),
        );
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["locale"], "th");
        assert_eq!(payload["alerts"][0]["objective"], "water_rune");
        assert_eq!(payload["alerts"][0]["phase"], "spawn");
        assert!(payload["alerts"][0]["lead_s"].is_null());
        assert!(payload.get("player").is_none());
    }

    #[test]
    fn alert_payload_carries_only_a_known_app_locale() {
        assert_eq!(locale_from_settings(&json!({"locale": "th"})), Some("th"));
        assert_eq!(locale_from_settings(&json!({"locale": "en"})), Some("en"));
        assert_eq!(locale_from_settings(&json!({"locale": "fr"})), None);
        assert_eq!(locale_from_settings(&json!({})), None);
        let batch = ObjectiveAlertBatch {
            observed_clock_seconds: 120,
            alerts: vec![ObjectiveAlert {
                objective: ObjectiveId::PowerRune,
                phase: ObjectivePhase::Spawn,
                occurrence_seconds: 120,
                due_seconds: 120,
            }],
        };
        assert!(alert_payload(&batch, None).get("locale").is_none());
        assert_eq!(alert_payload(&batch, Some("en"))["locale"], "en");
    }

    #[test]
    fn batch_tone_uses_the_fixed_objective_priority() {
        let batch = ObjectiveAlertBatch {
            observed_clock_seconds: 120,
            alerts: vec![
                kohbor_events::ObjectiveAlert {
                    objective: ObjectiveId::WaterRune,
                    phase: ObjectivePhase::Spawn,
                    occurrence_seconds: 120,
                    due_seconds: 120,
                },
                kohbor_events::ObjectiveAlert {
                    objective: ObjectiveId::PowerRune,
                    phase: ObjectivePhase::Spawn,
                    occurrence_seconds: 120,
                    due_seconds: 120,
                },
            ],
        };
        assert_eq!(tone_priority(&batch), ObjectiveId::PowerRune);
    }

    #[test]
    fn objective_log_fields_are_non_identifying() {
        let batch = ObjectiveAlertBatch {
            observed_clock_seconds: 90,
            alerts: vec![kohbor_events::ObjectiveAlert {
                objective: ObjectiveId::WaterRune,
                phase: ObjectivePhase::PreWarn { lead_s: 30 },
                occurrence_seconds: 120,
                due_seconds: 90,
            }],
        };
        let fields = log_fields(&batch, "deferred_played");
        let encoded = fields.to_string().to_ascii_lowercase();
        for private_marker in ["c:\\users", "username", "token", "secret", "\\appdata\\"] {
            assert!(!encoded.contains(private_marker));
        }
        assert_eq!(fields["kind"], "water_rune");
        assert_eq!(fields["lead_s"], "30");
        assert_eq!(fields["clock"], 90);
        assert_eq!(fields["tone"], "deferred_played");
    }
}
