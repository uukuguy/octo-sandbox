# EAASP v2.0 MVP Phase 0 工作摘要

> **时间跨度**：2026-04-11 ~ 2026-04-12
> **计划文件**：`docs/plans/2026-04-11-v2-mvp-phase0-plan.md`
> **最终状态**：🟢 完成（15/15 任务 + 15/15 E2E 断言通过）

---

## 一、总览

Phase 0 是 EAASP v2.0 的基础设施 MVP，目标是验证完整的 L4→L3→L2→L1 跨层链路 + Memory Engine + "阈值校准助手" skill 端到端贯通。

### 关键成果

- **16 方法 Runtime Interface Contract** — v2 proto 定义 + Rust trait + Python stub 全部对齐
- **两个 L1 runtime 通过 certifier** — grid-runtime (Rust) + claude-code-runtime (Python)
- **5 层服务栈** — L1 execution / L2 assets (skill-registry + memory-engine) / L3 governance / L4 orchestration
- **15 条 E2E 断言全部 PASS** — `make v2-mvp-e2e` exit 0 @ commit `a6fad2b`

---

## 二、阶段分解

### S1: Foundation (3 tasks, 2026-04-11)
- 归档 v1.7/v1.8 旧工具和文档
- 创建 v2 proto（16 方法，5-block SessionPayload）
- 移除 v1 proto

**提交**：`483882d`, `a459f84`, `4b4f6a1`, `04c89d7`

### S2: L1 Runtime Refactor (4 tasks, 2026-04-11)
- grid-runtime / eaasp-certifier / hermes-runtime / claude-code-runtime 全部对齐 v2 contract
- 144/145 tests passing（1 ignored）

**提交**：`b37e491`, `1130579`

### S3: L2/L3/L4 Build (5 tasks + 1 cross-cutting, 2026-04-11~12)
- **S3.T1** skill-registry v2 schema + 7 MCP tool REST facade (11 tests)
- **S3.T2** eaasp-l2-memory-engine: 3 层 SQLite + 6 MCP tools + REST facade (47 tests)
- **S3.T3** eaasp-l3-governance: Policy Deploy + Telemetry + Session validate (28 tests)
- **S3.T4** eaasp-l4-orchestration: Intent Gateway + Session Control + 三向握手 (31 tests)
- **S3.T4.5** 跨服务端口重映射 808x→1808x + 环境变量配置化
- **S3.T5** eaasp-cli-v2: typer CLI, 4 子应用 × 14 命令 (19 tests)

**提交**：`58c9814`, `afeb256`, `0907d13`, `9b32716`, `c4d2132`, `85c5c6e`, `a638bc5`

### S4: E2E Integration (3 tasks, 2026-04-12)
- **S4.T1** threshold-calibration skill: SKILL.md (v2 frontmatter) + 3 scoped hooks + mock-scada stub (9 tests)
- **S4.T2** verify-v2-mvp.{sh,py} — 15 断言 E2E 验证脚本 + D1/D2 真接入两个 L1 runtime (4b-lite scope per ADR-V2-004)
- **S4.T3** 文档收尾 + checkpoint 更新（本文档）

**提交**：`f85d1ca`, `98b594b`, `1f3addf`, `a6fad2b`, `b27d7a9`

---

## 三、技术栈与服务

| 服务 | 语言 | 端口 | 目录 |
|------|------|------|------|
| grid-runtime (L1) | Rust | 50051 (gRPC) | `crates/grid-runtime/` |
| claude-code-runtime (L1) | Python | 50052 (gRPC) | `lang/claude-code-runtime-python/` |
| skill-registry (L2) | Rust | 18081 | `tools/eaasp-skill-registry/` |
| l2-memory-engine (L2) | Python | 18085 | `tools/eaasp-l2-memory-engine/` |
| l3-governance (L3) | Python | 18083 | `tools/eaasp-l3-governance/` |
| l4-orchestration (L4) | Python | 18084 | `tools/eaasp-l4-orchestration/` |
| eaasp-cli-v2 | Python | — | `tools/eaasp-cli-v2/` |
| mock-scada | Python | — | `tools/mock-scada/` |

---

## 四、Deferred 项汇总

Phase 0 累计产生 **61 个 Deferred 项**（D1-D61）：
- **2 项已关闭**：D1 (policy_context 接入) ✅, D2 (memory_refs 接入) ✅
- **59 项待补**：分布在 Phase 1 ~ Phase 3 各阶段

关键待解 ADR（Phase 1 前置）：
- ADR-V2-001: `emitEvent()` 方法定义
- ADR-V2-002: Session Event Stream 后端选型
- ADR-V2-003: Event clustering 策略接口

---

## 五、RuFlo Swarm 执行记录

| Swarm ID | 任务 | 模式 |
|----------|------|------|
| swarm-1775923722601 | S3.T2 l2-memory-engine | scout → coder → reviewer |
| swarm-1775925175221 | S3.T3 l3-governance | scout → coder → reviewer |
| swarm-1775926706453 | S3.T4 l4-orchestration | scout → coder → reviewer |
| swarm-1775934399764 | S3.T5 cli-v2 | scout → coder → reviewer |
| swarm-1775937332511 | S4.T1 threshold-calibration | scout → coder → reviewer |
| swarm-1775943476918 | S4.T2 verify-v2-mvp | scout → 3 parallel coders → reviewer |

---

## 六、经验教训

1. **macOS 代理 + httpx + localhost** — 必须设置 `trust_env=False`，否则 Clash 等系统代理会将 127.0.0.1 请求转发导致 502
2. **SQLite FTS5 默认 tokenizer** — AND-of-tokens 语义，多词搜索用 OR 显式连接
3. **proto3 singular submessage** — `HasField` 检测必须用，truthy fallback 永远为 True
4. **uvicorn 清理** — 需要 `pkill -P` + `lsof` 端口扫描，否则子进程成为孤儿
5. **service hit-shape 差异** — 只有真实上游集成测试才能捕获，respx mock 容易掩盖

---

## 七、下一步

进入 **Phase 1: Event-driven foundation**：
1. 解决 ADR-V2-001 / 002 / 003 三个架构决策
2. 实现 L4 Event Engine (ingest → dedup → cluster → state machine)
3. 实现 Session Event Stream
4. 增加 L4 hooks (EventReceived / PreSessionCreate / PostSessionEnd)
