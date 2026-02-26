use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::stream::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

use octo_types::{
    ChatMessage, CompletionRequest, CompletionResponse, ContentBlock, MessageRole, StopReason,
    StreamEvent, TokenUsage, ToolSpec,
};

use super::traits::{CompletionStream, Provider};

pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        Self::with_base_url(api_key, "https://api.anthropic.com".into())
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");

        Self {
            api_key,
            base_url,
            client,
        }
    }
}

// --- Request serialization types (Anthropic Messages API format) ---

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ApiTool>,
    stream: bool,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: Vec<ApiContentBlock>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ApiContentBlock {
    Text {
        #[serde(rename = "type")]
        type_: String,
        text: String,
    },
    ToolUse {
        #[serde(rename = "type")]
        type_: String,
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        #[serde(rename = "type")]
        type_: String,
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
}

#[derive(Serialize)]
struct ApiTool {
    name: String,
    description: String,
    input_schema: Value,
}

// --- Response deserialization types ---

#[derive(Deserialize)]
struct ApiResponse {
    id: String,
    content: Vec<ApiResponseContent>,
    stop_reason: Option<String>,
    usage: ApiUsage,
}

#[derive(Deserialize)]
struct ApiResponseContent {
    #[serde(rename = "type")]
    type_: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<Value>,
}

#[derive(Deserialize, Default)]
struct ApiUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

fn convert_messages(messages: &[ChatMessage]) -> Vec<ApiMessage> {
    messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(|msg| {
            let role = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => unreachable!(),
            };
            let content = msg
                .content
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => ApiContentBlock::Text {
                        type_: "text".into(),
                        text: text.clone(),
                    },
                    ContentBlock::ToolUse { id, name, input } => ApiContentBlock::ToolUse {
                        type_: "tool_use".into(),
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => ApiContentBlock::ToolResult {
                        type_: "tool_result".into(),
                        tool_use_id: tool_use_id.clone(),
                        content: content.clone(),
                        is_error: *is_error,
                    },
                })
                .collect();
            ApiMessage {
                role: role.into(),
                content,
            }
        })
        .collect()
}

fn convert_tools(tools: &[ToolSpec]) -> Vec<ApiTool> {
    tools
        .iter()
        .map(|t| ApiTool {
            name: t.name.clone(),
            description: t.description.clone(),
            input_schema: t.input_schema.clone(),
        })
        .collect()
}

fn parse_stop_reason(s: &str) -> StopReason {
    match s {
        "end_turn" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        _ => StopReason::EndTurn,
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let api_req = ApiRequest {
            model: request.model,
            max_tokens: request.max_tokens,
            messages: convert_messages(&request.messages),
            system: request.system,
            temperature: request.temperature,
            tools: convert_tools(&request.tools),
            stream: false,
        };

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&api_req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Anthropic API error {status}: {body}"));
        }

        let api_resp: ApiResponse = resp.json().await?;

        let content = api_resp
            .content
            .into_iter()
            .map(|c| match c.type_.as_str() {
                "text" => ContentBlock::Text {
                    text: c.text.unwrap_or_default(),
                },
                "tool_use" => ContentBlock::ToolUse {
                    id: c.id.unwrap_or_default(),
                    name: c.name.unwrap_or_default(),
                    input: c.input.unwrap_or(Value::Object(Default::default())),
                },
                _ => ContentBlock::Text {
                    text: String::new(),
                },
            })
            .collect();

        let stop_reason = api_resp
            .stop_reason
            .map(|s| parse_stop_reason(&s));

        Ok(CompletionResponse {
            id: api_resp.id,
            content,
            stop_reason,
            usage: TokenUsage {
                input_tokens: api_resp.usage.input_tokens,
                output_tokens: api_resp.usage.output_tokens,
            },
        })
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        let api_req = ApiRequest {
            model: request.model,
            max_tokens: request.max_tokens,
            messages: convert_messages(&request.messages),
            system: request.system,
            temperature: request.temperature,
            tools: convert_tools(&request.tools),
            stream: true,
        };

        let url = format!("{}/v1/messages", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&api_req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Anthropic API error {status}: {body}"));
        }

        let byte_stream = resp.bytes_stream();
        Ok(Box::pin(SseStream::new(byte_stream)))
    }
}

// --- SSE Stream parser ---

struct SseStream<S> {
    inner: S,
    buffer: String,
    /// Parsed events waiting to be yielded (fixes multi-event-per-chunk data loss)
    pending_events: VecDeque<Result<StreamEvent>>,
    // Accumulate tool_use blocks across deltas
    tool_blocks: Vec<ToolBlockAccum>,
    // Track content block types by index
    block_types: Vec<(usize, BlockType)>,
    // Track content block index
    current_block_index: usize,
    // Final usage from message_delta
    final_usage: Option<TokenUsage>,
    finished: bool,
}

struct ToolBlockAccum {
    index: usize,
    id: String,
    name: String,
    input_json: String,
}

/// Track content block types to route deltas correctly
#[derive(Clone, Copy, PartialEq)]
enum BlockType {
    Text,
    Thinking,
    ToolUse,
}

impl<S> SseStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
            pending_events: VecDeque::new(),
            tool_blocks: Vec::new(),
            block_types: Vec::new(),
            current_block_index: 0,
            final_usage: None,
            finished: false,
        }
    }
}

impl<S> SseStream<S> {
    fn parse_sse_events(&mut self) -> Vec<Result<StreamEvent>> {
        let mut events = Vec::new();

        loop {
            // Find complete SSE event (ends with double newline)
            let boundary = if let Some(pos) = self.buffer.find("\n\n") {
                pos
            } else {
                break;
            };

            let raw_event = self.buffer[..boundary].to_string();
            self.buffer = self.buffer[boundary + 2..].to_string();

            let mut event_type = String::new();
            let mut data_lines = Vec::new();

            for line in raw_event.lines() {
                if let Some(et) = line.strip_prefix("event: ") {
                    event_type = et.trim().to_string();
                } else if let Some(d) = line.strip_prefix("data: ") {
                    data_lines.push(d.to_string());
                }
            }

            if data_lines.is_empty() {
                continue;
            }

            let data = data_lines.join("\n");

            match event_type.as_str() {
                "message_start" => {
                    if let Ok(v) = serde_json::from_str::<Value>(&data) {
                        let id = v["message"]["id"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        if let Some(usage) = v["message"]["usage"].as_object() {
                            let input_tokens = usage
                                .get("input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32;
                            self.final_usage = Some(TokenUsage {
                                input_tokens,
                                output_tokens: 0,
                            });
                        }
                        events.push(Ok(StreamEvent::MessageStart { id }));
                    }
                }
                "content_block_start" => {
                    if let Ok(v) = serde_json::from_str::<Value>(&data) {
                        let index = v["index"].as_u64().unwrap_or(0) as usize;
                        self.current_block_index = index;
                        let block_type = v["content_block"]["type"]
                            .as_str()
                            .unwrap_or("");

                        match block_type {
                            "tool_use" => {
                                let id = v["content_block"]["id"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                let name = v["content_block"]["name"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();

                                self.tool_blocks.push(ToolBlockAccum {
                                    index,
                                    id: id.clone(),
                                    name: name.clone(),
                                    input_json: String::new(),
                                });
                                self.block_types.push((index, BlockType::ToolUse));

                                events.push(Ok(StreamEvent::ToolUseStart {
                                    index,
                                    id,
                                    name,
                                }));
                            }
                            "thinking" => {
                                self.block_types.push((index, BlockType::Thinking));
                            }
                            "text" => {
                                self.block_types.push((index, BlockType::Text));
                            }
                            _ => {
                                debug!("Unknown content block type: {block_type}");
                            }
                        }
                    }
                }
                "content_block_delta" => {
                    if let Ok(v) = serde_json::from_str::<Value>(&data) {
                        let index = v["index"].as_u64().unwrap_or(0) as usize;
                        let delta_type = v["delta"]["type"].as_str().unwrap_or("");

                        match delta_type {
                            "text_delta" => {
                                let text = v["delta"]["text"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                if !text.is_empty() {
                                    events.push(Ok(StreamEvent::TextDelta { text }));
                                }
                            }
                            "thinking_delta" => {
                                let text = v["delta"]["thinking"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                if !text.is_empty() {
                                    events.push(Ok(StreamEvent::ThinkingDelta { text }));
                                }
                            }
                            "input_json_delta" => {
                                let partial = v["delta"]["partial_json"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                if let Some(block) =
                                    self.tool_blocks.iter_mut().find(|b| b.index == index)
                                {
                                    block.input_json.push_str(&partial);
                                }
                                events.push(Ok(StreamEvent::ToolUseInputDelta {
                                    index,
                                    partial_json: partial,
                                }));
                            }
                            "signature_delta" => {
                                // Ignore signature deltas (used by some proxy models)
                            }
                            _ => {
                                debug!("Unknown delta type: {delta_type}");
                            }
                        }
                    }
                }
                "content_block_stop" => {
                    if let Ok(v) = serde_json::from_str::<Value>(&data) {
                        let index = v["index"].as_u64().unwrap_or(0) as usize;

                        // If this is a tool_use block, emit ToolUseComplete
                        if let Some(pos) =
                            self.tool_blocks.iter().position(|b| b.index == index)
                        {
                            let block = self.tool_blocks.remove(pos);
                            let input: Value =
                                serde_json::from_str(&block.input_json).unwrap_or(Value::Object(
                                    serde_json::Map::new(),
                                ));
                            events.push(Ok(StreamEvent::ToolUseComplete {
                                index,
                                id: block.id,
                                name: block.name,
                                input,
                            }));
                        }
                    }
                }
                "message_delta" => {
                    if let Ok(v) = serde_json::from_str::<Value>(&data) {
                        let stop_reason = v["delta"]["stop_reason"]
                            .as_str()
                            .unwrap_or("end_turn");
                        let output_tokens = v["usage"]["output_tokens"]
                            .as_u64()
                            .unwrap_or(0) as u32;

                        if let Some(ref mut usage) = self.final_usage {
                            usage.output_tokens = output_tokens;
                        }

                        events.push(Ok(StreamEvent::MessageStop {
                            stop_reason: parse_stop_reason(stop_reason),
                            usage: self.final_usage.clone().unwrap_or_default(),
                        }));
                    }
                }
                "message_stop" => {
                    self.finished = true;
                }
                "ping" | "error" => {
                    if event_type == "error" {
                        warn!("SSE error event: {data}");
                    }
                }
                _ => {
                    debug!("Unknown SSE event type: {event_type}");
                }
            }
        }

        events
    }
}

impl<S> Stream for SseStream<S>
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
                    // Loop back to try parsing
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

pub fn create_provider(api_key: String, base_url: Option<String>) -> Box<dyn Provider> {
    match base_url {
        Some(url) => Box::new(AnthropicProvider::with_base_url(api_key, url)),
        None => Box::new(AnthropicProvider::new(api_key)),
    }
}
