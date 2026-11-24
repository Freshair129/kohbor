use serde_json::{json, Map, Value};

pub(crate) const ENDPOINT_HOST: &str = "api.anthropic.com";
pub(crate) const ENDPOINT_PATH: &str = "/v1/messages";
pub(crate) const API_VERSION: &str = "2026-10-21";
pub(crate) const BETA_FALLBACKS: &str = "server-side-fallback-2026-07-01";
pub(crate) const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MODELS: [&str; 3] = ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Model {
    Opus55,
    Sonnet55,
    Haiku55,
}
impl Model {
    pub(crate) fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "claude-opus-5-5" => Ok(Self::Opus55),
            "claude-sonnet-5-5" => Ok(Self::Sonnet55),
            "claude-haiku-5-5" => Ok(Self::Haiku55),
            _ => Err("model_not_allowed"),
        }
    }
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Opus55 => MODELS[0],
            Self::Sonnet55 => MODELS[1],
            Self::Haiku55 => MODELS[2],
        }
    }
    fn has_fallback(self) -> bool {
        !matches!(self, Self::Haiku55)
    }
}

pub(crate) struct RequestInput<'a> {
    pub(crate) key: &'a str,
    pub(crate) model: Model,
    pub(crate) rules: &'a str,
    pub(crate) digest: &'a str,
    pub(crate) session: &'a str,
    pub(crate) question: Option<&'a str>,
    pub(crate) volatile: &'a str,
    pub(crate) history: &'a [Value],
    pub(crate) tools: &'a [Value],
}

pub(crate) struct OutgoingRequest {
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

pub(crate) fn build_request(input: &RequestInput<'_>) -> Result<OutgoingRequest, &'static str> {
    if input.key.is_empty() || input.key.len() > 512 || !input.key.is_ascii() {
        return Err("key_invalid");
    }
    let clean = |value: &str| value.replace(input.key, "[redacted]");
    if input
        .history
        .iter()
        .any(|message| contains_secret(message, input.key))
    {
        return Err("history_contains_secret");
    }
    let mut messages = input.history.to_vec();
    if let Some(last) = messages.last_mut() {
        if let Some(blocks) = last.get_mut("content").and_then(Value::as_array_mut) {
            if let Some(block) = blocks.last_mut().and_then(Value::as_object_mut) {
                block.insert("cache_control".into(), json!({"type":"ephemeral"}));
            }
        }
    }
    if let Some(question) = input.question {
        let mut user = clean(input.volatile);
        if !user.is_empty() {
            user.push_str("\n\n");
        }
        user.push_str("Question:\n");
        user.push_str(&clean(question));
        messages.push(json!({"role":"user","content":user}));
    }
    let mut body = Map::new();
    body.insert("model".into(), json!(input.model.as_str()));
    body.insert("max_tokens".into(), json!(16000));
    body.insert("stream".into(), json!(true));
    body.insert(
        "system".into(),
        json!([
            {"type":"text","text":clean(input.rules)},
            {"type":"text","text":clean(input.digest),"cache_control":{"type":"ephemeral","ttl":"1h"}},
            {"type":"text","text":clean(input.session),"cache_control":{"type":"ephemeral"}}
        ]),
    );
    body.insert("messages".into(), Value::Array(messages));
    if !input.tools.is_empty() {
        let tools = input
            .tools
            .iter()
            .map(|tool| {
                let mut tool = tool.clone();
                if let Some(object) = tool.as_object_mut() {
                    object.insert("eager_input_streaming".into(), json!(true));
                }
                tool
            })
            .collect::<Vec<_>>();
        body.insert("tools".into(), json!(tools));
        body.insert("tool_choice".into(), json!({"type":"auto"}));
    }
    body.insert("output_config".into(), json!({"effort":"medium"}));
    let mut headers = vec![
        ("x-api-key".to_owned(), input.key.to_owned()),
        ("anthropic-version".to_owned(), API_VERSION.to_owned()),
        ("content-type".to_owned(), "application/json".to_owned()),
    ];
    if input.model.has_fallback() {
        body.insert("fallbacks".into(), json!("default"));
        headers.push(("anthropic-beta".to_owned(), BETA_FALLBACKS.to_owned()));
    }
    let body =
        serde_json::to_vec(&Value::Object(body)).map_err(|_| "request_serialization_failed")?;
    Ok(OutgoingRequest { headers, body })
}

fn contains_secret(value: &Value, key: &str) -> bool {
    match value {
        Value::String(text) => text.contains(key),
        Value::Array(items) => items.iter().any(|item| contains_secret(item, key)),
        Value::Object(items) => items.values().any(|item| contains_secret(item, key)),
        _ => false,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Usage {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) cache_read_input_tokens: Option<u64>,
    pub(crate) cache_creation_input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ParsedMessage {
    pub(crate) content: Vec<Value>,
    pub(crate) text_deltas: Vec<String>,
    pub(crate) stop_reason: Option<String>,
    pub(crate) stop_details: Option<Value>,
    pub(crate) usage: Usage,
    pub(crate) api_error: Option<(String, String)>,
}

#[derive(Default)]
pub(crate) struct SseParser {
    pending: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}
impl SseParser {
    pub(crate) fn feed(
        &mut self,
        bytes: &[u8],
        out: &mut Vec<(Option<String>, String)>,
    ) -> Result<(), &'static str> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > MAX_RESPONSE_BYTES {
            return Err("response_too_large");
        }
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line = self.pending.drain(..=end).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = std::str::from_utf8(&line).map_err(|_| "invalid_stream_utf8")?;
            self.line(line, out);
        }
        Ok(())
    }
    pub(crate) fn finish(
        &mut self,
        out: &mut Vec<(Option<String>, String)>,
    ) -> Result<(), &'static str> {
        if !self.pending.is_empty() {
            let line = std::str::from_utf8(&self.pending)
                .map_err(|_| "invalid_stream_utf8")?
                .to_owned();
            self.pending.clear();
            self.line(&line, out);
        }
        if !self.data.is_empty() {
            out.push((self.event.take(), self.data.join("\n")));
            self.data.clear();
        }
        Ok(())
    }
    fn line(&mut self, line: &str, out: &mut Vec<(Option<String>, String)>) {
        if line.is_empty() {
            if !self.data.is_empty() {
                out.push((self.event.take(), self.data.join("\n")));
                self.data.clear();
            }
        } else if let Some(value) = line.strip_prefix("event:") {
            self.event = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            self.data
                .push(value.strip_prefix(' ').unwrap_or(value).to_owned());
        }
    }
}

fn token(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

pub(crate) fn parse_events(
    events: &[(Option<String>, String)],
    on_text: &mut dyn FnMut(&str),
) -> ParsedMessage {
    let mut parsed = ParsedMessage::default();
    for (event_name, data) in events {
        if event_name.as_deref() == Some("ping") || data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        let event = event_name
            .as_deref()
            .or_else(|| value.get("type").and_then(Value::as_str))
            .unwrap_or("");
        match event {
            "message_start" => {
                if let Some(usage) = value.pointer("/message/usage") {
                    parsed.usage.input_tokens = token(usage, "input_tokens");
                    parsed.usage.cache_read_input_tokens = token(usage, "cache_read_input_tokens");
                    parsed.usage.cache_creation_input_tokens =
                        token(usage, "cache_creation_input_tokens");
                }
            }
            "content_block_start" => {
                if let (Some(index), Some(block)) = (
                    value.get("index").and_then(Value::as_u64),
                    value.get("content_block"),
                ) {
                    let index = index as usize;
                    if parsed.content.len() <= index {
                        parsed.content.resize(index + 1, Value::Null);
                    }
                    parsed.content[index] = block.clone();
                }
            }
            "content_block_delta" => {
                let Some(index) = value
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|v| v as usize)
                else {
                    continue;
                };
                if parsed.content.len() <= index {
                    parsed.content.resize(index + 1, Value::Null);
                }
                let delta = value.get("delta").unwrap_or(&Value::Null);
                match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text_delta" => {
                        if let Some(text) = delta.get("text").and_then(Value::as_str) {
                            let current = parsed.content[index]
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            if parsed.content[index].is_null() {
                                parsed.content[index] = json!({"type":"text","text":""});
                            }
                            if let Some(object) = parsed.content[index].as_object_mut() {
                                object.insert("text".into(), json!(format!("{current}{text}")));
                            }
                            parsed.text_deltas.push(text.to_owned());
                            on_text(text);
                        }
                    }
                    "thinking_delta" => {
                        if let Some(thinking) = delta.get("thinking").and_then(Value::as_str) {
                            append_string(
                                &mut parsed.content[index],
                                "thinking",
                                thinking,
                                "thinking",
                            );
                        }
                    }
                    "signature_delta" => {
                        if let Some(signature) = delta.get("signature").and_then(Value::as_str) {
                            append_string(
                                &mut parsed.content[index],
                                "signature",
                                signature,
                                "thinking",
                            );
                        }
                    }
                    "input_json_delta" => {
                        if let Some(partial) = delta.get("partial_json").and_then(Value::as_str) {
                            append_string(
                                &mut parsed.content[index],
                                "__partial_json",
                                partial,
                                "tool_use",
                            );
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {}
            "message_delta" => {
                if let Some(delta) = value.get("delta") {
                    parsed.stop_reason = delta
                        .get("stop_reason")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    parsed.stop_details = delta
                        .get("stop_details")
                        .filter(|item| !item.is_null())
                        .cloned();
                }
                if let Some(usage) = value.get("usage") {
                    parsed.usage.output_tokens = token(usage, "output_tokens");
                }
            }
            "message_stop" => {}
            "error" => {
                let kind = value
                    .pointer("/error/type")
                    .and_then(Value::as_str)
                    .unwrap_or("api_error")
                    .to_owned();
                let message = value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Anthropic stream error")
                    .to_owned();
                parsed.api_error = Some((kind, message));
            }
            _ => {}
        }
    }
    parsed
}

fn append_string(block: &mut Value, field: &str, value: &str, block_type: &str) {
    if block.is_null() {
        *block = json!({"type":block_type});
    }
    let old = block
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if let Some(object) = block.as_object_mut() {
        object.insert(field.to_owned(), json!(format!("{old}{value}")));
    }
}

pub(crate) fn finalize_tool_blocks(
    message: &mut ParsedMessage,
) -> Vec<Result<Value, &'static str>> {
    let mut tools = Vec::new();
    for block in &mut message.content {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let Some(object) = block.as_object_mut() else {
            tools.push(Err("invalid_tool_block"));
            continue;
        };
        let partial = object
            .remove("__partial_json")
            .and_then(|value| value.as_str().map(str::to_owned));
        if let Some(raw) = partial {
            match serde_json::from_str::<Value>(&raw) {
                Ok(input) => {
                    object.insert("input".into(), input);
                    tools.push(Ok(block.clone()));
                }
                Err(_) => {
                    tools.push(Err("invalid_tool_json"));
                }
            }
        } else if object.get("input").is_some_and(Value::is_object) {
            tools.push(Ok(block.clone()));
        } else {
            tools.push(Err("invalid_tool_json"));
        }
    }
    tools
}

pub(crate) fn valid_write_note_input(input: &Value) -> Result<(&str, &str), &'static str> {
    let Some(object) = input.as_object() else {
        return Err("invalid_tool_schema");
    };
    if object.len() != 2 || !object.contains_key("title") || !object.contains_key("markdown") {
        return Err("invalid_tool_schema");
    }
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .ok_or("invalid_tool_schema")?;
    let markdown = object
        .get("markdown")
        .and_then(Value::as_str)
        .ok_or("invalid_tool_schema")?;
    if title.trim().is_empty() || title.len() > 240 || markdown.len() > 64 * 1024 {
        return Err("invalid_tool_schema");
    }
    Ok((title, markdown))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ApiError {
    InvalidKey,
    Permission,
    BadRequest(String),
    Billing,
    NotFound,
    RateLimitSpendCap,
    RateLimitWait { retry_after_s: Option<u64> },
    Overloaded,
    LocalProgramUnavailable,
    NetworkTls,
    Cancelled,
    Refusal,
    MaxTokens,
    Stream(String),
}

impl ApiError {
    pub(crate) fn renderer_text(&self) -> String {
        match self {
            Self::InvalidKey => "API key ไม่ถูกต้อง · Provider API key is invalid".into(),
            Self::Permission => "คีย์นี้ไม่มีสิทธิ์ใช้ API · This key lacks API permission".into(),
            Self::BadRequest(message) => format!("คำขอไม่ถูกต้อง · Bad request: {message}"),
            Self::Billing => {
                "บัญชีผู้ให้บริการต้องดำเนินการเรื่องการชำระเงิน · Provider billing needs attention".into()
            }
            Self::NotFound => "ไม่พบโมเดลหรือปลายทาง API · Model or API endpoint not found".into(),
            Self::RateLimitSpendCap => "ถึงขีดจำกัดการใช้งานของบัญชี · Account usage cap reached".into(),
            Self::RateLimitWait {
                retry_after_s: Some(seconds),
            } => format!("จำกัดอัตราการเรียก รอ {seconds} วินาที · Rate limited; wait {seconds}s"),
            Self::RateLimitWait {
                retry_after_s: None,
            } => "จำกัดอัตราการเรียก โปรดลองภายหลัง · Rate limited; try again later".into(),
            Self::Overloaded => "ผู้ให้บริการรับคำขอมากเกินไป · Provider is overloaded".into(),
            Self::LocalProgramUnavailable => {
                "โปรแกรมโมเดลในเครื่องไม่พร้อม · Local model program is unavailable".into()
            }
            Self::NetworkTls => {
                "เชื่อมต่อผู้ให้บริการไม่สำเร็จ · Provider network or TLS connection failed".into()
            }
            Self::Cancelled => "ยกเลิกคำขอแล้ว · Request cancelled".into(),
            Self::Refusal => "ผู้ช่วยปฏิเสธคำขอนี้ · The assistant refused this request".into(),
            Self::MaxTokens => "คำตอบถึงขีดจำกัดโทเคน · The answer reached its token limit".into(),
            Self::Stream(message) => format!("สตรีมคำตอบผิดพลาด · Stream error: {message}"),
        }
    }
}

pub(crate) fn map_http_error(
    status: u16,
    retry_after: Option<&str>,
    body: &[u8],
    key: &str,
) -> ApiError {
    match status {
        401 => ApiError::InvalidKey,
        403 => ApiError::Permission,
        400 => {
            let raw = serde_json::from_slice::<Value>(body)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "API rejected the request".to_owned());
            let safe = if key.is_empty() {
                raw
            } else {
                raw.replace(key, "[redacted]")
            };
            ApiError::BadRequest(safe.chars().take(600).collect())
        }
        402 => ApiError::Billing,
        404 => ApiError::NotFound,
        429 => match retry_after.and_then(parse_retry_after_seconds) {
            Some(seconds) => ApiError::RateLimitWait {
                retry_after_s: Some(seconds),
            },
            None if retry_after.is_some() => ApiError::RateLimitWait {
                retry_after_s: None,
            },
            None => ApiError::RateLimitSpendCap,
        },
        500 | 529 => ApiError::Overloaded,
        _ => ApiError::Stream(format!("HTTP {status}")),
    }
}

pub(crate) fn retry_delay_ms(error: &ApiError, attempt: u8) -> Option<u64> {
    if attempt > 0 {
        return None;
    }
    match error {
        ApiError::RateLimitWait {
            retry_after_s: Some(seconds),
        } if *seconds <= 20 => Some(seconds.saturating_mul(1000)),
        ApiError::Overloaded => Some(500),
        _ => None,
    }
}

fn parse_retry_after_seconds(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(seconds);
    }
    // HTTP-date (IMF-fixdate), parsed without a date dependency. Past dates become zero.
    let fields = value.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() != 6 || !fields[5].eq_ignore_ascii_case("GMT") {
        return None;
    }
    let day = fields[1].parse::<u64>().ok()?;
    let month = match fields[2] {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year = fields[3].parse::<i64>().ok()?;
    let mut clock = fields[4].split(':');
    let hour = clock.next()?.parse::<i64>().ok()?;
    let minute = clock.next()?.parse::<i64>().ok()?;
    let second = clock.next()?.parse::<i64>().ok()?;
    if clock.next().is_some() || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60
    {
        return None;
    }
    let days = days_from_civil(year, month, day as i64);
    let target = days
        .saturating_mul(86400)
        .saturating_add(hour * 3600 + minute * 60 + second);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some(target.saturating_sub(now).max(0) as u64)
}

fn days_from_civil(year: i64, month: u64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146097 + day_of_era - 719468
}

pub(crate) struct AnthropicAdapter;

impl<'a> super::WireAdapter<RequestInput<'a>> for AnthropicAdapter {
    fn build(&self, input: &RequestInput<'a>) -> Result<OutgoingRequest, &'static str> {
        build_request(input)
    }
    fn new_stream(&self) -> Box<dyn super::StreamDecoder> {
        Box::new(AnthropicStreamDecoder::default())
    }
    fn map_http_error(
        &self,
        status: u16,
        retry_after: Option<&str>,
        body: &[u8],
        key: &str,
    ) -> ApiError {
        map_http_error(status, retry_after, body, key)
    }
}

#[derive(Default)]
struct AnthropicStreamDecoder {
    parser: SseParser,
    parsed: ParsedMessage,
}

impl super::StreamDecoder for AnthropicStreamDecoder {
    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<super::NeutralEvent>, &'static str> {
        let mut records = Vec::new();
        self.parser.feed(bytes, &mut records)?;
        Ok(self.accept(&records))
    }

    fn finish(&mut self) -> Result<Vec<super::NeutralEvent>, &'static str> {
        let mut records = Vec::new();
        self.parser.finish(&mut records)?;
        Ok(self.accept(&records))
    }

    fn parsed(&self) -> &ParsedMessage {
        &self.parsed
    }
}

impl AnthropicStreamDecoder {
    fn accept(&mut self, records: &[(Option<String>, String)]) -> Vec<super::NeutralEvent> {
        let mut neutral = Vec::new();
        let partial = parse_events(records, &mut |text| {
            neutral.push(super::NeutralEvent::TextDelta(text.to_owned()))
        });
        for (event_name, data) in records {
            if event_name.as_deref() == Some("ping") || data == "[DONE]" {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            let event = event_name
                .as_deref()
                .or_else(|| value.get("type").and_then(Value::as_str))
                .unwrap_or("");
            match event {
                "content_block_start" => {
                    let Some(block) = value.get("content_block") else {
                        continue;
                    };
                    if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                        if let (Some(index), Some(id), Some(name)) = (
                            value.get("index").and_then(Value::as_u64),
                            block.get("id").and_then(Value::as_str),
                            block.get("name").and_then(Value::as_str),
                        ) {
                            neutral.push(super::NeutralEvent::ToolCallStart {
                                index: index as usize,
                                id: id.to_owned(),
                                name: name.to_owned(),
                            });
                        }
                    }
                }
                "content_block_delta" => {
                    let Some(index) = value
                        .get("index")
                        .and_then(Value::as_u64)
                        .map(|index| index as usize)
                    else {
                        continue;
                    };
                    if value.pointer("/delta/type").and_then(Value::as_str)
                        == Some("input_json_delta")
                    {
                        if let Some(json) =
                            value.pointer("/delta/partial_json").and_then(Value::as_str)
                        {
                            neutral.push(super::NeutralEvent::ToolCallArgsDelta {
                                index,
                                json: json.to_owned(),
                            });
                        }
                    }
                }
                "message_start" | "message_delta" => {
                    let usage = if event == "message_start" {
                        value.pointer("/message/usage")
                    } else {
                        value.get("usage")
                    };
                    if let Some(usage) = usage {
                        let normalized = Usage {
                            input_tokens: token(usage, "input_tokens"),
                            cache_read_input_tokens: token(usage, "cache_read_input_tokens"),
                            cache_creation_input_tokens: token(
                                usage,
                                "cache_creation_input_tokens",
                            ),
                            output_tokens: token(usage, "output_tokens"),
                        };
                        if normalized != Usage::default() {
                            neutral.push(super::NeutralEvent::Usage(normalized));
                        }
                    }
                    if event == "message_delta" {
                        if let Some(stop) =
                            value.pointer("/delta/stop_reason").and_then(Value::as_str)
                        {
                            neutral.push(super::NeutralEvent::Stop(stop_kind(stop)));
                        }
                    }
                }
                "error" => neutral.push(super::NeutralEvent::StreamError {
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
                }),
                _ => {}
            }
        }
        merge_parsed(&mut self.parsed, partial);
        neutral
    }
}

fn stop_kind(value: &str) -> super::StopKind {
    match value {
        "end_turn" => super::StopKind::EndTurn,
        "tool_use" => super::StopKind::ToolCall,
        "max_tokens" => super::StopKind::MaxTokens,
        "refusal" => super::StopKind::Refusal,
        _ => super::StopKind::Other,
    }
}

fn merge_parsed(target: &mut ParsedMessage, mut update: ParsedMessage) {
    for (index, incoming) in update.content.drain(..).enumerate() {
        if incoming.is_null() {
            continue;
        }
        if target.content.len() <= index {
            target.content.resize(index + 1, Value::Null);
        }
        let current = &mut target.content[index];
        if current.is_null() {
            *current = incoming;
            continue;
        }
        let Some(incoming) = incoming.as_object() else {
            *current = incoming;
            continue;
        };
        let Some(current) = current.as_object_mut() else {
            continue;
        };
        for key in ["type", "id", "name"] {
            if let Some(value) = incoming.get(key) {
                current.insert(key.to_owned(), value.clone());
            }
        }
        for key in ["text", "thinking", "signature", "__partial_json"] {
            if let Some(value) = incoming.get(key).and_then(Value::as_str) {
                let old = current.get(key).and_then(Value::as_str).unwrap_or("");
                current.insert(key.to_owned(), json!(format!("{old}{value}")));
            }
        }
        if let Some(value) = incoming.get("input") {
            current
                .entry("input".to_owned())
                .or_insert_with(|| value.clone());
        }
    }
    target.text_deltas.append(&mut update.text_deltas);
    if update.stop_reason.is_some() {
        target.stop_reason = update.stop_reason;
    }
    if update.stop_details.is_some() {
        target.stop_details = update.stop_details;
    }
    if update.usage.input_tokens.is_some() {
        target.usage.input_tokens = update.usage.input_tokens;
    }
    if update.usage.cache_read_input_tokens.is_some() {
        target.usage.cache_read_input_tokens = update.usage.cache_read_input_tokens;
    }
    if update.usage.cache_creation_input_tokens.is_some() {
        target.usage.cache_creation_input_tokens = update.usage.cache_creation_input_tokens;
    }
    if update.usage.output_tokens.is_some() {
        target.usage.output_tokens = update.usage.output_tokens;
    }
    if update.api_error.is_some() {
        target.api_error = update.api_error;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(model: Model) -> OutgoingRequest {
        build_request(&RequestInput {
            key: "sk-ant-private-marker",
            model,
            rules: "rules-v2",
            digest: "{\"duration_s\":42}",
            session: "{\"viewer\":null}",
            question: Some("Why?"),
            volatile: "Replay time: 00:45 (45 s). Selected hero: Axe.",
            history: &[],
            tools: &crate::analytics::tools::definitions(true),
        })
        .unwrap()
    }

    #[test]
    fn request_contract_has_three_system_blocks_and_one_hour_digest_cache() {
        for model in [Model::Opus55, Model::Sonnet55, Model::Haiku55] {
            let request = request(model);
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["model"], model.as_str());
            assert_eq!(body["max_tokens"], 16000);
            assert_eq!(body["stream"], true);
            assert_eq!(body["system"].as_array().unwrap().len(), 3);
            assert_eq!(body["system"][1]["cache_control"]["type"], "ephemeral");
            assert_eq!(body["system"][1]["cache_control"]["ttl"], "1h");
            assert_eq!(body["system"][2]["cache_control"]["type"], "ephemeral");
            assert!(!String::from_utf8_lossy(&request.body).contains("sk-ant-private-marker"));
            assert_eq!(body["tool_choice"]["type"], "auto");
            assert_eq!(body["output_config"]["effort"], "medium");
            assert!(body.get("thinking").is_none());
            for key in ["temperature", "top_p", "top_k"] {
                assert!(body.get(key).is_none());
            }
            assert!(body["system"][1]["text"]
                .as_str()
                .unwrap()
                .contains("duration_s"));
            assert!(body["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("Replay time: 00:45"));
            assert!(request
                .headers
                .contains(&("x-api-key".into(), "sk-ant-private-marker".into())));
            assert!(request
                .headers
                .contains(&("anthropic-version".into(), API_VERSION.into())));
            assert!(request
                .headers
                .contains(&("content-type".into(), "application/json".into())));
            if model.has_fallback() {
                assert_eq!(body["fallbacks"], "default");
                assert!(request
                    .headers
                    .iter()
                    .any(|(key, value)| key == "anthropic-beta" && value == BETA_FALLBACKS));
            } else {
                assert!(body.get("fallbacks").is_none());
                assert!(!request
                    .headers
                    .iter()
                    .any(|(key, _)| key == "anthropic-beta"));
            }
            assert_eq!(body["tools"].as_array().unwrap().len(), 10);
            assert!(body["tools"]
                .as_array()
                .unwrap()
                .iter()
                .all(|tool| tool["strict"] == true));
        }
        assert_eq!(Model::parse("claude-opus-5-5"), Ok(Model::Opus55));
        assert!(Model::parse("claude-opus-5-5-20261124").is_err());
    }

    #[test]
    fn anthropic_request_matches_the_v2_request_snapshot() {
        let request = build_request(&RequestInput {
            key: "golden-secret-value",
            model: Model::Haiku55,
            rules: "RULES v2 fixture",
            digest: "{\"duration_s\":1}",
            session: "{\"notes\":[]}",
            question: Some("why?"),
            volatile: "Replay time: 00:00 (0 s). Selected hero: — ไม่ทราบ.",
            history: &[],
            tools: &[],
        })
        .unwrap();
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["system"][0]["text"], "RULES v2 fixture");
        assert_eq!(body["system"][1]["text"], "{\"duration_s\":1}");
        assert_eq!(
            body["system"][1]["cache_control"],
            json!({"type":"ephemeral","ttl":"1h"})
        );
        assert_eq!(body["system"][2]["text"], "{\"notes\":[]}");
        assert!(body.get("tools").is_none());
        assert_eq!(
            body["messages"][0]["content"],
            "Replay time: 00:00 (0 s). Selected hero: — ไม่ทราบ.\n\nQuestion:\nwhy?"
        );
        assert_eq!(body["output_config"]["effort"], "medium");
        assert_eq!(
            request.headers,
            vec![
                ("x-api-key".to_owned(), "golden-secret-value".to_owned()),
                ("anthropic-version".to_owned(), API_VERSION.to_owned()),
                ("content-type".to_owned(), "application/json".to_owned()),
            ]
        );
        assert!(!String::from_utf8_lossy(&request.body).contains("golden-secret-value"));
    }

    #[test]
    fn fake_transport_confirms_cached_digest_read_usage() {
        use crate::analytics::providers::endpoint::Endpoint;
        use crate::analytics::transport::fake::FakeTransport;
        use crate::analytics::transport::{Cancellation, Transport};
        use std::sync::atomic::Ordering;

        let history =
            vec![json!({"role":"assistant","content":[{"type":"text","text":"Earlier answer"}]})];
        let request = build_request(&RequestInput {
            key: "fake-key-without-network",
            model: Model::Haiku55,
            rules: "RULES v2 fixture",
            digest: "{\"stable\":true}",
            session: "{\"viewer\":null}",
            question: Some("what changed?"),
            volatile: "Replay time: 01:00 (60 s). Selected hero: Axe.",
            history: &history,
            tools: &[],
        })
        .unwrap();
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["system"][1]["cache_control"]["ttl"], "1h");
        assert_eq!(
            body["messages"][0]["content"][0]["cache_control"]["type"],
            "ephemeral"
        );

        let chunks = vec![concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":12,\"cache_read_input_tokens\":23}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"cached\"}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        ).as_bytes().to_vec()];
        let fake = FakeTransport {
            chunks,
            ..Default::default()
        };
        let endpoint = Endpoint::parse("https://api.anthropic.com")
            .unwrap()
            .request_endpoint("/v1/messages");
        let mut records = Vec::new();
        let mut parser = SseParser::default();
        let response = fake
            .post(
                &endpoint,
                &request,
                &Cancellation::default(),
                &mut |_, chunk| {
                    parser
                        .feed(chunk, &mut records)
                        .expect("synthetic SSE is valid");
                },
            )
            .unwrap();
        assert_eq!(response.status, 200);
        parser
            .finish(&mut records)
            .expect("SSE has no partial record");
        let parsed = parse_events(&records, &mut |_| {});
        assert_eq!(parsed.usage.cache_read_input_tokens, Some(23));
        assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn sse_parser_handles_text_tools_refusal_max_tokens_errors_and_split_chunks() {
        let chunks = [b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":12,\"cache_read_input_tokens\":8}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n".as_slice(), b"event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tool1\",\"name\":\"write_note\",\"input\":{}}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"title\\\":\\\"A\\\",\\\"markdown\\\":\\\"B\\\"}\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_details\":null},\"usage\":{\"output_tokens\":7}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".as_slice()];
        let mut parser = SseParser::default();
        let mut events = Vec::new();
        for chunk in chunks {
            for part in chunk.chunks(13) {
                parser.feed(part, &mut events).unwrap();
            }
        }
        parser.finish(&mut events).unwrap();
        let mut shown = String::new();
        let mut parsed = parse_events(&events, &mut |text| shown.push_str(text));
        assert_eq!(shown, "hi");
        assert_eq!(parsed.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(parsed.usage.input_tokens, Some(12));
        assert_eq!(parsed.usage.cache_read_input_tokens, Some(8));
        assert_eq!(parsed.usage.output_tokens, Some(7));
        assert!(finalize_tool_blocks(&mut parsed)[0].is_ok());

        let refusal = vec![(
            Some("message_delta".into()),
            json!({"type":"message_delta","delta":{"stop_reason":"refusal","stop_details":null}})
                .to_string(),
        )];
        let refused = parse_events(&refusal, &mut |_| {});
        assert_eq!(refused.stop_reason.as_deref(), Some("refusal"));
        assert!(refused.stop_details.is_none());
        let max = vec![(
            Some("message_delta".into()),
            json!({"delta":{"stop_reason":"max_tokens"}}).to_string(),
        )];
        assert_eq!(
            parse_events(&max, &mut |_| {}).stop_reason.as_deref(),
            Some("max_tokens")
        );
        let error = vec![(
            Some("error".into()),
            json!({"type":"error","error":{"type":"overloaded_error","message":"busy"}})
                .to_string(),
        )];
        assert_eq!(
            parse_events(&error, &mut |_| {}).api_error,
            Some(("overloaded_error".into(), "busy".into()))
        );
    }

    #[test]
    fn malformed_tool_input_fails_schema_without_accepting_partial_json() {
        let mut message = ParsedMessage {
            content: vec![
                json!({"type":"tool_use","name":"write_note","id":"1","input":{},"__partial_json":"{bad"}),
            ],
            ..Default::default()
        };
        assert_eq!(
            finalize_tool_blocks(&mut message),
            vec![Err("invalid_tool_json")]
        );
        assert!(valid_write_note_input(&json!({"title":"ok","markdown":"# note"})).is_ok());
        for invalid in [
            json!({"title":"","markdown":"x"}),
            json!({"title":"x","markdown":"x","extra":1}),
            json!({"title":"x","markdown":"x".repeat(65537)}),
        ] {
            assert!(valid_write_note_input(&invalid).is_err());
        }
    }

    #[test]
    fn secret_in_existing_history_fails_closed_without_editing_earlier_turns() {
        let history = vec![json!({"role":"user","content":"sk-ant-private-marker"})];
        let before = history.clone();
        let result = build_request(&RequestInput {
            key: "sk-ant-private-marker",
            model: Model::Haiku55,
            rules: "rules",
            digest: "{}",
            session: "{}",
            question: Some("question"),
            volatile: "",
            history: &history,
            tools: &[],
        });
        assert!(matches!(result, Err("history_contains_secret")));
        assert_eq!(history, before);
    }

    #[test]
    fn http_error_mapping_and_retry_policy_are_distinct() {
        assert_eq!(
            map_http_error(401, None, b"", "secret"),
            ApiError::InvalidKey
        );
        assert_eq!(
            map_http_error(403, None, b"", "secret"),
            ApiError::Permission
        );
        assert_eq!(map_http_error(402, None, b"", "secret"), ApiError::Billing);
        assert_eq!(map_http_error(404, None, b"", "secret"), ApiError::NotFound);
        assert_eq!(
            map_http_error(429, None, b"", "secret"),
            ApiError::RateLimitSpendCap
        );
        assert_eq!(
            map_http_error(429, Some("20"), b"", "secret"),
            ApiError::RateLimitWait {
                retry_after_s: Some(20)
            }
        );
        assert_eq!(
            retry_delay_ms(
                &ApiError::RateLimitWait {
                    retry_after_s: Some(20)
                },
                0
            ),
            Some(20_000)
        );
        assert_eq!(
            retry_delay_ms(
                &ApiError::RateLimitWait {
                    retry_after_s: Some(21)
                },
                0
            ),
            None
        );
        assert_eq!(retry_delay_ms(&ApiError::Overloaded, 0), Some(500));
        assert_eq!(retry_delay_ms(&ApiError::Overloaded, 1), None);
        let bad = map_http_error(
            400,
            None,
            br#"{"error":{"message":"secret is malformed"}}"#,
            "secret",
        );
        assert_eq!(bad, ApiError::BadRequest("[redacted] is malformed".into()));
        assert_eq!(
            map_http_error(529, None, b"", "secret"),
            ApiError::Overloaded
        );
    }
}
