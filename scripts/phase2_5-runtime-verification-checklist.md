# Phase 2.5 运行时 E2E 验证检查表

> **版本**: Phase 2.5 S4.T1  
> **用途**: 人工逐项检查，签字后关闭 Phase 2.5 出口门控  
> **脚本**: `bash scripts/phase2_5-runtime-verification.sh`

---

## 验证前提

- [ ] 至少 2 个真实 OpenAI-compat 端点可用（非 Mock）
- [ ] L2 Memory Service 运行中（或 stub 可接受写入）
- [ ] `tests/contract/` 依赖已安装（`make v2-phase2_5-ci-setup`）

---

## Runtime 1 — grid-runtime

**启动命令**:
```bash
cargo run -p grid-runtime -- --port 50061
```

| 检查项 | 结果 |
|--------|------|
| runtime 启动无 ERROR 日志 | ☐ |
| Initialize → session_id 非空 | ☐ |
| Send → ≥1 TOOL_CALL 事件 | ☐ |
| TOOL_RESULT 事件 status=ok | ☐ |
| PostToolUse hook 触发 | ☐ |
| 最终 chunk_type == "done" | ☐ |
| L2 evidence_anchor 写入 | ☐ |
| L2 memory_file 写入 | ☐ |

**签字**: ________________  **日期**: ________________

---

## Runtime 2 — claude-code-runtime

**启动命令**:
```bash
cd lang/claude-code-runtime-python
CLAUDE_CODE_RUNTIME_GRPC_ADDR=0.0.0.0:50062 \
  ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY \
  .venv/bin/python -m claude_code_runtime
```

| 检查项 | 结果 |
|--------|------|
| runtime 启动无 ERROR 日志 | ☐ |
| Initialize → session_id 非空 | ☐ |
| Send → ≥1 TOOL_CALL 事件 | ☐ |
| TOOL_RESULT 事件 status=ok | ☐ |
| PostToolUse hook 触发 | ☐ |
| 最终 chunk_type == "done" | ☐ |
| L2 evidence_anchor 写入 | ☐ |
| L2 memory_file 写入 | ☐ |

**签字**: ________________  **日期**: ________________

---

## Runtime 3 — eaasp-goose-runtime *(需 goose 二进制)*

**启动命令**:
```bash
GOOSE_BIN=$(which goose) \
  GOOSE_RUNTIME_GRPC_ADDR=0.0.0.0:50063 \
  cargo run -p eaasp-goose-runtime
```

| 检查项 | 结果 |
|--------|------|
| goose 二进制可用 | ☐ |
| runtime 启动无 ERROR 日志 | ☐ |
| Initialize → session_id 非空 | ☐ |
| Send → ≥1 事件（任意类型） | ☐ |
| Terminate 正常 | ☐ |

> ⚠️ Phase 2.5 goose Send 为 stub 实现，完整 E2E 留 Phase 3

**签字**: ________________  **日期**: ________________

---

## Runtime 4 — nanobot-runtime

**启动命令**:
```bash
cd lang/nanobot-runtime-python
OPENAI_BASE_URL=$OPENAI_BASE_URL \
  OPENAI_API_KEY=$OPENAI_API_KEY \
  OPENAI_MODEL_NAME=$OPENAI_MODEL_NAME \
  NANOBOT_RUNTIME_PORT=50064 \
  .venv/bin/python -m nanobot_runtime
```

| 检查项 | 结果 |
|--------|------|
| runtime 启动无 ERROR 日志 | ☐ |
| Initialize → session_id 非空 | ☐ |
| Send → ≥1 TOOL_CALL 事件 | ☐ |
| TOOL_RESULT 事件 status=ok | ☐ |
| 最终 chunk_type == "done" | ☐ |
| L2 evidence_anchor 写入 | ☐ |

**签字**: ________________  **日期**: ________________

---

## 总体签字门控

- [ ] ≥2 个 runtime 通过全部检查项
- [ ] 至少 1 个使用真实 OpenAI-compat 端点（非 Mock）
- [ ] `phase2_5-verification-log.txt` 已生成并包含 ≥2 PASS

**总体签字**: ________________  **日期**: ________________

---

## 参考

- 合约测试: `tests/contract/contract_v1/`
- 自动化门控: `make v2-phase2_5-e2e`
- ADR-V2-017: `docs/design/EAASP/adrs/ADR-V2-017-l1-runtime-ecosystem-strategy.md`
- ADR-V2-019: `docs/design/EAASP/adrs/ADR-V2-019-l1-runtime-deployment-model.md`
- Phase 2 参考: `scripts/s4t3-runtime-verification.sh`
