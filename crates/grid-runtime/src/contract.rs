//! EAASP L1 Runtime Contract — 13-method interface.
//!
//! Any agent runtime that implements `RuntimeContract` can join the
//! EAASP runtime pool. Grid implements this natively as `GridHarness`
//! (zero serialization overhead). External adapters implement the
//! mirrored gRPC service definition in `runtime.proto`.

use std::pin::Pin;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

// ── Core Contract Trait ──

/// The 13-method runtime interface contract.
///
/// This is the Rust-native form. For external adapters (Python/TS),
/// see `proto/eaasp/runtime/v1/runtime.proto`.
#[async_trait]
pub trait RuntimeContract: Send + Sync {
    /// Accept session initialization payload, return session handle.
    async fn initialize(&self, payload: SessionPayload) -> anyhow::Result<SessionHandle>;

    /// Accept user message, return streaming response.
    async fn send(
        &self,
        handle: &SessionHandle,
        message: UserMessage,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = ResponseChunk> + Send>>>;

    /// Load workflow-skill content, activate scoped hooks.
    async fn load_skill(
        &self,
        handle: &SessionHandle,
        content: SkillContent,
    ) -> anyhow::Result<()>;

    /// Pre-tool-call hook interception (no-op for native hook runtimes).
    async fn on_tool_call(
        &self,
        handle: &SessionHandle,
        call: ToolCall,
    ) -> anyhow::Result<HookDecision>;

    /// Post-tool-result hook interception (no-op for native hook runtimes).
    async fn on_tool_result(
        &self,
        handle: &SessionHandle,
        result: ToolResult,
    ) -> anyhow::Result<HookDecision>;

    /// Agent stop event. Returns `Continue(feedback)` to force-continue (exit-2).
    async fn on_stop(&self, handle: &SessionHandle) -> anyhow::Result<StopDecision>;

    /// Serialize full session state for persistence to L4 session store.
    async fn get_state(&self, handle: &SessionHandle) -> anyhow::Result<SessionState>;

    /// Restore session from serialized state.
    async fn restore_state(&self, state: SessionState) -> anyhow::Result<SessionHandle>;

    /// Connect to MCP servers.
    async fn connect_mcp(
        &self,
        handle: &SessionHandle,
        servers: Vec<McpServerConfig>,
    ) -> anyhow::Result<()>;

    /// Emit standardized telemetry events.
    async fn emit_telemetry(
        &self,
        handle: &SessionHandle,
    ) -> anyhow::Result<Vec<TelemetryEvent>>;

    /// Return capability manifest (synchronous — capabilities are static).
    fn get_capabilities(&self) -> CapabilityManifest;

    /// Clean up resources, emit SessionEnd, flush all async telemetry.
    async fn terminate(&self, handle: &SessionHandle) -> anyhow::Result<()>;

    /// Health check.
    async fn health(&self) -> anyhow::Result<HealthStatus>;
}

// ── Types ──

/// Session initialization payload from L3 three-way handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPayload {
    pub user_id: String,
    pub user_role: String,
    pub org_unit: String,
    /// managed-settings.json content (hooks configuration).
    pub managed_hooks_json: Option<String>,
    /// Quota limits (e.g., "max_tokens" -> "100000").
    pub quotas: std::collections::HashMap<String, String>,
    /// Additional context key-values.
    pub context: std::collections::HashMap<String, String>,
}

/// Opaque session handle returned by `initialize`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionHandle {
    pub session_id: String,
}

/// User message or structured intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub content: String,
    /// "text" or "intent".
    pub message_type: String,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Streaming response chunk from the runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseChunk {
    /// "text_delta" | "tool_start" | "tool_result" | "thinking" | "done" | "error"
    pub chunk_type: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_id: Option<String>,
    pub is_error: bool,
}

/// Skill content (SKILL.md parsed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContent {
    pub skill_id: String,
    pub name: String,
    /// YAML frontmatter (scoped hooks, runtime affinity, metadata).
    pub frontmatter_yaml: String,
    /// Natural language instructions.
    pub prose: String,
}

/// Tool call event for hook interception.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,
    pub tool_id: String,
    pub input: serde_json::Value,
}

/// Tool result event for hook interception.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_name: String,
    pub tool_id: String,
    pub output: String,
    pub is_error: bool,
}

/// Hook decision: allow, deny, or modify.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookDecision {
    Allow,
    Deny { reason: String },
    Modify { transformed_input: serde_json::Value },
}

/// Stop decision: complete or force-continue (exit-2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopDecision {
    Complete,
    Continue { feedback: String },
}

/// Serialized session state for cross-session persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub runtime_id: String,
    pub state_data: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// MCP server connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    /// "stdio" | "sse" | "streamable-http"
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: std::collections::HashMap<String, String>,
}

/// Standardized telemetry event (EAASP schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub session_id: String,
    pub runtime_id: String,
    pub user_id: Option<String>,
    /// "tool_call" | "tool_result" | "hook_decision" | "session_end"
    pub event_type: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub payload: serde_json::Value,
    pub resource_usage: ResourceUsage,
}

/// Resource consumption metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub compute_ms: u64,
}

/// Runtime capability manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub runtime_id: String,
    pub runtime_name: String,
    /// "harness" | "aligned" | "framework"
    pub tier: RuntimeTier,
    pub model: String,
    pub context_window: u32,
    pub supported_tools: Vec<String>,
    pub native_hooks: bool,
    pub native_mcp: bool,
    pub native_skills: bool,
    pub cost: Option<CostEstimate>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Runtime tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeTier {
    /// Tier 1: Native hooks, MCP, skills (Octo, Claude Code, Agent SDK).
    Harness,
    /// Tier 2: Headless CLI, partial MCP, needs hook bridge (Aider, Goose).
    Aligned,
    /// Tier 3: AI framework, needs thick adapter (LangGraph, CrewAI).
    Framework,
}

/// Cost estimate per 1k tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub input_cost_per_1k: f64,
    pub output_cost_per_1k: f64,
}

/// Runtime health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub runtime_id: String,
    /// Component checks, e.g. "provider" -> "ok", "mcp" -> "degraded".
    pub checks: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_payload_roundtrip() {
        let payload = SessionPayload {
            user_id: "user-1".into(),
            user_role: "developer".into(),
            org_unit: "engineering".into(),
            managed_hooks_json: None,
            quotas: Default::default(),
            context: Default::default(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let restored: SessionPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.user_id, "user-1");
    }

    #[test]
    fn hook_decision_variants() {
        let allow = HookDecision::Allow;
        let deny = HookDecision::Deny {
            reason: "policy violation".into(),
        };
        let modify = HookDecision::Modify {
            transformed_input: serde_json::json!({"sanitized": true}),
        };

        let json = serde_json::to_string(&allow).unwrap();
        assert!(json.contains("Allow"));
        let json = serde_json::to_string(&deny).unwrap();
        assert!(json.contains("policy violation"));
        let json = serde_json::to_string(&modify).unwrap();
        assert!(json.contains("sanitized"));
    }

    #[test]
    fn stop_decision_variants() {
        let complete = StopDecision::Complete;
        let cont = StopDecision::Continue {
            feedback: "missing section".into(),
        };

        let json = serde_json::to_string(&complete).unwrap();
        assert!(json.contains("Complete"));
        let json = serde_json::to_string(&cont).unwrap();
        assert!(json.contains("missing section"));
    }

    #[test]
    fn capability_manifest_serialization() {
        let manifest = CapabilityManifest {
            runtime_id: "grid-harness".into(),
            runtime_name: "Grid".into(),
            tier: RuntimeTier::Harness,
            model: "claude-sonnet-4-20250514".into(),
            context_window: 200_000,
            supported_tools: vec!["bash".into(), "read_file".into(), "edit_file".into()],
            native_hooks: true,
            native_mcp: true,
            native_skills: true,
            cost: Some(CostEstimate {
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            }),
            metadata: Default::default(),
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        assert!(json.contains("Harness"));
        assert!(json.contains("200000"));
    }

    #[test]
    fn telemetry_event_with_resource_usage() {
        let event = TelemetryEvent {
            session_id: "s-1".into(),
            runtime_id: "grid-harness".into(),
            user_id: Some("u-1".into()),
            event_type: "tool_call".into(),
            timestamp: chrono::Utc::now(),
            payload: serde_json::json!({"tool": "bash", "command": "ls"}),
            resource_usage: ResourceUsage {
                input_tokens: 500,
                output_tokens: 100,
                compute_ms: 1200,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("tool_call"));
        assert!(json.contains("1200"));
    }

    #[test]
    fn runtime_tier_equality() {
        assert_eq!(RuntimeTier::Harness, RuntimeTier::Harness);
        assert_ne!(RuntimeTier::Harness, RuntimeTier::Aligned);
        assert_ne!(RuntimeTier::Aligned, RuntimeTier::Framework);
    }

    #[test]
    fn health_status_creation() {
        let mut checks = std::collections::HashMap::new();
        checks.insert("provider".into(), "ok".into());
        checks.insert("mcp".into(), "degraded".into());
        let status = HealthStatus {
            healthy: false,
            runtime_id: "grid-harness".into(),
            checks,
        };
        assert!(!status.healthy);
        assert_eq!(status.checks.get("mcp").unwrap(), "degraded");
    }
}
