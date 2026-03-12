# Tool 系统代码级竞品深度分析

> 基于实际源码逐行对比，非文档推测。分析日期：2026-03-12

---

## 一、octo-sandbox 内置工具完整清单

### 1.1 默认注册工具（`default_tools()`）-- 9 个

| 工具名 | 文件 | RiskLevel | ApprovalRequirement | 说明 |
|--------|------|-----------|---------------------|------|
| `bash` | `bash.rs` | HighRisk | AutoApprovable | Shell 命令执行，含 ExecPolicy 白名单（Allowlist/Full/Deny 三模式） |
| `file_read` | `file_read.rs` | 默认 LowRisk | 默认 Never | 文件读取 |
| `file_write` | `file_write.rs` | 默认 LowRisk | 默认 Never | 文件写入 |
| `file_edit` | `file_edit.rs` | 默认 LowRisk | 默认 Never | 文件编辑（diff-based） |
| `grep` | `grep.rs` | 默认 LowRisk | 默认 Never | 内容搜索 |
| `glob` | `glob.rs` | 默认 LowRisk | 默认 Never | 文件模式匹配 |
| `find` | `find.rs` | 默认 LowRisk | 默认 Never | 文件查找 |
| `web_fetch` | `web_fetch.rs` | 默认 LowRisk | 默认 Never | URL 抓取 |
| `web_search` | `web_search.rs` | 默认 LowRisk | 默认 Never | 网页搜索 |

### 1.2 Memory 工具（需手动注册 `register_memory_tools()`）-- 5 个

| 工具名 | 文件 | 说明 |
|--------|------|------|
| `memory_store` | `memory_store.rs` | 存储记忆（需 Provider 做 embedding） |
| `memory_search` | `memory_search.rs` | 搜索记忆 |
| `memory_recall` | `memory_recall.rs` | 精确召回 |
| `memory_update` | `memory_update.rs` | 更新记忆 |
| `memory_forget` | `memory_forget.rs` | 删除记忆 |

### 1.3 SubAgent 工具（按需注册）-- 2 个

| 工具名 | 文件 | 说明 |
|--------|------|------|
| `spawn_subagent` | `subagent.rs` | 递归调用 `run_agent_loop()` 生成子 Agent |
| `query_subagent` | `subagent.rs` | 查询子 Agent 状态和结果 |

### 1.4 Skill 系统（动态桥接为工具）

通过 `register_skills_as_tools()` 将 YAML skill manifest 桥接为 Tool。20 个子模块：

- 核心：`SkillLoader`（42K 行 YAML 解析）, `SkillRegistry`, `SkillCatalog`, `SkillManager`
- 选择：`SkillSelector`（语义匹配）, `SkillSemanticIndex`, `SkillSlashRouter`
- 安全：`TrustManager`, `ToolConstraintEnforcer`, `standards`（结构校验）
- 运行时：`SkillRuntimeBridge` -> `PythonRuntime`, `NodeJsRuntime`, `ShellRuntime`, `WasmSkillRuntime`
- 依赖：`SkillDependencyGraph`, `ModelOverride`

### 1.5 基础设施

| 模块 | 关键设计 |
|------|---------|
| `traits.rs` | `risk_level() -> RiskLevel`（4 级：ReadOnly/LowRisk/HighRisk/Destructive）+ `approval() -> ApprovalRequirement`（3 级：Never/AutoApprovable/Always） |
| `truncation.rs` | `Head67Tail27`（保留头 67% + 尾 27%，中间插 omission marker）/ `HeadOnly` / `TailOnly`，50KB + 2000 行双重限制 |
| `approval.rs` | `ApprovalManager`（AlwaysApprove/SmartApprove/AlwaysAsk）+ `ApprovalGate`（oneshot channel 异步等待人工审批，30s 超时自动拒绝） |
| `parallel.rs` | `execute_parallel()`（Semaphore + join_all，可配置 max_parallel） |
| `recorder.rs` | `ToolExecutionRecorder`（执行日志和统计） |
| `interceptor.rs` | `ToolCallInterceptor`（工具调用拦截） |
| `path_safety.rs` | 路径遍历防护 |

**octo 内置工具总计：16 个**（9 默认 + 5 memory + 2 subagent）

---

## 二、竞品内置工具清单

### 2.1 ZeroClaw -- 约 70+ 内置工具

**Tool trait**（`src/tools/traits.rs`）：极简设计
```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult>;
    fn spec(&self) -> ToolSpec { ... }
}
```
无 risk_level、无 approval、无 ToolContext。安全通过外部 `SecurityPolicy` 在调用前注入。

| 类别 | 工具 | 数量 | 亮点 |
|------|------|------|------|
| 文件系统 | file_read, file_write, file_edit, glob_search, content_search | 5 | |
| Shell | shell, bg_run | 2 | bg_run 支持后台长时间执行 |
| Web | web_fetch, web_search_tool, browser, browser_open | 4 | |
| **文档解析** | **docx_read, pdf_read, pptx_read, xlsx_read** | **4** | **octo 无此类工具** |
| **截图** | **screenshot** | **1** | **macOS screencapture / Linux scrot，返回 base64** |
| 图片 | image_info | 1 | |
| Memory | memory_store, memory_recall, memory_forget, memory_observe | 4 | |
| **SubAgent** | subagent_spawn, subagent_manage, subagent_list, subagent_registry, delegate, delegate_coordination_status | **6** | 含委派协调状态 |
| **Agent IPC** | **agents_ipc** | **1(5)** | **SQLite 共享 DB：register/discover/send/recv/shared_state** |
| Cron | cron_add, cron_list, cron_remove, cron_run, cron_runs, cron_update, schedule | 7 | |
| **SOP 流程** | **sop_list, sop_execute, sop_advance, sop_approve, sop_status** | **5** | **标准操作流程引擎，octo 无** |
| Git | git_operations | 1 | |
| HTTP | http_request | 1 | |
| MCP | mcp_client, mcp_protocol, mcp_tool, mcp_transport | 4 | |
| WASM | wasm_module, wasm_tool | 2 | |
| Apply Patch | apply_patch | 1 | 多 hunk diff patch |
| 配置 | model_routing_config, web_access_config, web_search_config, proxy_config, channel_ack_config, orchestration_settings, quota_tools | 7 | |
| 认证 | auth_profile | 1 | |
| 任务 | task_plan | 1 | |
| 硬件 | hardware_board_info, hardware_memory_map, hardware_memory_read | 3 | feature-gated |
| 通知 | pushover | 1 | |
| 其他 | url_validation, cli_discovery, composio, agent_load_tracker, agent_selection, openclaw_migration | 6 | |

### 2.2 IronClaw -- 约 35+ 内置工具

**Tool trait**（`src/tools/tool.rs`）：**所有竞品中最丰富的 trait 设计**

```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, params: Value, ctx: &JobContext) -> Result<ToolOutput, ToolError>;

    // --- octo 没有的方法 ---
    fn estimated_cost(&self, _params: &Value) -> Option<Decimal> { None }
    fn estimated_duration(&self, _params: &Value) -> Option<Duration> { None }
    fn execution_timeout(&self) -> Duration { Duration::from_secs(60) }
    fn requires_sanitization(&self) -> bool { true }
    fn requires_approval(&self, _params: &Value) -> ApprovalRequirement { Never }  // 参数敏感！
    fn domain(&self) -> ToolDomain { Orchestrator }  // Orchestrator vs Container
    fn sensitive_params(&self) -> &[&str] { &[] }    // 自动脱敏
    fn rate_limit_config(&self) -> Option<ToolRateLimitConfig> { None }
}
```

关键差异点：
- `requires_approval(&self, _params: &Value)` -- **接受参数**，同一工具可根据参数内容动态决定是否需审批
- `domain()` -- 明确区分 Orchestrator（宿主进程）vs Container（Docker 容器）执行域
- `sensitive_params()` -- 返回需脱敏的参数名列表，框架自动在日志/审计/审批 UI 中替换为 `[REDACTED]`
- `rate_limit_config()` -- 每工具每用户的速率限制
- `estimated_cost()` / `estimated_duration()` -- 成本和耗时预估

另有 `ApprovalContext::Autonomous` 为无人值守场景专门设计。

| 类别 | 工具 | 数量 |
|------|------|------|
| 基础 | echo, time, json | 3 |
| 文件 | file_read, file_write, list_dir, apply_patch | 4 |
| Shell | shell（带环境变量清洗 + 敏感变量擦除） | 1 |
| Web | web_fetch（HTML->Markdown via readability）, http | 2 |
| Memory | memory_search, memory_write, memory_read, memory_tree | 4 |
| Job | create_job, list_jobs, job_status, cancel_job | 4 |
| Routine | routine_create/list/update/delete/history | 5 |
| Extension | extension_install/auth/activate/remove | 4 |
| Skill | skill_list/search/install/remove | 4 |
| Secret | secret_list, secret_delete（**零暴露**：不暴露值） | 2 |
| Message | message（跨 channel 主动推送） | 1 |
| Restart | restart | 1 |

另有完整的 WASM sandbox 工具运行时、MCP client、Dynamic tool builder 系统。

### 2.3 Moltis -- 约 30+ 内置工具

| 类别 | 工具 | 数量 | 亮点 |
|------|------|------|------|
| 执行 | exec | 1 | |
| **浏览器** | **browser** | **1** | **完整浏览器自动化** |
| 计算 | calc | 1 | |
| **会话通信** | **sessions_communicate (send/receive/broadcast), sessions_manage (list/kill), session_state, branch_session** | **6** | **Agent 间消息传递** |
| Cron | cron_tool | 1 | |
| 地理 | location, map | 2 | |
| 节点 | nodes (create/update/query) | 3 | |
| 进程 | process | 1 | |
| 沙箱 | sandbox, sandbox_packages | 2 | Docker + Apple Container |
| 图像 | send_image, image_cache | 2 | |
| Skill | skill_tools (list/search/install) | 3 | |
| Agent | spawn_agent（含 agent_query, agent_kill） | 3 | |
| 任务 | task_list | 1 | |
| Web | web_fetch, web_search | 2 | |
| WASM | embedded_wasm, wasm_component, wasm_engine, wasm_tool_runner | 4 | |
| 安全 | approval, policy, ssrf | 3 | 多层 allow/deny 策略 |

### 2.4 OpenFang -- 约 60 个内置工具

**非 trait 架构**：所有工具在 `tool_runner.rs` 中以 `ToolDefinition` struct 数组 + 巨型 `match` 分发。无法通过 trait 扩展。

| 类别 | 工具 | 数量 | 亮点 |
|------|------|------|------|
| 文件 | file_read, file_write, file_list, apply_patch | 4 | |
| Web | web_fetch（含 SSRF + taint tracking）, web_search（多 provider 自动 fallback） | 2 | |
| Shell | shell_exec（含 taint tracking：检测 `curl|sh`, `base64 -d`, `eval` 等注入模式） | 1 | |
| **Agent 通信** | **agent_send, agent_spawn, agent_list, agent_kill, agent_find** | **5** | **最完整的多 Agent 通信** |
| **共享内存** | **memory_store, memory_recall** | **2** | **跨 Agent 共享** |
| **任务队列** | **task_post, task_claim, task_complete, task_list** | **4** | **octo 无此能力** |
| **事件** | **event_publish** | **1** | **触发 proactive agents** |
| 调度 | schedule_create/list/delete | 3 | |
| **知识图谱** | **knowledge_add_entity, knowledge_add_relation, knowledge_query** | **3** | **LLM 可直接操作 KG** |
| 图像 | image_analyze（含 vision LLM 分析） | 1 | |
| 地理 | location_get | 1 | |
| **浏览器** | **browser_navigate/click/type/screenshot/read_page/close/scroll/wait/run_js/back** | **10** | **最完整的浏览器套件** |
| **媒体** | **media_describe（vision LLM）, media_transcribe（Whisper STT）** | **2** | **多模态理解** |
| **图像生成** | **image_generate（DALL-E 2/3/GPT-Image-1）** | **1** | |
| Cron | cron_create/list/cancel | 3 | |
| **Channel** | **channel_send（跨 email/telegram/slack/discord 主动推送）** | **1** | |
| **Hands** | **hand_list/activate/status/deactivate** | **4** | **策展式自主能力包** |
| **A2A** | **a2a_discover, a2a_send** | **2** | **Agent-to-Agent 跨实例通信** |
| **TTS/STT** | **text_to_speech, speech_to_text** | **2** | |
| Docker | docker_exec | 1 | |
| **持久进程** | **process_start/poll/write/kill/list** | **5** | **REPL/服务器生命周期管理** |
| 系统 | system_time | 1 | |
| **Canvas** | **canvas_present（HTML 可视化面板）** | **1** | |

### 2.5 Goose -- 纯 MCP 架构（0 内置工具）

Goose 的核心设计完全不内置工具。所有能力通过 MCP server 提供：
- `computercontroller` MCP server（web_scrape, automation_script, computer_control, screenshot, pdf/docx/xlsx 解析, cache）
- `memory` MCP server
- `peekaboo` MCP server（macOS 截图）

这是极端 MCP-first 架构，Agent 本体零工具。

---

## 三、维度对比分析

### 3.1 Tool Trait 设计对比

| 特性 | octo | ZeroClaw | IronClaw | Moltis | OpenFang |
|------|------|----------|----------|--------|----------|
| name/desc/params/execute | Y | Y | Y | Y | struct 定义 |
| **risk_level()** | **Y (4级)** | N | N | N | N |
| **approval()** | **Y (3级)** | N | **Y (3级+参数敏感)** | Y (policy) | N |
| execution_timeout() | N | N | **Y (默认60s)** | N | N |
| estimated_cost() | N | N | **Y (Decimal)** | N | N |
| estimated_duration() | N | N | **Y** | N | N |
| requires_sanitization() | N | N | **Y** | N | N |
| sensitive_params() 脱敏 | N | N | **Y** | N | N |
| rate_limit_config() | N | N | **Y** | N | N |
| domain() 执行域 | N | N | **Y (Orchestrator/Container)** | N | N |
| schema_validation() | N | N | **Y (lenient + strict)** | N | N |
| ToolContext 传入 | **Y** | N | **Y (JobContext)** | 闭包 | N |

**评价**：
- IronClaw 的 Tool trait 是行业标杆（9 个额外方法），octo 有正确的起点（risk_level + approval）但需要补齐 timeout/cost/sanitization/rate_limit
- IronClaw 的 `requires_approval(&self, _params: &Value)` 参数敏感审批是设计亮点
- octo 的 `ToolContext` 传入是正确做法（ZeroClaw 没有上下文传入）

### 3.2 输出截断对比

| 框架 | 截断机制 | 字节限制 | 行数限制 | 策略 |
|------|---------|---------|---------|------|
| **octo** | **truncation.rs** | **50KB** | **2000 行** | **Head67Tail27 / HeadOnly / TailOnly** |
| ZeroClaw | 无 | - | - | - |
| IronClaw | 输出净化 | - | - | sanitization only |
| Moltis | 无 | - | - | - |
| OpenFang | 无 | - | - | - |

**octo 的 Head67Tail27 截断策略是独有优势**。理由：头部通常包含摘要/结构/命令输出头部，尾部包含最终结果/错误码，中间是可丢弃的重复数据。这比简单的 head-only 截断对 LLM 更友好。**所有竞品均无类似机制。**

### 3.3 审批系统对比

| 特性 | octo | IronClaw | Moltis |
|------|------|----------|--------|
| 全局策略 | AlwaysApprove / SmartApprove / AlwaysAsk | - | policy.rs 多层 allow/deny |
| 三级审批 | Never / AutoApprovable / Always | Never / UnlessAutoApproved / Always | allow/deny 规则 |
| **参数敏感审批** | **N** | **Y** (`requires_approval(&params)`) | N |
| 异步等待人工 | **ApprovalGate**（oneshot + 30s timeout） | UI overlay 审批 | channel 审批 |
| **自主模式** | N | **ApprovalContext::Autonomous** | N |

octo 的 `ApprovalGate` 异步等待机制（oneshot channel + timeout 自动拒绝）是实用亮点。但缺少 IronClaw 的参数敏感审批和自主运行模式。

### 3.4 工具并行执行

octo 有 `parallel.rs`，用 `Semaphore + join_all` 实现可配置并发度的并行工具执行。这是正确且完整的实现，竞品中仅 ZeroClaw 和 pi_agent_rust 有类似能力。

### 3.5 Skill 系统对比

| 特性 | octo | IronClaw | Moltis |
|------|------|----------|--------|
| Manifest | YAML（42K 行 loader） | SKILL.md（YAML frontmatter + Markdown body） | YAML |
| 运行时 | **Python / NodeJS / Shell / WASM** | **Prompt 注入**（非代码执行） | WASM |
| 信任模型 | **TrustManager** | Trusted / Installed 两级 | 无 |
| 语义选择 | **SkillSemanticIndex** | 关键词/正则评分 | 无 |
| 依赖管理 | **SkillDependencyGraph** | 前置条件检查（bins/env） | 无 |
| 工具约束 | **ToolConstraintEnforcer** | Tool attenuation | 无 |
| 注册中心 | **SkillCatalog** | ClawHub registry | 无 |
| Slash 命令 | **SkillSlashRouter** | 无 | 无 |

**octo 的 Skill 系统是架构最完整的**（20 子模块）。IronClaw 的 SKILL.md 作为 prompt 注入方式更安全（不执行代码），但 octo 的多运行时方式能力更强。

---

## 四、octo 缺失的关键能力分析

### 4.1 必须内置（MCP 无法替代）

| 能力 | 存在于 | 为何 MCP 无法替代 | 建议优先级 |
|------|--------|------------------|-----------|
| **持久进程管理** (process_start/poll/write/kill) | OpenFang | MCP server 是外部进程，无法管理宿主进程的子进程 handle、stdin/stdout 管道 | P1 |
| **Agent IPC 通信** | ZeroClaw (SQLite 共享 DB), OpenFang (agent_send/find) | 多 Agent 共享内存/消息需要同进程空间或共享存储，MCP server 是隔离进程 | P1 |
| **任务队列** (task_post/claim/complete) | OpenFang | 需绑定到 Agent 运行时的状态机，MCP 无法感知 Agent 内部调度 | P2 |
| **知识图谱工具** | OpenFang (knowledge_add_entity/relation/query) | octo 已有 KnowledgeGraph 模块但未暴露为 Tool；KG 与内部存储强耦合 | **P0** |
| **execution_timeout() trait 方法** | IronClaw | 超时控制必须在框架层强制执行，MCP 只能靠 server 自己实现 | **P0** |
| **sensitive_params() trait 方法** | IronClaw | 参数脱敏必须在日志/审计记录前完成，MCP 工具的参数已经序列化 | P1 |
| **rate_limit_config() trait 方法** | IronClaw | 速率限制必须在框架层统一管控，防止 Agent 循环调用 | P1 |

### 4.2 建议内置（MCP 可部分替代但内置更优）

| 能力 | 竞品 | MCP 替代可行性 | 理由 |
|------|------|---------------|------|
| apply_patch | OpenFang, IronClaw, ZeroClaw | 中 | 多文件批量修改比 file_edit 效率高，且需要宿主文件缓存 |
| Shell 环境变量清洗 | IronClaw (shell.rs) | 低 | bash 工具的安全增强，必须在执行前注入 |
| 参数敏感审批 | IronClaw | 低 | 需修改 `approval()` 签名为 `approval(&self, params: &Value)` |

### 4.3 适合 MCP 外置（低优先级内置）

| 能力 | 竞品 | 理由 |
|------|------|------|
| 文档解析 (PDF/DOCX/XLSX/PPTX) | ZeroClaw (4种), Goose (3种) | 重依赖外部库，冷启动不影响核心流程 |
| 浏览器自动化 | OpenFang (10工具), Moltis | Playwright MCP server 已成熟 |
| 截图 | ZeroClaw, OpenFang | 平台特化，适合外置 |
| 图像生成 / TTS / STT | OpenFang | 纯外部 API 调用 |
| A2A 协议 | OpenFang | 可通过 MCP server 实现 |
| 地理位置 / 地图 / 计算器 | Moltis, OpenFang | LLM 能自己做或通过 MCP |

### 4.4 不需要的（竞品过度设计）

| 能力 | 竞品 | 理由 |
|------|------|------|
| echo / time / json 工具 | IronClaw | LLM 自己能做 |
| 硬件操控 (GPIO) | ZeroClaw | 极度特化 IoT 场景 |
| Canvas HTML 渲染 | OpenFang | 属于 UI 层职责 |
| SOP 流程引擎 | ZeroClaw | 过于特化，可作为 Skill 实现 |
| Pushover 通知 | ZeroClaw | MCP 外置即可 |

---

## 五、综合评分（10 分制）

### 5.1 Tool Trait 设计成熟度

| 框架 | 评分 | 理由 |
|------|------|------|
| **IronClaw** | **9.0** | 9 个额外 trait 方法覆盖 cost/timeout/sanitization/sensitive_params/rate_limit/domain，schema 校验完善 |
| **octo** | **7.0** | risk_level + approval 方向正确，ApprovalGate 实用；缺 timeout/cost/rate_limit/sensitive_params |
| **Moltis** | **6.5** | 多层 policy 系统 + SSRF 防护；trait 本身不如 IronClaw |
| **ZeroClaw** | **5.5** | trait 极简，安全全靠外部 SecurityPolicy 注入 |
| **OpenFang** | **4.5** | 非 trait 架构（struct 数组 + match 分发），无法通过 trait 扩展 |

### 5.2 内置工具覆盖度

| 框架 | 数量 | 评分 | 理由 |
|------|------|------|------|
| **ZeroClaw** | ~70+ | **8.5** | 覆盖最广：文档解析/截图/IPC/SOP/硬件 |
| **OpenFang** | ~60 | **8.5** | 极广覆盖：浏览器(10)/媒体/A2A/TTS/进程管理/Canvas |
| **IronClaw** | ~35+ | **7.5** | 精选实用 + WASM/MCP 扩展 + Job/Routine 系统 |
| **Moltis** | ~30+ | **7.0** | 浏览器/WASM/会话通信，缺文档解析 |
| **Goose** | 0 内置 | **6.0** | 纯 MCP 架构，灵活但依赖外部 |
| **octo** | 16 | **5.5** | 核心齐全但缺高级工具；MCP 生态可补齐大部分 |

### 5.3 工具结果处理

| 框架 | 评分 | 理由 |
|------|------|------|
| **octo** | **9.0** | Head67Tail27 截断独有，字节+行双重限制，策略可选 |
| **IronClaw** | **7.5** | 输出净化 + 敏感参数脱敏，但无截断 |
| **Moltis** | **5.0** | SSRF 防护，无通用截断 |
| **ZeroClaw** | **4.0** | 无明确截断 |
| **OpenFang** | **4.0** | taint tracking 防注入，无输出截断 |

### 5.4 Skill 系统

| 框架 | 评分 | 理由 |
|------|------|------|
| **octo** | **8.5** | 架构最完整（20 模块）：语义索引 + 依赖图 + 信任管理 + 4 种运行时 + 工具约束 |
| **IronClaw** | **8.0** | 最实用：SKILL.md prompt 注入 + 信任模型 + ClawHub + 工具衰减 |
| **Moltis** | **6.0** | skill_tools 基础功能 |
| **OpenFang** | **5.5** | SkillRegistry + "Hands" 概念 |
| **ZeroClaw** | **2.0** | 无 Skill 系统 |

### 5.5 Agent 间通信工具

| 框架 | 评分 | 理由 |
|------|------|------|
| **OpenFang** | **9.0** | agent_send/spawn/kill/find + task_post/claim + event_publish + a2a + Hands |
| **ZeroClaw** | **8.0** | agents_ipc（SQLite 共享 DB 5 工具）+ delegate + subagent 全套 |
| **Moltis** | **7.0** | sessions_communicate (send/receive/broadcast) + spawn_agent |
| **octo** | **5.0** | 仅 spawn_subagent + query_subagent，无 IPC/任务队列/事件发布 |
| **IronClaw** | **4.5** | Job 系统有但无直接 agent 间消息 |

### 5.6 综合加权评分

> 权重：Trait 设计 25% + 工具覆盖 25% + 结果处理 15% + Skill 20% + Agent 通信 15%

| 框架 | Trait | 覆盖 | 结果 | Skill | 通信 | **总分** |
|------|-------|------|------|-------|------|---------|
| **IronClaw** | 9.0 | 7.5 | 7.5 | 8.0 | 4.5 | **7.5** |
| **octo** | 7.0 | 5.5 | 9.0 | 8.5 | 5.0 | **6.8** |
| **OpenFang** | 4.5 | 8.5 | 4.0 | 5.5 | 9.0 | **6.5** |
| **Moltis** | 6.5 | 7.0 | 5.0 | 6.0 | 7.0 | **6.3** |
| **ZeroClaw** | 5.5 | 8.5 | 4.0 | 2.0 | 8.0 | **6.0** |

---

## 六、改进建议优先级排序

### P0 -- 高收益低成本

| # | 改进项 | 工作量 | 理由 |
|---|--------|--------|------|
| 1 | **暴露 KnowledgeGraph 为 Tool**（knowledge_add_entity/relation/query） | 小（模块已存在，只需写 3 个 Tool impl） | octo 已有 KG 模块但 LLM 无法调用，竞品 OpenFang 已实现 |
| 2 | **添加 `execution_timeout()` trait 方法** | 小（trait 加默认方法 + 执行层 tokio::timeout） | IronClaw 标配，防止工具执行无限挂起 |
| 3 | **添加 `sensitive_params()` trait 方法** | 小（trait 加默认方法 + recorder/日志层脱敏） | 防止 memory_store 等工具的敏感内容泄漏到日志 |

### P1 -- 中等优先级

| # | 改进项 | 工作量 | 理由 |
|---|--------|--------|------|
| 4 | **添加 apply_patch 工具** | 中 | 竞品普遍支持，比 file_edit 更适合多文件批量修改 |
| 5 | **添加 rate_limit_config() trait 方法** | 中（trait + 全局 RateLimiter） | 防止 Agent 循环调用同一工具 |
| 6 | **添加持久进程管理** (process_start/poll/write/kill) | 中 | OpenFang 实现了完整的 REPL/服务器生命周期管理 |
| 7 | **增强 Agent 间通信** | 大 | 从 subagent 扩展到 IPC 消息 + 共享状态 + 任务队列 |
| 8 | **bash 工具环境变量清洗** | 小 | IronClaw 做法：执行前擦除 API_KEY/TOKEN 等敏感环境变量 |
| 9 | **修改 approval() 签名为参数敏感** | 小（改签名为 `approval(&self, params: &Value)`） | IronClaw 做法：同一工具不同参数可需不同审批级别 |

### P2 -- 可通过 MCP 解决

| # | 改进项 | 建议方式 |
|---|--------|---------|
| 10 | 文档解析 (PDF/DOCX/XLSX) | MCP server |
| 11 | 浏览器自动化 | Playwright MCP server |
| 12 | 截图 | MCP server |
| 13 | 图像生成 / TTS / STT | MCP server |

---

## 七、octo 的独特优势（保持并强化）

1. **Head67Tail27 截断策略** -- 所有竞品均无。LLM 场景下比简单截断更有效，应考虑将此策略也应用于 MCP 工具结果
2. **Skill 系统架构** -- 20 子模块的完整设计，尤其是 SkillSemanticIndex（语义匹配选择 Skill）和 SkillDependencyGraph 在竞品中独一无二
3. **ApprovalGate 异步审批** -- oneshot channel + timeout 的实现优雅且实用
4. **risk_level + approval 在 trait 层面** -- 比 ZeroClaw/OpenFang 的外部注入方式更类型安全
5. **MCP 生态定位** -- McpManager + McpToolBridge 已为外部工具扩展打好基础，内置工具少不一定是劣势，关键是核心能力（审批/截断/并行/Skill）足够强
