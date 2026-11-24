use super::context::{
    build_context_blocks, report_stale_reasons, ContextBlocks, HISTORY_ESTIMATED_BYTES_PER_TOKEN,
    HISTORY_TOKEN_BUDGET, PROMPT_VERSION,
};
use super::knowledge::{self, KnowledgeInput, KNOWLEDGE_VERSION};
use super::protocol::{
    finalize_tool_blocks, retry_delay_ms, ApiError, Model, OutgoingRequest, ParsedMessage,
    RequestInput, Usage,
};
use super::providers::endpoint::Endpoint;
use super::providers::openai_compat::RequestInput as OpenAiRequestInput;
use super::providers::{
    AnthropicAdapter, NeutralEvent, OpenAiCompatAdapter, ToolsMode, WireAdapter,
};
use super::store::{AnalyticsPreferences, AnalyticsStore, LedgerRow, StoredAiReport};
use super::tools::{self, ToolContext};
use super::transport::{Cancellation, Transport, TransportError};
use crate::match_history::MatchEntry;
use crate::replay_library::{ReplayBundle, ReplayLibrary};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

const MAX_TOOL_ROUNDS: u8 = 8;

fn tool_round_limit_reached(rounds: u8) -> bool {
    rounds > MAX_TOOL_ROUNDS
}

#[derive(Clone)]
pub(crate) struct AnalyticsService {
    store: AnalyticsStore,
    transport: Arc<dyn Transport>,
    active: Arc<Mutex<HashMap<String, Cancellation>>>,
}

pub(crate) struct PreparedChat {
    request_id: String,
    match_id: String,
    provider_id: String,
    key: String,
    endpoint: Endpoint,
    model: String,
    tools_mode: ToolsMode,
    tool_defs: Vec<Value>,
    context: ContextBlocks,
    digest: Value,
    bundle: Option<ReplayBundle>,
    bundle_hash: String,
    own_hero: Option<String>,
    question: String,
    selected_hero: Option<String>,
    scrubber_time_s: Option<f64>,
    report: bool,
    history: Vec<Value>,
    cancellation: Cancellation,
}

pub(crate) enum ReportPreparation {
    Cached(StoredAiReport),
    Request(PreparedChat),
}

#[derive(Clone, Debug)]
pub(crate) struct ChatOptions {
    pub(crate) match_id: String,
    pub(crate) question: String,
    pub(crate) selected_hero: Option<String>,
    pub(crate) scrubber_time_s: Option<f64>,
}

impl AnalyticsService {
    pub(crate) fn open(
        data_dir: &Path,
        install_key: Option<[u8; 32]>,
        transport: Arc<dyn Transport>,
    ) -> Self {
        Self {
            store: AnalyticsStore::open(data_dir, install_key),
            transport,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn store(&self) -> &AnalyticsStore {
        &self.store
    }
    pub(crate) fn preferences(&self) -> Result<AnalyticsPreferences, &'static str> {
        self.store.preferences()
    }
    pub(crate) fn key_status(&self) -> Result<Value, &'static str> {
        let settings = self.store.preferences()?;
        let endpoint = self
            .store
            .provider_endpoint(&settings.provider, &settings)?;
        let mut status = self.store.key_status_for(&settings.provider)?;
        if let Some(object) = status.as_object_mut() {
            object.insert("requires_key".into(), json!(!endpoint.is_loopback()));
        }
        Ok(status)
    }
    pub(crate) fn set_key(&self, key: &str) -> Result<(), &'static str> {
        self.cancel_all();
        self.store.set_key(key)?;
        let mut settings = self.store.preferences()?;
        settings.ai_on = false;
        self.store.write_preferences(&settings)
    }
    pub(crate) fn delete_key(&self) -> Result<(), &'static str> {
        self.cancel_all();
        self.store.delete_key()?;
        let mut settings = self.store.preferences()?;
        settings.ai_on = false;
        self.store.write_preferences(&settings)
    }
    pub(crate) fn set_consent(&self, consent: bool) -> Result<AnalyticsPreferences, &'static str> {
        let current = self.store.preferences()?;
        let settings = self
            .store
            .set_provider_consent(&current.provider, consent)?;
        if !consent {
            self.cancel_all();
        }
        Ok(settings)
    }

    pub(crate) fn set_provider_key(&self, provider: &str, key: &str) -> Result<(), &'static str> {
        self.cancel_all();
        self.store.set_provider_key(provider, key)?;
        let mut settings = self.store.preferences()?;
        settings.ai_on = false;
        self.store.write_preferences(&settings)
    }

    pub(crate) fn delete_provider_key(&self, provider: &str) -> Result<(), &'static str> {
        self.cancel_all();
        self.store.delete_provider_key(provider)?;
        let mut settings = self.store.preferences()?;
        settings.ai_on = false;
        self.store.write_preferences(&settings)
    }

    pub(crate) fn fetch_models(&self, provider: &str) -> Result<Vec<String>, &'static str> {
        if provider == "anthropic" {
            return Err("model_list_closed");
        }
        let settings = self.store.preferences()?;
        let endpoint = self.store.provider_endpoint(provider, &settings)?;
        let consent = settings
            .provider_consents
            .get(provider)
            .is_some_and(|record| {
                record.origin == endpoint.origin()
                    && record.text_version == super::store::CONSENT_TEXT_VERSION
                    && record.granted_at_unix_ms > 0
            });
        if !consent {
            return Err("consent_required");
        }
        let key = self.store.read_provider_key(provider)?.unwrap_or_default();
        if !endpoint.is_loopback() && key.is_empty() {
            return Err("key_required");
        }
        let spec = super::providers::registry::spec(provider);
        let (header_name, prefix, models_path) = if let Some(spec) = spec {
            (spec.auth_header, spec.auth_prefix, spec.models_path)
        } else if provider.starts_with("custom-") {
            (Some("Authorization"), "Bearer ", "/models")
        } else {
            return Err("provider_not_allowed");
        };
        let mut headers = vec![("accept".to_owned(), "application/json".to_owned())];
        if !key.is_empty() {
            if let Some(name) = header_name {
                let key = String::from_utf8(key).map_err(|_| "key_invalid")?;
                headers.push((name.to_owned(), format!("{prefix}{key}")));
            }
        }
        let request_endpoint = endpoint.request_endpoint(models_path);
        let response = self
            .transport
            .get(&request_endpoint, &headers, &Cancellation::default())
            .map_err(|error| match error {
                TransportError::Cancelled => "cancelled",
                TransportError::TooLarge => "model_list_too_large",
                TransportError::Tls | TransportError::Network | TransportError::Timeout => {
                    if endpoint.is_loopback() {
                        "local_program_unavailable"
                    } else {
                        "model_list_unavailable"
                    }
                }
                TransportError::NotSent => "model_list_unavailable",
            })?;
        if (300..400).contains(&response.status) {
            return Err("redirect_refused");
        }
        if !(200..300).contains(&response.status) {
            return Err("model_list_unavailable");
        }
        super::providers::registry::parse_model_list(&response.body)
    }

    pub(crate) fn set_provider_endpoint(
        &self,
        provider: &str,
        endpoint: &str,
    ) -> Result<AnalyticsPreferences, &'static str> {
        self.cancel_all();
        self.store.set_provider_endpoint(provider, endpoint)
    }

    pub(crate) fn register_custom_endpoint(
        &self,
        endpoint: &str,
    ) -> Result<(String, AnalyticsPreferences), &'static str> {
        self.cancel_all();
        self.store.register_custom_endpoint(endpoint)
    }

    pub(crate) fn set_provider_verified(
        &self,
        provider: &str,
        evidence_sha256: &str,
    ) -> Result<AnalyticsPreferences, &'static str> {
        self.store.set_provider_verified(provider, evidence_sha256)
    }
    pub(crate) fn patch_preferences(
        &self,
        patch: &Value,
    ) -> Result<AnalyticsPreferences, &'static str> {
        let object = patch.as_object().ok_or("settings_patch_invalid")?;
        let mut settings = self.store.preferences()?;
        for (key, value) in object {
            match key.as_str() {
                "ai_on" => {
                    let enabled = value.as_bool().ok_or("settings_patch_invalid")?;
                    if enabled {
                        let endpoint = self
                            .store
                            .provider_endpoint(&settings.provider, &settings)?;
                        if !settings.consent
                            || (!endpoint.is_loopback()
                                && self.store.read_provider_key(&settings.provider)?.is_none())
                            || settings.model.is_empty()
                        {
                            return Err("ai_gate_closed");
                        }
                    }
                    settings.ai_on = enabled;
                    if !enabled {
                        self.cancel_all();
                    }
                }
                "model" => {
                    let model = value.as_str().ok_or("settings_patch_invalid")?;
                    if settings.provider == "anthropic" {
                        Model::parse(model)?;
                    } else if !model.is_empty() {
                        super::providers::registry::validate_model_id(model)?;
                    }
                    settings.model = model.to_owned();
                }
                "provider" => {
                    let provider = value.as_str().ok_or("settings_patch_invalid")?;
                    self.store.provider_endpoint(provider, &settings)?;
                    if settings.provider != provider {
                        self.cancel_all();
                        settings.provider = provider.to_owned();
                        settings.model = if provider == "anthropic" {
                            "claude-opus-5-5".into()
                        } else {
                            String::new()
                        };
                        settings.ai_on = false;
                        settings.consent = false;
                    }
                }
                "show_untested" => {
                    settings.show_untested = value.as_bool().ok_or("settings_patch_invalid")?;
                }
                "tools_mode" => {
                    let mode = value.as_str().ok_or("settings_patch_invalid")?;
                    if !matches!(mode, "unknown" | "supported" | "unsupported") {
                        return Err("settings_patch_invalid");
                    }
                    settings
                        .tools_modes
                        .insert(settings.provider.clone(), mode.to_owned());
                }
                "keep_newest" => {
                    settings.keep_newest =
                        usize::try_from(value.as_u64().ok_or("settings_patch_invalid")?)
                            .map_err(|_| "settings_patch_invalid")?
                }
                "max_bytes" => {
                    settings.max_bytes = value.as_u64().ok_or("settings_patch_invalid")?
                }
                "auto_analyse" => {
                    settings.auto_analyse = value.as_bool().ok_or("settings_patch_invalid")?
                }
                _ => return Err("settings_patch_invalid"),
            }
        }
        self.store.write_preferences(&settings)?;
        self.store.preferences()
    }

    pub(crate) fn prepare_chat(
        &self,
        library: &ReplayLibrary,
        history_entries: &[MatchEntry],
        options: ChatOptions,
    ) -> Result<PreparedChat, &'static str> {
        self.prepare_request(library, history_entries, options, false)
    }

    pub(crate) fn start_new_chat_from_summary(
        &self,
        match_id: &str,
        summary: &str,
    ) -> Result<String, &'static str> {
        if !lock(&self.active).is_empty() {
            return Err("chat_active");
        }
        self.store.start_new_chat_from_summary(match_id, summary)
    }

    pub(crate) fn prepare_report(
        &self,
        library: &ReplayLibrary,
        history_entries: &[MatchEntry],
        match_id: String,
        selected_hero: Option<String>,
        scrubber_time_s: Option<f64>,
        regenerate: bool,
    ) -> Result<ReportPreparation, &'static str> {
        let prepared = self.prepare_request(library, history_entries, ChatOptions {
            match_id,
            question: "Write the saved match analysis described by RULES v2. Analyze lanes, the mid and late phases, key teamfights, and what the selected hero could change. Cite each claim.".into(),
            selected_hero,
            scrubber_time_s,
        }, true)?;
        let cache_key = report_cache_key(
            &prepared.bundle_hash,
            &prepared.model,
            PROMPT_VERSION,
            KNOWLEDGE_VERSION,
        );
        if !regenerate {
            if let Ok(reports) = self.store.list_ai_reports(&prepared.match_id) {
                for summary in reports {
                    if summary.meta.cache_key == cache_key
                        && report_stale_reasons(
                            &summary.meta,
                            &prepared.bundle_hash,
                            KNOWLEDGE_VERSION,
                            prepared.own_hero.as_deref(),
                        )
                        .is_empty()
                    {
                        if let Ok(report) =
                            self.store.read_ai_report(&prepared.match_id, &summary.id)
                        {
                            lock(&self.active).remove(&prepared.request_id);
                            return Ok(ReportPreparation::Cached(report));
                        }
                    }
                }
            }
        }
        Ok(ReportPreparation::Request(prepared))
    }

    fn prepare_request(
        &self,
        library: &ReplayLibrary,
        history_entries: &[MatchEntry],
        options: ChatOptions,
        report: bool,
    ) -> Result<PreparedChat, &'static str> {
        let settings = self.store.preferences()?;
        if !settings.ai_on {
            return Err("ai_off");
        }
        if !settings.consent {
            return Err("consent_required");
        }
        let endpoint = self
            .store
            .provider_endpoint(&settings.provider, &settings)?;
        if settings.provider == "anthropic" {
            Model::parse(&settings.model)?;
        } else {
            super::providers::registry::validate_model_id(&settings.model)?;
        }
        let key = match self.store.read_provider_key(&settings.provider)? {
            Some(bytes) => String::from_utf8(bytes).map_err(|_| "key_invalid")?,
            None if endpoint.is_loopback() => String::new(),
            None => return Err("key_required"),
        };
        if options.question.trim().is_empty() || options.question.len() > 8 * 1024 {
            return Err("question_invalid");
        }
        let gsi = find_gsi_match(&self.store, &options.match_id, history_entries);
        let own_hero = gsi
            .and_then(|entry| entry.hero.clone())
            .or_else(|| self.store.own_hero(&options.match_id));
        let bundle = library.open_bundle(&options.match_id).ok();
        let (digest, bundle_hash) = if let Some(bundle) = bundle.as_ref() {
            let digest = knowledge::build(&KnowledgeInput {
                manifest: &bundle.manifest,
                match_data: &bundle.match_data,
                world: &bundle.world,
            })?;
            let bundle_hash = knowledge::bundle_hash(&bundle.manifest)?;
            let bytes = knowledge::canonical_bytes(&digest)?;
            self.store.write_knowledge(
                &options.match_id,
                &bundle_hash,
                KNOWLEDGE_VERSION,
                &bytes,
            )?;
            (digest, bundle_hash)
        } else {
            if report {
                return Err("replay_removed");
            }
            let (meta, digest) = self
                .store
                .read_latest_knowledge(&options.match_id)?
                .ok_or("bundle_unavailable")?;
            (digest, meta.bundle_hash)
        };
        let viewer = knowledge::session_viewer(&digest, own_hero.as_deref(), gsi);
        let notes = self
            .store
            .list_notes(&options.match_id, &summary_markdown(&digest))?;
        let current_own = viewer
            .get("own_hero")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let latest_report = self
            .store
            .list_ai_reports(&options.match_id)
            .ok()
            .and_then(|reports| reports.first().cloned())
            .and_then(|summary| {
                self.store
                    .read_ai_report(&options.match_id, &summary.id)
                    .ok()
                    .map(|mut report| {
                        report.meta.stale = !report_stale_reasons(
                            &report.meta,
                            &bundle_hash,
                            KNOWLEDGE_VERSION,
                            current_own.as_deref(),
                        )
                        .is_empty();
                        report
                    })
            });
        let tools_mode = match settings
            .tools_modes
            .get(&settings.provider)
            .map(String::as_str)
        {
            Some("supported") => ToolsMode::Supported,
            Some("unsupported") => ToolsMode::Unsupported,
            _ if settings.provider == "anthropic" => ToolsMode::Supported,
            _ => ToolsMode::Unknown,
        };
        let tools_available = tools_mode == ToolsMode::Supported && settings.provider != "qwen";
        let mut tool_defs = tools::definitions(tools_available);
        if report {
            tool_defs.retain(|tool| tool.get("name").and_then(Value::as_str) != Some("write_note"));
        }
        let context = build_context_blocks(
            &digest,
            Some(&viewer),
            &notes,
            latest_report.as_ref(),
            tools_available,
        )?;
        let selected_hero = options
            .selected_hero
            .as_deref()
            .and_then(|selected| {
                digest
                    .get("heroes")
                    .and_then(Value::as_array)
                    .and_then(|heroes| {
                        heroes
                            .iter()
                            .find(|hero| hero.get("id").and_then(Value::as_str) == Some(selected))
                    })
            })
            .and_then(|hero| hero.get("hero").and_then(Value::as_str))
            .map(str::to_owned)
            .or_else(|| {
                current_own
                    .as_deref()
                    .and_then(|id| {
                        digest
                            .get("heroes")
                            .and_then(Value::as_array)
                            .and_then(|heroes| {
                                heroes
                                    .iter()
                                    .find(|hero| hero.get("id").and_then(Value::as_str) == Some(id))
                            })
                    })
                    .and_then(|hero| hero.get("hero").and_then(Value::as_str))
                    .map(str::to_owned)
            });
        let history = if report {
            Vec::new()
        } else {
            self.store.chat(&options.match_id)?
        };
        if estimate_history_tokens(&history) > HISTORY_TOKEN_BUDGET {
            return Err("history_budget_exceeded");
        }
        let bytes = crate::secrets::random::<16>().ok_or("request_id_unavailable")?;
        let request_id = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let cancellation = Cancellation::default();
        lock(&self.active).insert(request_id.clone(), cancellation.clone());
        Ok(PreparedChat {
            request_id,
            match_id: options.match_id,
            provider_id: settings.provider.clone(),
            key,
            endpoint,
            model: settings.model.clone(),
            tools_mode,
            tool_defs,
            context,
            digest,
            bundle,
            bundle_hash,
            own_hero: current_own,
            question: options.question,
            selected_hero,
            scrubber_time_s: options.scrubber_time_s,
            report,
            history,
            cancellation,
        })
    }

    pub(crate) fn spawn_chat(&self, prepared: PreparedChat, app: tauri::AppHandle) {
        let service = self.clone();
        let request_id = prepared.request_id.clone();
        let thread_request_id = request_id.clone();
        let thread_app = app.clone();
        let spawned = thread::Builder::new()
            .name("analytics-claude-chat".to_owned())
            .spawn(move || {
                service.run_chat(prepared, &thread_app);
                lock(&service.active).remove(&thread_request_id);
            });
        if spawned.is_err() {
            lock(&self.active).remove(&request_id);
            let _ = app.emit(
                "analytics://complete",
                json!({"request_id":request_id,"error":ApiError::NetworkTls.renderer_text()}),
            );
        }
    }

    fn build_request(
        &self,
        prepared: &PreparedChat,
        history: &[Value],
        question: Option<&str>,
    ) -> Result<OutgoingRequest, &'static str> {
        let history = history_for_request(history, &prepared.provider_id, &prepared.model)?;
        let volatile = question
            .map(|_| format_volatile(prepared.scrubber_time_s, prepared.selected_hero.as_deref()));
        if prepared.provider_id == "anthropic" {
            AnthropicAdapter.build(&RequestInput {
                key: &prepared.key,
                model: Model::parse(&prepared.model)?,
                rules: &prepared.context.rules,
                digest: &prepared.context.digest,
                session: &prepared.context.session,
                question,
                volatile: volatile.as_deref().unwrap_or(""),
                history: &history,
                tools: &prepared.tool_defs,
            })
        } else {
            OpenAiCompatAdapter.build(&OpenAiRequestInput {
                provider: &prepared.provider_id,
                model: &prepared.model,
                key: (!prepared.key.is_empty()).then_some(prepared.key.as_str()),
                rules: &prepared.context.rules,
                digest: &prepared.context.digest,
                session: &prepared.context.session,
                question,
                volatile: volatile.as_deref().unwrap_or(""),
                history: &history,
                tools: &prepared.tool_defs,
                tools_mode: prepared.tools_mode,
            })
        }
    }

    pub(crate) fn cancel(&self, request_id: &str) {
        if let Some(cancellation) = lock(&self.active).get(request_id) {
            cancellation.cancel();
        }
        self.transport.cancel_current();
    }

    pub(crate) fn cancel_all(&self) {
        for cancellation in lock(&self.active).values() {
            cancellation.cancel();
        }
        self.transport.cancel_current();
    }

    fn remember_tools_mode(&self, provider: &str, mode: &str) -> Result<(), &'static str> {
        if !matches!(mode, "unknown" | "supported" | "unsupported") {
            return Err("settings_patch_invalid");
        }
        let mut settings = self.store.preferences()?;
        settings
            .tools_modes
            .insert(provider.to_owned(), mode.to_owned());
        self.store.write_preferences(&settings)
    }

    pub(crate) fn list_notes(
        &self,
        library: &ReplayLibrary,
        match_id: &str,
        history: &[MatchEntry],
    ) -> Result<Vec<super::store::NoteRow>, &'static str> {
        let summary = self.system_summary(library, match_id, history);
        self.store.list_notes(match_id, &summary)
    }

    pub(crate) fn knowledge_snapshot(
        &self,
        library: &ReplayLibrary,
        match_id: &str,
        history: &[MatchEntry],
    ) -> Result<Value, &'static str> {
        let gsi = find_gsi_match(&self.store, match_id, history);
        let own_hero = gsi
            .and_then(|entry| entry.hero.clone())
            .or_else(|| self.store.own_hero(match_id));
        let (digest, bundle_hash, bundle_available) = match library.open_bundle(match_id) {
            Ok(bundle) => {
                let digest = knowledge::build(&KnowledgeInput {
                    manifest: &bundle.manifest,
                    match_data: &bundle.match_data,
                    world: &bundle.world,
                })?;
                let bundle_hash = knowledge::bundle_hash(&bundle.manifest)?;
                let bytes = knowledge::canonical_bytes(&digest)?;
                self.store
                    .write_knowledge(match_id, &bundle_hash, KNOWLEDGE_VERSION, &bytes)?;
                (digest, bundle_hash, true)
            }
            Err(_) => {
                let (meta, digest) = self
                    .store
                    .read_latest_knowledge(match_id)?
                    .ok_or("bundle_unavailable")?;
                (digest, meta.bundle_hash, false)
            }
        };
        let note_index = self
            .store
            .list_notes(match_id, &summary_markdown(&digest))?;
        let history_tokens = self
            .store
            .chat(match_id)
            .map(|history| estimate_history_tokens(&history))
            .unwrap_or(0);
        let viewer = knowledge::session_viewer(&digest, own_hero.as_deref(), gsi);
        let own = viewer.get("own_hero").and_then(Value::as_str);
        let reports = self
            .store
            .list_ai_reports(match_id)?
            .into_iter()
            .map(|summary| {
                let reasons =
                    report_stale_reasons(&summary.meta, &bundle_hash, KNOWLEDGE_VERSION, own);
                let mut meta = serde_json::to_value(&summary.meta).unwrap_or(Value::Null);
                meta["stale"] = json!(!reasons.is_empty());
                json!({"id":summary.id,"meta":meta,"stale_reasons":reasons})
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "digest":digest,
            "bundle_hash":bundle_hash,
            "bundle_available":bundle_available,
            "own_hero_id":own,
            "viewer":viewer,
            "notes":note_index,
            "reports":reports,
            "chat_history_estimated_tokens":history_tokens,
            "chat_history_token_budget":HISTORY_TOKEN_BUDGET,
            "chat_history_over_budget":history_tokens>HISTORY_TOKEN_BUDGET,
            "provisional_threshold_label":"เกณฑ์ชั่วคราว",
            "report_cost_estimate_usd":null,
            "report_cost_reason":"provider token pricing is not verified in this build"
        }))
    }

    pub(crate) fn read_saved_analysis(
        &self,
        match_id: &str,
        id: &str,
    ) -> Result<Value, &'static str> {
        if let Ok(report) = self.store.read_ai_report(match_id, id) {
            return Ok(
                json!({"id":report.id,"kind":"ai_report","markdown":report.markdown,"meta":report.meta}),
            );
        }
        let markdown = self.store.read_note(match_id, id)?;
        Ok(json!({"id":id,"kind":"note","markdown":markdown}))
    }

    pub(crate) fn system_summary(
        &self,
        library: &ReplayLibrary,
        match_id: &str,
        _history: &[MatchEntry],
    ) -> String {
        if let Ok(bundle) = library.open_bundle(match_id) {
            if let Ok(digest) = knowledge::build(&KnowledgeInput {
                manifest: &bundle.manifest,
                match_data: &bundle.match_data,
                world: &bundle.world,
            }) {
                if let (Ok(hash), Ok(bytes)) = (
                    knowledge::bundle_hash(&bundle.manifest),
                    knowledge::canonical_bytes(&digest),
                ) {
                    let _ = self
                        .store
                        .write_knowledge(match_id, &hash, KNOWLEDGE_VERSION, &bytes);
                    return summary_markdown(&digest);
                }
            }
        }
        self.store
            .read_latest_knowledge(match_id)
            .ok()
            .flatten()
            .map(|(_, digest)| summary_markdown(&digest))
            .unwrap_or_else(|| {
                "# สรุปแมตช์\n\nรีเพลย์ถูกนำออกจากคลังแล้ว และยังไม่มี digest ที่จัดเก็บไว้.".to_owned()
            })
    }

    fn run_chat(&self, mut prepared: PreparedChat, app: &tauri::AppHandle) {
        let mut usage_total = UsageTotal::default();
        let mut bytes_sent = 0_u64;
        let mut bytes_received = 0_u64;
        let prior_history = prepared.history.clone();
        let first = self.build_request(&prepared, &prepared.history, Some(&prepared.question));
        let mut outgoing = match first {
            Ok(request) => request,
            Err(code) => {
                self.complete_error(
                    &prepared.request_id,
                    ApiError::Stream(code.into()),
                    None,
                    app,
                );
                return;
            }
        };
        let Some(user_message) = serde_json::from_slice::<Value>(&outgoing.body)
            .ok()
            .and_then(|body| {
                body.get("messages")
                    .and_then(Value::as_array)
                    .and_then(|messages| messages.last())
                    .cloned()
            })
        else {
            self.complete_error(
                &prepared.request_id,
                ApiError::Stream("request_serialization_failed".into()),
                None,
                app,
            );
            return;
        };
        let stored_user = chat_entry(&prepared.provider_id, &prepared.model, user_message.clone());
        prepared.history.push(stored_user);
        if !prepared.report
            && self
                .store
                .append_chat(
                    &prepared.match_id,
                    &prepared.provider_id,
                    &prepared.model,
                    user_message,
                )
                .is_err()
        {
            self.complete_error(
                &prepared.request_id,
                ApiError::Stream("chat_storage_failed".into()),
                None,
                app,
            );
            return;
        }
        let mut tool_rounds = 0_u8;
        let mut final_error: Option<ApiError>;
        let mut created_note = None;
        let mut report_markdown = String::new();
        let mut total_tool_calls = 0_u32;
        'conversation: loop {
            if prepared.cancellation.is_cancelled() {
                final_error = Some(ApiError::Cancelled);
                break;
            }
            let mut streamed = match self.send_request(
                &outgoing,
                &prepared,
                app,
                &mut bytes_sent,
                &mut bytes_received,
            ) {
                Ok(message) => message,
                Err(error) => {
                    let is_tool_rejection = matches!(
                        &error,
                        ApiError::BadRequest(message)
                            if {
                                let lowered = message.to_ascii_lowercase();
                                lowered.contains("tool") || lowered.contains("function_call")
                            }
                    );
                    if is_tool_rejection && prepared.tools_mode != ToolsMode::Unsupported {
                        prepared.tools_mode = ToolsMode::Unsupported;
                        prepared.tool_defs.clear();
                        if let Ok(mut session) =
                            serde_json::from_str::<Value>(&prepared.context.session)
                        {
                            session["tool_support"] = Value::from("provider rejected tool calling; answer from the digest only and state when a needed detail cannot be looked up");
                            prepared.context.session =
                                serde_json::to_string(&session).unwrap_or_default();
                        }
                        let mode_saved = self
                            .remember_tools_mode(&prepared.provider_id, "unsupported")
                            .is_ok();
                        if mode_saved {
                            match self.build_request(
                                &prepared,
                                &prior_history,
                                Some(&prepared.question),
                            ) {
                                Ok(without_tools) => {
                                    outgoing = without_tools;
                                    continue 'conversation;
                                }
                                Err(code) => {
                                    final_error = Some(ApiError::Stream(code.into()));
                                    break;
                                }
                            }
                        }
                    }
                    final_error = Some(error);
                    break;
                }
            };
            usage_total.add(&streamed.usage);
            let tools = finalize_tool_blocks(&mut streamed);
            total_tool_calls = total_tool_calls.saturating_add(tools.len() as u32);
            report_markdown.push_str(&text_from_content(&streamed.content));
            let assistant = json!({"role":"assistant","content":streamed.content});
            let stored_assistant =
                chat_entry(&prepared.provider_id, &prepared.model, assistant.clone());
            prepared.history.push(stored_assistant);
            if !prepared.report
                && self
                    .store
                    .append_chat(
                        &prepared.match_id,
                        &prepared.provider_id,
                        &prepared.model,
                        assistant,
                    )
                    .is_err()
            {
                final_error = Some(ApiError::Stream("chat_storage_failed".into()));
                break;
            }
            match streamed.stop_reason.as_deref() {
                Some("max_tokens") => {
                    final_error = Some(ApiError::MaxTokens);
                    break;
                }
                Some("refusal") => {
                    // The nullable stop_details are read defensively and never shown or logged.
                    let _safe_details = streamed
                        .stop_details
                        .as_ref()
                        .and_then(|details| serde_json::to_string(details).ok())
                        .map(|text| text.replace(&prepared.key, "[redacted]"));
                    final_error = Some(ApiError::Refusal);
                    break;
                }
                Some("tool_use") => {
                    tool_rounds = tool_rounds.saturating_add(1);
                    let over_round_limit = tool_round_limit_reached(tool_rounds);
                    let mut results = Vec::new();
                    let tool_blocks = prepared
                        .history
                        .last()
                        .and_then(|entry| entry.get("message").or(Some(entry)))
                        .and_then(|message| message.get("content"))
                        .and_then(Value::as_array)
                        .map(|blocks| {
                            blocks
                                .iter()
                                .filter(|block| {
                                    block.get("type").and_then(Value::as_str) == Some("tool_use")
                                })
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    for (index, tool_result) in tools.into_iter().enumerate() {
                        let failed_tool_parse = tool_result.is_err();
                        let fallback = tool_blocks.get(index).cloned().unwrap_or(Value::Null);
                        let block = match tool_result {
                            Ok(block) => block,
                            Err(_) => fallback,
                        };
                        let tool_id = block.get("id").and_then(Value::as_str).unwrap_or("invalid");
                        let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                        let input = block.get("input").unwrap_or(&Value::Null);
                        let result: Result<Value, &'static str> = if over_round_limit {
                            Err("tool_round_limit")
                        } else if name == "write_note" {
                            if prepared.report {
                                Err("write_note_unavailable_for_report")
                            } else if let Ok((title, markdown)) =
                                super::protocol::valid_write_note_input(input)
                            {
                                self.store
                                    .write_note(&prepared.match_id, title, markdown)
                                    .map(|id| {
                                        created_note = Some(id.clone());
                                        json!({"saved":true,"id":id})
                                    })
                            } else {
                                Err("invalid_tool_schema")
                            }
                        } else if failed_tool_parse {
                            Err("invalid_tool_schema")
                        } else {
                            tools::execute(
                                &ToolContext {
                                    match_id: &prepared.match_id,
                                    digest: &prepared.digest,
                                    bundle: prepared.bundle.as_ref(),
                                    store: &self.store,
                                },
                                name,
                                input,
                            )
                        };
                        match result {
                            Ok(value) => results.push(json!({"type":"tool_result","tool_use_id":tool_id,"content":serde_json::to_string(&value).unwrap_or_else(|_|"{\"error\":\"tool_result_invalid\"}".into())})),
                            Err(code) => results.push(json!({"type":"tool_result","tool_use_id":tool_id,"is_error":true,"content":serde_json::to_string(&json!({"error":code})).unwrap_or_else(|_|"{\"error\":\"tool_failed\"}".into())})),
                        }
                    }
                    let result_message = json!({"role":"user","content":results});
                    let stored_result = chat_entry(
                        &prepared.provider_id,
                        &prepared.model,
                        result_message.clone(),
                    );
                    prepared.history.push(stored_result);
                    if !prepared.report
                        && self
                            .store
                            .append_chat(
                                &prepared.match_id,
                                &prepared.provider_id,
                                &prepared.model,
                                result_message,
                            )
                            .is_err()
                    {
                        final_error = Some(ApiError::Stream("chat_storage_failed".into()));
                        break;
                    }
                    if over_round_limit {
                        final_error = Some(ApiError::Stream("tool_round_limit".into()));
                        break;
                    }
                    match self.build_request(&prepared, &prepared.history, None) {
                        Ok(next) => {
                            outgoing = next;
                            continue 'conversation;
                        }
                        Err(code) => {
                            final_error = Some(ApiError::Stream(code.into()));
                            break;
                        }
                    }
                }
                _ => {
                    final_error = None;
                    break;
                }
            }
        }
        prepared.key.clear();
        if prepared.cancellation.is_cancelled() {
            final_error = Some(ApiError::Cancelled);
        }
        if let Some(error) = final_error {
            let _ = app.emit("analytics://complete", json!({"request_id":prepared.request_id,"error":error.renderer_text(),"usage":usage_json(&usage_total),"bytes_sent":bytes_sent,"bytes_received":bytes_received}));
        } else {
            if prepared.report {
                if report_markdown.trim().is_empty() {
                    let _ = app.emit("analytics://complete", json!({"request_id":prepared.request_id,"error":"empty_report","usage":usage_json(&usage_total)}));
                    return;
                }
                match self.store.write_ai_report(
                    &prepared.match_id,
                    &prepared.bundle_hash,
                    &prepared.model,
                    PROMPT_VERSION,
                    KNOWLEDGE_VERSION,
                    prepared.own_hero.as_deref(),
                    report_markdown.trim(),
                    report_usage_json(&usage_total),
                    total_tool_calls,
                ) {
                    Ok(report) => {
                        let reasons = report_stale_reasons(
                            &report.meta,
                            &prepared.bundle_hash,
                            KNOWLEDGE_VERSION,
                            prepared.own_hero.as_deref(),
                        );
                        let _ = app.emit("analytics://complete", json!({"request_id":prepared.request_id,"report":report,"stale_reasons":reasons,"usage":usage_json(&usage_total),"bytes_sent":bytes_sent,"bytes_received":bytes_received}));
                    }
                    Err(code) => self.complete_error(
                        &prepared.request_id,
                        ApiError::Stream(code.into()),
                        None,
                        app,
                    ),
                }
                return;
            }
            let note_fallback = created_note.is_none();
            let _ = app.emit("analytics://complete", json!({"request_id":prepared.request_id,"usage":usage_json(&usage_total),"bytes_sent":bytes_sent,"bytes_received":bytes_received,"note_id":created_note,"note_fallback":note_fallback}));
        }
    }

    fn send_request(
        &self,
        outgoing: &super::protocol::OutgoingRequest,
        prepared: &PreparedChat,
        app: &tauri::AppHandle,
        bytes_sent_total: &mut u64,
        bytes_received_total: &mut u64,
    ) -> Result<ParsedMessage, ApiError> {
        let header_bytes = outgoing
            .headers
            .iter()
            .map(|(key, value)| key.len() as u64 + value.len() as u64 + 4)
            .sum::<u64>();
        let sent_this = outgoing.body.len() as u64 + header_bytes;
        let mut attempt = 0_u8;
        loop {
            if prepared.cancellation.is_cancelled() {
                return Err(ApiError::Cancelled);
            }
            let mut decoder = if prepared.provider_id == "anthropic" {
                AnthropicAdapter.new_stream()
            } else {
                OpenAiCompatAdapter.new_stream()
            };
            let mut stream_error = None;
            let suffix = if prepared.provider_id == "anthropic" {
                "/messages"
            } else {
                "/chat/completions"
            };
            let endpoint = prepared.endpoint.request_endpoint(suffix);
            let response = self.transport.post(
                &endpoint,
                outgoing,
                &prepared.cancellation,
                &mut |status, chunk| {
                    if status < 200 || status >= 300 {
                        return;
                    }
                    if stream_error.is_some() {
                        return;
                    }
                    match decoder.feed(chunk) {
                        Ok(events) => emit_neutral_text(
                            &events,
                            &prepared.request_id,
                            &prepared.cancellation,
                            app,
                        ),
                        Err(error) => stream_error = Some(error),
                    }
                },
            );
            match response {
                Ok(response) if response.status >= 200 && response.status < 300 => {
                    if let Some(error) = stream_error {
                        self.record_ledger(
                            prepared,
                            sent_this,
                            response.bytes_received,
                            &Usage::default(),
                            "stream_invalid",
                        );
                        return Err(ApiError::Stream(error.into()));
                    }
                    match decoder.finish() {
                        Ok(events) => emit_neutral_text(
                            &events,
                            &prepared.request_id,
                            &prepared.cancellation,
                            app,
                        ),
                        Err(error) => {
                            self.record_ledger(
                                prepared,
                                sent_this,
                                response.bytes_received,
                                &Usage::default(),
                                "stream_invalid",
                            );
                            return Err(ApiError::Stream(error.into()));
                        }
                    }
                    let mut parsed = decoder.parsed().clone();
                    if let Some((kind, message)) = parsed.api_error.take() {
                        let safe = message.replace(&prepared.key, "[redacted]");
                        let error = if kind.contains("overloaded") {
                            ApiError::Overloaded
                        } else {
                            ApiError::Stream(safe.chars().take(500).collect())
                        };
                        self.record_ledger(
                            prepared,
                            sent_this,
                            response.bytes_received,
                            &parsed.usage,
                            status_code(&error),
                        );
                        if let Some(delay) = retry_delay_ms(&error, attempt) {
                            *bytes_sent_total = bytes_sent_total.saturating_add(sent_this);
                            *bytes_received_total =
                                bytes_received_total.saturating_add(response.bytes_received);
                            if wait_cancel(&prepared.cancellation, delay) {
                                return Err(ApiError::Cancelled);
                            }
                            attempt += 1;
                            continue;
                        }
                        return Err(error);
                    }
                    *bytes_sent_total = bytes_sent_total.saturating_add(sent_this);
                    *bytes_received_total =
                        bytes_received_total.saturating_add(response.bytes_received);
                    self.record_ledger(
                        prepared,
                        sent_this,
                        response.bytes_received,
                        &parsed.usage,
                        "success",
                    );
                    return Ok(parsed);
                }
                Ok(response) => {
                    let error = if (300..400).contains(&response.status) {
                        ApiError::Stream(format!("redirect_refused_http_{}", response.status))
                    } else if prepared.provider_id == "anthropic" {
                        AnthropicAdapter.map_http_error(
                            response.status,
                            response.retry_after.as_deref(),
                            &response.body,
                            &prepared.key,
                        )
                    } else {
                        OpenAiCompatAdapter.map_http_error(
                            response.status,
                            response.retry_after.as_deref(),
                            &response.body,
                            &prepared.key,
                        )
                    };
                    self.record_ledger(
                        prepared,
                        sent_this,
                        response.bytes_received,
                        &Usage::default(),
                        status_code(&error),
                    );
                    *bytes_sent_total = bytes_sent_total.saturating_add(sent_this);
                    *bytes_received_total =
                        bytes_received_total.saturating_add(response.bytes_received);
                    if !(300..400).contains(&response.status) {
                        if let Some(delay) = retry_delay_ms(&error, attempt) {
                            if wait_cancel(&prepared.cancellation, delay) {
                                return Err(ApiError::Cancelled);
                            }
                            attempt += 1;
                            continue;
                        }
                    }
                    return Err(error);
                }
                Err(error) => {
                    let mapped = match error {
                        TransportError::Cancelled => ApiError::Cancelled,
                        TransportError::Tls | TransportError::Network | TransportError::Timeout
                            if prepared.endpoint.is_loopback() =>
                        {
                            ApiError::LocalProgramUnavailable
                        }
                        TransportError::Tls | TransportError::Network | TransportError::Timeout => {
                            ApiError::NetworkTls
                        }
                        TransportError::TooLarge => ApiError::Stream("response_too_large".into()),
                        TransportError::NotSent => ApiError::NetworkTls,
                    };
                    let sent = if matches!(error, TransportError::NotSent) {
                        0
                    } else {
                        sent_this
                    };
                    self.record_ledger(prepared, sent, 0, &Usage::default(), status_code(&mapped));
                    *bytes_sent_total = bytes_sent_total.saturating_add(sent);
                    return Err(mapped);
                }
            }
        }
    }

    fn record_ledger(
        &self,
        prepared: &PreparedChat,
        sent: u64,
        received: u64,
        usage: &Usage,
        status: &str,
    ) {
        let _ = self.store.add_ledger(LedgerRow {
            time_unix_ms: now_ms(),
            provider: prepared.provider_id.clone(),
            endpoint_class: if prepared.endpoint.is_loopback() {
                "loopback".into()
            } else {
                "cloud".into()
            },
            model: prepared.model.clone(),
            bytes_sent: sent,
            bytes_received: received,
            input_tokens: usage.input_tokens,
            cache_read_tokens: usage.cache_read_input_tokens,
            cache_creation_tokens: usage.cache_creation_input_tokens,
            output_tokens: usage.output_tokens,
            status: status.to_owned(),
        });
    }

    fn complete_error(
        &self,
        request_id: &str,
        error: ApiError,
        usage: Option<&Usage>,
        app: &tauri::AppHandle,
    ) {
        let _ = app.emit("analytics://complete", json!({"request_id":request_id,"error":error.renderer_text(),"usage":usage.map(usage_values)}));
    }
}

impl PreparedChat {
    pub(crate) fn request_id(&self) -> &str {
        &self.request_id
    }
}

#[derive(Default)]
struct UsageTotal {
    input: u64,
    cache_read: u64,
    cache_creation: u64,
    output: u64,
    has_input: bool,
    has_cache_read: bool,
    has_cache_creation: bool,
    has_output: bool,
}
impl UsageTotal {
    fn add(&mut self, usage: &Usage) {
        if let Some(value) = usage.input_tokens {
            self.input = self.input.saturating_add(value);
            self.has_input = true;
        }
        if let Some(value) = usage.cache_read_input_tokens {
            self.cache_read = self.cache_read.saturating_add(value);
            self.has_cache_read = true;
        }
        if let Some(value) = usage.cache_creation_input_tokens {
            self.cache_creation = self.cache_creation.saturating_add(value);
            self.has_cache_creation = true;
        }
        if let Some(value) = usage.output_tokens {
            self.output = self.output.saturating_add(value);
            self.has_output = true;
        }
    }
}
fn usage_values(usage: &Usage) -> Value {
    json!({"input":usage.input_tokens,"cache_read":usage.cache_read_input_tokens,"cache_creation":usage.cache_creation_input_tokens,"output":usage.output_tokens})
}
fn usage_json(usage: &UsageTotal) -> Value {
    json!({"input":usage.has_input.then_some(usage.input),"cache_read":usage.has_cache_read.then_some(usage.cache_read),"cache_creation":usage.has_cache_creation.then_some(usage.cache_creation),"output":usage.has_output.then_some(usage.output)})
}
fn report_usage_json(usage: &UsageTotal) -> Value {
    json!({"input":usage.has_input.then_some(usage.input),"cache_read":usage.has_cache_read.then_some(usage.cache_read),"cache_write":usage.has_cache_creation.then_some(usage.cache_creation),"output":usage.has_output.then_some(usage.output)})
}
fn status_code(error: &ApiError) -> &'static str {
    match error {
        ApiError::InvalidKey => "http_401",
        ApiError::Permission => "http_403",
        ApiError::BadRequest(_) => "http_400",
        ApiError::Billing => "http_402",
        ApiError::NotFound => "http_404",
        ApiError::RateLimitSpendCap => "rate_limit_spend_cap",
        ApiError::RateLimitWait { .. } => "rate_limited",
        ApiError::Overloaded => "overloaded",
        ApiError::LocalProgramUnavailable => "local_program_unavailable",
        ApiError::NetworkTls => "network_tls",
        ApiError::Cancelled => "cancelled",
        ApiError::Refusal => "refusal",
        ApiError::MaxTokens => "max_tokens",
        ApiError::Stream(_) => "stream_error",
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn report_cache_key(
    bundle_hash: &str,
    model: &str,
    prompt_version: u32,
    knowledge_version: u32,
) -> String {
    format!("{bundle_hash}:{model}:p{prompt_version}:k{knowledge_version}")
}
fn estimate_history_tokens(history: &[Value]) -> usize {
    serde_json::to_vec(history)
        .map(|bytes| bytes.len() / HISTORY_ESTIMATED_BYTES_PER_TOKEN)
        .unwrap_or(usize::MAX)
}
fn format_volatile(time_s: Option<f64>, selected_hero: Option<&str>) -> String {
    let time = time_s
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|seconds| {
            let total = seconds.floor() as u64;
            format!("{:02}:{:02} ({} s)", total / 60, total % 60, total)
        })
        .unwrap_or_else(|| "— ไม่ทราบ".into());
    format!(
        "Replay time: {time}. Selected hero: {}.",
        selected_hero.unwrap_or("— ไม่ทราบ")
    )
}
fn text_from_content(content: &[Value]) -> String {
    content
        .iter()
        .filter_map(|block| {
            (block.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| block.get("text").and_then(Value::as_str))
                .flatten()
        })
        .collect::<Vec<_>>()
        .join("")
}
fn summary_markdown(digest: &Value) -> String {
    let duration = digest
        .pointer("/meta/duration_s")
        .map(display_json)
        .unwrap_or_else(|| "— ไม่ทราบ".into());
    let patch = digest
        .pointer("/meta/patch")
        .map(display_json)
        .unwrap_or_else(|| "— ไม่ทราบ".into());
    let winner = digest
        .pointer("/meta/winner")
        .map(display_json)
        .unwrap_or_else(|| "— ไม่ทราบ".into());
    let mut out =
        format!("# สรุปแมตช์\n\n- ระยะเวลา: {duration}\n- แพตช์: {patch}\n- ผู้ชนะ: {winner}\n");
    for side in ["radiant", "dire"] {
        out.push_str(&format!(
            "\n## {}\n",
            if side == "radiant" { "Radiant" } else { "Dire" }
        ));
        let mut heroes = digest
            .get("heroes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|hero| hero.get("side").and_then(Value::as_str) == Some(side))
            .collect::<Vec<_>>();
        heroes.sort_by_key(|hero| hero.get("slot").and_then(Value::as_u64).unwrap_or(u64::MAX));
        if heroes.is_empty() {
            out.push_str("- — ไม่ทราบ\n");
        }
        for hero in heroes {
            let name = hero
                .get("hero")
                .map(display_json)
                .unwrap_or_else(|| "— ไม่ทราบ".into());
            let kills = hero
                .pointer("/final/k")
                .map(display_json)
                .unwrap_or_else(|| "— ไม่ทราบ".into());
            let deaths = hero
                .pointer("/final/d")
                .map(display_json)
                .unwrap_or_else(|| "— ไม่ทราบ".into());
            out.push_str(&format!("- {name} · kills {kills} · deaths {deaths}\n"));
        }
    }
    out.push_str("\nตัวเลขที่ไม่มีใน digest ระบุเป็น ‘— ไม่ทราบ’ พร้อมเหตุผลจากข้อมูลต้นทาง.\n");
    out
}
fn display_json(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => "— ไม่ทราบ".to_owned(),
        _ => value.to_string(),
    }
}
fn wait_cancel(cancellation: &Cancellation, millis: u64) -> bool {
    let until = std::time::Instant::now() + Duration::from_millis(millis);
    while std::time::Instant::now() < until {
        if cancellation.is_cancelled() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    cancellation.is_cancelled()
}
fn emit_neutral_text(
    events: &[NeutralEvent],
    request_id: &str,
    cancellation: &Cancellation,
    app: &tauri::AppHandle,
) {
    for event in events {
        if cancellation.is_cancelled() {
            break;
        }
        if let NeutralEvent::TextDelta(text) = event {
            let _ = app.emit(
                "analytics://delta",
                json!({"request_id":request_id,"text":text}),
            );
        }
    }
}

fn chat_entry(provider: &str, model: &str, message: Value) -> Value {
    json!({"provider":provider,"model":model,"message":message})
}

fn history_for_request(
    history: &[Value],
    target_provider: &str,
    target_model: &str,
) -> Result<Vec<Value>, &'static str> {
    history
        .iter()
        .map(|entry| {
            let Some(message) = entry.get("message") else {
                // Version-1 chat files contain bare Anthropic-shaped messages.
                return Ok(entry.clone());
            };
            let source_provider = entry
                .get("provider")
                .and_then(Value::as_str)
                .ok_or("history_invalid")?;
            let source_model = entry
                .get("model")
                .and_then(Value::as_str)
                .ok_or("history_invalid")?;
            if target_provider != "anthropic"
                || (source_provider == target_provider && source_model == target_model)
            {
                return Ok(message.clone());
            }
            let mut message = message.clone();
            if let Some(blocks) = message.get_mut("content").and_then(Value::as_array_mut) {
                blocks.retain(|block| {
                    !matches!(
                        block.get("type").and_then(Value::as_str),
                        Some("thinking" | "redacted_thinking" | "signature")
                    )
                });
            }
            Ok(message)
        })
        .collect()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn find_gsi_match<'a>(
    store: &AnalyticsStore,
    match_id: &str,
    history: &'a [MatchEntry],
) -> Option<&'a MatchEntry> {
    let match_key = store.match_key(match_id).ok()?;
    history.iter().find(|entry| entry.match_key == match_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::transport::fake::FakeTransport;
    use crate::match_history::{MatchEnd, MatchResult, MatchType};
    use crate::replay_library::ReplayLibraryLimits;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
    fn root() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "analytics-service-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn entry(match_key: String) -> MatchEntry {
        MatchEntry {
            match_key,
            hero: Some("npc_dota_hero_axe".into()),
            result: Some(MatchResult::Win),
            end: MatchEnd::Completed,
            kills: Some(8),
            deaths: Some(2),
            assists: Some(4),
            duration_s: Some(1800),
            ended_at: 123,
            started_at: None,
            hero_id: None,
            match_type: MatchType::Unknown,
            source: "kohbor".into(),
        }
    }
    fn options() -> ChatOptions {
        ChatOptions {
            match_id: "12345".into(),
            question: "why?".into(),
            selected_hero: None,
            scrubber_time_s: None,
        }
    }

    #[test]
    fn tool_round_limit_allows_eight_rounds_and_blocks_the_ninth() {
        assert!(!tool_round_limit_reached(8));
        assert!(tool_round_limit_reached(9));
    }

    #[test]
    fn closed_ai_gates_make_zero_transport_calls() {
        let directory = root();
        let transport = Arc::new(FakeTransport::default());
        let service = AnalyticsService::open(&directory, Some([11; 32]), transport.clone());
        let library = ReplayLibrary::open(
            directory.join("replays"),
            None,
            Arc::new(|| false),
            Arc::new(|| {}),
            ReplayLibraryLimits::default(),
        );
        let mut prefs = AnalyticsPreferences::default();

        prefs.ai_on = false;
        prefs.consent = false;
        service.store.write_preferences(&prefs).unwrap();
        assert_eq!(
            service.prepare_chat(&library, &[], options()).err(),
            Some("ai_off")
        );
        prefs.ai_on = true;
        service.store.write_preferences(&prefs).unwrap();
        assert_eq!(
            service.prepare_chat(&library, &[], options()).err(),
            Some("consent_required")
        );
        prefs.consent = true;
        service.store.write_preferences(&prefs).unwrap();
        service
            .store
            .set_provider_consent("anthropic", true)
            .unwrap();
        assert_eq!(
            service.prepare_chat(&library, &[], options()).err(),
            Some("key_required")
        );
        assert_eq!(transport.calls.load(Ordering::Relaxed), 0);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn model_list_redirect_fails_after_one_call_to_the_selected_endpoint() {
        let directory = root();
        let transport = Arc::new(FakeTransport {
            status: 302,
            ..Default::default()
        });
        let service = AnalyticsService::open(&directory, Some([21; 32]), transport.clone());
        service.store.set_provider_consent("litellm", true).unwrap();

        assert_eq!(service.fetch_models("litellm"), Err("redirect_refused"));
        assert_eq!(transport.calls.load(Ordering::Relaxed), 1);
        let endpoints = transport.endpoints.lock().unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].host, "127.0.0.1");
        assert_eq!(endpoints[0].port, 4000);
        assert_eq!(endpoints[0].path, "/v1/models");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn cross_provider_history_keeps_text_but_drops_foreign_reasoning_blocks() {
        let previous = chat_entry(
            "openai",
            "model-a",
            json!({"role":"assistant","content":[
                {"type":"thinking","thinking":"private"},
                {"type":"text","text":"public answer"}
            ]}),
        );
        let to_anthropic =
            history_for_request(&[previous.clone()], "anthropic", "claude-opus-5-5").unwrap();
        let blocks = to_anthropic[0]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["text"], "public answer");

        let same_provider_and_model = chat_entry(
            "anthropic",
            "claude-opus-5-5",
            json!({"role":"assistant","content":[{"type":"thinking","thinking":"same-model"}]}),
        );
        let forwarded =
            history_for_request(&[same_provider_and_model], "anthropic", "claude-opus-5-5")
                .unwrap();
        assert_eq!(forwarded[0]["content"][0]["thinking"], "same-model");
        assert!(history_for_request(
            &[json!({"provider":"anthropic","message":{}})],
            "anthropic",
            "claude-opus-5-5"
        )
        .is_err());
    }

    #[test]
    fn gsi_history_match_uses_install_key_and_misses_without_a_matching_hash() {
        let directory = root();
        let store = AnalyticsStore::open(&directory, Some([12; 32]));
        let matched_key = store.match_key("12345").unwrap();
        let matched = entry(matched_key);
        assert!(find_gsi_match(&store, "12345", std::slice::from_ref(&matched)).is_some());
        assert!(find_gsi_match(&store, "54321", std::slice::from_ref(&matched)).is_none());
        let unavailable = AnalyticsStore::open(&directory, None);
        assert!(find_gsi_match(&unavailable, "12345", std::slice::from_ref(&matched)).is_none());
        let _ = fs::remove_dir_all(directory);
    }
}
