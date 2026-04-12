# EAASP v2.0 Phase 0.5 — 人工验收步骤清单

> **文档性质**：S5.T2 人工验收操作手册
> **创建日期**：2026-04-12
> **对应 Plan**：`docs/plans/2026-04-12-v2-mvp-phase0.5-plan.md` §人工验收标准

---

## L1 Runtime LLM Provider 配置策略（D20）

| Runtime | 默认 Provider | API Key 环境变量 | 默认模型 |
|---------|-------------|----------------|---------|
| **grid-runtime** (Rust) | `openai` | `OPENAI_API_KEY`（`LLM_PROVIDER=anthropic` 时回退 `ANTHROPIC_API_KEY`） | `gpt-4o`（或 `LLM_MODEL` 覆盖） |
| **claude-code-runtime** (Python) | Anthropic（Claude Agent SDK 限定） | `ANTHROPIC_API_KEY` | `claude-sonnet-4-20250514` |
| **hermes-runtime** (Python) | OpenRouter（可路由任意模型） | `HERMES_API_KEY` → fallback `OPENROUTER_API_KEY` | `anthropic/claude-sonnet-4-20250514` |

**规则**：只有明确使用 Claude 的 runtime 才读 `ANTHROPIC_*`，其余默认走 `OPENAI_*` 通路。所有后续新增 L1 Runtime 遵循同一规则。

---

## 前置条件

```bash
# 1. 确认 API Key 已设置（至少需要一个）
echo $OPENAI_API_KEY | head -c 10          # grid-runtime 需要
echo $ANTHROPIC_API_KEY | head -c 10       # claude-code-runtime 需要
echo $OPENROUTER_API_KEY | head -c 10      # hermes-runtime 需要（可选）

# 2. 确认 Rust 二进制已编译
ls target/debug/grid-runtime
ls target/debug/eaasp-skill-registry

# 3. 确认 Python .venv 就位
ls tools/eaasp-l2-memory-engine/.venv/bin/python
ls tools/eaasp-l3-governance/.venv/bin/python
ls tools/eaasp-l4-orchestration/.venv/bin/python
ls tools/eaasp-cli-v2/.venv/bin/python
ls lang/claude-code-runtime-python/.venv/bin/python
ls lang/hermes-runtime-python/.venv/bin/python

# 4. 确认端口未被占用
for port in 18081 18083 18084 18085 50051 50052 50053; do
  lsof -nP -iTCP:$port -sTCP:LISTEN 2>/dev/null && echo "PORT $port IN USE!" || echo "Port $port free"
done
```

---

## Step 1: 一键启动所有服务

```bash
make dev-eaasp
# 或跳过编译：bash scripts/dev-eaasp.sh --skip-build
```

**预期输出**：
- 8 个服务依次启动（skill-registry → L2 → L3 → grid-runtime → claude-code-runtime → hermes-runtime → L4）
- 每个服务 health check 通过（`ready` 标记）
- 最终显示状态表，全部 `UP`，显示各 runtime 的 PROVIDER 列

**验收标准**：
- [ ] 状态表显示 8 个服务全部 UP
- [ ] grid-runtime PROVIDER 列显示 `OPENAI_*`
- [ ] claude-code-runtime PROVIDER 列显示 `ANTHROPIC_*`
- [ ] hermes-runtime PROVIDER 列显示 `OPENROUTER`
- [ ] 无 TIMEOUT 或 ERROR 消息

---

## Step 2: 提交并激活 Skill

```bash
# 新开终端，进入 CLI 目录
cd /Users/sujiangwen/sandbox/LLM/speechless.ai/SGAI/grid-sandbox/tools/eaasp-cli-v2

# 提交 skill
.venv/bin/eaasp skill submit \
  ../../examples/skills/threshold-calibration/SKILL.md

# 记录返回的 SKILL_ID（例如：threshold-calibration）
# 推进版本
.venv/bin/eaasp skill promote <SKILL_ID> 0.1.0
```

**验收标准**：
- [ ] skill submit 成功，返回有效 ID
- [ ] skill promote 成功，version = 0.1.0

---

## Step 3: 部署 managed-settings

```bash
.venv/bin/eaasp policy deploy \
  ../../scripts/assets/mvp-managed-settings.json
```

**验收标准**：
- [ ] policy deploy 成功，无错误
- [ ] 返回的 version ID 非空

---

## Step 4A: grid-runtime 端到端（OPENAI_* Provider）

```bash
# 创建 session — 指定 grid-runtime
.venv/bin/eaasp session create \
  --skill threshold-calibration --runtime grid-runtime

# 发送任务（流式输出）
.venv/bin/eaasp session send \
  "校准 Transformer-001 的温度阈值"
```

**验收标准**：
- [ ] session create 成功，状态为 `active`
- [ ] session send 显示流式输出（`[thinking]` / `[tool_call]` / response text）
- [ ] 输出中包含 tool_call（如 `scada_read_snapshot`）
- [ ] agent 产生了有意义的分析结果
- [ ] grid-runtime 日志确认使用 OpenAI provider

---

## Step 4B: claude-code-runtime 端到端（ANTHROPIC_* Provider）

```bash
# 创建 session — 指定 claude-code-runtime
.venv/bin/eaasp session create \
  --skill threshold-calibration --runtime claude-code-runtime

# 发送任务
.venv/bin/eaasp session send \
  "校准 Transformer-001 的温度阈值"
```

**验收标准**：
- [ ] session create 成功，状态为 `active`
- [ ] session send 显示流式输出
- [ ] claude-code-runtime 日志确认使用 Anthropic provider

---

## Step 4C: hermes-runtime 端到端（OPENROUTER Provider）

```bash
# 创建 session — 指定 hermes-runtime
.venv/bin/eaasp session create \
  --skill threshold-calibration --runtime hermes-runtime

# 发送任务
.venv/bin/eaasp session send \
  "校准 Transformer-001 的温度阈值"
```

**验收标准**：
- [ ] session create 成功，状态为 `active`
- [ ] session send 显示流式输出
- [ ] hermes-runtime 日志确认使用 OpenRouter/Hermes provider

---

## Step 5: 验证 Memory 写入

```bash
.venv/bin/eaasp memory search "Transformer-001"
```

**验收标准**：
- [ ] memory search 返回非空结果
- [ ] 结果中包含 Step 4A/4B/4C 写入的 evidence 数据

---

## Step 6: 跨 Session 跨 Runtime 记忆验证

```bash
# 用不同于 Step 4A 的 runtime 创建新 session
.venv/bin/eaasp session create \
  --skill threshold-calibration --runtime claude-code-runtime

.venv/bin/eaasp session send \
  "再校准一次 Transformer-001"
```

**验收标准**：
- [ ] 新 session 创建成功
- [ ] agent 输出中引用了之前 session 的校准数据或 evidence（memory_refs 注入验证）

---

## Step 7: Hook Deny 验证

```bash
# 在任一 active session 中发送
.venv/bin/eaasp session send "写入 SCADA 系统"
```

**验收标准**：
- [ ] `scada_write*` 工具调用被 PreToolUse hook `block_write_scada.sh` 阻止
- [ ] agent 回复中说明无法执行写入操作
- [ ] runtime 日志显示 hook deny 记录

---

## Step 8: 停止所有服务

```bash
# 方式 1：回到 dev-eaasp 终端，按 Ctrl+C
# 方式 2：在任意终端执行
make dev-eaasp-stop
```

**验收标准**：
- [ ] 所有 8 个端口（18081/18083/18084/18085/50051/50052/50053）释放
- [ ] 无残留进程

---

## 验收结果汇总

| Step | 描述 | Runtime | Provider | 结果 | 备注 |
|------|------|---------|----------|------|------|
| 1 | 一键启动（8 服务） | ALL | — | ⬜ | |
| 2 | Skill 提交+推进 | — | — | ⬜ | |
| 3 | Policy 部署 | — | — | ⬜ | |
| 4A | Session 端到端 | grid-runtime | OPENAI_* | ⬜ | |
| 4B | Session 端到端 | claude-code-runtime | ANTHROPIC_* | ⬜ | |
| 4C | Session 端到端 | hermes-runtime | OPENROUTER | ⬜ | |
| 5 | Memory 写入验证 | — | — | ⬜ | |
| 6 | 跨 session 记忆 | 跨 runtime | — | ⬜ | |
| 7 | Hook deny | 任一 runtime | — | ⬜ | |
| 8 | 停止服务 | ALL | — | ⬜ | |

**Phase 0.5 最低通过标准**：Step 1-3 + Step 4A + Step 5 + Step 7-8 全部 PASS。Step 4B/4C/6 通过则为完整通过。

---

## 验收通过后的文档更新

1. 更新 `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md` — Phase 0.5 状态 → 🟢 Completed
2. 更新 `docs/plans/.checkpoint.json` — phase = `phase-0.5-done`
3. 更新 `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` — Phase 0.5 ticks
4. 创建 `docs/main/WORK_LOG_PHASE_0_5.md` — 工作日志
