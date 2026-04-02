# octo-sandbox 下一会话指南

**最后更新**: 2026-04-02 08:30 GMT+8
**当前分支**: `main`
**当前状态**: Phase AR 完成，无活跃 Phase

---

## 项目状态

### 已完成 Phases
- Phase A-H: Core Engine + Eval 基础
- Phase I-R: 外部 Benchmark + 标准测试 + 评估
- Phase S: Agent Capability Boost
- Phase T: TUI OpenDev 整合 (24 tasks)
- Phase U: TUI Production Hardening (10 tasks)
- Phase V: Agent Skills (11 tasks)
- Phase W-Z: OctoRoot + TUI + Playbook + Landmine
- Phase AA-AF: 部署配置 + 沙箱容器 + Workspace + SSM
- Phase AG: 记忆和上下文机制增强 (11 tasks + 5 deferred)
- Phase AH: Hook 系统增强 (15 tasks + 3 deferred)
- Phase AI: WASM Component Model Hook 插件 (11 tasks + 4 deferred)
- Phase AJ: 多会话复用 (13 tasks + D4 resolved)
- Phase AK: Server 安全加固 (7 tasks)
- Phase AL: Web 前端完善 (7 tasks)
- Phase AM: 可观测性 (6 tasks)
- Phase AO: octo-server 功能完善 (10 tasks + 2 stubs)
- Phase AP: 追赶 CC-OSS (18 tasks + 4 deferred resolved)
- Phase AQ: 自主能力 + 智能交互 (6 tasks + integration wiring)
- **Phase AR: CC-OSS 缺口补齐 (7 tasks, 3 waves) @ beb741b**

### 最新提交
```
12e6a20 docs: Phase AR complete — checkpoint + memory index updated
beb741b feat(engine): Phase AR — CC-OSS gap closure (7 tasks, 3 waves, ~660 lines)
c0d3b4e checkpoint: save Phase AR design checkpoint
e0de3c2 docs: Phase AR design — CC-OSS gap closure (7 tasks, 3 waves, ~660 lines)
```

### 测试基线
- 2476+ tests passing（建议跑全量确认：`cargo test --workspace -- --test-threads=1`）
- DB Version: 13
- 新增 29 个测试覆盖 Phase AR 功能

---

## 下一步优先级

1. **功能整合测试** — 验证 TokenEscalation、TranscriptWriter、BlobGc 在实际 LLM 交互中的端到端行为
2. **前端跟进** — Fork/Rewind UI 组件（AR-D2），MCP 工具面板增强
3. **部署管道** — CI/CD pipeline，容器镜像发布
4. **平台分支** — octo-platform-server 多租户功能

---

## ⚠️ Deferred 未清项（下次 session 启动时必查）

> 以下暂缓项来自近期阶段计划，前置条件尚未满足。

| 来源计划 | ID | 内容 | 前置条件 |
|---------|----|----|---------|
| Phase AR | AR-D1 | TranscriptWriter 压缩归档（gzip 老 transcript） | T2 完成 + 存储策略确定 |
| Phase AR | AR-D2 | Fork API 前端 UI（分支可视化） | T4 完成 + 前端 thread 组件 |
| Phase AR | AR-D3 | TriggerSource Redis/NATS 具体实现 | T6 完成 + 消息队列部署 |
| Phase AR | AR-D4 | 语义搜索 index 持久化（避免每次重建） | T7 完成 + index 序列化 |

---

## 关键代码路径

| 模块 | 路径 |
|------|------|
| Agent Harness | `crates/octo-engine/src/agent/harness.rs` |
| Agent Executor | `crates/octo-engine/src/agent/executor.rs` |
| TokenEscalation | `crates/octo-engine/src/agent/token_escalation.rs` |
| TranscriptWriter | `crates/octo-engine/src/session/transcript.rs` |
| BlobGc | `crates/octo-engine/src/storage/blob_gc.rs` |
| Autonomous Trigger | `crates/octo-engine/src/agent/autonomous_trigger.rs` |
| Tool Search (hybrid) | `crates/octo-engine/src/tools/tool_search.rs` |
| Session API (fork/rewind) | `crates/octo-server/src/api/sessions.rs` |
| Autonomous API (webhook) | `crates/octo-server/src/api/autonomous.rs` |
