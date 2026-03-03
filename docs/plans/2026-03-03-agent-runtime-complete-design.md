# AgentRuntime 完整重构实施方案

> 本文档指导 AgentRuntime 完全内化所有组件，实现设计文档中的完整目标

## 背景

根据架构分析，当前实现与设计文档存在以下差距：

| 问题 | 当前状态 | 目标状态 |
|------|----------|----------|
| McpManager | 在 AppState 中独立管理 | 移入 AgentRuntime |
| EventBus | 未接入（传 None） | 默认启用，完全接入 |
| working_dir | hardcoded `/tmp/octo-sandbox` | 可配置 |
| main.rs | 仍有部分初始化逻辑 | 只传 Config |

---

## 架构目标

```
┌─────────────────────────────────────────────────────────────────┐
│                         main.rs                                  │
│  Config::load() → AgentRuntime::new(config) → AppState         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      AgentRuntime                                │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌─────────────┐  ┌──────────────────────┐ │
│  │ AgentCatalog │  │  Provider   │  │    ToolRegistry       │ │
│  └──────────────┘  └─────────────┘  └──────────────────────┘ │
│  ┌──────────────┐  ┌─────────────┐  ┌──────────────────────┐ │
│  │    Memory    │  │   Skills    │  │    ProviderChain     │ │
│  └──────────────┘  └─────────────┘  └──────────────────────┘ │
│  ┌──────────────┐  ┌─────────────┐  ┌──────────────────────┐ │
│  │   Sessions  │  │  EventBus   │  │    McpManager        │ │
│  └──────────────┘  └─────────────┘  └──────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

---

## 详细设计

### 1. AgentRuntimeConfig 扩展

```rust
// crates/octo-engine/src/agent/runtime.rs

#[derive(Debug, Clone)]
pub struct AgentRuntimeConfig {
    pub db_path: String,
    pub provider: ProviderConfig,
    pub skills_dirs: Vec<String>,
    pub provider_chain: Option<ProviderChainConfig>,
    pub working_dir: Option<PathBuf>,  // 新增：可配置工作目录
    pub enable_event_bus: bool,        // 新增：是否启用 EventBus
}

impl AgentRuntimeConfig {
    pub fn from_parts(
        db_path: String,
        provider: ProviderConfig,
        skills_dirs: Vec<String>,
        provider_chain: Option<ProviderChainConfig>,
        working_dir: Option<PathBuf>,
        enable_event_bus: bool,
    ) -> Self {
        Self {
            db_path,
            provider,
            skills_dirs,
            provider_chain,
            working_dir,
            enable_event_bus,
        }
    }
}
```

### 2. AgentRuntime 新增字段

```rust
pub struct AgentRuntime {
    // 现有字段...
    primary_handle: Mutex<Option<AgentExecutorHandle>>,
    agent_handles: DashMap<AgentId, CancellationToken>,
    catalog: Arc<AgentCatalog>,
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    skill_registry: Option<Arc<SkillRegistry>>,
    memory: Arc<dyn WorkingMemory>,
    memory_store: Arc<dyn MemoryStore>,
    session_store: Arc<dyn SessionStore>,
    default_model: String,
    event_bus: Option<Arc<EventBus>>,      // 改为默认启用
    recorder: Arc<ToolExecutionRecorder>,
    provider_chain: Option<Arc<ProviderChain>>,

    // 新增字段
    mcp_manager: Option<Arc<tokio::sync::Mutex<McpManager>>>,  // 新增
    working_dir: PathBuf,                                      // 新增
}
```

### 3. McpManager 内化 API

```rust
impl AgentRuntime {
    /// 添加 MCP Server → 自动注册 tools
    pub async fn add_mcp_server(
        &self,
        config: McpServerConfig,
    ) -> Result<Vec<McpToolInfo>, AgentError> {
        let mcp = self.mcp_manager.as_ref().ok_or(
            AgentError::McpNotInitialized
        )?;

        let mut guard = mcp.lock().await;
        let tools = guard.add_server(config).await.map_err(|e|
            AgentError::McpError(e.to_string())
        )?;

        // 注册到 ToolRegistry
        for tool_info in &tools {
            let bridge = McpToolBridge::new(tool_info.clone());
            self.tools.register(bridge);
        }

        Ok(tools)
    }

    /// 移除 MCP Server → 自动注销 tools
    pub async fn remove_mcp_server(
        &self,
        name: &str,
    ) -> Result<(), AgentError> {
        let mcp = self.mcp_manager.as_ref().ok_or(
            AgentError::McpNotInitialized
        )?;

        let mut guard = mcp.lock().await;
        let removed_tools = guard.remove_server(name).await.map_err(|e|
            AgentError::McpError(e.to_string())
        )?;

        // 从 ToolRegistry 注销
        for tool in removed_tools {
            self.tools.unregister(&tool.name);
        }

        Ok(())
    }

    /// 列出运行中的 MCP servers
    pub fn list_mcp_servers(&self) -> Vec<McpServerStatus> {
        // 返回服务器状态列表
    }

    /// 获取 MCP Manager 引用（供 API 层内部使用）
    pub fn mcp_manager(
        &self,
    ) -> Option<&Arc<tokio::sync::Mutex<McpManager>>> {
        self.mcp_manager.as_ref()
    }
}
```

### 4. AgentError 扩展

```rust
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    // 现有错误...

    #[error("MCP manager not initialized")]
    McpNotInitialized,

    #[error("MCP error: {0}")]
    McpError(String),

    #[error("MCP server not found: {0}")]
    McpServerNotFound(String),
}
```

### 5. EventBus 接入

```rust
impl AgentRuntime {
    pub async fn new(
        catalog: Arc<AgentCatalog>,
        config: AgentRuntimeConfig,
    ) -> Result<Self, AgentError> {
        // ... 现有初始化 ...

        // EventBus 初始化（默认启用）
        let event_bus = if config.enable_event_bus {
            Some(Arc::new(EventBus::new(
                1000,
                1000,
                Arc::new(MetricsRegistry::new()),
            )))
        } else {
            None
        };

        // McpManager 初始化
        let mcp_manager = Some(Arc::new(tokio::sync::Mutex::new(
            McpManager::new()
        )));

        // Working directory
        let working_dir = config.working_dir
            .unwrap_or_else(|| PathBuf::from("/tmp/octo-sandbox"));

        Ok(Self {
            // ... existing fields ...
            event_bus,
            mcp_manager,
            working_dir,
        })
    }
}
```

### 6. working_dir 传递给 AgentExecutor

```rust
impl AgentRuntime {
    pub async fn start_primary(
        &self,
        // ... existing params
    ) -> AgentExecutorHandle {
        // ... existing logic ...

        let executor = AgentExecutor::new(
            // ... existing params ...
            working_dir: self.working_dir.clone(),  // 新增
            event_bus: self.event_bus.clone(),       // 新增
        );

        // ... rest unchanged ...
    }
}
```

---

## main.rs 变更

### 变更前

```rust
// main.rs (当前)
let config = Config::load(...)?;
let db = Database::open(&config.database.path).await?;
let conn = db.conn().clone();

// AgentStore & Catalog
let store = AgentStore::new(conn.clone());
let catalog = Arc::new(AgentCatalog::new(store));

// AgentRuntimeConfig
let runtime_config = AgentRuntimeConfig::from_parts(
    config.database.path.clone(),
    config.provider.clone(),
    config.skills.dirs.clone(),
    config.provider_chain.clone(),
);

// AgentRuntime
let agent_runtime = Arc::new(
    AgentRuntime::new(catalog.clone(), runtime_config).await?
);

// McpManager (独立创建)
let mcp_manager = Arc::new(tokio::sync::Mutex::new(McpManager::new()));

// AppState
let state = AppState::new(
    config,
    agent_runtime,
    mcp_manager,  // ❌ 独立传递
    // ...
);
```

### 变更后

```rust
// main.rs (目标)
let config = Config::load(...)?;

// AgentStore & Catalog (仍需外部创建，因为 catalog 需暴露给 API)
let db = Database::open(&config.database.path).await?;
let conn = db.conn().clone();
let store = AgentStore::new(conn.clone());
let catalog = Arc::new(AgentCatalog::new(store));

// AgentRuntimeConfig (完整配置)
let runtime_config = AgentRuntimeConfig::from_parts(
    config.database.path.clone(),
    config.provider.clone(),
    config.skills.dirs.clone(),
    config.provider_chain.clone(),
    config.working_dir.clone(),           // 新增
    config.enable_event_bus.unwrap_or(true), // 新增
);

// AgentRuntime (完全内化)
let agent_runtime = Arc::new(
    AgentRuntime::new(catalog.clone(), runtime_config).await?
);

// AppState (简化)
let state = AppState::new(
    config,
    agent_runtime,
    // 不再需要单独传 mcp_manager
);
```

### AppState 简化

```rust
// state.rs
pub struct AppState {
    pub config: Config,
    pub agent_runtime: Arc<AgentRuntime>,  // ✅ 内含 McpManager
    pub agent_handle: AgentExecutorHandle,
    // 移除了 mcp_manager 字段
}
```

---

## API 层变更

### api/mcp_servers.rs

```rust
// 变更前
pub async fn add_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(config): Json<McpServerConfig>,
) -> Result<Json<Vec<McpToolInfo>>, AppError> {
    let mut mcp = state.mcp_manager.lock().await;
    let tools = mcp.add_server(config).await?;
    // 手动注册到 tools...
    Ok(Json(tools))
}

// 变更后
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

---

## 实施步骤

| 步骤 | 任务 | 修改文件 | 预估复杂度 |
|------|------|----------|------------|
| 1 | AgentRuntimeConfig 添加新字段 | `runtime.rs`, `config.rs` | 低 |
| 2 | AgentRuntime 添加 mcp_manager 字段 | `runtime.rs` | 低 |
| 3 | 实现 add/remove/list_mcp_servers API | `runtime.rs` | 中 |
| 4 | AgentError 添加 MCP 相关错误 | `error.rs` | 低 |
| 5 | new() 中初始化 McpManager | `runtime.rs` | 低 |
| 6 | new() 中启用 EventBus | `runtime.rs` | 低 |
| 7 | working_dir 可配置化 | `runtime.rs`, `executor.rs` | 低 |
| 8 | start_primary 传递新参数 | `runtime.rs` | 低 |
| 9 | 更新 main.rs | `main.rs` | 中 |
| 10 | 更新 AppState | `state.rs` | 低 |
| 11 | 更新 api/mcp_servers.rs | `api/mcp_servers.rs` | 中 |
| 12 | 编译验证 | - | - |
| 13 | 运行时测试 | - | - |

---

## 验证标准

### 编译验证
- [ ] `cargo check` 通过
- [ ] `cargo clippy` 无警告

### 功能验证
- [ ] MCP Server 可通过 API 添加/删除
- [ ] MCP Tools 自动注册到 ToolRegistry
- [ ] EventBus 事件正常发布
- [ ] working_dir 可通过配置指定

### 回归验证
- [ ] 现有 agent 生命周期 API 正常（start/stop/pause/resume）
- [ ] WebSocket 消息通信正常
- [ ] Scheduler 触发正常

---

## 风险与回滚

### 风险
- McpManager 内部使用 `HashMap` 非线程安全，需要 Mutex 保护
- 运行时添加 MCP Server 需要 ToolRegistry 支持动态注册（已支持）

### 回滚计划
- 如有问题，可通过 Feature Flag 禁用 McpManager 内化
- 保持 `AgentRuntime::new_legacy()` 备用

---

## 备注

- 本方案遵循设计文档《AgentRuntime 重构设计文档》的完整目标
- McpManager 内化后，MCP Server 的生命周期由 AgentRuntime 管理
- EventBus 默认启用，可通过配置关闭（用于调试）
