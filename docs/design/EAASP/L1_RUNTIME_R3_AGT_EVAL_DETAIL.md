# R3: Microsoft Agent Governance Toolkit 评估报告

> **评估日期**: 2026-04-12
> **AGT 版本**: v3.1.0 (Public Preview)
> **源码路径**: `3th-party/eaasp-governances/agent-governance-toolkit/`

---

## 1. 项目概况

Microsoft Agent Governance Toolkit (AGT) 是一个 **运行时治理框架**，为 AI Agent 提供确定性的策略执行、零信任身份、执行沙箱和 SRE 能力。核心定位："坐在 Agent 框架和 Agent 动作之间，每个工具调用/资源访问/跨 Agent 消息都在执行前经过策略评估"。

### 关键数据

| 指标 | 数值 |
|------|------|
| License | MIT |
| 语言覆盖 | Python (主), TypeScript, .NET, Rust, Go — 5 语言 SDK |
| OWASP Agentic Top 10 覆盖 | 10/10 |
| 测试文件数 | 471 (Python) + 26 (Rust, 含内联 `#[test]`) |
| Rust 代码量 | 31 文件, ~8190 行 |
| 策略评估延迟 | < 0.1ms per action (p50: 0.012ms 单规则) |
| 并发吞吐 | 47K ops/sec @ 1000 agents |
| 框架集成 | 20+ (LangChain, CrewAI, AutoGen, OpenAI Agents, Google ADK, etc.) |

### 包结构

```
packages/
├── agent-os/              # 策略引擎核心 (Python): PolicyEvaluator, StatelessKernel, MCP Gateway
├── agent-mesh/             # 零信任身份 + 信任评分 + 多语言 SDK
│   └── sdks/
│       ├── rust/           # ★ agentmesh + agentmesh-mcp Rust crate
│       ├── typescript/     # @agentmesh/sdk
│       ├── go/             # Go SDK
│       └── ...
├── agent-runtime/          # 执行环(Ring 0-3), Saga 编排, Kill Switch
├── agent-sre/              # SLO, Error Budget, 熔断器, Chaos 测试
├── agent-compliance/       # agt CLI, OWASP 验证, Prompt Defense
├── agent-discovery/        # Shadow AI 发现
├── agent-hypervisor/       # 可逆性验证, 执行计划验证
├── agent-mcp-governance/   # 薄封装: GovernanceMiddleware + AuditMiddleware + TrustGate + BehaviorMonitor
├── agentmesh-integrations/ # 20+ 框架适配器
└── ...
```

---

## 2. 架构分析

### 2.1 核心引擎

AGT 的核心是 **StatelessKernel** (`packages/agent-os/src/agent_os/stateless.py`, 742 行):

- **无状态设计**: 每个请求携带完整的 `ExecutionContext`(agent_id, policies, history)，Kernel 不存储会话状态
- **可插拔状态后端**: `StateBackend` Protocol (MemoryBackend / RedisBackend)
- **水平扩展**: N 个副本无粘性会话
- **熔断器**: CircuitBreaker 包装后端调用
- **可观测性**: 可选 OpenTelemetry

### 2.2 策略引擎

**Python 版** (`agent_os.policies`):

- `PolicyEvaluator` — 声明式规则评估，支持优先级排序、条件匹配 (9 种操作符: EQ/NE/GT/LT/GTE/LTE/IN/CONTAINS/MATCHES)
- `PolicyDocument` / `PolicyRule` / `PolicyCondition` — Pydantic schema
- `OPABackend` — OPA/Rego 集成
- `CedarBackend` — Amazon Cedar 集成
- `AsyncPolicyEvaluator` — 异步版本
- `ConflictResolutionStrategy` — 4 种冲突解决策略: DenyOverrides / AllowOverrides / PriorityFirstMatch / MostSpecificWins
- `PolicyScope` — Global / Tenant / Agent 多级作用域

**Rust 版** (`agentmesh` crate, `policy.rs`):

- `PolicyEngine` — YAML 策略加载 + 4 种决策 (Allow / Deny / RequiresApproval / RateLimited)
- 支持 `capability` / `approval` / `rate_limit` 三种规则类型
- Glob 风格 action 匹配 (`shell:*`, `data.*`, `*`)
- 条件上下文匹配 (`conditions` HashMap)
- 4 种冲突解决策略与 Python 对齐

### 2.3 MCP 安全

**Python**: `mcp_gateway.py` (MCP Security Gateway) + `mcp_security.py` (35KB, MCP Security Scanner)

- 工具允许/拒绝列表
- 参数消毒 (shell 注入, PII 检测)
- 每 Agent 速率限制
- 人工审批工作流
- 结构化审计日志

**Rust**: `agentmesh-mcp` crate (独立) — 完整的 MCP 治理原语

---

## 3. OWASP Agentic Top 10 覆盖度

基于 `docs/OWASP-COMPLIANCE.md` 和源码验证:

| OWASP 风险 | AGT 组件 | 实现深度 | 对 EAASP L3 的价值 |
|-----------|---------|---------|-------------------|
| ASI-01 Goal Hijacking | PolicyEngine deny-list | **深** — 确定性策略拦截 | **高** — 直接映射 managed_hooks |
| ASI-02 Tool Misuse | Capability Model + MCP Gateway | **深** — 能力沙箱 + 输入消毒 | **高** — PreToolUse hook |
| ASI-03 Identity Abuse | DID + Ed25519 + Trust Scoring | **深** — 零信任身份 | 中 — EAASP 有自己的身份层 |
| ASI-04 Supply Chain | AI-BOM v2.0 | **中** — 模型/数据溯源 | 低 — 超出 L3 范围 |
| ASI-05 Code Execution | Execution Rings 0-3 | **深** — 特权分层 | 中 — EAASP 有自己的沙箱 |
| ASI-06 Memory Poisoning | VFS Policies + CMVK | **深** — 策略控制的虚拟文件系统 | **高** — L2 memory 治理 |
| ASI-07 Insecure Comms | IATP + 加密通道 | **中** — 协议级 | 低 — 超出 L3 范围 |
| ASI-08 Cascading Failures | Circuit Breakers + SLOs | **深** — 生产级 SRE | 中 — 可选增强 |
| ASI-09 Trust Exploitation | Approval Workflows | **深** — 人在环中 | **高** — require_approval action |
| ASI-10 Rogue Agents | Kill Switch + Ring Isolation | **深** — 即时终止 | 中 — EAASP 有自己的终止机制 |

**关键发现**: ASI-01/02/06/09 四项对 EAASP L3 有直接价值，可通过对接 AGT 获得成熟实现。

---

## 4. agentmesh-mcp Rust Crate 分析

### 4.1 接口面

**路径**: `packages/agent-mesh/sdks/rust/agentmesh-mcp/`

**Cargo.toml 依赖** (纯 Rust, 零 async 运行时):
- `serde`, `serde_json` — 序列化
- `sha2`, `hmac`, `base64` — 密码学
- `regex` — 模式匹配
- `thiserror` — 错误处理
- `rand` — 随机数

**模块结构** (12 个模块):

| 模块 | 公开 API | 说明 |
|------|---------|------|
| `gateway` | `McpGateway`, `McpGatewayConfig`, `McpGatewayRequest`, `McpGatewayDecision`, `McpGatewayStatus` | **核心**: 执行 deny-list -> allow-list -> sanitization -> rate-limit -> approval 管线 |
| `security` | `McpSecurityScanner`, `McpToolDefinition`, `McpThreat`, `McpThreatType` (6 种) | MCP 工具元数据安全扫描 (投毒/Rug-pull/跨服务器/描述注入/Schema 滥用/隐藏指令) |
| `rate_limit` | `McpSlidingRateLimiter`, `McpSlidingWindowDecision` | 滑动窗口速率限制, trait `McpRateLimitStore` |
| `response` | `McpResponseScanner`, `McpSanitizedResponse` | 响应扫描和消毒 |
| `redactor` | `CredentialRedactor`, `CredentialKind` | 凭证脱敏 (API key, JWT, SSH key, AWS key 等) |
| `audit` | `McpAuditEntry`, trait `McpAuditSink`, `InMemoryAuditSink` | 审计日志抽象 |
| `signing` | `McpMessageSigner`, `McpSignedMessage` | HMAC-SHA256 消息签名 + Nonce 防重放 |
| `session` | `McpSessionAuthenticator`, `McpSession` | 会话认证 + 过期管理 |
| `metrics` | `McpMetricsCollector`, `McpMetricsSnapshot` | 扫描/决策/威胁计数 |
| `clock` | `Clock` trait, `SystemClock`, `FixedClock` | 可注入时钟 (测试友好) |
| `error` | `McpError` | 统一错误类型 |

### 4.2 可嵌入性评估

**优势**:
- **零 async 依赖**: 纯同步 API, 不引入 tokio/async-std, 可直接在任何 Rust 运行时调用
- **trait 抽象**: `McpAuditSink`, `McpRateLimitStore`, `Clock`, `McpNonceStore`, `McpSessionStore` — 全部可自定义实现
- **最小依赖**: 仅 8 个 crate, 编译快, 二进制体积小
- **纯逻辑**: 不包含网络/IO, 只做内存中的策略评估
- **MIT License**: 完全兼容嵌入

**风险**:
- **不包含策略引擎**: `agentmesh-mcp` 只做 MCP 治理 (deny/allow list + 安全扫描), 不含通用策略评估 (`PolicyEngine` 在 `agentmesh` crate 中)
- **无 gRPC/proto 集成**: 纯 Rust struct, 需要自己做 proto ↔ struct 转换
- **无 async**: `McpGateway::process_request()` 是同步的, 在 tokio 运行时中需要 `spawn_blocking` 或确认无阻塞

### 4.3 关键接口示例

```rust
// McpGateway 核心接口 — gateway.rs:96-160
pub fn process_request(&self, request: &McpGatewayRequest) -> Result<McpGatewayDecision, McpError> {
    // 1. 扫描 payload (消毒)
    // 2. 检查 deny_list
    // 3. 检查 allow_list
    // 4. 检查 suspicious payload
    // 5. 检查 rate limit
    // 6. 检查 approval_required
    // → McpGatewayDecision { status, allowed, sanitized_payload, findings }
}
```

```rust
// McpSecurityScanner — security.rs
// 6 种威胁检测: ToolPoisoning, RugPull, CrossServerAttack,
// DescriptionInjection, SchemaAbuse, HiddenInstruction
pub fn scan_tool(&self, tool: &McpToolDefinition) -> Result<Vec<McpThreat>, McpError>
pub fn scan_server(&self, server_name: &str, tools: &[McpToolDefinition]) -> Result<McpSecurityScanResult, McpError>
```

---

## 5. EAASP L3 对接可行性

### 5.1 协议差距分析

| 维度 | EAASP HookBridge | AGT McpGateway | 差距 |
|------|-----------------|---------------|------|
| **传输** | gRPC bidirectional streaming | 纯内存函数调用 | AGT 无网络层, 需包装 |
| **事件类型** | 8 种 (PreToolCall, PostToolResult, Stop, SessionStart/End, PrePolicyDeploy, PreApproval, EventReceived) | 仅 tool call (deny/allow/rate-limit/approval) | AGT 覆盖 PreToolCall + PostToolResult, 不覆盖 Stop/Session/Policy events |
| **决策类型** | allow / deny / mutate / warn | Allowed / Denied / RateLimited / RequiresApproval | 差距小: AGT 缺 "mutate" 和 "warn", 有 RateLimited |
| **策略格式** | managed_settings JSON (managed_hooks_mode_overrides) | YAML/JSON PolicyDocument + OPA/Rego + Cedar | AGT 更丰富, 需要适配层映射 |
| **策略推送** | PolicyUpdate via stream | 无 (静态加载) | 需实现动态策略更新 |
| **遥测** | HookTelemetryBatch gRPC | McpMetricsCollector (内存计数) | 可桥接 |
| **作用域** | managed / frontmatter / user | Global / Tenant / Agent | 可映射 |

### 5.2 对接模式推荐

经过分析, 推荐 **模式 B: 进程内嵌入 + gRPC 适配层**:

```
L1 Runtime → gRPC → [HookBridge Adapter (Rust)]
                         ↓
                    [agentmesh-mcp McpGateway]     ← 进程内调用, <0.1ms
                    [agentmesh PolicyEngine]        ← 进程内调用
                         ↓
                    [managed_settings ↔ PolicyDocument 转换器]
```

**三种模式比较**:

| 模式 | 延迟 | 复杂度 | 推荐度 |
|------|------|--------|--------|
| A: HTTP sidecar (AGT Python 服务) | 1-5ms | 低 — 直接调 AGT Python API | **否** — 引入 Python 依赖 + 延迟不可接受 |
| **B: 进程内 Rust crate 嵌入** | **<0.1ms** | 中 — 需要写适配层 | **推荐** — 零额外延迟, 纯 Rust |
| C: WASM 沙箱 | 0.5-2ms | 高 — WASM 编译 + FFI | **否** — 过度工程化 |

### 5.3 适配层设计

需要实现的组件:

```
crates/grid-hook-bridge/src/
├── agt_bridge.rs          # 新增: AgtHookBridge impl HookBridge
├── agt_policy_mapper.rs   # 新增: managed_settings ↔ AGT PolicyDocument 互转
└── agt_security.rs        # 新增: McpSecurityScanner 集成
```

**AgtHookBridge** 实现思路:

```rust
pub struct AgtHookBridge {
    gateway: McpGateway,           // agentmesh-mcp
    policy_engine: PolicyEngine,    // agentmesh
    security_scanner: McpSecurityScanner,
}

#[async_trait]
impl HookBridge for AgtHookBridge {
    async fn evaluate_pre_tool_call(&self, session_id: &str, tool_name: &str,
        tool_id: &str, input: &Value) -> Result<HookDecision> {
        // 1. McpGateway.process_request() — deny/allow/rate-limit/approval
        // 2. PolicyEngine.evaluate() — 通用策略
        // 3. 组合决策 → HookDecision
    }

    async fn evaluate_post_tool_result(&self, ...) -> Result<HookDecision> {
        // McpResponseScanner.scan_value() — 输出扫描
    }

    async fn evaluate_stop(&self, session_id: &str) -> Result<StopDecision> {
        // AGT 不覆盖 — 直接返回 Complete (保留 EAASP 原生逻辑)
    }
}
```

**managed_settings ↔ PolicyDocument 映射**:

| EAASP managed_settings 字段 | AGT PolicyDocument 字段 | 映射方式 |
|---------------------------|----------------------|---------|
| `managed_hooks[].hook_type: PreToolUse` | `PolicyRule.rule_type: "capability"` | hook_type → rule_type |
| `managed_hooks[].body.tool_pattern` | `PolicyRule.denied_actions / allowed_actions` | body → denied/allowed |
| `managed_hooks[].body.action: "deny"` | `PolicyAction.DENY` | 1:1 |
| `managed_hooks[].mode_override` | — | EAASP 独有, 保留原始逻辑 |
| `managed_hooks[].precedence` | `PolicyRule.priority` | 1:1 |
| — | `PolicyRule.conditions` | AGT 独有, 可扩展利用 |

### 5.4 工作量估计

| 任务 | 预估人天 | 说明 |
|------|---------|------|
| Cargo.toml 引入 agentmesh + agentmesh-mcp | 0.5 | 添加依赖, 确认编译 |
| `AgtHookBridge` impl HookBridge | 3 | 核心适配层, 含 PreToolCall + PostToolResult |
| managed_settings ↔ PolicyDocument 转换器 | 2 | 双向映射 + 动态更新 |
| McpSecurityScanner 集成 | 1 | 工具注册 + 扫描管线 |
| 审计日志桥接 (McpAuditSink → L3 telemetry_events) | 1 | 实现 `McpAuditSink` trait |
| 测试 (单元 + 集成) | 3 | 覆盖 deny/allow/rate-limit/approval + 安全扫描 |
| 文档 + ADR | 1 | AGT 集成 ADR |
| **合计** | **~11.5 人天** | |

---

## 6. 策略模式与 managed_settings 映射

### 6.1 AGT 策略定义格式

AGT 支持三种策略后端:

**YAML 原生** (Python + Rust):
```yaml
version: "1.0"
agent: threshold-agent
policies:
  - name: block-dangerous-tools
    type: capability
    denied_actions:
      - "shell:*"
      - "execute_code"
    allowed_actions:
      - "data.read"
      - "memory_search"
    priority: 100
    scope: agent
    conditions:
      environment: "production"
```

**OPA/Rego** (Python only):
```rego
package agentos
default allow = false
allow {
    input.tool_name == "web_search"
    input.trust_score > 500
}
```

**Cedar** (Python only):
```cedar
permit (
    principal == Agent::"data-analyst",
    action == Action::"data.read",
    resource == Resource::"reports"
);
```

### 6.2 与 EAASP managed_settings 的映射可能性

EAASP `managed_settings` 当前结构:
```json
{
  "managed_hooks": [
    {
      "hook_type": "PreToolUse",
      "body": {
        "type": "command",
        "tool_pattern": "scada_write*",
        "action": "deny"
      },
      "precedence": 100
    }
  ],
  "managed_hooks_mode_overrides": [
    {
      "hook_type": "PostToolUse",
      "mode": "audit"
    }
  ]
}
```

**映射策略**:

1. **hook_type → rule_type**: PreToolUse → `capability`/`approval`, PostToolUse → 输出扫描 (McpResponseScanner), Stop → EAASP 原生
2. **body.tool_pattern → denied/allowed_actions**: `scada_write*` → `["scada_write*"]`
3. **body.action → PolicyAction**: deny → DENY, allow → ALLOW, audit → AUDIT
4. **mode_override → 运行时模式**: AGT 无直接对应, 需在适配层处理
5. **precedence → priority**: 直接映射

**结论**: 基础映射覆盖 ~80% 场景。AGT 的 `conditions` (上下文条件) 和 `OPA/Cedar` 后端可作为 EAASP Phase 2+ 的扩展能力。

---

## 7. 风险评估

### 7.1 License 兼容性

- **MIT License** — 完全兼容嵌入、修改、分发。无 copyleft 风险。

### 7.2 依赖复杂度

**Rust crate 依赖** (极简):
- `agentmesh`: 10 个依赖 (serde, ed25519-dalek, sha2, hmac, regex, thiserror, rand, base64, serde_yaml, serde_json)
- `agentmesh-mcp`: 8 个依赖 (无 ed25519-dalek 和 serde_yaml)
- **零 async 运行时依赖** — 不会与 EAASP 的 tokio 冲突
- 编译时间影响: ed25519-dalek 是最重依赖, 首次编译约 +15s

**Python 包** (如果需要):
- `agent-os-kernel` 依赖链较深 (Pydantic, httpx, etc.) — **不建议引入**

### 7.3 维护活跃度

- Microsoft 官方项目, GitHub Actions CI
- 最新版本 v3.1.0 (2026-03)
- Public Preview 状态 — 可能有 breaking changes before GA
- OpenSSF Scorecard + CodeQL + Gitleaks + ClusterFuzzLite (7 个 fuzz target)

### 7.4 Rust SDK 功能覆盖局限

根据 SDK Feature Matrix:

| 能力 | Python | Rust |
|------|--------|------|
| Policy Engine | 完整 (YAML + OPA + Cedar) | YAML only |
| MCP Security | 完整 (35KB scanner) | 完整 (8190 行, 独立 crate) |
| Execution Rings | 有 | **无** |
| SRE / SLOs | 有 | **无** |
| Kill Switch | 有 | **无** |
| Framework Integrations | 20+ | **无** |

**影响**: Rust 嵌入只能获得策略评估 + MCP 安全扫描。Execution Rings / SRE / Kill Switch 需要自己实现或走 Python sidecar。

### 7.5 风险总结

| 风险 | 级别 | 缓解措施 |
|------|------|---------|
| AGT GA 前 breaking changes | 中 | Pin 到 v3.0.2, 监控 CHANGELOG |
| Rust SDK 功能不全 | 低 | MCP Gateway + PolicyEngine 已够 L3 核心需求 |
| 同步 API 在 async 运行时的阻塞 | 低 | McpGateway 纯计算 (<0.1ms), 无需 spawn_blocking |
| managed_settings 映射遗漏 | 中 | mode_override 需要适配层特殊处理 |

---

## 8. 结论和建议

### 8.1 核心结论

1. **AGT 是 EAASP L3 HookBridge 的优质可替换后端** — 成熟的策略引擎 + MCP 安全扫描, MIT License, Rust 原生 crate, 零 async 依赖
2. **Rust crate `agentmesh` + `agentmesh-mcp`** 可直接嵌入 `grid-hook-bridge`, 延迟 <0.1ms, 不引入额外运行时
3. **OWASP Top 10 中 4 项 (ASI-01/02/06/09)** 通过 AGT 直接获得成熟实现, 无需从零开发
4. **MCP 安全扫描 (6 种威胁检测)** 是 EAASP 当前完全没有的能力, 对接后一次性获得

### 8.2 推荐行动

**Phase 1 (Phase 0.5 后, ~11.5 人天)**:
- 在 `grid-hook-bridge` 中添加 `AgtHookBridge` 实现
- 引入 `agentmesh` + `agentmesh-mcp` 作为可选 feature gate
- 实现 managed_settings ↔ PolicyDocument 双向映射
- 集成 McpSecurityScanner (工具注册时自动扫描)

**Phase 2 (Phase 1-2)**:
- 扩展 OPA/Rego 后端 (需要引入 Python sidecar 或 Go 版 OPA)
- 信任评分集成 (AGT TrustManager ↔ EAASP session trust)
- Approval Workflow 对接 (AGT RequiresApproval → EAASP 人在环中)

**Phase 3**:
- 完整 OWASP Top 10 治理矩阵
- AGT 框架适配器复用 (通过 agentmesh-integrations 为 EAASP L1 Runtime 候选框架提供开箱即用的治理能力)

### 8.3 不推荐的路径

- **不引入 AGT Python 包**: 依赖链深, 运行时复杂, 不如 Rust 嵌入
- **不做 HTTP sidecar**: 延迟不可接受 (1-5ms vs <0.1ms)
- **不尝试 WASM 隔离**: 过度工程化, AGT Rust crate 已经无副作用

---

## 附录: 关键源码路径

| 组件 | 路径 |
|------|------|
| Rust workspace | `packages/agent-mesh/sdks/rust/Cargo.toml` |
| agentmesh crate | `packages/agent-mesh/sdks/rust/agentmesh/` |
| agentmesh-mcp crate | `packages/agent-mesh/sdks/rust/agentmesh-mcp/` |
| McpGateway | `packages/agent-mesh/sdks/rust/agentmesh-mcp/src/mcp/gateway.rs` |
| McpSecurityScanner | `packages/agent-mesh/sdks/rust/agentmesh-mcp/src/mcp/security.rs` |
| PolicyEngine (Rust) | `packages/agent-mesh/sdks/rust/agentmesh/src/policy.rs` |
| PolicyEvaluator (Python) | `packages/agent-os/src/agent_os/policies/evaluator.py` |
| StatelessKernel (Python) | `packages/agent-os/src/agent_os/stateless.py` |
| MCP Gateway (Python) | `packages/agent-os/src/agent_os/mcp_gateway.py` |
| OWASP 合规映射 | `docs/OWASP-COMPLIANCE.md` |
| SDK Feature Matrix | `docs/SDK-FEATURE-MATRIX.md` |
| Benchmarks | `BENCHMARKS.md` |
| EAASP HookBridge proto | `proto/eaasp/runtime/v2/hook.proto` |
| EAASP HookBridge trait | `crates/grid-hook-bridge/src/traits.rs` |
