pub mod agent;
pub mod audit;
pub mod auth;
pub mod commands;
pub mod context;
pub mod db;
pub mod event;
pub mod hooks;
pub mod logging;
pub mod mcp;
pub mod memory;
pub mod metering;
pub mod metrics;
pub mod providers;
pub mod root;
pub mod sandbox;
pub mod scheduler;
pub mod secret;
pub mod security;
pub mod session;
pub mod skill_runtime;
pub mod skills;
pub mod sync;
pub mod tls;
pub mod tools;

pub use agent::{
    run_agent_loop, AgentCapability, AgentCatalog, AgentEntry, AgentError, AgentEvent,
    AgentExecutor, AgentExecutorHandle, AgentId, AgentLoop, AgentLoopConfig, AgentLoopResult,
    AgentManifest, AgentMessage, AgentProfile, AgentRouter, AgentRuntime, AgentRuntimeConfig,
    AgentSlot, AgentStatus, AgentStore, DualAgentManager, DualAgentProfile,
    NormalizedStopReason, PlanStep, RouteAlternative, RouteResult, TenantContext, ToolFilterMode,
    TurnGate,
};
pub use audit::{AuditEvent, AuditRecord, AuditStorage};
pub use auth::{
    Action, ApiKey, ApiKeyConfig, ApiKeyResponse, ApiKeyStorage, AuthConfig, AuthConfigYaml,
    AuthMode, Permission, Role, UserContext,
};
pub use context::{
    BootstrapFile, ContextBudgetManager, ContextPruner, DegradationLevel, SystemPromptBuilder,
};
pub use db::Database;
pub use event::{TelemetryBus, EventCountProjection, EventStore, TelemetryEvent, Projection, StoredEvent};
pub use hooks::{
    BoxHookHandler, HookAction, HookContext, HookFailureMode, HookHandler, HookPoint, HookRegistry,
};
pub use logging::{
    init_logging, init_logging_with_filter, init_pretty_logging, init_pretty_logging_with_filter,
};
// audit_log macro is exported at crate root via #[macro_export] in logging module
pub use mcp::{
    McpClient, McpManager, McpPromptArgument, McpPromptInfo, McpPromptMessage, McpPromptResult,
    McpResourceContent, McpResourceInfo, McpServerConfig, McpServerConfigV2, McpToolBridge,
    McpToolInfo, McpTransport, SseMcpClient, StdioMcpClient,
};
pub use memory::{
    AutoMemoryCategory, Entity, ExtractedMemory, FtsStore, GraphStats, GraphStore,
    HybridQueryEngine, HybridSearchResult, InMemoryWorkingMemory, KnowledgeGraph, MemoryExtractor,
    MemoryStore, MemorySystem, QueryType, Relation, RuleBasedExtractor, SqliteMemoryStore,
    SqliteWorkingMemory, TokenBudgetManager, VectorEntry, VectorIndex, VectorIndexConfig,
    VectorSearchResult, WorkingMemory,
};
pub use metering::{Metering, MeteringSnapshot};
pub use metrics::{Counter, Gauge, Histogram, MetricsRegistry};
pub use providers::{
    create_anthropic_provider, create_openai_provider, create_provider, Provider, ProviderConfig,
};
pub use sandbox::{
    DockerAdapter, ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxType,
    SubprocessAdapter, WasmAdapter,
};
pub use security::{ActionTracker, AutonomyLevel, CommandRiskLevel, SecurityPolicy};
pub use sync::{
    ChangeTracker, ConflictResolution, HlcTimestamp, HybridClock, LwwResolver, SyncChange,
    SyncClient, SyncConflict, SyncOperation, SyncPullRequest, SyncPullResponse, SyncPushRequest,
    SyncPushResponse, SyncReport, SyncServer, SyncStatus,
};
pub use session::{
    InMemorySessionStore, SessionData, SessionEvent, SessionEventBus, SessionStore,
    SessionSummary, SqliteSessionStore,
};
pub use skill_runtime::{RuntimeType, SkillContext, SkillRuntime, ToolInfo};
pub use skills::{SkillLoader, SkillRegistry, SkillTool};
pub use root::OctoRoot;
pub use tools::recorder::ToolExecutionRecorder;
pub use tools::{default_tools, default_tools_with_search_priority, register_memory_tools, Tool, ToolRegistry};
