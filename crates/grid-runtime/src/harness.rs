//! GridHarness — Tier 1 Harness implementation of RuntimeContract.
//!
//! Bridges grid-engine's AgentRuntime/AgentExecutor to the 13-method
//! EAASP RuntimeContract. This is a zero-adapter implementation:
//! all calls are direct Rust function calls with no serialization.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_stream::{Stream, StreamExt};
use tracing::{info, warn};

use grid_engine::{
    AgentEvent, AgentMessage, AgentRuntime,
    McpServerConfigV2, McpTransport as EngineMcpTransport,
};
use grid_types::id::{SandboxId, SessionId, UserId};

use crate::contract::*;
use crate::telemetry::TelemetryCollector;

/// Grid Tier 1 Harness — native RuntimeContract implementation.
///
/// Wraps an `AgentRuntime` and exposes it through the 13-method contract.
/// Hooks, MCP, and skills are handled natively by grid-engine internals;
/// `on_tool_call`, `on_tool_result`, and `on_stop` are no-ops for Grid.
pub struct GridHarness {
    runtime: Arc<AgentRuntime>,
    runtime_id: String,
    telemetry: TelemetryCollector,
}

impl GridHarness {
    /// Create a GridHarness wrapping an existing AgentRuntime.
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        let runtime_id = "grid-harness".to_string();
        Self {
            runtime,
            telemetry: TelemetryCollector::new(&runtime_id),
            runtime_id,
        }
    }

    /// Create a GridHarness with a custom runtime ID.
    pub fn with_runtime_id(mut self, id: impl Into<String>) -> Self {
        self.runtime_id = id.into();
        self.telemetry = TelemetryCollector::new(&self.runtime_id);
        self
    }

    /// Access the underlying AgentRuntime.
    pub fn runtime(&self) -> &Arc<AgentRuntime> {
        &self.runtime
    }

    /// Convert an AgentEvent broadcast receiver into a ResponseChunk stream.
    fn map_events_to_chunks(
        rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    ) -> Pin<Box<dyn Stream<Item = ResponseChunk> + Send>> {
        let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
            .filter_map(|result| {
                match result {
                    Ok(event) => Self::event_to_chunk(event),
                    Err(_) => None, // lagged — skip
                }
            });
        Box::pin(stream)
    }

    /// Map a single AgentEvent to an optional ResponseChunk.
    fn event_to_chunk(event: AgentEvent) -> Option<ResponseChunk> {
        match event {
            AgentEvent::TextDelta { text } => Some(ResponseChunk {
                chunk_type: "text_delta".into(),
                content: text,
                tool_name: None,
                tool_id: None,
                is_error: false,
            }),
            AgentEvent::TextComplete { text } => Some(ResponseChunk {
                chunk_type: "text_delta".into(),
                content: text,
                tool_name: None,
                tool_id: None,
                is_error: false,
            }),
            AgentEvent::ThinkingDelta { text } => Some(ResponseChunk {
                chunk_type: "thinking".into(),
                content: text,
                tool_name: None,
                tool_id: None,
                is_error: false,
            }),
            AgentEvent::ToolStart { tool_id, tool_name, input } => Some(ResponseChunk {
                chunk_type: "tool_start".into(),
                content: serde_json::to_string(&input).unwrap_or_default(),
                tool_name: Some(tool_name),
                tool_id: Some(tool_id),
                is_error: false,
            }),
            AgentEvent::ToolResult { tool_id, output, success } => Some(ResponseChunk {
                chunk_type: "tool_result".into(),
                content: output,
                tool_name: None,
                tool_id: Some(tool_id),
                is_error: !success,
            }),
            AgentEvent::Error { message } => Some(ResponseChunk {
                chunk_type: "error".into(),
                content: message,
                tool_name: None,
                tool_id: None,
                is_error: true,
            }),
            AgentEvent::Done | AgentEvent::Completed(_) => Some(ResponseChunk {
                chunk_type: "done".into(),
                content: String::new(),
                tool_name: None,
                tool_id: None,
                is_error: false,
            }),
            // Other events are internal — not exposed through contract
            _ => None,
        }
    }
}

#[async_trait]
impl RuntimeContract for GridHarness {
    async fn initialize(&self, payload: SessionPayload) -> anyhow::Result<SessionHandle> {
        let session_id = SessionId::new();
        let user_id = UserId::from_string(&payload.user_id);
        let sandbox_id = SandboxId::from_string("default");

        let _handle = self
            .runtime
            .start_session(
                session_id.clone(),
                user_id,
                sandbox_id,
                vec![], // empty initial history
                None,   // no agent_id override
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start session: {}", e))?;

        info!(
            session_id = %session_id,
            user = %payload.user_id,
            role = %payload.user_role,
            "GridHarness: session initialized"
        );

        Ok(SessionHandle {
            session_id: session_id.as_str().to_string(),
        })
    }

    async fn send(
        &self,
        handle: &SessionHandle,
        message: UserMessage,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = ResponseChunk> + Send>>> {
        let session_id = SessionId::from_string(&handle.session_id);

        let executor_handle = self
            .runtime
            .get_session_handle(&session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", handle.session_id))?;

        // Subscribe to events before sending to avoid missing early events
        let rx = executor_handle.subscribe();

        // Send user message
        executor_handle
            .send(AgentMessage::UserMessage {
                content: message.content,
                channel_id: "eaasp".to_string(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))?;

        Ok(Self::map_events_to_chunks(rx))
    }

    async fn load_skill(
        &self,
        handle: &SessionHandle,
        content: SkillContent,
    ) -> anyhow::Result<()> {
        info!(
            session_id = %handle.session_id,
            skill = %content.name,
            "GridHarness: load_skill (skill loading via SkillRegistry)"
        );
        // Skills in Grid are loaded via SkillRegistry at runtime init.
        // Dynamic skill injection during session will be implemented
        // when L2 Skill Assets layer integration is built.
        Ok(())
    }

    async fn on_tool_call(
        &self,
        _handle: &SessionHandle,
        _call: ToolCall,
    ) -> anyhow::Result<HookDecision> {
        // No-op for Grid: PreToolUse hooks fire natively inside AgentLoop.
        Ok(HookDecision::Allow)
    }

    async fn on_tool_result(
        &self,
        _handle: &SessionHandle,
        _result: ToolResult,
    ) -> anyhow::Result<HookDecision> {
        // No-op for Grid: PostToolUse hooks fire natively inside AgentLoop.
        Ok(HookDecision::Allow)
    }

    async fn on_stop(&self, _handle: &SessionHandle) -> anyhow::Result<StopDecision> {
        // No-op for Grid: Stop hooks fire natively inside AgentLoop.
        Ok(StopDecision::Complete)
    }

    async fn get_state(&self, handle: &SessionHandle) -> anyhow::Result<SessionState> {
        let session_id = SessionId::from_string(&handle.session_id);

        // Retrieve messages from session store
        let messages = self
            .runtime
            .session_store()
            .get_messages(&session_id)
            .await;

        // Serialize messages as state
        let state_data = match messages {
            Some(msgs) => serde_json::to_vec(&msgs)
                .map_err(|e| anyhow::anyhow!("Failed to serialize session state: {}", e))?,
            None => Vec::new(),
        };

        Ok(SessionState {
            session_id: handle.session_id.clone(),
            runtime_id: self.runtime_id.clone(),
            state_data,
            created_at: chrono::Utc::now(),
            state_format: "rust-serde-v1".into(),
        })
    }

    async fn restore_state(&self, state: SessionState) -> anyhow::Result<SessionHandle> {
        if state.state_data.is_empty() {
            return Err(anyhow::anyhow!("Empty state data"));
        }

        let messages: Vec<grid_types::ChatMessage> = serde_json::from_slice(&state.state_data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize session state: {}", e))?;

        let session_id = SessionId::from_string(&state.session_id);
        let user_id = UserId::from_string("restored");
        let sandbox_id = SandboxId::from_string("default");

        let _handle = self
            .runtime
            .start_session(
                session_id.clone(),
                user_id,
                sandbox_id,
                messages,
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to restore session: {}", e))?;

        info!(session_id = %state.session_id, "GridHarness: session restored");

        Ok(SessionHandle {
            session_id: state.session_id,
        })
    }

    async fn connect_mcp(
        &self,
        handle: &SessionHandle,
        servers: Vec<McpServerConfig>,
    ) -> anyhow::Result<()> {
        let mcp_manager = self.runtime.mcp_manager();
        let mut mcp_guard = mcp_manager.lock().await;

        for server in &servers {
            let transport = match server.transport.as_str() {
                "stdio" => EngineMcpTransport::Stdio,
                "sse" | "http" | "streamable-http" => EngineMcpTransport::Sse,
                other => {
                    warn!(server = %server.name, transport = %other, "Unknown MCP transport, defaulting to stdio");
                    EngineMcpTransport::Stdio
                }
            };

            let config = McpServerConfigV2 {
                id: format!("eaasp-{}", server.name),
                name: server.name.clone(),
                source: "eaasp-platform".to_string(),
                command: server.command.clone().unwrap_or_default(),
                args: server.args.clone(),
                env: server.env.clone(),
                enabled: true,
                transport,
                url: server.url.clone(),
                oauth: None,
            };

            if let Err(e) = mcp_guard.add_server(config.into()).await {
                warn!(server = %server.name, error = %e, "Failed to add MCP server");
            }
        }

        info!(
            session_id = %handle.session_id,
            count = servers.len(),
            "GridHarness: MCP servers connected"
        );
        Ok(())
    }

    async fn emit_telemetry(
        &self,
        handle: &SessionHandle,
    ) -> anyhow::Result<Vec<TelemetryEvent>> {
        Ok(self.telemetry.collect(&handle.session_id, &self.runtime).await)
    }

    fn get_capabilities(&self) -> CapabilityManifest {
        let tool_names: Vec<String> = {
            let tools = self.runtime.tools().lock().unwrap();
            tools.names()
        };

        CapabilityManifest {
            runtime_id: self.runtime_id.clone(),
            runtime_name: "Grid".into(),
            tier: RuntimeTier::Harness,
            model: "claude-sonnet-4-20250514".into(),
            context_window: 200_000,
            supported_tools: tool_names,
            native_hooks: true,
            native_mcp: true,
            native_skills: true,
            cost: Some(CostEstimate {
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            }),
            metadata: Default::default(),
            requires_hook_bridge: false,
        }
    }

    async fn terminate(&self, handle: &SessionHandle) -> anyhow::Result<()> {
        let session_id = SessionId::from_string(&handle.session_id);
        self.runtime.stop_session(&session_id).await;

        info!(session_id = %handle.session_id, "GridHarness: session terminated");
        Ok(())
    }

    async fn disconnect_mcp(
        &self,
        handle: &SessionHandle,
        server_name: &str,
    ) -> anyhow::Result<()> {
        let mcp_manager = self.runtime.mcp_manager();
        let mut mcp_guard = mcp_manager.lock().await;
        let _ = mcp_guard.remove_server(server_name).await;
        info!(
            session_id = %handle.session_id,
            server = %server_name,
            "GridHarness: MCP server disconnected"
        );
        Ok(())
    }

    async fn pause_session(&self, handle: &SessionHandle) -> anyhow::Result<()> {
        let session_id = SessionId::from_string(&handle.session_id);
        self.runtime.stop_session(&session_id).await;
        info!(session_id = %handle.session_id, "GridHarness: session paused");
        Ok(())
    }

    async fn resume_session(&self, session_id: &str) -> anyhow::Result<SessionHandle> {
        warn!(session_id = %session_id, "GridHarness: resume_session stub — use restore_state with persisted state");
        Err(anyhow::anyhow!("resume_session requires state from L4 session store; use restore_state instead"))
    }

    async fn health(&self) -> anyhow::Result<HealthStatus> {
        let mut checks = std::collections::HashMap::new();

        // Check MCP manager
        {
            let mcp_guard = self.runtime.mcp_manager().lock().await;
            let server_count = mcp_guard.server_count();
            checks.insert("mcp".into(), format!("ok ({} servers)", server_count));
        }

        // Check session count
        let session_count = self.runtime.active_session_count();
        checks.insert("sessions".into(), format!("ok ({} active)", session_count));

        // Check event bus
        let has_event_bus = self.runtime.event_bus().is_some();
        checks.insert(
            "telemetry".into(),
            if has_event_bus { "ok" } else { "disabled" }.into(),
        );

        Ok(HealthStatus {
            healthy: true,
            runtime_id: self.runtime_id.clone(),
            checks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_to_chunk_text_delta() {
        let event = AgentEvent::TextDelta { text: "hello".into() };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "text_delta");
        assert_eq!(chunk.content, "hello");
        assert!(!chunk.is_error);
    }

    #[test]
    fn event_to_chunk_tool_start() {
        let event = AgentEvent::ToolStart {
            tool_id: "t1".into(),
            tool_name: "bash".into(),
            input: serde_json::json!({"command": "ls"}),
        };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "tool_start");
        assert_eq!(chunk.tool_name.as_deref(), Some("bash"));
        assert_eq!(chunk.tool_id.as_deref(), Some("t1"));
    }

    #[test]
    fn event_to_chunk_tool_result() {
        let event = AgentEvent::ToolResult {
            tool_id: "t1".into(),
            output: "file1.rs".into(),
            success: true,
        };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "tool_result");
        assert!(!chunk.is_error);
    }

    #[test]
    fn event_to_chunk_error() {
        let event = AgentEvent::Error { message: "boom".into() };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "error");
        assert!(chunk.is_error);
    }

    #[test]
    fn event_to_chunk_done() {
        let chunk = GridHarness::event_to_chunk(AgentEvent::Done).unwrap();
        assert_eq!(chunk.chunk_type, "done");
    }

    #[test]
    fn event_to_chunk_thinking() {
        let event = AgentEvent::ThinkingDelta { text: "analyzing...".into() };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "thinking");
    }

    #[test]
    fn event_to_chunk_internal_event_returns_none() {
        let event = AgentEvent::IterationStart { round: 1 };
        assert!(GridHarness::event_to_chunk(event).is_none());

        let event = AgentEvent::TokenBudgetUpdate {
            budget: grid_types::TokenBudgetSnapshot {
                total: 200_000,
                system_prompt: 5000,
                dynamic_context: 1000,
                history: 10000,
                free: 184000,
                usage_percent: 8.0,
                degradation_level: 0,
            },
        };
        assert!(GridHarness::event_to_chunk(event).is_none());
    }
}
