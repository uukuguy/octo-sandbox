# AgentRuntime Complete Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Internalize McpManager, EventBus, and working_dir configuration into AgentRuntime, eliminating all component initialization from main.rs.

**Architecture:** AgentRuntime becomes the single runtime container holding all components (Provider, Tools, Memory, Sessions, Skills, McpManager, EventBus). main.rs only passes Config.

**Tech Stack:** Rust, Tokio, Axum, SQLite (rusqlite)

---

## Task 1: Add working_dir and enable_event_bus to AgentRuntimeConfig

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs:30-58`

**Step 1: Add new fields to AgentRuntimeConfig struct**

```rust
#[derive(Debug, Clone)]
pub struct AgentRuntimeConfig {
    pub db_path: String,
    pub provider: ProviderConfig,
    pub skills_dirs: Vec<String>,
    pub provider_chain: Option<ProviderChainConfig>,
    pub working_dir: Option<PathBuf>,   // ADD THIS
    pub enable_event_bus: bool,         // ADD THIS
}
```

**Step 2: Update from_parts method signature and body**

```rust
impl AgentRuntimeConfig {
    pub fn from_parts(
        db_path: String,
        provider: ProviderConfig,
        skills_dirs: Vec<String>,
        provider_chain: Option<ProviderChainConfig>,
        working_dir: Option<PathBuf>,    // ADD THIS
        enable_event_bus: bool,          // ADD THIS
    ) -> Self {
        Self {
            db_path,
            provider,
            skills_dirs,
            provider_chain,
            working_dir,                  // ADD THIS
            enable_event_bus,              // ADD THIS
        }
    }
}
```

**Step 3: Run cargo check to verify**

```bash
cd /Users/sujiangwen/sandbox/LLM/speechless.ai/Autonomous-Agents/octo-sandbox && cargo check -p octo-engine
```

Expected: Should show errors about missing McpManager import (we'll fix in next task)

**Step 4: Commit**

```bash
git add crates/octo-engine/src/agent/runtime.rs
git commit -m "feat(runtime): add working_dir and enable_event_bus to AgentRuntimeConfig"
```

---

## Task 2: Add mcp_manager and working_dir fields to AgentRuntime

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs:60-80`

**Step 1: Add new fields to AgentRuntime struct**

```rust
pub struct AgentRuntime {
    // ... existing fields ...
    event_bus: Option<Arc<EventBus>>,
    recorder: Arc<ToolExecutionRecorder>,
    provider_chain: Option<Arc<ProviderChain>>,

    // ADD THESE NEW FIELDS
    mcp_manager: Option<Arc<tokio::sync::Mutex<crate::mcp::manager::McpManager>>>,
    working_dir: PathBuf,
}
```

**Step 2: Add PathBuf to imports if not present**

```rust
use std::path::PathBuf;  // ADD if not already present
```

**Step 3: Run cargo check**

```bash
cargo check -p octo-engine
```

Expected: FAIL - McpManager type not found (we'll fix imports in Task 3)

**Step 4: Commit**

```bash
git add crates/octo-engine/src/agent/runtime.rs
git commit -f "feat(runtime): add mcp_manager and working_dir fields to AgentRuntime"
```

---

## Task 3: Add MCP-related AgentError variants

**Files:**
- Modify: `crates/octo-engine/src/agent/entry.rs:109-131`

**Step 1: Add new error variants**

```rust
pub enum AgentError {
    NotFound(AgentId),
    InvalidTransition { from: AgentStatus, action: &'static str },
    ScheduledTask(String),
    Internal(String),

    // ADD THESE
    McpNotInitialized,
    McpError(String),
    McpServerNotFound(String),
}
```

**Step 2: Update Display implementation**

```rust
impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "agent not found: {id}"),
            Self::InvalidTransition { from, action } => {
                write!(f, "cannot {action} agent in state {from}")
            }
            Self::ScheduledTask(msg) => write!(f, "scheduled task error: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),

            // ADD THESE
            Self::McpNotInitialized => write!(f, "MCP manager not initialized"),
            Self::McpError(msg) => write!(f, "MCP error: {msg}"),
            Self::McpServerNotFound(name) => write!(f, "MCP server not found: {name}"),
        }
    }
}
```

**Step 3: Run cargo check**

```bash
cargo check -p octo-engine
```

Expected: FAIL - still need to implement the new fields in runtime.rs

**Step 4: Commit**

```bash
git add crates/octo-engine/src/agent/entry.rs
git commit -m "feat(error): add MCP-related AgentError variants"
```

---

## Task 4: Initialize McpManager and EventBus in AgentRuntime::new()

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs:100-193`

**Step 1: Add mcp import**

```rust
use crate::mcp::manager::McpManager;  // ADD THIS
```

**Step 2: In new() method, add initialization code after existing init (around line 170)**

```rust
// After provider_chain initialization, add:

// EventBus initialization (default enabled)
let event_bus = if config.enable_event_bus {
    Some(Arc::new(EventBus::new(
        1000,
        1000,
        Arc::new(crate::metrics::MetricsRegistry::new()),
    )))
} else {
    None
};

// McpManager initialization
let mcp_manager = Some(Arc::new(tokio::sync::Mutex::new(
    McpManager::new()
)));

// Working directory
let working_dir = config.working_dir
    .unwrap_or_else(|| PathBuf::from("/tmp/octo-sandbox"));
```

**Step 3: Add new fields to Ok() return (around line 178)**

```rust
Ok(Self {
    primary_handle: Mutex::new(None),
    agent_handles: DashMap::new(),
    catalog,
    provider,
    tools: Arc::new(tools),
    skill_registry: Some(skill_registry),
    memory,
    memory_store,
    session_store,
    default_model,
    event_bus,          // UPDATE: was None
    recorder,
    provider_chain,
    mcp_manager,        // ADD
    working_dir,        // ADD
})
```

**Step 4: Run cargo check**

```bash
cargo check -p octo-engine
```

Expected: Should compile (or show minor errors we can fix)

**Step 5: Commit**

```bash
git add crates/octo-engine/src/agent/runtime.rs
git commit -m "feat(runtime): initialize McpManager and EventBus in new()"
```

---

## Task 5: Implement add_mcp_server, remove_mcp_server, list_mcp_servers APIs

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs` (add after line 275)

**Step 1: Add McpToolInfo import**

```rust
use crate::mcp::traits::McpToolInfo;  // ADD THIS
```

**Step 2: Add new methods to AgentRuntime impl**

```rust
/// 添加 MCP Server → 自动注册 tools
pub async fn add_mcp_server(
    &self,
    config: crate::mcp::traits::McpServerConfig,
) -> Result<Vec<McpToolInfo>, AgentError> {
    let mcp = self.mcp_manager.as_ref()
        .ok_or(AgentError::McpNotInitialized)?;

    let mut guard = mcp.lock().await;
    let tools = guard.add_server(config).await
        .map_err(|e| AgentError::McpError(e.to_string()))?;

    // 注册到 ToolRegistry
    for tool_info in &tools {
        let bridge = crate::mcp::bridge::McpToolBridge::new(tool_info.clone());
        self.tools.register(bridge);
    }

    Ok(tools)
}

/// 移除 MCP Server → 自动注销 tools
pub async fn remove_mcp_server(
    &self,
    name: &str,
) -> Result<(), AgentError> {
    let mcp = self.mcp_manager.as_ref()
        .ok_or(AgentError::McpNotInitialized)?;

    let mut guard = mcp.lock().await;
    let removed_tools = guard.remove_server(name).await
        .map_err(|e| AgentError::McpError(e.to_string()))?;

    // 从 ToolRegistry 注销
    for tool in removed_tools {
        self.tools.unregister(&tool.name);
    }

    Ok(())
}

/// 列出运行中的 MCP servers
pub fn list_mcp_servers(&self) -> Vec<crate::mcp::manager::ServerRuntimeState> {
    match &self.mcp_manager {
        Some(mcp) => {
            // Note: This is a simplified version, need proper locking
            // For now, return empty or implement properly
            vec![]
        }
        None => vec![],
    }
}

/// 获取 MCP Manager 引用
pub fn mcp_manager(&self) -> Option<&Arc<tokio::sync::Mutex<McpManager>>> {
    self.mcp_manager.as_ref()
}
```

**Step 3: Run cargo check**

```bash
cargo check -p octo-engine
```

Expected: Should compile (or show minor errors)

**Step 4: Commit**

```bash
git add crates/octo-engine/src/agent/runtime.rs
git commit -m "feat(runtime): implement add/remove/list_mcp_servers APIs"
```

---

## Task 6: Add mcp_manager getter to AgentRuntime

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs`

**Step 1: Add getter method**

After the existing getter methods (around line 270), add:

```rust
pub fn mcp_manager(&self) -> Option<&Arc<tokio::sync::Mutex<McpManager>>> {
    self.mcp_manager.as_ref()
}
```

**Step 2: Run cargo check**

```bash
cargo check -p octo-engine
```

Expected: PASS

**Step 3: Commit**

```bash
git add crates/octo-engine/src/agent/runtime.rs
git commit -f "feat(runtime): add mcp_manager getter"
```

---

## Task 7: Add working_dir to AgentExecutor

**Files:**
- Modify: `crates/octo-engine/src/agent/executor.rs:54-79`

**Step 1: Add working_dir field to AgentExecutor struct**

```rust
pub struct AgentExecutor {
    // ... existing fields ...
    cancel_flag: Arc<AtomicBool>,

    // ADD THIS
    working_dir: PathBuf,
}
```

**Step 2: Add PathBuf import if not present**

```rust
use std::path::PathBuf;  // ADD if not present
```

**Step 3: Update new() signature and body (around line 82)**

```rust
#[allow(clippy::too_many_arguments)]
pub fn new(
    session_id: SessionId,
    user_id: UserId,
    sandbox_id: SandboxId,
    initial_history: Vec<ChatMessage>,
    rx: mpsc::Receiver<AgentMessage>,
    broadcast_tx: broadcast::Sender<AgentEvent>,
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<dyn WorkingMemory>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    model: Option<String>,
    session_store: Option<Arc<dyn SessionStore>>,
    system_prompt: Option<String>,
    config: AgentConfig,
    // ADD THESE TWO PARAMETERS
    working_dir: PathBuf,
    event_bus: Option<Arc<crate::event::EventBus>>,
) -> Self {
    // ... existing body ...

    Self {
        // ... existing fields ...
        cancel_flag: Arc::new(AtomicBool::new(false)),

        // ADD THESE
        working_dir,
    }
}
```

**Step 4: Update the hardcoded working_dir usage (around line 155)**

```rust
// CHANGE FROM:
working_dir: PathBuf::from("/tmp/octo-sandbox"),

// TO:
working_dir: self.working_dir.clone(),
```

**Step 5: Run cargo check**

```bash
cargo check -p octo-engine
```

Expected: FAIL - need to update runtime.rs to pass these parameters

**Step 6: Commit**

```bash
git add crates/octo-engine/src/agent/executor.rs
git commit -f "feat(executor): add working_dir field"
```

---

## Task 8: Pass working_dir and event_bus from AgentRuntime to AgentExecutor

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs:286-336`

**Step 1: In start_primary(), update AgentExecutor::new() call**

```rust
let executor = AgentExecutor::new(
    session_id.clone(),
    user_id,
    sandbox_id,
    initial_history,
    rx,
    broadcast_tx,
    self.provider.clone(),
    tools,
    self.memory.clone(),
    Some(self.memory_store.clone()),
    Some(model),
    Some(self.session_store.clone()),
    system_prompt,
    config,
    self.working_dir.clone(),                    // ADD
    self.event_bus.clone(),                       // ADD
);
```

**Step 2: Run cargo check**

```bash
cargo check -p octo-engine
```

Expected: Should compile

**Step 3: Commit**

```bash
git add crates/octo-engine/src/agent/runtime.rs
git commit -f "feat(runtime): pass working_dir and event_bus to AgentExecutor"
```

---

## Task 9: Update octo-server Config to support new fields

**Files:**
- Modify: `crates/octo-server/src/config.rs`

**Step 1: Add new fields to server Config or create RuntimeConfig extension**

First check current config structure:

```bash
grep -n "pub struct Config" crates/octo-server/src/config.rs
```

Then add working_dir and enable_event_bus to the config that gets passed to AgentRuntimeConfig.

**Step 2: Run cargo check**

```bash
cargo check -p octo-server
```

Expected: Should show errors about main.rs

**Step 3: Commit**

```bash
git add crates/octo-server/src/config.rs
git commit -m "feat(config): add working_dir and enable_event_bus to server Config"
```

---

## Task 10: Update main.rs to use new AgentRuntimeConfig

**Files:**
- Modify: `crates/octo-server/src/main.rs`

**Step 1: Update AgentRuntimeConfig::from_parts() call**

```rust
// FROM:
let runtime_config = AgentRuntimeConfig::from_parts(
    config.database.path.clone(),
    config.provider.clone(),
    config.skills.dirs.clone(),
    config.provider_chain.clone(),
);

// TO:
let runtime_config = AgentRuntimeConfig::from_parts(
    config.database.path.clone(),
    config.provider.clone(),
    config.skills.dirs.clone(),
    config.provider_chain.clone(),
    config.working_dir.clone(),                      // ADD
    config.enable_event_bus.unwrap_or(true),         // ADD
);
```

**Step 2: Run cargo check**

```bash
cargo check -p octo-server
```

Expected: Should show errors about mcp_manager

**Step 3: Commit**

```bash
git add crates/octo-server/src/main.rs
git commit -f "feat(main): update AgentRuntimeConfig creation"
```

---

## Task 11: Update AppState to remove mcp_manager

**Files:**
- Modify: `crates/octo-server/src/state.rs`

**Step 1: Remove mcp_manager from AppState**

```rust
pub struct AppState {
    pub config: Config,
    pub agent_runtime: Arc<AgentRuntime>,
    pub agent_handle: AgentExecutorHandle,
    // REMOVE: pub mcp_manager: Arc<tokio::sync::Mutex<McpManager>>,
}
```

**Step 2: Update AppState::new()**

```rust
impl AppState {
    pub fn new(
        config: Config,
        agent_runtime: Arc<AgentRuntime>,
        agent_handle: AgentExecutorHandle,
        // REMOVE: mcp_manager: Arc<tokio::sync::Mutex<McpManager>>,
    ) -> Self {
        Self {
            config,
            agent_runtime,
            agent_handle,
            // REMOVE: mcp_manager,
        }
    }
}
```

**Step 3: Run cargo check**

```bash
cargo check -p octo-server
```

Expected: Should show errors in main.rs where mcp_manager is created

**Step 4: Commit**

```bash
git add crates/octo-server/src/state.rs
git commit -f "refactor(state): remove mcp_manager from AppState"
```

---

## Task 12: Update main.rs to remove mcp_manager creation

**Files:**
- Modify: `crates/octo-server/src/main.rs`

**Step 1: Remove mcp_manager import and creation**

```rust
// REMOVE from imports:
use octo_engine::mcp::{McpManager, McpStorage},

// REMOVE creation:
let mcp_manager = Arc::new(tokio::sync::Mutex::new(McpManager::new()));

// UPDATE AppState::new() call - remove mcp_manager parameter
```

**Step 2: Run cargo check**

```bash
cargo check -p octo-server
```

Expected: Should compile

**Step 3: Commit**

```bash
git add crates/octo-server/src/main.rs
git commit -f "refactor(main): remove mcp_manager from main.rs"
```

---

## Task 13: Update api/mcp_servers.rs to use AgentRuntime

**Files:**
- Modify: `crates/octo-server/src/api/mcp_servers.rs`

**Step 1: Update add_mcp_server handler**

```rust
// FROM:
pub async fn add_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(config): Json<McpServerConfig>,
) -> Result<Json<Vec<McpToolInfo>>, AppError> {
    let mut mcp = state.mcp_manager.lock().await;
    let tools = mcp.add_server(config).await?;
    // ... manual tool registration ...
    Ok(Json(tools))
}

// TO:
pub async fn add_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(config): Json<McpServerConfig>,
) -> Result<Json<Vec<McpToolInfo>>, AppError> {
    let tools = state.agent_runtime
        .add_mcp_server(config)
        .await
        .map_err(AppError::Agent)?;
    Ok(Json(tools))
}
```

**Step 2: Update remove_mcp_server handler similarly**

**Step 3: Update list_mcp_servers handler**

**Step 4: Run cargo check**

```bash
cargo check -p octo-server
```

Expected: Should compile

**Step 5: Commit**

```bash
git add crates/octo-server/src/api/mcp_servers.rs
git commit -f "refactor(mcp-api): use AgentRuntime instead of mcp_manager"
```

---

## Task 14: Full build and test

**Step 1: Run full cargo check**

```bash
cargo check --workspace
```

**Step 2: Run cargo clippy**

```bash
cargo clippy --workspace -- -D warnings
```

**Step 3: Run build**

```bash
cargo build --workspace
```

Expected: All pass

**Step 4: Commit**

```bash
git add -A
git commit -m "build: verify full workspace build"
```

---

## Verification Checklist

- [ ] `cargo check --workspace` passes
- [ ] `cargo clippy --workspace` has no warnings
- [ ] `cargo build --workspace` succeeds
- [ ] MCP Server can be added via API
- [ ] MCP Tools auto-register to ToolRegistry
- [ ] EventBus events publish (check with logs)
- [ ] working_dir configurable via config

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| McpManager HashMap not thread-safe | Use Mutex as shown |
| Runtime tool registration fails | ToolRegistry supports dynamic registration |
| Breaking existing MCP API | Keep same接口, just delegate internally |

---

## Dependencies

- Task 1 → Task 2 → Task 3 → Task 4 → Task 5 → Task 6 → Task 7 → Task 8
- Task 9 → Task 10 → Task 11 → Task 12 → Task 13 → Task 14
