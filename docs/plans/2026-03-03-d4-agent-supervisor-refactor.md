# D4: AgentSupervisor 重构 — 删除 AgentRunner，打通定义层与运行层

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** 将 AgentRunner 的所有有价值职责合并到 AgentSupervisor，使 AgentCatalog（定义层）与 AgentSupervisor（运行层）真正打通，消除 manifest 信息在运行时的丢失，ws.rs 精简为纯消息转发层。

**Architecture:**
- `AgentSupervisor` 新增构造参数：`catalog + provider + tools + memory + skill_registry + ...`，内化所有共享依赖
- `AgentSupervisor.get_or_spawn()` 参数精简为 5 个，内部从 catalog 读取 manifest（system_prompt / tool_filter / AgentConfig）
- `AgentSupervisor` 新增 `start/stop/pause/resume` 方法，真正控制 Runtime 生命周期 + catalog 状态机
- `AgentRunner` 整体删除；`AppState` 直接暴露 `catalog: Arc<AgentCatalog>`
- `agents.rs` REST API 改为访问 `state.catalog` 和 `state.agent_supervisor`

**Tech Stack:** Rust, Tokio (mpsc, broadcast), DashMap, 现有 AgentCatalog / AgentRuntime / AgentLoop / SkillRegistry

---

## 背景：当前问题

```
AgentRunner 持有：catalog + provider + tools + memory + skill_registry
AgentSupervisor 持有：handles: DashMap<SessionId, Handle>

问题：
1. get_or_spawn() 有 10 个参数，ws.rs 每次调用都要注入 6 个依赖
2. AgentRuntime 构建 AgentLoop 时丢失 manifest（无 system_prompt/tool_filter/AgentConfig）
3. AgentRunner.start/stop/pause/resume 只改状态机，不控制真实 Runtime
4. agents.rs 通过 agent_runner.catalog.xxx() 访问，中间多一层
5. ws.rs 承担依赖注入职责，违背"纯 channel 层"设计
```

---

## Task 1: AgentSupervisor 新增构造参数和共享依赖

**目标：** 给 AgentSupervisor 注入所有共享依赖（从 AgentRunner 搬来），`new()` 改为 `new_with_deps()`。

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime_registry.rs`

**Step 1: 读懂现有结构**

```bash
cat crates/octo-engine/src/agent/runtime_registry.rs
cat crates/octo-engine/src/agent/runner.rs
```

**Step 2: 修改 runtime_registry.rs**

在文件顶部 use 段添加：
```rust
use crate::agent::{AgentCatalog, AgentConfig, AgentId, AgentManifest, CancellationToken};
use crate::context::SystemPromptBuilder;
use crate::event::EventBus;
use crate::session::SessionStore;
use crate::skills::{SkillRegistry, SkillTool};
use crate::tools::recorder::ToolExecutionRecorder;
```

将 `AgentSupervisor` 结构体改为：
```rust
/// Session → AgentRuntimeHandle 的注册表，同时持有所有共享运行时依赖
pub struct AgentSupervisor {
    handles: DashMap<SessionId, AgentRuntimeHandle>,
    // 定义层
    pub catalog: Arc<AgentCatalog>,
    // 共享依赖（构造时注入一次）
    provider: Arc<dyn crate::providers::Provider>,
    tools: Arc<ToolRegistry>,
    skill_registry: Option<Arc<SkillRegistry>>,
    memory: Arc<dyn WorkingMemory>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    session_store: Option<Arc<dyn SessionStore>>,
    default_model: String,
    event_bus: Option<Arc<EventBus>>,
    recorder: Option<Arc<ToolExecutionRecorder>>,
}
```

**Step 3: 添加构造函数**

```rust
impl AgentSupervisor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        catalog: Arc<AgentCatalog>,
        provider: Arc<dyn crate::providers::Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        default_model: String,
    ) -> Self {
        Self {
            handles: DashMap::new(),
            catalog,
            provider,
            tools,
            skill_registry: None,
            memory,
            memory_store: None,
            session_store: None,
            default_model,
            event_bus: None,
            recorder: None,
        }
    }

    pub fn with_skill_registry(mut self, skills: Arc<SkillRegistry>) -> Self {
        self.skill_registry = Some(skills);
        self
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn with_session_store(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_recorder(mut self, recorder: Arc<ToolExecutionRecorder>) -> Self {
        self.recorder = Some(recorder);
        self
    }
```

**Step 4: 删除旧的 `new()` 无参构造（如果存在），保留 Default impl 调用新 new()**

注意：旧的 `AgentSupervisor::new()` 是无参的，新版需要必填参数，**删除 Default impl**（或改为 panic，防止误用）。

**Step 5: 编译验证（预期失败，因为 state.rs 还在用旧接口）**
```bash
cargo check -p octo-engine 2>&1 | head -30
```

**Step 6: Commit**
```bash
git add crates/octo-engine/src/agent/runtime_registry.rs
git commit -m "refactor(supervisor): add shared deps to AgentSupervisor constructor"
```

---

## Task 2: AgentSupervisor 内化 build_tool_registry 和 build_system_prompt

**目标：** 将 AgentRunner 中的 `build_tool_registry()` 和 `build_system_prompt()` 搬入 AgentSupervisor 作为私有方法。

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime_registry.rs`

**Step 1: 添加私有方法**

在 `impl AgentSupervisor` 块中添加以下私有方法：

```rust
    /// 按 tool_filter 构建 ToolRegistry（含 SkillRegistry 热重载 overlay）
    fn build_tool_registry(&self, tool_filter: &[String]) -> Arc<ToolRegistry> {
        // 快速路径：无动态 skills 且无 filter
        if self.skill_registry.is_none() && tool_filter.is_empty() {
            return self.tools.clone();
        }

        // 从全局工具快照构建
        let mut registry = ToolRegistry::new();
        for (name, tool) in self.tools.iter() {
            registry.register_arc(name.clone(), tool);
        }

        // 覆盖当前热重载的 skill tools
        if let Some(ref skills) = self.skill_registry {
            for skill in skills.invocable_skills() {
                let name = skill.name.clone();
                registry.register_arc(name, std::sync::Arc::new(SkillTool::new(skill)));
            }
        }

        // 应用 per-agent tool filter
        if tool_filter.is_empty() {
            return Arc::new(registry);
        }
        let mut filtered = ToolRegistry::new();
        for name in tool_filter {
            if let Some(tool) = registry.get(name) {
                filtered.register_arc(name.clone(), tool);
            }
        }
        Arc::new(filtered)
    }

    /// 从 AgentManifest 构建 system prompt
    fn build_system_prompt(manifest: &AgentManifest) -> Option<String> {
        if let Some(ref prompt) = manifest.system_prompt {
            return Some(prompt.clone());
        }
        if manifest.role.is_some() || manifest.goal.is_some() || manifest.backstory.is_some() {
            let mut parts: Vec<String> = Vec::new();
            if let Some(ref role) = manifest.role {
                parts.push(format!("## Role\n{role}"));
            }
            if let Some(ref goal) = manifest.goal {
                parts.push(format!("## Goal\n{goal}"));
            }
            if let Some(ref backstory) = manifest.backstory {
                parts.push(format!("## Backstory\n{backstory}"));
            }
            return Some(parts.join("\n\n"));
        }
        None  // 返回 None 表示使用 AgentLoop 默认（SOUL.md）
    }

    /// 按 agent_id 解析运行时配置（从 catalog 读取 manifest）
    fn resolve_runtime_config(
        &self,
        agent_id: Option<&AgentId>,
    ) -> (Arc<ToolRegistry>, Option<String>, String, AgentConfig) {
        if let Some(id) = agent_id {
            if let Some(entry) = self.catalog.get(id) {
                let manifest = &entry.manifest;
                let tools = self.build_tool_registry(&manifest.tool_filter);
                let system_prompt = Self::build_system_prompt(manifest);
                let model = manifest.model.clone().unwrap_or_else(|| self.default_model.clone());
                let config = manifest.config.clone();
                return (tools, system_prompt, model, config);
            }
        }
        // 无 agent_id 或 agent 不存在：使用全局默认
        (
            self.tools.clone(),
            None,
            self.default_model.clone(),
            AgentConfig::default(),
        )
    }
```

**Step 2: 编译验证**
```bash
cargo check -p octo-engine 2>&1 | head -30
```

**Step 3: Commit**
```bash
git add crates/octo-engine/src/agent/runtime_registry.rs
git commit -m "refactor(supervisor): internalize build_tool_registry and build_system_prompt"
```

---

## Task 3: AgentSupervisor.get_or_spawn() 精简为 5 个参数

**目标：** `get_or_spawn()` 签名精简，内部自行从依赖和 manifest 解析运行时配置。同时让 AgentRuntime 接收 manifest 配置（system_prompt / AgentConfig）。

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime_registry.rs`
- Modify: `crates/octo-engine/src/agent/runtime.rs`

**Step 1: 扩展 AgentRuntime 持有 manifest 配置**

在 `crates/octo-engine/src/agent/runtime.rs` 的 `AgentRuntime` struct 添加字段：
```rust
// manifest 配置（来自 AgentCatalog）
system_prompt: Option<String>,
config: AgentConfig,
```

在 `AgentRuntime::new()` 参数末尾添加：
```rust
system_prompt: Option<String>,
config: AgentConfig,
```

在 `Self { ... }` 初始化中同步添加这两个字段。

**Step 2: AgentRuntime::run() 中将 manifest 配置传给 AgentLoop**

在 `runtime.rs` 的 `run()` 方法中，构建 AgentLoop 后添加：
```rust
// 注入 manifest 配置
if let Some(ref prompt) = self.system_prompt {
    agent_loop = agent_loop.with_system_prompt(prompt.clone());
}
agent_loop = agent_loop.with_config(self.config.clone());

// 注入 event_bus（如果有）
// 注入 recorder（如果有）
```

注意：AgentRuntime 目前没有 event_bus 和 recorder 字段，先只处理 system_prompt 和 config，event_bus/recorder 在后续 Task 中处理。

**Step 3: AgentSupervisor.get_or_spawn() 改造**

将旧的 10 参数版本替换为：

```rust
/// 获取或 spawn 与 session 绑定的 AgentRuntime。
/// agent_id: 可选，指定要绑定的 AgentCatalog 中的 agent 定义（携带 manifest）。
pub fn get_or_spawn(
    &self,
    session_id: SessionId,
    user_id: UserId,
    sandbox_id: SandboxId,
    initial_history: Vec<ChatMessage>,
    agent_id: Option<&AgentId>,
) -> AgentRuntimeHandle {
    // 已有 handle 则直接复用
    if let Some(handle) = self.get(&session_id) {
        return handle;
    }

    // 从 manifest 解析运行时配置
    let (tools, system_prompt, model, config) = self.resolve_runtime_config(agent_id);

    let (tx, rx) = mpsc::channel::<AgentMessage>(MPSC_CAPACITY);
    let (broadcast_tx, _) = broadcast::channel::<AgentEvent>(BROADCAST_CAPACITY);

    let handle = AgentRuntimeHandle {
        tx,
        broadcast_tx: broadcast_tx.clone(),
        session_id: session_id.clone(),
    };

    let runtime = AgentRuntime::new(
        session_id.clone(),
        user_id,
        sandbox_id,
        initial_history,
        rx,
        broadcast_tx,
        self.provider.clone(),
        tools,
        self.memory.clone(),
        self.memory_store.clone(),
        Some(model),
        self.session_store.clone(),
        system_prompt,   // ← 新增
        config,          // ← 新增
    );

    tokio::spawn(async move {
        runtime.run().await;
    });

    if let Some(id) = agent_id {
        let cancel_token = CancellationToken::new();
        let _ = self.catalog.mark_running(id, cancel_token);
    }

    info!(session_id = %session_id.as_str(), "AgentRuntime spawned");
    self.handles.insert(session_id, handle.clone());
    handle
}
```

**Step 4: 编译验证**
```bash
cargo check -p octo-engine 2>&1 | head -40
```

**Step 5: Commit**
```bash
git add crates/octo-engine/src/agent/runtime.rs \
        crates/octo-engine/src/agent/runtime_registry.rs
git commit -m "refactor(supervisor): simplify get_or_spawn to 5 params, pass manifest config to AgentRuntime"
```

---

## Task 4: AgentSupervisor 添加 start/stop/pause/resume 方法

**目标：** 生命周期方法从 AgentRunner 搬入 AgentSupervisor，`start()` 真正 spawn AgentRuntime；`stop()` 真正关闭 Runtime。

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime_registry.rs`

**Step 1: 添加生命周期方法**

```rust
    /// 启动 agent：从 catalog 读取 manifest，spawn AgentRuntime，更新状态机。
    /// session_id：为该 agent 创建或复用的会话标识。
    pub async fn start(
        &self,
        agent_id: &AgentId,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
    ) -> Result<AgentRuntimeHandle, crate::agent::AgentError> {
        // 验证 agent 存在
        self.catalog
            .get(agent_id)
            .ok_or_else(|| crate::agent::AgentError::NotFound(agent_id.clone()))?;

        // spawn Runtime（内部会调用 catalog.mark_running）
        let handle = self.get_or_spawn(
            session_id,
            user_id,
            sandbox_id,
            initial_history,
            Some(agent_id),
        );

        Ok(handle)
    }

    /// 停止 agent：发送 Cancel，移除 handle，更新 catalog 状态。
    pub async fn stop(&self, agent_id: &AgentId, session_id: &SessionId) -> Result<(), crate::agent::AgentError> {
        // 发 Cancel 给 Runtime
        if let Some(handle) = self.get(session_id) {
            let _ = handle.send(AgentMessage::Cancel).await;
        }
        self.remove(session_id);
        self.catalog.mark_stopped(agent_id)
    }

    /// 暂停 agent：发送 Cancel（中断当前 round），更新 catalog 状态。
    pub async fn pause(&self, agent_id: &AgentId, session_id: &SessionId) -> Result<(), crate::agent::AgentError> {
        if let Some(handle) = self.get(session_id) {
            let _ = handle.send(AgentMessage::Cancel).await;
        }
        self.catalog.mark_paused(agent_id)
    }

    /// 恢复 agent：更新 catalog 状态（Runtime 仍在运行，cancel_flag 已重置）。
    pub async fn resume(&self, agent_id: &AgentId) -> Result<(), crate::agent::AgentError> {
        let cancel_token = CancellationToken::new();
        self.catalog.mark_resumed(agent_id, cancel_token)
    }
```

**Step 2: 编译验证**
```bash
cargo check -p octo-engine 2>&1 | head -30
```

**Step 3: Commit**
```bash
git add crates/octo-engine/src/agent/runtime_registry.rs
git commit -m "feat(supervisor): add real start/stop/pause/resume controlling AgentRuntime lifecycle"
```

---

## Task 5: 更新 AppState — 删除 agent_runner，直接暴露 catalog

**目标：** AppState 删除 `agent_runner` 字段，添加 `catalog: Arc<AgentCatalog>`，`agent_supervisor` 通过新接口初始化。

**Files:**
- Modify: `crates/octo-server/src/state.rs`
- Modify: `crates/octo-server/src/main.rs`

**Step 1: 修改 state.rs**

将：
```rust
use octo_engine::{
    AgentRunner, AgentSupervisor, ...
};
pub struct AppState {
    ...
    pub agent_runner: Arc<AgentRunner>,
    pub agent_supervisor: Arc<AgentSupervisor>,
}
```

改为：
```rust
use octo_engine::{
    AgentCatalog, AgentSupervisor, ...  // 删除 AgentRunner
};
pub struct AppState {
    ...
    pub catalog: Arc<AgentCatalog>,          // ← 新增，取代 agent_runner.catalog
    pub agent_supervisor: Arc<AgentSupervisor>,
    // 删除 agent_runner 字段
}
```

同步更新 `AppState::new()` 参数：删除 `agent_runner: Arc<AgentRunner>`，改为：
```rust
pub fn new(
    ...
    // 删除 agent_runner 参数
    catalog: Arc<AgentCatalog>,
    agent_supervisor: Arc<AgentSupervisor>,
) -> Self {
    // 删除 let agent_supervisor = Arc::new(AgentSupervisor::new());
    Self {
        ...
        catalog,
        agent_supervisor,
    }
}
```

**Step 2: 修改 main.rs**

将：
```rust
let agent_runner = Arc::new(AgentRunner::new(
    agent_catalog,
    provider.clone(),
    tools.clone(),
    memory.clone(),
    default_model,
).with_skill_registry(skill_registry.clone()));

let state = Arc::new(AppState::new(
    ...
    agent_runner,
));
```

改为：
```rust
let agent_supervisor = Arc::new(
    AgentSupervisor::new(
        agent_catalog.clone(),
        provider.clone(),
        tools.clone(),
        memory.clone(),
        default_model,
    )
    .with_skill_registry(skill_registry.clone())
    .with_memory_store(memory_store.clone())
    .with_session_store(sessions.clone())
    .with_recorder(recorder.clone()),
);

let state = Arc::new(AppState::new(
    ...
    agent_catalog,        // ← catalog 直接传入 AppState
    agent_supervisor,
));
```

**Step 3: 编译（预期有错误，agents.rs 和 ws.rs 仍引用旧接口）**
```bash
cargo check -p octo-server 2>&1 | head -50
```

**Step 4: Commit（中间状态，后续 Task 修复编译错误）**
```bash
git add crates/octo-server/src/state.rs crates/octo-server/src/main.rs
git commit -m "refactor(state): remove AgentRunner, expose AgentCatalog directly, init AgentSupervisor with deps"
```

---

## Task 6: 更新 agents.rs REST API

**目标：** 将 `agents.rs` 中的 `s.agent_runner.catalog.xxx` 改为 `s.catalog.xxx`；`start/stop/pause/resume` 改为调用 `s.agent_supervisor` 的新方法。

**Files:**
- Modify: `crates/octo-server/src/api/agents.rs`

**Step 1: 读取当前 agents.rs 的全部调用**

```bash
grep -n "agent_runner\|agent_supervisor" crates/octo-server/src/api/agents.rs
```

**Step 2: 替换 catalog 访问**

所有 `s.agent_runner.catalog.` → `s.catalog.`

具体替换：
```rust
// list_agents
Json(s.catalog.list_all())

// create_agent
let id = s.catalog.register(manifest);
let entry = s.catalog.get(&id).unwrap();

// get_agent
s.catalog.get(&AgentId(id)).map(Json).ok_or(StatusCode::NOT_FOUND)

// delete_agent
if s.catalog.unregister(&AgentId(id)).is_some() { ... }
```

**Step 3: 更新生命周期方法调用**

`start/stop/pause/resume` 需要 session_id。当前 REST API 没有传 session_id，需要决策：

**方案：** `start` 时由 server 为该 agent 创建一个新 session：
```rust
async fn start_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    // 为 agent 创建专属 session
    let session = s.sessions.create_session().await;
    let initial_history = vec![];
    s.agent_supervisor
        .start(
            &agent_id,
            session.session_id,
            session.user_id,
            session.sandbox_id,
            initial_history,
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    s.catalog.get(&agent_id).map(Json).ok_or(StatusCode::NOT_FOUND)
}
```

`stop/pause/resume` 暂时简化：通过 catalog 更新状态即可（AgentRuntime 由消息触发，REST API 的 pause/resume 控制语义稍后完善）：
```rust
async fn stop_agent(...) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.catalog.mark_stopped(&agent_id).map_err(|_| StatusCode::CONFLICT)?;
    s.catalog.get(&agent_id).map(Json).ok_or(StatusCode::NOT_FOUND)
}
```

**Step 4: 编译验证**
```bash
cargo check -p octo-server 2>&1 | head -40
```

**Step 5: Commit**
```bash
git add crates/octo-server/src/api/agents.rs
git commit -m "refactor(agents-api): use state.catalog directly, update lifecycle to use agent_supervisor"
```

---

## Task 7: 精简 ws.rs 为纯 channel 层

**目标：** 删除 ws.rs 中对 `agent_runner` 的 3 行依赖注入调用，改为 `agent_supervisor.get_or_spawn()` 新接口（5 个参数）。

**Files:**
- Modify: `crates/octo-server/src/ws.rs`

**Step 1: 找到需要修改的区域**

```bash
grep -n "agent_runner\|agent_supervisor\|get_or_spawn" crates/octo-server/src/ws.rs
```

当前（约第 215-233 行）：
```rust
let handle = state.agent_supervisor.get_or_spawn(
    session.session_id.clone(),
    session.user_id.clone(),
    session.sandbox_id.clone(),
    initial_history,
    state.agent_runner.provider(),           // ← 删除
    state.agent_runner.build_tool_registry(&[]),  // ← 删除
    state.agent_runner.memory(),             // ← 删除
    Some(state.memory_store.clone()),        // ← 删除
    state.model.clone(),                     // ← 删除
    Some(state.sessions.clone()),            // ← 删除
);
```

**Step 2: 替换为新接口**

```rust
let handle = state.agent_supervisor.get_or_spawn(
    session.session_id.clone(),
    session.user_id.clone(),
    session.sandbox_id.clone(),
    initial_history,
    None,  // 无绑定的 agent_id（使用默认配置）
);
```

**Step 3: 删除 ws.rs 中不再使用的 import（如有）**

```bash
cargo check -p octo-server 2>&1 | grep "unused import"
```

按提示删除对应 use 行。

**Step 4: 编译验证**
```bash
cargo check -p octo-server 2>&1 | head -20
```

**Step 5: Commit**
```bash
git add crates/octo-server/src/ws.rs
git commit -m "refactor(ws): simplify get_or_spawn call, ws.rs is now pure channel layer"
```

---

## Task 8: 删除 AgentRunner

**目标：** 彻底删除 `runner.rs` 文件，从 `mod.rs` 和 `lib.rs` 移除导出。

**Files:**
- Delete: `crates/octo-engine/src/agent/runner.rs`
- Modify: `crates/octo-engine/src/agent/mod.rs`
- Modify: `crates/octo-engine/src/lib.rs`

**Step 1: 确认无引用残留**

```bash
grep -rn "AgentRunner\|agent_runner" crates/ --include="*.rs" | grep -v "target/"
```

预期：**零输出**（若有，先修复）

**Step 2: 从 mod.rs 移除**

删除 `crates/octo-engine/src/agent/mod.rs` 中的：
```rust
pub mod runner;
pub use runner::AgentRunner;
```

**Step 3: 从 lib.rs 移除**

在 `crates/octo-engine/src/lib.rs` 的 agent pub use 行中删除 `AgentRunner`。

**Step 4: 删除文件**

```bash
rm crates/octo-engine/src/agent/runner.rs
```

**Step 5: 完整编译验证**

```bash
cargo build --workspace 2>&1 | grep -E "^error" | head -20
```

预期：**零 error**

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor(agent): delete AgentRunner - responsibilities merged into AgentSupervisor"
```

---

## Task 9: 全量编译 + 集成验证

**目标：** 确保整个工作区编译通过，并验证架构正确性。

**Step 1: 完整构建**

```bash
cargo build --workspace 2>&1 | tail -5
```

预期：`Finished` 无 error

**Step 2: 运行现有测试**

```bash
cargo test --workspace 2>&1 | tail -20
```

**Step 3: 架构语义检查**

```bash
# AgentRunner 已完全消失
grep -rn "AgentRunner" crates/ --include="*.rs" | grep -v target
# 预期：零结果

# AgentSupervisor 现在持有 catalog
grep -n "pub catalog" crates/octo-engine/src/agent/runtime_registry.rs
# 预期：有输出

# ws.rs 不再引用 agent_runner
grep -n "agent_runner" crates/octo-server/src/ws.rs
# 预期：零结果
```

**Step 4: Final Commit**

```bash
git add -A
git commit -m "checkpoint: D4 complete - AgentSupervisor unified, AgentRunner deleted"
```

---

## 关键设计决策备忘

| 决策 | 选择 | 理由 |
|------|------|------|
| AgentCatalog 归属 | AppState 直接持有 + AgentSupervisor 共享同一 Arc | agents.rs 需要直接 CRUD，supervisor 需要读 manifest |
| get_or_spawn agent_id 参数 | Option<&AgentId> | ws.rs 普通对话传 None（默认配置），REST API start 时传 Some |
| start() 的 session_id 来源 | server 为 agent 创建专属 session | agent 不应与 WebSocket session 耦合，独立生命周期 |
| stop/pause via REST | 直接调 catalog.mark_xxx | 简化实现，完整控制可后续通过 session_id 注册表实现 |
| AgentRunner 生命周期方法 | 迁移到 AgentSupervisor，不保留 AgentRunner | AgentSupervisor 现在有完整上下文（catalog + handle），才能真正控制 |
| Default impl | 删除 AgentSupervisor::Default | 有必填参数，不允许"空" supervisor 存在 |
