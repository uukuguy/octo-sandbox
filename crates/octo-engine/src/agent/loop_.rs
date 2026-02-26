use std::sync::Arc;

use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use octo_types::{
    ChatMessage, CompletionRequest, ContentBlock, MessageRole, SandboxId, SessionId,
    StopReason, StreamEvent, ToolContext, UserId,
};

use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::tools::ToolRegistry;

use super::context::ContextBuilder;

const MAX_ROUNDS: u32 = 10;
const MAX_RETRIES: u32 = 3;

/// Events sent from AgentLoop to consumers (WebSocket handler)
#[derive(Debug, Clone)]
pub enum AgentEvent {
    TextDelta {
        text: String,
    },
    TextComplete {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    ThinkingComplete {
        text: String,
    },
    ToolStart {
        tool_id: String,
        tool_name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_id: String,
        output: String,
        success: bool,
    },
    Error {
        message: String,
    },
    Done,
}

pub struct AgentLoop {
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<dyn WorkingMemory>,
    model: String,
    max_tokens: u32,
}

impl AgentLoop {
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
    ) -> Self {
        let model = match provider.id() {
            "openai" => "gpt-4o".into(),
            _ => "claude-sonnet-4-20250514".into(),
        };
        Self {
            provider,
            tools,
            memory,
            model,
            max_tokens: 4096,
        }
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub async fn run(
        &self,
        session_id: &SessionId,
        user_id: &UserId,
        sandbox_id: &SandboxId,
        messages: &mut Vec<ChatMessage>,
        tx: broadcast::Sender<AgentEvent>,
        tool_ctx: ToolContext,
    ) -> Result<()> {
        info!(
            session = %session_id,
            "AgentLoop starting, {} messages in history",
            messages.len()
        );

        // Build system prompt with working memory
        let memory_xml = self
            .memory
            .compile(user_id, sandbox_id)
            .await
            .unwrap_or_default();

        let system_prompt = ContextBuilder::new()
            .with_memory(memory_xml)
            .with_instructions(String::new())
            .build_system_prompt();

        debug!("System prompt length: {} chars", system_prompt.len());

        let tool_specs = self.tools.specs();

        for round in 0..MAX_ROUNDS {
            debug!(round, "Agent round starting");

            let request = CompletionRequest {
                model: self.model.clone(),
                system: Some(system_prompt.clone()),
                messages: messages.clone(),
                max_tokens: self.max_tokens,
                temperature: None,
                tools: tool_specs.clone(),
                stream: true,
            };

            // Retry on transient API errors (5xx)
            let mut stream = None;
            let mut last_err = None;
            for attempt in 0..MAX_RETRIES {
                match self.provider.stream(request.clone()).await {
                    Ok(s) => {
                        stream = Some(s);
                        break;
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        let is_retryable = err_str.contains("500")
                            || err_str.contains("502")
                            || err_str.contains("503")
                            || err_str.contains("520")
                            || err_str.contains("529");
                        if is_retryable && attempt < MAX_RETRIES - 1 {
                            let delay = std::time::Duration::from_millis(1000 * (attempt as u64 + 1));
                            warn!("API error (attempt {}), retrying in {:?}: {}", attempt + 1, delay, err_str);
                            tokio::time::sleep(delay).await;
                            last_err = Some(e);
                            continue;
                        }
                        last_err = Some(e);
                        break;
                    }
                }
            }

            let mut stream = match stream {
                Some(s) => s,
                None => {
                    let e = last_err.unwrap_or_else(|| anyhow::anyhow!("stream failed"));
                    let _ = tx.send(AgentEvent::Error {
                        message: e.to_string(),
                    });
                    let _ = tx.send(AgentEvent::Done);
                    return Err(e);
                }
            };

            let mut full_text = String::new();
            let mut full_thinking = String::new();
            let mut tool_uses: Vec<PendingToolUse> = Vec::new();
            let mut current_tool: Option<PendingToolUse> = None;

            while let Some(event) = stream.next().await {
                match event {
                    Ok(StreamEvent::MessageStart { .. }) => {}
                    Ok(StreamEvent::TextDelta { text }) => {
                        full_text.push_str(&text);
                        let _ = tx.send(AgentEvent::TextDelta { text });
                    }
                    Ok(StreamEvent::ThinkingDelta { text }) => {
                        full_thinking.push_str(&text);
                        let _ = tx.send(AgentEvent::ThinkingDelta { text });
                    }
                    Ok(StreamEvent::ToolUseStart { id, name, .. }) => {
                        current_tool = Some(PendingToolUse {
                            id,
                            name,
                            input_json: String::new(),
                        });
                    }
                    Ok(StreamEvent::ToolUseInputDelta { partial_json, .. }) => {
                        if let Some(ref mut tool) = current_tool {
                            tool.input_json.push_str(&partial_json);
                        }
                    }
                    Ok(StreamEvent::ToolUseComplete {
                        id, name, input, ..
                    }) => {
                        tool_uses.push(PendingToolUse {
                            id,
                            name,
                            input_json: input.to_string(),
                        });
                        current_tool = None;
                    }
                    Ok(StreamEvent::MessageStop { stop_reason, usage }) => {
                        debug!(
                            ?stop_reason,
                            input_tokens = usage.input_tokens,
                            output_tokens = usage.output_tokens,
                            "Message complete"
                        );

                        // If there's thinking but no text, the model put everything
                        // in thinking blocks (common with proxy/relay models like MiniMax).
                        // Fall back: treat thinking as the reply text.
                        if full_text.is_empty() && !full_thinking.is_empty() {
                            debug!("No text content, falling back thinking to text");
                            full_text = full_thinking.clone();
                            full_thinking.clear();
                        }

                        // Emit ThinkingComplete if there was thinking content
                        if !full_thinking.is_empty() {
                            let _ = tx.send(AgentEvent::ThinkingComplete {
                                text: full_thinking.clone(),
                            });
                            full_thinking.clear();
                        }

                        // If no tool uses, this is final response
                        if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
                            if !full_text.is_empty() {
                                let _ = tx.send(AgentEvent::TextComplete {
                                    text: full_text.clone(),
                                });
                            }
                            let _ = tx.send(AgentEvent::Done);
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        warn!("Stream error: {e}");
                        let _ = tx.send(AgentEvent::Error {
                            message: e.to_string(),
                        });
                        return Err(e);
                    }
                }
            }

            // If we have tool uses, execute them
            if tool_uses.is_empty() {
                // Stream ended without explicit MessageStop with tool_use
                if !full_text.is_empty() {
                    let _ = tx.send(AgentEvent::TextComplete {
                        text: full_text.clone(),
                    });
                }
                let _ = tx.send(AgentEvent::Done);
                return Ok(());
            }

            // Build assistant message with text + tool_use blocks
            let mut assistant_content: Vec<ContentBlock> = Vec::new();
            if !full_text.is_empty() {
                assistant_content.push(ContentBlock::Text {
                    text: full_text.clone(),
                });
            }
            for tu in &tool_uses {
                let input: serde_json::Value =
                    serde_json::from_str(&tu.input_json).unwrap_or_default();
                assistant_content.push(ContentBlock::ToolUse {
                    id: tu.id.clone(),
                    name: tu.name.clone(),
                    input,
                });
            }
            messages.push(ChatMessage {
                role: MessageRole::Assistant,
                content: assistant_content,
            });

            // Execute tools and build tool result messages
            let mut tool_results: Vec<ContentBlock> = Vec::new();
            for tu in &tool_uses {
                let input: serde_json::Value =
                    serde_json::from_str(&tu.input_json).unwrap_or_default();

                let _ = tx.send(AgentEvent::ToolStart {
                    tool_id: tu.id.clone(),
                    tool_name: tu.name.clone(),
                    input: input.clone(),
                });

                let result = if let Some(tool) = self.tools.get(&tu.name) {
                    match tool.execute(input, &tool_ctx).await {
                        Ok(r) => r,
                        Err(e) => octo_types::ToolResult::error(format!("Tool error: {e}")),
                    }
                } else {
                    octo_types::ToolResult::error(format!("Unknown tool: {}", tu.name))
                };

                let _ = tx.send(AgentEvent::ToolResult {
                    tool_id: tu.id.clone(),
                    output: result.output.clone(),
                    success: !result.is_error,
                });

                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tu.id.clone(),
                    content: result.output,
                    is_error: result.is_error,
                });
            }

            messages.push(ChatMessage {
                role: MessageRole::User,
                content: tool_results,
            });

            // Reset for next round
            full_text = String::new();
        }

        warn!("Max rounds ({MAX_ROUNDS}) exceeded");
        let _ = tx.send(AgentEvent::Error {
            message: format!("Max rounds ({MAX_ROUNDS}) exceeded"),
        });
        let _ = tx.send(AgentEvent::Done);
        Ok(())
    }
}

struct PendingToolUse {
    id: String,
    name: String,
    input_json: String,
}
