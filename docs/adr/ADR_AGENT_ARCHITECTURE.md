# ADR：AGENT 架构决策记录

**项目**：octo-sandbox
**版本**：v1.0
**日期**：2026-03-07
**状态**：已完成

---

## 目录

- [ADR-014：AgentRuntime 模块化架构](#adr-014agentruntime-模块化架构)
- [ADR-015：AgentRouter 路由决策](#adr-015agentrouter-路由决策)
- [ADR-016：ManifestLoader YAML 声明式 Agent](#adr-016manifestloader-yaml-声明式-agent)

---

## ADR-014：AgentRuntime 模块化架构

### 状态

**已完成** — 2026-03-07

### 上下文

Agent 模块需要支持完整的 Agent 生命周期管理，包括：
- Agent 运行时初始化和配置
- Agent 实例创建和销毁
- 多租户隔离
- 工具和 MCP 服务集成

原有设计将所有功能集中在单一模块中，导致：
- 代码耦合度高，难以测试
- 缺乏清晰的边界
- 扩展困难

### 决策

采用模块化架构，将 Agent 拆分为多个子模块：

| 子模块 | 职责 |
|--------|------|
| `runtime.rs` | AgentRuntime 主入口，管理生命周期 |
| `executor.rs` | AgentExecutor，每个会话的 Agent 实例 |
| `loop.rs` | AgentLoop，单轮对话循环 |
| `catalog.rs` | AgentCatalog，Agent 注册表和状态机 |
| `store.rs` | AgentStore，SQLite 持久化 |
| `router.rs` | AgentRouter，任务路由决策 |
| `manifest_loader.rs` | ManifestLoader，YAML 声明式加载 |
| `config.rs` | AgentRuntimeConfig，配置管理 |

### 架构设计

```
AgentRuntime
    ├── providers: Arc<ProviderChain>       # LLM 提供商
    ├── memory: Arc<MemorySystem>           # 记忆系统
    ├── tools: Arc<ToolRegistry>            # 工具注册表
    ├── mcp: Arc<McpManager>               # MCP 管理器
    ├── security_policy: Arc<SecurityPolicy> # 安全策略
    ├── catalog: AgentCatalog               # Agent 目录
    └── store: SqliteAgentStore            # 持久化
```

### 涉及文件

| 文件 | 变更 |
|------|------|
| `src/agent/mod.rs` | 导出所有子模块 |
| `src/agent/runtime.rs` | AgentRuntime 主实现 |
| `src/agent/executor.rs` | AgentExecutor |
| `src/agent/loop.rs` | AgentLoop |
| `src/agent/catalog.rs` | AgentCatalog 状态机 |
| `src/agent/router.rs` | AgentRouter |
| `src/agent/manifest_loader.rs` | ManifestLoader |

### 后果

#### 正面
- 职责分离，代码可维护性提升
- 便于单元测试（可单独测试各模块）
- 扩展性好，新增功能不影响现有代码

#### 负面
- 模块间依赖需要显式管理
- 初期开发工作量增加

---

## ADR-015：AgentRouter 路由决策

### 状态

**已完成** — 2026-03-07

### 上下文

系统需要支持多种类型的 Agent（coder, reviewer, tester 等），根据任务特征自动选择合适的 Agent。

### 决策

实现 `AgentRouter` 提供基于任务描述的路由能力：

```rust
pub trait AgentRouter: Send + Sync {
    fn route(&self, task: &str) -> Result<RouteDecision>;
}
```

路由决策包含：
- `agent_type`: 推荐的 Agent 类型
- `confidence`: 置信度 (0.0-1.0)
- `fallback`: 备用 Agent 类型

### 涉及文件

| 文件 | 变更 |
|------|------|
| `src/agent/router.rs` | AgentRouter trait 和实现 |

---

## ADR-016：ManifestLoader YAML 声明式 Agent

### 状态

**已完成** — 2026-03-07

### 上下文

用户需要以声明式 YAML 方式定义 Agent 能力，而非硬编码。

### 决策

实现 `ManifestLoader` 从 YAML 文件加载 Agent 定义：

```yaml
name: coder
description: 代码编写智能体
capabilities:
  - code_generation
  - refactoring
  - bug_fix
model_preference: sonnet
max_tokens: 8192
```

### 涉及文件

| 文件 | 变更 |
|------|------|
| `src/agent/manifest_loader.rs` | ManifestLoader 实现 |
| `src/agent/config.rs` | AgentManifest 结构体 |
