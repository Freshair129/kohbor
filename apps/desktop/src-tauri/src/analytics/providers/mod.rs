pub(crate) mod anthropic;
pub(crate) mod endpoint;
pub(crate) mod openai_compat;
pub(crate) mod registry;
pub(crate) use anthropic::AnthropicAdapter;
pub(crate) use openai_compat::{OpenAiCompatAdapter, ToolsMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopKind {
    EndTurn,
    ToolCall,
    MaxTokens,
    Refusal,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NeutralEvent {
    TextDelta(String),
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolCallArgsDelta {
        index: usize,
        json: String,
    },
    Usage(anthropic::Usage),
    Stop(StopKind),
    StreamError {
        kind: String,
        message: String,
    },
}

pub(crate) trait StreamDecoder: Send {
    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<NeutralEvent>, &'static str>;
    fn finish(&mut self) -> Result<Vec<NeutralEvent>, &'static str>;
    fn parsed(&self) -> &anthropic::ParsedMessage;
}

pub(crate) trait WireAdapter<Input> {
    fn build(&self, input: &Input) -> Result<anthropic::OutgoingRequest, &'static str>;
    fn new_stream(&self) -> Box<dyn StreamDecoder>;
    fn map_http_error(
        &self,
        status: u16,
        retry_after: Option<&str>,
        body: &[u8],
        key: &str,
    ) -> anthropic::ApiError;
}
