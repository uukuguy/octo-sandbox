use std::sync::Arc;

use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use octo_types::{
    ChatMessage, CompletionRequest, ContentBlock, MessageRole, SandboxId, SessionId,
    StopReason, StreamEvent, ToolContext, ToolSource, UserId,
};

use crate::context::{ContextBudgetManager, ContextPruner, DegradationLevel, MemoryFlusher};
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::{LlmErrorKind, Provider, RetryPolicy};
use crate::tools::ToolRegistry;

use super::context::ContextBuilder;

const MAX_ROUNDS: u32 = 10;
const TOOL_RESULT_SOFT_LIMIT: usize = 30_000;

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
    ToolExecution {
        execution: octo_types::ToolExecution,
    },
    TokenBudgetUpdate {
        budget: octo_types::TokenBudgetSnapshot,
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
    memory_store: Option<Arc<dyn MemoryStore>>,
    model: String,
    max_tokens: u32,
    budget: ContextBudgetManager,
    pruner: ContextPruner,
    recorder: Option<Arc<crate::tools::recorder::ToolExecutionRecorder>>,
    loop_guard: super::loop_guard::LoopGuard,
    event_bus: Option<Arc<crate::event::EventBus>>,
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
            memory_store: None,
            model,
            max_tokens: 4096,
            budget: ContextBudgetManager::default(),
            pruner: ContextPruner::new(),
            recorder: None,
            loop_guard: super::loop_guard::LoopGuard::new(),
            event_bus: None,
        }
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<crate::event::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_recorder(mut self, recorder: Arc<crate::tools::recorder::ToolExecutionRecorder>) -> Self {
        self.recorder = Some(recorder);
        self
    }

    pub async fn run(
        &mut self,
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

            // Apply context pruning based on budget
            let level = self.budget.compute_degradation_level(
                &system_prompt,
                messages,
                &tool_specs,
            );
            if level != DegradationLevel::None {
                debug!(?level, "Applying context degradation");

                // At OverflowCompaction level: flush facts before pruning to prevent info loss
                if level >= DegradationLevel::OverflowCompaction && level != DegradationLevel::FinalError {
                    let boundary = ContextPruner::find_compaction_boundary(messages, 20_000);
                    if boundary > 0 {
                        let _ = MemoryFlusher::flush(
                            messages,
                            boundary,
                            &*self.provider,
                            &*self.memory,
                            self.memory_store.as_deref(),
                            &self.model,
                            user_id.as_str(),
                        )
                        .await;
                    }
                }

                self.pruner.apply(messages, level);
            }

            let request = CompletionRequest {
                model: self.model.clone(),
                system: Some(system_prompt.clone()),
                messages: messages.clone(),
                max_tokens: self.max_tokens,
                temperature: None,
                tools: tool_specs.clone(),
                stream: true,
            };

            // Retry on transient API errors using LlmErrorKind classification + exponential backoff
            let retry_policy = RetryPolicy::default();
            let mut stream = None;
            let mut last_err = None;
            let mut attempt = 0u32;
            loop {
                match self.provider.stream(request.clone()).await {
                    Ok(s) => {
                        stream = Some(s);
                        break;
                    }
                    Err(e) => {
                        let err_str = e.to_string().to_lowercase();
                        if retry_policy.should_retry_str(&err_str, attempt) {
                            let delay = retry_policy.delay_for(attempt);
                            let kind = LlmErrorKind::classify_from_str(&err_str);
                            warn!(
                                "LLM stream failed (attempt {}/{}, kind={:?}), retrying in {:?}: {}",
                                attempt + 1, retry_policy.max_retries, kind, delay, e
                            );
                            tokio::time::sleep(delay).await;
                            last_err = Some(e);
                            attempt += 1;
                            continue;
                        } else {
                            let kind = LlmErrorKind::classify_from_str(&err_str);
                            tracing::error!(
                                "LLM stream failed (non-retryable, kind={:?}): {}",
                                kind, e
                            );
                            last_err = Some(e);
                            break;
                        }
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

                        // Update budget with actual usage
                        self.budget.update_actual_usage(usage.input_tokens, messages.len());

                        // Emit budget snapshot to frontend
                        let snapshot = self.budget.snapshot(&system_prompt, messages, &tool_specs);
                        let _ = tx.send(AgentEvent::TokenBudgetUpdate { budget: snapshot });

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

                // Loop Guard: 检查是否陷入循环
                if let Some(violation) = self.loop_guard.record_call(&tu.name, &tu.input_json) {
                    tracing::warn!("Loop Guard triggered: {}", violation);
                    return Err(anyhow::anyhow!("Loop Guard: {}", violation));
                }

                let _ = tx.send(AgentEvent::ToolStart {
                    tool_id: tu.id.clone(),
                    tool_name: tu.name.clone(),
                    input: input.clone(),
                });
                if let Some(ref bus) = self.event_bus {
                    bus.publish(crate::event::OctoEvent::ToolCallStarted {
                        session_id: session_id.as_str().to_string(),
                        tool_name: tu.name.clone(),
                    }).await;
                }

                let exec_id = if let Some(ref recorder) = self.recorder {
                    let source = self.tools.get(&tu.name)
                        .map(|t| t.source())
                        .unwrap_or(ToolSource::BuiltIn);
                    recorder.record_start(
                        session_id.as_str(),
                        &tu.name,
                        &source,
                        &input,
                    ).await.ok()
                } else {
                    None
                };

                let started_at_ms = chrono::Utc::now().timestamp_millis();
                let exec_start = std::time::Instant::now();

                let result = if let Some(tool) = self.tools.get(&tu.name) {
                    match tool.execute(input.clone(), &tool_ctx).await {
                        Ok(r) => r,
                        Err(e) => octo_types::ToolResult::error(format!("Tool error: {e}")),
                    }
                } else {
                    octo_types::ToolResult::error(format!("Unknown tool: {}", tu.name))
                };

                let exec_duration = exec_start.elapsed().as_millis() as u64;
                if let Some(ref bus) = self.event_bus {
                    bus.publish(crate::event::OctoEvent::ToolCallCompleted {
                        session_id: session_id.as_str().to_string(),
                        tool_name: tu.name.clone(),
                        duration_ms: exec_duration,
                    }).await;
                }
                if let (Some(ref recorder), Some(ref eid)) = (&self.recorder, &exec_id) {
                    if result.is_error {
                        let _ = recorder.record_failed(eid, &result.output, exec_duration).await;
                    } else {
                        let output_val = serde_json::Value::String(result.output.clone());
                        let _ = recorder.record_complete(eid, &output_val, exec_duration).await;
                    }
                }

                if let Some(ref eid) = exec_id {
                    let exec = octo_types::ToolExecution {
                        id: eid.clone(),
                        session_id: session_id.as_str().to_string(),
                        tool_name: tu.name.clone(),
                        source: self.tools.get(&tu.name)
                            .map(|t| t.source())
                            .unwrap_or(ToolSource::BuiltIn),
                        input: input.clone(),
                        output: Some(serde_json::Value::String(result.output.clone())),
                        status: if result.is_error {
                            octo_types::ExecutionStatus::Failed
                        } else {
                            octo_types::ExecutionStatus::Success
                        },
                        started_at: started_at_ms,
                        duration_ms: Some(exec_duration),
                        error: if result.is_error { Some(result.output.clone()) } else { None },
                    };
                    let _ = tx.send(AgentEvent::ToolExecution { execution: exec });
                }

                let _ = tx.send(AgentEvent::ToolResult {
                    tool_id: tu.id.clone(),
                    output: result.output.clone(),
                    success: !result.is_error,
                });

                // Soft-trim large tool results before injecting into messages
                let trimmed_output = maybe_trim_tool_result(&result.output);

                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tu.id.clone(),
                    content: trimmed_output,
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

/// Soft-trim tool result if it exceeds the limit (67% head + 27% tail).
fn maybe_trim_tool_result(result: &str) -> String {
    if result.len() <= TOOL_RESULT_SOFT_LIMIT {
        return result.to_string();
    }
    let head_end = result
        .char_indices()
        .nth(20_000)
        .map(|(idx, _)| idx)
        .unwrap_or(result.len());
    let char_count = result.chars().count();
    let tail_start = result
        .char_indices()
        .nth(char_count.saturating_sub(8_000))
        .map(|(idx, _)| idx)
        .unwrap_or(result.len());
    let omitted = result.len().saturating_sub(20_000 + 8_000);
    format!(
        "{}\n\n[... omitted {} chars ...]\n\n{}",
        &result[..head_end],
        omitted,
        &result[tail_start..]
    )
}
