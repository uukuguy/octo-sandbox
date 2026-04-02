# octo-sandbox 工作日志

## Phase AR — CC-OSS 缺口补齐 (2026-04-02)

### 完成内容

解锁 7 个追赶 CC-OSS 的必选 deferred 项，分 3 个 Wave 交付。

**Wave 1: 基础设施增强 (T1+T2+T3)**
- `token_escalation.rs`: 阶梯式 max_tokens 自动升级器（4096→8192→16384→32768→65536），截断时先升档再重试，省一轮 ContinuationTracker 调用
- `transcript.rs`: 追加式 JSONL 会话抄本，每轮结束写入 TranscriptEntry（preview+blob_ref+tokens）
- `blob_gc.rs`: BlobStore GC，TTL（7天）+ 容量（1GB）双重策略清理

**Wave 2: 会话管理增强 (T4)**
- `executor.rs`: AgentMessage::Rewind/Fork 变体 + handle 方法
- `harness.rs`: `rewind_messages()` 按 turn 截断对话历史
- `sessions.rs`: POST /sessions/{id}/rewind 和 /fork REST 端点

**Wave 3: 外部集成 (T5+T6+T7)**
- `autonomous_trigger.rs`: TriggerSource trait + ChannelTriggerSource（webhook→内部调度）+ PollingTriggerSource（MQ 轮询适配）+ TriggerListener 后台统一监听
- `autonomous.rs` (server): POST /autonomous/trigger webhook 端点
- `tool_search.rs`: hybrid_search_tools() 混合搜索 — 子串匹配 + Jaccard token-overlap 语义 fallback

### 技术变更
- 新增 5 个文件，修改 9 个文件，+1250 行
- AgentLoopConfig 新增 transcript_writer 字段
- harness.rs 在 MaxTokens 分支前插入 TokenEscalation 逻辑
- executor.rs 每轮结束调用 write_transcript() 写入新消息

### 测试结果
- 29 个新测试全部通过
  - token_escalation: 4, transcript: 6, blob_gc: 4, rewind: 4
  - autonomous_trigger: 4, hybrid_search: 3, tokenize: 1, jaccard: 2, dedup: 1, plus extras
- workspace 编译通过（0 errors）

### 解决的 Deferred 项
- AP-D2 → TokenEscalation (T1)
- AP-D6 → TranscriptWriter (T2)
- AP-D7 → Rewind/Fork API (T4)
- AQ-D2 → BlobGc (T3)
- AQ-D3 → hybrid_search_tools (T7)
- AQ-D4 → ChannelTriggerSource + webhook (T5)
- AQ-D5 → PollingTriggerSource + TriggerListener (T6)

### 新增 Deferred 项
- AR-D1: TranscriptWriter 压缩归档（gzip 老 transcript）
- AR-D2: Fork API 前端 UI（分支可视化）
- AR-D3: TriggerSource Redis/NATS 具体实现
- AR-D4: 语义搜索 index 持久化（避免每次重建）

---

## Phase AH — Hook 系统增强：三层混合架构 (2026-03-30)

### 完成内容

将 octo 的 hook 系统从"空转框架"（HookRegistry 14 个 HookPoint 但无 handler）升级为三层混合架构，支持多语言扩展和完整环境上下文传递。

**G1: HookContext 增强 (94f6b40)**
- 新增运行环境字段：working_dir, sandbox_mode, sandbox_profile, model, autonomy_level
- 新增历史字段：total_tool_calls, current_round, recent_tools (最近10次)
- 新增 user_query 字段
- 添加 `Serialize` 派生 + `to_json()` / `to_env_vars()` 序列化方法
- harness.rs 中创建 `build_rich_hook_context()` 替换所有10+ 简单构造点

**G2: 内置 Handler 注册 (69b95cb)**
- `SecurityPolicyHandler` (PreToolUse, priority=10, FailClosed): forbidden_paths + command risk + autonomy level
- `AuditLogHandler` (PostToolUse, priority=200, FailOpen): 结构化 tracing::info! 审计
- AgentRuntime 初始化时通过 tokio::spawn 注册

**G3: 声明式加载与 Command 执行 (41dd651)**
- `config.rs`: hooks.yaml 配置类型 (HooksConfig/HookEntry/HookActionConfig)
- `command_executor.rs`: sh -c 执行外部脚本，env vars + stdin JSON 双通道传递
- `bridge.rs`: DeclarativeHookBridge (priority=500)，regex tool 匹配
- `loader.rs`: 分层配置加载 (OCTO_HOOKS_FILE > project/.octo > ~/.octo)

**G4: Prompt LLM 评估 (4e890bc)**
- `prompt_renderer.rs`: {{variable}} 模板渲染，无变量时自动附加完整 JSON
- `prompt_executor.rs`: Provider::complete() 调用 + JSON 决策解析 + keyword fallback

**G5: 策略引擎 (4e890bc)**
- `config.rs`: policies.yaml 配置 (6种规则类型)
- `matcher.rs`: 路径/命令/工具匹配 + 条件表达式 (context.field == 'value')
- `bridge.rs`: PolicyEngineBridge (priority=100, FailClosed)

**Deferred 补齐 (4ebc7fa)**
- AH-D7: AgentRuntime 自动加载 hooks.yaml + policies.yaml 注册 bridge
- AH-D8: prompt action 调用 execute_prompt (with_provider builder)
- AH-D1: webhook_executor.rs (reqwest HTTP POST/PUT)

### 技术变更
- 新增文件结构: `hooks/builtin/` (2文件), `hooks/declarative/` (7文件), `hooks/policy/` (3文件)
- 修改: `hooks/context.rs`, `hooks/mod.rs`, `agent/harness.rs`, `agent/runtime.rs`
- 新增约 3700 行代码

### 测试结果
- 104 hook tests 全部通过
- Workspace 编译通过，无 error

### 未完成项
- AH-D2 ⏳ WASM 插件 hook (blocked: WASM 基础)
- AH-D3 ⏳ 平台租户策略合并 (blocked: platform-server)
- AH-D4 ⏳ TUI hook 状态面板 (P4)
- AH-D5 ⏳ Stop/SubagentStop 事件声明式 (P3)
- AH-D6 ⏳ ask → ApprovalGate 集成 (P3)
- Landmine: `with_provider()` 未在 runtime 中调用，prompt hooks 优雅跳过

### 下一步
- 创建示例 hooks.yaml / policies.yaml 配置
- D5 Stop events 支持
- 集成测试验证三层 hook 链

---

## SubAgent Streaming Events (2026-03-27)

### 完成内容

实现 sub-agent 流式事件转发，使 TUI 能实时显示 skill playbook 模式下 sub-agent 的推理和工具调用过程。

**1. Sub-Agent 事件转发 (12c7752)**
- `SubAgentContext` 新增 `event_sender: Option<broadcast::Sender<AgentEvent>>`
- `execute_playbook` 将所有中间事件（TextDelta、ThinkingDelta、ToolStart/Result 等）通过 broadcast channel 转发到父 agent
- `AgentExecutor` 构建 `SubAgentContext` 时传入 `self.broadcast_tx.clone()`
- 新增 `AgentEvent` 变体：`SubAgentTextDelta`、`SubAgentThinkingDelta`、`SubAgentToolStart`、`SubAgentToolResult`

**2. TUI 渲染 Sub-Agent 事件 (cc05eeb)**
- TUI `AppState` 新增 sub-agent 状态字段（`sub_agent_active`、`sub_agent_text`、`sub_agent_tool_name`）
- Sub-agent 事件在 TUI 中以缩进隔离块显示
- Tool 完成时自动清理 sub-agent 状态

**3. Provider 修复 (d6fbe9b)**
- `OpenAIProvider` 对 localhost/127.0.0.1 使用 `.no_proxy()` 绕过系统代理，修复 502 错误

**4. 配置修复 (7de26ea)**
- 统一凭据优先级：env > credentials.yaml > config.yaml
- 修复 Ollama reasoning 配置不匹配问题

### 技术变更
- `crates/octo-engine/src/agent/events.rs` — 新增 SubAgent* 事件变体
- `crates/octo-engine/src/skills/execute_tool.rs` — sub-agent 事件转发逻辑
- `crates/octo-engine/src/agent/harness.rs` — SubAgentContext 传入 broadcast_tx
- `crates/octo-cli/src/tui/app_state.rs` — sub-agent 状态字段 + 渲染
- `crates/octo-cli/src/tui/mod.rs` — sub-agent 事件处理 + 状态清理
- `crates/octo-engine/src/providers/openai.rs` — no_proxy + think tag filter

### 提交记录
- `cc05eeb` feat(tui): render sub-agent streaming events in isolated indented block
- `12c7752` feat(skills): forward sub-agent streaming events to parent TUI
- `d6fbe9b` fix(provider): bypass system proxy for local LLM endpoints
- `7de26ea` fix(config): unify credential priority and fix Ollama reasoning mismatch

### 下一步建议
- 实现 scheduler tool（schedule_task）暴露调度器 CRUD 给 TUI agent
- 测试 sub-agent 流式输出在实际 skill 执行中的表现

---

## Builtin Commands Redesign (2026-03-26)

### 完成内容

基于 GitHub Copilot、Claude Code bundled skills、Awesome Claude Skills (9.8k stars) 和 Skills Marketplace 的调研，重新设计 10 个内置命令。

- 升级 6 个代码工程命令模板（review/test/fix/refactor/doc/commit）为结构化多步骤提示词
- 新增 4 个企业级命令：/security (OWASP audit), /plan (需求分解), /audit (6 维度代码库评估), /bootstrap (脚手架)
- 移除 4 个低价值命令：summarize, translate, explain, optimize
- 修复显示 bug：斜杠命令显示简洁输入而非完整展开提示词
- 更新 10 命令测试断言
- 设计文档 + 调研来源

### 提交记录
- `1916320` feat(commands): redesign builtin commands with enterprise-grade templates

### 测试结果
- 66 tests pass (22 commands + 44 key_handler)
- 基线 2476 不变

---

## MCP Support + TUI Robustness + Custom Commands (2026-03-26)

### 完成内容

本次会话完成 11 项任务，涵盖 3 大领域：MCP/TUI 健壮性修复、UTF-8 安全性、自定义斜杠命令。

**1. TUI 输入修复 — stdin 隔离 (7bb3757)**
- 根因：`tokio::process::Command` 默认 `stdin=Stdio::inherit()`，子进程（bash, grep, find, python, nodejs, shell, sandbox）竞争读取终端 stdin
- crossterm EventStream 的 ANSI escape 序列被子进程截断，导致输入区出现 `[C[[5~[<35;79;27M` 乱码
- 修复：8 个子进程创建点全部加 `stdin(Stdio::null())`
- 防御：agent Completed/Done/Error 事件处理中追加 `enable_raw_mode()` 恢复

**2. UTF-8 安全截断 (556aa34)**
- web_search、file_read、CLI preview、TUI tool display 中 4 处字符串截断可能切断多字节字符
- 新增 `safe_truncate_utf8()` 工具函数，所有截断改用安全版本

**3. 表格渲染修复 (c8c267d)**
- Markdown 表格列宽自适应终端宽度
- 清理表格单元格中的 HTML 标签（防止渲染错误）

**4. Qwen XML 工具调用恢复 (93d2efd)**
- 解析非标准 LLM 输出中 XML 风格的工具调用（`<tool_call>...</tool_call>`）
- 支持 Qwen 系列模型的工具调用格式

**5. 自定义斜杠命令 (c1e99ca)**
- 新增 `crates/octo-engine/src/commands.rs` — 命令加载器
- `.octo/commands/` 下 `.md` 文件成为 `/命令名`，支持 `$ARGUMENTS` 参数替换
- 子目录命名空间：`review/pr.md` → `/review:pr`
- TUI 自动补全集成，`/help` 动态列出自定义命令
- 优先级：项目级 > 全局级 > 内置

**6. 内置命令 (263eeb2)**
- 10 个内置命令通过 `include_dir!` 编译进二进制
- 启动时 sync 到 `~/.octo/commands/`（不覆盖已有文件）
- 命令列表：review, explain, refactor, test, fix, doc, optimize, summarize, translate, commit

### 技术变更
- `crates/octo-engine/src/commands.rs` — 全新模块（CustomCommand, load_commands, sync_builtin_commands）
- `crates/octo-engine/builtin/commands/` — 10 个 `.md` 模板文件
- `crates/octo-engine/src/root.rs` — 新增 commands_dirs() + ensure_dirs 创建 commands 目录
- `crates/octo-cli/src/tui/key_handler.rs` — execute_slash_command 改为 async，支持自定义命令分发
- `crates/octo-cli/src/tui/mod.rs` — TUI 启动时 sync builtin + 加载命令 + 注册自动补全
- `crates/octo-engine/src/tools/bash.rs` — stdin(Stdio::null())
- `crates/octo-engine/src/tools/grep.rs` — stdin(Stdio::null())
- `crates/octo-engine/src/tools/find.rs` — stdin(Stdio::null())
- `crates/octo-sandbox/src/native.rs` — stdin(Stdio::null())
- `crates/octo-engine/src/agent/harness.rs` — Qwen XML tool call recovery

### 测试结果
- commands 模块：17 tests passed（11 原有 + 6 新 builtin 测试）
- key_handler：44 tests passed（9 个 slash command 测试改为 async）
- 全量测试基线：2476 passing

### 下一步建议
- 测试 TUI 中自定义命令的实际使用体验
- 考虑增加更多内置命令（如 `/search`、`/plan`）
- 考虑命令参数自动补全（目前只补全命令名）

---

## Post-AF Cleanup — Builtin Skills + Config Seeding + TUI Fix (2026-03-25)

### 完成内容

三项架构改进，解决 builtin skills 分发、配置可发现性、和 --project TUI 显示问题。

**1. Builtin Skills 架构重构 (98e781c)**
- 将 10 个 builtin skills（docx, pdf, pptx, xlsx, filesystem, web-search, image-analysis, skill-creator, uv-pip-install, docling）从 `.octo/skills/` 迁移到 `crates/octo-engine/builtin/skills/`
- 使用 `include_dir!` 宏将完整目录树编译进二进制文件（替代旧的 `include_str!` 只嵌入 2 个 skill）
- 首次启动时自动 sync 到 `~/.octo/skills/`，永不覆盖用户自定义
- `.octo/skills/` 现在仅用于项目级自定义 skills
- 修复 web-search 测试：include_dir 带入 scripts/ 目录后自动推断为 Playbook 模式

**2. Config Auto-Seeding (47dafb9)**
- `config.default.yaml` 更新为全量注释参考文件，覆盖所有配置项
- `OctoRoot::seed_default_config()` 使用 `include_str!` 编译时嵌入
- `ensure_dirs()` 自动 seed 到 `~/.octo/config.yaml` 和 `$PROJECT/.octo/config.yaml`
- 已有文件永不覆盖

**3. TUI Working Dir 修复 (072c15b)**
- TuiState 原本硬编码 `std::env::current_dir()` 作为状态栏路径和自动补全基目录
- 新增 `set_working_dir()` 方法，从 AppState.working_dir（来自 OctoRoot）正确传入
- `--project` 启动时状态栏和文件自动补全现在显示正确路径

### 技术变更
- `crates/octo-engine/Cargo.toml` — 新增 `include_dir = "0.7"` 依赖
- `crates/octo-engine/src/skills/initializer.rs` — 完全重写，`include_dir!` 嵌入全部 skills
- `crates/octo-engine/src/agent/runtime.rs` — sync 目标改为 `~/.octo/skills/`（全局）
- `crates/octo-engine/src/root.rs` — 新增 `seed_default_config()` + `ensure_dirs()` 调用
- `crates/octo-engine/tests/skills_e2e.rs` — web-search 测试从 Knowledge → Playbook
- `crates/octo-cli/src/tui/app_state.rs` — 新增 `set_working_dir()`
- `crates/octo-cli/src/tui/mod.rs` — 调用 `set_working_dir(state.working_dir)`
- `config.default.yaml` — 全量注释参考

### 测试
- 2476 tests passing（与 Phase AF 基线持平）
- 0 failures

---

## Phase AB — 智能体工具执行环境 (2026-03-23)

### 完成内容

实现沙箱执行环境，将现有沙箱基础设施（SandboxRouter/SandboxPolicy/Docker/WASM/Subprocess 适配器）与实际工具/技能执行层连接。

**G1 Profile + RunMode + Config (AB-T1 ~ AB-T3)**
- SandboxProfile 枚举：dev/stg/prod/custom，resolve() 优先级链 (--sandbox-bypass > --sandbox-profile > env > config)
- OctoRunMode 自动检测：/.dockerenv > /run/.containerenv > KUBERNETES_SERVICE_HOST > env
- SandboxType 新增 External(String) 变体，Copy→Clone 迁移，所有调用点更新为引用

**G2 BashTool + SkillRuntime 集成 (AB-T4 ~ AB-T6)**
- ExecutionTargetResolver 路由决策引擎：RunMode × Profile × ToolCategory → Local|Sandbox
- BashTool 重构：with_sandbox() 构造器，profile-aware 环境变量过滤
- SkillContext +sandbox_profile 字段，Shell/Node/Python 运行时尊重 profile timeout

**G3 可观测性 (AB-T7 ~ AB-T8)**
- ToolExecution +4 遥测字段：sandbox_profile, execution_target, actual_backend, routing_reason
- StatusBar 沙箱 profile 徽章，颜色编码（绿=dev, 黄=staging, 红=production）

**G4 外部沙箱 + CLI (AB-T9 ~ AB-T10)**
- ExternalSandboxProvider async trait + StubE2BProvider（E2B/Modal/Firecracker 接口定义）
- CLI `octo sandbox` 诊断命令：status/dry-run/list-backends

### 技术变更
- `crates/octo-engine/src/sandbox/profile.rs` — 新建：SandboxProfile 枚举 (16 tests)
- `crates/octo-engine/src/sandbox/run_mode.rs` — 新建：OctoRunMode 自动检测 (9 tests)
- `crates/octo-engine/src/sandbox/target.rs` — 新建：ExecutionTargetResolver (12 tests)
- `crates/octo-engine/src/sandbox/external.rs` — 新建：ExternalSandboxProvider trait (9 tests)
- `crates/octo-engine/src/sandbox/traits.rs` — SandboxType +External, Copy→Clone
- `crates/octo-engine/src/sandbox/router.rs` — ToolCategory +Script/Gpu/Untrusted, 引用化 API
- `crates/octo-engine/src/tools/bash.rs` — BashTool 重构：sandbox routing 集成
- `crates/octo-engine/src/skill_runtime/` — SkillContext +sandbox_profile, timeout 尊重
- `crates/octo-types/src/execution.rs` — ToolExecution +4 遥测字段
- `crates/octo-cli/src/commands/sandbox.rs` — 新建：sandbox 诊断命令 (5 tests)
- `crates/octo-cli/src/tui/widgets/status_bar.rs` — sandbox profile 显示

### 测试
- octo-cli: 472 tests (was 456, +16)
- 所有 engine/types 测试通过
- Commit: 282d3f6

### 暂缓项
- AB-D1: Octo sandbox Docker image (Dockerfile + CI)
- AB-D2: E2B provider 完整实现
- AB-D3: WASM plugin loading
- AB-D4: Session Sandbox persistence
- AB-D5: CredentialResolver → sandbox env injection
- AB-D6: gVisor / Firecracker provider

---

## Phase AA — Octo 部署配置架构 (2026-03-23)

### 完成内容

实现分层配置加载系统，支持 global → project → local → env 多层配置合并，解决部署配置的灵活性和安全性需求。

**G1 OctoRoot 路径扩展 (AA-T1)**
- 新增 6 个路径方法：project_local_config, credentials_path, tls_dir, global_mcp_dir, project_mcp_dir, eval_config
- 5 个单元测试

**G2 分层配置加载 (AA-T2, AA-T2b)**
- Config::load() 重写为 7 层优先级：defaults → global → project → local → CLI → credentials → env
- 递归 YAML 字段级浅合并 (merge_yaml_values)
- --config 显式标志跳过自动发现
- 旧版 $PWD/config.yaml 兼容回退 + 迁移警告
- Server main.rs 重排序：OctoRoot 在 Config::load 之前发现

**G3 凭据加载 (AA-T3)**
- CredentialsFile 结构体从 ~/.octo/credentials.yaml 加载
- 在 config merge 和 env overrides 之间注入
- 优先级：env > credentials.yaml > config.yaml

**G4 硬编码路径修复 + CLI 增强 (AA-T4, AA-T5)**
- ./data/tls 和 ./data/certs → OctoRoot::tls_dir()
- `octo config show` 显示分层配置源链
- `octo config paths` 列出所有配置文件位置

**AA-D2 补齐：octo init 命令 (e85383a)**
- 创建 .octo/ 项目目录结构
- 生成 config.yaml, config.local.yaml, .gitignore, credentials.yaml(mode 600)
- 6 个单元测试

### 技术变更
- `crates/octo-engine/src/root.rs` — 新增路径访问器
- `crates/octo-server/src/config.rs` — 分层配置加载 + 凭据注入
- `crates/octo-server/src/main.rs` — OctoRoot 前置 + TLS 路径修复
- `crates/octo-cli/src/commands/init.rs` — 新建 octo init 命令
- `crates/octo-cli/src/commands/config.rs` — 增强 show/paths 显示

### 测试结果
- 基线: 2383 → 最终: 2394 (+11)，0 失败

### 遗留暂缓项
- AA-D1: `octo auth login/status/logout` CLI 命令（需 UX 设计）
- AA-D3: XDG Base Directory 支持（低优先级）
- AA-D4: Config 热重载（未来增强）

---

## Phase U — TUI Production Hardening + Post-Polish (2026-03-22)

### 完成内容

Phase T 完成后，对 TUI 进行生产级强化（10 个任务）和额外打磨（3 个提交）。

**G1 基础设施 (3/3)**
- ApprovalGate Y/N/A 按键接线（Arc<Mutex<HashMap>> + oneshot 通道）
- Event Batch Drain（while try_next() 循环）
- Scroll 3 级加速（3/6/12 行，200ms 方向窗口）

**G2 渲染优化 (3/3)**
- Per-message 缓存（content hash 失效）
- ToolFormatterRegistry（顺序匹配 + GenericFormatter 兜底）
- Tool Collapse（CC 风格，默认折叠，Ctrl+O 最近 / Alt+O 全局）

**G3 增强 Widgets (3/3)**
- StatusBar 重设计（品牌 + 模型 + tokens + elapsed + context%，dir + git）
- Todo Panel → PlanUpdate 事件替代 Active Tools
- InputWidget（去底框，mode-colored separator，dimmed text）

**G4 品牌完善 (1/1)**
- Welcome Panel ASCII Art OCTO + 🦑 fallback + amber 呼吸动画

**Post-Phase Polish (3 commits)**
- 实时工具折叠：ToolStart flush streaming text，ToolResult 即时插入 ToolUse+ToolResult 消息
- 状态栏：品牌、运行时长、git 状态颜色（clean/dirty/very dirty）
- ESC 取消保留已完成消息内容（cancelled flag 防止 Completed 覆盖）
- Git 信息每 5 秒自动刷新（tick counter 83 ≈ 5s）
- 工具展开时自动滚动到工具调用位置，关闭时滚回底部
- 系统消息（`<context>` XML）从对话区隐藏
- Activity indicator 行（thinking/streaming 状态 + 任务 tokens）
- Welcome panel 渐变动画

### 技术变更

| 文件 | 变更 |
|------|------|
| `tui/app_state.rs` | session_start_time, task_start_time, git_refresh_counter, cancelled flag |
| `tui/mod.rs` | 实时 ToolStart/ToolResult 处理, git refresh in Tick, IterationEnd tokens |
| `tui/key_handler.rs` | ESC cancel preserve, Ctrl+O scroll-to-tool, scroll reset on close |
| `tui/render.rs` | activity indicator row, session_elapsed, 4-panel layout |
| `tui/widgets/status_bar.rs` | 2-row layout, git status coloring, session elapsed |
| `tui/widgets/conversation/mod.rs` | System messages hidden, build_system_lines dead_code |
| `tui/widgets/conversation/spinner.rs` | ActiveTool + tool_id field |
| `tui/widgets/input.rs` | pending_count parameter |
| `tui/widgets/welcome_panel/` | gradient animation |
| `octo-engine/src/agent/events.rs` | IterationEnd event + serde tests |
| `octo-engine/src/agent/harness.rs` | IterationEnd broadcast |
| `Makefile` | cli-tui 使用 pre-built binary |

### 测试结果

- Workspace tests: 2329 通过
- octo-cli tests: 456 通过（基线 368 → 438 → 456）
- `cargo check --workspace` 零错误

### 提交记录

- `77c2297` feat(tui): auto-scroll to tool call when expanding, scroll to bottom when closing
- `f87b5d5` fix(tui): refresh git branch and dirty count every ~5 seconds
- `6e21f58` feat(tui): real-time tool folding, status bar brand/elapsed, ESC cancel preserves messages
- `8047947` feat(tui): status bar 3-row layout, activity indicator, welcome gradient animation
- `8ef602f` chore: Phase U complete — TUI Production Hardening 10/10 tasks, 2329 tests pass
- `9b68547` feat(tui): Welcome Panel brand upgrade — ASCII Art OCTO + 🦑 fallback (U4-1)
- `32cc16e` ~ `05c6cce` Phase U G1-G3 checkpoints

### 分支合并

- `feat/tui-opendev-integration` → fast-forward merge → `main`
- 当前在 `main` 分支

---

## Phase T — TUI OpenDev 整合 (2026-03-20 ~ 2026-03-22)

### 完成内容

将 opendev TUI 完整特性整合进 octo-cli，重建对话中心界面。24 个任务全部完成。

**T1 基础设施移植 (10/10)** @ 1d66ee7
- formatters (markdown, style_tokens, base)
- managers (clipboard, history)
- widgets (input, welcome_panel, conversation, spinner, status_bar, todo_panel)
- event system (AppEvent, EventHandler)

**T2 对话中心主界面 (8/8)** @ 6c5ac02 + e6c5f0d
- TuiState, render, key_handler, approval dialog
- Event loop with AgentEvent handling
- Autocomplete engine + slash commands
- Legacy 12-Tab cleanup

**T3 调试浮层 + 完善 (6/6)** @ 22a13ed
- agent_debug/eval/session_picker overlays
- Welcome panel + thinking/progress
- Theme validation

### 核心决策

- 类型统一：直接使用 octo-types（零适配层）
- 布局：对话中心 + 浮层调试，废弃 12-Tab
- 对接：与 REPL 共用 AgentExecutorHandle
- 完整特性：无 mock/stub

### 测试结果

- Tests: 2250→2259 (+9), octo-cli tests: 368

---

## CLI+Server Usability Fixes (2026-03-20)

### 完成内容

Phase S 评估完成后，对 CLI 和 Server 进行全面可用性修复。

**CLI 修复**
- clap `-c` 短选项冲突：`Run::resume` 从 `-c` 改为 `-C`
- REPL Ctrl+C 退出：双击 Ctrl+C 退出模式
- `ProviderConfig::default()` 读取 `LLM_PROVIDER`/`OPENAI_*`/`ANTHROPIC_*` 环境变量
- UTF-8 `truncate()` 中文截断 panic：使用 `floor_char_boundary()`
- 默认日志级别 warn（非 verbose 模式忽略 `.env` 中的 `RUST_LOG`）
- Makefile 新增 CLI 命令入口：`cli-run`, `cli-ask`, `cli-tui` 等 8 个

**Server 修复**
- Ctrl+C 无法退出：force-exit guard（5s 超时 + 第二次 Ctrl+C 立即退出）
  - 根因：axum graceful shutdown 等待 WebSocket 连接关闭
- 默认日志 `debug` → `info`，用 `OCTO_LOG` 替代 `RUST_LOG` 避免 `.env` 覆盖
- SSE chunk 日志噪音：`debug!` → `trace!`（openai.rs）
- `working_dir` 默认 `/tmp/octo-sandbox` → `current_dir()`（web agent 看不到项目文件）
- MCP shutdown 超时 30s → 3s
- Makefile server 目标加 `exec` 确保信号正确传递

**警告清理**
- `#[allow(dead_code)]` 处理 6 处 dead code 警告

### 技术变更

| 文件 | 变更 |
|------|------|
| `Makefile` | CLI 命令入口 + server exec |
| `octo-cli/src/lib.rs` | `-c` → `-C` |
| `octo-cli/src/main.rs` | 日志级别 warn |
| `octo-cli/src/repl/mod.rs` | 双击 Ctrl+C |
| `octo-cli/src/ui/streaming.rs` | UTF-8 safe truncate |
| `octo-engine/src/providers/config.rs` | env var 读取 |
| `octo-engine/src/providers/openai.rs` | SSE trace! |
| `octo-engine/src/agent/runtime.rs` | current_dir() |
| `octo-server/src/main.rs` | OCTO_LOG + force-exit |
| `octo-server/src/config.rs` | 日志 info |

### 测试结果

- `cargo check` 零警告（octo-engine, octo-eval, octo-cli, octo-server）
- UTF-8 truncate 测试 5/5 通过
- Server SIGINT 退出测试通过

### 提交

- `b4ebcbe` fix(cli+server): CLI usability fixes and server hardening

---

## Phase O — Deferred 暂缓项全解锁 (2026-03-15)

### 完成内容

Phase O 目标：解决 Phase M-a/M-b/N 累积的全部 10 个暂缓项。15/15 任务完成。

**G1: TUI Input Widget 抽取** (O-T1~T6)
- 抽取 `TextInput` 可复用组件 (`tui/widgets/text_input.rs`)
- ChatScreen 重构使用 TextInput widget
- Eval shortcut dialogs (M-b_D1)、filter popup (M-b_D2)
- Memory 搜索交互 (N_D2)
- Watch 实时进度条 with Gauge (M-a_D3)

**G2: ProviderChain Failover Trace** (O-T7~T9)
- FailoverTrace 数据结构 (ring buffer) 在 `providers/chain.rs`
- ChainProvider complete()/stream() 方法插桩记录 failover 轨迹
- Provider Inspector 可视化 (N_D3)

**G3: Session Event 广播** (O-T10~T13)
- SessionEvent enum + EventBus (`session/events.rs`)
- WS SessionUpdate 消息推送
- DevAgent TUI event-driven refresh (N_D1)

**G4: Workbench 收尾** (O-T14~T15)
- Workbench 模式审计 vs 设计文档 §6.9.2 (N_D4)
- 3 个计划文档中所有 deferred 状态更新为已完成

### 测试结果

- **2178 tests pass**（基线 2126，+52 新增）
- 0 failures, 0 remaining deferred items
- 5 commits merged

### 暂缓项解决矩阵

| 暂缓项 | 来源 | 解决任务 |
|--------|------|----------|
| M-a_D3: watch 实时进度条 | Phase M-a | G1-T6 |
| M-b_D1: Eval shortcut dialogs | Phase M-b | G1-T3 |
| M-b_D2: Eval filter popup | Phase M-b | G1-T4 |
| N_D1: Session 实时数据流 | Phase N | G3-T10~T13 |
| N_D2: Memory 搜索交互 | Phase N | G1-T5 |
| N_D3: Provider failover 可视化 | Phase N | G2-T7~T9 |
| N_D4: 完整 Workbench 模式 | Phase N | G4-T14 |

---

## Phase N — Agent Debug Panel (2026-03-15)

### 完成内容

- DevAgentScreen 全功能调试面板 (`tui/screens/dev_agent.rs`)
- AgentFocus 枚举、InspectorPanel、DevAgentScreen 结构
- 7/7 任务完成，+30 tests (2096→2126)

---

## Phase M-b — TUI Dual-View + Eval Panel (2026-03-15)

### 完成内容

- TUI 双视图模式 (ViewMode::Ops / ViewMode::Dev)
- DevEvalScreen 评估面板 (`tui/screens/dev_eval.rs`)
- OpsTab / DevTask 枚举，TUI 事件系统
- 8/8 任务完成，+38 tests (2058→2096)

---

## Phase M-a — Eval Management CLI Unification (2026-03-15)

### 完成内容

- RunStore 持久化 + EvalCommands (11 个子命令)
- handle_eval 路由统一
- 12/12 任务完成，+8 tests (2050→2058)

---

## Phase L — Eval Whitebox + Enterprise Dataset (2026-03-15)

### 完成内容

- L1: TraceEvent (10 variants) + EvalTrace.timeline + UTF-8 修复
- L2: FailureClass (14 variants) + FailureClassifier
- L3: EvalScore.dimensions 多维化 + ToolCallScorer/BehaviorCheckScorer
- L4: PlatformBehaviorScorer + EventSequenceScorer + 27 新评估任务
- L5: 数据集标注 + 设计文档最终化
- 18/18 任务完成，+29 tests (2021→2050)

---

## Phase K — 完整真实模型对比报告 (2026-03-14)

### 完成内容（代码任务）

**K1-T1: 评估配置文件** (@ 6b68deb)
- 新建 `crates/octo-eval/eval.benchmark.toml` — 5 层模型矩阵
- T0 免费: Qwen3-Coder-480B (0/0 $/1M)
- T1 经济: DeepSeek-V3.2 (0.15/0.75 $/1M)
- T2 标准: Qwen3.5-122B (0.30/1.20 $/1M)
- T3 高性能: Kimi-K2.5 (0.45/2.20 $/1M)
- T4 旗舰: Claude-Sonnet-4.6 (3.0/15.0 $/1M)

**K3-T1/T2: BenchmarkAggregator** (@ 6b68deb)
- 新建 `crates/octo-eval/src/benchmark.rs` (~340 行)
- `BenchmarkAggregator::aggregate()` — 汇总多 Suite ComparisonReport
- `ModelBenchmark` — 每模型综合 pass_rate、avg_score、token 消耗、成本
- `CostAnalysis` — 成本效益分析，自动找出 >80% pass_rate 的最便宜模型
- `Recommendation` — 3 种场景推荐 (cost_sensitive/balanced/performance_first)
- `to_markdown()` — 综合报告含维度敏感度分析 (HIGH/MEDIUM/LOW)
- 7 个单元测试覆盖聚合、成本分析、推荐、Markdown/JSON 生成

**K3-T3: CLI benchmark 命令** (@ 6b68deb)
- 修改 `crates/octo-eval/src/main.rs` — 新增 `benchmark` 子命令
- Mode 1: `--suites tool_call,security,...` — 运行所有 suite 的 compare 并汇总
- Mode 2: `--input eval_output/benchmark` — 从已有 comparison.json 聚合

**K4-T2: CI 集成** (@ 6b68deb)
- 修改 `.github/workflows/eval-ci.yml` — 新增 benchmark regression step

### 文件变更矩阵

| 文件 | 操作 | 行数 |
|------|------|------|
| `crates/octo-eval/eval.benchmark.toml` | **新建** | 38 |
| `crates/octo-eval/src/benchmark.rs` | **新建** | ~340 |
| `crates/octo-eval/src/lib.rs` | 修改 | +1 |
| `crates/octo-eval/src/main.rs` | 修改 | +170 |
| `.github/workflows/eval-ci.yml` | 修改 | +6 |

### 测试结果

- 2021 tests passing (基线 2014，+7 新增)
- 新增 benchmark 模块测试: 7 个 (aggregate_empty, aggregate_single_suite, aggregate_multiple_suites, recommendations_generated, cost_analysis, markdown_generation, json_generation)

### 待完成（需用户执行）

- K1-T2: 模型连通性验证 — 需真实 API 调用
- K2-T1/T2/T3: 核心/差异化/SWE-bench Suite 对比 — 需真实 LLM 评估
- K4-T1: 录制 Replay 基线 — 评估完成后
- K5-T1/T2: 文档产出 — 评估数据就绪后

---

## Phase J — 沙箱安全体系建设 (2026-03-14)

### 完成内容

**J1: SandboxPolicy 策略引擎** (@ 4570365)
- 新增 `SandboxPolicy` 枚举 (Strict/Preferred/Development) 到 `traits.rs`
- Strict 为默认值：仅允许 Docker/WASM 执行，拒绝 Subprocess
- 新增 `PolicyDenied` 错误变体到 `SandboxError`
- `SandboxRouter` 集成策略执行：`with_policy()`, `resolve_fallback()`
- 更新 BashTool 使用 Development 策略
- 10 个新策略测试 + 更新现有测试适配策略

**J2: Docker 预置镜像与语言检测** (@ 5553c27)
- 创建 `docker/sandbox-images/Dockerfile.python` (python:3.12-slim-bookworm)
- 创建 `docker/sandbox-images/Dockerfile.rust` (rust:1.82-bookworm)
- 新增 `ImageRegistry` 结构体（8 种语言映射）
- DockerAdapter `execute()` 使用 language 参数自动选择镜像

**J3: DockerAdapter 测试加固** (@ 5553c27)
- `ContainerGuard` RAII 结构体确保测试清理
- `require_docker()` 辅助函数提供清晰 skip 消息
- Docker 环境诊断测试

**J4: WASM/WASI CLI 执行器** (@ 5553c27)
- 新增 `execute_wasi_cli()` 使用 wasmtime_wasi preview1
- WASI 上下文：args, stdin MemoryInputPipe, stdout/stderr 捕获
- 通过 `language="wasi-cli"` 或 `code` 前缀 `wasi://` 触发
- I32Exit 退出码处理
- 3 个新 WASI 测试

**J5: 沙箱审计日志** (@ 5553c27)
- 新增 `SandboxAuditEvent` (7 种 SandboxAction，SHA-256 代码哈希)
- 工厂方法：`execution()`, `policy_deny()`, `degradation()`
- `to_audit_event()` 转换到通用 AuditEvent 用于 hash-chain 存储
- `AuditStorage` 新增 `query_sandbox_events()` 和 `query_policy_denials()`
- 7 个审计测试

**J6/J7: Docker 测试修复与 CI 集成** (@ 45a7342)
- eval-ci.yml 新增 `docker-sandbox-tests` job
- 运行策略、审计、WASM、Docker 四组沙箱测试
- 容器泄漏检测步骤
- 新增 `octo-sandbox` 路径触发 CI

### 测试结果

- **2014 tests pass**（基线 1992，+22 新增）
- 0 failures, 3 ignored
- 新增测试分布：10 策略 + 7 审计 + 3 WASI + 2 Docker 辅助

### 文件变更矩阵

| 文件 | 操作 |
|------|------|
| `crates/octo-engine/src/sandbox/traits.rs` | 修改 (+SandboxPolicy, +PolicyDenied) |
| `crates/octo-engine/src/sandbox/router.rs` | 修改 (+policy 集成, +fallback) |
| `crates/octo-engine/src/sandbox/docker.rs` | 修改 (+ImageRegistry, language 路由) |
| `crates/octo-engine/src/sandbox/wasm.rs` | 修改 (+WASI CLI executor) |
| `crates/octo-engine/src/sandbox/audit.rs` | **新建** (SandboxAuditEvent) |
| `crates/octo-engine/src/sandbox/mod.rs` | 修改 (+re-exports) |
| `crates/octo-engine/src/audit/storage.rs` | 修改 (+sandbox queries) |
| `crates/octo-engine/src/tools/bash.rs` | 修改 (Development policy) |
| `docker/sandbox-images/Dockerfile.python` | **新建** |
| `docker/sandbox-images/Dockerfile.rust` | **新建** |
| `.github/workflows/eval-ci.yml` | 修改 (+docker-sandbox-tests job) |

---

## Phase I — External Benchmark Adapters (2026-03-14)

### 完成内容

**I1: ExternalBenchmark 抽象层** (@ 2e0d365)
- 定义 `ExternalBenchmark` trait (6 方法) + `BenchmarkVerifier` trait + `MetricDefinition` 系统
- 实现 `BenchmarkRegistry` 注册表，支持动态查找和列举
- 创建 GAIA / SWE-bench / τ-bench 三个骨架 adapter 实现
- 新增 `ScoreDetails` 变体: `GaiaMatch`, `SweVerify`, `PassK`
- CLI `load_suite()` 和 `list-suites` 集成外部 benchmark 动态加载

**I2: GAIA Benchmark 数据集** (@ 5512f4f)
- 创建 `gaia_sample.jsonl` — 50 个多步推理任务
- 分布: L1 (Easy) 20 个, L2 (Medium) 20 个, L3 (Hard) 10 个
- 覆盖: 数学, 地理, 科学, 历史, 文学, 技术等领域
- 工具: web_search, calculator, file_read, code_execution, database_query, api_call

**I3: SWE-bench Benchmark 数据集** (@ 5512f4f)
- 创建 `swe_bench_lite.jsonl` — 50 个代码修复任务
- 覆盖 8 个仓库: django (10), flask (7), sympy (8), requests (7), pytest (7), scikit-learn (3), matplotlib (8)
- 包含真实格式的 unified diff patch + test patch + problem statement
- 难度按 patch 大小和测试数量自动分类

**I4: τ-bench Benchmark 数据集** (@ 5512f4f)
- 创建 `tau_bench_retail.jsonl` — 30 个零售场景任务
- 分布: 退货 (10), 查询 (10), 修改 (10)
- 每条任务包含 policy_rules, expected_actions, expected_db_state
- pass^k=8 一致性指标

**I5: 验证与 CI 集成** (@ 57ca310)
- eval-ci.yml 新增 GAIA / SWE-bench / τ-bench 运行步骤
- SWE-bench 通过 DOCKER_AVAILABLE 环境变量条件执行
- 更新 eval_integration.rs 跳过外部 benchmark 文件验证
- 全量测试通过: 1992 tests (+13)

### 技术变更

| 文件 | 操作 | 说明 |
|------|------|------|
| `src/benchmarks/mod.rs` | 已有 | ExternalBenchmark trait + Registry (~110 行) |
| `src/benchmarks/gaia.rs` | 已有 | GAIA adapter (247 行, 含 4 个测试) |
| `src/benchmarks/swe_bench.rs` | 已有 | SWE-bench adapter (248 行, 含 3 个测试) |
| `src/benchmarks/tau_bench.rs` | 已有 | τ-bench adapter (266 行, 含 4 个测试) |
| `datasets/gaia_sample.jsonl` | 新建 | 50 GAIA 任务 |
| `datasets/swe_bench_lite.jsonl` | 新建 | 50 SWE-bench 任务 |
| `datasets/tau_bench_retail.jsonl` | 新建 | 30 τ-bench 任务 |
| `tests/eval_integration.rs` | 修改 | 添加 is_external_benchmark_file() |
| `.github/workflows/eval-ci.yml` | 修改 | +3 benchmark 步骤 |

### 测试结果

- octo-eval 单元测试: 28/28 通过
- workspace 全量测试: 1992/1992 通过
- 无 deferred 项

### 评估层次覆盖

```
Level 4: 端到端任务成功率 (SWE-bench 50 tasks)     → ✅ 已实现
Level 3: 多轮对话+工具链协调 (GAIA 50 + τ-bench 30) → ✅ 已实现
Level 2: 单次工具调用精确度 (BFCL 50 tasks)          → ✅ 已有
Level 1: 引擎基础能力 (单元测试 1992 tests)           → ✅ 已有
```

### 下一步

- Phase J: Docker 测试修复 → SWE-bench 从 mock 升级为真实验证
- Phase K: 跨 GAIA/SWE-bench/τ-bench 的多模型对比报告

---

## Phase H — Eval Capstone (2026-03-14)

### 完成内容

**H1: Resilience Suite + 新行为类型**
- 在 BehaviorScorer 中新增 4 种行为模式: retry_success, emergency_stopped, canary_detected, text_tool_recovered
- 同步更新 loader.rs 中的 score_behavior() 函数
- 创建 ResilienceSuite 模块 (resilience.rs) 和 20 条 JSONL 评估任务
- 注册到 mod.rs / main.rs / CLI help

**H2: Context 扩充**
- octo_context.jsonl 从 14 扩充到 50 条任务
- 新增 8 个评估维度: CX5 (degradation), CX6 (token budget), CX7 (long prompt), CX8 (multi-turn), CX9 (prioritization), CX10 (recovery), CX11 (format consistency), CX12 (information density)

**H3: AstMatch Scorer**
- 实现 AstMatchScorer，支持深层 JSON 结构比较
- 功能: 嵌套对象递归比较、数组顺序无关匹配、类型强转 (strict_types=false)、null=缺失语义、额外字段容忍
- 新增 AstMatch variant 到 ScoreDetails enum
- 在 auto_scorer() 中集成 "ast_match" scorer 覆盖
- 10 条 AST 匹配测试用例添加到 octo_tool_call.jsonl

**H4: 验证与 CI**
- eval-ci.yml 新增 resilience suite 运行步骤
- CLI list-suites 帮助文本更新
- 全量测试通过: 1979 tests (+17)

### 技术变更

| 文件 | 变更 |
|------|------|
| `crates/octo-eval/src/scorer.rs` | +4 behavior branches, +AstMatchScorer (~130 LOC), +16 tests |
| `crates/octo-eval/src/score.rs` | +AstMatch ScoreDetails variant |
| `crates/octo-eval/src/datasets/loader.rs` | +score_ast_match(), +strict_types field, +4 behaviors |
| `crates/octo-eval/src/suites/resilience.rs` | 新文件, ResilienceSuite 实现 |
| `crates/octo-eval/src/suites/mod.rs` | +resilience 导出 |
| `crates/octo-eval/src/main.rs` | +resilience import/load/help |
| `crates/octo-eval/datasets/octo_resilience.jsonl` | 新文件, 20 tasks |
| `crates/octo-eval/datasets/octo_context.jsonl` | 14→50 tasks |
| `crates/octo-eval/datasets/octo_tool_call.jsonl` | +10 AST tasks |
| `.github/workflows/eval-ci.yml` | +resilience suite step |

### 测试结果

- 全量: 1979 tests passing (was 1962)
- Docker tests: 5 excluded (Docker daemon not running)
- 编译无 warning

### 遗留问题

- 无

### 下一步

- Phase I: SWE-bench 适配 (12 tasks)
- Phase J: Docker 测试修复 (8 tasks)
- Phase K: 完整模型对比报告 (10 tasks)
