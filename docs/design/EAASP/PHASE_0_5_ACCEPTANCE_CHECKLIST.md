# EAASP v2.0 Phase 0.5 — 人工验收步骤清单

> **文档性质**：S5.T2 人工验收操作手册
> **创建日期**：2026-04-12
> **对应 Plan**：`docs/plans/2026-04-12-v2-mvp-phase0.5-plan.md` §人工验收标准

---

## 前置条件

```bash
# 1. 确认 API Key 已设置
echo $ANTHROPIC_API_KEY | head -c 10   # 应显示 sk-ant-xxx

# 2. 确认 Rust 二进制已编译
ls target/debug/grid-runtime
ls target/debug/eaasp-skill-registry

# 3. 确认 Python .venv 就位
ls tools/eaasp-l2-memory-engine/.venv/bin/python
ls tools/eaasp-l3-governance/.venv/bin/python
ls tools/eaasp-l4-orchestration/.venv/bin/python
ls tools/eaasp-cli-v2/.venv/bin/python

# 4. 确认端口未被占用
for port in 18081 18083 18084 18085 50051; do
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
- 5 个服务依次启动（skill-registry → L2 → L3 → grid-runtime → L4）
- 每个服务 health check 通过（`ready` 标记）
- 最终显示状态表，全部 `UP`

**验收标准**：
- [ ] 状态表显示 5 个服务全部 UP
- [ ] 无 TIMEOUT 或 ERROR 消息

---

## Step 2: 提交并激活 Skill

```bash
# 新开终端，进入项目根目录
cd /Users/sujiangwen/sandbox/LLM/speechless.ai/SGAI/grid-sandbox

# 提交 skill
cd tools/eaasp-cli-v2 && .venv/bin/python -m eaasp_cli_v2 skill submit \
  ../../examples/skills/threshold-calibration/SKILL.md

# 记录返回的 SKILL_ID（例如：threshold-calibration）
# 推进版本
.venv/bin/python -m eaasp_cli_v2 skill promote <SKILL_ID> 0.1.0
```

**预期输出**：
- `skill submit` 返回 skill ID 和 metadata
- `skill promote` 返回版本确认

**验收标准**：
- [ ] skill submit 成功，返回有效 ID
- [ ] skill promote 成功，version = 0.1.0

---

## Step 3: 部署 managed-settings

```bash
.venv/bin/python -m eaasp_cli_v2 policy deploy \
  ../../scripts/assets/mvp-managed-settings.json
```

**预期输出**：
- 策略部署成功，返回 version ID

**验收标准**：
- [ ] policy deploy 成功，无错误
- [ ] 返回的 version ID 非空

---

## Step 4: 创建 Session 并发送任务（grid-runtime）

```bash
# 创建 session
.venv/bin/python -m eaasp_cli_v2 session create \
  --skill threshold-calibration --runtime grid-runtime

# 记录返回的 SESSION_ID
# 发送任务（默认流式输出）
.venv/bin/python -m eaasp_cli_v2 session send \
  "校准 Transformer-001 的温度阈值"
```

**预期输出**：
- `session create` 返回 session ID + 状态 `active`
- `session send` 实时显示 agent 流式输出：
  - `[thinking]` 思考过程
  - `[tool_call: scada_read_snapshot]` 工具调用
  - response text 最终回复

**验收标准**：
- [ ] session create 成功，状态为 `active`
- [ ] session send 显示流式输出（非空白等待）
- [ ] 输出中包含 tool_call（如 `scada_read_snapshot`）
- [ ] agent 产生了有意义的分析结果

---

## Step 5: 验证 Memory 写入

```bash
.venv/bin/python -m eaasp_cli_v2 memory search "Transformer-001"
```

**预期输出**：
- 返回包含 "Transformer-001" 相关的 memory anchor 或 memory file
- 至少 1 条结果

**验收标准**：
- [ ] memory search 返回非空结果
- [ ] 结果中包含 Step 4 写入的 evidence 数据

---

## Step 6: 跨 Session 记忆验证（可选，需 claude-code-runtime）

> **注意**：`dev-eaasp.sh` 当前不启动 claude-code-runtime。
> 如需验证跨 runtime 记忆，需手动启动：
> ```bash
> cd lang/claude-code-runtime-python && .venv/bin/python -m claude_code_runtime.main
> ```

```bash
# 创建新 session（不同 runtime）
.venv/bin/python -m eaasp_cli_v2 session create \
  --skill threshold-calibration --runtime claude-code-runtime

.venv/bin/python -m eaasp_cli_v2 session send \
  "再校准一次 Transformer-001"
```

**预期输出**：
- 第二个 session 的 agent 输出中引用了第一次 session 的记忆
- memory_refs 注入到 system prompt 中

**验收标准**：
- [ ] 第二个 session 创建成功
- [ ] agent 输出中提到了之前的校准数据或 evidence

---

## Step 7: Hook Deny 验证

```bash
.venv/bin/python -m eaasp_cli_v2 session send "写入 SCADA 系统"
```

**预期输出**：
- PreToolUse hook `block_write_scada.sh` 拦截 `scada_write*` 工具调用
- hook exit code = 2（deny）
- agent 报告无法写入 SCADA

**验收标准**：
- [ ] scada_write 类工具调用被阻止
- [ ] agent 回复中说明无法执行写入操作
- [ ] grid-runtime 日志显示 hook deny 记录

---

## Step 8: 停止所有服务

```bash
# 方式 1：回到 dev-eaasp 终端，按 Ctrl+C
# 方式 2：在任意终端执行
make dev-eaasp-stop
```

**验收标准**：
- [ ] 所有 5 个端口释放
- [ ] 无残留进程

---

## 验收结果汇总

| Step | 描述 | 结果 | 备注 |
|------|------|------|------|
| 1 | 一键启动 | ⬜ PASS / FAIL | |
| 2 | Skill 提交 | ⬜ PASS / FAIL | |
| 3 | Policy 部署 | ⬜ PASS / FAIL | |
| 4 | Session + 流式输出 | ⬜ PASS / FAIL | |
| 5 | Memory 写入 | ⬜ PASS / FAIL | |
| 6 | 跨 session 记忆（可选） | ⬜ PASS / FAIL / SKIP | |
| 7 | Hook deny | ⬜ PASS / FAIL | |
| 8 | 停止服务 | ⬜ PASS / FAIL | |

**Phase 0.5 最低通过标准**：Step 1-5 + Step 7-8 全部 PASS（Step 6 可选）。

---

## 验收通过后的文档更新

1. 更新 `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md` — Phase 0.5 状态 → 🟢 Completed
2. 更新 `docs/plans/.checkpoint.json` — phase = `phase-0.5-done`
3. 更新 `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` — Phase 0.5 ticks
4. 创建 `docs/main/WORK_LOG_PHASE_0_5.md` — 工作日志
