# EAASP v2.0 — 人工 E2E 验证长期指南

> **性质**: 长期指南（Living Document）。每个大阶段结束前参考本文进行人工 E2E 验证。
> **演进规则**: Phase 推进时**追加**能力行到矩阵；**不改**已有行结构；**不创建**新 `PHASE_X_E2E_VERIFICATION_GUIDE.md`。
> **最近更新**: 2026-04-18（Phase 3 sign-off）

---

## 一、验证哲学

### 何时做人工 E2E

| 时机 | 是否要做 |
|------|----------|
| 大阶段（Phase N.0 / N.5）收尾前 | ✅ 必须 |
| 新 L1 runtime 接入后首轮 sign-off | ✅ 必须 |
| 新增 **拦截器事件类型** / **hook 类型** / **Provider** | ✅ 必须 |
| 单 Task 结束 | ❌ 不需要（合约/集成/单元测试已覆盖） |
| Deferred 修复 | ❌ 不需要（对应测试用例覆盖即可） |

### 人工 E2E 的唯一价值

**验证真实 LLM + 真实 MCP server + 真实容器部署在端到端路径上正确工作。** mock-based 的单元/集成/合约测试覆盖不到这些。

### 不做什么

- ❌ 不代替合约套件（`make v2-phase2_5-e2e` 等自动化门控）
- ❌ 不代替单元测试覆盖（`cargo test` + `pytest`）
- ❌ 不覆盖性能/压力测试
- ❌ 不 cover mock 已验证过的字段形状

---

## 二、前提与快速启动

### 2.1 工作目录 + .env

```bash
cd /Users/sujiangwen/sandbox/LLM/speechless.ai/SGAI/grid-sandbox

# .env 必需（MEMORY.md Env Var Conventions）:
#   OPENAI_API_KEY / OPENAI_BASE_URL / OPENAI_MODEL_NAME  → grid + nanobot
#   ANTHROPIC_API_KEY / ANTHROPIC_BASE_URL                → claude-code
#   LLM_PROVIDER                                          → grid provider 选择
```

### 2.2 CLI 别名

```bash
alias eaasp='/Users/sujiangwen/sandbox/LLM/speechless.ai/SGAI/grid-sandbox/tools/eaasp-cli-v2/.venv/bin/eaasp'
```

### 2.3 一条命令起全栈

```bash
# 首次（或容器镜像变更后）构建 goose 镜像
make goose-runtime-container-build

# 起全栈（含 4 runtime）
make dev-eaasp
```

**预期状态表**：

```
SERVICE                  PORT   PID    PROVIDER     STATUS
skill-registry           18081  ...    -            UP
L2 memory-engine         18085  ...    -            UP
L3 governance            18083  ...    -            UP
mock-scada(SSE)          18090  ...    tool-sandbox UP
L2 MCP Orchestrator      18082  ...    -            UP
grid-runtime             50051  ...    OPENAI_*     UP
claude-code-runtime      50052  ...    ANTHROPIC_*  UP
nanobot-runtime          50054  ...    OPENAI_*     UP
goose-runtime(docker)    50063  ...    OPENAI_*     UP
L4 orchestration         18084  ...    -            UP
```

**失败自愈**：`make dev-eaasp-stop && make dev-eaasp`。

---

## 三、Runtime 能力矩阵

> **演进规则**: 新 runtime 接入时加一列；能力升级时改该列对应行的 ✅/⚠️/❌。

| 能力 | grid | claude-code | nanobot | goose | pydantic-ai | claw-code | ccb |
|---|---|---|---|---|---|---|---|
| gRPC 16 方法合约 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Initialize / Terminate / Health | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 真实 LLM Provider | ✅ OpenAI-compat | ✅ Anthropic SDK | ✅ OpenAI-compat | ❌ | ✅ OpenAI-compat | ✅ OpenAI-compat | ❌ stub |
| Agent loop（多轮 tool 调用） | ✅ | ✅ | ⚠️ 骨架无工具 | ❌ stub | ✅ pydantic-ai | ✅ agent loop | ❌ stub echo |
| ConnectMCP（工具注入） | ✅ | ✅ | ✅ Phase 3 | ✅ Phase 3 ACP | ❌ stub | ❌ stub | ❌ stub |
| PreToolUse / PostToolUse Hook | ✅ | ✅ | ✅ Phase 3 | ❌ | ⚠️ stub | ⚠️ stub | ⚠️ stub |
| Stop Hook（ADR-V2-006） | ✅ | ✅ | ✅ Phase 3 | ❌ | ⚠️ stub | ⚠️ stub | ⚠️ stub |
| HookContext envelope parity（D120） | ✅ | ✅ | ✅ | ❌ | ⚠️ stub | ⚠️ stub | ⚠️ stub |
| 容器部署（ADR-V2-019） | — | — | ❌ native only | ✅ Docker | ❌ native | ❌ native | ❌ native |
| contract v1.1（ADR-V2-020） | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **参与人工 E2E 分类** | **必验** | **必验** | **基线** | **基线** | **对比基线** | **对比基线** | **对比基线** |

### Runtime 角色（ADR-V2-017）

- **主力**: grid-runtime（Rust，自研完整实现）
- **样板**: claude-code-runtime（Python + Anthropic SDK）、nanobot-runtime（Python + OpenAI-compat）
- **对比**: goose-runtime（容器模板，ADR-V2-019 baseline，Phase 3 接完整 ACP）、pydantic-ai-runtime（Python + pydantic-ai，真实 LLM loop）、claw-code-runtime（Rust，Claw Code agent loop）、ccb-runtime（Bun/TypeScript，stub Send）
- **冻结**: hermes-runtime（2026-04-14 冻结）

---

## 四、功能特性 × 验证方法矩阵

> **演进规则**: 每行一特性。Phase 推进时**追加行**，不改已有行。"本次必跑" 在 **七、Phase 收尾历史** 附录按阶段标注。

### 4.1 Group A — threshold-calibration 一轮覆盖

一个 session 就能触发的功能，放在 A 组。**每次大阶段收尾必跑所有已完整接线的 runtime。** Phase 3 后：grid / claude-code / nanobot / goose / pydantic-ai / claw-code 各跑一轮（ccb 为 stub，走 B14 基线即可）。

| # | Phase | 能力 | 验收断言（events） |
|---|---|---|---|
| A1 | 0.75 | MCP stdio 连接 + 工具注入 | `SESSION_MCP_CONNECTED` 出现 |
| A2 | 1 | Event Engine 拦截器：SESSION_START | `SESSION_START` 出现 |
| A3 | 1 | Event Engine 拦截器：PRE_TOOL_USE | `PRE_TOOL_USE ≥ 4`（D87 多步 workflow） |
| A4 | 1 | Event Engine 拦截器：POST_TOOL_USE | `POST_TOOL_USE ≥ PRE_TOOL_USE - 1` |
| A5 | 1 | Event Engine 拦截器：STOP | `STOP == 1` |
| A6 | 1 | source metadata | `source == "interceptor:<runtime>"` |
| A7 | 1 | cluster_id 填充 | `cluster_id` 非空（至少部分事件） |
| A8 | 2.S1.T4 | tool_name threading | `PRE_TOOL_USE.payload.tool_name` 非空 |
| A9 | 2.S1.T5 | response_text 抽取 | `RESPONSE_CHUNK.content` 非空 |
| A10 | 2.S3.T4 | Stop Hook InjectAndContinue | `require_anchor.sh` 拒绝空 evidence_anchor |
| A11 | 2.S3.T5 | ScopedHookExecutor + ADR-V2-006 envelope | runtime 日志：hook exit code + stdin 含 `skill_id/event/tool_args` |
| A12 | 2.5.S0.T3 | D120 HookContext envelope parity | PRE_TOOL_USE.payload 含 `event/skill_id/tool_args/tool_result/is_error` |
| A13 | 2.5 | L1 生态扩展（≥3 runtime） | 4 runtime 在状态表 UP |
| A14 | 3.S3.T1-T2 | goose ACP Send 流式 | `SESSION_START` + `STOP` 出现，无 gRPC UNAVAILABLE |
| A15 | 3.S3.T3-T5 | nanobot ConnectMCP + Stop hook | `SESSION_MCP_CONNECTED` 出现；Stop hook payload 含 `evidence_anchor_id`（若 skill 写入） |

**A 组验证命令**：

```bash
# ── 主力 runtime（A1-A15 全验）──────────────────────────────────
# grid
eaasp session create --skill threshold-calibration --runtime grid-runtime
export SID=<id>
eaasp session send $SID "校准 Transformer-001 的温度阈值"
eaasp session events $SID                              # A2-A12 人工过一遍
eaasp session events $SID --format json | head -80     # A11-A12 深度验

# claude-code
eaasp session create --skill threshold-calibration --runtime claude-code-runtime
export SID2=<id>
eaasp session send $SID2 "校准 Transformer-001 的温度阈值"
eaasp session events $SID2

# ── Phase 3 新增接线 runtime（A14-A15 + 基线事件链）─────────────
# nanobot（A15: SESSION_MCP_CONNECTED + Stop hook）
eaasp session create --skill threshold-calibration --runtime nanobot-runtime
export SID_N=<id>
eaasp session send $SID_N "校准 Transformer-001 的温度阈值"
eaasp session events $SID_N

# goose（A14: SESSION_START + STOP，无 gRPC UNAVAILABLE）
eaasp session create --skill threshold-calibration --runtime goose-runtime
export SID_G=<id>
eaasp session send $SID_G "校准 Transformer-001 的温度阈值"
eaasp session events $SID_G

# pydantic-ai（基线事件链：SESSION_START + STOP）
eaasp session create --skill threshold-calibration --runtime pydantic-ai-runtime
export SID_PA=<id>
eaasp session send $SID_PA "校准 Transformer-001 的温度阈值"
eaasp session events $SID_PA

# claw-code（基线事件链：SESSION_START + STOP）
eaasp session create --skill threshold-calibration --runtime claw-code-runtime
export SID_CC=<id>
eaasp session send $SID_CC "校准 Transformer-001 的温度阈值"
eaasp session events $SID_CC
```

---

### 4.2 Group B — 分项回归（一轮触发不到）

单轮 threshold-calibration 不触发、需要另造会话/环境的能力。**每次完整人工 E2E 必须 B 组全跑**（回归测试原则：Phase N 验过不代表 Phase M 没破坏）。由 `scripts/eaasp-e2e.sh` 自动驱动。

| # | Phase | 能力 | 触发方法 | 验收断言 |
|---|---|---|---|---|
| B1 | 2.S1.T6 | ErrorClassifier 14 FailoverReason | 改 `.env` 造错的 `OPENAI_API_KEY` + send | `RUNTIME_SEND_FAILED.payload.failover_reason` 匹配 |
| B2 | 2.S1.T7 | graduated retry + jitter | 同 B1 | 日志有 2-3 次重试，间隔递增 |
| B3 | 2.S2.T1 | HNSW 向量 + Ollama embedding | 起 Ollama，`eaasp memory search --query` | score 分布合理（非全 0） |
| B4 | 2.S2.T2 | 混合检索权重可调 | `EAASP_HYBRID_WEIGHTS=0.3,0.7 eaasp memory search` | 返回顺序变化可观察 |
| B5 | 2.S2.T3 | memory_confirm MCP 工具 | 第二轮会话："确认建议写为 confirmed" | `PRE_TOOL_USE(memory_confirm)` 出现 |
| B6 | 2.S2.T4 | 状态机 agent_suggested→confirmed→archived | B5 后 `eaasp memory list --status confirmed` | 有新 confirmed 条目 |
| B7 | 2.S2.T5 | L3 聚合溢出 blob ref | skill 里造 >10K 字符 tool output | `RESPONSE_CHUNK.payload` 含 `blob_ref` |
| B8 | 2.S3.T1 | PreCompactHook | 长对话超上下文窗口 | events 含 `PRE_COMPACT` |
| B9 | 2.S3.T2-T3 | skill-extraction meta-skill | `--skill skill-extraction` 另跑一轮 | 写出 skill_draft memory_file |
| B10 | 2.5.W1.T2.5 | goose 容器 F1 gate | `make goose-runtime-container-verify-f1` | exit 0 |
| B11 | 2.5.S0 | 合约套件 v1 四 runtime GREEN | `make v2-phase2_5-e2e` | 全通过 |
| B12 | 3.S3.T6-T7 | pydantic-ai LLM E2E（真实 loop） | `eaasp session create --runtime pydantic-ai-runtime` + send | `SESSION_START` + `STOP`，无 ERROR |
| B13 | 3.S3.T8-T9 | claw-code LLM E2E（Rust agent loop） | `eaasp session create --runtime claw-code-runtime` + send | 同 B12 |
| B14 | 3.S3.T10-T11 | ccb gRPC 基线（stub echo） | grpc Health + Initialize + Terminate 三连 | 全部 OK，无 panic |
| B15 | 3 | 合约套件 v1.1 七 runtime GREEN | `make v2-phase3-e2e` | 112 pytest PASS |

---

## 五、核心验证流程

### 5.1 唯一入口：一条命令跑完整 E2E

```bash
# 1. 起全栈 (Terminal A, 常驻)
make dev-eaasp-stop && make dev-eaasp

# 2. 一条命令跑 A + B 全矩阵 (Terminal B)
bash scripts/eaasp-e2e.sh
```

### 5.2 脚本行为契约

`scripts/eaasp-e2e.sh` 的职责（由下一节验证矩阵驱动）：

1. **Pre-flight** — 验 L4 健康 / CLI 可用 / skill 已注册（未注册自动 submit）
2. **A 组** — 对 grid-runtime + claude-code-runtime 各跑一轮 threshold-calibration，逐项断言 A1-A13
3. **B 组** — 顺序执行 B1-B11 各自的触发 + 断言（能自动化的直接跑，需真实数据的复用 A 组 session）
4. **Runtime 基线** — nanobot / goose 跑 Initialize/Terminate/Health 最小合约
5. **汇总** — 写 `.e2e/verify-$(date +%Y%m%d-%H%M).log` + 打印表格：行号 / PASS/FAIL/SKIP/XFAIL / 说明
6. **退出码** — 0 (全 PASS/XFAIL/SKIP) / 1 (任何 FAIL) / 2 (pre-flight 失败)

### 5.3 脚本 flag

```bash
bash scripts/eaasp-e2e.sh                 # 全量
bash scripts/eaasp-e2e.sh --only A        # 只 A 组
bash scripts/eaasp-e2e.sh --only B        # 只 B 组
bash scripts/eaasp-e2e.sh --skip B7,B8    # 跳过耗费大的
bash scripts/eaasp-e2e.sh --runtime grid  # 只测单 runtime
```

### 5.4 手动逐项（紧急排错）

所有行都有独立触发命令，见第四节矩阵的"触发方法"列。脚本失败时复制对应命令手动跑。

---

### 5.5 人工分步观察方案

**目的**: 人眼能看到实际运行过程，可观察、可复现。不追求一键，追求**逐步可确认**。

**前置**: `make dev-eaasp` 已在 Terminal A 运行（grid / claude-code / nanobot 已在状态表 UP）。

```bash
cd /Users/sujiangwen/sandbox/LLM/speechless.ai/SGAI/grid-sandbox
alias eaasp='tools/eaasp-cli-v2/.venv/bin/eaasp'
```

**整体顺序**：Step 0 自动化门控 → Step 1-2 主力 runtime 完整回归 → Step 3 系统级功能回归 → Step 4 各 runtime 分项验证

---

#### Step 0 — 自动化门控（先跑，失败则不继续人工步骤）

```bash
# 合约套件 v1.1 七 runtime（112 pytest）
make v2-phase3-e2e

# Phase 2.5 合约套件 v1 回归
make v2-phase2_5-e2e
```

**标准**: 全部 PASS（允许 XFAIL）。有 FAIL 则停，查原因后再继续。

---

#### Step 1 — 注册 skills（首次或重置后）

```bash
eaasp skill submit examples/skills/threshold-calibration
eaasp skill submit examples/skills/skill-extraction
eaasp skill submit examples/skills/memory-confirm-test
eaasp skill submit examples/skills/transformer-calibration
```

---

#### Step 2 — 主力 runtime 完整事件链回归（A1-A12）

每个 runtime 跑同一条命令，验收标准相同。

```bash
# ── grid-runtime ──────────────────────────────────────────────
eaasp session create --skill threshold-calibration --runtime grid-runtime
export SID=<session_id>
eaasp session send $SID "请校准 Transformer-001 的温度阈值，完整执行工作流"
eaasp session events $SID --format json | jq '{
  PRE:([.events[]|select(.event_type=="PRE_TOOL_USE")]|length),
  POST:([.events[]|select(.event_type=="POST_TOOL_USE")]|length),
  STOP:([.events[]|select(.event_type=="STOP")]|length),
  tools:[.events[]|select(.event_type=="PRE_TOOL_USE")|.payload.tool_name]
}'

# ── claude-code-runtime ────────────────────────────────────────
eaasp session create --skill threshold-calibration --runtime claude-code-runtime
export SID2=<id>
eaasp session send $SID2 "请校准 Transformer-001 的温度阈值，完整执行工作流"
eaasp session events $SID2 --format json | jq '{PRE:([.events[]|select(.event_type=="PRE_TOOL_USE")]|length),STOP:([.events[]|select(.event_type=="STOP")]|length)}'
```

**标准**: PRE ≥ 4 / POST ≈ PRE / STOP == 1（两个 runtime 均需满足）。

---

#### Step 3 — 系统级功能回归（Phase 2 能力，runtime 无关）

以下各项复用 Step 2 的 `$SID2`（claude-code session，PRE_TOOL_USE 有数据）。

**3a — hook envelope parity（A11-A12，ADR-V2-006 + D120）**

```bash
eaasp session events $SID2 --format json | \
  jq '.events[] | select(.event_type=="PRE_TOOL_USE") | .payload' | head -30
```

**观察**: payload 含 `tool_name` + `arguments`。（`skill_id` / `event` 字段为 D120 deferred — Phase 3 仍 None，不阻塞。）

**3b — 混合检索 + HNSW（Phase 2.S2.T1-T2）**

```bash
# query 是 positional argument，无 --query flag
eaasp memory search "Transformer-001 温度阈值" --top-k 5
EAASP_HYBRID_WEIGHTS=1.0,0.0 eaasp memory search "Transformer-001 温度阈值" --top-k 5
```

**观察**: 两次返回顺序不同（纯 FTS vs FTS+语义融合）。

**3c — 状态机 memory_confirm（Phase 2.S2.T3-T4）**

```bash
eaasp session send $SID2 "确认刚才的阈值建议，调用 memory_confirm 把 status 设为 confirmed"
# memory list 无 --status flag，用 list 后人眼看 status 列
eaasp memory list --limit 5
```

**观察**: `memory list` 输出中目标 memory_id 的 status 列变为 `confirmed`。

**3d — skill-extraction meta-skill（Phase 2.S3.T2-T3）**

```bash
eaasp session create --skill skill-extraction --runtime grid-runtime
export SID_SX=<id>
eaasp session send $SID_SX "从刚才 Transformer-001 的校准会话抽取可复用 skill 草稿"
```

**观察**: 流式输出 LLM 调 `memory_search → memory_read → memory_write_file`，结果含结构化 YAML skill draft。

---

#### Step 4 — 各 runtime 分项验证（Phase 3 新接线）

各节独立，可按需单跑。每节标注前提条件。

##### 4a — nanobot：ConnectMCP + Stop hook（Phase 3.S3.T3-T5）

> **前提**: nanobot-runtime `:50054` 已在 `make dev-eaasp` 中启动。

```bash
eaasp session create --skill threshold-calibration --runtime nanobot-runtime
export SID_N=<id>
eaasp session send $SID_N "校准 Transformer-001 的温度阈值"
eaasp session events $SID_N
```

**观察**: `SESSION_MCP_CONNECTED` 出现；`PRE_TOOL_USE ≥ 1`；Stop hook payload 含 `evidence_anchor_id`。

##### 4b — goose：ACP Send 流式（Phase 3.S3.T1-T2）

> **前提**: `make goose-runtime-container-run`（或本地 goose 二进制）已启动，端口 `:50063`。

```bash
# 先验容器健康
make goose-runtime-container-verify-f1

eaasp session create --skill threshold-calibration --runtime goose-runtime
export SID_G=<id>
eaasp session send $SID_G "校准 Transformer-001 的温度阈值"
eaasp session events $SID_G
```

**观察**: `SESSION_START` + `STOP` 出现；无 gRPC `UNAVAILABLE`；流式不卡死。goose 无 MCP 工具注入时 `PRE_TOOL_USE` 为 0（正常，XFAIL）。

##### 4c — pydantic-ai：LLM E2E（Phase 3.S3.T6-T7）

> **前提**: `cd lang/pydantic-ai-runtime-python && PYDANTIC_AI_RUNTIME_PORT=50065 .venv/bin/python -m pydantic_ai_runtime`

```bash
eaasp session create --skill threshold-calibration --runtime pydantic-ai-runtime
export SID_PA=<id>
eaasp session send $SID_PA "校准 Transformer-001 的温度阈值"
eaasp session events $SID_PA
```

**观察**: `SESSION_START` + `STOP` 出现；runtime 日志无 ERROR；有工具调用记录。

##### 4d — claw-code：LLM E2E（Phase 3.S3.T8-T9）

> **前提**: `cargo run -p eaasp-claw-code-runtime -- --port 50066`
> **配置**: `OPENAI_BASE_URL` / `OPENAI_API_KEY` / `OPENAI_MODEL_NAME` 已设。

```bash
eaasp session create --skill threshold-calibration --runtime claw-code-runtime
export SID_CL=<id>
eaasp session send $SID_CL "校准 Transformer-001 的温度阈值"
eaasp session events $SID_CL
```

**观察**: `SESSION_START` + `STOP` 出现；Rust 日志无 panic。

##### 4e — ccb：gRPC 基线（Phase 3.S3.T10-T11，stub Send）

> **前提**: `cd lang/ccb-runtime-ts && CCB_RUNTIME_PORT=50067 bun run src/main.ts`

```bash
.venv/bin/python -c "
import sys, grpc, uuid
sys.path.insert(0, 'lang/claude-code-runtime-python/src')
from claude_code_runtime._proto.eaasp.runtime.v2 import common_pb2, runtime_pb2, runtime_pb2_grpc
ch = grpc.insecure_channel('127.0.0.1:50067')
stub = runtime_pb2_grpc.RuntimeServiceStub(ch)
print('Health:', stub.Health(common_pb2.Empty(), timeout=5).healthy)
init = stub.Initialize(runtime_pb2.InitializeRequest(payload=common_pb2.SessionPayload(
    session_id='ccb-'+str(uuid.uuid4())[:8], user_id='verifier', runtime_id='ccb-runtime')), timeout=10)
print('session:', init.session_id)
stub.Terminate(common_pb2.Empty(), timeout=5)
print('Terminate OK')
"
```

**观察**: `Health: True` + `session: <id>` + `Terminate OK`；无 Bun panic。Send 为 stub echo，有响应即 PASS。

---

**Sign-off 判定**:
- Part A Step 2+4: PRE ≥ 4 / STOP = 1
- Part B 所有 5.5.N 小节可观察到预期输出
- Part C + 全矩阵: `bash scripts/eaasp-e2e.sh` exit 0

三者满足 → `/end-phase`。

---

### 5.6 持续维护承诺（两条路径同步演进）

每次大阶段新增能力，**必须同步更新两处**：

| 路径 | 更新位置 | 内容 |
|------|---------|------|
| 自动化全矩阵 | `scripts/eaasp-e2e.sh` | A/B 组函数追加断言 |
| 人工分步观察 | 本节 5.5 Part B | 追加 5.5.N 小节（不改已有） |

目标：任何时刻任何人：
- 一条命令看全量回归结果（自动化）
- 按步骤亲眼看新功能跑起来（分步观察）

---

## 六、Sign-off 门控

### 6.1 必要条件（全矩阵必验）

| # | 条件 | 证据 |
|---|---|---|
| 1 | A 组 grid + claude-code 全通过（A1-A13 全绿） | `.e2e/verify-*.log` |
| 2 | B 组 B1-B11 全部 PASS 或明确 XFAIL（无 FAIL） | `.e2e/verify-*.log` |
| 3 | 所有已接入 runtime 至少基线通过 | 同上 |
| 4 | 脚本退出码 = 0 | `echo $?` |

**原则**：每次完整人工 E2E = 全矩阵回归。不允许"Phase N 验过不用再验" — 这违背回归测试原则。

### 6.2 Sign-off 判定表

| 条件 | 动作 |
|---|---|
| 脚本 exit 0 + A+B 全绿 | → `/end-phase` |
| A 组任一 FAIL | ⛔ 阻塞 — 核心能力回归，根因分析 |
| B 组 FAIL（非 XFAIL） | ⛔ 阻塞 — 回归测试失败，必须查清 |
| 新 runtime 基线 FAIL | ⛔ 阻塞 — 该 runtime 合约退出 |
| XFAIL（已知预期失败） | ✅ 允许 — 要有 Deferred 记录引用 |
| 新暴露 gap | 记 Deferred，该行改 XFAIL + 给出归属 Phase |

### 6.3 Deferred 记录规范

每条新 Deferred 必须含：
- D-编号（DEFERRED_LEDGER.md 顺延）
- 哪次 E2E 暴露的
- 期望归属 Phase
- 严重度（P1-P3）
- 触发条件（复现命令）

---

## 七、Phase 收尾历史

> **每个大阶段收尾时追加一节。不编辑其他阶段的节。**

### Phase 1 — Event Engine（2026-04-14）

- **A 组**: 全通过（A1-A7，当时还没 A8+ 特性）
- **B 组**: 无（所有能力都在 A 组）
- **结果**: 🟢 Completed
- **原 artifact**: `docs/main/PHASE1_E2E_VERIFICATION_GUIDE.md`

### Phase 2 — Memory & Evidence（2026-04-15）

- **A 组**: A1-A11 全通过（S3.T5 ScopedHookExecutor 加入 A 组）
- **B 组**: 全跑（当时脚本手动驱动，见 `scripts/s4t3-runtime-verification.sh`）
- **结果**: 🟢 Completed 23/23

### Phase 2.5 — L1 Runtime Ecosystem（2026-04-18 🟢 Completed）

- **本次引入能力**: A12 (D120 HookContext parity) + A13 (L1 生态扩展) + B10 (goose F1) + B11 (合约 v1) + nanobot/goose runtime
- **全矩阵回归**: Sign-off E2E PASS (34 checks, 0 FAIL, 8 TODO, 4 SKIP, exit 0)
- **本次 sign-off 挖出并治本的 grid-engine/grid-runtime 核心 bug**:
  1. BROADCAST_CAPACITY 256 过小 → Done chunk 丢失 → gRPC stream 不关 (fix: 4096 + Lag-fallback)
  2. `EAASP_TOOL_FILTER=on` env 读取逻辑在 055badf 被 squash 丢失 (fix: 恢复)
  3. KG/MCP-manage tools 绕过 tool_filter (fix: 尊重 filter)
  4. executor.rs AgentTool/QueryAgentTool 无条件注册 (fix: 检查 session registry)
  5. Stop hook envelope 缺 evidence_anchor_id/draft_memory_id → 永远 InjectAndContinue → cap reached (fix: memory_write_anchor/file 后捕获 id 注入)
  6. 只 materialize SKILL.md 不 copy hooks/ → `${SKILL_DIR}/hooks/*.sh` 不存在 (fix: copy hooks/ + exec bit)
  7. L4 token-level chunk 不聚合 → 612 events/session 撑爆 API (fix: send/stream 都聚合 text_delta/thinking → 35 events/session)
- **10 个新回归测试固定**: L4 coalescing (5) + Rust phase2_5_regression (3) + hooks materialization (2)，全 PASS
- **新 runtime 能力**: nanobot（真实 LLM + agent loop 骨架，无 MCP 工具注入）+ goose（Initialize/Terminate/Health + 容器 F1 gate，Send stub → Phase 3）
- **新 Deferred**: D144（nanobot/goose ConnectMCP 工具注入 → Phase 3）
- **结果**: 🟢 25/25 Completed @ 2026-04-18
- **本次 artifact**: `docs/main/PHASE2_5_E2E_VERIFICATION_GUIDE.md`（历史归档）

### Phase 3 — L1 Runtime Functional Completeness（2026-04-18 🟢 Completed）

- **本次引入能力**:
  - A14 (goose ACP Send 流式，S3.T1-T2)
  - A15 (nanobot ConnectMCP + Stop hook，S3.T3-T5，D144 关闭)
  - B12 (pydantic-ai LLM E2E，S3.T6-T7)
  - B13 (claw-code LLM E2E，S3.T8-T9)
  - B14 (ccb gRPC 基线，S3.T10-T11)
  - B15 (合约套件 v1.1 七 runtime，`make v2-phase3-e2e` 112 pytest PASS)
- **自动化门控**: `make v2-phase3-e2e` 112 pytest PASS（B1-B8 八组）；cargo check workspace clean；contract v1.1 7 runtimes × 42 PASS 22 XFAIL
- **新 runtime 能力**:
  - goose: ACP parser + next_event() + Send 流式接线（容器模板）
  - nanobot: ConnectMCP stdio MCP client + Stop hook dispatch（D144 ✅）
  - pydantic-ai: 真实 LLM agent loop（OpenAI-compat），contract v1.1 通过
  - claw-code: Rust agent loop，真实 LLM Send，contract v1.1 通过
  - ccb: Bun/TypeScript stub Send，gRPC 16 方法合约通过，contract v1.1 通过
- **ADR**: ADR-V2-020 Accepted（tool namespace L0/L1/L2，contract-v1.1.0 tag）
- **人工 sign-off runbook**: `scripts/phase3-runtime-verification.sh`（≥4/7 runtime PASS）
- **结果**: 🟢 35/35 Completed @ 23a7520（2026-04-18）

- 预期新 A 组行: goose/nanobot 的 ConnectMCP 工具注入
- 预期新 B 组行: pydantic-ai / claw-code / ccb runtime 评估
- 预期升级: goose Send 完整 ACP 接线

---

## 八、故障排查速查

| 症状 | 可能原因 | 处理 |
|---|---|---|
| `make dev-eaasp` 超时 | 某 runtime .venv/镜像缺失 | `--skip-nanobot` / `--skip-goose` |
| nanobot 404 `/v1/v1/chat/completions` | base_url 重复 `/v1` | provider.py 已做 normalize（2026-04-17） |
| PRE_TOOL_USE < 4 | D87 capability matrix 未启用 | 检查 runtime 日志 tool_choice 决策 |
| events 只有 SESSION_CREATED/USER_MESSAGE | 拦截器没触发 | 检查 runtime 运行中，L4 EventEngine 在 lifespan 启动 |
| cluster_id 全空 | pipeline worker 未启动 | L4 启动日志查 `pipeline_worker: running` |
| Stop hook 不 inject | ScopedHookExecutor 没 register_session_stop_hooks | Rust 日志查 `session_stop_hooks` 计数 |

---

## 九、参考

- **核心 ADR**: ADR-V2-001 / 006 / 015 / 016 / 017 / 019
- **演进路径**: `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md`
- **Deferred Ledger**: `docs/design/EAASP/DEFERRED_LEDGER.md`
- **自动化门控**: `make v2-phase2_5-e2e` / `scripts/s4t3-runtime-verification.sh`
- **环境约定**: `MEMORY.md` → env var conventions / no fallback / integration before E2E

---

## 十、维护承诺

- 每次大阶段收尾必更新第 7 节
- 新增能力先在合约/单元测试 GREEN → 才进本文第 4 节矩阵
- runtime 能力矩阵第 3 节变更必须同步到 ADR-V2-017
- 本文作者放 `docs/design/EAASP/`（per CLAUDE.md File Organization），不与 `docs/main/` 混放
