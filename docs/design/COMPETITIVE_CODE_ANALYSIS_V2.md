# 自主智能体框架代码实现级竞品分析（修正版 V2）

> 基于 8 个并行研究智能体对 octo-sandbox 及 3th-party/harnesses/ 下 10 个项目的**实际代码**深入分析。
> 分析日期：2026-03-12
> **V2 修正**：纠正 V1 中基于关键词匹配的误判，所有结论基于代码功能等价性。

---

## 一、V1 报告重大误判纠正

| V1 结论 | 事实 | 代码证据 |
|---------|------|----------|
| "无死循环检测" | **LoopGuard 是所有竞品中最完整的实现**：877行，SHA-256 哈希、乒乓检测、outcome-aware、poll 宽容、渐进升级 | `agent/loop_guard.rs` |
| "Provider 覆盖 1/5，仅 2 个" | **OpenAIProvider.with_base_url() 可接入任何 OpenAI-compatible 服务**（Ollama/Azure/通义/智谱/DeepSeek 等），ProviderChain 支持 per-instance base_url | `providers/openai.rs:29-52`, `providers/config.rs:48-58` |
| "OS 级沙箱缺失" | Docker 已含 seccomp profiles；WASM 提供确定性隔离；zeroclaw 的 Landlock 实现实际返回 Unsupported | `sandbox/router.rs`, `sandbox/docker.rs`, `sandbox/wasm.rs` |
| "缺少 taint tracking" | **已实现**：TaintLabel (Public/Internal/Confidential/Secret) + TaintedValue + zeroize-on-drop | `secret/taint.rs`, `secret/vault.rs` |

---

## 二、修正后综合评分

### 加权总分（1-10 分，基于代码实现深度）

| 维度 | octo | goose | ironclaw | moltis | openfang | localgpt | 权重 |
|------|------|-------|----------|--------|----------|----------|------|
| Agent 架构 | **9.0** | 6.0 | 8.5 | 5.0 | 7.0 | 4.0 | 15% |
| Provider 层 | **8.4** | 7.7 | 6.0 | 7.0 | 6.2 | 4.0 | 10% |
| Tool 系统 | 6.8 | 5.0 | **7.5** | 6.0 | 6.5 | 4.5 | 10% |
| MCP 集成 | 6.4 | **6.7** | 4.8 | 6.6 | 2.4 | 3.0 | 10% |
| Memory 系统 | **6.4** | 0.8 | 5.0 | 5.0 | 5.3 | 6.3 | 15% |
| Context Eng. | **7.5** | 4.3 | 5.0 | 3.0 | 7.1 | 3.0 | 15% |
| 安全模型 | **8.5** | 5.0 | 6.0 | 5.5 | 4.0 | 7.0 | 15% |
| 多租户 | **6.9** | 0.5 | 0.5 | 1.9 | 3.1 | 0.5 | 10% |
| **加权总分** | **7.55** | 4.38 | 5.41 | 4.69 | 5.01 | 3.94 | 100% |

### octo-sandbox 排名：**第一**（7.55 / 10）

---

## 三、octo-sandbox 独有优势（竞品均无）

### Agent 架构
1. **四层分离**（Runtime → Executor → Harness → Steps）— 最清晰的分层设计
2. **纯函数 Harness** 返回 `BoxStream<AgentEvent>` — 完全解耦，可测试
3. **LoopGuard 877行**（乒乓检测 + outcome-aware + poll 宽容 + 渐进升级）— 所有竞品中最完整
4. **MCP 热插拔** — 每轮从共享 `Arc<StdMutex<ToolRegistry>>` 做快照

### Provider 层
5. **装饰器管线** Pipeline（CircuitBreaker → CostGuard → ResponseCache → UsageRecorder → Metering）— 竞品均无
6. **SmartRouter V2** 按复杂度分层路由 — 竞品均无
7. **Thinking/Reasoning 三字段兼容**（reasoning_content + thinking + reasoning + content array blocks）— 覆盖面最广

### 安全模型
8. **可组合 SafetyPipeline**（InjectionDetector → PiiScanner → CanaryGuard → CredentialScrubber，短路语义）— 唯一管线化设计
9. **Taint Tracking + Zeroize**（4 级标签 + 自动清零）— 竞品中仅 octo 有完整实现
10. **PII 中国场景**（手机号 + 身份证号模式）— 独有

### Context Engineering
11. **ContextFork**（隔离 skill 执行上下文）— 独有
12. **ObservationMasker**（非破坏性 turn-based 工具结果隐藏）— 独有
13. **MemoryFlusher**（compaction 前 LLM 事实提取）— 独有
14. **CJK-Aware 计数**（ASCII 0.25 vs CJK 0.67 tokens/char）— 独有
15. **6 级渐进降级**（None → SoftTrim → AutoCompaction → Overflow → Truncation → Error）— 最多级别

### Tool 系统
16. **Head67Tail27 截断策略** — 所有竞品均无
17. **Skill 系统 20 子模块**（语义索引、依赖图、信任管理、多运行时）— 最完整

### 多租户
18. **三级 Agent 池隔离策略**（519 行 agent_pool.rs）— 竞品中仅 octo 有

---

## 四、真实差距（按优先级排序）

### P0 — 企业落地阻塞项

| # | 差距 | 借鉴对象 | 具体方案 | 预估工作量 |
|---|------|----------|----------|-----------|
| 1 | **自修复系统** — 卡住检测 + broken tool 重建 | ironclaw SelfRepair | 新建 `agent/self_repair.rs`：StuckJob 检测 + BrokenTool 重建 + RepairResult 枚举 | 2-3 天 |
| 2 | **上下文 compaction 三策略** — 仅裁剪，无 LLM 摘要 | ironclaw ContextMonitor | 扩展 `context/pruner.rs`：MoveToWorkspace / Summarize / Truncate 三策略 | 2 天 |
| 3 | **文本工具调用恢复** — LLM 输出文本格式工具调用时无 fallback | moltis, openfang | 在 `harness.rs` 增加 `parse_tool_calls_from_text()` 正则/JSON 解析 | 0.5 天 |
| 4 | **紧急停止 (E-Stop)** — 无一键终止所有 agent 的机制 | zeroclaw estop.rs | 新建 `agent/estop.rs`：全局 AtomicBool + broadcast 通知 | 1 天 |
| 5 | **Prompt Cache 优化** — 动态内容在 system prompt 中破坏缓存 | openfang | 将 `build_dynamic_context()` 输出从 system prompt 移到 user message | 0.5 天 |

### P1 — 显著提升竞争力

| # | 差距 | 借鉴对象 | 具体方案 | 预估工作量 |
|---|------|----------|----------|-----------|
| 6 | **MCP OAuth 2.1** — 无法连接认证 MCP Server | moltis (RFC 9728/8414) | 实现 PKCE + token refresh + path-aware discovery | 3-4 天 |
| 7 | **LLM Reranking** — 记忆搜索无二次排序 | moltis retrieve-then-rerank | 在 HybridQueryEngine 增加可选 LLM reranking 层 | 1-2 天 |
| 8 | **Session 三层模型** — 扁平 session，无 Thread/Turn 粒度 | ironclaw Session→Thread→Turn | 扩展 `session/` 增加 Thread/Turn + Undo | 2-3 天 |
| 9 | **Provider name 映射表** — 用户需手动写 base_url | openfang provider_defaults | 增加 ~30 个常见 provider 的 name→base_url 映射 | 0.5 天 |
| 10 | **结构化 API 重试** — 缺少 Retry-After header 解析 | moltis | 增强 `providers/retry.rs`：解析 HTTP header + 区分 billing 错误 | 0.5 天 |
| 11 | **Tool trait 增强** — 缺 execution_timeout/rate_limit/sensitive_params | ironclaw | 扩展 Tool trait 增加 3 个方法 | 1 天 |
| 12 | **KnowledgeGraph 工具暴露** — KG 模块存在但 LLM 无法调用 | openfang | 新建 `tools/knowledge_graph.rs`：graph_query/graph_add/graph_relate | 1 天 |
| 13 | **动态工具结果预算** — 固定 8000 字符截断 | openfang 30% context_window | 使截断限制与 context_window 成比例 | ~30 行 |
| 14 | **rmcp 升级** — 0.16 → 1.1，支持 StreamableHTTP | goose | 依赖升级 + API 适配 | 1-2 天 |

### P2 — 锦上添花

| # | 差距 | 借鉴对象 | 具体方案 | 预估工作量 |
|---|------|----------|----------|-----------|
| 15 | FTS+Vector 融合用 RRF 替代硬编码权重 | moltis | 改 HybridQueryEngine 为 Reciprocal Rank Fusion | 0.5 天 |
| 16 | Merkle 链审计防篡改 | openfang | AuditRecord 增加 prev_hash/hash 字段 | 0.5 天 |
| 17 | 消息优先级队列（steering 插队） | pi_agent_rust | executor.rs 增加 PriorityMailbox | 0.5 天 |
| 18 | 计量持久化 + 模型定价表 | openfang MeteringEngine | metering/ 增加 SQLite 存储和 ~40 模型定价 | 1 天 |
| 19 | Per-turn canary rotation | zeroclaw | CanaryGuardLayer 增加轮次旋转 | 0.5 天 |
| 20 | MCP Server 角色 | goose | 暴露 octo 工具为 MCP server | 2-3 天 |
| 21 | 图片 token 估算修正 | — | 使用固定值替代 base64_length/4 | ~40 行 |
| 22 | 工具执行进度事件 (ToolProgress) | pi_agent_rust | events.rs 增加 ToolProgress variant | 0.5 天 |
| 23 | 结构化工具 schema token 建模 | goose | 参考 FUNC_INIT 7 + PROP_KEY 3 精确估算 | 1 天 |

---

## 五、V1 伪差距确认清单

以下是 V1 报告中标为"差距"但实际为**伪差距**的项目：

| V1 标记 | 实际情况 | 证据 |
|---------|---------|------|
| "无死循环检测" | LoopGuard 877 行，17 测试，功能超过所有竞品 | `agent/loop_guard.rs` |
| "Provider 仅 2 个，1/5 分" | with_base_url 覆盖无限 OpenAI-compatible 服务 | `providers/openai.rs:29` |
| "OS 级沙箱缺失" | Docker seccomp + WASM 确定性隔离，zeroclaw Landlock 返回 Unsupported | `sandbox/docker.rs`, `sandbox/wasm.rs` |
| "缺少 taint tracking" | TaintLabel 4 级 + TaintedValue + zeroize-on-drop | `secret/taint.rs` |
| openfang LoopGuard 不同 | 与 octo-sandbox 同源，代码几乎一致 | 对比 loop_guard.rs |
| ironclaw AgentDeps 优于 AgentLoopConfig | 功能等价的 DI 容器，形式不同 | `agent/loop_config.rs` |
| goose CancellationToken | octo-sandbox 已有完全相同的实现 | `agent/loop_.rs`, `agent/harness.rs` |

---

## 六、战略定位总结

### octo-sandbox 的护城河

1. **架构深度** — 四层分离 + 纯函数 harness + 装饰器管线，是唯一具备企业级架构抽象的项目
2. **安全纵深** — 可组合 SafetyPipeline + Taint Tracking + PII/Canary/SSRF，安全层数最多
3. **多租户唯一性** — 三级 Agent 池隔离 + JWT + RBAC + 配额，竞品几乎为零
4. **Context 精细度** — 6 级降级 + ContextFork + ObservationMasker + CJK-aware，独有能力最多

### 补齐 P0 后的预期评分

| 维度 | 当前 | 补齐 P0 后 |
|------|------|-----------|
| Agent 架构 | 9.0 | **9.5** (自修复 + text tool recovery) |
| Context Eng. | 7.5 | **8.5** (compaction 三策略 + prompt cache) |
| 安全模型 | 8.5 | **9.0** (E-Stop) |
| **加权总分** | 7.55 | **8.1** |

### 与竞品的关系

| 竞品 | 定位 | 与 octo 的差异化 |
|------|------|-----------------|
| ironclaw | 最成熟的单用户 Agent | 会话模型 + 自修复更深，但无多租户、无安全管线 |
| moltis | 多渠道 Agent 平台 | Provider + API 重试更成熟，但架构扁平、无 Context 精细控制 |
| openfang | Agent 操作系统雏形 | Agent 间通信丰富，但 Kernel god-object 反模式、安全薄弱 |
| goose | MCP-native Agent | MCP 集成最新，但无记忆、无安全、无多租户 |
| localgpt | 本地安全 Agent | 记忆系统接近 octo 水平，但架构简单、无平台能力 |

---

## 七、增强路线图建议

### Wave 7: 运行时防护强化（P0，~6 天）

```
T1: 自修复系统 (self_repair.rs)                    — 2-3 天
T2: 上下文 compaction 三策略 (pruner.rs)             — 2 天
T3: 文本工具调用恢复 (harness.rs)                    — 0.5 天
T4: 紧急停止 E-Stop (estop.rs)                      — 1 天
T5: Prompt Cache 优化 (system_prompt_builder.rs)     — 0.5 天
```

### Wave 8: 集成与记忆增强（P1，~10 天）

```
T1: MCP OAuth 2.1                                   — 3-4 天
T2: LLM Reranking                                   — 1-2 天
T3: Session Thread/Turn 模型                         — 2-3 天
T4: Provider name 映射表                             — 0.5 天
T5: 结构化 API 重试增强                              — 0.5 天
T6: Tool trait 增强 (timeout/rate_limit/sensitive)    — 1 天
T7: KnowledgeGraph 工具暴露                          — 1 天
T8: 动态工具结果预算                                  — 0.5 天
```

### Wave 9: 精细优化（P2，按需选择）

```
FTS+Vector RRF 融合、Merkle 审计、消息优先级队列、
计量持久化、Per-turn canary、MCP Server 角色、
图片 token 修正、ToolProgress 事件、Schema token 建模
```
