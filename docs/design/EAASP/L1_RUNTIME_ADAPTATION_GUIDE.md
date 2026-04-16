# EAASP L1 运行时适配指南

> **版本**: v1.0 (Phase 2.5 S2.T1)
> **适用范围**: 任何希望成为 EAASP L1 Runtime 的代理执行运行时
> **参考 ADR**: ADR-V2-006 (Hook 信封协议), ADR-V2-017 (L1 生态策略), ADR-V2-019 (部署模型)

---

## 1. 概述

EAASP L1 Runtime 是 EAASP 体系中负责**执行代理逻辑**的核心组件。L4 编排层通过 gRPC 协议与 L1 Runtime 通信，将技能（Skill）注入运行时并接收事件流。

任何语言、框架均可实现 L1 Runtime，只需满足以下三个条件：

1. 实现 `RuntimeService` gRPC 服务（16 个方法）
2. 遵循 ADR-V2-006 Hook 信封协议
3. 能够响应合约测试套件（`tests/contract/`）的 GREEN 要求

本指南基于 Phase 2.5 W1（goose-runtime，Rust）和 W2（nanobot-runtime，Python）的实践经验提炼而成。

---

## 2. 合约测试套件

### 2.1 位置

```
tests/contract/
├── conftest.py              # 运行时启动 + mock 服务器 + HookProbe fixtures
├── contract_v1/             # 冻结的 v1 合约测试用例（~35 cases）
├── harness/
│   ├── runtime_launcher.py  # RuntimeLauncher：子进程启动 + gRPC stub
│   ├── assertions.py        # 断言辅助函数
│   ├── mock_openai_server.py # FastAPI mock OpenAI 端点
│   ├── mock_anthropic_server.py # FastAPI mock Anthropic 端点
│   └── hook_probe.py        # HookProbe：物化 hook 信封并验证
└── pyproject.toml           # pytest 配置，grpcio-tools 依赖
```

### 2.2 本地运行

```bash
# 安装合约测试依赖（在 repo 根目录）
pip install grpcio grpcio-tools pytest pytest-asyncio httpx uvicorn fastapi

# 针对特定 runtime 运行（--runtime 参数）
cd tests/contract
python -m pytest contract_v1/ --runtime=grid -v         # grid-runtime
python -m pytest contract_v1/ --runtime=claude-code -v  # claude-code-runtime
python -m pytest contract_v1/ --runtime=goose -v        # eaasp-goose-runtime（需 goose 二进制）
python -m pytest contract_v1/ --runtime=nanobot -v      # nanobot-runtime

# 跳过（当 runtime 二进制不可用时）
# conftest.py 的 skip-if-absent 机制会自动跳过，不会 FAIL
```

### 2.3 版本管理

合约版本冻结在 `tests/contract/VERSION` 文件。当前版本：`v1.0.0`（git tag: `contract-v1.0.0`）。

新 runtime 必须使现有合约全部 GREEN，不得修改 `contract_v1/` 下的测试用例。

---

## 3. gRPC 接口：16 个方法快速参考

以下是 `RuntimeService` 的完整方法列表及最小实现说明：

| # | 方法名 | 请求类型 | 返回类型 | 最小实现要求 |
|---|--------|----------|----------|--------------|
| 1 | `Initialize` | `InitializeRequest` | `InitializeResponse` | 创建会话，返回 `session_id` + `runtime_id` |
| 2 | `Send` | `SendRequest` | `stream SendResponse` | 驱动代理循环，流式返回 chunk/tool_call/tool_result/done/error |
| 3 | `Terminate` | `Empty` | `Empty` | 清理活跃会话，释放资源 |
| 4 | `GetCapabilities` | `Empty` | `Capabilities` | 返回 runtime 能力声明（tier/model/deployment_mode 等） |
| 5 | `OnToolCall` | `ToolCallEvent` | `ToolCallAck` | Hook 拦截点：返回 `decision="allow"/"deny"` |
| 6 | `OnToolResult` | `ToolResultEvent` | `ToolResultAck` | Hook 拦截点：返回 `decision="allow"/"deny"` |
| 7 | `OnStop` | `StopEvent` | `StopAck` | Hook 拦截点：返回 `decision="allow"/"deny"` |
| 8 | `LoadSkill` | `LoadSkillRequest` | `LoadSkillResponse` | 接收 SKILL.md 内容，注入工具和 hook 脚本路径 |
| 9 | `GetState` | `Empty` | `StateResponse` | 序列化当前会话状态（用于暂停/恢复） |
| 10 | `RestoreState` | `RestoreStateRequest` | `Empty` | 从序列化状态恢复会话 |
| 11 | `PauseSession` | `Empty` | `StateResponse` | 暂停并序列化（等价于 GetState + 标记暂停） |
| 12 | `ResumeSession` | `ResumeSessionRequest` | `Empty` | 恢复已暂停的会话 |
| 13 | `ConnectMCP` | `ConnectMCPRequest` | `ConnectMCPResponse` | 连接 MCP 服务器（可返回 stub success=true） |
| 14 | `DisconnectMcp` | `DisconnectMcpRequest` | `Empty` | 断开 MCP 连接 |
| 15 | `Health` | `Empty` | `HealthResponse` | 健康检查：返回 `healthy=true` + `runtime_id` |
| 16 | `EmitEvent` | `EventStreamEntry` | `Empty` | 事件上报（可返回 UNIMPLEMENTED） |

> **最小可用集**：仅实现 Initialize / Send / Terminate / GetCapabilities / Health 即可通过基础合约测试。其余方法返回 stub（success=true 或 UNIMPLEMENTED）也可通过。

---

## 4. Send 事件类型语义（7 种 chunk_type）

`Send` 方法是 L1 Runtime 的核心，通过流式返回 `SendResponse`，L4 根据 `chunk_type` 字段区分事件类型：

| chunk_type | 含义 | 必填字段 | 触发时机 |
|------------|------|---------|---------|
| `"text"` | LLM 生成的文本片段 | `content` | LLM 返回非 tool_call 内容 |
| `"tool_call"` | 代理请求执行工具 | `tool_name`, `tool_id`, `content`（参数 JSON） | LLM 返回 tool_calls |
| `"tool_result"` | 工具执行结果 | `tool_name`, `tool_id`, `content`, `is_error` | 工具执行完毕 |
| `"done"` | 会话轮次正常结束 | `content`（最终输出） | 代理停止（无更多 tool_calls） |
| `"error"` | 不可恢复错误 | `content`（错误信息）, `is_error=true` | Provider 异常、超出 max_turns 等 |
| `"hook_fired"` | Hook 脚本执行结果（可选） | `content`（decision） | PostToolUse hook 完成 |
| `"pre_compact"` | 上下文压缩前快照（可选） | `content` | 触发 PreCompact hook 时 |

> **关键规则**：每次 `Send` 调用**必须**以 `"done"` 或 `"error"` 结尾，L4 依赖此信号关闭流。

---

## 5. MCP Bridge 集成要点

L1 Runtime 与 MCP 工具服务器的集成有两种模式：

### 5.1 原生 MCP（推荐，tier="native"）

runtime 直接管理 MCP 连接（如 grid-runtime、claude-code-runtime）：

```
L4 → ConnectMCP(servers=[{name, cmd, args}])
L1 → 启动 MCP 子进程 / SSE 连接 → 注册工具到代理工具列表
L4 → Send(message) → L1 内部通过 MCP 调用工具 → 流式返回事件
```

### 5.2 间接 MCP（stub，tier="aligned"）

runtime 不直接管理 MCP（如 nanobot-runtime）：

- `ConnectMCP` 返回 `success=true, connected=[server_names]`（接受但不实际连接）
- `capabilities.supports_native_mcp = false`
- 工具通过 `StubToolExecutor` / `ToolExecutor` 协议注入

### 5.3 goose-runtime 模式（eaasp-scoped-hook-mcp 中间件）

goose 通过 `extensions` 配置注入 `eaasp-scoped-hook-mcp` 作为 MCP stdio 中间件：

```
L4 → Send → goose_adapter → goose 子进程（ACP stdio）
                               ↓ tools/call
                         eaasp-scoped-hook-mcp（stdio proxy）
                               ↓ Pre/PostToolUse hook dispatch
                         下游 MCP 服务器
```

---

## 6. Hook 信封协议（ADR-V2-006 §2/§3）

### 6.1 stdin 信封结构

Hook 脚本通过 stdin 接收 JSON 信封，格式固定：

```json
{
  "event": "PostToolUse",
  "session_id": "<会话 ID>",
  "skill_id": "<技能 ID 或空字符串>",
  "tool_name": "<工具名>",
  "tool_input": { /* 工具参数 */ },
  "tool_result": "<工具结果字符串>",
  "is_error": false,
  "draft_memory_id": "",
  "evidence_anchor_id": "",
  "created_at": "<ISO 8601 UTC 时间戳>"
}
```

### 6.2 环境变量

| 变量名 | 值 |
|--------|-----|
| `GRID_SESSION_ID` | 当前会话 ID |
| `GRID_TOOL_NAME` | 工具名 |
| `GRID_SKILL_ID` | 技能 ID（可为空） |
| `GRID_EVENT` | 事件类型（`PreToolUse` / `PostToolUse` / `Stop`） |

### 6.3 退出码语义

| 退出码 | 含义 | runtime 行为 |
|--------|------|-------------|
| `0` | 允许（allow） | 继续正常执行 |
| `2` | 拒绝/注入（deny / inject） | 注入 System 消息并继续（InjectAndContinue） |
| 其他 / 超时 | fail-open | 忽略错误，继续执行 |

### 6.4 超时

Hook 脚本的默认超时为 **5 秒**（`HOOK_TIMEOUT_SECS=5`）。超时后 SIGKILL 并 fail-open。

### 6.5 最小实现检查清单

- [ ] `OnToolCall` / `OnToolResult` / `OnStop` 返回正确的 `decision` 字段
- [ ] Hook 信封字段名与 ADR-V2-006 §2.1 完全匹配（包括 `created_at` ISO 8601 格式）
- [ ] `GRID_*` 环境变量在 hook subprocess 中正确传递
- [ ] 超时后 fail-open（不要让 hook 超时导致会话失败）

---

## 7. 最小骨架结构

### 7.1 Python runtime（参考 nanobot-runtime）

```
lang/{runtime-name}-python/
├── src/{runtime_name}/
│   ├── __init__.py          # __version__ = "0.1.0"
│   ├── __main__.py          # grpc.aio server 入口（读 PORT env）
│   ├── service.py           # RuntimeServiceServicer 实现
│   ├── provider.py          # LLM provider（OpenAI compat / Anthropic SDK）
│   ├── session.py           # 代理循环（AsyncGenerator[AgentEvent]）
│   └── _proto/              # 从 proto/eaasp/runtime/v2/*.proto 生成
│       └── eaasp/runtime/v2/
│           ├── runtime_pb2.py
│           ├── runtime_pb2_grpc.py
│           ├── common_pb2.py
│           └── *.pyi        # Pyright 类型存根
├── tests/
│   ├── test_smoke.py        # 包导入 + 基础断言
│   ├── test_service.py      # 直接方法调用（MockContext，不启 gRPC 服务器）
│   ├── test_session.py      # 代理循环单元测试（mock provider）
│   └── test_skill_extraction_e2e.py  # 技能提取 E2E 烟雾测试
└── pyproject.toml           # asyncio_mode = "auto", testpaths = ["tests"]
```

### 7.2 Rust runtime（参考 eaasp-goose-runtime）

```
crates/{runtime-name}/
├── Cargo.toml               # tonic + tokio + prost + anyhow + tracing
├── build.rs                 # tonic_build::compile_protos(["proto/..."])
├── src/
│   ├── lib.rs               # pub mod service; pub mod adapter;
│   ├── main.rs              # tonic Server::builder().add_service(...).serve()
│   ├── service.rs           # RuntimeServiceServer impl
│   └── adapter.rs           # 底层 runtime 适配器（subprocess / SDK）
└── tests/
    ├── service_test.rs      # 基础 gRPC 方法单元测试
    └── skill_extraction_e2e_test.rs  # 技能提取 E2E 烟雾测试
```

---

## 8. 常见陷阱

### 8.1 macOS Clash 代理劫持（trust_env 问题）

**现象**：runtime 向本地 mock OpenAI server（127.0.0.1:XXXXX）发送请求，被 Clash 代理拦截，返回错误或连接超时。

**解决方案**：
- Python：`httpx.AsyncClient(trust_env=False)` — 禁止读取 `HTTPS_PROXY` 等环境变量
- 子进程运行时：设置 `NO_PROXY=127.0.0.1,localhost` 环境变量

### 8.2 Provider 重试策略

**陷阱**：在 runtime 层面加入重试逻辑会导致合约测试中 mock server 被调用次数超预期。

**建议**：L1 Runtime 层**不实现**重试（fail-fast）。重试由 L4 编排层或 ProviderChain（grid 专属）负责。详见 ADR-V2-017 §3.2。

### 8.3 错误分类（ErrorClassifier）

**陷阱**：将所有 LLM 错误都当作不可恢复错误，导致短暂网络故障中断会话。

**建议**：
- `429 RateLimited` → 可重试（graduated backoff）
- `503 ServiceUnavailable` → 可 failover 到备用 provider
- `400 BadRequest` → 不可重试（prompt 有问题）
- 详见 `crates/grid-engine/src/agent/error_classifier.rs` 中的 `FailoverReason` 矩阵

### 8.4 tool_choice 能力矩阵

**陷阱**：假设所有模型都支持 `tool_choice=Required`，导致部分模型陷入无限循环。

**建议**：
- 启动时通过 Eager Probe 检测模型是否支持 `tool_choice=Required`
- 不支持的模型优雅退出并设置 `capabilities.supports_native_hooks=false`
- 详见 `docs/design/EAASP/PROVIDER_CAPABILITY_MATRIX.md` 和 ADR-V2-016

### 8.5 gRPC proto stubs 复用

**建议**：优先从 `lang/claude-code-runtime-python/src/claude_code_runtime/_proto/` 复制 Python stubs（字节等同），避免重复 codegen。复制后修改 `import` 路径为本运行时的包路径。

---

## 9. 部署模型（ADR-V2-019）

| 模式 | 环境变量 | 含义 |
|------|----------|------|
| `shared`（默认） | `EAASP_DEPLOYMENT_MODE=shared` | 一个容器服务多个会话（多路复用） |
| `per_session` | `EAASP_DEPLOYMENT_MODE=per_session` | 每个会话独立容器（强隔离） |

**容错分级**：

| 级别 | 描述 | 适用场景 |
|------|------|---------|
| 基线 | 会话不隔离，无资源限制 | 开发/测试 |
| 加固 A | max_sessions 门控 | 生产 shared 模式 |
| 加固 B | 内存/CPU cgroups | 生产高负载 |
| 加固 C | 网络命名空间隔离 | 多租户 |
| 加固 D | gVisor/seccomp | 不可信代码执行 |

---

## 10. 实施路径建议

对于新 runtime 的实施，推荐以下顺序：

```
第 1 天：
  T1 - 骨架脚手架（Cargo.toml / pyproject.toml + proto stubs + smoke test）
  T2 - Provider 层（LLM 接口，unit test 5 cases）

第 2 天：
  T3 - 代理循环（AgentSession / AgentLoop，unit test 5 cases）
  T4 - 16 个 gRPC 方法（service.rs / service.py，unit test 8 cases）

第 3 天：
  T5 - 合约测试 GREEN（接入 tests/contract/conftest.py）
  T6 - 技能提取 E2E 烟雾测试（mock LLM，unit test 7 cases）
```

完成上述 6 个任务后，新 runtime 即可加入 `v2-phase2_5-e2e` CI 矩阵。

---

## 11. 参考资料

| 文档 | 位置 |
|------|------|
| ADR-V2-006 Hook 信封协议 | `docs/design/EAASP/adrs/ADR-V2-006-*.md` |
| ADR-V2-017 L1 生态策略 | `docs/design/EAASP/adrs/ADR-V2-017-*.md` |
| ADR-V2-019 部署模型 | `docs/design/EAASP/adrs/ADR-V2-019-*.md` |
| L1 候选分析 | `docs/design/EAASP/L1_RUNTIME_CANDIDATE_ANALYSIS.md` |
| L1 能力矩阵 | `docs/design/EAASP/PROVIDER_CAPABILITY_MATRIX.md` |
| 合约测试套件 | `tests/contract/README.md` |
| goose-runtime（W1 参考） | `crates/eaasp-goose-runtime/` |
| nanobot-runtime（W2 参考） | `lang/nanobot-runtime-python/` |
