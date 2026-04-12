# R3: Microsoft Agent Governance Toolkit 评估备忘录

> 评估日期: 2026-04-12
> 评估目标: EAASP L3 对接可行性
> 源码路径: `3th-party/eaasp-governances/agent-governance-toolkit/`

---

## 1. 项目概况

| 属性 | 值 |
|------|-----|
| **项目名称** | Agent Governance Toolkit (AGT) |
| **维护方** | Microsoft |
| **License** | MIT |
| **当前版本** | v3.1.0 (Public Preview) |
| **主语言** | Python (主实现) + TypeScript + .NET + Rust + Go (多 SDK) |
| **测试规模** | 9,500+ 测试 (505 Python 测试文件 + 26 Rust 测试模块) |
| **OWASP 覆盖** | Agentic Top 10 全 10 项 |
| **PyPI 包名** | `agent-governance-toolkit[full]` |
| **Rust crate** | `agentmesh` (核心) + `agentmesh-mcp` (MCP 安全原语) |
| **核心定位** | **Agent 动作(action)治理**，非 LLM 输出/提示词护栏 |
| **性能** | 策略评估 < 0.1ms，72K ops/sec (单规则)，35K ops/sec (50 并发 agent) |

**关键定位区分**: AGT 治理的是 agent **做什么**（tool call、资源访问、inter-agent 消息），不是 LLM **说什么**。这与 EAASP 的 HookBridge 拦截器模型完美互补。

---

## 2. 架构与核心模块

### 2.1 包结构

```
packages/
├── agent-os/           # 核心：策略引擎 + 能力模型 + 审计 + MCP 网关
├── agent-mesh/         # 零信任身份 + 信任评分 + IATP 协议 + 多语言 SDK
│   └── sdks/
│       ├── rust/agentmesh/      # Rust 核心 SDK (策略+身份+信任+审计)
│       ├── rust/agentmesh-mcp/  # Rust MCP 治理原语 (独立 crate)
│       ├── typescript/
│       └── go/
├── agent-runtime/      # 执行环(Ring 0-3) + 沙箱 + Kill Switch
├── agent-hypervisor/   # Saga 编排 + 可逆性验证
├── agent-sre/          # SLO + 熔断器 + 混沌测试 + 渐进发布
├── agent-compliance/   # OWASP 验证 + 策略 lint + CLI (agt)
├── agent-discovery/    # Shadow AI 发现
├── agent-marketplace/  # 插件市场
├── agent-lightning/    # RL 训练治理
├── agent-mcp-governance/ # MCP 治理包 (薄包装)
└── agentmesh-integrations/ # 20+ 框架集成
    ├── langchain-agentmesh/
    ├── crewai-agentmesh/
    ├── openai-agents-agentmesh/
    ├── mcp-trust-proxy/    # MCP 信任代理中间件
    └── ... (mastra, dify, haystack, etc.)
```

### 2.2 架构分层

```
Agent Action ──► PolicyEngine.evaluate() ──► Allow/Deny/Audit ──► AuditLog
                       ↑                          ↑
                  YAML/OPA/Cedar              TrustScoring
                  (策略后端)                  (0-1000 分)
```

AGT 采用**应用层拦截**模型（Python 中间件），非 OS 内核隔离。这与 EAASP 的 gRPC HookBridge 拦截器模型在概念上高度一致——都是"在执行前确定性判定"。

### 2.3 部署模式

| 模式 | 说明 |
|------|------|
| **嵌入式 (In-process)** | `pip install` 后直接 `from agent_os.policies import PolicyEvaluator` |
| **代理式 (Proxy)** | `mcp-trust-proxy` 作为 MCP 中间件拦截 tool call |
| **服务式 (Server)** | `agent-os` 自带 FastAPI server (`agent_os.server`) |
| **Rust 嵌入** | `cargo add agentmesh` 直接编译进 Rust 二进制 |

---

## 3. OWASP Agentic Top 10 覆盖矩阵

| # | OWASP 风险 | 实现组件 | 关键源码路径 | 深度评估 |
|---|-----------|---------|-------------|---------|
| ASI-01 | Agent 目标劫持 | Agent OS — PolicyEngine | `agent-os/src/agent_os/policies/evaluator.py` | **生产级** — 声明式规则 + OPA/Rego/Cedar 后端 + 冲突解决策略 |
| ASI-02 | 工具滥用 | Agent OS — 能力模型 | `agent-os/src/agent_os/policies/schema.py` (PolicyCondition + PolicyOperator) | **生产级** — 9 种操作符 (eq/ne/gt/lt/gte/lte/in/matches/contains) |
| ASI-03 | 身份与权限滥用 | AgentMesh — DID + 信任评分 | `agentmesh/src/identity.rs`, `agentmesh/src/trust.rs` | **生产级** — Ed25519 密钥对 + 5 级信任 (0-1000) + 信任衰减 |
| ASI-04 | 供应链漏洞 | AgentMesh — AI-BOM | `agent-mesh/docs/RFC_AGENT_SBOM.md` | **基础** — 规范级，非自动扫描 |
| ASI-05 | 非预期代码执行 | Agent Runtime — 执行环 | `agent-runtime/src/` | **生产级** — Ring 0-3 + 资源限制 + Kill Switch |
| ASI-06 | 记忆/上下文投毒 | Agent OS — VFS + CMVK | `agent-os/src/agent_os/mcp_security.py` | **生产级** — 提示注入检测 + 凭证脱敏 |
| ASI-07 | 不安全 agent 间通信 | AgentMesh — IATP | `agent-mesh/sdks/typescript/src/` | **基础级** — 签名+验证协议，非端到端加密 |
| ASI-08 | 级联故障 | Agent SRE — 熔断器 | `agent-sre/src/` | **生产级** — SLO + Error Budget + 混沌测试 + 渐进发布 |
| ASI-09 | 人机信任滥用 | Agent OS — 审批工作流 | `agent-os/extensions/mcp-server/src/services/approval-workflow.ts` | **基础级** — 审批队列 + 仲裁逻辑 + 过期追踪 |
| ASI-10 | 失控 Agent | Agent Runtime — Kill Switch | `agent-runtime/src/` + `agentmesh/src/trust.rs` | **生产级** — Ring 隔离 + 信任衰减 + 异常检测 |

**整体评估**: ASI-01/02/03/05/08/10 达到生产级，ASI-04/07/09 为基础级。对 EAASP 最有价值的是 ASI-01 (PolicyEngine) 和 ASI-02 (能力模型)——这两项直接映射到 HookBridge 的 PreToolUse/PostToolUse 判定逻辑。

---

## 4. 策略模型分析

### 4.1 策略定义格式

AGT 支持 **三种策略语言**：

| 格式 | 后端 | 源码路径 | 说明 |
|------|------|---------|------|
| **YAML/JSON** (原生) | `PolicyEvaluator` | `policies/evaluator.py` + `policies/schema.py` | Pydantic 模型，声明式规则 |
| **OPA/Rego** | `OPABackend` | `policies/backends.py` | 调用 `opa eval` 子进程 |
| **Cedar** | `CedarBackend` | `policies/backends.py` | 调用 `cedar` CLI 子进程 |

### 4.2 原生 YAML 策略模型

```python
# Python 侧 (Pydantic)
class PolicyDocument:
    version: str          # "1.0"
    name: str
    rules: list[PolicyRule]
    defaults: PolicyDefaults  # default action + limits

class PolicyRule:
    name: str
    condition: PolicyCondition  # field + operator + value
    action: PolicyAction        # allow / deny / audit / block
    priority: int
    message: str

class PolicyCondition:
    field: str             # "tool_name", "token_count", etc.
    operator: PolicyOperator  # eq/ne/gt/lt/gte/lte/in/matches/contains
    value: Any
```

```yaml
# Rust 侧 (serde YAML)
version: "1.0"
agent: test-agent
policies:
  - name: capability-gate
    type: capability              # capability / approval / rate_limit
    allowed_actions: ["data.read"]
    denied_actions: ["shell:*"]   # glob 匹配
    conditions:                   # 上下文条件匹配
      environment: "production"
    priority: 10
    scope: agent                  # global / tenant / agent
```

### 4.3 策略评估 API

| 语言 | 评估入口 | 返回值 |
|------|---------|--------|
| **Python** | `PolicyEvaluator.evaluate(context: dict) -> PolicyDecision` | `{allowed, matched_rule, action, reason, audit_entry}` |
| **Rust** | `PolicyEngine.evaluate(action, context) -> PolicyDecision` | `Allow / Deny(reason) / RequiresApproval(msg) / RateLimited{retry}` |
| **Rust MCP** | `McpGateway.process_request(req) -> McpGatewayDecision` | `{status, allowed, sanitized_payload, findings, retry_after}` |

### 4.4 冲突解决策略 (Rust `PolicyEngine`)

```rust
enum ConflictResolutionStrategy {
    DenyOverrides,        // Deny 总是胜出
    AllowOverrides,       // Allow 总是胜出
    PriorityFirstMatch,   // 最高优先级胜出 (默认)
    MostSpecificWins,     // scope specificity 排序: Agent > Tenant > Global
}
```

### 4.5 策略 CRUD 与版本管理

- **加载**: `load_from_yaml(path)` / `load_from_file(path)` / `load_policies(directory)`
- **版本**: PolicyDocument 有 `version` 字段，但 **AGT 本身不提供版本管理存储层**——策略是无状态的文件，版本由 Git 或外部系统管理
- **动态更新**: `PolicyEngine` 内部用 `RwLock<Option<PolicyProfile>>`，可运行时替换

**关键差距**: AGT 的策略是**无状态的文件/内存模型**，没有内置的版本持久化 (SQLite/DB)。而 EAASP L3 的 `managed_settings_versions` 表正是提供这种版本化存储。

---

## 5. MCP/Rust 集成面

### 5.1 `agentmesh` Rust Crate (核心 SDK)

**存在且功能完整**。路径: `packages/agent-mesh/sdks/rust/agentmesh/`

API Surface:

| 模块 | 核心类型 | 说明 |
|------|---------|------|
| `policy` | `PolicyEngine`, `PolicyRule`, `PolicyProfile`, `PolicyError` | YAML 策略加载+评估，4 种决策 (Allow/Deny/RequiresApproval/RateLimited)，4 种冲突解决策略 |
| `identity` | `AgentIdentity`, `PublicIdentity` | Ed25519 密钥对生成，DID (`did:agentmesh:{id}`)，签名/验证 |
| `trust` | `TrustManager`, `TrustConfig`, `TrustScore`, `TrustTier` | 0-1000 信任评分，5 级 (Untrusted→Verified)，衰减+奖惩 |
| `audit` | `AuditLogger`, `AuditEntry`, `AuditFilter` | 哈希链审计日志，`verify()` 完整性校验 |
| `types` | `GovernanceResult`, `PolicyDecision`, `CandidateDecision`, etc. | 共享类型 |
| `lib` | `AgentMeshClient`, `ClientOptions` | 统一入口：identity + policy + trust + audit 管道 |

**依赖**: `ed25519-dalek`, `serde`, `serde_yaml`, `serde_json`, `sha2`, `hmac`, `regex`, `rand`, `thiserror`, `base64`

### 5.2 `agentmesh-mcp` Rust Crate (MCP 安全原语)

**存在且功能丰富**。路径: `packages/agent-mesh/sdks/rust/agentmesh-mcp/`

API Surface:

| 模块 | 核心类型 | 说明 |
|------|---------|------|
| `gateway` | `McpGateway`, `McpGatewayConfig`, `McpGatewayDecision` | **MCP 治理网关**: deny-list → allow-list → 净化 → 限流 → 人工审批 |
| `security` | `McpSecurityScanner`, `McpThreat`, `McpToolDefinition` | **MCP 工具安全扫描**: 投毒检测、rug-pull (指纹变更)、typosquatting (Levenshtein)、schema 滥用、隐藏指令 |
| `signing` | `McpMessageSigner`, `McpSignedMessage` | HMAC-SHA256 消息签名 + nonce 防重放 |
| `session` | `McpSessionAuthenticator`, `McpSession` | 会话认证 + 令牌管理 |
| `rate_limit` | `McpSlidingRateLimiter` | 滑动窗口限流 |
| `redactor` | `CredentialRedactor` | 凭证脱敏 (Bearer token, API key, etc.) |
| `audit` | `McpAuditEntry`, `McpAuditSink` (trait) | MCP 审计日志 sink |
| `metrics` | `McpMetricsCollector` | 分类计数器 (scan/decision/threat/rate-limit) |
| `response` | `McpResponseScanner` | MCP 响应安全扫描 |

**依赖**: `serde`, `serde_json`, `sha2`, `hmac`, `regex`, `rand`, `base64`, `thiserror` (无 `ed25519-dalek`，无 `serde_yaml`)

### 5.3 与 EAASP `grid-hook-bridge` 的对接可行性

**高度可行**。分析如下：

| EAASP 组件 | AGT 对应 | 对接方式 |
|-----------|---------|---------|
| `HookBridge::pre_tool_use()` | `PolicyEngine.evaluate(action, context)` | 直接调用，将 hook 上下文映射为 AGT context |
| `HookBridge::post_tool_use()` | `AuditLogger.log()` + `TrustManager.record_success/failure()` | 执行后审计+信任更新 |
| `ManagedHook.mode = "enforce"/"shadow"` | `PolicyAction::DENY` vs `PolicyAction::AUDIT` | 语义一致，可直接映射 |
| `ManagedHook.phase` | 无直接对应 | EAASP 概念，AGT 按 action name 区分 |
| MCP tool 安全检查 | `McpSecurityScanner.scan_tool()` | 新增 hook 阶段或作为 PreToolUse 前置扫描 |

---

## 6. 与 EAASP L3 的差距与对接方案

### 6.1 EAASP L3 当前状态

EAASP L3 (`tools/eaasp-l3-governance/`) 是一个 **薄 FastAPI 服务** (port 18083)，实现：

| 功能 | 实现位置 | 说明 |
|------|---------|------|
| **策略部署** (Contract 1) | `policy_engine.py` — `PolicyEngine.deploy()` | `managed_settings_versions` 表，版本化存储 |
| **模式切换** | `policy_engine.py` — `PolicyEngine.switch_mode()` | `managed_hooks_mode_overrides` 表，upsert enforce/shadow |
| **版本列表** | `policy_engine.py` — `list_versions()` | 降序分页查询 |
| **遥测采集** (Contract 4) | `audit.py` — `AuditStore` | `telemetry_events` 表 |
| **会话验证** (Contract 5 stub) | `api.py` — `/v1/sessions/{id}/validate` | 三方握手桩 |

**核心数据模型**: `ManagedSettings` → `ManagedHook` (hook_id, phase, mode, agent_id, skill_id, handler)

### 6.2 AGT 能替代/增强的部分

| EAASP L3 功能 | AGT 能否替代 | 说明 |
|-------------|------------|------|
| 策略评估逻辑 | **可替代并大幅增强** | AGT 提供 9 种操作符 + glob 匹配 + OPA/Rego/Cedar 后端 + 冲突解决 — EAASP L3 当前无策略评估逻辑 |
| MCP 工具安全扫描 | **新增能力** | `McpSecurityScanner`: 投毒/rug-pull/typosquatting — EAASP 完全缺失 |
| 身份/信任 | **新增能力** | Ed25519 DID + 0-1000 信任评分 — EAASP 当前无 agent 身份体系 |
| 审计日志 | **可增强** | 哈希链审计 + 完整性校验 — EAASP 当前仅 append-only SQLite |
| 策略版本管理 | **不能替代** | AGT 无版本持久化层，EAASP L3 的 `managed_settings_versions` 仍然必要 |
| 模式切换 (enforce/shadow) | **不能替代** | AGT 无 hook mode 概念，需桥接 |
| 遥测采集 | **不能替代** | AGT 有 metrics 但非遥测存储服务 |
| 会话验证 | **不能替代** | AGT 无 session 三方握手概念 |

### 6.3 需要桥接的部分

1. **策略格式映射**: `ManagedHook` (hook_id + phase + mode) → AGT `PolicyRule` (name + type + action)
2. **HookMode 语义**: `enforce` → 评估结果直接生效; `shadow` → 评估但仅记录 (AGT 的 `PolicyAction::AUDIT`)
3. **Phase 映射**: `PreToolUse` → 执行前拦截; `PostToolUse` → 执行后审计; `Stop` → 会话结束检查
4. **版本持久化**: AGT 策略需要存入 EAASP L3 的 `managed_settings_versions` 才能版本化管理

### 6.4 对接方案评估

#### 方案 A: AGT 作为 L3 后端 (EAASP L3 → AGT API)

```
L1 Runtime ──► EAASP L3 ──► AGT PolicyEvaluator ──► Allow/Deny
                  │                    ↑
                  │         AGT PolicyDocument (从 managed_settings 转换)
                  └── managed_settings_versions (SQLite 版本管理不变)
```

**实现方式**: EAASP L3 的 `policy_engine.py` 新增 `evaluate()` 方法，内部实例化 AGT 的 `PolicyEvaluator`，将 `ManagedHook` 列表转换为 AGT `PolicyDocument`。

**优势**:
- EAASP L3 保持现有 REST API 不变，L4/L1 无需改动
- 版本管理、模式切换、遥测采集全部复用现有实现
- AGT 策略评估能力立即可用 (9 种操作符 + OPA/Rego/Cedar)
- 渐进式对接，风险最低

**劣势**:
- Python-only (需 `pip install agent-governance-toolkit`)
- 多一层间接调用 (但 AGT 评估 <0.1ms，可忽略)

**工作量**: **约 2-3 人天**
- 新增 `agt_bridge.py` (~150 行): ManagedHook → PolicyDocument 转换 + 评估适配
- 修改 `api.py`: 在 `/validate` 和新增 `/evaluate` 端点中调用 AGT
- 测试: ~20 个新测试

#### 方案 B: AGT 嵌入 grid-hook-bridge (Rust crate 直接调用)

```
L1 Runtime (Rust) ──► grid-hook-bridge ──► agentmesh::PolicyEngine ──► Allow/Deny
                            │                        ↑
                            │              PolicyProfile (YAML)
                            └── gRPC ──► EAASP L3 (版本管理)
```

**实现方式**: `grid-hook-bridge` crate 新增 `agentmesh` 依赖，在 `InProcessHookBridge` 中直接调用 `PolicyEngine.evaluate()`。策略文件从 L3 REST API 拉取并缓存。

**优势**:
- 零网络延迟 (in-process Rust 调用，~0.01ms)
- 类型安全，编译时保证
- `agentmesh-mcp` 的 MCP 安全扫描可直接嵌入

**劣势**:
- `agentmesh` crate 依赖 `ed25519-dalek` + `serde_yaml` — 增加编译时间和二进制体积
- 策略格式需要从 EAASP `managed_settings.json` 转换为 AGT YAML
- 策略热更新需要额外机制 (watch + reload)

**工作量**: **约 4-5 人天**
- `grid-hook-bridge/Cargo.toml` 新增 `agentmesh` 可选依赖
- 新增 `agt_evaluator.rs` (~200 行): PolicyProfile 构建 + 评估适配
- 策略同步模块 (~100 行): 从 L3 REST 拉取 + 本地缓存
- 测试: ~15 个 Rust 测试

#### 方案 C: AGT MCP Server 作为 L1 Runtime 额外 MCP Tool

```
L1 Runtime ──► MCP connectMCP ──► AGT MCP Server ──► Policy/Security/Audit
                                        ↑
                                   mcp-trust-proxy
```

**实现方式**: 部署 AGT 的 `agent-os` server 或 `mcp-trust-proxy` 作为独立 MCP server，L1 Runtime 通过 `connectMCP` 自动连接。

**优势**:
- 完全解耦，AGT 独立升级
- 最完整的 AGT 能力 (Python full stack)
- `mcp-trust-proxy` 现成可用

**劣势**:
- 额外进程 + 网络开销
- L1 Runtime 的 tool call 全部经过 AGT 代理 — 架构复杂度高
- EAASP 的 hook phase 语义无法直接映射到 MCP tool call

**工作量**: **约 1-2 人天** (部署) + **约 3 人天** (集成+测试)

#### 方案推荐

**Phase 0.5/1.0: 采用方案 A** (AGT 作为 L3 后端)
- 风险最低，工作量最小，保持现有架构稳定
- 立即获得 OWASP 全 10 项治理能力

**Phase 2.0+: 渐进演进到方案 A+B 混合**
- Rust 侧嵌入 `agentmesh-mcp` 的 MCP 安全扫描能力
- 高频策略评估走 Rust in-process 路径 (方案 B)
- 低频策略管理仍走 L3 REST (方案 A)

---

## 7. 竞品对比 (vs OPA / cedar-agent)

| 维度 | AGT | OPA (Open Policy Agent) | cedar-agent (Permit.io) |
|------|-----|------------------------|------------------------|
| **定位** | Agent 动作治理全栈 | 通用策略引擎 | 基于 Cedar 的授权即服务 |
| **策略语言** | YAML + OPA/Rego + Cedar (全支持) | Rego (专属) | Cedar (专属) |
| **Agent 特化** | 是 — DID 身份、信任评分、MCP 安全扫描、Kill Switch、SRE | 否 — 通用，需自建 agent 治理层 | 否 — 通用 RBAC/ABAC/ReBAC |
| **OWASP Agentic 覆盖** | 10/10 | ~2/10 (仅策略评估) | ~1/10 (仅授权) |
| **MCP 支持** | 原生 (McpGateway + McpSecurityScanner + mcp-trust-proxy) | 无 | 无 |
| **Rust SDK** | 有 (agentmesh + agentmesh-mcp) | 有 (opa-wasm 嵌入式) | 有 (cedar-policy crate) |
| **延迟** | <0.1ms (in-process) | ~1-5ms (Rego 评估) / <0.1ms (Wasm 编译后) | ~0.5ms (Cedar 评估) |
| **License** | MIT | Apache 2.0 | Apache 2.0 |
| **与 EAASP 对接成本** | **低** — 已有 agent 语义，策略模型可直接映射 | **中** — 需自建 agent 上下文到 Rego input 的映射层 | **中** — 需自建 principal/action/resource 到 EAASP 概念的映射 |
| **社区成熟度** | Public Preview (2026) — 快速迭代中 | GA 多年，CNCF 毕业项目 — 极成熟 | GA，Permit.io 商业支持 — 成熟 |

### AGT 优于 OPA/Cedar 的场景

- **需要 MCP 安全扫描** (工具投毒、rug-pull 检测) — OPA/Cedar 完全不具备
- **需要 agent 身份 + 信任评分** — OPA/Cedar 需完全自建
- **需要一站式 OWASP 合规** — AGT 开箱 10/10
- **已有多 agent 框架** (LangChain/CrewAI/OpenAI Agents) — AGT 有 20+ 现成集成

### AGT 劣于 OPA/Cedar 的场景

- **需要极成熟的策略语言生态** — Rego/Cedar 有更大的社区和更多教程
- **需要 Kubernetes/微服务原生策略** — OPA Gatekeeper 是 K8s 标配
- **需要细粒度 RBAC/ABAC/ReBAC** — Cedar 的关系型授权模型更成熟
- **需要 Wasm 沙箱评估** — OPA 的 Wasm 编译链路更成熟

---

## 8. 结论与建议

### 8.1 核心结论

1. **AGT 是 EAASP L3 HookBridge 的理想可替换后端**。其策略评估引擎 (`PolicyEngine`) 直接映射到 EAASP 的 PreToolUse/PostToolUse 判定逻辑，且提供 AGT 独有的 MCP 安全扫描、agent 身份、信任评分等增值能力。

2. **`agentmesh` Rust crate 确实存在且功能完整**。包含策略评估 + 身份 + 信任 + 审计 + MCP 安全全套原语，可直接嵌入 `grid-hook-bridge`。

3. **AGT 不能替代 EAASP L3 的全部功能**。版本管理 (`managed_settings_versions`)、模式切换 (`managed_hooks_mode_overrides`)、遥测存储 (`telemetry_events`)、会话验证仍需 EAASP L3 自有实现。

4. **策略模型可映射但非 1:1**。EAASP 的 `ManagedHook` (hook_id + phase + mode) 与 AGT 的 `PolicyRule` (name + type + action) 概念相似但结构不同，需要适配层。

### 8.2 行动建议

| 优先级 | 建议 | Phase | 工作量 |
|-------|------|-------|--------|
| **P0** | 采用方案 A: 在 L3 `policy_engine.py` 中引入 AGT `PolicyEvaluator` 作为策略评估后端 | 0.5/1.0 | 2-3 天 |
| **P1** | 新增 MCP 安全扫描: 在 L4 session handshake 阶段调用 AGT `McpSecurityScanner` 检查连接的 MCP tools | 1.0 | 2 天 |
| **P2** | 在 Rust 侧 (`grid-hook-bridge`) 可选嵌入 `agentmesh-mcp` 进行高频 MCP 工具安全判定 | 2.0 | 3 天 |
| **P3** | 引入 OPA/Rego 策略后端: 通过 AGT 的 `OPABackend` 支持 Rego 策略文件 | 2.0+ | 1 天 |
| **Deferred** | 评估 AGT 的 `agent-sre` 模块用于 EAASP 平台级 SLO/熔断器 | 3.0 | TBD |

### 8.3 风险提示

- AGT 当前是 **Public Preview**，API 可能在 GA 前有破坏性变更
- AGT 的 Python 完整栈依赖较多 (`[full]` 安装约 30+ 包)，建议仅安装 `agent-os-kernel`
- AGT 的 Rust crate 尚未在 crates.io 发布正式版 (v3.0.2)，需确认版本稳定性
- AGT 的策略评估是**确定性的、非概率性的** — 这恰好是 EAASP 想要的，但不能用于 LLM 输出内容过滤

---

*备忘录结束。源码证据均来自 `3th-party/eaasp-governances/agent-governance-toolkit/` 目录的直接阅读。*
