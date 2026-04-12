# EAASP v2.0 Phase 0.5 — 人工验收步骤清单

> **文档性质**：S5.T2 人工验收操作手册
> **创建日期**：2026-04-12
> **所有命令在项目根目录执行**

---

## L1 Runtime LLM Provider 配置（D20）

| Runtime | Provider | API Key | Model |
|---------|----------|---------|-------|
| grid-runtime | `OPENAI_*` | `OPENAI_API_KEY` | `OPENAI_MODEL_NAME` |
| claude-code-runtime | `ANTHROPIC_*` | `ANTHROPIC_API_KEY` | `ANTHROPIC_MODEL_NAME` |
| hermes-runtime | OpenRouter | `HERMES_API_KEY` / `OPENROUTER_API_KEY` | `HERMES_MODEL` |

所有 key 在 `.env` 文件中，通过 OpenRouter 统一接入。

---

## 前置条件

```bash
# 确认 .env 存在且有必要变量
grep -E "LLM_PROVIDER|OPENAI_API_KEY|OPENAI_BASE_URL|OPENAI_MODEL_NAME|ANTHROPIC_API_KEY" .env

# 确认 Rust 二进制已编译
ls target/debug/grid-runtime target/debug/eaasp-skill-registry

# 确认 Python .venv 就位
ls tools/eaasp-cli-v2/.venv/bin/eaasp
```

---

## Step 1: 一键启动所有服务

```bash
make dev-eaasp
# 或跳过编译：bash scripts/dev-eaasp.sh --skip-build
```

**验收标准**：
- [ ] 状态表显示 8 个服务全部 UP
- [ ] pre-flight 打印出正确的 LLM_PROVIDER / MODEL_NAME / BASE_URL
- [ ] 无 TIMEOUT 或 ERROR 消息

---

## Step 2: 提交 Skill + 部署 Policy

```bash
# 新开终端，在项目根目录执行：

# 提交 skill
make eaasp-skill-submit SKILL=examples/skills/threshold-calibration/

# 推进 skill 状态
tools/eaasp-cli-v2/.venv/bin/eaasp skill promote threshold-calibration 0.1.0 tested

# 部署 managed-settings
make eaasp-policy-deploy CONFIG=scripts/assets/mvp-managed-settings.json

# 验证
make eaasp-skill-list
make eaasp-policy-list
```

**验收标准**：
- [ ] skill submit 返回 `id: "threshold-calibration"`
- [ ] policy deploy 返回 version + hook_count
- [ ] skill list 显示 threshold-calibration
- [ ] policy list 显示部署的版本

---

## Step 3: grid-runtime 端到端（OPENAI_* Provider）

```bash
make eaasp-session-run SKILL=threshold-calibration MSG="校准 Transformer-001 的温度阈值"

# 或指定 runtime（默认就是 grid-runtime）：
make eaasp-session-run SKILL=threshold-calibration RUNTIME=grid-runtime MSG="校准 Transformer-001 的温度阈值"
```

**验收标准**：
- [ ] session 创建成功（显示 session_id）
- [ ] 流式输出显示 agent 思考 + tool 调用 + 回复
- [ ] grid-runtime 日志确认使用 OpenAI/OpenRouter provider

---

## Step 4: claude-code-runtime 端到端（ANTHROPIC_* Provider）

```bash
make eaasp-session-run SKILL=threshold-calibration RUNTIME=claude-code-runtime MSG="校准 Transformer-001 的温度阈值"
```

**验收标准**：
- [ ] session 创建成功
- [ ] 流式输出正常
- [ ] claude-code-runtime 日志确认使用 Anthropic provider

---

## Step 5: hermes-runtime 端到端（OpenRouter Provider）

```bash
make eaasp-session-run SKILL=threshold-calibration RUNTIME=hermes-runtime MSG="校准 Transformer-001 的温度阈值"
```

**验收标准**：
- [ ] session 创建成功
- [ ] 流式输出正常
- [ ] hermes-runtime 日志确认使用 OpenRouter provider

---

## Step 6: Memory 写入验证

```bash
make eaasp-memory-search Q="Transformer-001"
```

**验收标准**：
- [ ] 返回非空结果
- [ ] 结果包含 Step 3/4/5 写入的 evidence 数据

---

## Step 7: 跨 Session 跨 Runtime 记忆验证

```bash
# 用不同 runtime 创建新 session，验证引用了之前的记忆
make eaasp-session-run SKILL=threshold-calibration RUNTIME=claude-code-runtime MSG="再校准一次 Transformer-001"
```

**验收标准**：
- [ ] agent 输出中引用了之前 session 的校准数据

---

## Step 8: Hook Deny 验证

```bash
# 查看当前 session 列表
make eaasp-session-list

# 在任一 active session 中发送（替换 SID）
make eaasp-session-send SID=<SESSION_ID> MSG="写入 SCADA 系统"
```

**验收标准**：
- [ ] `scada_write*` 工具调用被 PreToolUse hook 阻止
- [ ] agent 回复说明无法执行写入

---

## Step 9: 停止所有服务

```bash
# 回到 dev-eaasp 终端按 Ctrl+C，或：
make dev-eaasp-stop
```

**验收标准**：
- [ ] 所有端口释放，无残留进程

---

## 验收结果汇总

| Step | 描述 | Runtime | 结果 |
|------|------|---------|------|
| 1 | 一键启动（8 服务） | ALL | ⬜ |
| 2 | Skill + Policy | — | ⬜ |
| 3 | 端到端 | grid-runtime (OPENAI_*) | ⬜ |
| 4 | 端到端 | claude-code-runtime (ANTHROPIC_*) | ⬜ |
| 5 | 端到端 | hermes-runtime (OPENROUTER) | ⬜ |
| 6 | Memory 写入 | — | ⬜ |
| 7 | 跨 session 记忆 | 跨 runtime | ⬜ |
| 8 | Hook deny | 任一 runtime | ⬜ |
| 9 | 停止服务 | ALL | ⬜ |

**最低通过**：Step 1-3 + Step 6 + Step 8-9 全部 PASS。
**完整通过**：全部 PASS。

---

## 验收通过后的文档更新

1. `EAASP_v2_0_EVOLUTION_PATH.md` — Phase 0.5 → 🟢 Completed
2. `docs/plans/.checkpoint.json` — phase = `phase-0.5-done`
3. `EAASP_v2_0_MVP_SCOPE.md` — Phase 0.5 ticks
