# OpenFang 架构研究与引入计划

**阶段**: OpenFang 架构研究与引入
**日期**: 2026-02-27
**目标**: 研究 OpenFang 高价值架构，引入到 octo-sandbox 项目

---

## 1. OpenFang 核心架构分析

### 1.1 项目规模
- **137K+ LOC** 的 Rust 生产级 Agent OS
- **14 个 crate** 模块化架构
- **1,767+ 测试** 通过

### 1.2 14 个 Crate 模块

| Crate | 功能 | 核心组件 |
|-------|------|----------|
| openfang-kernel | 核心编排引擎 | Kernel, AgentRegistry, EventBus, Scheduler |
| openfang-runtime | Agent 运行时 | AgentLoop, MCP, LLM Drivers, Sandbox |
| openfang-api | REST API | 140+ 端点, Axum, WebSocket |
| openfang-memory | 记忆系统 | Structured, Semantic, Knowledge Graph |
| openfang-types | 类型定义 | Agent, Tool, Memory, Config |
| openfang-skills | Skill 插件 | 60+ Skills, SKILL.md 解析 |
| openfang-channels | 消息通道 | 40 适配器 (Telegram, Discord, etc.) |
| openfang-hands | 自主 Agent | 7 Hands (Clip, Lead, Researcher, etc.) |
| openfang-extensions | 扩展集成 | 25 MCP 模板, 密钥保险库 |
| openfang-wire | P2P 协议 | OFP 协议, HMAC-SHA256 |
| openfang-cli | CLI 工具 | Daemon, TUI Dashboard |
| openfang-desktop | 桌面应用 | Tauri 2.0 |
| openfang-migrate | 迁移工具 | OpenClaw, LangChain 迁移 |

---

## 2. octo-sandbox 当前架构

### 2.1 项目规模
- **约 30+ Rust 文件**
- **3 个 crate**: octo-sandbox, octo-engine, octo-types
- **2 个前端文件**: React + TypeScript

### 2.2 当前模块

| 模块 | 功能 | 状态 |
|-----|------|------|
| Agent Loop | 对话 + 工具执行 | ✅ 完成 |
| Providers | Anthropic, OpenAI | ✅ 完成 |
| Memory | Working + Session + Persistent | ✅ 完成 |
| Skills | Skill 加载 + 热重载 | ✅ 完成 |
| MCP | stdio transport, Manager | ✅ 完成 |
| API | REST + WebSocket | ✅ 完成 |
| Debug UI | Timeline, LogViewer | ✅ 完成 |
| MCP Workbench | Server 管理 | ✅ 完成 |

---

## 3. 对比分析

### 3.1 架构对比

| 维度 | OpenFang | octo-sandbox | 差距 |
|------|----------|--------------|------|
| **Crate 数量** | 14 | 3 | 大 |
| **Agent 数量** | 多代理注册表 | 单代理 | 大 |
| **Provider 数量** | 27 | 2 | 大 |
| **API 端点** | 140+ | 20+ | 大 |
| **记忆系统** | 三层 (结构+语义+图谱) | 三层 (简化) | 中 |
| **消息通道** | 40 适配器 | 无 | 大 |
| **自主 Agent** | Hands (7个) | 无 | 大 |
| **安全系统** | 16 层 | 基础 | 大 |

### 3.2 可引入的高价值模块

#### 🔴 高优先级 (对当前 octo-workbench 有直接价值)

| 模块 | 引入价值 | 实施难度 | 预计工作量 |
|-----|---------|---------|-----------|
| **MCP Client 实现** | 完善 MCP Workbench 运行时 | 中 | 2-3 天 |
| **EventBus 事件驱动** | 统一事件系统 | 低 | 1 天 |
| **API 设计模式** | 140+ 端点最佳实践 | 低 | 参考 |
| **配置管理** | TOML 配置 + 热重载 | 低 | 1 天 |

#### 🟠 中优先级 (对 octo-platform 有价值)

| 模块 | 引入价值 | 实施难度 | 预计工作量 |
|-----|---------|---------|-----------|
| **AgentRegistry 多代理** | 多用户/多代理支持 | 中 | 3-5 天 |
| **Memory 三层存储** | 知识图谱增强 | 中 | 3 天 |
| **Channel Adapters** | 消息通道集成 | 高 | 5-7 天 |
| **Hands 自主 Agent** | 自动化任务 | 高 | 7+ 天 |

#### 🟡 低优先级 (长期价值)

| 模块 | 引入价值 | 实施难度 |
|-----|---------|---------|
| **16 层安全系统** | 生产级安全 | 高 |
| **WorkflowEngine** | 工作流编排 | 中 |
| **Scheduler** | Cron 定时任务 | 中 |

---

## 4. 引入计划

### Phase 3: 架构升级 (建议)

#### Task 1: MCP Client 完善 (P0)
- **目标**: 完整 MCP stdio + SSE 传输
- **参考**: `openfang-runtime/src/mcp.rs`
- **产出**: 增强 MCP Workbench 运行时

#### Task 2: EventBus 事件系统 (P1)
- **目标**: 统一事件驱动架构
- **参考**: `openfang-kernel/src/event_bus.rs`
- **产出**: 解耦组件通信

#### Task 3: AgentRegistry 多代理 (P2)
- **目标**: 支持多代理注册和管理
- **参考**: `openfang-kernel/src/registry.rs`
- **产出**: 为 octo-platform 打基础

#### Task 4: Memory 增强 (P2)
- **目标**: 知识图谱集成
- **参考**: `openfang-memory/src/knowledge.rs`
- **产出**: 语义搜索增强

---

## 5. 关键代码参考

### 5.1 MCP Client
```
github.com/openfang/crates/openfang-runtime/src/mcp.rs
- McpServerConfig: 服务器配置
- McpTransport: stdio + SSE 传输
- McpConnection: 连接管理
- JSON-RPC 2.0 处理
```

### 5.2 EventBus
```
github.com/openfang/crates/openfang-kernel/src/event_bus.rs
- broadcast::Sender 订阅发布
- 历史 Ring Buffer
- per-agent 通道
```

### 5.3 AgentRegistry
```
github.com/openfang/crates/openfang-kernel/src/registry.rs
- DashMap 并发存储
- 多索引 (ID, Name, Tag)
- Agent 生命周期管理
```

### 5.4 MemorySubstrate
```
github.com/openfang/crates/openfang-memory/src/substrate.rs
- StructuredStore: SQLite
- SemanticStore: FTS5/向量
- KnowledgeStore: 知识图谱
- 共享连接池
```

---

## 6. 结论

OpenFang 是一个**成熟的 Agent OS**，其架构设计对 octo-sandbox 有极高的参考价值：

1. **短期**: 借鉴 MCP Client 实现完善 MCP Workbench
2. **中期**: 引入 AgentRegistry 和 EventBus 为多代理打基础
3. **长期**: 借鉴完整的 14 crate 架构构建 octo-platform 生态

**建议**: 优先引入 MCP Client 和 EventBus，这两个模块与当前 Phase 2 工作直接相关且实施难度较低。
