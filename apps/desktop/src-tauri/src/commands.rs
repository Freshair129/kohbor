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
pub(crate) fn replay_library_list(
    webview: tauri::Webview,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    require_control(&webview)?;
    let mut list = serde_json::to_value(library.list().map_err(str::to_owned)?)
        .map_err(|_| "library_serialization_failed".to_owned())?;
    let history = state.match_history.entries();
    if let Some(rows) = list.get_mut("matches").and_then(Value::as_array_mut) {
        for row in rows {
            let Some(match_id) = row
                .get("match_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
            else {
                continue;
            };
            let match_id = match_id.as_str();
            // The install-keyed hash lets the renderer open a match it only knows by its
            // GSI history key (the post-game panel) without passing a raw match ID around.
            let match_key = analytics.store().match_key(match_id).ok();
            let local = match_key
                .as_deref()
                .and_then(|key| history.iter().find(|entry| entry.match_key == key));
            row["match_key"] = match_key.map_or(Value::Null, Value::String);
            if let Some(entry) = local {
                row["own_hero"] = entry.hero.clone().map_or(Value::Null, Value::String);
                row["local_history"] = json!({
                    "result": entry.result.map(|result| match result { crate::match_history::MatchResult::Win => "win", crate::match_history::MatchResult::Loss => "loss" }),
                    "kills": entry.kills,
                    "deaths": entry.deaths,
                    "assists": entry.assists,
                    "ended_at_unix_s": entry.ended_at
                });
            } else {
                row["own_hero"] = analytics
                    .store()
                    .own_hero(match_id)
                    .map_or(Value::Null, Value::String);
                row["local_history"] = Value::Null;
            }
        }
    }
    Ok(list)
}

#[tauri::command]
pub(crate) fn replay_library_pick_file(webview: tauri::Webview) -> Result<Option<String>, String> {
    require_control(&webview)?;
    #[cfg(windows)]
    {
        return replay_file_dialog::pick_dem_file();
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

#[cfg(windows)]
mod replay_file_dialog {
    use std::ffi::c_void;
    use windows::core::w;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
        SIGDN_FILESYSPATH,
    };

    struct ComApartment;

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    pub(super) fn pick_dem_file() -> Result<Option<String>, String> {
        std::thread::Builder::new()
            .name("replay-file-dialog".to_owned())
            .spawn(pick_dem_file_sta)
            .map_err(|_| "file_dialog_thread_unavailable".to_owned())?
            .join()
            .map_err(|_| "file_dialog_thread_failed".to_owned())?
    }

    fn pick_dem_file_sta() -> Result<Option<String>, String> {
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if initialized.is_err() {
            return Err("file_dialog_com_unavailable".to_owned());
        }
        let _apartment = ComApartment;
        let dialog: IFileOpenDialog =
            unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
                .map_err(|_| "file_dialog_unavailable".to_owned())?;
        let filters = [COMDLG_FILTERSPEC {
            pszName: w!("Dota replay files"),
            pszSpec: w!("*.dem"),
        }];
        unsafe {
            dialog
                .SetFileTypes(&filters)
                .map_err(|_| "file_dialog_filter_failed".to_owned())?;
            dialog
                .SetFileTypeIndex(1)
                .map_err(|_| "file_dialog_filter_failed".to_owned())?;
            dialog
                .SetDefaultExtension(w!("dem"))
                .map_err(|_| "file_dialog_filter_failed".to_owned())?;
            dialog
                .SetOptions(FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
                .map_err(|_| "file_dialog_options_failed".to_owned())?;
        }
        if let Err(error) = unsafe { dialog.Show(None) } {
            if error.code().0 as u32 == 0x8007_04c7 {
                return Ok(None);
            }
            return Err("file_dialog_failed".to_owned());
        }
        let item = unsafe { dialog.GetResult() }
            .map_err(|_| "file_dialog_result_unavailable".to_owned())?;
        let display_name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
            .map_err(|_| "file_dialog_path_unavailable".to_owned())?;
        let path = unsafe { display_name.to_string() }
            .map_err(|_| "file_dialog_path_unavailable".to_owned())?;
        unsafe { CoTaskMemFree(Some(display_name.0 as *const c_void)) };
        if !path.to_ascii_lowercase().ends_with(".dem") {
            return Err("invalid_dem_extension".to_owned());
        }
        Ok(Some(path))
    }
}

#[tauri::command]
pub(crate) fn replay_library_open(
    webview: tauri::Webview,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    match_id: String,
) -> Result<crate::replay_library::ReplayBundle, String> {
    require_control(&webview)?;
    library.open_bundle(&match_id).map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn replay_library_parse(
    webview: tauri::Webview,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    id_or_path: String,
) -> Result<crate::replay_library::ReplayJob, String> {
    require_control(&webview)?;
    library.parse(&id_or_path).map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn replay_library_remove(
    webview: tauri::Webview,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    match_id: String,
) -> Result<(), String> {
    require_control(&webview)?;
    library.remove(&match_id).map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn replay_library_clear(
    webview: tauri::Webview,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
) -> Result<(), String> {
    require_control(&webview)?;
    library.clear().map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn replay_library_pin(
    webview: tauri::Webview,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    match_id: String,
    pinned: bool,
) -> Result<(), String> {
    require_control(&webview)?;
    library.set_pinned(&match_id, pinned).map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn replay_library_open_folder(
    webview: tauri::Webview,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    match_id: String,
) -> Result<(), String> {
    require_control(&webview)?;
    let folder = library.bundle_folder(&match_id).map_err(str::to_owned)?;
    open_local_folder(&folder)
}

fn require_control(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() == "control" {
        Ok(())
    } else {
        Err("window_not_allowed".to_owned())
    }
}

fn open_local_folder(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|_| "folder_open_failed".to_owned())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("folder_open_unavailable".to_owned())
    }
}

#[tauri::command]
pub(crate) fn analytics_provider_list(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<Value, String> {
    require_control(&webview)?;
    let settings = analytics.preferences().map_err(str::to_owned)?;
    let mut providers = Vec::new();
    let mut untested_count = 0_usize;
    for spec in crate::analytics::providers::registry::PROVIDERS {
        if spec.id == "custom" {
            continue;
        }
        let verified = settings.verified_providers.contains_key(spec.id) || spec.verified;
        if !verified {
            untested_count += 1;
        }
        if !verified && !settings.show_untested {
            continue;
        }
        let endpoint = analytics.store().provider_endpoint(spec.id, &settings).ok();
        let loopback = endpoint
            .as_ref()
            .is_some_and(|endpoint| endpoint.is_loopback());
        providers.push(serde_json::json!({
            "id":spec.id,"label":spec.label,"verified":verified,"selected":settings.provider == spec.id,
            "endpoint":endpoint.map(|endpoint| endpoint.origin()),"loopback":loopback,"requires_key":!loopback,
            "endpoint_kind":format!("{:?}",spec.kind).to_ascii_lowercase(),
            "configured_endpoint":settings.provider_endpoints.get(spec.id).cloned().unwrap_or_else(|| spec.default_endpoint.to_owned()),
            "models":crate::analytics::providers::registry::known_models(spec.id),"model_list_fetch":spec.id != "anthropic",
        }));
    }
    for (provider, raw_endpoint) in &settings.provider_endpoints {
        if !provider.starts_with("custom-") {
            continue;
        }
        let verified = settings.verified_providers.contains_key(provider);
        if !verified {
            untested_count += 1;
        }
        if !verified && !settings.show_untested {
            continue;
        }
        let endpoint = analytics
            .store()
            .provider_endpoint(provider, &settings)
            .ok();
        let loopback = endpoint
            .as_ref()
            .is_some_and(|endpoint| endpoint.is_loopback());
        providers.push(serde_json::json!({
            "id":provider,"label":"OpenAI-compatible custom endpoint","verified":verified,"selected":settings.provider == *provider,
            "endpoint":endpoint.map(|endpoint| endpoint.origin()),"loopback":loopback,"requires_key":!loopback,
            "models":[],"model_list_fetch":true,"endpoint_kind":"user","configured_endpoint":raw_endpoint,
        }));
    }
    Ok(
        serde_json::json!({"providers":providers,"untested_count":untested_count,"show_untested":settings.show_untested}),
    )
}

#[tauri::command]
pub(crate) fn analytics_provider_endpoint_set(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    provider_id: String,
    endpoint: String,
) -> Result<crate::analytics::AnalyticsPreferences, String> {
    require_control(&webview)?;
    analytics
        .set_provider_endpoint(&provider_id, &endpoint)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_custom_endpoint_register(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    endpoint: String,
) -> Result<Value, String> {
    require_control(&webview)?;
    let (provider_id, settings) = analytics
        .register_custom_endpoint(&endpoint)
        .map_err(str::to_owned)?;
    Ok(serde_json::json!({"provider_id":provider_id,"settings":settings}))
}

#[tauri::command]
pub(crate) fn analytics_provider_verified_set(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    provider_id: String,
    evidence_sha256: String,
) -> Result<crate::analytics::AnalyticsPreferences, String> {
    require_control(&webview)?;
    if !cfg!(any(debug_assertions, feature = "lab")) {
        return Err("verification_action_requires_developer_build".into());
    }
    analytics
        .set_provider_verified(&provider_id, &evidence_sha256)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_provider_models_fetch(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    state: tauri::State<'_, AppState>,
    provider_id: String,
) -> Result<Vec<String>, String> {
    require_control(&webview)?;
    if (state.match_active)() {
        return Err("match_active".into());
    }
    analytics.fetch_models(&provider_id).map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_key_status(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics.key_status().map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_key_set(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    key: String,
) -> Result<(), String> {
    require_control(&webview)?;
    analytics.set_key(&key).map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_key_delete(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<(), String> {
    require_control(&webview)?;
    analytics.delete_key().map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_provider_key_status(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    provider_id: String,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics
        .store()
        .key_status_for(&provider_id)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_provider_key_set(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    provider_id: String,
    key: String,
) -> Result<(), String> {
    require_control(&webview)?;
    analytics
        .set_provider_key(&provider_id, &key)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_provider_key_delete(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    provider_id: String,
) -> Result<(), String> {
    require_control(&webview)?;
    analytics
        .delete_provider_key(&provider_id)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_settings_get(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<crate::analytics::AnalyticsPreferences, String> {
    require_control(&webview)?;
    analytics.preferences().map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_settings_patch(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    patch: Value,
) -> Result<crate::analytics::AnalyticsPreferences, String> {
    require_control(&webview)?;
    let old = analytics.preferences().map_err(str::to_owned)?;
    let next = analytics.patch_preferences(&patch).map_err(str::to_owned)?;
    if old.keep_newest != next.keep_newest || old.max_bytes != next.max_bytes {
        if let Err(error) = library.set_limits(crate::replay_library::ReplayLibraryLimits {
            max_matches: next.keep_newest,
            max_bytes: next.max_bytes,
        }) {
            let _ = analytics.store().write_preferences(&old);
            return Err(error.to_owned());
        }
    }
    if old.auto_analyse != next.auto_analyse {
        library.set_auto_analyse(next.auto_analyse);
    }
    Ok(next)
}

#[tauri::command]
pub(crate) fn analytics_consent_set(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    consent: bool,
) -> Result<crate::analytics::AnalyticsPreferences, String> {
    require_control(&webview)?;
    analytics.set_consent(consent).map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_ledger_list(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<Vec<crate::analytics::LedgerRow>, String> {
    require_control(&webview)?;
    analytics.store().ledger().map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_notes_list(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    state: tauri::State<'_, AppState>,
    match_id: Option<String>,
) -> Result<Vec<crate::analytics::NoteRow>, String> {
    require_control(&webview)?;
    match match_id {
        Some(match_id) => analytics
            .list_notes(
                &library,
                &match_id,
                &state.match_history.entries(),
                &state.opendota.entries(),
            )
            .map_err(str::to_owned),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub(crate) fn analytics_note_read(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    state: tauri::State<'_, AppState>,
    match_id: String,
    note_id: String,
) -> Result<String, String> {
    require_control(&webview)?;
    let _ = analytics
        .list_notes(
            &library,
            &match_id,
            &state.match_history.entries(),
            &state.opendota.entries(),
        )
        .map_err(str::to_owned)?;
    analytics
        .store()
        .read_note(&match_id, &note_id)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_note_delete(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    match_id: String,
    note_id: String,
) -> Result<(), String> {
    require_control(&webview)?;
    analytics
        .store()
        .delete_note(&match_id, &note_id)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_note_open_folder(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    match_id: String,
) -> Result<(), String> {
    require_control(&webview)?;
    let folder = analytics
        .store()
        .open_note_folder(&match_id)
        .map_err(str::to_owned)?;
    open_local_folder(&folder)
}

#[tauri::command]
pub(crate) fn analytics_own_hero_set(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    match_id: String,
    hero: String,
) -> Result<String, String> {
    require_control(&webview)?;
    let bundle = library.open_bundle(&match_id).map_err(str::to_owned)?;
    let normalized = if hero.starts_with("npc_dota_hero_") {
        hero
    } else {
        format!("npc_dota_hero_{hero}")
    };
    let present = bundle
        .match_data
        .get("heroes")
        .and_then(Value::as_array)
        .is_some_and(|heroes| {
            heroes
                .iter()
                .any(|item| item.get("id").and_then(Value::as_str) == Some(normalized.as_str()))
        });
    if !present {
        return Err("invalid_hero".to_owned());
    }
    analytics
        .store()
        .set_own_hero(&match_id, &normalized)
        .map_err(str::to_owned)?;
    Ok(normalized)
}

#[tauri::command]
pub(crate) fn analytics_chat_submit(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    state: tauri::State<'_, AppState>,
    match_id: String,
    question: String,
    selected_hero: Option<String>,
    scrubber_time_s: Option<f64>,
) -> Result<String, String> {
    require_control(&webview)?;
    if (state.match_active)() {
        return Err("match_active".into());
    }
    let prepared = analytics
        .prepare_chat(
            &library,
            &state.match_history.entries(),
            &state.opendota.entries(),
            crate::analytics::ChatOptions {
                match_id,
                question,
                selected_hero,
                scrubber_time_s,
            },
        )
        .map_err(str::to_owned)?;
    let request_id = prepared.request_id().to_owned();
    analytics.spawn_chat(prepared, app);
    Ok(request_id)
}

#[tauri::command]
pub(crate) fn analytics_chat_transcript(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    match_id: String,
) -> Result<Vec<Value>, String> {
    require_control(&webview)?;
    analytics
        .store()
        .chat_transcript(&match_id)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_chat_start_from_summary(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    match_id: String,
    summary: String,
) -> Result<String, String> {
    require_control(&webview)?;
    analytics
        .start_new_chat_from_summary(&match_id, &summary)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_knowledge_get(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    state: tauri::State<'_, AppState>,
    match_id: String,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics
        .knowledge_snapshot(
            &library,
            &match_id,
            &state.match_history.entries(),
            &state.opendota.entries(),
        )
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_lane_calibration_get(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics.lane_calibration_report().map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_lane_calibration_run(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics
        .run_lane_calibration(
            &library,
            &state.match_history.entries(),
            &state.opendota.entries(),
        )
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_lane_review_start(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics
        .start_lane_coach_review(
            &library,
            &state.match_history.entries(),
            &state.opendota.entries(),
        )
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_lane_review_current(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics.lane_coach_review_current().map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_lane_review_submit_blind(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    tier: i32,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics
        .lane_coach_review_submit_blind(tier)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_lane_review_submit_half_blind(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    verdict: String,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics
        .lane_coach_review_submit_half_blind(&verdict)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_lane_review_next(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics.lane_coach_review_next().map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_lane_review_export(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics.lane_coach_review_export().map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_saved_analysis_read(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    match_id: String,
    id: String,
) -> Result<Value, String> {
    require_control(&webview)?;
    analytics
        .read_saved_analysis(&match_id, &id)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_report_generate(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    library: tauri::State<'_, crate::replay_library::ReplayLibrary>,
    state: tauri::State<'_, AppState>,
    match_id: String,
    selected_hero: Option<String>,
    scrubber_time_s: Option<f64>,
    regenerate: Option<bool>,
) -> Result<Value, String> {
    require_control(&webview)?;
    if (state.match_active)() {
        return Err("match_active".into());
    }
    let prepared = analytics
        .prepare_report(
            &library,
            &state.match_history.entries(),
            &state.opendota.entries(),
            match_id,
            selected_hero,
            scrubber_time_s,
            regenerate.unwrap_or(false),
        )
        .map_err(str::to_owned)?;
    match prepared {
        crate::analytics::ReportPreparation::Cached(report) => {
            Ok(json!({"cached":true,"report":report,"request_id":null}))
        }
        crate::analytics::ReportPreparation::Request(prepared) => {
            let request_id = prepared.request_id().to_owned();
            analytics.spawn_chat(prepared, app);
            Ok(json!({
                "cached":false,
                "request_id":request_id,
                "report":null,
                "cost_estimate_usd":null,
                "cost_reason":"provider token pricing is not verified in this build"
            }))
        }
    }
}

#[tauri::command]
pub(crate) fn analytics_note_save_text(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    match_id: String,
    title: String,
    markdown: String,
) -> Result<String, String> {
    require_control(&webview)?;
    analytics
        .store()
        .write_note(&match_id, &title, &markdown)
        .map_err(str::to_owned)
}

#[tauri::command]
pub(crate) fn analytics_chat_cancel(
    webview: tauri::Webview,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
    request_id: String,
) {
    if require_control(&webview).is_err() {
        return;
    }
    analytics.cancel(&request_id);
}

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
pub(crate) async fn update_download(state: tauri::State<'_, AppState>) -> Result<Value, String> {
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

#[tauri::command(async)]
pub(crate) fn objective_read_model(state: tauri::State<'_, AppState>) -> Value {
    state.runtime.objective_read_model()
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
    state
        .match_history
        .view_with_opendota(&state.opendota.entries(), month_start)
}

/// Deletes both local and OpenDota match history (user action on the Overview).
#[tauri::command(async)]
pub(crate) fn match_history_clear(
    state: tauri::State<'_, AppState>,
    analytics: tauri::State<'_, crate::analytics::AnalyticsService>,
) -> Result<Value, String> {
    let local = state.match_history.clear();
    let remote = state.opendota.clear_cache();
    let lanes = analytics.clear_lane_history();
    if let Err(error) = local {
        return Err(error);
    }
    remote?;
    lanes.map_err(str::to_owned)?;
    crate::log_info!(
        "history",
        "match_history_cleared",
        "local, OpenDota and lane histories cleared"
    );
    Ok(json!({"cleared": true}))
}

#[tauri::command]
pub(crate) fn opendota_prefs_get(state: tauri::State<'_, AppState>) -> Value {
    state.opendota.prefs()
}

/// The renderer supplies consent only. A verified GSI account ID is stored when one becomes available.
#[tauri::command]
pub(crate) fn opendota_prefs_set(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<Value, String> {
    let account_id = if enabled {
        state.runtime.current_own_account_id()
    } else {
        None
    };
    state.opendota.set_enabled(enabled, account_id)
}

#[tauri::command]
pub(crate) fn opendota_view_get(state: tauri::State<'_, AppState>) -> Value {
    state.opendota.view()
}

/// The Overview polls while waiting for an account; the first verified account is bound and fetched once.
#[tauri::command(async)]
pub(crate) fn opendota_refresh(state: tauri::State<'_, AppState>, retry: bool) -> Value {
    if state.opendota.prefs()["enabled"] == true {
        if let Some(account_id) = state.runtime.current_own_account_id() {
            if state
                .opendota
                .update_account_if_enabled(account_id)
                .is_err()
            {
                return json!({
                    "state": "unavailable", "enabled": true, "cached_entries": 0,
                    "fetched_at": null, "last_attempt_at": null, "retry_available": false
                });
            }
        }
    }
    state.opendota.refresh(retry)
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
