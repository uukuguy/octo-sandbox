# Phase B — 端到端评估手册实施计划

**日期**: 2026-03-13
**前置**: Phase A COMPLETE (33 assessment tests, 1807 total)
**目标**: 设计 12 个标准化端到端评估任务 + 评估结果记录模板

---

## 总体目标

根据评估设计文档（docs/design/AGENT_EVALUATION_DESIGN.md）第七节：

> Phase B — 轨道 B 评估手册（预计 1-2 天）
> - 10-15 个预定义编程任务（难度分级）
> - 每个任务：前置条件、执行命令、预期输出、评判标准
> - 评估结果记录模板

---

## 任务分解

### Group A — 评估手册核心（并行, P0）

| ID | 任务 | 产出文件 | 依赖 |
|----|------|---------|------|
| T1 | CLI 端评估任务集（6 个任务） | `docs/design/EVAL_HANDBOOK_CLI.md` | 无 |
| T2 | Server 端评估任务集（6 个任务） | `docs/design/EVAL_HANDBOOK_SERVER.md` | 无 |

### Group B — 评估基础设施（并行, P1）

| ID | 任务 | 产出文件 | 依赖 |
|----|------|---------|------|
| T3 | 评估结果记录模板 | `docs/design/EVAL_RESULT_TEMPLATE.md` | 无 |
| T4 | 评估脚本工具 | `scripts/eval/` 目录 | T1, T2 |

### Group C — 文档更新（串行, P2）

| ID | 任务 | 产出文件 | 依赖 |
|----|------|---------|------|
| T5 | 更新评估设计文档 Phase B 状态 | `docs/design/AGENT_EVALUATION_DESIGN.md` | T1-T4 |
| T6 | 更新 checkpoint + 全量验证 | `docs/plans/.checkpoint.json` | T5 |

---

## 评估任务设计概要

### CLI 端评估任务（T1, 6 个任务）

基于 `octo ask` 无头模式，可脚本化执行：

| # | 任务名 | 难度 | 评估维度 | 验证方式 |
|---|--------|------|---------|---------|
| C1 | 文件读写往返 | Easy | 工具调用精确度 | 检查文件内容一致 |
| C2 | Bash 命令执行 | Easy | 工具调用 + 输出解析 | 检查 stdout |
| C3 | 多步文件操作 | Medium | 多步推理 + 工具链 | 检查最终文件状态 |
| C4 | 代码生成与验证 | Medium | 代码生成能力 | 编译/运行验证 |
| C5 | 记忆存取一致性 | Medium | 记忆系统端到端 | 跨 session 检索 |
| C6 | MCP 工具发现与调用 | Hard | MCP 集成完整性 | 工具列表 + 调用结果 |

### Server 端评估任务（T2, 6 个任务）

基于 HTTP API + WebSocket，需启动 octo-server：

| # | 任务名 | 难度 | 评估维度 | 验证方式 |
|---|--------|------|---------|---------|
| S1 | Agent 生命周期管理 | Easy | API 正确性 | HTTP 状态码 + 响应体 |
| S2 | WebSocket 流式对话 | Medium | 流式输出完整性 | Event 序列校验 |
| S3 | Session 持久化与恢复 | Medium | 会话连续性 | 消息历史比对 |
| S4 | Token Budget 监控 | Medium | Context 管理 | Budget 快照变化 |
| S5 | Provider Chain 故障模拟 | Hard | 容错能力 | Failover 事件检测 |
| S6 | 审计日志完整性 | Easy | 可观测性 | Audit 记录比对 |

---

## 执行策略

- **执行模式**: subagent-driven-development
- **并行度**: Group A 两个任务可并行（T1 || T2），Group B 可并行（T3 || T4）
- **验证**: 文档审查 + 脚本可执行性验证
- **总任务数**: 6
- **预计产出**: 3 个评估手册文档 + 1 个脚本目录
