---
id: ADR-V2-020
title: "工具命名空间契约（L0/L1/L2 分层）"
type: contract
status: Accepted
date: 2026-04-18
phase: "Phase 3 — L1 Runtime Functional Completeness"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: contract-test
  trace:
    - "tests/contract/cases/test_tool_namespace_enforcement.py"
    - "tests/contract/cases/test_tool_conflict_resolution.py"
    - "tests/contract/cases/test_pre_phase3_skill_compat.py"
  review_checklist: null
affected_modules:
  - "crates/grid-engine/"
  - "crates/grid-runtime/"
  - "tools/eaasp-skill-registry/"
  - "examples/skills/"
  - "tests/contract/"
related: [ADR-V2-006, ADR-V2-016, ADR-V2-017]
---

# ADR-V2-020 — 工具命名空间契约（L0/L1/L2 分层）

**Status:** Accepted
**Date:** 2026-04-18
**Accepted:** 2026-04-18 (Phase 3 S1.T8 sign-off)
**Phase:** Phase 3 — L1 Runtime Functional Completeness
**Author:** Jiangwen Su
**Related:** ADR-V2-006（hook envelope），ADR-V2-017（L1 生态策略），ADR-V2-016（agent loop）

---

## Context / 背景

Phase 2.5 sign-off 后，工具过滤机制由 `EAASP_TOOL_FILTER` env 变量 + `executor.rs:378-400` 的 GUARD snapshot 实现。这个方案能运行，但暴露了 7 类结构债根因：

1. **命名空间混乱**：`memory_recall`（L1 内置）vs `memory_search`（L2 MCP）共享 flat namespace，skill 作者无法确定性控制 LLM 工具选择。
2. **env-driven 打补丁**：`EAASP_TOOL_FILTER=memory_search,...` 是运维配置而非 skill 语义声明，无法随 skill 一起版本化。
3. **解析碎片化**：executor.rs 的 `GUARD snapshot` 逻辑与 skill frontmatter 解析分离，工具集合定义散落在两处。
4. **冲突规则隐式**：L1 和 L2 同名工具（e.g. `memory_search`）的优先级由 runtime 启动配置而非 skill 声明决定。
5. **MCP 路由隐式**：executor 依赖 `tool_name` 前缀或 `ToolSource::Mcp` 字段推断路由，规则未文档化。
6. **L1 runtime 可移植性差**：goose/nanobot/pydantic-ai 等 L1 runtime 接入时，必须重实现相同的隐式路由逻辑。
7. **合约测试盲区**：`contract-v1.0.0` 无 namespace 断言，无法验证跨 runtime 的工具路由一致性。

---

## Decision / 决策

采用 **L0/L1/L2 三层分层命名空间**，配合 **skill 显式声明优先** 规则。

### §1 分层定义

```
L0 — runtime-core（gRPC 契约层，对 LLM 不可见）
  l0:lifecycle.initialize / .terminate / .snapshot
  l0:session.create / .destroy / .list
  l0:telemetry.*
  → 由 eaasp.runtime.v2 proto 直接调用，不经 LLM tool call

L1 — engine-builtin（runtime 内置，LLM 可见）
  l1:memory.recall / .timeline / .graph_search
  l1:filesystem.read / .write / .glob / .grep
  l1:bash.execute
  l1:agent.spawn / .query
  l1:web.search / .fetch
  l1:...（runtime 自定义 L1 工具）

L2 — MCP-provided（外部 MCP server 注册，LLM 可见）
  l2:memory.search / .read / .write_file / .confirm / .write_anchor
  l2:skill-registry.submit / .list / .fetch
  l2:{任意 MCP server}.{domain}.{action}
```

### §2 命名规则

格式：`{layer}:{domain}.{action}`

- `layer` ∈ {`l0`, `l1`, `l2`}（小写字母 + 数字）
- `domain` 用点连接表达嵌套（e.g. `memory.graph_search`）
- `action` 单一动词，snake_case

**合法示例**：`l1:bash.execute`，`l2:memory.search`，`l1:memory.recall`
**非法示例**：`l3:anything`（无效层），`L1:bash`（大写），`memory_search`（无前缀，视为 pre-Phase 3 兼容）

### §3 skill 显式声明优先

skill YAML frontmatter 的 `workflow.required_tools` 支持命名空间前缀：

```yaml
workflow:
  required_tools:
    - l2:memory.search       # 强制路由到 L2 MCP（即使 runtime 内置同名 L1 工具）
    - l1:filesystem.read     # 强制用 L1 内置
    - l2:skill-registry.submit
```

**冲突规则（优先级从高到低）**：
1. skill YAML 显式 `l{N}:name` 声明（最高优先级）
2. runtime 启动配置（DEPRECATED `EAASP_TOOL_FILTER` env）
3. runtime 内置默认（最低）

### §4 pre-Phase 3 skill 向后兼容

未声明前缀的 `required_tools` 条目（e.g. 旧 `memory_search`）按 **fallback 查找链** 解析：
1. 先查 L2（保留 Phase 2.5 MCP 优先行为）
2. 再查 L1
3. 再查 L0（通常不命中）
4. 未找到时记录 warn log，不 panic

### §5 ToolRegistry 实现规范

- `ToolRegistry` 内部以 `HashMap<(ToolLayer, String), Arc<dyn Tool>>` 双键存储
- `Tool` trait 新增 `fn layer(&self) -> ToolLayer`（默认 `ToolLayer::L1`，不破坏现有实现）
- 暴露 `resolve(full_name: &str) -> Option<Arc<dyn Tool>>`：解析 `"l1:bash.execute"` 形式
- 暴露 `resolve_with_fallback(name: &str) -> Option<Arc<dyn Tool>>`：无前缀 fallback 查找链

### §6 EAASP_TOOL_FILTER env 退役路径

- **Phase 3**：保留 env 读取，但 startup 时 emit `tracing::warn!("EAASP_TOOL_FILTER is deprecated; use skill YAML required_tools with namespace prefix instead")`
- **Phase 4**：彻底移除

---

## Consequences / 后果

### Positive

- skill 作者可系统性控制 LLM 工具选择，消除命名歧义
- L1 runtime 接入指南有明确合规标准（contract v1.1 namespace assertions）
- `executor.rs` filter 路径统一从 skill metadata 读取，消除两处维护点
- 合约测试 v1.1 可断言 namespace routing 正确性

### Negative

- 现有 `examples/skills/*/SKILL.md` 需全部更新 `required_tools` 前缀（S1.T4 实施）
- 所有 L1 runtime 实现者需支持 `resolve_with_fallback()`（合约 v1.1 纳入验证）

### Risks

- pre-Phase 3 skill fallback 链可能 resolve 到非预期 L1/L2 工具（特别是同名冲突场景），需合约测试覆盖

---

## Affected Modules / 影响范围

| Module | Impact |
|--------|--------|
| `crates/grid-engine/src/tools/traits.rs` | 加 `ToolLayer` enum + `Tool::layer()` 方法 |
| `crates/grid-engine/src/tools/mod.rs` | `ToolRegistry` 内部改为双键 Map + resolve/resolve_with_fallback |
| `crates/grid-engine/src/agent/executor.rs` | filter 路径改从 skill metadata 读（退役 env guard） |
| `crates/grid-runtime/src/harness.rs` | Initialize 传 skill metadata（含 resolved required_tools） |
| `tools/eaasp-skill-registry/src/skill_parser.rs` | `WorkflowMetadata.required_tools` 从 `Vec<String>` 升级为 `Vec<RequiredTool>`（layer + name） |
| `examples/skills/*/SKILL.md` | required_tools 全部加 namespace 前缀 |
| `tests/contract/` | v1.1 新增 namespace enforcement / conflict resolution / pre-phase3-compat 三个 case |

---

## Alternatives Considered / 候选方案

### Option A: MCP-First（全部工具走 MCP 注册）

所有工具（包括 L1 内置）统一通过 MCP 协议暴露，runtime 不再区分内置 vs MCP。

**拒绝理由**：L1 内置工具（`bash.execute` / `filesystem.read` 等）有 <1ms latency 要求，走 MCP 引入 stdout JSON-RPC 序列化开销不可接受；部分 runtime（claude-code-runtime）内置工具由 Anthropic SDK 直接提供，无 MCP 替代路径。

### Option B: Filter-Only（保留 env + 改善文档）

不改 code，只改善 `EAASP_TOOL_FILTER` 文档和命名规范。

**拒绝理由**：env-driven 过滤是运维配置而非 skill 语义，无法随 skill 一起版本化；对 L1 runtime 可移植性无改善；contract v1.1 无法断言 namespace routing 正确性。

### Option C: Proto 扩展（在 runtime.Initialize 传工具层级）

在 `eaasp.runtime.v2.InitializeRequest` 中加 `tool_namespace` 字段，由 L4 orchestrator 决定。

**拒绝理由**：将 skill-level 声明下推到 gRPC message 会造成 L4 和 skill 重复维护工具集合，职责不清；proto 修改需 minor bump，不如 skill YAML 声明灵活。

---

## Migration / 迁移指南

### pre-Phase 3 skill 升级

```yaml
# Before (pre-Phase 3)
workflow:
  required_tools:
    - memory_search
    - memory_read
    - memory_write_anchor
    - memory_write_file

# After (Phase 3)
workflow:
  required_tools:
    - l2:memory.search
    - l2:memory.read
    - l2:memory.write_anchor
    - l2:memory.write_file
```

### L1 runtime 接入方义务

runtime `initialize()` 实现需：
1. 解析 skill metadata 的 `required_tools`（带或不带前缀）
2. 对带前缀条目调用 `resolve(full_name)`，对不带前缀条目调用 `resolve_with_fallback(name)`
3. 把解析出的工具集合注入 session 上下文（GUARD snapshot）
4. 合约 v1.1 `test_tool_namespace_enforcement` 和 `test_conflict_resolution` 必须 PASS

---

## Open Questions / 遗留问题

见 `docs/design/EAASP/PHASE_3_DESIGN.md` §5（已解决，表中决策锁定）。

---

## Implementation Evidence / 实施证据

| Task | Commit | 内容 |
|------|--------|------|
| S1.T1 | `126be00` | ADR-V2-020 Proposed + Phase 3 设计文件 |
| S1.T2 | session | `ToolLayer` enum + `Tool::layer()` trait 方法 |
| S1.T3 | session | `ToolRegistry::register_layered / resolve / resolve_with_fallback` |
| S1.T4 | session | `crates/grid-engine/tests/tool_namespace_test.rs` — 10 tests PASS |
| S1.T5 | session | `crates/grid-runtime/src/harness.rs` — skill-declared filter 优先于 env |
| S1.T5 | session | `tools/eaasp-skill-registry/src/skill_parser.rs` — `RequiredTool` + `WorkflowMetadata` 升级 |
| S1.T5 | session | `examples/skills/*/SKILL.md` — `required_tools` 升级为命名空间格式 |
| S1.T6 | `7bb59fb` | contract-v1.1.0 — 23 cases PASS (tests/contract/cases/) |
| S1.T7 | `67f6206` | L1_RUNTIME_ADAPTATION_GUIDE.md §10 命名空间治理章节 |
| S1.T8 | this | ADR 状态升为 Accepted |

验证命令：
```bash
cargo test -p grid-engine          # 2363 PASS
cargo test -p eaasp-skill-registry # 24 PASS
pytest tests/contract/cases/ -q    # 23 PASS
```

---

## References / 参考

- `docs/plans/2026-04-18-v2-phase3-CONTEXT.md` §Decisions 1-2
- `docs/design/EAASP/PHASE_3_DESIGN.md` §3.1
- `ADR-V2-017` — L1 生态策略（三轨规划）
- `ADR-V2-006` — hook envelope 契约
- `crates/grid-engine/src/agent/executor.rs` L378-400+（待退役的 GUARD snapshot）
