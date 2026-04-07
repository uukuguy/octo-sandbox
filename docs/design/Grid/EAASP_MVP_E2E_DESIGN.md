# EAASP MVP E2E 设计 — 业务智能体全流程验证

> **Phase**: BH-MVP
> **创建日期**: 2026-04-07
> **基线**: Phase BG 完成 @ aa3179a（SDK 107 tests, 2476 Rust workspace tests）
> **权威参考**: `EAASP_-_企业自主智能体支撑平台设计规范_v1.7_.pdf`

---

## 一、目标

构建**纵向切片 MVP**，用 HR 入职智能体走通 L4→L3→L2→L1 完整生命周期：

```
SDK 创作 Skill → 提交 L2 Registry → L4 接收意图 → L3 编译策略 + 选运行时
  → L1 执行 + Hook 强制（PII 拦截） → 遥测回传 → 会话终止
```

**不是**完整 L3 治理层，但 Mock 的**架构、协议、数据模型**按生产级设计，直接作为未来真实实现的蓝图和验收契约。

---

## 二、设计原则

### 2.1 Mock = 生产蓝图

- REST API 路径、请求/响应 schema、错误码就是未来 production 的契约
- 数据模型用 Pydantic V2 定义，带完整 JSON Schema 导出
- Mock 内部可用内存存储、简单逻辑，但**接口签名**不能 MVP-only

### 2.2 严格对齐规范 v1.7 的 5 个 API 契约（§8）

| 契约 | 规范章节 | MVP 范围 | 未来扩展 |
|------|---------|---------|--------|
| **契约 1: 策略部署** | §8.1 | YAML → managed_hooks_json 编译 + 部署 | 版本回滚、灰度 |
| **契约 2: 意图网关** | §8.2 | 关键词映射 → skill_id | NLU、多轮意图 |
| **契约 3: 技能生命周期** | §8.3 | L2 Skill 拉取 + 状态查询 | promote pipeline |
| **契约 4: 遥测采集** | §8.4 | 内存接收 + 结构化日志 | 持久化、告警 |
| **契约 5: 会话控制** | §8.5 | 三方握手 + 消息代理 + 终止 | 暂停/恢复、迁移 |

### 2.3 L4 四个内部平面（§3）

| 平面 | MVP 实现 | 未来扩展 |
|------|---------|--------|
| **体验平面** (Experience) | CLI + REST API | Web Portal, Slack Bot |
| **集成平面** (Integration) | API Gateway 路由 | Event Bus, CDC |
| **控制平面** (Control) | Session Manager + 运行时路由 | Observability Hub, Cost Gov |
| **持久化平面** (Persistence) | SQLite session store | PostgreSQL, Cost Ledger |

### 2.4 Hook 全生命周期（§10）

```
L4 Origin（管理员创建策略）
  → L3 Compile（策略 YAML → managed_hooks_json）
    → L3 Deploy（注入 SessionPayload.managed_hooks_json）
      → L2 Scoped（Skill frontmatter 中的作用域 hooks 合并）
        → L1 Execute（InProcessHookBridge / HookExecutor 评估）
          → L1 Report（遥测事件回传 L3 → L4）
```

### 2.5 E2E 测试双模式

- **快速模式** (`--mock-llm`): 通过 gRPC OnToolCall 直接验证 Hook 逻辑，CI 友好
- **真实模式** (`--live-llm`): 连接 LLM API，验证 Agent→工具→Hook 全链路

---

## 三、策略 DSL 规范

### 3.1 格式定义

采用 Kubernetes 风格声明式语言，面向企业管理员：

```yaml
apiVersion: eaasp.io/v1
kind: PolicyBundle
metadata:
  name: hr-department-policies
  scope: bu                              # enterprise | bu | department | team
  org_unit: hr-dept
  version: "1.0.0"

rules:
  - id: pii-guard
    name: 个人敏感信息拦截
    description: 禁止在文件写入操作中包含身份证、社保号等 PII
    event: PreToolUse                    # 9 种生命周期事件
    handler_type: command                # command < http < prompt < agent
    match:
      tool_name: "^file_write$"
      input_pattern: "\\d{3}-\\d{2}-\\d{4}|\\d{17}[\\dXx]|\\w+@\\w+"
    action: deny
    reason: "检测到 PII，已阻止写入"
    severity: critical

  - id: audit-all-writes
    name: 文件写入审计
    event: PostToolUse
    handler_type: command
    match:
      tool_name: "^file_write$"
    action: allow
    audit: true

  - id: checklist-enforcement
    name: 入职清单强制完成
    event: Stop
    handler_type: prompt
    action: deny
    reason: "请确认入职清单全部完成"
    config:
      prompt: "验证入职清单：IT账号 ✓、门禁 ✓、培训 ✓"
```

### 3.2 编译输出

编译器将策略 DSL 转换为 `managed_hooks_json`（两个 L1 Runtime 共用的执行格式）：

```json
{
  "rules": [
    {
      "id": "r-pii-check",
      "name": "PII detection on file_write",
      "hook_type": "pre_tool_call",
      "action": "deny",
      "reason": "PII detected in file content",
      "tool_pattern": "^file_write$",
      "input_pattern": "\\d{3}-\\d{2}-\\d{4}|\\d{17}[\\dXx]",
      "enabled": true
    }
  ]
}
```

### 3.3 层级合并规则

按规范 §4.5 四作用域层级合并，deny-always-wins（§10.8）：

```
managed (enterprise) — 最高优先级，不可覆盖
  > skill-scoped (L2 frontmatter hooks)
    > project
      > user — 最低优先级
```

合并策略：
- 所有层级的 rules 合并到同一列表
- 评估时按 deny-always-wins：任一 rule 返回 deny，最终结果为 deny
- disabled 的 rule 跳过

---

## 四、架构与数据流

### 4.1 服务端口清单

| Service | Port | Language | Protocol | 角色 |
|---------|------|----------|----------|------|
| L1 grid-runtime | 50051 | Rust | gRPC | 智能体执行 |
| L1 claude-code-runtime | 50052 | Python | gRPC | 智能体执行 |
| L2 Skill Registry | 8081 | Rust | HTTP | 技能资产 |
| L2 MCP Orchestrator | 8082 | Rust | HTTP | MCP 管理 |
| **L3 Governance** | **8083** | **Python** | **HTTP** | **智能体治理** |
| **L4 Session Manager** | **8084** | **Python** | **HTTP** | **人机协作** |

### 4.2 E2E 数据流

```
用户: "新员工张三入职"
  ↓ POST /v1/conversations {user_id, org_unit, input}
[L4 :8084 — 体验平面]
  ├─ 持久化: INSERT sessions + execution_log("intent_dispatch")
  └─ 集成平面 → POST /v1/sessions → [L3 :8083]
      ├─ 契约 3: GET /api/v1/skills/hr-onboarding/content → [L2 :8081]
      │   └─ 返回 SKILL.md (frontmatter + prose)
      ├─ 契约 1: 编译策略
      │   enterprise.yaml (managed scope)
      │   + bu_hr.yaml (bu scope)
      │   + skill frontmatter hooks (skill scope)
      │   → managed_hooks_json (deny-always-wins 合并)
      ├─ RuntimePool.select(preferred="grid") → grid:50051
      ├─ gRPC L1.Initialize(SessionPayload{
      │     user_id, org_unit, managed_hooks_json,
      │     skill_ids: ["hr-onboarding"],
      │     skill_registry_url: "http://l2:8081"
      │   })
      ├─ gRPC L1.LoadSkill(SkillContent{frontmatter, prose})
      └─ 返回 {session_id, runtime_id, governance: {hooks_count, scope_chain}}
  ↓
[L4 更新 session status → ACTIVE]
  ↓ POST /v1/conversations/{id}/message {content}
[L4] → [L3 契约 5] → gRPC L1.Send(UserMessage)
  ├─ Agent 执行: 收集员工信息 → 调用 file_write
  ├─ L1 PreToolUse hook 触发:
  │   ├─ rule "pii-guard" matches file_write + SSN pattern → DENY
  │   └─ (无 PII 时) → ALLOW → 执行工具
  ├─ L1 PostToolUse hook 触发:
  │   └─ rule "audit-all-writes" → ALLOW + 审计日志
  ├─ 流式 ResponseChunk → L3 → SSE → L4 → 用户
  └─ 契约 4: L1 遥测 → POST L3 /v1/telemetry → L4 持久化
  ↓ DELETE /v1/conversations/{id}
[L4] → [L3] → gRPC L1.Terminate → 最终遥测
[L4 更新 session status → TERMINATED, 持久化 execution_log]
```

### 4.3 三方握手时序图（§8.6）

```
L4(用户)          L3(治理)          L2(资产)          L1(运行时)
   │                 │                 │                 │
   │──POST /sessions→│                 │                 │
   │                 │──GET /skills/id→│                 │
   │                 │←─SkillContent───│                 │
   │                 │                 │                 │
   │                 │ compile policies                  │
   │                 │ (enterprise + bu + skill-scoped)  │
   │                 │ → managed_hooks_json              │
   │                 │                 │                 │
   │                 │ select runtime(preferred, pool)   │
   │                 │                 │                 │
   │                 │────gRPC Initialize(SessionPayload)→│
   │                 │←──────session_id──────────────────│
   │                 │                 │                 │
   │                 │────gRPC LoadSkill(SkillContent)──→│
   │                 │←──────success────────────────────│
   │                 │                 │                 │
   │←─{session_id}──│                 │                 │
   │                 │                 │                 │
   │──POST /message─→│                 │                 │
   │                 │────gRPC Send(UserMessage)────────→│
   │                 │←───stream ResponseChunk──────────│
   │←──SSE chunks───│                 │                 │
```

---

## 五、L3 治理服务 API 规范

### 5.1 契约 1: 策略部署（§8.1）

```
PUT  /v1/policies/deploy
  Request:  multipart/form-data (YAML file) or application/yaml
  Response: { policy_id, rules_count, compiled_hooks_digest }

GET  /v1/policies
  Response: [{ id, name, scope, org_unit, version, rules_count, deployed_at }]

GET  /v1/policies/{id}
  Response: { ...metadata, rules: [...], compiled_hooks_json }
```

### 5.2 契约 2: 意图网关（§8.2）

```
POST /v1/intents
  Request:  { text, user_id, org_unit }
  Response: { intent_id, skill_id, confidence, skill_name }
```

### 5.3 契约 3: 技能生命周期（§8.3）

```
GET  /v1/skills/{id}/governance
  Response: { skill_id, status, applicable_policies: [...], hooks_summary }
```

### 5.4 契约 4: 遥测采集（§8.4）

```
POST /v1/telemetry
  Request:  { session_id, events: [{ event_type, timestamp, payload, resource_usage }] }
  Response: { accepted: N, rejected: N }

GET  /v1/telemetry/sessions/{id}
  Response: { session_id, events: [...], resource_summary }
```

### 5.5 契约 5: 会话控制（§8.5）

```
POST /v1/sessions
  Request:  { user_id, user_role, org_unit, skill_id, runtime_preference? }
  Response: { session_id, runtime_id, runtime_endpoint, governance_summary }

GET  /v1/sessions/{id}
  Response: { session_id, status, runtime_id, skill_id, hooks_count, created_at }

POST /v1/sessions/{id}/message
  Request:  { content, message_type? }
  Response: SSE stream of { chunk_type, content, tool_name?, tool_id? }

DELETE /v1/sessions/{id}
  Response: { session_id, status: "terminated", final_telemetry }
```

---

## 六、L4 会话管理器 API 规范

### 6.1 体验平面（面向用户）

```
POST /v1/conversations
  Request:  { user_id, org_unit, input?, skill_id? }
  Response: { conversation_id, session_id, skill_name, runtime }

POST /v1/conversations/{id}/message
  Request:  { content }
  Response: SSE stream

GET  /v1/conversations/{id}
  Response: { id, status, skill, messages_count, created_at }

DELETE /v1/conversations/{id}
  Response: { id, status: "terminated" }
```

### 6.2 控制平面（面向管理员）

```
GET  /v1/sessions
  Response: [{ id, user, skill, runtime, status, duration }]

GET  /v1/sessions/{id}/telemetry
  Response: { tools_called, hooks_fired, tokens_used, duration_ms }
```

### 6.3 持久化 Schema（SQLite → PostgreSQL）

```sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    org_unit TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    runtime_id TEXT,
    runtime_endpoint TEXT,
    status TEXT NOT NULL DEFAULT 'creating',
    managed_hooks_digest TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    terminated_at TEXT
);

CREATE TABLE execution_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    event_type TEXT NOT NULL,
    payload_json TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE telemetry_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    runtime_id TEXT,
    event_type TEXT NOT NULL,
    resource_usage_json TEXT,
    created_at TEXT NOT NULL
);
```

---

## 七、设计决策

| ID | 决策 | 理由 |
|----|------|------|
| KD-BH1 | 策略 DSL 用 Kubernetes 风格 YAML | 企业运维友好，结构化，支持 schema 校验 |
| KD-BH2 | 四作用域层级合并 (managed > skill > project > user) | 规范 §4.5 |
| KD-BH3 | 编译器幂等输出 | 可测试、可缓存、可审计 |
| KD-BH4 | L4 用 conversations、L3 用 sessions | 语义分离，面向不同受众 |
| KD-BH5 | L4 持久化用 SQLite，schema 对标 PostgreSQL | MVP 轻量，生产无缝迁移 |
| KD-BH6 | L4 不直接调 L1，全部经 L3 | 规范: L4 requests, L3 authorizes, L1 executes |
| KD-BH7 | L4→用户 SSE，L3→L1 gRPC stream | 协议适配 |
| KD-BH8 | L3/L4 用 Python FastAPI | 与 SDK 生态统一，REST 契约即生产蓝图 |
| KD-BH9 | L3/L4 两个独立服务 | 规范明确分层 |
| KD-BH10 | E2E 测试双模式 (mock/live) | CI 不依赖 LLM API |

---

## 八、运行指南

### 8.1 前置条件

```bash
# Rust 工具链
rustup show  # 确认 Rust 1.75+

# Python 环境
python3 --version  # 确认 3.12+
uv --version       # 确认 uv 包管理器

# 可选: LLM API Key（真实模式）
export ANTHROPIC_API_KEY=sk-ant-xxxxx
```

### 8.2 启动全部服务

```bash
# 一键启动
make e2e-setup

# 或手动分步：
# 1. L2 Skill Registry
cargo run -p eaasp-skill-registry -- --port 8081 &

# 2. L1 Grid Runtime
GRID_RUNTIME_ADDR=0.0.0.0:50051 cargo run -p grid-runtime &

# 3. L3 Governance
cd tools/eaasp-governance && uv run python -m eaasp_governance --port 8083 &

# 4. L4 Session Manager
cd tools/eaasp-session-manager && uv run python -m eaasp_session --port 8084 &
```

### 8.3 注册资产

```bash
# 提交 HR 入职 Skill 到 L2
eaasp submit ./sdk/examples/hr-onboarding/ --registry http://localhost:8081

# 部署策略到 L3
curl -X PUT http://localhost:8083/v1/policies/deploy \
  -H "Content-Type: application/yaml" \
  --data-binary @sdk/examples/hr-onboarding/policies/enterprise.yaml

curl -X PUT http://localhost:8083/v1/policies/deploy \
  -H "Content-Type: application/yaml" \
  --data-binary @sdk/examples/hr-onboarding/policies/bu_hr.yaml
```

### 8.4 运行 E2E

```bash
# 快速 Mock 模式（不需要 LLM API Key）
eaasp run ./sdk/examples/hr-onboarding/ \
  --platform http://localhost:8084 \
  --mock-llm

# 真实 LLM 模式（需要 API Key）
eaasp run ./sdk/examples/hr-onboarding/ \
  --platform http://localhost:8084 \
  --live-llm \
  --input "新员工张三入职，工号 E2024001"
```

### 8.5 运行测试

```bash
# 单元测试
cd tools/eaasp-governance && pytest tests/ -xvs
cd tools/eaasp-session-manager && pytest tests/ -xvs

# E2E Mock 测试
pytest tests/e2e/ -m "e2e and mock_llm" -xvs

# E2E Live 测试（需要 API Key + 运行中的服务）
pytest tests/e2e/ -m "e2e and live_llm" -xvs

# 一键全量
make e2e-full
```

### 8.6 清理

```bash
make e2e-teardown
```

---

## 九、Deferred（MVP 不做）

| 项目 | 规范章节 | 前置条件 |
|------|---------|---------|
| RBAC 访问控制 | §4.5 | 用户身份管理系统 |
| 审批闸门 | §4.2 | L4 审批收件箱 UI |
| 审计持久化 | §4.3 | L4 持久化平面完善 |
| MCP 注册中心 | §4.4 | L2 MCP Orchestrator 扩展 |
| NLU 意图解析 | §8.2 | NLU 模型 |
| L4 管理控制台 UI | §3.1 | Web 前端框架 |
| L4 事件总线 | §3.2 | 消息队列 |
| 多租户 | §3.5 | PostgreSQL + org tree |
| 策略版本回滚 | §8.1 | 策略版本存储 |
| 成本治理 | §3.3 | Cost Ledger |
| T2/T3 HookBridge 验证 | §6.3 | 非 T1 运行时 |
