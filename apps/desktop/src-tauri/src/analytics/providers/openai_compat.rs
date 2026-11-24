use super::anthropic::{self, OutgoingRequest, ParsedMessage, SseParser, Usage};
use super::{NeutralEvent, StopKind, StreamDecoder, WireAdapter};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_RESPONSE_BYTES: usize = anthropic::MAX_RESPONSE_BYTES;
const SYSTEM_WITH_TOOLS: &str = "Answer only from the supplied replay data, in the user's language (Thai by default). Cite evidence as [07:45](#t=465) and [deaths#13](#ev=deaths/13). Unknown values must be written as \"— ไม่ทราบ\"; never estimate missing values. Kills mean hero kills only. There is no win probability. Use write_note for long analyses and keep the chat reply short.";
const SYSTEM_WITHOUT_TOOLS: &str = "Answer only from the supplied replay data, in the user's language (Thai by default). Cite evidence as [07:45](#t=465) and [deaths#13](#ev=deaths/13). Unknown values must be written as \"— ไม่ทราบ\"; never estimate missing values. Kills mean hero kills only. There is no win probability.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolsMode {
    Unknown,
    Supported,
    Unsupported,
}

pub(crate) struct RequestInput<'a> {
    pub(crate) provider: &'a str,
    pub(crate) model: &'a str,
    pub(crate) key: Option<&'a str>,
    pub(crate) context: &'a str,
    pub(crate) question: Option<&'a str>,
    pub(crate) scrubber_time_s: Option<f64>,
    pub(crate) selected_hero: Option<&'a str>,
    pub(crate) open_document: &'a str,
    pub(crate) include_clock: bool,
    pub(crate) include_selected_hero: bool,
    pub(crate) include_document: bool,
    pub(crate) history: &'a [Value],
    pub(crate) tools_mode: ToolsMode,
}

pub(crate) struct OpenAiCompatAdapter;

impl WireAdapter<RequestInput<'_>> for OpenAiCompatAdapter {
    fn build(&self, input: &RequestInput<'_>) -> Result<OutgoingRequest, &'static str> {
        build_request(input)
    }
    fn new_stream(&self) -> Box<dyn StreamDecoder> {
        Box::new(OpenAiStreamDecoder::default())
    }
    fn map_http_error(
        &self,
        status: u16,
        retry_after: Option<&str>,
        body: &[u8],
        key: &str,
    ) -> anthropic::ApiError {
        anthropic::map_http_error(status, retry_after, body, key)
    }
}

fn build_request(input: &RequestInput<'_>) -> Result<OutgoingRequest, &'static str> {
    super::registry::validate_model_id(input.model)?;
    if let Some(key) = input.key {
        if key.is_empty()
            || key.len() > 512
            || !key.is_ascii()
            || key
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        {
            return Err("key_invalid");
        }
    }
    if input
        .history
        .iter()
        .any(|message| input.key.is_some_and(|key| contains_secret(message, key)))
    {
        return Err("history_contains_secret");
    }
    let clean = |value: &str| {
        input
            .key
            .map_or_else(|| value.to_owned(), |key| value.replace(key, "[redacted]"))
    };
    let mut system = if input.tools_mode == ToolsMode::Unsupported {
        SYSTEM_WITHOUT_TOOLS
    } else {
        SYSTEM_WITH_TOOLS
    }
    .to_owned();
    system.push_str("\n\nSupplied match context:\n");
    system.push_str(&clean(input.context));
    let mut user = String::new();
    if input.include_clock {
        user.push_str("Current replay time: ");
        match input
            .scrubber_time_s
            .filter(|value| value.is_finite() && *value >= 0.0)
        {
            Some(seconds) => user.push_str(&format!("{seconds:.3}s\n")),
            None => user.push_str("— ไม่ทราบ\n"),
        }
    }
    if input.include_selected_hero {
        user.push_str("Selected hero: ");
        user.push_str(&clean(input.selected_hero.unwrap_or("— ไม่ทราบ")));
        user.push('\n');
    }
    if input.include_document {
        user.push_str("Open document text:\n");
        user.push_str(&clean(input.open_document));
        user.push_str("\n\n");
    }
    if let Some(question) = input.question {
        user.push_str("Question:\n");
        user.push_str(&clean(question));
    }
    let mut messages = vec![json!({"role":"system","content":system})];
    messages.extend(convert_history(input.history)?);
    if input.question.is_some() {
        messages.push(json!({"role":"user","content":user}));
    }
    let stream = input.provider != "qwen";
    let tools_enabled = input.tools_mode != ToolsMode::Unsupported && input.provider != "qwen";
    let mut body = json!({
        "model":input.model,
        "messages":messages,
        "max_tokens":16000,
        "stream":stream,
    });
    // Gemini and xAI usage in streaming was not verified; omit the optional vendor
    // extension there and leave absent usage unknown.
    if stream && !matches!(input.provider, "gemini" | "grok") {
        body["stream_options"] = json!({"include_usage":true});
    }
    if tools_enabled {
        body["tools"] = json!([{"type":"function","function":{"name":"write_note","description":"Write a long match analysis as a Markdown note in the local Analytics workspace.","parameters":{"type":"object","properties":{"title":{"type":"string"},"markdown":{"type":"string"}},"required":["title","markdown"],"additionalProperties":false},"strict":true}}]);
        body["tool_choice"] = json!("auto");
    }
    let mut headers = vec![("content-type".to_owned(), "application/json".to_owned())];
    if let Some(key) = input.key.filter(|key| !key.is_empty()) {
        let (name, prefix) = if let Some(provider) = super::registry::spec(input.provider) {
            (provider.auth_header, provider.auth_prefix)
        } else if input.provider.starts_with("custom-") {
            (Some("Authorization"), "Bearer ")
        } else {
            return Err("provider_not_allowed");
        };
        if let Some(name) = name {
            headers.push((name.to_owned(), format!("{prefix}{key}")));
        }
    }
    Ok(OutgoingRequest {
        headers,
        body: serde_json::to_vec(&body).map_err(|_| "request_serialization_failed")?,
    })
}

fn convert_history(history: &[Value]) -> Result<Vec<Value>, &'static str> {
    let mut result = Vec::new();
    for message in history {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .ok_or("history_invalid")?;
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .ok_or("history_invalid")?;
            result.push(json!({"role":role,"content":content}));
            continue;
        };
        if role == "assistant" {
            let mut text = String::new();
            let mut tool_calls = Vec::new();
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(value) = block.get("text").and_then(Value::as_str) {
                            text.push_str(value);
                        }
                    }
                    Some("tool_use") => {
                        let id = block
                            .get("id")
                            .and_then(Value::as_str)
                            .ok_or("history_invalid")?;
                        let name = block
                            .get("name")
                            .and_then(Value::as_str)
                            .ok_or("history_invalid")?;
                        let args = block.get("input").cloned().unwrap_or_else(|| json!({}));
                        tool_calls.push(json!({"id":id,"type":"function","function":{"name":name,"arguments":serde_json::to_string(&args).map_err(|_| "history_invalid")?}}));
                    }
                    _ => {} // Provider-specific reasoning blocks are not forwarded.
                }
            }
            let mut converted = json!({"role":"assistant","content":if text.is_empty() { Value::Null } else { json!(text) }});
            if !tool_calls.is_empty() {
                converted["tool_calls"] = json!(tool_calls);
            }
            result.push(converted);
        } else if role == "user" {
            let mut text = Vec::new();
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(value) = block.get("text").and_then(Value::as_str) {
                            text.push(value.to_owned());
                        }
                    }
                    Some("tool_result") => {
                        let tool_call_id = block
                            .get("tool_use_id")
                            .and_then(Value::as_str)
                            .ok_or("history_invalid")?;
                        let content = block.get("content").and_then(Value::as_str).unwrap_or("");
                        result.push(
                            json!({"role":"tool","tool_call_id":tool_call_id,"content":content}),
                        );
                    }
                    _ => {}
                }
            }
            if !text.is_empty() {
                result.push(json!({"role":"user","content":text.join("\n")}));
            }
        } else {
            return Err("history_invalid");
        }
    }
    Ok(result)
}

fn contains_secret(value: &Value, key: &str) -> bool {
    match value {
        Value::String(text) => text.contains(key),
        Value::Array(items) => items.iter().any(|item| contains_secret(item, key)),
        Value::Object(items) => items.values().any(|item| contains_secret(item, key)),
        _ => false,
    }
}

#[derive(Default)]
struct ToolAccumulator {
    id: String,
    name: String,
    arguments: String,
    started: bool,
}

#[derive(Default)]
struct OpenAiStreamDecoder {
    parser: SseParser,
    parsed: ParsedMessage,
    tools: BTreeMap<usize, ToolAccumulator>,
    raw: Vec<u8>,
    raw_mode: Option<bool>,
}

impl StreamDecoder for OpenAiStreamDecoder {
    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<NeutralEvent>, &'static str> {
        if self.raw.len().saturating_add(bytes.len()) > MAX_RESPONSE_BYTES {
            return Err("response_too_large");
        }
        self.raw.extend_from_slice(bytes);
        if self.raw_mode.is_none() {
            let first = self
                .raw
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace());
            if let Some(first) = first {
                self.raw_mode = Some(first == b'{');
            }
        }
        if self.raw_mode == Some(true) {
            return Ok(Vec::new());
        }
        let mut records = Vec::new();
        self.parser.feed(bytes, &mut records)?;
        Ok(self.accept(&records))
    }

    fn finish(&mut self) -> Result<Vec<NeutralEvent>, &'static str> {
        if self.raw_mode == Some(true) {
            let value =
                serde_json::from_slice::<Value>(&self.raw).map_err(|_| "invalid_stream_json")?;
            return self.accept_response(&value);
        }
        let mut records = Vec::new();
        self.parser.finish(&mut records)?;
        Ok(self.accept(&records))
    }

    fn parsed(&self) -> &ParsedMessage {
        &self.parsed
    }
}

impl OpenAiStreamDecoder {
    fn accept(&mut self, records: &[(Option<String>, String)]) -> Vec<NeutralEvent> {
        let mut events = Vec::new();
        for (_, data) in records {
            if data == "[DONE]" {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            if value.get("error").is_some() {
                events.push(NeutralEvent::StreamError {
                    kind: value
                        .pointer("/error/type")
                        .and_then(Value::as_str)
                        .unwrap_or("api_error")
                        .to_owned(),
                    message: value
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("provider stream error")
                        .to_owned(),
                });
                self.parsed.api_error = Some((
                    "api_error".into(),
                    value
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("provider stream error")
                        .into(),
                ));
                continue;
            }
            if let Some(usage) = normalize_usage(value.get("usage")) {
                self.parsed.usage = usage.clone();
                events.push(NeutralEvent::Usage(usage));
            }
            let choices = value
                .get("choices")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for choice in choices {
                let delta = choice.get("delta").unwrap_or(&Value::Null);
                if let Some(text) = delta.get("content").and_then(Value::as_str) {
                    if !text.is_empty() {
                        self.append_text(text);
                        events.push(NeutralEvent::TextDelta(text.to_owned()));
                    }
                }
                if let Some(tools) = delta.get("tool_calls").and_then(Value::as_array) {
                    for tool in tools {
                        self.accept_tool_delta(tool, &mut events);
                    }
                }
                if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                    let stop = stop_kind(reason);
                    self.parsed.stop_reason = Some(
                        match reason {
                            "tool_calls" => "tool_use",
                            "length" => "max_tokens",
                            "content_filter" => "refusal",
                            "stop" => "end_turn",
                            other => other,
                        }
                        .to_owned(),
                    );
                    events.push(NeutralEvent::Stop(stop));
                }
            }
        }
        events
    }

    fn accept_response(&mut self, value: &Value) -> Result<Vec<NeutralEvent>, &'static str> {
        let mut events = Vec::new();
        if let Some(usage) = normalize_usage(value.get("usage")) {
            self.parsed.usage = usage.clone();
            events.push(NeutralEvent::Usage(usage));
        }
        if value.get("error").is_some() {
            self.parsed.api_error = Some((
                value
                    .pointer("/error/type")
                    .and_then(Value::as_str)
                    .unwrap_or("api_error")
                    .to_owned(),
                value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("provider response error")
                    .to_owned(),
            ));
            return Ok(events);
        }
        for choice in value
            .get("choices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(text) = choice.pointer("/message/content").and_then(Value::as_str) {
                self.append_text(text);
                events.push(NeutralEvent::TextDelta(text.to_owned()));
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                self.parsed.stop_reason = Some(
                    match reason {
                        "tool_calls" => "tool_use",
                        "length" => "max_tokens",
                        "content_filter" => "refusal",
                        "stop" => "end_turn",
                        other => other,
                    }
                    .to_owned(),
                );
                events.push(NeutralEvent::Stop(stop_kind(reason)));
            }
        }
        Ok(events)
    }

    fn append_text(&mut self, text: &str) {
        if self.parsed.content.is_empty() {
            self.parsed.content.push(json!({"type":"text","text":""}));
        }
        if let Some(block) = self.parsed.content.first_mut() {
            let prior = block.get("text").and_then(Value::as_str).unwrap_or("");
            block["text"] = json!(format!("{prior}{text}"));
        }
        self.parsed.text_deltas.push(text.to_owned());
    }

    fn accept_tool_delta(&mut self, tool: &Value, events: &mut Vec<NeutralEvent>) {
        let Some(index) = tool
            .get("index")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
        else {
            return;
        };
        let entry = self.tools.entry(index).or_default();
        if let Some(id) = tool.get("id").and_then(Value::as_str) {
            entry.id.push_str(id);
        }
        if let Some(name) = tool.pointer("/function/name").and_then(Value::as_str) {
            entry.name.push_str(name);
        }
        if let Some(arguments) = tool.pointer("/function/arguments").and_then(Value::as_str) {
            entry.arguments.push_str(arguments);
            events.push(NeutralEvent::ToolCallArgsDelta {
                index,
                json: arguments.to_owned(),
            });
        }
        if !entry.started && !entry.name.is_empty() {
            entry.started = true;
            events.push(NeutralEvent::ToolCallStart {
                index,
                id: entry.id.clone(),
                name: entry.name.clone(),
            });
        }
        let slot = index
            + usize::from(
                self.parsed
                    .content
                    .first()
                    .and_then(|block| block.get("type"))
                    .and_then(Value::as_str)
                    == Some("text"),
            );
        while self.parsed.content.len() <= slot {
            self.parsed.content.push(Value::Null);
        }
        self.parsed.content[slot] = json!({"type":"tool_use","id":entry.id,"name":entry.name,"__partial_json":entry.arguments});
    }
}

fn normalize_usage(value: Option<&Value>) -> Option<Usage> {
    let value = value?;
    let prompt = value.get("prompt_tokens").and_then(Value::as_u64);
    let output = value
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .or_else(|| value.get("output_tokens").and_then(Value::as_u64));
    let cached = value
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(Value::as_u64)
        })
        .or_else(|| value.get("prompt_cache_hit_tokens").and_then(Value::as_u64));
    let cache_creation = value
        .pointer("/prompt_tokens_details/cache_write_tokens")
        .and_then(Value::as_u64);
    let input = match (prompt, cached) {
        (Some(prompt), Some(cached)) if cached <= prompt => Some(prompt - cached),
        (Some(_), Some(_)) => None,
        (Some(prompt), None) => Some(prompt),
        (None, _) => value.get("input_tokens").and_then(Value::as_u64),
    };
    let usage = Usage {
        input_tokens: input,
        cache_read_input_tokens: cached,
        cache_creation_input_tokens: cache_creation,
        output_tokens: output,
    };
    (usage != Usage::default()).then_some(usage)
}

fn stop_kind(value: &str) -> StopKind {
    match value {
        "tool_calls" | "function_call" => StopKind::ToolCall,
        "length" => StopKind::MaxTokens,
        "content_filter" => StopKind::Refusal,
        "stop" => StopKind::EndTurn,
        _ => StopKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        provider: &'a str,
        model: &'a str,
        history: &'a [Value],
        mode: ToolsMode,
    ) -> RequestInput<'a> {
        RequestInput {
            provider,
            model,
            key: Some("sk-test-secret-123456"),
            context: "{\"hero\":\"Axe\"}",
            question: Some("What next?"),
            scrubber_time_s: None,
            selected_hero: None,
            open_document: "",
            include_clock: false,
            include_selected_hero: false,
            include_document: false,
            history,
            tools_mode: mode,
        }
    }

    #[test]
    fn request_uses_provider_auth_header_and_never_puts_key_in_the_body() {
        let outgoing = OpenAiCompatAdapter
            .build(&input("openai", "gpt-test", &[], ToolsMode::Unknown))
            .unwrap();
        let body = String::from_utf8(outgoing.body).unwrap();
        assert!(body.contains("\"stream\":true"));
        assert!(body.contains("\"tools\":[{"));
        assert!(!body.contains("sk-test-secret"));
        assert!(outgoing.headers.contains(&(
            "Authorization".into(),
            "Bearer sk-test-secret-123456".into()
        )));
        let qwen = OpenAiCompatAdapter
            .build(&input("qwen", "qwen-model", &[], ToolsMode::Unknown))
            .unwrap();
        let qwen: Value = serde_json::from_slice(&qwen.body).unwrap();
        assert_eq!(qwen.get("stream").and_then(Value::as_bool), Some(false));
        assert!(qwen.get("tools").is_none());
    }

    #[test]
    fn every_compatible_registry_row_uses_only_the_minimal_request_shape() {
        for provider in [
            "openai",
            "openrouter",
            "gemini",
            "grok",
            "deepseek",
            "qwen",
            "ollama",
            "litellm",
            "custom-a1b2c3d4",
        ] {
            let outgoing = OpenAiCompatAdapter
                .build(&input(provider, "typed-model", &[], ToolsMode::Unknown))
                .unwrap();
            let body: Value = serde_json::from_slice(&outgoing.body).unwrap();
            assert_eq!(body["model"], "typed-model", "{provider}");
            assert!(body.get("messages").is_some(), "{provider}");
            assert!(body.get("temperature").is_none(), "{provider}");
            assert!(body.get("top_p").is_none(), "{provider}");
            assert!(!String::from_utf8(outgoing.body.clone())
                .unwrap()
                .contains("sk-test-secret"));
            assert!(outgoing.headers.contains(&(
                "Authorization".into(),
                "Bearer sk-test-secret-123456".into()
            )));
            assert_eq!(body["stream"], provider != "qwen");
            assert_eq!(body.get("tools").is_some(), provider != "qwen");
        }
    }

    #[test]
    fn unsupported_tools_mode_omits_tools_and_the_tool_instruction() {
        let outgoing = OpenAiCompatAdapter
            .build(&input("ollama", "local-model", &[], ToolsMode::Unsupported))
            .unwrap();
        let body: Value = serde_json::from_slice(&outgoing.body).unwrap();
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
        assert!(!body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("write_note"));
    }

    #[test]
    fn error_mapping_redacts_the_key_from_provider_bodies() {
        let error = OpenAiCompatAdapter.map_http_error(
            400,
            None,
            br#"{"error":{"message":"invalid key sk-test-secret-123456"}}"#,
            "sk-test-secret-123456",
        );
        let display = error.renderer_text();
        assert!(!display.contains("sk-test-secret-123456"));
        assert!(display.contains("[redacted]"));
    }

    #[test]
    fn conversation_conversion_omits_reasoning_and_maps_tool_round_trips() {
        let history = vec![
            json!({"role":"assistant","content":[{"type":"thinking","thinking":"private reasoning"},{"type":"text","text":"I will save"},{"type":"tool_use","id":"tool-1","name":"write_note","input":{"title":"Plan","markdown":"# Plan"}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"Note created"}]}),
        ];
        let mapped = convert_history(&history).unwrap();
        assert_eq!(mapped.len(), 2);
        assert_eq!(
            mapped[0]
                .pointer("/tool_calls/0/id")
                .and_then(Value::as_str),
            Some("tool-1")
        );
        assert_eq!(mapped[1].get("role").and_then(Value::as_str), Some("tool"));
        assert!(!serde_json::to_string(&mapped)
            .unwrap()
            .contains("private reasoning"));
    }

    #[test]
    fn stream_accumulates_split_tool_arguments_and_usage_without_choices() {
        let mut decoder = OpenAiStreamDecoder::default();
        let fixture = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"write_note\",\"arguments\":\"{\\\"title\\\":\\\"Plan\\\",\"}}]}}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2}}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"markdown\\\":\\\"# x\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        )
        .as_bytes();
        let mut events = Vec::new();
        for byte in fixture {
            events.extend(decoder.feed(&[*byte]).unwrap());
        }
        events.extend(decoder.finish().unwrap());
        let parsed = decoder.parsed().clone();
        assert_eq!(parsed.usage.input_tokens, Some(7));
        assert_eq!(parsed.usage.output_tokens, Some(2));
        assert_eq!(parsed.usage.cache_creation_input_tokens, None);
        assert!(events.iter().any(|event| matches!(event, NeutralEvent::ToolCallStart { name, .. } if name == "write_note")));
        let mut parsed = parsed;
        let tools = anthropic::finalize_tool_blocks(&mut parsed);
        assert_eq!(tools.len(), 1);
        let tool = tools.into_iter().next().unwrap().unwrap();
        assert_eq!(tool["input"]["title"], "Plan");
        assert_eq!(tool["input"]["markdown"], "# x");
    }

    #[test]
    fn stream_fixtures_cover_final_usage_errors_stops_and_every_byte_boundary() {
        let fixtures: &[(&[u8], Option<u64>, Option<u64>, Option<&str>)] = &[
            (
                b"data: {\"choices\":[{\"delta\":{\"content\":\"cached\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":5}}}\n\ndata: [DONE]\n\n",
                Some(7), Some(3), None,
            ),
            (
                b"data: {\"choices\":[{\"delta\":{\"content\":\"final usage\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":2}}\n\ndata: [DONE]\n\n",
                Some(8), Some(2), Some("end_turn"),
            ),
            (
                b": keepalive\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"no usage\"},\"finish_reason\":\"content_filter\"}]}\n\ndata: [DONE]\n\n",
                None, None, Some("refusal"),
            ),
            (
                b"data: {\"error\":{\"type\":\"provider_error\",\"message\":\"bad request\"}}\n\n",
                None, None, None,
            ),
            (
                b"data: {\"choices\":[{\"delta\":{\"content\":\"cut\"},\"finish_reason\":\"length\"}]}\n\n",
                None, None, Some("max_tokens"),
            ),
        ];
        for (fixture, input, output, stop) in fixtures {
            let mut decoder = OpenAiStreamDecoder::default();
            let mut events = Vec::new();
            for byte in *fixture {
                events.extend(decoder.feed(&[*byte]).unwrap());
            }
            events.extend(decoder.finish().unwrap());
            assert_eq!(decoder.parsed().usage.input_tokens, *input);
            assert_eq!(decoder.parsed().usage.output_tokens, *output);
            if let Some(stop) = stop {
                assert_eq!(decoder.parsed().stop_reason.as_deref(), Some(*stop));
            }
            if fixture
                .windows(b"\"error\"".len())
                .any(|window| window == b"\"error\"")
            {
                assert!(decoder.parsed().api_error.is_some());
            }
        }
        assert_eq!(
            normalize_usage(Some(&json!({
                "prompt_tokens": 4,
                "prompt_tokens_details": {"cached_tokens": 7},
                "completion_tokens": 1
            })))
            .unwrap()
            .input_tokens,
            None,
            "inconsistent usage stays unknown"
        );
        let deepseek = normalize_usage(Some(&json!({
            "prompt_tokens": 9,
            "prompt_cache_hit_tokens": 4,
            "prompt_cache_miss_tokens": 5,
            "completion_tokens": 2
        })))
        .unwrap();
        assert_eq!(deepseek.input_tokens, Some(5));
        assert_eq!(deepseek.cache_read_input_tokens, Some(4));
        assert_eq!(deepseek.cache_creation_input_tokens, None);
    }

    #[test]
    fn qwen_non_stream_json_response_is_decoded_without_fabricating_usage() {
        let mut decoder = OpenAiStreamDecoder::default();
        decoder
            .feed(br#"{"choices":[{"message":{"content":"hello"},"finish_reason":"stop"}]}"#)
            .unwrap();
        let events = decoder.finish().unwrap();
        assert!(events
            .iter()
            .any(|event| matches!(event, NeutralEvent::TextDelta(text) if text == "hello")));
        assert_eq!(decoder.parsed().usage.input_tokens, None);
        assert_eq!(decoder.parsed().usage.output_tokens, None);
    }

    #[test]
    fn missing_usage_stays_unknown_and_comments_are_ignored() {
        let mut decoder = OpenAiStreamDecoder::default();
        let mut events = decoder
            .feed(b": keepalive\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")
            .unwrap();
        events.extend(decoder.finish().unwrap());
        assert!(events
            .iter()
            .any(|event| matches!(event, NeutralEvent::TextDelta(text) if text == "hello")));
        assert_eq!(decoder.parsed().usage.input_tokens, None);
        assert_eq!(decoder.parsed().usage.output_tokens, None);
    }
}
