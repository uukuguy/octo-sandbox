# octo-sandbox 下一会话指南

**最后更新**: 2026-03-20 06:40 GMT+8
**当前分支**: `main`
**当前状态**: Phase S COMPLETE + CLI/Server 可用性修复完成

---

## 项目状态

从 Wave 1 到 Phase S，所有计划阶段均已完成。CLI 和 Server 可用性问题已修复。

```
CLI/Server Fixes: Usability hardening    → COMPLETE @ b4ebcbe
Phase S:  Agent Capability Boost (13/13) → COMPLETE @ 68ad13e
Phase R:  GAIA Filtered Eval (8/8)       → COMPLETE @ 50df5e6
Phase Q:  GAIA & SWE-bench (15/15)       → COMPLETE @ 1ce10e5
Phase P:  Baseline Eval R2 (16/16)       → COMPLETE @ b0ba059
Phase O:  Deferred 暂缓项全解锁 (15/15)  → COMPLETE @ 9da42de
Phase N:  Agent Debug Panel (7/7)        → COMPLETE @ 3ba3351
Phase M-b: TUI Dual-View + Eval (8/8)   → COMPLETE @ 76bc12e
Phase M-a: Eval CLI Unification (12/12)  → COMPLETE @ e2b505b
Phase L:  Eval Whitebox (18/18)          → COMPLETE @ f28ad6c
Phase K:  Model Benchmark (11/12)        → COMPLETE @ 07f7ae9
Phase J:  Sandbox Security (16/16)       → COMPLETE @ 45a7342
Phase I:  External Benchmarks (13/13)    → COMPLETE @ 57ca310
Phase H:  Eval Capstone (10/10)          → COMPLETE @ 37680ec
Phase A-G: Eval Framework (85/85)        → COMPLETE @ ca5c898
Wave 1-10: Core Engine + CLI            → COMPLETE @ 675155d
```

### 基线数据

- **Tests**: 2250 passing
- **评估任务**: ~297 个 (内部 167 + 外部 130)
- **GAIA 结果**: MiniMax-M2.1 41.6%, Qwen3.5-27B 39.2%
- **测试命令**: `cargo test --workspace -- --test-threads=1`

### 本次修复重点

| 问题 | 修复 |
|------|------|
| CLI `-c` 冲突 | resume 改为 `-C` |
| CLI Ctrl+C 不退出 | 双击退出模式 |
| CLI/Server 401 | ProviderConfig 读 env var |
| UTF-8 truncate panic | `floor_char_boundary()` |
| Server Ctrl+C 卡死 | force-exit guard (5s) |
| Server DEBUG 刷屏 | `OCTO_LOG` 替代 `RUST_LOG` |
| Web agent 看不到文件 | working_dir → current_dir() |

---

## 下一步建议

1. **S-D1 Agent Skills 规范研究** — Phase S 遗留专题
2. **前端集成** — 将 TUI 新功能同步到 web/ 前端
3. **更强模型评估** — 使用 Claude/GPT-4o 级模型跑 GAIA 对比
4. **Agent 工具链增强** — 更多工具、更好的搜索策略
5. **多 agent 协作** — dual agent mode 完善

---

## 快速启动

```bash
# 编译检查
cargo check --workspace

# 全量测试
cargo test --workspace -- --test-threads=1

# CLI 交互模式
make cli-run

# 启动 server + web
make dev

# Server 单独启动
make server
```
