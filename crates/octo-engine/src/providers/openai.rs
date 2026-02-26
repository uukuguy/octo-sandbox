use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::stream::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, trace};

use octo_types::{
    ChatMessage, CompletionRequest, CompletionResponse, ContentBlock, MessageRole, StopReason,
    StreamEvent, TokenUsage, ToolSpec,
};

use super::traits::{CompletionStream, Provider};

pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl OpenAIProvider {
    pub fn new(api_key: String) -> Self {
        Self::with_base_url(api_key, "https://api.openai.com".into())
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");

        // Normalize: strip trailing slash, then strip /v1 suffix if present
        // so we always store the root URL without /v1
        let base_url = base_url.trim_end_matches('/').to_string();
        let base_url = if base_url.ends_with("/v1") {
            base_url[..base_url.len() - 3].to_string()
        } else {
            base_url
        };

        Self {
            api_key,
            base_url,
            client,
        }
    }
}

// --- Request serialization types (OpenAI Chat Completions API format) ---

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ApiTool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<ApiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ApiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(untagged)]
enum ApiContent {
    Text(String),
}

#[derive(Serialize, Clone)]
struct ApiToolCall {
    id: String,
    #[serde(rename = "type")]
    type_: String,
    function: ApiToolCallFunction,
}

#[derive(Serialize, Clone)]
struct ApiToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct ApiTool {
    #[serde(rename = "type")]
    type_: String,
    function: ApiToolFunction,
}

#[derive(Serialize)]
struct ApiToolFunction {
    name: String,
    description: String,
    parameters: Value,
}

// --- Response deserialization types ---

#[derive(Deserialize)]
struct ApiResponse {
    id: String,
    choices: Vec<ApiChoice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct ApiChoice {
    message: ApiResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ApiResponseToolCall>>,
}

#[derive(Deserialize, Clone)]
struct ApiResponseToolCall {
    id: String,
    function: ApiResponseFunction,
}

#[derive(Deserialize, Clone)]
struct ApiResponseFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize, Default)]
struct ApiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

fn convert_messages(messages: &[ChatMessage], system: Option<&str>) -> Vec<ApiMessage> {
    let mut api_messages = Vec::new();

    // System message goes as the first message with role "system"
    if let Some(sys) = system {
        if !sys.is_empty() {
            api_messages.push(ApiMessage {
                role: "system".into(),
                content: Some(ApiContent::Text(sys.to_string())),
                tool_calls: None,
                tool_call_id: None,
            });
        }
    }

    for msg in messages {
        if msg.role == MessageRole::System {
            continue;
        }

        match msg.role {
            MessageRole::User => {
                // Check if this is a tool result message
                let has_tool_results = msg
                    .content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolResult { .. }));

                if has_tool_results {
                    // Each tool result becomes a separate "tool" role message
                    for block in &msg.content {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } = block
                        {
                            let output = if *is_error {
                                format!("Error: {content}")
                            } else {
                                content.clone()
                            };
                            api_messages.push(ApiMessage {
                                role: "tool".into(),
                                content: Some(ApiContent::Text(output)),
                                tool_calls: None,
                                tool_call_id: Some(tool_use_id.clone()),
                            });
                        }
                    }
                } else {
                    // Normal user message — concatenate text blocks
                    let text: String = msg
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    api_messages.push(ApiMessage {
                        role: "user".into(),
                        content: Some(ApiContent::Text(text)),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            }
            MessageRole::Assistant => {
                // Collect text content
                let text: String = msg
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                // Collect tool calls
                let tool_calls: Vec<ApiToolCall> = msg
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolUse { id, name, input } => Some(ApiToolCall {
                            id: id.clone(),
                            type_: "function".into(),
                            function: ApiToolCallFunction {
                                name: name.clone(),
                                arguments: input.to_string(),
                            },
                        }),
                        _ => None,
                    })
                    .collect();

                api_messages.push(ApiMessage {
                    role: "assistant".into(),
                    content: if text.is_empty() {
                        None
                    } else {
                        Some(ApiContent::Text(text))
                    },
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                    tool_call_id: None,
                });
            }
            MessageRole::System => unreachable!(),
        }
    }

    api_messages
}

fn convert_tools(tools: &[ToolSpec]) -> Vec<ApiTool> {
    tools
        .iter()
        .map(|t| ApiTool {
            type_: "function".into(),
            function: ApiToolFunction {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            },
        })
        .collect()
}

fn parse_finish_reason(s: &str) -> StopReason {
    match s {
        "stop" => StopReason::EndTurn,
        "tool_calls" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        "content_filter" => StopReason::EndTurn,
        _ => StopReason::EndTurn,
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn id(&self) -> &str {
        "openai"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let api_req = ApiRequest {
            model: request.model,
            messages: convert_messages(&request.messages, request.system.as_deref()),
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
            tools: convert_tools(&request.tools),
            stream: false,
            stream_options: None,
        };

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&api_req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("OpenAI API error {status}: {body}"));
        }

        let api_resp: ApiResponse = resp.json().await?;

        let choice = api_resp.choices.into_iter().next().ok_or_else(|| {
            anyhow!("OpenAI API returned empty choices")
        })?;

        let mut content = Vec::new();

        if let Some(text) = choice.message.content {
            if !text.is_empty() {
                content.push(ContentBlock::Text { text });
            }
        }

        if let Some(tool_calls) = choice.message.tool_calls {
            for tc in tool_calls {
                let input: Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Object(
                        serde_json::Map::new(),
                    ));
                content.push(ContentBlock::ToolUse {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                });
            }
        }

        let stop_reason = choice
            .finish_reason
            .map(|s| parse_finish_reason(&s));

        let usage = api_resp.usage.unwrap_or_default();

        Ok(CompletionResponse {
            id: api_resp.id,
            content,
            stop_reason,
            usage: TokenUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
            },
        })
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        let api_req = ApiRequest {
            model: request.model,
            messages: convert_messages(&request.messages, request.system.as_deref()),
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
            tools: convert_tools(&request.tools),
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
        };

        let url = format!("{}/v1/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&api_req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("OpenAI API error {status}: {body}"));
        }

        let byte_stream = resp.bytes_stream();
        Ok(Box::pin(OpenAISseStream::new(byte_stream)))
    }
}

// --- SSE Stream parser for OpenAI format ---

struct OpenAISseStream<S> {
    inner: S,
    buffer: String,
    /// Parsed events waiting to be yielded (fixes multi-event-per-chunk data loss)
    pending_events: VecDeque<Result<StreamEvent>>,
    /// Track tool call accumulators by index
    tool_calls: Vec<ToolCallAccum>,
    /// Track message id from first chunk
    message_id: Option<String>,
    /// Final usage
    final_usage: Option<TokenUsage>,
    /// Whether we've sent MessageStart
    started: bool,
    /// Whether we've already sent MessageStop (avoid duplicates)
    stopped: bool,
    finished: bool,
}

struct ToolCallAccum {
    index: usize,
    id: String,
    name: String,
    arguments: String,
}

impl<S> OpenAISseStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
            pending_events: VecDeque::new(),
            tool_calls: Vec::new(),
            message_id: None,
            final_usage: None,
            started: false,
            stopped: false,
            finished: false,
        }
    }
}

impl<S> OpenAISseStream<S> {
    fn parse_sse_events(&mut self) -> Vec<Result<StreamEvent>> {
        let mut events = Vec::new();

        loop {
            // Find next data line
            let boundary = if let Some(pos) = self.buffer.find("\n\n") {
                pos
            } else if let Some(pos) = self.buffer.find("\r\n\r\n") {
                pos
            } else {
                break;
            };

            let raw_event = self.buffer[..boundary].to_string();
            // Skip past the double newline (handle both \n\n and \r\n\r\n)
            let skip = if self.buffer[boundary..].starts_with("\r\n\r\n") {
                4
            } else {
                2
            };
            self.buffer = self.buffer[boundary + skip..].to_string();

            // Extract data lines
            let mut data_lines = Vec::new();
            for line in raw_event.lines() {
                if let Some(d) = line.strip_prefix("data: ") {
                    data_lines.push(d.to_string());
                } else if line.starts_with("data:") {
                    // Handle "data:" with no space
                    data_lines.push(line[5..].to_string());
                }
            }

            if data_lines.is_empty() {
                continue;
            }

            let data = data_lines.join("\n");

            // [DONE] marker
            if data.trim() == "[DONE]" {
                // Emit any pending tool calls as ToolUseComplete
                let has_pending_tools = !self.tool_calls.is_empty();
                let pending: Vec<ToolCallAccum> = self.tool_calls.drain(..).collect();
                for tc in pending {
                    let input: Value =
                        serde_json::from_str(&tc.arguments).unwrap_or(Value::Object(
                            serde_json::Map::new(),
                        ));
                    events.push(Ok(StreamEvent::ToolUseComplete {
                        index: tc.index,
                        id: tc.id,
                        name: tc.name,
                        input,
                    }));
                }

                // Only emit MessageStop if we haven't already
                if !self.stopped {
                    let stop_reason = if has_pending_tools {
                        StopReason::ToolUse
                    } else {
                        StopReason::EndTurn
                    };

                    events.push(Ok(StreamEvent::MessageStop {
                        stop_reason,
                        usage: self.final_usage.clone().unwrap_or_default(),
                    }));
                    self.stopped = true;
                }

                self.finished = true;
                break;
            }

            // Parse JSON chunk
            trace!("SSE chunk: {data}");
            let chunk: Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(e) => {
                    debug!("Failed to parse SSE chunk: {e}");
                    continue;
                }
            };

            // Extract message id and emit MessageStart on first chunk
            if !self.started {
                let id = chunk["id"].as_str().unwrap_or("").to_string();
                self.message_id = Some(id.clone());
                events.push(Ok(StreamEvent::MessageStart { id }));
                self.started = true;
            }

            // Handle usage (stream_options.include_usage sends usage in last chunk)
            if let Some(usage) = chunk["usage"].as_object() {
                let prompt_tokens = usage
                    .get("prompt_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let completion_tokens = usage
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                self.final_usage = Some(TokenUsage {
                    input_tokens: prompt_tokens,
                    output_tokens: completion_tokens,
                });
            }

            // Process choices
            let choices = match chunk["choices"].as_array() {
                Some(c) => c,
                None => continue,
            };

            for choice in choices {
                let delta = &choice["delta"];

                // Parse finish_reason carefully — some providers send "null" string
                let finish_reason = match &choice["finish_reason"] {
                    Value::String(s) if s != "null" => Some(s.as_str()),
                    _ => None,
                };

                // Text content delta
                if let Some(content) = delta["content"].as_str() {
                    if !content.is_empty() {
                        events.push(Ok(StreamEvent::TextDelta {
                            text: content.to_string(),
                        }));
                    }
                }

                // Reasoning/thinking content (Qwen, DeepSeek, o1 etc.)
                if let Some(reasoning) = delta["reasoning_content"].as_str() {
                    if !reasoning.is_empty() {
                        events.push(Ok(StreamEvent::ThinkingDelta {
                            text: reasoning.to_string(),
                        }));
                    }
                }

                // Tool calls delta
                if let Some(tool_calls) = delta["tool_calls"].as_array() {
                    for tc in tool_calls {
                        let index = tc["index"].as_u64().unwrap_or(0) as usize;

                        // Check if this is a new tool call (has id and function.name)
                        let tc_id = tc["id"].as_str().unwrap_or("");
                        let func_name = tc["function"]["name"].as_str().unwrap_or("");
                        let func_args = tc["function"]["arguments"].as_str().unwrap_or("");

                        if !tc_id.is_empty() && !func_name.is_empty() {
                            // New tool call start
                            self.tool_calls.push(ToolCallAccum {
                                index,
                                id: tc_id.to_string(),
                                name: func_name.to_string(),
                                arguments: func_args.to_string(),
                            });
                            events.push(Ok(StreamEvent::ToolUseStart {
                                index,
                                id: tc_id.to_string(),
                                name: func_name.to_string(),
                            }));
                            if !func_args.is_empty() {
                                events.push(Ok(StreamEvent::ToolUseInputDelta {
                                    index,
                                    partial_json: func_args.to_string(),
                                }));
                            }
                        } else if !func_args.is_empty() {
                            // Argument delta for existing tool call
                            if let Some(accum) =
                                self.tool_calls.iter_mut().find(|a| a.index == index)
                            {
                                accum.arguments.push_str(func_args);
                            }
                            events.push(Ok(StreamEvent::ToolUseInputDelta {
                                index,
                                partial_json: func_args.to_string(),
                            }));
                        }
                    }
                }

                // Handle finish_reason in the chunk
                if let Some(reason) = finish_reason {
                    debug!("finish_reason: {reason}");
                    if reason == "tool_calls" {
                        // Tool calls will be finalized on [DONE]
                    } else if !self.stopped {
                        // Normal stop — emit pending tool uses if any, then MessageStop
                        let pending: Vec<ToolCallAccum> = self.tool_calls.drain(..).collect();
                        for tc in pending {
                            let input: Value = serde_json::from_str(&tc.arguments)
                                .unwrap_or(Value::Object(serde_json::Map::new()));
                            events.push(Ok(StreamEvent::ToolUseComplete {
                                index: tc.index,
                                id: tc.id,
                                name: tc.name,
                                input,
                            }));
                        }
                        events.push(Ok(StreamEvent::MessageStop {
                            stop_reason: parse_finish_reason(reason),
                            usage: self.final_usage.clone().unwrap_or_default(),
                        }));
                        self.stopped = true;
                    }
                }
            }
        }

        events
    }
}

impl<S> Stream for OpenAISseStream<S>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<StreamEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // Drain any previously parsed events first
        if let Some(event) = this.pending_events.pop_front() {
            return Poll::Ready(Some(event));
        }

        if this.finished {
            return Poll::Ready(None);
        }

        loop {
            // Try to parse events from the buffer
            let parsed = this.parse_sse_events();
            if !parsed.is_empty() {
                // Enqueue all parsed events, then yield the first one
                this.pending_events.extend(parsed);
                if let Some(event) = this.pending_events.pop_front() {
                    return Poll::Ready(Some(event));
                }
            }

            // Poll the inner stream for more bytes
            let inner = Pin::new(&mut this.inner);
            match inner.poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        this.buffer.push_str(text);
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(anyhow!("Stream error: {e}"))));
                }
                Poll::Ready(None) => {
                    // Stream ended, flush remaining buffer
                    if !this.buffer.is_empty() {
                        this.buffer.push_str("\n\n");
                        let parsed = this.parse_sse_events();
                        this.pending_events.extend(parsed);
                    }
                    if let Some(event) = this.pending_events.pop_front() {
                        return Poll::Ready(Some(event));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

pub fn create_openai_provider(api_key: String, base_url: Option<String>) -> Box<dyn Provider> {
    match base_url {
        Some(url) => Box::new(OpenAIProvider::with_base_url(api_key, url)),
        None => Box::new(OpenAIProvider::new(api_key)),
    }
}
