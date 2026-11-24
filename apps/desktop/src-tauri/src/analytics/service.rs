use super::context::{build_match_context, MatchContextInput};
use super::protocol::{
    build_request, finalize_tool_blocks, map_http_error, retry_delay_ms, ApiError, Model,
    ParsedMessage, RequestInput, SseParser, Usage,
};
use super::store::{AnalyticsPreferences, AnalyticsStore, LedgerRow};
use super::transport::{Cancellation, Transport, TransportError};
use crate::match_history::MatchEntry;
use crate::replay_library::ReplayLibrary;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

#[derive(Clone)]
pub(crate) struct AnalyticsService {
    store: AnalyticsStore,
    transport: Arc<dyn Transport>,
    active: Arc<Mutex<HashMap<String, Cancellation>>>,
}

pub(crate) struct PreparedChat {
    request_id: String,
    match_id: String,
    key: String,
    model: Model,
    context: String,
    question: String,
    document: String,
    selected_hero: Option<String>,
    scrubber_time_s: Option<f64>,
    include_clock: bool,
    include_selected_hero: bool,
    include_document: bool,
    history: Vec<Value>,
    cancellation: Cancellation,
}

#[derive(Clone, Debug)]
pub(crate) struct ChatOptions {
    pub(crate) match_id: String,
    pub(crate) question: String,
    pub(crate) open_document: String,
    pub(crate) selected_hero: Option<String>,
    pub(crate) include_own_hero: bool,
    pub(crate) include_match: bool,
    pub(crate) include_clock: bool,
    pub(crate) include_document: bool,
    pub(crate) include_selected_hero: bool,
    pub(crate) scrubber_time_s: Option<f64>,
}

impl AnalyticsService {
    pub(crate) fn open(data_dir: &Path, install_key: Option<[u8; 32]>, transport: Arc<dyn Transport>) -> Self {
        Self { store: AnalyticsStore::open(data_dir, install_key), transport, active: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub(crate) fn store(&self) -> &AnalyticsStore { &self.store }
    pub(crate) fn preferences(&self) -> Result<AnalyticsPreferences, &'static str> { self.store.preferences() }
    pub(crate) fn key_status(&self) -> Result<Value, &'static str> { self.store.key_status() }
    pub(crate) fn set_key(&self, key: &str) -> Result<(), &'static str> {
        self.cancel_all();
        self.store.set_key(key)?;
        let mut settings = self.store.preferences()?; settings.ai_on = false; self.store.write_preferences(&settings)
    }
    pub(crate) fn delete_key(&self) -> Result<(), &'static str> {
        self.cancel_all(); self.store.delete_key()?;
        let mut settings = self.store.preferences()?; settings.ai_on = false; self.store.write_preferences(&settings)
    }
    pub(crate) fn set_consent(&self, consent: bool) -> Result<AnalyticsPreferences, &'static str> {
        let mut settings = self.store.preferences()?; settings.consent = consent;
        if !consent { settings.ai_on = false; self.cancel_all(); }
        self.store.write_preferences(&settings)?; Ok(settings)
    }
    pub(crate) fn patch_preferences(&self, patch: &Value) -> Result<AnalyticsPreferences, &'static str> {
        let object = patch.as_object().ok_or("settings_patch_invalid")?;
        let mut settings = self.store.preferences()?;
        for (key, value) in object {
            match key.as_str() {
                "ai_on" => {
                    let enabled = value.as_bool().ok_or("settings_patch_invalid")?;
                    if enabled && (!settings.consent || self.store.read_key()?.is_none()) { return Err("ai_gate_closed"); }
                    settings.ai_on = enabled;
                    if !enabled { self.cancel_all(); }
                }
                "model" => settings.model = value.as_str().ok_or("settings_patch_invalid")?.to_owned(),
                "keep_newest" => settings.keep_newest = usize::try_from(value.as_u64().ok_or("settings_patch_invalid")?).map_err(|_| "settings_patch_invalid")?,
                "max_bytes" => settings.max_bytes = value.as_u64().ok_or("settings_patch_invalid")?,
                "auto_analyse" => settings.auto_analyse = value.as_bool().ok_or("settings_patch_invalid")?,
                _ => return Err("settings_patch_invalid"),
            }
        }
        self.store.write_preferences(&settings)?; Ok(settings)
    }

    pub(crate) fn prepare_chat(&self, library: &ReplayLibrary, history_entries: &[MatchEntry], options: ChatOptions) -> Result<PreparedChat, &'static str> {
        let settings = self.store.preferences()?;
        if !settings.ai_on { return Err("ai_off"); }
        if !settings.consent { return Err("consent_required"); }
        let key = self.store.read_key()?.ok_or("key_required")?;
        let key = String::from_utf8(key).map_err(|_| "key_invalid")?;
        if options.question.trim().is_empty() || options.question.len() > 8 * 1024 || options.open_document.len() > 64 * 1024 { return Err("question_invalid"); }
        let bundle = library.open_bundle(&options.match_id).map_err(|_| "bundle_unavailable")?;
        let gsi = find_gsi_match(&self.store, &options.match_id, history_entries);
        let own_hero = if !options.include_own_hero { None }
            else if let Some(entry) = gsi { entry.hero.clone() }
            else { self.store.own_hero(&options.match_id) };
        let context = if options.include_match {
            build_match_context(&MatchContextInput { manifest: &bundle.manifest, match_data: &bundle.match_data, world: &bundle.world,
                own_hero: own_hero.as_deref(), gsi_match: options.include_own_hero.then_some(gsi).flatten() })?
        } else { "{\"match_context\":\"omitted by user\"}".to_owned() };
        let heroes = bundle.match_data.get("heroes").and_then(Value::as_array).cloned().unwrap_or_default();
        let selected_hero = options.selected_hero.as_deref().and_then(|selected| heroes.iter().find(|hero| hero.get("id").and_then(Value::as_str) == Some(selected)))
            .and_then(|hero| hero.get("name").and_then(Value::as_str)).map(str::to_owned);
        let history = self.store.chat(&options.match_id)?;
        let bytes = crate::secrets::random::<16>().ok_or("request_id_unavailable")?;
        let request_id = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let cancellation = Cancellation::default();
        lock(&self.active).insert(request_id.clone(), cancellation.clone());
        Ok(PreparedChat { request_id, match_id: options.match_id, key, model: Model::parse(&settings.model)?, context,
            question: options.question, document: if options.include_document { options.open_document } else { String::new() },
            selected_hero, scrubber_time_s: options.include_clock.then_some(options.scrubber_time_s).flatten(),
            include_clock: options.include_clock, include_selected_hero: options.include_selected_hero,
            include_document: options.include_document, history, cancellation })
    }

    pub(crate) fn spawn_chat(&self, prepared: PreparedChat, app: tauri::AppHandle) {
        let service = self.clone();
        let request_id = prepared.request_id.clone();
        let thread_request_id = request_id.clone();
        let thread_app = app.clone();
        let spawned = thread::Builder::new().name("analytics-claude-chat".to_owned()).spawn(move || {
            service.run_chat(prepared, &thread_app);
            lock(&service.active).remove(&thread_request_id);
        });
        if spawned.is_err() {
            lock(&self.active).remove(&request_id);
            let _ = app.emit("analytics://complete", json!({"request_id":request_id,"error":ApiError::NetworkTls.renderer_text()}));
        }
    }

    pub(crate) fn cancel(&self, request_id: &str) {
        if let Some(cancellation) = lock(&self.active).get(request_id) { cancellation.cancel(); }
        self.transport.cancel_current();
    }

    pub(crate) fn cancel_all(&self) {
        for cancellation in lock(&self.active).values() { cancellation.cancel(); }
        self.transport.cancel_current();
    }

    pub(crate) fn list_notes(&self, library: &ReplayLibrary, match_id: &str, history: &[MatchEntry]) -> Result<Vec<super::store::NoteRow>, &'static str> {
        let summary = self.system_summary(library, match_id, history);
        self.store.list_notes(match_id, &summary)
    }

    pub(crate) fn system_summary(&self, library: &ReplayLibrary, match_id: &str, history: &[MatchEntry]) -> String {
        let Ok(list) = library.list() else { return "# สรุปแมตช์\n\nอ่านคลังรีเพลย์ไม่ได้ ข้อมูลโน้ตและแชทยังคงอยู่ในเครื่อง.".to_owned(); };
        let Some(entry) = list.matches.into_iter().find(|entry| entry.match_id == match_id) else {
            return "# สรุปแมตช์\n\nรีเพลย์ถูกนำออกจากคลังแล้ว ข้อมูลโน้ตและแชทยังคงอยู่ในเครื่อง.".to_owned();
        };
        if entry.status != crate::replay_library::ReplayStatus::Ready {
            return "# สรุปแมตช์\n\nรีเพลย์นี้ยังไม่มีชุดวิเคราะห์ที่พร้อมอ่าน.".to_owned();
        }
        let Ok(bundle) = library.open_bundle(match_id) else { return "# สรุปแมตช์\n\nเปิดชุดวิเคราะห์ของรีเพลย์นี้ไม่ได้.".to_owned(); };
        let gsi = find_gsi_match(&self.store, match_id, history);
        let own_hero = gsi.and_then(|entry| entry.hero.clone()).or_else(|| self.store.own_hero(match_id));
        let context = build_match_context(&MatchContextInput {
            manifest: &bundle.manifest, match_data: &bundle.match_data, world: &bundle.world,
            own_hero: own_hero.as_deref(), gsi_match: gsi,
        }).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok()).unwrap_or(Value::Null);
        let duration = context.get("duration_s").map(display_json).unwrap_or_else(|| "— ไม่ทราบ".into());
        let patch = context.get("patch").map(display_json).unwrap_or_else(|| "— ไม่ทราบ".into());
        let mut note = format!("# สรุปแมตช์\n\n- ระยะเวลา: {duration}\n- แพตช์: {patch}\n- ฮีโร่ของฉัน: {}\n", context.get("own_hero").map(display_json).unwrap_or_else(|| "— ไม่ทราบ".into()));
        if let Some(sides) = context.get("heroes_per_side").and_then(Value::as_object) {
            for side in ["radiant", "dire"] {
                note.push_str(&format!("\n## {}\n", if side == "radiant" { "Radiant" } else { "Dire" }));
                if let Some(heroes) = sides.get(side).and_then(Value::as_array) {
                    for hero in heroes {
                        note.push_str(&format!("- {} · kills {} · deaths {}\n",
                            hero.get("hero").map(display_json).unwrap_or_else(|| "— ไม่ทราบ".into()),
                            hero.get("hero_kills").map(display_json).unwrap_or_else(|| "— ไม่ทราบ".into()),
                            hero.get("deaths").map(display_json).unwrap_or_else(|| "— ไม่ทราบ".into())));
                    }
                } else { note.push_str("- — ไม่ทราบ\n"); }
            }
        }
        if let Some(history) = context.get("local_history") {
            if !history.is_null() { note.push_str(&format!("\n## ประวัติในเครื่อง\n\n{}\n", display_json(history))); }
        }
        note.push_str("\nสร้างจากข้อมูลในเครื่อง ไม่มีการใช้โมเดลภาษา.\n");
        note
    }

    fn run_chat(&self, mut prepared: PreparedChat, app: &tauri::AppHandle) {
        let mut usage_total = UsageTotal::default();
        let mut bytes_sent = 0_u64; let mut bytes_received = 0_u64;
        let first = build_request(&RequestInput { key: &prepared.key, model: prepared.model, context: &prepared.context, question: Some(&prepared.question),
            scrubber_time_s: prepared.scrubber_time_s, selected_hero: prepared.selected_hero.as_deref(), open_document: &prepared.document,
            include_clock: prepared.include_clock, include_selected_hero: prepared.include_selected_hero,
            include_document: prepared.include_document, history: &prepared.history });
        let mut outgoing = match first { Ok(request) => request, Err(code) => { self.complete_error(&prepared.request_id, ApiError::Stream(code.into()), None, app); return; } };
        let Some(user_message) = serde_json::from_slice::<Value>(&outgoing.body).ok().and_then(|body| body.get("messages").and_then(Value::as_array).and_then(|messages| messages.last()).cloned()) else {
            self.complete_error(&prepared.request_id, ApiError::Stream("request_serialization_failed".into()), None, app); return;
        };
        prepared.history.push(user_message.clone());
        if self.store.append_chat(&prepared.match_id, user_message).is_err() { self.complete_error(&prepared.request_id, ApiError::Stream("chat_storage_failed".into()), None, app); return; }
        let mut tool_rounds = 0_u8; let mut final_error: Option<ApiError>; let mut created_note = None;
        'conversation: loop {
            if prepared.cancellation.is_cancelled() { final_error = Some(ApiError::Cancelled); break; }
            let mut streamed = match self.send_request(&outgoing, &prepared, app, &mut bytes_sent, &mut bytes_received) {
                Ok(message) => message, Err(error) => { final_error = Some(error); break; }
            };
            usage_total.add(&streamed.usage);
            let tools = finalize_tool_blocks(&mut streamed);
            let assistant = json!({"role":"assistant","content":streamed.content});
            prepared.history.push(assistant.clone());
            if self.store.append_chat(&prepared.match_id, assistant).is_err() { final_error = Some(ApiError::Stream("chat_storage_failed".into())); break; }
            match streamed.stop_reason.as_deref() {
                Some("max_tokens") => { final_error = Some(ApiError::MaxTokens); break; }
                Some("refusal") => {
                    // The nullable stop_details are read defensively and never shown or logged.
                    let _safe_details = streamed.stop_details.as_ref().and_then(|details| serde_json::to_string(details).ok()).map(|text| text.replace(&prepared.key, "[redacted]"));
                    final_error = Some(ApiError::Refusal); break;
                }
                Some("tool_use") => {
                    tool_rounds = tool_rounds.saturating_add(1);
                    let over_round_limit = tool_rounds > 4;
                    let mut results = Vec::new();
                    let tool_blocks = prepared.history.last().and_then(|message| message.get("content")).and_then(Value::as_array)
                        .map(|blocks| blocks.iter().filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use")).cloned().collect::<Vec<_>>()).unwrap_or_default();
                    for (index, tool_result) in tools.into_iter().enumerate() {
                        let fallback = tool_blocks.get(index).cloned().unwrap_or(Value::Null);
                        let block = match tool_result { Ok(block) => block, Err(_) => fallback };
                        let tool_id = block.get("id").and_then(Value::as_str).unwrap_or("invalid");
                        let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                        let input = block.get("input").unwrap_or(&Value::Null);
                        let result = if over_round_limit || name != "write_note" { Err("tool_round_limit") }
                            else if let Ok((title, markdown)) = super::protocol::valid_write_note_input(input) { self.store.write_note(&prepared.match_id, title, markdown) }
                            else { Err("invalid_tool_schema") };
                        match result {
                            Ok(note_id) => { created_note = Some(note_id.clone()); results.push(json!({"type":"tool_result","tool_use_id":tool_id,"content":format!("Note created: {note_id}")})); }
                            Err(_) => results.push(json!({"type":"tool_result","tool_use_id":tool_id,"is_error":true,"content":"Invalid or truncated tool input; no file was written."})),
                        }
                    }
                    let result_message = json!({"role":"user","content":results});
                    prepared.history.push(result_message.clone());
                    if self.store.append_chat(&prepared.match_id, result_message).is_err() { final_error = Some(ApiError::Stream("chat_storage_failed".into())); break; }
                    if over_round_limit { final_error = Some(ApiError::Stream("tool_round_limit".into())); break; }
                    match build_request(&RequestInput { key:&prepared.key, model:prepared.model, context:&prepared.context, question:None,
                        scrubber_time_s:None, selected_hero:None, open_document:"", include_clock:false,
                        include_selected_hero:false, include_document:false, history:&prepared.history }) {
                        Ok(next) => { outgoing = next; continue 'conversation; }
                        Err(code) => { final_error = Some(ApiError::Stream(code.into())); break; }
                    }
                }
                _ => { final_error = None; break; }
            }
        }
        prepared.key.clear();
        if prepared.cancellation.is_cancelled() { final_error = Some(ApiError::Cancelled); }
        if let Some(error) = final_error {
            let _ = app.emit("analytics://complete", json!({"request_id":prepared.request_id,"error":error.renderer_text(),"usage":usage_json(&usage_total),"bytes_sent":bytes_sent,"bytes_received":bytes_received}));
        } else {
            let _ = app.emit("analytics://complete", json!({"request_id":prepared.request_id,"usage":usage_json(&usage_total),"bytes_sent":bytes_sent,"bytes_received":bytes_received,"note_id":created_note}));
        }
    }

    fn send_request(&self, outgoing: &super::protocol::OutgoingRequest, prepared: &PreparedChat, app: &tauri::AppHandle,
        bytes_sent_total: &mut u64, bytes_received_total: &mut u64) -> Result<ParsedMessage, ApiError> {
        let header_bytes = outgoing.headers.iter().map(|(key,value)| key.len() as u64 + value.len() as u64 + 4).sum::<u64>();
        let sent_this = outgoing.body.len() as u64 + header_bytes;
        let mut attempt = 0_u8;
        loop {
            if prepared.cancellation.is_cancelled() { return Err(ApiError::Cancelled); }
            let mut parser = SseParser::default(); let mut events = Vec::new(); let mut stream_error = None;
            let response = self.transport.post(outgoing, &prepared.cancellation, &mut |status, chunk| {
                if status < 200 || status >= 300 { return; }
                let start = events.len();
                if let Err(error) = parser.feed(chunk, &mut events) { stream_error = Some(error); return; }
                emit_text_deltas(&events[start..], &prepared.request_id, &prepared.cancellation, app);
            });
            match response {
                Ok(response) if response.status >= 200 && response.status < 300 => {
                    if let Some(error) = stream_error { self.record_ledger(prepared.model, sent_this, response.bytes_received, &Usage::default(), "stream_invalid"); return Err(ApiError::Stream(error.into())); }
                    let start = events.len();
                    if let Err(error) = parser.finish(&mut events) { self.record_ledger(prepared.model, sent_this, response.bytes_received, &Usage::default(), "stream_invalid"); return Err(ApiError::Stream(error.into())); }
                    emit_text_deltas(&events[start..], &prepared.request_id, &prepared.cancellation, app);
                    let mut parsed = super::protocol::parse_events(&events, &mut |_| {});
                    if let Some((kind, message)) = parsed.api_error.take() {
                        let safe = message.replace(&prepared.key, "[redacted]");
                        let error = if kind.contains("overloaded") { ApiError::Overloaded } else { ApiError::Stream(safe.chars().take(500).collect()) };
                        self.record_ledger(prepared.model, sent_this, response.bytes_received, &parsed.usage, status_code(&error));
                        if let Some(delay) = retry_delay_ms(&error, attempt) {
                            *bytes_sent_total = bytes_sent_total.saturating_add(sent_this); *bytes_received_total = bytes_received_total.saturating_add(response.bytes_received);
                            if wait_cancel(&prepared.cancellation, delay) { return Err(ApiError::Cancelled); }
                            attempt += 1; continue;
                        }
                        return Err(error);
                    }
                    *bytes_sent_total = bytes_sent_total.saturating_add(sent_this); *bytes_received_total = bytes_received_total.saturating_add(response.bytes_received);
                    self.record_ledger(prepared.model, sent_this, response.bytes_received, &parsed.usage, "success"); return Ok(parsed);
                }
                Ok(response) => {
                    let error = map_http_error(response.status, response.retry_after.as_deref(), &response.body, &prepared.key);
                    self.record_ledger(prepared.model, sent_this, response.bytes_received, &Usage::default(), status_code(&error));
                    *bytes_sent_total = bytes_sent_total.saturating_add(sent_this); *bytes_received_total = bytes_received_total.saturating_add(response.bytes_received);
                    if let Some(delay) = retry_delay_ms(&error, attempt) {
                        if wait_cancel(&prepared.cancellation, delay) { return Err(ApiError::Cancelled); }
                        attempt += 1; continue;
                    }
                    return Err(error);
                }
                Err(error) => {
                    let mapped = match error { TransportError::Cancelled => ApiError::Cancelled, TransportError::Tls | TransportError::Network | TransportError::Timeout => ApiError::NetworkTls, TransportError::TooLarge => ApiError::Stream("response_too_large".into()), TransportError::NotSent => ApiError::NetworkTls };
                    let sent = if matches!(error, TransportError::NotSent) { 0 } else { sent_this };
                    self.record_ledger(prepared.model, sent, 0, &Usage::default(), status_code(&mapped));
                    *bytes_sent_total = bytes_sent_total.saturating_add(sent);
                    return Err(mapped);
                }
            }
        }
    }

    fn record_ledger(&self, model: Model, sent: u64, received: u64, usage: &Usage, status: &str) {
        let _ = self.store.add_ledger(LedgerRow { time_unix_ms: now_ms(), model: model.as_str().to_owned(), bytes_sent: sent, bytes_received: received,
            input_tokens: usage.input_tokens, cache_read_tokens: usage.cache_read_input_tokens, cache_creation_tokens: usage.cache_creation_input_tokens,
            output_tokens: usage.output_tokens, status: status.to_owned() });
    }

    fn complete_error(&self, request_id: &str, error: ApiError, usage: Option<&Usage>, app: &tauri::AppHandle) {
        let _ = app.emit("analytics://complete", json!({"request_id":request_id,"error":error.renderer_text(),"usage":usage.map(usage_values)}));
    }
}

impl PreparedChat {
    pub(crate) fn request_id(&self) -> &str { &self.request_id }
}

#[derive(Default)]
struct UsageTotal { input: u64, cache_read: u64, cache_creation: u64, output: u64, has_input: bool, has_cache_read: bool, has_cache_creation: bool, has_output: bool }
impl UsageTotal {
    fn add(&mut self, usage: &Usage) {
        if let Some(value)=usage.input_tokens { self.input=self.input.saturating_add(value); self.has_input=true; }
        if let Some(value)=usage.cache_read_input_tokens { self.cache_read=self.cache_read.saturating_add(value); self.has_cache_read=true; }
        if let Some(value)=usage.cache_creation_input_tokens { self.cache_creation=self.cache_creation.saturating_add(value); self.has_cache_creation=true; }
        if let Some(value)=usage.output_tokens { self.output=self.output.saturating_add(value); self.has_output=true; }
    }
}
fn usage_values(usage: &Usage) -> Value { json!({"input":usage.input_tokens,"cache_read":usage.cache_read_input_tokens,"cache_creation":usage.cache_creation_input_tokens,"output":usage.output_tokens}) }
fn usage_json(usage: &UsageTotal) -> Value { json!({"input":usage.has_input.then_some(usage.input),"cache_read":usage.has_cache_read.then_some(usage.cache_read),"cache_creation":usage.has_cache_creation.then_some(usage.cache_creation),"output":usage.has_output.then_some(usage.output)}) }
fn status_code(error: &ApiError) -> &'static str { match error { ApiError::InvalidKey=>"http_401",ApiError::Permission=>"http_403",ApiError::BadRequest(_)=>"http_400",ApiError::Billing=>"http_402",ApiError::NotFound=>"http_404",ApiError::RateLimitSpendCap=>"rate_limit_spend_cap",ApiError::RateLimitWait{..}=>"rate_limited",ApiError::Overloaded=>"overloaded",ApiError::NetworkTls=>"network_tls",ApiError::Cancelled=>"cancelled",ApiError::Refusal=>"refusal",ApiError::MaxTokens=>"max_tokens",ApiError::Stream(_)=>"stream_error" } }
fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u64::MAX as u128) as u64 }
fn display_json(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => "— ไม่ทราบ".to_owned(),
        _ => value.to_string(),
    }
}
fn wait_cancel(cancellation: &Cancellation, millis: u64) -> bool {
    let until = std::time::Instant::now() + Duration::from_millis(millis);
    while std::time::Instant::now() < until { if cancellation.is_cancelled() { return true; } thread::sleep(Duration::from_millis(50)); }
    cancellation.is_cancelled()
}
fn emit_text_deltas(events: &[(Option<String>, String)], request_id: &str, cancellation: &Cancellation, app: &tauri::AppHandle) {
    for (name, data) in events {
        if cancellation.is_cancelled() || name.as_deref() != Some("content_block_delta") { continue; }
        let Ok(value) = serde_json::from_str::<Value>(data) else { continue; };
        if value.pointer("/delta/type").and_then(Value::as_str) != Some("text_delta") { continue; }
        if let Some(text) = value.pointer("/delta/text").and_then(Value::as_str) {
            let _ = app.emit("analytics://delta", json!({"request_id":request_id,"text":text}));
        }
    }
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> { mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner) }

fn find_gsi_match<'a>(store: &AnalyticsStore, match_id: &str, history: &'a [MatchEntry]) -> Option<&'a MatchEntry> {
    let match_key = store.match_key(match_id).ok()?;
    history.iter().find(|entry| entry.match_key == match_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_history::{MatchEnd, MatchResult, MatchType};
    use crate::replay_library::ReplayLibraryLimits;
    use crate::analytics::transport::fake::FakeTransport;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
    fn root() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("analytics-service-{}-{}", std::process::id(), NEXT_DIR.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&path).unwrap(); path
    }
    fn entry(match_key: String) -> MatchEntry {
        MatchEntry { match_key, hero: Some("npc_dota_hero_axe".into()), result: Some(MatchResult::Win), end: MatchEnd::Completed,
            kills: Some(8), deaths: Some(2), assists: Some(4), duration_s: Some(1800), ended_at: 123, started_at: None,
            hero_id: None, match_type: MatchType::Unknown, source: "kohbor".into() }
    }
    fn options() -> ChatOptions {
        ChatOptions { match_id: "12345".into(), question: "why?".into(), open_document: String::new(), selected_hero: None,
            include_own_hero: true, include_match: true, include_clock: false, include_document: false,
            include_selected_hero: false, scrubber_time_s: None }
    }

    #[test]
    fn closed_ai_gates_make_zero_transport_calls() {
        let directory = root();
        let transport = Arc::new(FakeTransport::default());
        let service = AnalyticsService::open(&directory, Some([11; 32]), transport.clone());
        let library = ReplayLibrary::open(directory.join("replays"), None, Arc::new(|| false), Arc::new(|| {}), ReplayLibraryLimits::default());
        let mut prefs = AnalyticsPreferences::default();

        prefs.ai_on = false; prefs.consent = false; service.store.write_preferences(&prefs).unwrap();
        assert_eq!(service.prepare_chat(&library, &[], options()).err(), Some("ai_off"));
        prefs.ai_on = true; service.store.write_preferences(&prefs).unwrap();
        assert_eq!(service.prepare_chat(&library, &[], options()).err(), Some("consent_required"));
        prefs.consent = true; service.store.write_preferences(&prefs).unwrap();
        assert_eq!(service.prepare_chat(&library, &[], options()).err(), Some("key_required"));
        assert_eq!(transport.calls.load(Ordering::Relaxed), 0);
        let _ = fs::remove_dir_all(directory);
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
