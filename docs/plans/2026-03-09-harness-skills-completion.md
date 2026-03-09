# Harness & Skills 设计完整实现计划

> 创建日期: 2026-03-09
> 基于: AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md + AGENT_SKILLS_BEST_IMPLEMENTATION_DESIGN.md
> 前置: harness-implementation 计划（28/28 已完成，872 tests）
> 当前基线: commit 9ada808, 872 tests passing

---

## 一、目标

将 Harness 和 Skills 两份设计文档的剩余未实现项全部落地。基于代码现实做了以下设计调整：

### 设计调整摘要

| 编号 | 调整项 | 原设计 | 调整后 |
|------|--------|--------|--------|
| A1 | ToolResult→ToolOutput | harness 层转换（渐进迁移） | **直接改 Tool trait 签名，删除 ToolResult** |
| A2 | OctoEvent 命名 | 保持 OctoEvent | **重命名为 TelemetryEvent，EventBus→TelemetryBus** |
| A3 | 事件系统 | EventStore 持久化 AgentEvent | **保持双轨制：AgentEvent 实时流 + TelemetryEvent 持久化** |
| A4 | SkillTool 自动激活 | 自动触发执行 | **注册制：SkillManager→ToolRegistry 注册，LLM 自行选择调用** |
| A5 | context-fork | 独立 ContextFork 机制 | **复用 SubAgent 工具实现 fork 语义** |
| A6 | Provider embed() | 装饰器链包装所有方法 | **embed() 直通，不进装饰器链** |
| A7 | SmartRouting | P1 优先 | **降级到 P3，短期 ROI 低** |
| A8 | ToolProgress | P1 优先 | **降级到 P2，改 trait 签名影响面大** |

---

## 二、阶段划分

### Phase 1: 类型统一与命名清理（基础设施）

解决技术债，为后续功能打基础。

| 编号 | 任务 | 涉及文件 | 复杂度 | 依赖 |
|------|------|---------|--------|------|
| T1-1 | **Tool trait execute() 返回值改为 Result\<ToolOutput\>** | `tools/traits.rs` | 低 | 无 |
| T1-2 | **批量迁移内置工具返回 ToolOutput** | `tools/bash.rs`, `file_read.rs`, `file_write.rs`, `file_edit.rs`, `grep.rs`, `glob.rs`, `memory_*.rs` 等 20+ 文件 | 中（量大但机械） | T1-1 |
| T1-3 | **迁移 MCP/Skill/SubAgent 工具返回 ToolOutput** | `mcp/tool_bridge.rs`, `skills/tool.rs`, `tools/subagent.rs` | 低 | T1-1 |
| T1-4 | **harness.rs 中使用 ToolOutput 的增强字段** | `agent/harness.rs` — 截断处理、duration 记录、metadata 注入 | 中 | T1-2 |
| T1-5 | **删除 ToolResult 类型** | `octo-types/src/tool.rs`, 删除 From impl | 低 | T1-2, T1-3 |
| T1-6 | **更新所有测试中的 ToolResult 引用** | `tests/` 目录下所有引用 ToolResult 的测试 | 中（量大但机械） | T1-5 |
| T1-7 | **OctoEvent → TelemetryEvent 重命名** | `event/bus.rs`, `event/mod.rs`, `lib.rs` | 低 | 无 |
| T1-8 | **EventBus → TelemetryBus 重命名** | `event/bus.rs`, `event/mod.rs`, `lib.rs`, `agent/harness.rs` | 低 | T1-7 |
| T1-9 | **更新所有 OctoEvent/EventBus 引用** | harness.rs, executor.rs, ws.rs, 测试文件 | 中 | T1-7, T1-8 |
| T1-10 | **更新设计文档中的类型名** | `docs/design/AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md` | 低 | T1-5, T1-9 |

**验收标准**: cargo test --workspace 全部通过，ToolResult 零引用，OctoEvent 零引用

---

### Phase 2: Skills ↔ AgentLoop 集成（核心断联修复）

打通 Skills 系统与 Agent 核心循环的所有集成点。

| 编号 | 任务 | 涉及文件 | 复杂度 | 依赖 |
|------|------|---------|--------|------|
| T2-1 | **SkillManager → ToolRegistry 注册桥接** | 新增 `skills/registry_bridge.rs` 或在 `AgentRuntime` 初始化时注册 | 中 | Phase 1 |
| T2-2 | **SystemPromptBuilder.with_skill_index()** — L1 技能索引注入 system prompt | `context/system_prompt.rs` | 中 | 无 |
| T2-3 | **SystemPromptBuilder.with_active_skill()** — L2 激活技能 body 注入 | `context/system_prompt.rs` | 低 | T2-2 |
| T2-4 | **harness.rs 构建 system prompt 时集成 SkillManager** | `agent/harness.rs` (build_context 阶段) | 中 | T2-2, T2-3 |
| T2-5 | **ContextPruner 集成 always 标记豁免** | `context/pruner.rs` — 标记 skill-injected 消息为 protected，pruning 时跳过 | 中 | T2-4 |
| T2-6 | **Symlink 防护增强** — canonicalize() + 路径验证 | `skills/loader.rs` | 低 | 无 |
| T2-7 | **SkillTool context-fork 通过 SubAgent 实现** | `skills/tool.rs` — context_fork=true 时调用 spawn_subagent | 中 | T2-1 |
| T2-8 | **SkillTool model 覆盖集成** | `skills/tool.rs` + `agent/harness.rs` — 激活 skill 时临时切换 provider | 中 | T2-1 |
| T2-9 | **LRU 缓存 — L2 加载结果缓存** | `skills/manager.rs` — 用 `lru` crate 缓存已加载 skill body | 低 | 无 |
| T2-10 | **MCP 工具支持 allowed-tools** | `skills/trust.rs` — 识别 `mcp__server__tool` 格式，通配符匹配 | 低 | 无 |

**验收标准**: Skills 自动注册为 Tool，LLM 可选择调用；always skill 在 compact 模式下保留；context-fork 通过 SubAgent 执行

---

### Phase 3: Harness 安全与审批（安全闭环）

完成工具审批系统的端到端集成。

| 编号 | 任务 | 涉及文件 | 复杂度 | 依赖 |
|------|------|---------|--------|------|
| T3-1 | **ApprovalManager 集成到 harness 工具执行路径** | `agent/harness.rs` — PreToolUse → Approval → execute | 中 | Phase 1 |
| T3-2 | **AgentEvent::ApprovalRequired 增加 oneshot::Sender** | `agent/events.rs` — 让 WS handler 回调审批结果 | 中 | T3-1 |
| T3-3 | **WS handler 处理 ApprovalRequired 事件** | `octo-server/src/ws.rs` — 接收前端审批，回调 oneshot | 中 | T3-2 |
| T3-4 | **Tool Approval 三级强制执行** | `agent/harness.rs` — 根据 tool.approval() 决定是否需要审批 | 低 | T3-1 |
| T3-5 | **SafetyLayer 子类实现 — InjectionDetector** | 新文件 `security/injection_detector.rs` | 中 | 无 |
| T3-6 | **SafetyLayer 子类实现 — PiiScanner** | 新文件 `security/pii_scanner.rs` | 中 | 无 |
| T3-7 | **SafetyLayer 子类实现 — CanaryGuard** | 新文件 `security/canary_guard.rs` | 低 | 无 |
| T3-8 | **SafetyPipeline 集成到 harness** | `agent/harness.rs` — 在 LLM 输入/输出/工具结果处调用 pipeline.check() | 中 | T3-5, T3-6, T3-7 |

**验收标准**: 高危工具执行前发出审批事件，WS 可回调；SafetyPipeline 在 harness 中实际运行

---

### Phase 4: Provider Pipeline 完善与 REST API

补齐 Provider 装饰器链和 Skills REST API。

| 编号 | 任务 | 涉及文件 | 复杂度 | 依赖 |
|------|------|---------|--------|------|
| T4-1 | **ResponseCache 装饰器** | `providers/pipeline.rs` — 缓存相同请求的响应 | 中 | 无 |
| T4-2 | **UsageRecorder 装饰器** | `providers/pipeline.rs` — 记录所有请求的 token usage | 低 | 无 |
| T4-3 | **Tiktoken 默认启用** | `Cargo.toml` 启用 tiktoken feature；`context/manager.rs` 默认用 TiktokenCounter | 低 | 无 |
| T4-4 | **Skills REST API — execute 端点实现** | `octo-server/src/api/skills.rs` — 调用 SkillManager.execute_script() | 中 | Phase 2 |
| T4-5 | **Skills REST API — delete 端点实现** | `octo-server/src/api/skills.rs` | 低 | 无 |
| T4-6 | **更新设计文档 — 实现状态同步** | 两份设计文档中所有 ⏳/🟡 标记更新为 ✅ | 低 | Phase 3 |

**验收标准**: Provider pipeline 支持缓存和记录；Skills REST API 全功能可用；设计文档状态同步

---

### Phase 5（远期）: 高级特性

| 编号 | 任务 | 复杂度 | 说明 |
|------|------|--------|------|
| T5-1 | SmartRouting — 查询复杂度分类器 | 高 | 需要训练/规则引擎 |
| T5-2 | ToolProgress trait — 实时进度回调 | 中 | 改 Tool trait 签名 |
| T5-3 | TelemetryEvent 扩展 — 更多遥测变体 | 低 | 按需扩展 |
| T5-4 | 远程 Skill Registry | 高 | 生态功能 |
| T5-5 | Skill 签名验证 | 中 | 供应链安全 |

---

## 三、任务总览

| Phase | 名称 | 任务数 | 关键交付物 |
|-------|------|--------|-----------|
| **Phase 1** | 类型统一与命名清理 | 10 | ToolOutput 统一，TelemetryEvent 命名 |
| **Phase 2** | Skills ↔ AgentLoop 集成 | 10 | Skills 注册 Tool、Context 集成、安全增强 |
| **Phase 3** | 安全与审批 | 8 | ApprovalManager 端到端、SafetyPipeline |
| **Phase 4** | Pipeline 完善与 API | 6 | Provider 缓存/记录、REST API、文档 |
| **Phase 5** | 远期高级特性 | 5 | SmartRouting、ToolProgress、Registry |
| **合计** | | **39** (P1-P4: 34) | |

---

## 四、关键依赖关系

```
Phase 1 (类型统一)
  ├─ T1-1..T1-6: ToolOutput 迁移链
  │   T1-1 (trait) → T1-2 (内置工具) → T1-3 (MCP/Skill) → T1-4 (harness) → T1-5 (删除) → T1-6 (测试)
  └─ T1-7..T1-9: TelemetryEvent 重命名链（可与 ToolOutput 并行）
      T1-7 (enum) → T1-8 (bus) → T1-9 (引用)

Phase 2 (Skills 集成) — 依赖 Phase 1 完成
  ├─ T2-1 (注册桥接) → T2-7 (context-fork) / T2-8 (model 覆盖)
  ├─ T2-2 → T2-3 → T2-4 (SystemPrompt 集成链)
  ├─ T2-4 → T2-5 (ContextPruner always 豁免)
  └─ T2-6, T2-9, T2-10 (独立，可并行)

Phase 3 (安全审批) — 依赖 Phase 1 完成
  ├─ T3-1 → T3-2 → T3-3 → T3-4 (审批链)
  └─ T3-5, T3-6, T3-7 → T3-8 (SafetyLayer 链)

Phase 4 — 依赖 Phase 2, Phase 3 完成
```

---

## 五、风险与缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| ToolOutput 迁移导致大量测试失败 | 高 | 中 | T1-2 用 sed 批量替换，每个文件改动极小 |
| ApprovalManager oneshot 阻塞导致 harness 死锁 | 中 | 高 | 设置审批超时（30s），超时自动拒绝 |
| SkillTool 注册到 ToolRegistry 后工具列表过长 | 低 | 低 | 仅注册 user-invocable=true 的 skill |
| Tiktoken 依赖增加编译时间 | 低 | 低 | 保留 feature flag，CI 默认启用 |

---

## 六、设计文档更新清单

完成实施后需同步更新：

| 文档 | 更新内容 |
|------|---------|
| `AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md` | ToolOutput 替代 ToolResult 的说明；TelemetryEvent 命名；实现状态标记更新 |
| `AGENT_SKILLS_BEST_IMPLEMENTATION_DESIGN.md` | SkillTool 注册制说明；context-fork 通过 SubAgent 实现；实现状态更新 |
| `CLAUDE.md` | 更新 Key Patterns 中的类型名（如有引用） |
