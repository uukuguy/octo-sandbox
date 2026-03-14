# octo-sandbox 下一会话指南

**最后更新**: 2026-03-14 20:50 GMT+8
**当前分支**: `main`
**当前状态**: Phase J ACTIVE — 沙箱安全体系建设

---

## 项目状态：沙箱安全体系建设中

评估框架 Phase A-I 全部完成。1992 tests passing @ `500e444`。
Phase J 已完成计划，准备执行。

```
Level 4: 端到端任务成功率 (SWE-bench 50 tasks)     → ✅
Level 3: 多轮对话+工具链协调 (GAIA 50 + τ-bench 30) → ✅
Level 2: 单次工具调用精确度 (BFCL 50 tasks)          → ✅
Level 1: 引擎基础能力 (单元测试 1992 tests)           → ✅
沙箱安全: SandboxPolicy + 审计日志                   → 🔄 Phase J
```

### 完成清单

| 阶段 | Tasks | 状态 | Commit |
|------|-------|------|--------|
| Wave 1-10: v1.0-v1.1 | 全部 | COMPLETE | `675155d` |
| Phase A-I: 评估框架 | 全部 | COMPLETE | `500e444` |
| **Phase J: 沙箱安全体系** | **0/16** | **ACTIVE** | — |
| Phase K: 模型报告 | 0/10 | PLANNED | — |

---

## Phase J: 沙箱安全体系建设 (16 tasks, 7 groups)

### 设计文档
- 设计方案: `docs/design/SANDBOX_SECURITY_DESIGN.md`
- 实施计划: `docs/plans/2026-03-14-phase-j-sandbox-security.md`
- Checkpoint: `docs/plans/.checkpoint.json`

### 任务分组

| Group | Tasks | Description | 依赖 | 状态 |
|-------|-------|-------------|------|------|
| J1 | T1-T2 | SandboxPolicy 策略引擎 | — | PENDING |
| J2 | T1-T2 | Docker 镜像 + 语言自动检测 | J1 | PENDING |
| J3 | T1-T2 | DockerAdapter 修复加固 | J1 | PENDING |
| J4 | T1-T3 | WASM/WASI 完整可用 | J1 | PENDING |
| J5 | T1-T3 | 沙箱审计日志 | J2,J3,J4 | PENDING |
| J6 | T1-T2 | Docker 测试修复 | J3 | PENDING |
| J7 | T1-T2 | CI 集成 + 全量验证 | J6 | PENDING |

**执行顺序**: J1 → J2|J3|J4(并行) → J5 → J6 → J7

### 关键设计决策

1. **SandboxPolicy::Strict 为默认** — 生产环境拒绝 Subprocess
2. **Docker 镜像**: `python:3.12-slim-bookworm`, `rust:1.92-bookworm`, `node:22-bookworm-slim`, `alpine:latest`
3. **WASM 完整 WASI** — `wasmtime_wasi` 捕获 stdio，支持 CLI 工具
4. **审计复用 AuditStorage** — event_type="sandbox" + metadata JSON + hash-chain

### 关键代码路径

| 组件 | 文件 | 说明 |
|------|------|------|
| 沙箱 traits | `crates/octo-engine/src/sandbox/traits.rs` | SandboxPolicy + RuntimeAdapter |
| Router | `crates/octo-engine/src/sandbox/router.rs` | SandboxRouter + ToolCategory |
| Docker | `crates/octo-engine/src/sandbox/docker.rs` | DockerAdapter + ImageRegistry |
| WASM | `crates/octo-engine/src/sandbox/wasm.rs` | WasmAdapter + WASI CLI |
| Subprocess | `crates/octo-engine/src/sandbox/subprocess.rs` | SubprocessAdapter |
| 审计 | `crates/octo-engine/src/audit/storage.rs` | AuditStorage + hash-chain |
| 安全策略 | `crates/octo-engine/src/security/policy.rs` | SecurityPolicy + AutonomyLevel |
| Docker 测试 | `crates/octo-engine/tests/sandbox_docker_test.rs` | 7+1 个测试 |

---

## 基线

- **Tests**: 1992 passing @ `500e444`
- **评估任务**: ~297 个 (内部 167 + 外部 130)
- **测试命令**: `cargo test --workspace -- --test-threads=1`
- **LLM 配置**: `.env` 中 OpenRouter 端点
