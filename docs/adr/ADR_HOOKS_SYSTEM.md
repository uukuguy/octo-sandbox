# ADR：HOOKS SYSTEM 架构决策记录

**项目**：octo-sandbox
**版本**：v1.0
**日期**：2026-03-07
**状态**：已完成

---

## 目录

- [ADR-023：HookRegistry 全局钩子注册](#adr-023hookregistry-全局钩子注册)
- [ADR-024：HookHandler 事件处理机制](#adr-024hookhandler-事件处理机制)
- [ADR-025：HookContext 上下文传播](#adr-025hookcontext-上下文传播)

---

## ADR-023：HookRegistry 全局钩子注册

### 状态

**已完成** — 2026-03-07

### 上下文

系统需要在关键节点支持扩展，允许外部逻辑注入。

### 决策

实现 `HookRegistry` 管理 11 个钩子点：

| 钩子点 | 触发时机 | 用途 |
|--------|---------|------|
| 工具调用前 PreToolUse | | 权限检查、参数验证 |
| PostToolUse | 工具调用后 | 结果记录、清理 |
| PreTask | 任务开始前 | 初始化、准备 |
| PostTask | 任务完成后 | 总结、清理 |
| SessionStart | 会话开始 | 加载会话状态 |
| SessionEnd | 会话结束 | 保存会话状态 |
| ContextDegraded | 上下文降级 | 触发记忆提取 |
| LoopTurnStart | Agent 循环开始 | 每轮初始化 |
| LoopTurnEnd | Agent 循环结束 | 每轮总结 |
| AgentRoute | Agent 路由决策 | 自定义路由 |
| Notify | 通知事件 | 事件订阅 |

```rust
pub struct HookRegistry {
    hooks: HashMap<HookPoint, Vec<Arc<dyn HookHandler>>>,
}
```

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/hooks/registry.rs` | HookRegistry |
| `src/hooks/mod.rs` | HookPoint 枚举 |

---

## ADR-024：HookHandler 事件处理机制

### 状态

**已完成** — 2026-03-07

### 上下文

每个钩子点需要支持多个 Handler，按优先级执行。

### 决策

实现 `HookHandler` trait：

```rust
#[async_trait]
pub trait HookHandler: Send + Sync {
    async fn handle(&self, ctx: &HookContext) -> Result<HookResult>;
}
```

Handler 类型：
- **Block**: 阻止操作继续，返回错误
- **Transform**: 修改输入/输出
- **Observe**: 仅观察，不影响流程

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/hooks/handler.rs` | HookHandler trait |

---

## ADR-025：HookContext 上下文传播

### 状态

**已完成** — 2026-03-07

### 上下文

Handler 需要访问运行时上下文信息。

### 决策

实现 `HookContext` 携带必要信息：

```rust
pub struct HookContext {
    pub hook_point: HookPoint,
    pub session_id: Option<SessionId>,
    pub sandbox_id: Option<SandboxId>,
    pub user_id: Option<UserId>,
    pub metadata: HashMap<String, String>,
}
```

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/hooks/context.rs` | HookContext |
