# Phase BH-MVP — E2E 业务智能体全流程验证

**Phase**: BH-MVP
**日期**: 2026-04-07
**基线**: Phase BG 完成 @ aa3179a
**设计文档**: `docs/design/Grid/EAASP_MVP_E2E_DESIGN.md`

---

## Waves 总览

| Wave | 内容 | Tests | 状态 |
|------|------|-------|------|
| W1 | 策略 DSL + 编译器 + HR 策略示例 | 8 | **complete** |
| W2 | L3 治理服务 — 5 API 契约 | 12 | **complete** |
| W3 | L4 会话管理器 — 四平面骨架 | 10 | **complete** |
| W4 | SDK `eaasp run` + E2E 编排脚本 | 8 | **complete** |
| W5 | E2E 集成测试 — 双模式 | 14 | pending |
| W6 | HR 示例完善 + 审计 Hook | 6 | pending |
| W7 | Makefile + 设计文档收尾 | 0 | pending |
| **总计** | | **~58** | |

---

## W1: 策略 DSL + 编译器 (8 tests)

### 产出文件

| 文件 | 说明 |
|------|------|
| `tools/eaasp-governance/pyproject.toml` | Python 包配置 |
| `tools/eaasp-governance/src/eaasp_governance/__init__.py` | 包初始化 |
| `tools/eaasp-governance/src/eaasp_governance/models/policy.py` | PolicyBundle/PolicyRule Pydantic V2 模型 |
| `tools/eaasp-governance/src/eaasp_governance/compiler.py` | YAML DSL → managed_hooks_json |
| `tools/eaasp-governance/src/eaasp_governance/merger.py` | 四层级合并器 (deny-always-wins) |
| `sdk/examples/hr-onboarding/policies/enterprise.yaml` | 企业级策略 |
| `sdk/examples/hr-onboarding/policies/bu_hr.yaml` | HR BU 策略 |
| `tools/eaasp-governance/tests/test_compiler.py` | 8 tests |

### 验收标准
- `pytest tools/eaasp-governance/tests/test_compiler.py -xvs` 全通过
- 编译输出的 managed_hooks_json 可被 `HookExecutor.load_rules()` 正确加载
- 层级合并遵循 deny-always-wins

---

## W2: L3 治理服务 — 5 API 契约 (12 tests)

### 产出文件

| 文件 | 说明 |
|------|------|
| `tools/eaasp-governance/src/eaasp_governance/main.py` | FastAPI app :8083 |
| `tools/eaasp-governance/src/eaasp_governance/api/policy_deploy.py` | 契约 1 |
| `tools/eaasp-governance/src/eaasp_governance/api/intent_gateway.py` | 契约 2 |
| `tools/eaasp-governance/src/eaasp_governance/api/skill_lifecycle.py` | 契约 3 |
| `tools/eaasp-governance/src/eaasp_governance/api/telemetry_ingest.py` | 契约 4 |
| `tools/eaasp-governance/src/eaasp_governance/api/session_control.py` | 契约 5 |
| `tools/eaasp-governance/src/eaasp_governance/clients/l1_runtime.py` | gRPC L1 客户端 |
| `tools/eaasp-governance/src/eaasp_governance/clients/l2_registry.py` | HTTP L2 客户端 |
| `tools/eaasp-governance/src/eaasp_governance/runtime_pool.py` | 运行时池 |
| `tools/eaasp-governance/src/eaasp_governance/session_state.py` | 会话状态机 |
| `tools/eaasp-governance/config/runtimes.yaml` | 运行时池配置 |
| `tools/eaasp-governance/tests/test_api.py` | 12 tests |

### 验收标准
- 5 个 API 契约路由全部可访问
- 三方握手: POST /v1/sessions 能串通 L2 + L1
- `pytest tools/eaasp-governance/tests/ -xvs` 全通过

---

## W3: L4 会话管理器 — 四平面 (10 tests)

### 产出文件

| 文件 | 说明 |
|------|------|
| `tools/eaasp-session-manager/pyproject.toml` | Python 包配置 |
| `tools/eaasp-session-manager/src/eaasp_session/__init__.py` | 包初始化 |
| `tools/eaasp-session-manager/src/eaasp_session/main.py` | FastAPI app :8084 |
| `tools/eaasp-session-manager/src/eaasp_session/planes/experience.py` | 体验平面 |
| `tools/eaasp-session-manager/src/eaasp_session/planes/integration.py` | 集成平面 |
| `tools/eaasp-session-manager/src/eaasp_session/planes/control.py` | 控制平面 |
| `tools/eaasp-session-manager/src/eaasp_session/planes/persistence.py` | 持久化平面 |
| `tools/eaasp-session-manager/src/eaasp_session/models.py` | Pydantic 模型 |
| `tools/eaasp-session-manager/src/eaasp_session/clients/l3_client.py` | L3 HTTP 客户端 |
| `tools/eaasp-session-manager/config/intents.yaml` | 意图映射 |
| `tools/eaasp-session-manager/tests/test_session_manager.py` | 10 tests |

### 验收标准
- POST /v1/conversations 能通过 L3 创建会话
- SQLite 持久化: sessions + execution_log + telemetry_events
- `pytest tools/eaasp-session-manager/tests/ -xvs` 全通过

---

## W4: SDK `eaasp run` + E2E 编排 (8 tests)

### 产出文件

| 文件 | 说明 |
|------|------|
| `sdk/python/src/eaasp/cli/run_cmd.py` | `eaasp run` CLI 命令 |
| `sdk/python/src/eaasp/client/platform_client.py` | L4 HTTP 客户端 |
| `scripts/e2e-mvp.sh` | 一键 E2E 编排脚本 |
| 更新 `sdk/python/src/eaasp/cli/__main__.py` | 注册 run 命令 |
| `sdk/python/tests/test_run_cmd.py` | 8 tests |

### 验收标准
- `eaasp run --platform --mock-llm` 能走完全链路
- `eaasp run --platform --live-llm` 能连接真实 LLM（需 API Key）
- `scripts/e2e-mvp.sh` 全自动执行无人工干预

---

## W5: E2E 集成测试 (14 tests)

### 产出文件

| 文件 | 说明 |
|------|------|
| `tests/e2e/conftest.py` | 服务启动/停止 fixture |
| `tests/e2e/helpers.py` | 测试工具函数 |
| `tests/e2e/test_api_contracts.py` | 5 API 契约冒烟 (5 tests) |
| `tests/e2e/test_three_way_handshake.py` | 三方握手 (3 tests) |
| `tests/e2e/test_hook_enforcement.py` | Hook 强制 (4 tests) |
| `tests/e2e/test_session_lifecycle.py` | 会话生命周期 (2 tests) |

### 验收标准
- `pytest tests/e2e/ -m mock_llm -xvs` 全通过（不需要 API Key）
- `pytest tests/e2e/ -m live_llm -xvs` 全通过（需要 API Key + 服务运行中）

---

## W6: HR 示例完善 (6 tests)

### 产出文件

| 文件 | 说明 |
|------|------|
| `sdk/examples/hr-onboarding/hooks/audit_logger.py` | PostToolUse 审计 hook |
| 更新 `sdk/examples/hr-onboarding/SKILL.md` | 增加审计 hook |
| 更新 `sdk/examples/hr-onboarding/tests/test_cases.jsonl` | PII 正反例 |
| `sdk/examples/hr-onboarding/run_e2e.py` | 自包含 E2E 脚本 |
| `sdk/examples/hr-onboarding/README.md` | 操作指南 |

---

## W7: Makefile + 文档收尾 (0 tests)

### 产出

| 项目 | 说明 |
|------|------|
| Makefile 新增 | `l3-setup/start/test`, `l4-setup/start/test`, `e2e-setup/run/test/teardown/full` |
| 更新 EAASP_ROADMAP.md | Phase BH-MVP 节 |
| 更新 NEXT_SESSION_GUIDE.md | Phase BH-MVP 成果 + 后续 |

---

## Deferred Items

| ID | 内容 | 前置条件 |
|----|------|---------|
| BH-D1 | RBAC 访问控制 | 用户身份管理 |
| BH-D2 | 审批闸门 | L4 审批 UI |
| BH-D3 | 审计持久化 | L4 持久化完善 |
| BH-D4 | MCP 注册中心 | L2 MCP 扩展 |
| BH-D5 | NLU 意图解析 | NLU 模型 |
| BH-D6 | L4 管理控制台 UI | Web 框架 |
| BH-D7 | L4 事件总线 | 消息队列 |
| BH-D8 | L4 可观测性枢纽 | Grafana/Prometheus |
| BH-D9 | 多租户 | PostgreSQL |
| BH-D10 | 策略版本回滚 | 版本存储 |
| BH-D11 | 成本治理 | Cost Ledger |
| BH-D12 | T2/T3 HookBridge 验证 | 非 T1 运行时 |
