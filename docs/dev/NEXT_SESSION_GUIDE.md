# octo-sandbox 下一会话指南

**最后更新**: 2026-04-02 21:30 GMT+8
**当前分支**: `main`
**当前状态**: Phase AT 完成，无活跃 Phase

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
- **Phase AT: 提示词体系增强 + 编译优化 (T1-T5) @ c1fad3d**

### 最新提交
```
c1fad3d feat(engine): Phase AT — prompt system enhancement + build optimization
886d1b3 perf(runtime): parallelize MCP server connections for instant TUI startup
a30617a docs: update WORK_LOG with Phase AS deferred resolution
6acb2d1 feat(engine): Phase AS deferred — InteractionGate wiring, dead code cleanup, NotebookEdit, Zone B+ pinned
```

### 测试基线
- 2476+ tests passing（建议跑全量确认：`cargo test --workspace -- --test-threads=1`）
- DB Version: 13
- System prompt 相关 21 测试通过，memory injector 13 测试通过

---

## 下一步优先级

1. **Anthropic Prompt Caching API** — AT-D5: ApiRequest.system 改为数组格式 + cache_control 标记
2. **MCP Instructions 注入** — AT-D1: 从 rmcp InitializeResult 提取 server instructions
3. **前端跟进** — Fork/Rewind UI 组件（AR-D2），MCP 工具面板增强
4. **部署管道** — CI/CD pipeline，容器镜像发布
5. **平台分支** — octo-platform-server 多租户功能

---

## ⚠️ Deferred 未清项（下次 session 启动时必查）

> 以下暂缓项来自近期阶段计划，前置条件尚未满足。

| 来源计划 | ID | 内容 | 前置条件 |
|---------|----|----|---------|
| Phase AT | AT-D1 | MCP instructions 从 rmcp InitializeResult 提取 | rmcp 0.16 instructions 字段确认 |
| Phase AT | AT-D2 | SecurityPolicy 当前值动态注入 | SecurityPolicy 可序列化为人类可读文本 |
| Phase AT | AT-D3 | Coordinator prompt（多 agent 编排模式） | Coordinator 架构设计 |
| Phase AT | AT-D4 | 补全所有 memory/skill 工具的详细 description | 当前 9 个核心工具优先 |
| Phase AT | AT-D5 | Anthropic prompt caching API（cache_control） | ApiRequest.system 改为数组格式 |
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
