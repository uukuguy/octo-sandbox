# EAASP v2.0 L1 Runtime 候选项目分析

> **分析日期**：2026-04-12
> **分析范围**：`3th-party/eaasp-runtimes/` + `3th-party/claude-code-opensource/` 下 6 个候选项目
> **决策目标**：为 EAASP v2.0 Stage S5+ 的 L1 Runtime 选型提供依据
> **当前 L1 基线**：claude-code-runtime (Python) + hermes-runtime (Python) 双轨方案

---

## 0. 背景与分析框架

### L1 Runtime 的真实定位

L1 Runtime **本质是一个薄适配层**，职责是把各类原生 agent 框架（Claude Code、Hermes、Goose、Nanobot 等）的原生会话/工具/hook 语义翻译成 EAASP 的 16 方法 gRPC 契约，**不是要重新造一个 agent 内核**。

```
原生 Agent (任意语言)
      ↓  adapter (语言跟宿主走)
 L1 Runtime (16-method gRPC)
      ↓
 L2 / L3 / L4 (跨语言统一协议)
```

因此 L1 选型的核心问题是：**哪个候选框架的"原生能力集"与 EAASP 规范的重叠度最高，改造成本最低？**

### 五个分析维度

1. **Hook 机制** — 能否把 PreToolUse/PostToolUse 拦截点转成 gRPC HookBridge 往返
2. **MCP 客户端** — 是否原生支持、transport 覆盖度、生命周期管理
3. **Skill 系统** — 是否有 Markdown + YAML frontmatter 格式、作用域、加载方式
4. **服务方式** — CLI / HTTP / gRPC / SDK / 消息驱动？是否 headless 可调用？是否 multi-session？
5. **场景契合度** — 项目被设计服务的真实场景与 EAASP 企业 agent-as-a-service 目标的匹配度

### 候选清单

| 分组 | 项目 | 路径 |
|---|---|---|
| 通用 agent 框架 | Goose | `3th-party/eaasp-runtimes/goose/` |
| | Nanobot | `3th-party/eaasp-runtimes/nanobot/` |
| | Pi-mono | `3th-party/eaasp-runtimes/pi-mono/` |
| Claude Code 复刻 | CCB (Claude Code Best) | `3th-party/claude-code-opensource/CCB/` |
| | CCR (Claude Code Rust) | `3th-party/claude-code-opensource/CCR/` |
| | claw-code | `3th-party/claude-code-opensource/claw-code/` |

---

## 1. 维度 1：Hook 机制对比

### 1.1 Goose — `ToolConfirmationRouter` 模式

**证据文件**：
- `crates/goose/src/agents/tool_confirmation_router.rs:1-46`
- `crates/goose/src/agents/tool_execution.rs:76-161`

**核心机制**：
```rust
pub struct ToolConfirmationRouter {
    pending: Mutex<HashMap<String, oneshot::Sender<PermissionConfirmation>>>,
}
```

- `register(request_id)` → 返回 `oneshot::Receiver`
- `deliver(request_id, confirmation)` → 外部调用放行/拒绝
- `tool_execution.rs:99` 注册，L111 `confirmation_rx.await` 阻塞

**gRPC 桥接可行性**：**✅ 零改造**。外部 gRPC server 持有 `Arc<Agent>`，收到 HookBridge 的 `ApproveToolCall(request_id, perm)` 请求时直接调 `router.deliver()`。路径全 `async`，天然兼容 bidi streaming。

**粒度**：单 tool。返回语义：`AllowOnce / AlwaysAllow / DenyOnce / AlwaysDeny`。

### 1.2 Nanobot — 批级 `AgentHook`

**证据文件**：
- `nanobot/agent/hook.py:29-54`
- `nanobot/agent/runner.py:256`

**核心机制**：
```python
async def before_execute_tools(self, context: AgentHookContext) -> None:
    pass  # context.tool_calls 是整批 tool call
```

**gRPC 桥接可行性**：**✅ 可行**。写 `GrpcHookBridge(AgentHook)` 子类，`await grpc_stub.PreToolBatch(context.tool_calls)`。

**粒度**：**整批**（context.tool_calls 是一个列表）。这是独特卖点（事务性 / 预算 / 组合风险场景），也是局限（无法对单个 tool 单独审批）。

**返回语义**：hook 返回 None，拒绝需 raise 或改 `context.tool_calls = []`。

### 1.3 Pi-mono — `beforeToolCall` 回调

**证据文件**：
- `packages/agent/src/types.ts:46-49, 68-78`
- `packages/agent/src/agent-loop.ts:491-492`

**核心机制**：
```typescript
beforeToolCall?: (context: BeforeToolCallContext, signal?: AbortSignal)
  => Promise<BeforeToolCallResult | undefined>;

interface BeforeToolCallResult {
  block?: boolean;
  reason?: string;
}
```

**gRPC 桥接可行性**：**✅ 可行**。传一个 async 函数内部调 `@grpc/grpc-js` 客户端。

**粒度**：单 tool。返回语义 `{block, reason}`。带 `AbortSignal` 支持取消——接口设计是三个通用框架里最干净的。

### 1.4 CCB — **HTTP hook transport（已有）**

**证据文件**：
- `src/utils/hooks/execHttpHook.ts:123-239`
- `src/utils/hooks/hookEvents.ts`

**核心机制**：
```typescript
export async function execHttpHook(
  hook: HttpHook,
  _hookEvent: HookEvent,
  jsonInput: string,
  signal?: AbortSignal,
): Promise<{ ok: boolean; statusCode?: number; body: string; error?: string; aborted?: boolean }>
```

CCB **已经把 hook 做成 HTTP 远程调用**：SSRF guard、URL allowlist、env var interpolation、sandbox proxy、timeout、AbortSignal 全齐。hook 类型是枚举 `Command | Http | Agent | Prompt` 四种。

**独立事件总线**：`hookEvents.ts` 提供 `started / progress / response` 三事件流，天然匹配 gRPC bidi streaming 上行。

**gRPC 桥接可行性**：**✅ 几乎零成本**。加一个 `Grpc` variant 到 hook type enum，核心 `executeHook` dispatcher 不用动。返回值 `body` 已是 JSON，直接映射 proto message。

### 1.5 CCR — 过于简陋

**证据文件**：`CCR/crates/runtime/src/hooks.rs:1-118`

```rust
pub struct HookConfig {
    pub enabled: bool,
    pub pre_hooks: Vec<Hook>,
    pub post_hooks: Vec<Hook>,
}
```

只是 session-wide shell 命令列表，**无 per-tool matcher**、**无 denied/allow 语义**、无 AbortSignal、无事件流、无 plugin 加载。跟 Anthropic Claude Code 规范几乎无关。

**gRPC 桥接可行性**：**改造 ≈ 重写**。不推荐。

### 1.6 claw-code — **Anthropic 规范对齐最严**

**证据文件**：`claw-code/rust/crates/runtime/src/hooks.rs:18-489`

```rust
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
}

pub struct HookRunResult {
    denied: bool,
    failed: bool,
    cancelled: bool,
    messages: Vec<String>,
    permission_override: Option<PermissionOverride>,
    permission_reason: Option<String>,
    updated_input: Option<String>,
}
```

- 完整实现 Anthropic Claude Code hook 规范
- exit code 0 / 2 / 其它 = allow / deny / failed 语义（对齐 S4.T1 `block_write_scada.sh` 契约）
- `HookAbortSignal` + `HookProgressReporter` trait
- 字段 `denied / permission_override / updated_input / permission_reason` 全齐

**但**：只支持 subprocess transport。要做 gRPC 桥接需抽象 `HookTransport` trait。

### 1.7 Hook 维度小结

| 项目 | 成熟度 | 规范对齐 | Transport 现状 | 桥接成本 |
|---|---|---|---|---|
| CCB | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ 原生 Anthropic | ✅ HTTP 已有 | 0.5-1 天 |
| claw-code | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ 原生 Anthropic | ❌ 只有 subprocess | 2-3 天 |
| Goose | ⭐⭐⭐⭐ | ⭐⭐ 自研语义 | ❌ 需加 tonic | 1-2 天 |
| Pi | ⭐⭐⭐⭐ | ⭐⭐⭐ 签名清晰 | ❌ 需加 grpc client | 1 天 |
| Nanobot | ⭐⭐⭐ | ⭐⭐ 批级语义 | ❌ 需加 grpc client | 2-3 天 |
| CCR | ⭐ | ⭐ | ❌ | 5-7 天（等于重写） |

### 1.8 对 EAASP HookBridge 契约的启示

**批级 hook 的独特价值**（Nanobot 启发）：

| 粒度 | 适用场景 | 代表项目 |
|---|---|---|
| 单 tool | 逐个风险审计、RBAC per-tool、用户逐条确认 | Goose / Pi / CCB / claw-code |
| 批级 | 原子事务、预算检查、组合风险、批量优化 | Nanobot |

S4.T1 的 `threshold-calibration` skill 实际更接近批级场景（多 tool 原子流程 + Safety-Envelope 全局约束），这反过来说明 **EAASP HookBridge 契约可能需要支持批级语义**——建议在 Stage S2 schema v2.1 阶段开 ADR 讨论双轨契约：
1. `PreToolUse`（单 tool，已有）
2. `PreToolBatch`（整批，pre-execution 汇总评估）

---

## 2. 维度 2：MCP 客户端对比

### 2.1 Goose

- **证据**：`crates/goose/src/agents/mcp_client.rs` 1012 行 + `extension_manager.rs` 81KB
- **SDK**：`rmcp` 0.16（Anthropic 官方 Rust SDK）
- **Transport**：stdio + streamable-http（rmcp 原生支持）
- **生命周期**：`extension_manager` 管理 spawn/stop，支持动态 connect/disconnect
- **命名空间**：多 server 通过 extension_manager 隔离
- **EAASP SessionPayload.mcp_servers 映射**：**Good** —— 能直接从 5-block 启动

### 2.2 Nanobot

- **证据**：`nanobot/agent/tools/mcp.py`
- **实现**：自研轻量 MCP client
- **Transport**：stdio + SSE
- **生命周期**：spawn stdio 子进程管理
- **映射难度**：中等（自研 SDK 与 rmcp/官方 TS SDK 不一致，可能有规范漂移）

### 2.3 Pi-mono

- **证据**：`find pi-mono -name "*mcp*"` **零结果**
- **结论**：**无 MCP 客户端**。tool 是纯 TS 定义，不接 MCP 生态
- **映射难度**：极高（要自己写一套 MCP client）

### 2.4 CCB — 最完整

- **证据**：`src/services/mcp/client.ts` **116KB** + `config.ts` 50KB + `auth.ts` 86KB
- **SDK**：Anthropic 官方 `@modelcontextprotocol/sdk`
- **Transport**：**stdio + SSE + streamable-http + WebSocket** 四种全支持
- **附加能力**：OAuth 端口协议、`InProcessTransport`、`MCPConnectionManager`、SSRF guard、sandbox network proxy
- **映射难度**：**零**——这就是 Anthropic Claude Code 的上游实现

### 2.5 CCR

- **证据**：`src/mcp/` 含 `client.rs / server.rs / transport.rs / prompts.rs / resources.rs / sampling.rs`
- **实现**：自研 MCP（非 rmcp）
- **Transport**：未深入验证
- **评价**：代码规模中等，质量未知

### 2.6 claw-code

- **证据**：`rust/crates/runtime/src/` 含 6 个 mcp 文件：`mcp_client.rs / mcp_server.rs / mcp_stdio.rs / mcp_tool_bridge.rs / mcp_lifecycle_hardened.rs / mcp.rs`
- **实现**：自研 MCP，专注 stdio transport，有 lifecycle hardening（subprocess 崩溃恢复）
- **评价**：**做得精细**，但不如 CCB 全面

---

## 3. 维度 3：Skill 系统对比

### 3.1 Goose — **无 skill frontmatter**

- **证据**：`crates/goose/src/agents/builtin_skills/` 仅代码注册 + `src/recipe/`
- **"Recipe"**：Goose 的可分享流程文件，但不是 EAASP skill 格式
- **EAASP Skill v2 frontmatter 映射难度**：**高**（需自写一层 Markdown frontmatter → Goose Recipe adapter）

### 3.2 Nanobot — **原生 SKILL.md + 轻量 frontmatter**

- **证据**：`nanobot/agent/skills.py` + `nanobot/skills/skill-creator/SKILL.md`（18.7KB 示例）
- **格式**：`---\nname: skill-creator\n---` Markdown + YAML frontmatter
- **解析**：Regex
- **映射难度**：**低**

### 3.3 Pi-mono — **有 skill 系统**

- **证据**：`packages/coding-agent/src/core/skills.ts`
- **格式**：`parseFrontmatter` 解析，`MAX_NAME_LENGTH=64`, `MAX_DESCRIPTION_LENGTH=1024`（对齐 Anthropic spec）
- **测试 fixtures**：`packages/coding-agent/test/fixtures/skills/`
- **映射难度**：**低**

### 3.4 CCB — **最完整 skill 系统**

- **证据**：`src/skills/` 含 `loadSkillsDir.ts` + `bundledSkills.ts` + `mcpSkillBuilders.ts`
- **5 个 skill 来源**：`bundled / plugin / managed / mcp / skills`
- **Parser**：`utils/frontmatterParser.ts` 完整 frontmatter
- **附加能力**：
  - `argumentSubstitution`（`${arg}` 替换）
  - `EFFORT_LEVELS`（effort 等级）
  - `HooksSettings` schema（skill 级 hook 定义）
  - `parseSlashCommandToolsFromFrontmatter`
  - `pluginOnlyPolicy`
- **映射难度**：**零**——这就是 EAASP Skill v2 的上游规范

**关键观察**：我们 S4.T1 写的 `threshold-calibration/SKILL.md` 的 frontmatter（`runtime_affinity / access_scope / scoped_hooks / dependencies`）本来就是基于 Anthropic Claude Code 规范扩展的。CCB 是这个规范的完整 TypeScript 实现，**无损继承**。

### 3.5 CCR — **有 skill 但质量未知**

- **证据**：`src/skills/` 有 `loader.rs / builtin.rs / executor.rs / registry.rs` + `tests/skills_test.rs`
- **评价**：代码存在但未深入验证，质量参差

### 3.6 claw-code — **无 skill**

- **证据**：`find rust -name "*skill*"` **零结果**
- **评价**：claw-code 专注 Claude Code 内核复写，skill 系统未实现

---

## 4. 维度 4：服务方式与部署形态

### 4.1 Goose

- **证据**：`goose-server` crate (axum HTTP + WebSocket) + `goose-cli` + Desktop (Tauri)
- **入口**：
  - HTTP server (axum)
  - WebSocket server
  - CLI REPL
  - Desktop app (Tauri)
- **Multi-session**：✅ `SessionType` enum
- **Headless**：✅ server 模式
- **无 gRPC**

### 4.2 Nanobot

- **证据**：
  - `nanobot/api/server.py` 7KB 轻量 server（非 FastAPI，自研）
  - `nanobot/channels/`：Telegram / Slack / Discord / Feishu / WeChat 6 个 channel bridge
- **入口**：CLI + 轻量 API server + 6 个消息 channel
- **架构重心**：消息 channel bridge（主场景），API 是次要的
- **Multi-session**：单进程串行 session（`unified_session` config）
- **被调用难度**：中等（需改造 channel bus 为 gRPC transport）

### 4.3 Pi-mono

- **证据**：
  - `packages/coding-agent` CLI (`main.ts` 23KB)
  - `packages/tui` 终端 UI
  - `packages/web-ui` 浏览器
  - `packages/mom` Slack bridge
  - `packages/pods` vLLM 管理
- **入口**：**全部是客户端形态**，无服务端 agent server
- **场景**：IDE / 终端 coding agent、Slack bot、本地 TUI、Web UI
- **L1 定位**：不契合（无 multi-session，无 headless server）

### 4.4 CCB

- **证据**：`src/entrypoints/` 含 `cli.tsx` + `mcp.ts` + `sdk/` + `server/`
- **入口**：
  - CLI REPL
  - MCP server 模式（被外部调用）
  - SDK library 嵌入
  - HTTP server
- **Headless**：✅
- **Multi-session**：✅

### 4.5 CCR

- **证据**：
  - `src/web/server.rs` axum TcpListener
  - `src/wasm/bridge.rs` WASM 目标
  - `crates/cli` CLI
  - `gui-tauri/` Desktop
- **入口**：Web server + CLI + WASM + Tauri Desktop
- **评价**：工程雄心大，实际成熟度参差

### 4.6 claw-code

- **证据**：`rust/crates/rusty-claude-cli/src/main.rs` CLI + `mock-anthropic-service`（测试用）
- **入口**：**纯 CLI 二进制**
- **无 HTTP/gRPC server**
- **L1 定位**：不契合（要自己加一层 server）

---

## 5. 维度 5：场景契合度

| 项目 | 真实服务场景 | 与 EAASP 目标契合度 |
|---|---|---|
| Goose | 桌面 AI 助手 (Block 出品) + CLI + HTTP API | **高**：原生 agent-as-service |
| Nanobot | 消息机器人网关 (Telegram/Slack/...) | **中**：场景错位，但 skill 设计接近 EAASP |
| Pi | 个人 IDE 助手 + 终端 + Web UI | **低**：客户端形态，非服务 |
| CCB | Claude Code 完整替代品，可被调用 | **极高**：企业 agent runtime |
| CCR | Claude Code Rust 全功能复刻 | **中**：雄心大、成熟度低 |
| claw-code | Claude Code 内核严谨 Rust 重写 | **中**：规范对齐好、场景窄 |

---

## 6. 综合总表

| 项目 | 语言 | Hook | MCP Client | Skill | 服务方式 | Multi-session | 场景契合 | **L1 裁决** |
|---|---|---|---|---|---|---|---|---|
| **CCB** | TS/Bun | ⭐⭐⭐⭐⭐ HTTP/Agent/Prompt/Cmd | ⭐⭐⭐⭐⭐ 4 transport | ⭐⭐⭐⭐⭐ 5 来源 | CLI+MCPsrv+SDK+HTTP | ✅ | Claude Code 等价物 | **Excellent** |
| **Goose** | Rust | ⭐⭐⭐⭐ router | ⭐⭐⭐⭐ rmcp stdio+http | ⭐ 无 | HTTP+WS+CLI+Desktop | ✅ | 桌面助手/API | **Good** |
| **Nanobot** | Python | ⭐⭐⭐ 批级 | ⭐⭐⭐ 自研 stdio+SSE | ⭐⭐⭐⭐⭐ SKILL.md | Channel bridge + 轻 API | ❌ | 消息机器人 | **Acceptable** |
| **claw-code** | Rust | ⭐⭐⭐⭐⭐ 规范对齐 | ⭐⭐⭐⭐ 硬化 | ❌ 无 | 纯 CLI | ❌ | Claude Code 内核 | **Marginal** |
| **Pi** | TS | ⭐⭐⭐⭐ 签名干净 | ❌ 无 | ⭐⭐⭐⭐ frontmatter | 全客户端 | ❌ | 客户端 agent | **Poor** |
| **CCR** | Rust | ⭐ 过薄 | ⭐⭐⭐ 自研 | ⭐⭐ 有但浅 | CLI+Web+WASM+Tauri | ? | 全能复刻 | **Marginal** |

---

## 7. 最终推荐

### 7.1 第一名：CCB ⭐⭐⭐⭐⭐

**五维全满足，几乎是我们要的 L1 runtime 本身。**

| 维度 | 情况 |
|---|---|
| Hook | HTTP/Agent/Prompt/Command 四种 transport 已有，加 gRPC 即第五种 |
| MCP | 4 transport 全支持 + OAuth + InProcessTransport |
| Skill | 5 来源 × Anthropic frontmatter × HooksSettings schema（EAASP Skill v2 的上游） |
| 服务方式 | CLI + MCP server + SDK library + HTTP server 四种入口都齐 |
| 场景 | 被设计成 "能被外部系统调用的 Claude Code 等价 runtime" |

**唯一代价**：TypeScript/Bun，`grid-runtime` 需通过 subprocess 托管（与当前托管 `claude-code-runtime` Python 同构，非新问题）

**改造工作量**：
1. gRPC transport 包装：**1-2 天**（SDK entrypoint 已存在，加一层 tonic adapter）
2. SessionPayload 5-block 映射：**2-3 天**（HooksSettings / McpServers / Skills 字段都有对应）
3. **总计：1 周内可跑通 spike**

### 7.2 第二名：Goose ⭐⭐⭐⭐（Rust 路线最佳）

**如果坚持 L1 必须 Rust 原生嵌入 `grid-runtime`，Goose 是唯一实用选择。**

- rmcp 原生 MCP（Rust 生态一致）
- HTTP + WebSocket server 原生
- `ToolConfirmationRouter` 作为 hook 基础已经很成熟
- multi-session 原生
- 唯一短板：**无 skill frontmatter**，需自写一层 Markdown frontmatter → Goose Recipe adapter（3-5 天）

**改造总工作量**：**2 周**

### 7.3 第三名：claw-code ⭐⭐⭐（Rust 备胎）

- hook 规范对齐最严（`HookRunResult` 是 EAASP hook envelope 的完美超集）
- MCP lifecycle hardening 做得细
- **但**无 skill、无 server——两大硬伤
- 仅推荐给"Rust 内核洁癖 + 愿意自建 skill loader 和 server" 场景

### 7.4 第四名：Nanobot ⭐⭐（轻量 Python 备胎）

- skill 格式最接近 EAASP（SKILL.md）
- 批级 hook 有独特价值（事务/预算/组合风险）
- **但**架构是消息 bridge 驱动，server 层薄弱
- 仅推荐给需要批级 hook 语义的特殊场景

### 7.5 不推荐

- **Pi**：无 MCP 是硬伤，全客户端形态不契合
- **CCR**：hook 过薄 + 代码质量参差

---

## 8. 对 EAASP 路线图的启示

### 8.1 Skill v2 frontmatter 规范溯源

**重要发现**：EAASP 的 Skill v2 frontmatter（`runtime_affinity / access_scope / scoped_hooks / dependencies`）**本来就是 Anthropic Claude Code 规范的扩展**。CCB 是这个规范的完整 TypeScript 实现，选 CCB 作为 L1 等于**免费继承规范对齐**，skill 文件可以在"真实 Claude Code"和"我们的 L1 runtime"之间无损迁移。

### 8.2 `ScopedHookBody` 应抄 Anthropic hook transport 枚举

CCB 的 hook 类型枚举 `Command | Http | Agent | Prompt` 加上 `Grpc` 就是 EAASP HookBridge 的完整 transport 矩阵。S4.T1 目前只实现了 Command 类型，Deferred 清单中的 D51（PostToolUse prompt-hook executor runtime）正好对应 Prompt 类型。

**建议**：Stage S2 schema v2.1 阶段将 `ScopedHookBody` 的 body 类型扩展为 5 种 variant。

### 8.3 批级 hook 需要独立契约

Nanobot 的 `before_execute_tools(context)` 批级语义暴露了 EAASP HookBridge 的一个缺口。threshold-calibration skill 的原子校准流程（读快照 → 计算 → 写回）本质就是批级场景。

**建议**：Stage S2 schema v2.1 开 ADR 讨论双轨 HookBridge 契约：
1. `PreToolUse`（单 tool，已有）
2. `PreToolBatch`（整批，新增）

### 8.4 L1 语言跟 agent 走的原则确认

分析再次证实："L1 Runtime 语言应跟被适配的 agent 框架语言走"：
- CCB → TypeScript/Bun subprocess
- Goose → Rust crate 直接嵌入 `grid-runtime`
- Nanobot → Python subprocess
- claw-code → Rust crate
- Pi → TypeScript/Node subprocess
- CCR → Rust crate（但不推荐）

没有"L1 必须 Rust"这回事。当前的 `claude-code-runtime (Python)` + `hermes-runtime (Python)` 双轨架构是**正确的**。

### 8.5 L2 Memory / L3 Governance / L4 Orchestration 职责确认

分析过程纠正了一个误解：**会话持久化不是 L1 的职责**，而是 L2 Memory Engine 的职责（S3.T2 已实现）。L1 只需把原生 session 映射到 SessionPayload 5-block 转发给 L2 即可。因此 Pi / claw-code "无会话持久化"**不是问题**，真正的问题是它们没有 server 可以被 L4 orchestrator 调用。

---

## 9. 实际行动建议

### 9.1 强烈建议：CCB 2-Day Spike

**目标**：验证 CCB 作为 L1 runtime 的可行性，不改一行 CCB 源码。

**Day 1 — 代码熟悉**：
- 读 `src/entrypoints/sdk/`（SDK 入口）
- 读 `src/services/mcp/config.ts` 50KB（MCP server 配置格式）
- 读 `src/skills/loadSkillsDir.ts`（skill 加载流程）
- 读 `src/utils/hooks/execHttpHook.ts`（hook HTTP transport）

**Day 2 — gRPC wrapper PoC**：
- 写一个 tonic gRPC server
- 实现 `RuntimeService::CreateSession`
- 内部通过 CCB 的 SDK entrypoint 启动 agent
- 把 SessionPayload.mcp_servers → CCB 的 `.mcp.json`
- 把 SessionPayload.skills → `~/.claude/skills/`
- 把 HookBridge gRPC 调用 → CCB 的 HTTP hook endpoint（`execHttpHook` 直接往 HookBridge 发 HTTP）

**验收**：一个完整的 tool call 流程能通过 PreToolUse hook 拦截，L3 HookBridge 能放行或拒绝。

### 9.2 并行（可选）：Goose Rust Spike

**触发条件**：如果 CCB spike 验证了可行性，且团队希望有一个 Rust 原生 L1 可直接嵌入 `grid-runtime`。

**范围**：
- goose-server 加 tonic layer
- 写 Markdown skill loader → Goose Recipe adapter
- 包装 `ToolConfirmationRouter` 为 gRPC HookBridge 客户端

**工作量**：2 周

### 9.3 对现有 `claude-code-runtime` 的影响

如果 CCB spike 成功：

- 目前的 `claude-code-runtime` 是对 Anthropic `claude-agent-sdk` 的薄 Python 封装
- CCB 是对 Claude Code 本体的完整 TypeScript 复刻
- **CCB 可以替换 `claude-code-runtime`**，继承完整 Anthropic 兼容性
- 我们的 SKILL.md v2 frontmatter 几乎不用改（因为本来就是 CCB 上游规范）

### 9.4 决策时间窗

- **S4.T1 ~ S4.T3 期间不做此决策**——当前重点是跑通 threshold-calibration 验证
- **Stage S4 结束后**（15/15 完成后）启动 CCB spike
- Stage S5 根据 spike 结果决定是否引入 CCB 作为第三个 L1 runtime（或替换 claude-code-runtime）

---

## 10. 附录：分析过程中被纠正的错误判断

本分析在多轮迭代中修正了若干错误结论，记录于此作为方法论提醒：

| 错误判断 | 纠正 | 教训 |
|---|---|---|
| "L1 必须 Rust 嵌入 grid-runtime" | L1 语言跟 agent 走，Python/TS subprocess 同样可行 | 不要把语言偏好当原则 |
| "Pi 无会话持久化是 killer flaw" | 会话持久化是 L2 职责，L1 不需要 | 分清各层职责 |
| "Nanobot 批级 hook 粒度粗=短板" | 批级 hook 是独特卖点（事务/预算/组合风险） | 粒度细不等于更好 |
| "CCB 是 IDE 插件架构 / 无 MCP client" | CCB 有 116KB MCP client + 5 来源 skill 系统 | 不能只依赖 subagent，必须一手源码验证 |
| "CCR 是纯工具提取器无 agent 能力" | CCR 有 `src/mcp/` + `src/skills/` + axum server | 同上 |
| 只看一个维度就下结论 | 必须五维综合（hook/MCP/skill/服务/场景） | 单维度易被表面假象误导 |

**核心教训**：架构评估必须亲自读源码、交叉验证，subagent 调研结果只能作为线索而非结论。

---

## 11. Grid 自身在同维度下的定位（关键修正）

> **本节追加于 2026-04-12**，修正前 10 节的一个根本性错位——前文把 6 个候选项目独立评分，**没有把 Grid 本身作为参照基线纳入对比**。这个遗漏会让读者误以为我们在"还没有 L1 runtime"的空白区选型，实际上 Grid 就是 EAASP 的 L1 参考实现。

### 11.1 Grid 的真实定位：不是候选，是基线

Grid 不应该和 CCB / Goose / Nanobot / Pi / claw-code / CCR 放在同一条起跑线上——它们不是同一品类：

| 品类 | 项目 | 与 EAASP 的关系 |
|---|---|---|
| **通用 agent 框架** | CCB / Goose / Nanobot / Pi / claw-code / CCR | 需要被"改造/包装"成 L1 |
| **EAASP L1 参考实现** | **Grid** | **定义者**：`grid-runtime` 实现 17 方法 gRPC，`grid-engine` 提供全套治理面，`grid-hook-bridge` 定义协议，`grid-types` 持有 proto |

换句话说，正确的问题不是"Grid 能排第几"，而是"**Grid 作为 baseline，其他 6 个能替换它的哪些子系统**？"

### 11.2 Grid 五维自评（基于源码）

#### 维度 1：Hook 机制 — ⭐⭐⭐⭐⭐ **最强**

| 对比项 | Grid | CCB（前文冠军） |
|---|---|---|
| HookPoint 种类 | **17 种** | 5-6 种 |
| HookAction variant | **9 种** (`Continue/Modify/Abort/Block/Redirect/ModifyInput/InjectContext/PermissionOverride/...`) | 2-4 种 |
| Transport 种类 | **6 种** (Command/Prompt/Webhook/WASM/Policy-DSL/Builtin) | 4 种 |
| gRPC bridge | ✅ **原生** `GrpcHookBridge` + `InProcessHookBridge` 双实现 | ❌ 需改造 |
| Policy DSL | ✅ `policy/matcher.rs` + `PolicyRule` 热加载 | ❌ |
| FailureMode | ✅ FailOpen / FailClosed per-hook | ⚠️ 部分 |
| Priority | ✅ per-handler priority (u32) | ❌ |
| Async fire-and-forget | ✅ `is_async()` trait method | ❌ |
| WASM plugin hook | ✅ `wasm/handler.rs` + `host_impl.rs` + manifest | ❌ 全部 6 候选都没有 |

**17 个 HookPoint 枚举**：
```
PreToolUse / PostToolUse / PreTask / PostTask
SessionStart / SessionEnd / Stop / SubagentStop
ContextDegraded / LoopTurnStart / LoopTurnEnd
AgentRoute / SkillsActivated / SkillDeactivated / SkillScriptStarted
ToolConstraintViolated / UserPromptSubmit
```

**证据**：
- `crates/grid-engine/src/hooks/mod.rs:21-57` (17 HookPoint)
- `crates/grid-engine/src/hooks/handler.rs:28-45` (HookAction + PermissionHookDecision)
- `crates/grid-engine/src/hooks/{declarative,wasm,policy,builtin}/` (6 transport)
- `crates/grid-hook-bridge/src/{traits.rs,in_process.rs,grpc_bridge.rs}` (trait + 双实现)

**裁决**：Grid 的 hook 系统是 7 个项目里最强的，**而且是唯一一个原生内置 gRPC HookBridge** 的——其他 6 个全部需要"加一层"才能对接 L3。

#### 维度 2：MCP 客户端 — ⭐⭐⭐⭐ 与 Goose 并列第二

| 对比项 | Grid | CCB |
|---|---|---|
| Transport | stdio + SSE | stdio + SSE + StreamableHTTP + WebSocket |
| OAuth | ✅ `mcp/oauth.rs` 20.1KB | ✅ `auth.ts` 86KB |
| Config v2 | ✅ `McpServerConfigV2` 版本化 | ✅ |
| 生命周期 | ✅ `manager.rs` 26KB | ✅ |
| SQLite 持久化 | ✅ `storage.rs` | ✅ |
| 动态 connect/disconnect | ✅ grid-runtime 有 ConnectMcp/DisconnectMcp 方法 | ✅ |

**证据**：
- `crates/grid-engine/src/mcp/` 共 9 文件（`manager/traits/oauth/sse/stdio/storage/bridge/convert/server`）
- `crates/grid-runtime/src/service.rs:367,478`

**差距**：比 CCB 少 StreamableHTTP 和 WebSocket 两种 transport——**这是 Grid 的真实短板之一**。

#### 维度 3：Skill 系统 — ⭐⭐⭐⭐ 与 CCB 实质持平

| 对比项 | Grid | CCB |
|---|---|---|
| Frontmatter 格式 | ✅ `name/description/version/allowed-tools` + EAASP v2 扩展 (`runtime_affinity/access_scope/scoped_hooks/dependencies`) | ✅ Anthropic 规范 |
| Skill 来源数 | 3 种 (bundled + project + user) | 5 种 (bundled + plugin + managed + mcp + skills) |
| Loader 规模 | **1247 行** (`loader.rs`) | 类似 |
| 依赖管理 | ✅ `dependency.rs` | ⚠️ |
| 作用域 | ✅ `trust.rs` + `constraint.rs` | ✅ |
| 版本管理 | ✅ `standards.rs` 9.6KB | ✅ |
| **Semantic index** | ✅ `semantic_index.rs`（向量搜索触发） | ❌ |
| Slash router | ✅ | ✅ |
| **L2 skill registry** | ✅ **独立 `eaasp-skill-registry` crate + L2 REST** | ❌ 纯本地文件系统 |

**证据**：`crates/grid-engine/src/skills/` **20 个文件**，`tools/eaasp-skill-registry/` L2 市场集成

**裁决**：来源数略少于 CCB，但在 **semantic index + L2 市场集成**两项上领先——Grid 是唯一实现 skill 市场的项目。**总体与 CCB 持平，企业级维度领先**。

#### 维度 4：服务方式 — ⭐⭐⭐⭐⭐ **7 个项目里最全**

| 对比项 | Grid | 最接近的对手 |
|---|---|---|
| **gRPC server** | ✅ **17 方法原生实现** | ❌ 全部 6 候选都没有 |
| HTTP/REST server | ✅ **31 个 API 模块** | Goose 有但规模小 |
| WebSocket | ✅ | Goose |
| CLI | ✅ `grid-cli` | 全部都有 |
| Desktop | ✅ `grid-desktop` | 仅 Goose / CCR |
| **Platform (multi-tenant)** | ✅ **独立 `grid-platform` crate** | ❌ **独此一家** |
| Multi-session | ✅ | Goose / CCB |
| **完整 Sandbox 抽象** | ✅ **Docker + WASM + subprocess + external + SandboxRouter + RunMode + SessionSandbox** | ❌ 无人原生支持 |
| Headless | ✅ | ✅ |

**31 个 REST API 模块**：
```
agents / audit / autonomous / budget / collaboration / config / context / error
eval_sessions / events / executions / hooks / knowledge_graph / mcp_logs
mcp_servers / mcp_tools / memories / metering / metrics / ...
```

**证据**：
- `crates/grid-runtime/src/service.rs` (17 gRPC methods)
- `crates/grid-server/src/api/` (31 REST 模块)
- `crates/grid-platform/` (multi-tenant crate)
- `crates/grid-sandbox/` + `crates/grid-engine/src/sandbox/` (10 文件)

**裁决**：**没有任何一个候选项目同时拥有 gRPC + REST + WS + CLI + Desktop + 多租户 + 沙箱的完整 stack**。Grid 的服务广度是唯一量级。

#### 维度 5：场景契合度 — ⭐⭐⭐⭐⭐ **完美契合**

因为 Grid 本来就是为 EAASP 设计的——不是巧合，它是定义方。

| 维度 | Grid |
|---|---|
| EAASP 16 方法 gRPC 契约 | ✅ 原生实现（17 个 method） |
| SessionPayload 5-block | ✅ `session_payload.rs` 243 行 + `harness.rs` 548 行 |
| HookBridge 协议 | ✅ 独立 `grid-hook-bridge` crate |
| L2 Memory Engine 集成 | ✅ `l2_client.rs` |
| L3 Governance 集成 | ✅ `contract.rs` validate_policy |
| L4 Orchestration 集成 | ✅ `emit_event` / `emit_telemetry` |
| Telemetry (OTel) | ✅ `telemetry.rs` 12.3KB |
| Certifier | ✅ `eaasp-certifier` 独立 crate |
| Enterprise SDK | ✅ `sdk/python/` |
| 多 provider | ✅ Anthropic + OpenAI + chain + smart_router + response_cache |
| 企业治理面 | ✅ **6 个独立子模块** (`metering/audit/auth/secret/security/tls`) |

### 11.3 包含 Grid 的完整排名（7 项目）

| 排名 | 项目 | Hook | MCP | Skill | 服务方式 | 场景 | 综合 |
|---|---|---|---|---|---|---|---|
| 🏆 **1** | **Grid** | ⭐⭐⭐⭐⭐ (17pt+9act+6transport+gRPC) | ⭐⭐⭐⭐ (stdio+SSE+OAuth+L2) | ⭐⭐⭐⭐ (v2+L2+semantic) | ⭐⭐⭐⭐⭐ (gRPC+REST+WS+CLI+Desktop+Platform+Sandbox) | ⭐⭐⭐⭐⭐ (原生 EAASP) | **Excellent+** |
| 2 | CCB | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | **Excellent** |
| 3 | Goose | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | **Good** |
| 4 | Nanobot | ⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ | **Acceptable** |
| 5 | claw-code | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ❌ | ⭐⭐ | ⭐⭐⭐ | **Marginal** |
| 6 | Pi | ⭐⭐⭐⭐ | ❌ | ⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐ | **Poor** |
| 7 | CCR | ⭐ | ⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ | **Marginal** |

### 11.4 重新定位 6 个外部项目的真实角色

在 Grid 已经是完整实现的前提下，6 个外部项目只可能扮演 3 种角色：

#### 角色 A：平行 L1 实例

在 Grid 宿主内额外托管一个异构 agent runtime，用于**扩展生态兼容性**。

- **当前已有**：`claude-code-runtime` (Python) + `hermes-runtime` (Python)
- **建议新增**：**CCB** 作为第三个 L1 实例（TypeScript/Bun subprocess），覆盖 Anthropic Claude Code **skill 生态**
- **理由**：CCB 的 `src/skills/` 是 EAASP Skill v2 frontmatter 的上游规范完整实现，托管 CCB 意味着 EAASP 可以直接消费原生 Claude Code skill 生态

#### 角色 B：特定能力参考

Grid 从中借鉴某个子系统的设计或填补短板。

| 短板 | 借鉴来源 | 预期收益 |
|---|---|---|
| MCP 只有 2 transport | **CCB** `client.ts` 116KB（4 transport） | 加 StreamableHTTP + WebSocket 支持 |
| Skill 来源只有 3 种 | **CCB** 5 种 (`bundled/plugin/managed/mcp/skills`) | 扩 `SkillSource` enum，覆盖 plugin+MCP 两种新场景 |
| Batch hook 语义缺失 | **Nanobot** `before_execute_tools(context.tool_calls)` | 新增 `PreToolBatch` HookPoint 处理事务/预算/组合风险场景 |
| Hook Agent transport | **CCB** `Agent` hook variant | 允许用 subagent 实现 hook 审批逻辑 |
| HookResult 字段对照 | **claw-code** `{denied/permission_override/updated_input/permission_reason}` | 交叉检查 Grid 的 HookAction 是否有缺失字段 |

#### 角色 C：Skill 规范下游

因为 EAASP skill v2 frontmatter 是 Anthropic Claude Code 规范的扩展，CCB 作为规范完整实现意味着 **Grid 和 CCB 之间的 skill 文件可以无损互通**——这为 L2 skill 市场打开了 Anthropic 生态的大门。

### 11.5 Grid 的 4 个真实短板 + 改进清单

自评要诚实。对照 6 个项目，Grid 有 4 个可改进的地方：

| # | 短板 | 证据 | 借鉴来源 | 工作量估计 | 建议 Stage |
|---|---|---|---|---|---|
| 1 | MCP 只有 stdio + SSE 两种 transport | `mcp/stdio.rs` + `mcp/sse.rs` | **CCB** `client.ts` | 1 周 | S5 |
| 2 | Skill 来源只有 3 种 | `loader.rs` 的 bundled+project+user | **CCB** 5 种 | 3-5 天 | S5 |
| 3 | 缺批级 hook 语义 | 17 HookPoint 全是单 tool 或 session 级 | **Nanobot** | 1 周（含 ADR + schema v2.1） | S5/S6 |
| 4 | Hook 缺 Agent transport | `declarative/` 仅 Command/Prompt/Webhook | **CCB** Agent variant | 1 周（需对接 subagent runtime） | S6 |

**总工作量**：约 3-4 周。完成后 Grid 在五维上**每一维都是唯一冠军**。

### 11.6 对前面 7-9 节最终推荐的修正

前文 7.1 节推荐"CCB 作为第一名 L1 候选"，**措辞不够准确**。正确表述：

> ~~第一名：CCB（最佳 L1 候选）~~
> **修正：Grid 是现有 L1 参考实现，CCB 是最值得新增的第三个 L1 实例（平行角色）。**

前文 9.1 节的"CCB 2-Day Spike"建议仍然有效，但目标应重新表述为：

> ~~验证 CCB 作为 L1 runtime 的可行性~~
> **验证 CCB 作为 Grid 宿主下的第三个 L1 实例的可行性，以及从 CCB 借鉴哪些子系统（4 transport MCP / 5 skill 来源 / Agent hook）来改进 Grid 本体。**

### 11.7 核心结论（TL;DR）

1. **Grid 不是 L1 候选，Grid 是 EAASP 的 L1 参考实现基线**
2. **5 维综合评分 Grid 第一**，尤其在 Hook（17pt+9act+6transport+原生 gRPC）、服务方式（gRPC+REST+WS+CLI+Desktop+Platform+Sandbox 完整 stack）、场景契合度（原生 EAASP 协议）三个维度上是唯一量级
3. **6 个外部项目的真实角色**：平行 L1 实例 / 能力参考 / skill 规范下游
4. **CCB 最适合作为第三个 L1 实例**（继 claude-code-runtime + hermes-runtime 之后），继承 Anthropic Claude Code skill 生态
5. **Grid 有 4 个真实短板**（MCP 2 transport / skill 3 来源 / 无批级 hook / 无 Agent hook transport），S5-S6 阶段可 3-4 周补齐
6. **Grid 的 6 子系统是 6 候选都没有的企业级治理面**（metering/audit/auth/secret/security/tls）——这不是对比的维度，是 Grid 独占的领先

---

---

## 12. 候选池拓宽调研记录（2026-04-12，未下结论）

> **本节追加于 2026-04-12**，目的是**为 L1 Runtime Pool 生态开放收集更多候选**，而不是"选最佳 L1"。
>
> **分层定位校正（重要）**：T0/T1/T2/T3 分层的原始意图是**降低不同技术栈团队接入 EAASP 的门槛**——让使用 Rust 的团队有 Rust 入口、用 Python 的团队有 Python 入口、用 TypeScript 的团队有 TS 入口、熟悉某个 agent 框架的团队可以基于那个框架做一个 L1 适配器。
>
> **因此本节只记录原始调研内容，不下选型结论，不重排前 11 节的排名。每个候选都是未来可能被某个团队选中作为 L1 Runtime 起点的种子。**

### 12.1 分层定位的重新澄清

| Tier | 原始定义 | 生态目的 |
|---|---|---|
| **T0** | Governance-native hybrid（治理和 agent 内核共生，不是事后加的 hook） | 为**治理/合规团队**提供起点 |
| **T1** | Claude Code / OpenAI Codex 等开源复刻 | 为**熟悉 Claude Code 生态**的团队提供起点，继承 Anthropic skill 规范 |
| **T2** | 生产级 agent server（原生 HTTP/gRPC/WS，multi-tenant，非 IDE 工具） | 为**企业后端团队**提供起点，直接对接 K8s/云平台 |
| **T3** | 传统 AI Framework（LangGraph / Pydantic AI / AutoGen / CrewAI / LlamaIndex / Semantic Kernel）+ 实验性 framework | 为**已有 AI/ML 项目经验**的团队提供起点，复用现有框架知识 |

**关键认知**：L1 Runtime Pool 的价值**不在于选出"最佳"**，而在于**覆盖面足够宽**——当一个团队想把他们现有的 agent 接入 EAASP 时，能找到**最接近他们技术栈**的参考实现。

### 12.2 T0 候选调研（governance-native hybrid）

#### 12.2.1 Microsoft Agent Governance Toolkit

- **GitHub**：`github.com/microsoft/agent-governance-toolkit`
- **License**：MIT
- **发布时间**：2026-04-02 首发（Public Preview v2.1.0）
- **原始描述**：Policy Enforcement Kernel + Runtime Supervisor 双模块架构
- **关键组件**：
  - `agent-os-kernel`（Policy Engine）
  - `agentmesh-runtime`（Runtime Supervisor）
  - `agentmesh-mcp`（独立 Rust crate，做 "MCP governance and security primitives"）
  - `agentmesh-marketplace`（插件市场）
- **Hook 能力**：每个 agent action（tool call / resource access / inter-agent comm）在执行前被策略引擎拦截。明确覆盖 **OWASP Agentic Top 10** 全部 10 项。
- **服务方式**：多语言 SDK (Python / TS / .NET / Rust / Go)，Azure AKS / Container Apps 部署指引，**OpenClaw sidecar 模式**（和任意 agent 框架并排运行的 governance sidecar）
- **定位特殊性**：**不是 agent runtime 本身**，而是一个 "governance sidecar"，宣称 "works with LangChain / CrewAI / AutoGen / OpenAI Agents / LlamaIndex"
- **EAASP 潜在价值**：
  - ⚠️ 可能**不适合做 L1**（它不是 runtime）
  - ✅ 但可能非常适合做 **L3 HookBridge 的真实后端**（提供 OWASP Agentic Top 10 全套策略引擎 + Rust crate）
  - 需要进一步研究 `agentmesh-mcp` 的接口面，看是否能直接作为 EAASP L3 的策略后端
- **调研状态**：**待深度阅读官方文档 + 架构白皮书**

#### 12.2.2 Permit.io cedar-agent

- **GitHub**：`github.com/permitio/cedar-agent`
- **License**：Apache 2.0
- **性质**：**不是 agent runtime**，是 Cedar 策略引擎的 HTTP 服务器
- **EAASP 潜在价值**：L3 策略后端候选，不参与 L1 选型
- **调研状态**：已了解定位，无需深挖

#### 12.2.3 T0 品类的检索结论

- Reddit `r/AI_Agents/1ribz4g`（2026-03）明确指出 "没有广泛认可的 open-source production agent runtime 覆盖全部 governance 特性"
- **推论**：严格意义的 "governance-native agent runtime" 在 2026-04 仍是空白市场
- **对 EAASP 的意义**：T0 很可能需要**自研**或**组合**（L1 agent + L3 governance sidecar）

### 12.3 T1 候选调研（Claude Code / Codex 复刻增强）

#### 12.3.1 OpenCode (anomalyco/opencode)

- **GitHub**：`github.com/anomalyco/opencode`
- **License**：MIT
- **官方文档**：`opencode.ai/docs/plugins`, `opencode.ai/docs/mcp-servers`
- **活跃度**：2026-04 仍在更新
- **架构亮点**：
  - **client/server 架构**——TUI 只是一个 frontend，可以远程驱动
  - 原生 plugin hook 系统，有 `tool.execute.before` / `tool.execute.after`
  - 明确与 Claude Code hooks 规范对齐
- **MCP 能力**：原生 MCP client，支持 stdio + SSE + HTTP，**自动 OAuth 处理**
- **Skill / Extension**：agents / commands / modes / plugins 四类扩展，社区已有完整 skill 模板仓库（如 `jjmartres/opencode`）
- **已知问题**：
  - Issue #5894 — subagent 层 hook 尚有瑕疵
  - Issue #21146 — MCP tool 的 hook 有已知 bug
  - 但 API 面已齐全
- **EAASP 潜在价值**：
  - T1 分层里**除 CCB 外最值得纳入**的候选
  - client/server 架构天然适合 L1 适配（不用像 claw-code 那样从 0 加 server 层）
  - 可以作为**"TypeScript 团队起点"**，与 CCB 并列
- **调研状态**：**待本地 clone + 源码阅读**（重点读 `tool.execute.before` hook 实现 + MCP OAuth 自动化）

#### 12.3.2 其他 Claude Code alternative 筛除说明

Cline / Aider / Codex CLI / Gemini CLI 等项目大多是 **IDE 客户端**，不是可独立服务化的 runtime——**不纳入 T1 候选池**。

### 12.4 T2 候选调研（生产级 agent server）

#### 12.4.1 Agno 2.0 + AgentOS

- **GitHub**：`github.com/agno-agi/agno`
- **官网**：`agno.com`
- **License**：MPL-2.0
- **活跃度**：2025-09 v2 重写 + 2026-03 有生产案例发布
- **Hook 机制**：
  - 原生 `pre_hook` / `post_hook`（v1.0 起提供）
  - 明确用途："security guardrails, PII, validation"
  - 官方文档：`docs.agno.com/hooks/overview`
  - 特色：**background post-hooks**（作为 FastAPI background task 非阻塞执行）
- **MCP 能力**：
  - 完整 `MCPTools`，支持 multi-server + stdio/SSE
  - Issue #5568 正在加 multi-tenant 动态 header（A2A 协议场景）
- **Skill**：有 SKILLs 加载机制（外部 substack 案例："5 SKILLs that load on demand"）
- **服务方式**：
  - **AgentOS = FastAPI app with ready-to-use API endpoints**
  - 原生 HTTP 服务器 + multi-agent + A2A 协议支持
- **场景定位**：明确宣称 "enterprise-ready agentic operating system"
- **外部评价**：Starlog 评测称其为 "production runtime that treats agent approval as first-class"
- **EAASP 潜在价值**：
  - Python 生态里目前发现的**最成熟完整**的候选（hooks + MCP + HTTP server + multi-tenant 全齐）
  - 可以作为 **"Python 团队起点"**，与 Nanobot 并列或取代 Nanobot
- **调研状态**：**待本地 clone + 源码阅读**（重点读 `pre_hook` / `post_hook` 实现 + AgentOS FastAPI 端点注册机制）

#### 12.4.2 HexAgent (UnicomAI/hexagent)

- **GitHub**：`github.com/UnicomAI/hexagent`
- **License**：Apache 2.0
- **背景**：中国联通企业内部项目开源
- **活跃度**：2026-03-29 活跃
- **架构亮点**：
  - 独创 **"Computer 协议"**：把 agent runtime 和 computer（执行环境）解耦
  - 支持 LocalNativeComputer / LocalVM / RemoteE2BComputer 可插拔
  - README 架构图显式标注 "Middleware & hooks"
- **与 EAASP 的潜在共鸣**：Computer 协议的解耦理念和 EAASP 的 `docs/design/SANDBOX_EXECUTION_DESIGN.md` 四种沙箱执行模式**高度同构**
- **场景定位**："production, multi-tenant, CI/CD"
- **未验证事项**：
  - MCP 能力证据不足
  - hook 细节需要看源码确认
- **EAASP 潜在价值**：
  - 作为 **"非 Claude Code 系 harness"** 的参考实现
  - Computer 协议可能反哺 EAASP 的沙箱抽象设计
- **调研状态**：**待本地 clone + 架构文档阅读**

#### 12.4.3 Wippy (wippyai)

- **GitHub**：`github.com/wippyai`
- **官网**：`wippy.ai/en/start/about`
- **License**：Apache 2.0 core
- **外部评价**：SpiralScout 2026-03 文章明确说 Wippy 是 "agent platform and runtime for software that needs to change while it's running"，Apache 2.0 可自托管
- **定位特殊性**："runtime for plugin architectures"——专注 plugin 热更新
- **未验证事项**：GitHub README 未抓到详细内容
- **调研状态**：**需二次深挖**

#### 12.4.4 datalayer/agent-runtimes

- **GitHub**：`github.com/datalayer/agent-runtimes`
- **定位**："expose AI agents through multiple protocols"
- **架构方向**：明确与 EAASP 的多协议暴露理念契合
- **未验证事项**：repo 细节未抓到
- **调研状态**：**需二次深挖**

#### 12.4.5 LangGraph Platform（作为 server 候选）

- **GitHub**：`github.com/langchain-ai/langgraph`
- **Server 形态**：LangGraph Platform 2025-05 GA，支持 1-click deploy
- **Hook 能力**：有 breakpoints / interrupts / human-in-the-loop，但**是图节点级别**（不是 per-tool）
- **MCP 能力**：2025-07 起已支持 MCP
- **语义错位**：LangGraph 的 agent 抽象是"状态机图"，与 EAASP 的 "session + tool + hook" 语义不对齐——per-tool hook 拦截需要 **重新发明** 而非直接映射
- **调研状态**：架构定位已明确，不推荐作为 T2 L1 起点，但可作为 **T3 代表**（见下）

### 12.5 T3 候选调研（传统 AI Framework）

#### 12.5.1 LangGraph (LangChain)

- **GitHub**：`github.com/langchain-ai/langgraph`
- **Hook**：图节点 breakpoint/interrupt（**非 per-tool**）
- **MCP**：✅ 2025-07 起支持
- **Skill**：❌ 无 skill 格式
- **Server**：✅ LangGraph Platform GA
- **抽象错位**：agent = 图节点状态机
- **L1 适配成本**：**高**——需在图节点外层包一层 per-tool hook 拦截，等于另起炉灶
- **适合团队**：已有 LangGraph 资产且愿意做大量 glue code 的团队

#### 12.5.2 Pydantic AI

- **GitHub**：`github.com/pydantic/pydantic-ai`
- **官方 MCP 文档**：`ai.pydantic.dev/mcp/overview`
- **Hook**：✅ **原生 hooks 系统**
  - decorator + constructor 两种注册方式
  - 含 tool filtering / wrap hooks / timeouts
  - hook 语义是 **function call 级别**——这一点和 Claude Code hooks 对齐
- **MCP**：✅ 原生支持
- **Skill**：部分（无 Markdown frontmatter 概念）
- **Server**：⚠️ 定位为 library，**无内置 HTTP server**
- **L1 适配成本**：**中等**——hook 和 MCP 都原生，但要自己包 FastAPI/gRPC server 层
- **适合团队**：已有 Pydantic 生态、Python 类型驱动开发文化的团队
- **独特价值**：**6 个 T3 候选里 hook 机制最干净**

#### 12.5.3 AutoGen (Microsoft)

- **GitHub**：`github.com/microsoft/autogen`
- **Hook**：⚠️ 事件记录但缺显式 per-tool hook
- **MCP**：⚠️ 社区适配
- **Skill**：❌
- **Server**：⚠️ 主要是 library
- **抽象**：multi-agent conversation loop
- **L1 适配成本**：**高**——hook 拦截需要从 event log 反推
- **适合团队**：专注 multi-agent 对话场景的团队

#### 12.5.4 CrewAI

- **GitHub**：`github.com/crewAIInc/crewAI`
- **Hook**：⚠️ 静态 workflow，**不是 hook 模型**
- **MCP**：⚠️ 可以做 MCP server 但非原生
- **Skill**：❌
- **Server**：✅ CrewAI AMP（Agent Management Platform）
- **抽象错位**：agent = role + task 的 crew
- **L1 适配成本**：**高**——workflow 模型和 session+tool+hook 模型错位

#### 12.5.5 LlamaIndex Agents

- **GitHub**：`github.com/run-llama/llama_index`
- **Hook**：❌ RAG 导向，非 agent runtime
- **MCP**：⚠️ 社区适配
- **Skill**：❌
- **Server**：❌
- **L1 适配成本**：**极高**——定位本身就是 RAG 框架，不是 agent runtime

#### 12.5.6 Semantic Kernel (Microsoft)

- **GitHub**：`github.com/microsoft/semantic-kernel`
- **Hook**：✅ **Function Invocation Filter**（.NET 原生、Python 有）——这是 .NET 生态里 hook 最干净的
- **MCP**：✅ 2025-03 起官方 devblog 确认支持
- **Skill**：❌（SK 的 "skill" 是代码级 plugin，不是 Markdown frontmatter）
- **Server**：⚠️ library 为主
- **L1 适配成本**：**中等**——hook 机制好，但 .NET 生态与 EAASP 的 Rust/Python/TS 栈**有摩擦**
- **适合团队**：**.NET 技术栈的企业团队**（给微软生态团队一条 L1 接入路径）

#### 12.5.7 Google ADK (Agent Development Kit)

- **官网**：`adk.dev`
- **Hook**：表面上有 `before_tool_callback` / `after_tool_callback`，但 **Issue #4704 证实 live path 下 plugin callback 不触发**
- **MCP**：✅
- **Skill**：⚠️
- **Server**：✅
- **成熟度评价**：**不足**（live path bug 是硬伤）
- **调研状态**：需要观察 Google 的修复节奏再评估

### 12.6 新候选的 GitHub URL 汇总

| 分层 | 候选 | URL | License | 活跃度 |
|---|---|---|---|---|
| T0 | Microsoft Agent Governance Toolkit | `github.com/microsoft/agent-governance-toolkit` | MIT | 2026-04 首发 |
| T0 | Permit.io cedar-agent | `github.com/permitio/cedar-agent` | Apache 2.0 | 中等活跃 |
| T1 | OpenCode | `github.com/anomalyco/opencode` | MIT | 2026-04 活跃 |
| T2 | Agno 2.0 / AgentOS | `github.com/agno-agi/agno` | MPL-2.0 | 高活跃 |
| T2 | HexAgent | `github.com/UnicomAI/hexagent` | Apache 2.0 | 2026-03 活跃 |
| T2 | Wippy | `github.com/wippyai` | Apache 2.0 | 未深挖 |
| T2 | datalayer/agent-runtimes | `github.com/datalayer/agent-runtimes` | 未确认 | 未深挖 |
| T3 | LangGraph | `github.com/langchain-ai/langgraph` | MIT | 高活跃 |
| T3 | Pydantic AI | `github.com/pydantic/pydantic-ai` | MIT | 高活跃 |
| T3 | AutoGen | `github.com/microsoft/autogen` | MIT | 高活跃 |
| T3 | CrewAI | `github.com/crewAIInc/crewAI` | MIT | 高活跃 |
| T3 | LlamaIndex | `github.com/run-llama/llama_index` | MIT | 高活跃 |
| T3 | Semantic Kernel | `github.com/microsoft/semantic-kernel` | MIT | 高活跃 |
| T3 | Google ADK | `adk.dev` | Apache 2.0 | 高活跃但有 bug |

### 12.7 按团队技术栈划分的接入路径建议（保留为选项，不下结论）

这一小节为未来贡献者提供"**如果你是 X 团队，可以从 Y 开始**"的指引，所有选项并列，不按排名：

| 团队背景 | 可选起点（T1/T2/T3 分层） |
|---|---|
| **Rust 系统团队** | Goose（T2）/ claw-code（T1）/ CCR（T1）/ Microsoft AGT Rust crate（T0 sidecar） |
| **Python AI/ML 团队** | **Agno 2.0**（T2，新发现）/ Nanobot（T2）/ **Pydantic AI**（T3，需加 server 层）/ HexAgent（T2，Computer 协议） |
| **TypeScript/Node 团队** | CCB（T1）/ **OpenCode**（T1，新发现）/ Pi-mono（T2） |
| **.NET/企业 Microsoft 栈** | **Semantic Kernel**（T3，需加 server 层）/ **Microsoft Agent Governance Toolkit**（T0，sidecar 模式） |
| **已有 LangChain 生态** | **LangGraph**（T3，图模型有语义错位但可接） |
| **已有 multi-agent conversation 场景** | **AutoGen**（T3）/ **CrewAI**（T3） |
| **已有 RAG 生态** | **LlamaIndex**（T3，需重写为 agent loop） |
| **合规/治理优先团队** | Microsoft AGT（T0 sidecar）+ 任意 T1/T2 agent runtime 组合 |
| **希望零改造 EAASP 原生体验** | **Grid**（baseline，Rust 自研） |

### 12.8 调研的未决问题（后续深挖清单）

这些是本次 Web 调研抓到了但证据不足的项目/方向，列出作为**后续任务池**而不是结论：

1. **Wippy** 的 plugin 热更新机制 vs EAASP 的 WASM hook（grid-engine 已有）
2. **datalayer/agent-runtimes** 的多协议暴露机制 vs EAASP 的 gRPC+REST+WS 三合一
3. **Microsoft AGT** 的 `agentmesh-mcp` Rust crate 接口面是否可直接做 L3 策略后端
4. **OpenCode** 的 subagent hook bug（Issue #5894）修复节奏
5. **Google ADK** 的 live path callback bug（Issue #4704）修复节奏
6. **Agno** 的 A2A 协议多租户 header（Issue #5568）是否可对齐 EAASP 的 `session_id` + `tenant_id` 模型
7. **HexAgent** 的 Computer 协议 spec 是否可吸收到 `SANDBOX_EXECUTION_DESIGN.md`

### 12.9 方法论提醒：为什么这一节不下结论

**用户明确指出的意图**：
> "T0/T1/T2/T3 的分类是为了更多的使用不同智能体框架或 AI Framework 的团队可以加入开发 EAASP L1 Runtime，丰富 L1 Runtime Pool"

这改变了候选池调研的目的：
- ❌ 以前的目的："选一个最佳 L1"
- ✅ 真正的目的："**画一张尽可能全的候选地图**，让每个技术栈的团队都能找到自己熟悉的起点"

因此第 12 节只记录**原始调研证据**，不参与前 11 节的"最佳选型"排名。前 11 节的结论仍然有效（作为"Grid 基线 + CCB 第三 L1 实例"的内部路径），但**不应该影响生态开放策略**——我们希望未来有 Python 团队基于 Agno 做 L1、TypeScript 团队基于 OpenCode 做 L1、.NET 团队基于 Semantic Kernel 做 L1，甚至有 LangGraph 团队硬刚适配。

**下一步工作**（不在本文档内做，留给后续独立 session）：
1. 对 Agno / OpenCode / HexAgent / Microsoft AGT 做 4 份独立的**架构摘要文档**（每份 2-3 页），作为贡献者入门包
2. 在 `tools/eaasp-l1-runtimes/` 下规划**分层的候选样例目录**（`t0/` / `t1/` / `t2/` / `t3/`），每个 tier 放一个最简可运行 stub
3. 撰写 `L1_RUNTIME_CONTRIBUTOR_GUIDE.md`（贡献者指南），说明如何把某个 framework 包装成 L1

---

---

## 13. T0-T3 定义校正记录（2026-04-12）

> **本节性质**：**只做记录，不修改原文**。原文指 `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md` 2.3 节"运行时分层（T0-T3）"。本节是基于本文档第 12 节的网络调研证据 + 多轮用户澄清后，对 T0-T3 判据和代表项目的**认知更新**。原文何时更新由另一次独立决策触发。

### 13.1 校正背景

本次校正由以下 4 条用户澄清触发（2026-04-12 会话中依次给出）：

1. **T1 判据澄清**："T1 是完整的 MCP + Skills + Hooks **直接对齐 EAASP 的要求**"
2. **T2 判据澄清**："T2 一般是智能体框架，但某部分不太完整"
3. **身份澄清**："claude-code-runtime 不就是 Claude Agent SDK 的实现吗？"
4. **重要性澄清**："前面提到的治理能力框架（Microsoft Agent Governance Toolkit 这一类）很重要"

每一条都修正了本文档前 12 节中一个具体错误。记录如下。

### 13.2 T1 判据（校正后）

**校正后定义**：
> **T1 = 原生完整的 MCP Client + Skills (Markdown+YAML frontmatter) + Hooks (PreToolUse/PostToolUse 等)，且三件套直接对齐 EAASP 规范要求的 runtime 实现**。adapter 薄——只做协议转发。

**关键词是"直接对齐"**，不只是"有"。判据细化：
- **MCP**：原生 MCP Client（stdio + SSE 至少，OAuth 有加分项），能消费 EAASP SessionPayload.mcp_servers 5-block 定义
- **Skills**：Markdown + YAML frontmatter 格式，支持 `name / description / version / allowed-tools`，能直接加载 EAASP Skill v2 frontmatter 的扩展字段（`runtime_affinity / access_scope / scoped_hooks / dependencies`）**或者可以无损映射**
- **Hooks**：function-call 级别（per-tool）的 PreToolUse/PostToolUse 拦截点，拦截返回值语义能映射 `{Allow / Deny / Modify}` 三元决策

**Adapter 典型工作量**：1-2 天（只做 gRPC transport 包装）

**不是 T1 的常见反例**：
- 有 MCP 但 skill 是代码注册（无 Markdown frontmatter）→ T2
- 有 skill 但 hook 是会话级或图节点级 → T2 或 T3
- 有 hook 和 skill 但没有 MCP → T3（需要 adapter 造 MCP 层）

### 13.3 T2 判据（校正后）

**校正后定义**：
> **T2 = 智能体框架，MCP/Skills/Hooks 三件套中至少一项不完整或不对齐 EAASP 规范**。adapter 要补齐缺失部分 + 做映射转换。

**典型不完整形态**：

| 不完整维度 | 代表 | Adapter 需补齐 |
|---|---|---|
| **无 Skill manifest**（只有 recipe/代码注册） | Goose | 写 Markdown frontmatter → Recipe 映射层 |
| **Hook 粒度不对**（批级、非 per-tool） | Nanobot | 把单 tool hook 拆分/聚合到批级 hook |
| **Server 层薄弱**（channel bridge 为主） | Nanobot | 包 FastAPI/gRPC server 层 |
| **无 MCP**（纯代码 tool） | 部分 framework | 写 MCP client 适配层 |
| **Hook 语义错位**（图节点级、conversation 级） | LangGraph、AutoGen | 事实上已退到 T3 |

**Adapter 典型工作量**：3-7 天

**关键观察**：
- T1 和 T2 的分水岭**不是"有无 hook"**（网络调研证明 2025-2026 主流 runtime 都跟进了 hook），而是**三件套的对齐完整度**
- Goose 可能从"T2 首选"退到"T2 的 skill 维度不完整"——Goose 用 Recipe 而非 Markdown frontmatter skill，这是它和 EAASP 之间最大的 skill 格式差

### 13.4 身份校正：claude-code-runtime ≡ Claude Agent SDK

**原来的错误理解**：我在前 11 节把 `lang/claude-code-runtime-python/` 当成"某个独立第三方 runtime 的 EAASP 包装"，和 CCB/CCR/claw-code 相提并论。

**正确理解**：
> **claude-code-runtime 就是 Claude Agent SDK 的 L1 gRPC 包装**。
>
> Claude Agent SDK 是 Anthropic 官方 Python 库（`pip install claude-agent-sdk`），claude-code-runtime 是"把这个 SDK 包装成 EAASP 16 方法 gRPC 服务"的 adapter。

**因此**：
- 原文 `EAASP_v2_0_EVOLUTION_PATH.md:73` 的 T1 代表列表 "Claude Code, Claude Agent SDK, Octo, Hermes" 里，"Claude Code" 和 "Claude Agent SDK" 指的**是同一个东西**——Claude Code CLI 的底层就是 Claude Agent SDK
- `lang/claude-code-runtime-python/` 是原文点名的 "Claude Agent SDK" 的 EAASP L1 实现
- `lang/hermes-runtime-python/` 是原文点名的 "Hermes" 的 EAASP L1 实现
- 所以 **EAASP 目前已交付的 2 个 T1 L1 实例，正好对应原文 T1 代表清单里的 2 个**（另外 2 个 "Claude Code" 和 "Octo" 还没做——其中 "Claude Code" 和 claude-code-runtime 是同一个）
- 也就是说，**原文 4 个 T1 代表中有 3 个是同一条线的不同名字**（Claude Code = Claude Agent SDK = claude-code-runtime），实际候选池是 `{Claude Code/Agent SDK, Hermes, Octo}` 三个

**对 T1 候选池的影响**：
- T1 **已交付**：claude-code-runtime (Claude Agent SDK 包装) + hermes-runtime = 2/3 覆盖原文代表
- T1 **未交付但在代表清单里**：Octo（Octo 就是 Grid 的前身，在这个仓库 CLAUDE.md 的"Mono-Repo Product Structure"章节就有记录——Grid 本身可以理解为原文 T1 代表 "Octo" 的当前形态）
- T1 **新候选**（不在原文代表清单但值得加入）：CCB、OpenCode、claw-code

**因此 T1 已交付覆盖率实际上可能是 3/3**（claude-code-runtime + hermes-runtime + Grid），但这取决于"Grid 算不算 Octo 的继承"这个定性问题。

### 13.5 T0 判据（校正后，基于用户提供的历史语境）

**用户补充的关键定义**：
> "T0 是在引入 Managed Agents 设计概念时，对 **harness 和 tools 容器分离模式**的一种实现。"

**校正后的 T0 定义**：
> **T0 = Harness 和 Tools 容器物理分离的架构模式**。agent 主体（harness）在一个容器/进程里，tools 执行环境（另一个容器/VM/远程 sandbox）通过解耦协议通信。"Managed Agents" 是这种模式最早被广泛讨论的语境（Anthropic 用这个概念描述 Computer Use 产品），但 T0 本身是**架构模式**，不是"商业托管服务"。

**关键特征**：
- harness 和 tools 在**不同进程/容器/甚至不同机器**
- 通过解耦协议通信（Computer Protocol / Sandbox API / RPC）
- 凭证和策略通过**协议层**注入，不内嵌在 harness 或 tools 任一侧
- tools 容器可独立缩放/隔离/换掉
- 适合多租户、跨信任域场景（例如"agent 在云端，tools 在客户侧内网"）

**已知实现候选**：
- **HexAgent** (`github.com/UnicomAI/hexagent`，Python，Apache 2.0) — 独创 "Computer 协议"，LocalNativeComputer / LocalVM / RemoteE2BComputer 可插拔。**这是最接近 T0 原始意图的开源实现**
- **Anthropic Computer Use** (2024-10) — Claude 操作远程虚拟桌面，商业封闭但理念同源
- **E2B.dev** — Cloud sandbox 作为远程 tool 执行环境（HexAgent 直接集成）
- **Browserbase** — 类似的远程 headless 浏览器即 tool 容器模式
- **OpenAI Apps SDK** — 同类分离模式

**EAASP 本体呼应**：
- `docs/design/SANDBOX_EXECUTION_DESIGN.md` 的四种沙箱执行模式
- `grid-engine/src/sandbox/{docker,wasm,external,subprocess}.rs` 已经有接口基础
- **Grid 的 `external` sandbox 适配器本身就是 T0 理念的一种实现**（subprocess 容器 / 外部进程 tool）

**T0 Adapter 典型工作量**：未估算（EAASP Phase 0 明确不做 T0 实例）

**T0 已交付**：无

### 13.6 治理框架独立线（平行于 T0-T3）

**用户强调**：
> "前面提到的治理能力的框架很重要"

**关键认知**：
- **治理框架不是 L1 候选**，和 T0-T3 的 L1 分层**平行共存**
- 它们是 **L3 HookBridge 的可替换后端**——EAASP L3 可以不自己实现所有策略引擎，而是对接某个现成的治理框架
- 这比"选一个 L1"对 EAASP 架构的长期影响**可能更大**

**已识别的治理框架**：

| 项目 | GitHub | License | 定位 | 与 EAASP 对接方式 |
|---|---|---|---|---|
| **Microsoft Agent Governance Toolkit** | `github.com/microsoft/agent-governance-toolkit` | MIT | Policy Enforcement Kernel + Runtime Supervisor，覆盖 OWASP Agentic Top 10 全 10 项 | 通过 `agentmesh-mcp` Rust crate 对接 EAASP L3 HookBridge gRPC 协议 |
| **Permit.io cedar-agent** | `github.com/permitio/cedar-agent` | Apache 2.0 | Cedar 策略引擎 HTTP 服务器 | 作为 EAASP L3 的策略评估后端（HTTP 调用） |
| **Open Policy Agent (OPA)** | `github.com/open-policy-agent/opa` | Apache 2.0 | 通用策略引擎，Rego DSL | 作为 EAASP L3 的策略评估后端（Rego 评估） |

**Microsoft AGT 的特殊重要性**：
- **是目前业界第一个系统性尝试**——覆盖 OWASP Agentic Top 10 全套
- **9500+ 测试 + MIT 许可 + 多语言 SDK**（Python / TS / .NET / Rust / Go）
- **有独立 Rust crate** (`agentmesh-mcp`)，可以直接嵌入 `grid-hook-bridge`
- **官方宣称 "works with LangChain / CrewAI / AutoGen / OpenAI Agents / LlamaIndex"**——这意味着如果 EAASP L3 对齐 Microsoft AGT，**所有这些框架的 L1 实例都能自动获得 AGT 提供的治理能力**

**对 EAASP 架构路径图的可能影响**：
- 如果 S4-S5 阶段启动 "EAASP L3 ↔ Microsoft AGT 对接" 独立评估线
- 并且对接可行
- 那 EAASP 不需要自己造 OWASP Top 10 策略库——**直接站在微软的 governance 生态上**
- 这可能大幅降低 EAASP 自研 L3 的工作量，把精力集中在 L1 Runtime Pool 的生态开放上

**调研状态**：仅 web 摘要，待深度阅读：
1. Microsoft AGT 官方 whitepaper
2. `agentmesh-mcp` Rust crate 接口面
3. EAASP `hook_bridge.proto` 与 AGT 协议的差距评估

**建议**：开一条**独立的 "治理框架对接评估" 工作线**（不在 L1 选型内），优先级 P1，触发时机在 S4 完成后。

### 13.7 前 12 节错误清单

本次校正暴露了前 12 节的若干具体错误：

| # | 所在节 | 错误 | 纠正 |
|---|---|---|---|
| E1 | 12.1 | 用自编的 "governance-native hybrid" 定义 T0 | T0 是 harness-tools 容器分离模式，代表是 HexAgent Computer 协议 |
| E2 | 12.4, 前 11 节 | 把 claude-code-runtime 当成独立第三方候选 | claude-code-runtime 是 Claude Agent SDK 的 L1 gRPC 包装，就是原文 T1 代表本身 |
| E3 | 全文 | 完全忘记提及 `lang/claude-code-runtime-python/` 和 `lang/hermes-runtime-python/` | 这两个是 EAASP 目前唯二的已交付 L1 实例，是 T1 的实证 |
| E4 | 12.2 | 把 Microsoft AGT 试图归入 T0 L1 候选 | AGT 不是 L1，是 L3 HookBridge 后端，和 T0-T3 平行 |
| E5 | 13.3 之前 | 用 "T2 = 无原生 hook" 作为判据 | T2 真正判据是"三件套某部分不完整"，hook 有无不是关键 |
| E6 | 前几轮分析 | 把 Agno / OpenCode 默认归到 T2 | 需要看源码确认 skill 是否对齐 Anthropic frontmatter 格式，可能是 T1 |

### 13.8 校正后的当前 T0-T3 最佳候选快照

**基于本次校正，当前已知的最佳 L1 候选如下**（仍需本地源码验证 tier 归属）：

| Tier | 判据 | 🥇 首选候选 | 🥈 次选 | EAASP 已交付实例 |
|---|---|---|---|---|
| **T0** | Harness-Tools 容器分离 | **HexAgent** (Computer 协议，Python) | E2B.dev / Anthropic Computer Use | ❌（Phase 0 不做） |
| **T1** | MCP+Skills+Hooks 三件套完整对齐 EAASP | **Claude Agent SDK** (Python/TS) | CCB / OpenCode / claw-code | ✅ **claude-code-runtime** (= Claude Agent SDK 包装) + ✅ **hermes-runtime** |
| **T2** | 智能体框架，某部分不完整 | **Goose** (Rust，skill 维度不完整：用 Recipe 非 frontmatter) | Agno 2.0 / Nanobot / Aider / Cline / Roo | ❌ |
| **T3** | 传统 AI Framework，厚 adapter | **Pydantic AI** (Python，hook 最干净) | LangGraph / Semantic Kernel / CrewAI / AutoGen | ❌ |
| **治理线**（平行） | L3 HookBridge 后端 | **Microsoft Agent Governance Toolkit** | OPA / cedar-agent | ❌（独立工作线） |

**已交付覆盖率**：
- T0: 0/∞
- T1: 2/3（claude-code-runtime + hermes-runtime，只差 Octo/Grid 的定性认领）
- T2: 0/∞
- T3: 0/∞
- 治理线: 0/3

### 13.9 未决问题（需要后续验证）

1. **"Grid 是原文 Octo 的继承"** — 这个定性问题决定 Grid 在 T1 已交付清单中的位置
2. **OpenCode 的 tier 归属** — 它的 `tool.execute.before/after` 是否对齐 EAASP HookBridge 语义？skill 格式是 Anthropic 兼容还是自定义？
3. **Agno 2.0 的 tier 归属** — `pre_hook`/`post_hook` 是 per-tool 还是批级？skill 格式是什么？
4. **HexAgent Computer 协议的接口面** — 能否直接映射 EAASP SessionPayload.mcp_servers 到 Computer 协议的 tool container 定义？
5. **Microsoft AGT 的 `agentmesh-mcp` Rust crate 接口面** — 能否直接嵌入 `grid-hook-bridge`？
6. **T0 在 Phase 0 之外的时机** — Phase 0 明确不做 T0 实例，但 HexAgent 作为开源实现是否值得作为 Phase 1 启动点？

### 13.10 本节要点总结

1. **T0-T3 判据已更新**，核心是 T1/T2 分水岭从"有无 hook"改为"三件套对齐度"
2. **claude-code-runtime + hermes-runtime 是 EAASP 目前仅有的 2 个已交付 L1 实例**，都在 T1
3. **Claude Agent SDK = Claude Code = claude-code-runtime** 是同一条技术线的三个层次名称
4. **HexAgent Computer 协议是 T0 理念的最佳开源实证**
5. **Microsoft AGT 治理框架是独立的 L3 后端候选**，重要性可能超过某个 L1 选型
6. **不修改原文**——原文 `EAASP_v2_0_EVOLUTION_PATH.md` 2.3 节的更新由另一次独立决策触发，本节只做记录

---

**文档维护**：
- 创建：2026-04-12
- 第 11 节追加：2026-04-12（Grid 自评 + 重新定位）
- 第 12 节追加：2026-04-12（网络调研候选池拓宽，不下结论）
- 第 13 节追加：2026-04-12（T0-T3 定义校正记录，**不修改原文**）
- 下次更新触发：HexAgent / OpenCode / Agno 本地源码深度评估完成时 / Microsoft AGT L3 对接评估启动时 / 任意外部团队贡献第一个非 claude-code/hermes L1 实例时
- 相关 memory：`memory/project_eaasp_v2_l1_runtime_candidate_analysis.md`（待创建）
