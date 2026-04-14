# Phase 1 Event Engine — 人工 E2E 验证指南

> **执行时机**: Phase 1 所有自动化测试通过后（123/123 L4 + 13 CLI），准备进入 end-phase 前。
> **目标**: 证明 Event Engine 在真实 agent loop 中端到端工作，不是纸面承诺。
> **参考提交**: `8c174c2`（第二轮审计修复）/ `081c21b`（第一轮审计修复）

---

## 一、Prerequisites

### 1.1 工作目录

```bash
cd /Users/sujiangwen/sandbox/LLM/speechless.ai/SGAI/grid-sandbox
```

### 1.2 `.env` 必需变量

```bash
# claude-code-runtime
ANTHROPIC_API_KEY=sk-ant-xxx

# grid-runtime via OpenRouter
OPENAI_API_KEY=sk-or-xxx
OPENAI_BASE_URL=https://openrouter.ai/api/v1
OPENAI_MODEL_NAME=<model>
LLM_PROVIDER=openai
```

### 1.3 CLI 别名

```bash
alias eaasp='/Users/sujiangwen/sandbox/LLM/speechless.ai/SGAI/grid-sandbox/tools/eaasp-cli-v2/.venv/bin/eaasp'
```

---

## 二、Execution Steps

### Step 1（Terminal A）: 启动所有服务

```bash
make dev-eaasp
```

**预期关键 stdout**:
- `L2 memory on :18085`
- `L3 governance on :18083`
- `L4 orchestration on :18084`
- `skill-registry on :18081`
- `mcp-orchestrator on :18082`
- `grid-runtime on :50051` 或 claude-code-runtime/hermes-runtime

**失败标志**:
- `port already in use` → 先 `make dev-eaasp-stop`
- `ANTHROPIC_API_KEY is required` / `OPENAI_API_KEY is required` → 补 `.env`

---

### Step 2（Terminal B）: 创建 session + 发送消息（grid-runtime）

```bash
eaasp session create --skill threshold-calibration --runtime grid-runtime
# 记下返回的 session_id
export SID='<paste session_id>'
```

**预期**: 返回 JSON，`session_id` 形如 `sess_xxxxxxxxxxxx`，`status: "active"`。

```bash
eaasp session send $SID "校准 Transformer-001 的温度阈值"
```

**预期**:
- agent 流式输出（文本累积）
- 若调用 tool: `[tool_call: scada_read_snapshot]` / `[tool_result: ...]`
- 收尾 `── N events, M chars total`

---

### Step 3（★ Phase 1 核心验收点）: 查事件流

```bash
eaasp session events $SID
```

**必须出现的事件序列**:

| Event Type | 来源 | 验证点 |
|------------|------|--------|
| `SESSION_CREATED` | L4 直写 | Phase 0 功能 |
| `RUNTIME_INITIALIZED` | L4 直写 | Phase 0 功能 |
| **`SESSION_START`** | ★ Interceptor | **Phase 1 新能力** |
| `SESSION_MCP_CONNECTED` | Phase 0.75 | MCP 连接成功 |
| `USER_MESSAGE` | L4 直写 | — |
| `RESPONSE_CHUNK` × N | L4 直写 | — |
| **`PRE_TOOL_USE`** | ★ Interceptor | **Phase 1 新能力**（依赖 agent 调 tool） |
| **`POST_TOOL_USE`** | ★ Interceptor | **Phase 1 新能力** |
| **`STOP`** | ★ Interceptor | **Phase 1 新能力** |

**关键验收断言**:
1. ✅ `SESSION_START / PRE_TOOL_USE / POST_TOOL_USE / STOP` 至少各出现 1 次 → **拦截器工作**
2. ✅ 这些事件至少部分 `cluster_id` 非空（形如 `c-xxxxxxxx`）→ **pipeline worker 运行**
3. ✅ source metadata = `interceptor:grid-runtime` → **来源追踪正确**

**失败标志**:
- 只有 `SESSION_CREATED / RUNTIME_INITIALIZED / USER_MESSAGE / RESPONSE_CHUNK` → **拦截器没触发**
- 有事件但 `cluster_id` 全空 → **pipeline worker 未启动**
- `PRE_TOOL_USE` 缺失但 `RESPONSE_CHUNK` 带 tool_name → **chunk_type 仍然不匹配**

---

### Step 4: JSON 格式深度验证

```bash
eaasp session events $SID --format json | head -80
```

**找一个 `PRE_TOOL_USE` 事件，验证字段完整**:

```json
{
  "seq": 7,
  "event_type": "PRE_TOOL_USE",
  "payload": {
    "tool_name": "scada_read_snapshot",
    "arguments": {}
  },
  "event_id": "xxx-xxx-xxx",
  "source": "interceptor:grid-runtime",
  "cluster_id": "c-abcd1234"
}
```

---

### Step 5: 第二个 runtime 验证（claude-code-runtime）

```bash
eaasp session create --skill threshold-calibration --runtime claude-code-runtime
export SID2='<new session_id>'
eaasp session send $SID2 "校准 Transformer-001"
eaasp session events $SID2
```

**预期**: 同样应该看到 `PRE_TOOL_USE / POST_TOOL_USE / STOP`。

**意义**: 证明 **ADR-V2-001 决策（T2 runtime 零改动走拦截器）实际可行**。

---

### Step 6（可选）: Phase 0.5 → Phase 1 FTS 迁移验证

```bash
eaasp session list --limit 20 | grep -E "closed|created"
# 选一个老的 OLD_SID（Phase 0.75 之前）
eaasp session events $OLD_SID | head -10
# 应该能看到老事件 → 证明 migration 未破坏旧数据
```

---

### Step 7: 停止服务

```bash
make dev-eaasp-stop
```

---

## 三、反馈清单

执行完后请提交：

1. **Step 3 完整输出**（所有事件列表）
2. **Step 4 JSON 片段**（一条 `PRE_TOOL_USE` 的完整 JSON）
3. **错误点**（如果有）
4. **claude-code-runtime 是否成功**（Step 5）

---

## 四、验收结果分支

| 结果 | 后续动作 |
|------|---------|
| ✅ 全部通过 | 进入 end-phase，关闭 Phase 1，标 🟢 Completed |
| ⚠️ 部分通过 | 定位断点，修复 + 补测试，重验 |
| ❌ 完全失败 | dev-eaasp.sh 环境排查 / 根因分析 |

---

## 五、自动化测试覆盖的断言（不用手动验）

以下已被 `test_event_integration.py` 覆盖（123/123 通过），不用再人工验：

- `stream_message` 触发 interceptor 提取 `tool_start` chunk → `PRE_TOOL_USE`
- `/v1/events/ingest` 不存在的 session → 404
- `/v1/events/ingest` 成功事件 → 真实落盘 + `cluster_id` 回写
- EventEngine 在 lifespan 里启动并能 ingest
- engine.ingest 抛异常时 stream 不中断（resilience）
- Phase 0.5 → Phase 1 FTS5 迁移不丢搜索能力

**人工 E2E 的价值**: 验证**真实 LLM + 真实 MCP server + 真实 tool 调用**在端到端路径上正确工作，单元/集成测试用 stub 覆盖不到这些。

---

## 六、验收后的 end-phase 流程

通过后执行：

1. 更新 `docs/plans/.checkpoint.json`: `status: "ARCHIVED — Phase 1 complete"`
2. 更新 `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md`: Phase 1 → 🟢 Completed (2026-04-XX)
3. 写 WORK_LOG 条目（docs/main/）
4. mem-save: `project_eaasp_v2_phase1_complete.md`
5. 提交 commit: `docs(eaasp): end-phase Phase 1 🟢 — Event Engine E2E verified`
6. 制定 Phase 2 (Memory and Evidence) 启动计划
