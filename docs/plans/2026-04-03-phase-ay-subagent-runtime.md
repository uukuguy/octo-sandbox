# Phase AY — SubAgent Runtime 完整生命周期

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 SubAgent 从 "配置不同的一次 LLM 调用" 升级为 "有完整生命周期的运行时实体"。对齐 CC-OSS 的 AgentTool + runAgent 架构模式。

**Architecture:** 引入 SubAgentRuntime 结构体封装 sub-agent 的全部状态和资源。AgentTool（原 SpawnSubAgentTool）变成调度总控，不再直接操作 run_agent_loop()。

**Tech Stack:** Rust, tokio, octo-engine (SubAgentRuntime, AgentTool, AgentCatalog, run_agent_loop)

---

## 核心架构决策

### ADR: Agent vs Skill 边界

- **Agent** = 有完整生命周期的隔离执行体（独立上下文、工具、模型、资源管理）
- **Skill** = 行为指令（Knowledge 模式注入 prompt，Playbook 模式委托给 SubAgentRuntime）
- **Builtin agents** 只在代码中定义（`builtin_agents.rs`），不用 YAML 配置文件
- **用户自定义 agent** 通过 Playbook skill 实现
- `agents/` YAML 目录回退删除（AX-D5 回退）

### ADR: SubAgent 生命周期模型

CC-OSS 的 runAgent() 生命周期：
```
Build (资源初始化) → Run (执行循环) → Cleanup (资源释放)
```

Octo 对标实现：
```
SubAgentRuntime::build() → run_sync() / run_async() → Drop (自动清理)
```

### ADR: 工具重命名

| 旧名 | 新名 | 原因 |
|------|------|------|
| `spawn_subagent` | `agent` | 与 CC-OSS 的 Agent tool 对齐，更简洁 |
| `query_subagent` | `query_agent` | 配套重命名 |
| `SpawnSubAgentTool` | `AgentTool` | 结构体重命名 |
| `QuerySubAgentTool` | `QueryAgentTool` | 结构体重命名 |

---

## Wave 1: SubAgentRuntime 核心 + 工具重命名

### T1: SubAgentRuntime 结构体 + build()

**Files:**
- Create: `crates/octo-engine/src/agent/subagent_runtime.rs`
- Modify: `crates/octo-engine/src/agent/mod.rs`

**SubAgentRuntime 设计：**

```rust
//! SubAgent Runtime — 封装 sub-agent 的完整生命周期。
//!
//! 对标 CC-OSS runAgent()。每个 sub-agent 是一个独立的运行时实体，
//! 有自己的资源、上下文和清理逻辑。

pub struct SubAgentRuntime {
    // 身份
    pub id: String,
    pub agent_type: Option<String>,
    pub manifest: Option<AgentManifest>,

    // 执行配置（build 阶段构建）
    config: AgentLoopConfig,
    task: String,

    // 父 agent 连接
    manager: Arc<SubAgentManager>,
    event_sender: Option<broadcast::Sender<AgentEvent>>,
}

pub struct SubAgentResult {
    pub id: String,
    pub output: String,
    pub rounds: u32,
    pub status: SubAgentStatus,
}

impl SubAgentRuntime {
    /// 构建运行时：manifest → 资源初始化 → config
    pub async fn build(
        task: String,
        manifest: Option<AgentManifest>,
        parent_config: &AgentLoopConfig,
        manager: Arc<SubAgentManager>,
        event_sender: Option<broadcast::Sender<AgentEvent>>,
    ) -> Result<Self> {
        // 1. 检查递归深度
        let child_mgr = Arc::new(manager.child()?);
        if !manager.can_spawn().await {
            bail!("Maximum concurrent sub-agents reached");
        }

        // 2. 生成 ID
        let id = format!("agent-{}", uuid::Uuid::new_v4());
        let agent_type = manifest.as_ref().map(|m| m.name.clone());

        // 3. 注册
        let desc = if let Some(ref m) = manifest {
            format!("[{}] {}", m.name, &task[..task.len().min(80)])
        } else {
            task.clone()
        };
        manager.register(id.clone(), desc).await?;

        // 4. 构建工具集：manifest (whitelist + blacklist) → parent
        let tools = Self::resolve_tools(&manifest, parent_config);

        // 5. 解析模型
        let model = manifest.as_ref()
            .and_then(|m| m.model.as_ref())
            .filter(|m| m.as_str() != "inherit")
            .cloned()
            .unwrap_or_else(|| parent_config.model.clone());

        // 6. 解析 max_turns
        let max_iter = manifest.as_ref()
            .and_then(|m| m.max_turns)
            .unwrap_or(10);

        // 7. 构建 system prompt (manifest prompt + task)
        let child_manifest = manifest.clone().map(|mut m| {
            if let Some(ref sp) = m.system_prompt {
                m.system_prompt = Some(format!("{}\n\n## Your Task\n{}", sp, task));
            }
            m
        });

        // 8. 构建 AgentLoopConfig
        let config = AgentLoopConfig {
            max_iterations: max_iter,
            provider: parent_config.provider.clone(),
            tools,
            memory: parent_config.memory.clone(),
            model,
            session_id: octo_types::SessionId::from_string(id.clone()),
            user_id: parent_config.user_id.clone(),
            sandbox_id: parent_config.sandbox_id.clone(),
            tool_ctx: parent_config.tool_ctx.clone(),
            manifest: child_manifest,
            subagent_manager: Some(child_mgr),
            // 继承 hook_registry (全局 hooks 对子 agent 也生效)
            hook_registry: parent_config.hook_registry.clone(),
            ..AgentLoopConfig::default()
        };

        Ok(Self {
            id,
            agent_type,
            manifest,
            config,
            task,
            manager,
            event_sender,
        })
    }

    /// 同步执行：等待完成，转发事件，返回结果
    pub async fn run_sync(self) -> Result<SubAgentResult> {
        let id = self.id.clone();
        let display_id = self.agent_type.clone().unwrap_or_else(|| id.clone());
        let mgr = self.manager.clone();
        let event_sender = self.event_sender.clone();

        let messages = vec![ChatMessage::user(&self.task)];
        let mut stream = run_agent_loop(self.config, messages);
        let mut final_output = String::new();
        let mut rounds = 0u32;

        while let Some(event) = stream.next().await {
            match event {
                AgentEvent::Completed(result) => {
                    rounds = result.rounds;
                    final_output = /* extract text from result */ ...;
                    // 转发完成事件
                    if let Some(ref sender) = event_sender {
                        let _ = sender.send(AgentEvent::SubAgentEvent {
                            source_id: display_id.clone(),
                            inner: Box::new(AgentEvent::Completed(result)),
                        });
                    }
                }
                AgentEvent::Error { message } => {
                    let _ = mgr.fail(&id, message.clone()).await;
                    if let Some(ref sender) = event_sender {
                        let _ = sender.send(AgentEvent::SubAgentEvent {
                            source_id: display_id.clone(),
                            inner: Box::new(AgentEvent::Error { message: message.clone() }),
                        });
                    }
                    return Ok(SubAgentResult {
                        id, output: String::new(), rounds,
                        status: SubAgentStatus::Failed(message),
                    });
                }
                AgentEvent::Done => {}
                other => {
                    // 转发中间事件 (TextDelta, ToolStart, etc.)
                    if let Some(ref sender) = event_sender {
                        let _ = sender.send(AgentEvent::SubAgentEvent {
                            source_id: display_id.clone(),
                            inner: Box::new(other),
                        });
                    }
                }
            }
        }

        if final_output.is_empty() {
            let _ = mgr.fail(&id, "No output".into()).await;
            Ok(SubAgentResult { id, output: String::new(), rounds, status: SubAgentStatus::Failed("No output".into()) })
        } else {
            let _ = mgr.complete(&id, Some(final_output.clone())).await;
            Ok(SubAgentResult { id, output: final_output, rounds, status: SubAgentStatus::Completed })
        }
    }

    /// 异步执行：tokio::spawn，立即返回 session_id
    pub fn run_async(self) -> String {
        let id = self.id.clone();
        let mgr = self.manager.clone();
        let sa_id = id.clone();

        tokio::spawn(async move {
            let messages = vec![ChatMessage::user(&self.task)];
            let mut stream = run_agent_loop(self.config, messages);
            let mut final_output = String::new();

            while let Some(event) = stream.next().await {
                if let AgentEvent::Completed(result) = event {
                    final_output = /* extract text */ ...;
                }
            }

            if final_output.is_empty() {
                let _ = mgr.fail(&sa_id, "No output".into()).await;
            } else {
                let _ = mgr.complete(&sa_id, Some(final_output)).await;
            }
        });

        id
    }

    fn resolve_tools(manifest: &Option<AgentManifest>, parent_config: &AgentLoopConfig) -> Option<Arc<ToolRegistry>> {
        // 复用当前 SpawnSubAgentTool::resolve_tools 逻辑
        ...
    }
}
```

**Tests:** 4 (build 成功, 工具过滤, model 解析, max_turns)

---

### T2: AgentTool 重命名 + 使用 SubAgentRuntime

**Files:**
- Rename: `crates/octo-engine/src/tools/subagent.rs` → 内部重命名结构体
- Modify: `crates/octo-engine/src/tools/subagent.rs`
- Modify: `crates/octo-engine/src/agent/executor.rs`
- Modify: `crates/octo-cli/src/tui/formatters/tool_registry.rs`

**改动：**

1. `SpawnSubAgentTool` → `AgentTool`，`name()` 返回 `"agent"`
2. `QuerySubAgentTool` → `QueryAgentTool`，`name()` 返回 `"query_agent"`
3. `AgentTool.execute()` 改为使用 SubAgentRuntime：
   ```rust
   async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
       let manifest = self.resolve_manifest(agent_type);
       let runtime = SubAgentRuntime::build(
           task, manifest, &self.parent_config, self.manager.clone(), self.event_sender.clone()
       ).await?;

       let is_background = manifest.as_ref().map_or(false, |m| m.background);
       if is_background {
           let session_id = runtime.run_async();
           Ok(ToolOutput::success(json!({"session_id": session_id, "status": "spawned"})))
       } else {
           let result = runtime.run_sync().await?;
           Ok(ToolOutput::success(result.output))
       }
   }
   ```
4. TUI formatters 更新 tool name 匹配

**关键变化**：
- sync 是默认模式（LLM 等待结果后继续）
- background=true 时才 async
- 事件转发内置于 SubAgentRuntime

**Tests:** 2 (sync 调用返回结果, async 调用返回 session_id)

---

### T3: ExecuteSkillTool Playbook 模式复用 SubAgentRuntime

**Files:**
- Modify: `crates/octo-engine/src/skills/execute_tool.rs`

**改动：**

ExecuteSkillTool 的 `execute_playbook()` 改为构建 SubAgentRuntime 而非直接调用 run_agent_loop：

```rust
async fn execute_playbook(&self, skill: &SkillDefinition, request: &str) -> Result<ToolOutput> {
    let manifest = AgentManifest {
        name: format!("skill-{}", skill.name),
        system_prompt: Some(system_prompt),
        model: skill.model.clone(),
        background: skill.background,
        ..Default::default()
    };

    let runtime = SubAgentRuntime::build(
        request.to_string(),
        Some(manifest),
        /* parent_config from SubAgentContext */,
        self.subagent_ctx.manager.clone(),
        self.subagent_ctx.event_sender.clone(),
    ).await?;

    if skill.background {
        let session_id = runtime.run_async();
        Ok(ToolOutput::success(format!("Skill '{}' launched in background: {}", skill.name, session_id)))
    } else {
        let result = runtime.run_sync().await?;
        Ok(ToolOutput::success(format!("## Skill '{}' Result\n\n{}", skill.name, result.output)))
    }
}
```

**效果：** Agent 和 Skill Playbook 共享同一个执行路径。

**Tests:** 1 (Playbook skill 通过 SubAgentRuntime 执行)

---

### T4: 删除 agents/ YAML 目录 + 回退 AX-D5

**Files:**
- Delete: `agents/*.yaml` (6 个文件)
- Modify: `crates/octo-engine/src/agent/runtime.rs` (移除 agents_dir fallback)

**原因：** Builtin agents 只在代码中定义。用户自定义通过 Playbook skill 实现。

**Tests:** 编译通过

---

## Wave 2: 生命周期增强

### T5: event_sender 注入 AgentTool

**Files:**
- Modify: `crates/octo-engine/src/tools/subagent.rs` (AgentTool)
- Modify: `crates/octo-engine/src/agent/executor.rs`

**改动：**

AgentTool 新增 `event_sender` 字段，从 AgentExecutor 传入（broadcast_tx.clone()）。
SubAgentRuntime 使用 event_sender 转发所有中间事件到父 agent 的 broadcast channel。

**效果：** TUI 实时显示 sub-agent 的流式输出。

---

### T6: 清理保证 (Drop guard)

**Files:**
- Modify: `crates/octo-engine/src/agent/subagent_runtime.rs`

**改动：**

```rust
impl Drop for SubAgentRuntime {
    fn drop(&mut self) {
        // 确保 manager 中的状态被清理
        // 如果 still Running → mark as Cancelled
        let mgr = self.manager.clone();
        let id = self.id.clone();
        tokio::spawn(async move {
            let agents = mgr.list().await;
            if let Some(h) = agents.iter().find(|a| a.id == id) {
                if h.status == SubAgentStatus::Running {
                    let _ = mgr.cancel(&id).await;
                }
            }
        });
    }
}
```

---

### T7: 编译验证 + 全量测试

```bash
cargo check --workspace
cargo test --workspace -- --test-threads=1 -q
```

---

## 任务总览

| Wave | Task | 描述 | 预估行数 | 测试数 |
|------|------|------|---------|--------|
| W1 | T1 | SubAgentRuntime 结构体 + build/run_sync/run_async | ~250 | 4 |
| W1 | T2 | AgentTool 重命名 + 使用 SubAgentRuntime | ~100 | 2 |
| W1 | T3 | ExecuteSkillTool 复用 SubAgentRuntime | ~60 | 1 |
| W1 | T4 | 删除 agents/ YAML + 回退 AX-D5 | ~-260 | 编译 |
| W2 | T5 | event_sender 注入 AgentTool | ~30 | 0 |
| W2 | T6 | Drop guard 清理保证 | ~20 | 1 |
| W2 | T7 | 编译验证 + 全量测试 | 0 | 全量 |
| | **Total** | | **~200 净增** | **~8** |

---

## Deferred Items

| ID | 描述 | 前置条件 |
|----|------|---------|
| AY-D1 | Worktree 隔离 (git worktree for file isolation) | Worktree tool 实现 |
| AY-D2 | Transcript recording (独立 sidechain 记录) | TranscriptWriter per-session |
| AY-D3 | CancellationToken 传递 (cooperative cancel for sync agents) | AgentLoopConfig.cancel_token wiring |
| AY-D4 | SubAgentRuntime MCP scoped lifecycle (AX-D2 对接) | McpManager scoped API |
| AY-D5 | SubAgentRuntime Hook scoping (AX-D3 对接) | HookHandler agent_scope |
| AY-D6 | SubAgentRuntime Permission mode (AX-D1 对接) | ApprovalManager per-instance |
