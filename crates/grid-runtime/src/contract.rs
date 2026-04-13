//! EAASP L1 Runtime Contract — v2.0 16-method interface.
//!
//! Any agent runtime that implements `RuntimeContract` can join the
//! EAASP runtime pool. Grid implements this natively as `GridHarness`
//! (zero serialization overhead). External adapters implement the
//! mirrored gRPC service definition in `proto/eaasp/runtime/v2/runtime.proto`.
//!
//! ## v2.0 Key Changes from v1
//!
//! - `SessionPayload` is now a **structured priority stack** of 5 blocks
//!   (P1 PolicyContext → P5 UserPreferences). See §8.6.
//! - 12 MUST + 5 OPTIONAL (incl. `EmitEvent`, ADR-V2-001 Accepted).
//! - Deterministic context budget trimming: P5 → P4 → P3, never P1/P2.

use std::pin::Pin;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

// ── Core Contract Trait ──

/// The v2.0 16-method runtime interface contract.
///
/// This is the Rust-native form. For external adapters (Python/TS),
/// see `proto/eaasp/runtime/v2/runtime.proto`.
#[async_trait]
pub trait RuntimeContract: Send + Sync {
    // ── 12 MUST methods ──

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

    /// Restore session from serialized state.
    async fn restore_state(&self, state: SessionState) -> anyhow::Result<SessionHandle>;

    // ── 4 OPTIONAL methods ──

    /// Health check.
    async fn health(&self) -> anyhow::Result<HealthStatus>;

    /// Disconnect a specific MCP server by name.
    async fn disconnect_mcp(
        &self,
        handle: &SessionHandle,
        server_name: &str,
    ) -> anyhow::Result<()>;

    /// Pause session: serialize state and release resources.
    async fn pause_session(&self, handle: &SessionHandle) -> anyhow::Result<()>;

    /// Resume a previously paused session.
    async fn resume_session(&self, session_id: &str) -> anyhow::Result<SessionHandle>;

    // ── OPTIONAL method (ADR-V2-001 Accepted — Phase 1) ──

    /// EmitEvent — OPTIONAL per ADR-V2-001.
    ///
    /// T1 runtimes SHOULD implement to emit enriched events (THINKING,
    /// TOKEN_USAGE, PRE_COMPACT) that the L4 interceptor cannot capture.
    /// Core events (PRE_TOOL_USE, POST_TOOL_USE, STOP) are already
    /// captured by the L4 platform interceptor.
    ///
    /// Default: no-op (silently succeeds). T1 implementations can override
    /// to POST events to L4's `/v1/events/ingest` endpoint.
    async fn emit_event(&self, _entry: EventStreamEntry) -> anyhow::Result<()> {
        Ok(())
    }
}

// ── SessionPayload (v2.0 structured P1-P5 blocks) ──

/// v2.0 structured session payload.
///
/// Five priority blocks; context budget trimming is deterministic:
/// P5 → P4 → P3 in that order when allowed. P1 and P2 are NEVER trimmed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionPayload {
    /// P1 — PolicyContext. Highest priority; never removable.
    pub policy_context: Option<PolicyContext>,
    /// P2 — EventContext. Only if session was event-triggered.
    pub event_context: Option<EventContext>,
    /// P3 — Cross-session memories (L2 Memory Engine).
    pub memory_refs: Vec<MemoryRef>,
    /// P4 — Skill prose + frontmatter-scoped hooks.
    pub skill_instructions: Option<SkillInstructions>,
    /// P5 — User preferences. Lowest priority; trimmed first.
    pub user_preferences: Option<UserPreferences>,

    /// When set, trim pass may remove P5. Default true.
    pub allow_trim_p5: bool,
    /// When set, trim pass may remove P4 after P5. Default false.
    pub allow_trim_p4: bool,
    /// When set, trim pass may remove P3 after P4. Default false.
    pub allow_trim_p3: bool,

    /// Session metadata (populated by L4 orchestration).
    pub session_id: String,
    pub user_id: String,
    pub runtime_id: String,
    pub created_at: String,
}

impl SessionPayload {
    /// Construct an empty payload with `allow_trim_p5 = true`.
    pub fn new() -> Self {
        Self {
            allow_trim_p5: true,
            ..Default::default()
        }
    }
}

/// P1 — Policy context (L3 governance).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyContext {
    pub hooks: Vec<ManagedHook>,
    pub org_unit: String,
    pub policy_version: String,
    pub quotas: std::collections::HashMap<String, String>,
    pub deploy_timestamp: String,
}

/// L3-managed hook rule (P1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedHook {
    pub hook_id: String,
    pub hook_type: String,
    pub condition: String,
    pub action: String,
    pub precedence: i32,
    pub scope: String,
}

/// P2 — Event context (L4 orchestration trigger).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventContext {
    pub event_id: String,
    pub event_type: String,
    pub severity: String,
    pub source: String,
    pub payload_json: String,
    pub timestamp: String,
}

/// P3 — Cross-session memory reference (L2 Memory Engine).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRef {
    pub memory_id: String,
    pub memory_type: String,
    pub relevance_score: f64,
    pub content: String,
    pub source_session_id: String,
    pub created_at: String,
    pub tags: std::collections::HashMap<String, String>,
}

/// P4 — Skill instructions (L2 Skill Registry).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillInstructions {
    pub skill_id: String,
    pub name: String,
    pub content: String,
    pub frontmatter_hooks: Vec<ScopedHook>,
    pub metadata: std::collections::HashMap<String, String>,
    /// MCP and other dependencies declared in skill frontmatter.
    /// Convention: `mcp:<name>` for MCP server dependencies.
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Skill-frontmatter scoped hook (P4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedHook {
    pub hook_id: String,
    pub hook_type: String,
    pub condition: String,
    pub action: String,
    pub precedence: i32,
}

/// P5 — User preferences.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserPreferences {
    pub user_id: String,
    pub prefs: std::collections::HashMap<String, String>,
    pub language: String,
    pub timezone: String,
    /// LLM provider hint (e.g. "anthropic", "openai"). L1 runtime env takes precedence.
    pub llm_provider: String,
    /// LLM model hint (e.g. "claude-sonnet-4-20250514"). L1 runtime env takes precedence.
    pub llm_model: String,
}

// ── Opaque handle + runtime event types ──

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

/// Skill content (SKILL.md parsed). Used by `load_skill`.
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
    /// Serialization format identifier (e.g., "rust-serde-v2").
    pub state_format: String,
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
    /// Whether this runtime requires HookBridge (false for Tier 1).
    pub requires_hook_bridge: bool,
    /// Deployment mode: "shared" (multi-session per process) or "per_session" (one container per session).
    pub deployment_mode: DeploymentMode,
}

/// How L3 should schedule containers for this runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentMode {
    /// Single process serves multiple sessions (e.g. grid-runtime with tokio async).
    Shared,
    /// One container per session, destroyed on terminate (e.g. claude-code-runtime).
    PerSession,
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

/// v2.0 EmitEvent placeholder payload (ADR-V2-001 pending).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventStreamEntry {
    pub session_id: String,
    pub event_id: String,
    /// HookEventType as string (e.g., "PRE_TOOL_USE").
    pub event_type: String,
    pub payload_json: String,
    pub timestamp: String,
}

// ── Proto ↔ contract conversions ──
//
// SessionPayload round-trips between the Rust-native contract form and
// the generated `proto::SessionPayload` (v2 priority blocks).

impl From<crate::proto::SessionPayload> for SessionPayload {
    fn from(p: crate::proto::SessionPayload) -> Self {
        SessionPayload {
            policy_context: p.policy_context.map(Into::into),
            event_context: p.event_context.map(Into::into),
            memory_refs: p.memory_refs.into_iter().map(Into::into).collect(),
            skill_instructions: p.skill_instructions.map(Into::into),
            user_preferences: p.user_preferences.map(Into::into),
            allow_trim_p5: p.allow_trim_p5,
            allow_trim_p4: p.allow_trim_p4,
            allow_trim_p3: p.allow_trim_p3,
            session_id: p.session_id,
            user_id: p.user_id,
            runtime_id: p.runtime_id,
            created_at: p.created_at,
        }
    }
}

impl From<SessionPayload> for crate::proto::SessionPayload {
    fn from(p: SessionPayload) -> Self {
        crate::proto::SessionPayload {
            policy_context: p.policy_context.map(Into::into),
            event_context: p.event_context.map(Into::into),
            memory_refs: p.memory_refs.into_iter().map(Into::into).collect(),
            skill_instructions: p.skill_instructions.map(Into::into),
            user_preferences: p.user_preferences.map(Into::into),
            allow_trim_p5: p.allow_trim_p5,
            allow_trim_p4: p.allow_trim_p4,
            allow_trim_p3: p.allow_trim_p3,
            session_id: p.session_id,
            user_id: p.user_id,
            runtime_id: p.runtime_id,
            created_at: p.created_at,
        }
    }
}

impl From<crate::proto::PolicyContext> for PolicyContext {
    fn from(p: crate::proto::PolicyContext) -> Self {
        PolicyContext {
            hooks: p.hooks.into_iter().map(Into::into).collect(),
            org_unit: p.org_unit,
            policy_version: p.policy_version,
            quotas: p.quotas,
            deploy_timestamp: p.deploy_timestamp,
        }
    }
}

impl From<PolicyContext> for crate::proto::PolicyContext {
    fn from(p: PolicyContext) -> Self {
        crate::proto::PolicyContext {
            hooks: p.hooks.into_iter().map(Into::into).collect(),
            org_unit: p.org_unit,
            policy_version: p.policy_version,
            quotas: p.quotas,
            deploy_timestamp: p.deploy_timestamp,
        }
    }
}

impl From<crate::proto::ManagedHook> for ManagedHook {
    fn from(h: crate::proto::ManagedHook) -> Self {
        ManagedHook {
            hook_id: h.hook_id,
            hook_type: h.hook_type,
            condition: h.condition,
            action: h.action,
            precedence: h.precedence,
            scope: h.scope,
        }
    }
}

impl From<ManagedHook> for crate::proto::ManagedHook {
    fn from(h: ManagedHook) -> Self {
        crate::proto::ManagedHook {
            hook_id: h.hook_id,
            hook_type: h.hook_type,
            condition: h.condition,
            action: h.action,
            precedence: h.precedence,
            scope: h.scope,
        }
    }
}

impl From<crate::proto::EventContext> for EventContext {
    fn from(e: crate::proto::EventContext) -> Self {
        EventContext {
            event_id: e.event_id,
            event_type: e.event_type,
            severity: e.severity,
            source: e.source,
            payload_json: e.payload_json,
            timestamp: e.timestamp,
        }
    }
}

impl From<EventContext> for crate::proto::EventContext {
    fn from(e: EventContext) -> Self {
        crate::proto::EventContext {
            event_id: e.event_id,
            event_type: e.event_type,
            severity: e.severity,
            source: e.source,
            payload_json: e.payload_json,
            timestamp: e.timestamp,
        }
    }
}

impl From<crate::proto::MemoryRef> for MemoryRef {
    fn from(m: crate::proto::MemoryRef) -> Self {
        MemoryRef {
            memory_id: m.memory_id,
            memory_type: m.memory_type,
            relevance_score: m.relevance_score,
            content: m.content,
            source_session_id: m.source_session_id,
            created_at: m.created_at,
            tags: m.tags,
        }
    }
}

impl From<MemoryRef> for crate::proto::MemoryRef {
    fn from(m: MemoryRef) -> Self {
        crate::proto::MemoryRef {
            memory_id: m.memory_id,
            memory_type: m.memory_type,
            relevance_score: m.relevance_score,
            content: m.content,
            source_session_id: m.source_session_id,
            created_at: m.created_at,
            tags: m.tags,
        }
    }
}

impl From<crate::proto::SkillInstructions> for SkillInstructions {
    fn from(s: crate::proto::SkillInstructions) -> Self {
        SkillInstructions {
            skill_id: s.skill_id,
            name: s.name,
            content: s.content,
            frontmatter_hooks: s.frontmatter_hooks.into_iter().map(Into::into).collect(),
            metadata: s.metadata,
            dependencies: s.dependencies,
        }
    }
}

impl From<SkillInstructions> for crate::proto::SkillInstructions {
    fn from(s: SkillInstructions) -> Self {
        crate::proto::SkillInstructions {
            skill_id: s.skill_id,
            name: s.name,
            content: s.content,
            frontmatter_hooks: s.frontmatter_hooks.into_iter().map(Into::into).collect(),
            metadata: s.metadata,
            dependencies: s.dependencies,
        }
    }
}

impl From<crate::proto::ScopedHook> for ScopedHook {
    fn from(h: crate::proto::ScopedHook) -> Self {
        ScopedHook {
            hook_id: h.hook_id,
            hook_type: h.hook_type,
            condition: h.condition,
            action: h.action,
            precedence: h.precedence,
        }
    }
}

impl From<ScopedHook> for crate::proto::ScopedHook {
    fn from(h: ScopedHook) -> Self {
        crate::proto::ScopedHook {
            hook_id: h.hook_id,
            hook_type: h.hook_type,
            condition: h.condition,
            action: h.action,
            precedence: h.precedence,
        }
    }
}

impl From<crate::proto::UserPreferences> for UserPreferences {
    fn from(u: crate::proto::UserPreferences) -> Self {
        UserPreferences {
            user_id: u.user_id,
            prefs: u.prefs,
            language: u.language,
            timezone: u.timezone,
            llm_provider: u.llm_provider,
            llm_model: u.llm_model,
        }
    }
}

impl From<UserPreferences> for crate::proto::UserPreferences {
    fn from(u: UserPreferences) -> Self {
        crate::proto::UserPreferences {
            user_id: u.user_id,
            prefs: u.prefs,
            language: u.language,
            timezone: u.timezone,
            llm_provider: u.llm_provider,
            llm_model: u.llm_model,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_payload_default_is_trimmable_p5() {
        let p = SessionPayload::new();
        assert!(p.allow_trim_p5);
        assert!(!p.allow_trim_p4);
        assert!(!p.allow_trim_p3);
    }

    #[test]
    fn session_payload_priority_blocks_default_none() {
        let p = SessionPayload::new();
        assert!(p.policy_context.is_none());
        assert!(p.event_context.is_none());
        assert!(p.memory_refs.is_empty());
        assert!(p.skill_instructions.is_none());
        assert!(p.user_preferences.is_none());
    }

    #[test]
    fn session_payload_roundtrip_serde() {
        let mut p = SessionPayload::new();
        p.user_id = "u-1".into();
        p.session_id = "s-1".into();
        p.user_preferences = Some(UserPreferences {
            user_id: "u-1".into(),
            language: "en".into(),
            ..Default::default()
        });
        let json = serde_json::to_string(&p).unwrap();
        let restored: SessionPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.user_id, "u-1");
        assert_eq!(
            restored.user_preferences.unwrap().language,
            "en"
        );
    }

    #[test]
    fn session_payload_proto_roundtrip_empty() {
        let p = SessionPayload::new();
        let proto_p: crate::proto::SessionPayload = p.clone().into();
        let back: SessionPayload = proto_p.into();
        assert_eq!(back.allow_trim_p5, p.allow_trim_p5);
        assert_eq!(back.session_id, p.session_id);
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
            requires_hook_bridge: false,
            deployment_mode: DeploymentMode::Shared,
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        assert!(json.contains("Harness"));
        assert!(json.contains("200000"));
        assert!(json.contains("requires_hook_bridge"));
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
