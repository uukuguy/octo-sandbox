# octo-engine CLI 验证案例集合

> 基于 6 个并行研究智能体的综合成果，覆盖 Agent Skills、MCP、内置工具、安全、内存、Provider、行业基准等 octo-engine 核心能力。
> 日期：2026-03-09 | 研究方法：RuFlo 6 智能体并行（主流 Skills、MCP Servers、octo-engine 能力分析、agentskills.io 标准、Skills 清单、行业基准）

---

## 目录

1. [验证架构总览](#1-验证架构总览)
2. [V1: Agent Skills 验证](#2-v1-agent-skills-验证)
3. [V2: MCP 集成验证](#3-v2-mcp-集成验证)
4. [V3: 内置工具验证](#4-v3-内置工具验证)
5. [V4: Agent Loop 与 Provider 验证](#5-v4-agent-loop-与-provider-验证)
6. [V5: 内存系统验证](#6-v5-内存系统验证)
7. [V6: 安全与审计验证](#7-v6-安全与审计验证)
8. [V7: Session 与配置验证](#8-v7-session-与配置验证)
9. [V8: Hook 系统验证](#9-v8-hook-系统验证)
10. [V9: Context 管理验证](#10-v9-context-管理验证)
11. [推荐的 MCP 测试服务器](#11-推荐的-mcp-测试服务器)
12. [主流 Agent Skills 清单](#12-主流-agent-skills-清单)
13. [行业基准实测指南](#13-行业基准实测指南)
14. [octo-engine 差距分析](#14-octo-engine-差距分析)

---

## 1. 验证架构总览

### 验证维度矩阵

| 维度 | 案例数 | 优先级 | 依赖 |
|------|--------|--------|------|
| V1: Agent Skills | 25 | P0 | Skills 模块 + agentskills.io 标准 |
| V2: MCP 集成 | 20 | P0 | MCP 模块 + 外部 MCP 服务器 |
| V3: 内置工具 | 30 | P0 | Tools 模块 |
| V4: Agent Loop & Provider | 15 | P0 | Agent + Provider 模块 |
| V5: 内存系统 | 12 | P0 | Memory 模块 |
| V6: 安全与审计 | 15 | P0 | Security + Audit 模块 |
| V7: Session & Config | 10 | P1 | Session + Config 模块 |
| V8: Hook 系统 | 10 | P1 | Hooks 模块 |
| V9: Context 管理 | 8 | P1 | Context 模块 |
| **合计** | **145** | | |

### CLI 验证命令设计（目标态）

```bash
# 列出所有验证案例
octo verify list [--category V1|V2|V3|...]

# 运行单个验证
octo verify run <case-id>

# 运行某个类别的全部验证
octo verify suite <category>

# 运行全部验证
octo verify all

# 查看验证报告
octo verify report
```

### 验证案例格式

每个案例包含：
- **ID**: `V{维度}-{序号}` (如 V1-01)
- **名称**: 简明描述
- **前置条件**: 测试前需要准备的状态
- **操作步骤**: CLI 命令或 API 调用
- **期望结果**: 成功/失败的判定标准
- **验证方式**: 自动/半自动/手动

---

## 2. V1: Agent Skills 验证

> 对标：agentskills.io 标准、Claude Code Skills、OpenAI Codex Skills、SWE-agent 工具集

### V1-A: Skill 标准合规性（10 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V1-01 | SKILL.md 基本解析 | 加载含有效 frontmatter（name + description）的 SKILL.md | Skill 注册成功，name/description 正确提取 | P0 |
| V1-02 | 缺失必填字段 | 加载缺少 `name` 字段的 SKILL.md | 返回验证错误：required field missing | P0 |
| V1-03 | name 格式验证 | 加载 name 含大写字母的 SKILL.md | 返回错误：name 必须小写字母+连字符 | P0 |
| V1-04 | name 长度限制 | 加载 name 超过 64 字符的 SKILL.md | 返回错误：name exceeds 64 chars | P0 |
| V1-05 | description 长度限制 | 加载 description 超过 1024 字符的 SKILL.md | 返回错误：description exceeds 1024 chars | P1 |
| V1-06 | description 禁用字符 | 加载 description 含 `<script>` 的 SKILL.md | 返回错误：angle brackets not allowed | P1 |
| V1-07 | 目录结构验证 | 验证含 scripts/、references/、assets/ 目录的 Skill | 通过验证，可选目录正确识别 | P0 |
| V1-08 | 无效目录结构 | scripts 是文件而非目录 | 返回错误：scripts must be a directory | P1 |
| V1-09 | 模板变量替换 | Body 含 `${baseDir}`，验证加载后替换为实际路径 | `${baseDir}` 替换为 Skill 目录的绝对路径 | P0 |
| V1-10 | 格式错误的 YAML | 加载 frontmatter YAML 语法错误的 SKILL.md | 优雅返回解析错误，不崩溃 | P0 |

### V1-B: 渐进式加载（L1/L2/L3）（5 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V1-11 | L1 索引构建 | `build_index()` 加载 10 个 Skills | 仅加载 name + description（~100 tokens/skill），不加载 body | P0 |
| V1-12 | L2 按需激活 | 对已索引的 Skill 调用 `load_skill(name)` | 完整 body 加载，包含 instructions | P0 |
| V1-13 | L3 脚本按需加载 | L2 加载后，引用 scripts/ 中的脚本 | 脚本内容加载并可执行 | P0 |
| V1-14 | 未激活 Skill 的 body 不泄露 | 查询未激活 Skill 的 body | 返回 None 或 body 未加载标记 | P1 |
| V1-15 | 多 Skill 并行加载性能 | 同时加载 50 个 L1 Skills | 加载时间 < 100ms，内存占用合理 | P2 |

### V1-C: 信任与权限（5 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V1-16 | allowed-tools 格式验证 | 加载含 `allowed-tools: bash file_read` 的 Skill | 解析成功，工具列表正确 | P0 |
| V1-17 | allowed-tools 运行时拦截 | 激活含 `allowed-tools: file_read` 的 Skill，尝试调用 `bash` | **调用被拦截/拒绝**（当前缺失：P0 CRITICAL） | P0 |
| V1-18 | 无 allowed-tools 的 Skill | 激活无 allowed-tools 限制的 Skill | 所有工具可用（无限制） | P0 |
| V1-19 | TrustLevel 衰减 | Unknown 来源的 Skill 尝试执行脚本 | 根据 TrustLevel 限制执行权限 | P1 |
| V1-20 | Skill 异常行为检测 | Skill 指令尝试 prompt injection | 检测并报告异常行为 | P2 |

### V1-D: Skill 运行时（5 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V1-21 | SkillTool 执行连接 | SkillTool.execute() 调用含脚本的 Skill | **脚本通过 SkillRuntimeBridge 执行**（当前缺失：P0 CRITICAL） | P0 |
| V1-22 | Python 脚本执行 | 通过 PythonRuntime 执行 Skill 脚本 | Python 脚本执行并返回结果 | P0 |
| V1-23 | Shell 脚本执行 | 通过 ShellRuntime 执行 Skill 脚本 | Shell 脚本执行并返回结果（当前 stub） | P1 |
| V1-24 | 脚本执行超时 | 执行耗时超过限制的脚本 | 超时终止，返回超时错误 | P0 |
| V1-25 | 脚本输出截断 | 脚本输出超过最大长度 | 输出截断到合理大小 + 截断标记 | P1 |

---

## 3. V2: MCP 集成验证

> 对标：MCP 规范 2025-11-25、Everything Server、Filesystem Server、主流 MCP 服务器

### V2-A: 服务器生命周期（7 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V2-01 | Stdio 服务器启动 | 启动 `@modelcontextprotocol/server-filesystem` | 服务器进程启动，连接建立 | P0 |
| V2-02 | SSE 服务器连接 | 连接 Everything Server (SSE 模式) | SSE 连接建立，心跳正常 | P1 |
| V2-03 | 服务器优雅停止 | 停止已运行的 MCP 服务器 | 进程正常退出，资源释放 | P0 |
| V2-04 | 服务器异常退出 | Kill MCP 服务器进程 | 检测到断连，错误处理正常 | P0 |
| V2-05 | 多服务器并发管理 | 同时启动 Filesystem + Memory + Everything | 三个服务器独立运行，互不干扰 | P0 |
| V2-06 | 服务器配置持久化 | 添加服务器配置，重启后验证 | 配置从 SQLite 恢复 | P1 |
| V2-07 | 服务器不存在 | 配置不存在的命令路径 | 返回清晰的启动失败错误 | P0 |

### V2-B: 工具发现与调用（7 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V2-08 | 工具列表发现 | 连接 Everything Server，调用 `list_tools` | 返回 15+ 工具的 schema 列表 | P0 |
| V2-09 | 简单工具调用 | 调用 Everything Server 的 `echo` 工具 | 返回回显内容 | P0 |
| V2-10 | 带参数工具调用 | 调用 `get-sum` 工具，参数 `{a: 3, b: 7}` | 返回结果 `10` | P0 |
| V2-11 | 无效参数工具调用 | 调用 `get-sum` 传入字符串参数 | 返回参数验证错误 | P0 |
| V2-12 | 不存在的工具调用 | 调用名为 `nonexistent_tool` 的工具 | 返回 tool not found 错误 | P0 |
| V2-13 | 长时间运行工具 | 调用 Everything 的 `trigger-long-running-operation` | 返回进度通知 + 最终结果 | P1 |
| V2-14 | 二进制内容工具结果 | 调用 `get-tiny-image` | 返回含图像的 base64 内容 | P1 |

### V2-C: 资源与提示词（6 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V2-15 | 资源列表 | 调用 Everything Server 的 `list_resources` | 返回分页资源列表（10/页） | P1 |
| V2-16 | 资源读取 | 按 URI 读取具体资源 | 返回资源内容 | P1 |
| V2-17 | 资源分页 | 请求第二页资源 | 返回下一批资源 | P2 |
| V2-18 | 提示词列表 | 调用 `list_prompts` | 返回 3 个提示词模板 | P1 |
| V2-19 | 带参数的提示词 | 获取 `complex_prompt`，传入 `temperature=0.7` | 返回渲染后的提示词文本 | P1 |
| V2-20 | 缺失必填参数的提示词 | 获取 `complex_prompt` 不传参数 | 返回参数缺失错误 | P1 |

---

## 4. V3: 内置工具验证

> 对标：Claude Code 内置工具、SWE-agent 工具集、Codex CLI

### V3-A: 文件操作工具（12 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V3-01 | file_read 基本读取 | 读取已存在的文件 | 返回带行号的文件内容 | P0 |
| V3-02 | file_read 分页 | `offset=2, limit=1` 读取 | 只返回第 2 行 | P0 |
| V3-03 | file_read 文件不存在 | 读取不存在的路径 | 返回 file not found 错误 | P0 |
| V3-04 | file_read 路径穿越 | 读取 `../../../etc/passwd` | 被路径验证拦截 | P0 |
| V3-05 | file_write 创建新文件 | 写入不存在路径 | 文件创建，内容正确 | P0 |
| V3-06 | file_write 自动建目录 | 写入深层嵌套路径 | 父目录自动创建 | P0 |
| V3-07 | file_write 覆盖已有文件 | 写入已存在的文件 | 内容被覆盖 | P0 |
| V3-08 | file_edit 单次替换 | `old_string` 唯一匹配时替换 | 替换成功，其他内容不变 | P0 |
| V3-09 | file_edit 歧义检测 | `old_string` 匹配多次（无 replace_all） | 返回错误：found N occurrences | P0 |
| V3-10 | file_edit 全部替换 | `replace_all=true` | 所有匹配项被替换 | P0 |
| V3-11 | file_edit 未找到匹配 | `old_string` 不存在 | 返回 not found 错误 | P0 |
| V3-12 | glob 模式匹配 | `**/*.rs` 匹配 Rust 文件 | 返回按 mtime 排序的文件列表 | P0 |

### V3-B: 命令执行工具（8 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V3-13 | bash 基本执行 | `echo hello` | 返回 `hello\n` | P0 |
| V3-14 | bash 非零退出码 | `exit 1` | 返回 exit code 1 | P0 |
| V3-15 | bash 超时终止 | `sleep 999`，timeout=2 | 超时错误 | P0 |
| V3-16 | bash Allowlist 拦截 | Allowlist 模式下执行 `rm -rf /` | 安全拦截 | P0 |
| V3-17 | bash 元字符拦截 | Allowlist 模式下 `echo a; echo b` | 拦截分号 | P0 |
| V3-18 | bash 路径穿越拦截 | `cat ../../../etc/passwd` | 路径穿越拦截 | P0 |
| V3-19 | bash 环境变量过滤 | 输出 `$SECRET_KEY` | 敏感环境变量不泄露 | P1 |
| V3-20 | bash 输出截断 | 产生超过 100KB 输出 | 输出截断 + 截断标记 | P1 |

### V3-C: 搜索工具（5 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V3-21 | grep 基本搜索 | 搜索 `fn main` 在 `*.rs` | 返回匹配行+文件路径+行号 | P0 |
| V3-22 | grep 无匹配 | 搜索不存在的模式 | 返回 no matches | P0 |
| V3-23 | grep 无效正则 | `[invalid regex` | 返回正则错误 | P0 |
| V3-24 | glob 无匹配 | `**/*.nonexistent` | 返回 no files found | P0 |
| V3-25 | glob 无效模式 | `[invalid` | 返回模式错误 | P1 |

### V3-D: 网络工具（5 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V3-26 | web_fetch 基本获取 | fetch `https://httpbin.org/get` | 返回 JSON 响应 | P0 |
| V3-27 | web_fetch SSRF 回环拦截 | fetch `http://127.0.0.1:8080` | 被 SSRF 保护拦截 | P0 |
| V3-28 | web_fetch SSRF 云元数据 | fetch `http://169.254.169.254/...` | 被 SSRF 保护拦截 | P0 |
| V3-29 | web_fetch 非 HTTP 协议 | fetch `ftp://example.com` | 被协议检查拦截 | P0 |
| V3-30 | web_fetch 内容截断 | fetch 大页面，max_length=100 | 内容截断到 100 字符 | P1 |

---

## 5. V4: Agent Loop 与 Provider 验证

> 对标：重构计划 P0-P3、Agent Harness 最佳实践

### V4-A: AgentLoop 核心流程（8 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V4-01 | 基本对话轮次 | 发送用户消息，Agent 响应 | 返回 LLM 文本响应 | P0 |
| V4-02 | 工具调用轮次 | 发送需要工具调用的请求 | Agent 调用工具 → 获取结果 → 生成最终响应 | P0 |
| V4-03 | 多轮工具调用 | 需要 2+ 次工具调用的任务 | Agent 正确链式调用多个工具 | P0 |
| V4-04 | 迭代限制 | 设置 max_iterations=2 | 达到限制后停止，返回 StopReason | P0 |
| V4-05 | 取消令牌 | 运行中发送取消信号 | AgentLoop 优雅停止 | P0 |
| V4-06 | LoopGuard 检测 | 连续调用相同工具+相同参数 | 检测到工具循环，中断 | P0 |
| V4-07 | AgentLoopConfig 构建 | 通过 Builder 构建配置 | 所有 17+ 参数正确设置 | P0 |
| V4-08 | AgentEvent 流输出 | 订阅 AgentEvent 流 | 收到 TurnStart, ToolUse, ToolResult, TurnEnd 事件 | P0 |

### V4-B: Provider 验证（7 案例）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V4-09 | Anthropic Provider | 使用 Anthropic API 发送请求 | 返回 Claude 响应 | P0 |
| V4-10 | OpenAI Provider | 使用 OpenAI API 发送请求 | 返回 GPT 响应 | P0 |
| V4-11 | Provider 流式输出 | 开启 streaming | 逐 token 接收响应 | P0 |
| V4-12 | ProviderChain 故障转移 | 主 Provider 返回错误 | 自动切换到备用 Provider | P1 |
| V4-13 | 重试策略 | Provider 返回 429 | 指数退避重试 | P0 |
| V4-14 | Token 计量 | 发送请求并追踪 token 使用 | 记录 input/output tokens | P0 |
| V4-15 | API Key 缺失 | 未配置 API Key | 返回清晰的配置错误 | P0 |

---

## 6. V5: 内存系统验证

> 对标：多层内存架构（L0-L2 + KG + FTS）

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V5-01 | memory_store 基本存储 | 存储一条记忆 | 返回存储 ID | P0 |
| V5-02 | memory_search 基本搜索 | 存储后搜索相关关键词 | 返回匹配的记忆条目 | P0 |
| V5-03 | memory_recall 按 ID 召回 | 存储后按 ID 召回 | 返回完整内容 | P0 |
| V5-04 | memory_update 更新 | 存储后更新内容 | 再次召回返回更新后的内容 | P0 |
| V5-05 | memory_forget 删除 | 存储后删除 | 再次召回返回 not found | P0 |
| V5-06 | 空内容存储 | 存储空字符串 | 返回验证错误 | P0 |
| V5-07 | 搜索结果限制 | `limit=1` 搜索 | 最多返回 1 条结果 | P1 |
| V5-08 | 全文搜索（FTS） | 存储含特定关键词的文本，搜索该关键词 | FTS 引擎返回精确匹配 | P1 |
| V5-09 | Knowledge Graph 实体创建 | 创建实体 + 关系 | 实体和关系持久化 | P1 |
| V5-10 | Knowledge Graph 查询 | 查询实体的关系 | 返回正确的关系图 | P1 |
| V5-11 | 混合搜索（BM25 + 向量） | 使用 HybridQueryEngine 搜索 | 综合排序结果 | P2 |
| V5-12 | 跨 Session 持久化 | Session 1 存储，Session 2 搜索 | 跨会话检索成功 | P0 |

---

## 7. V6: 安全与审计验证

> 对标：IronClaw SafetyLayer、ToolEmu 安全评估

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V6-01 | 命令风险评估 | 评估 `rm -rf /` 的风险等级 | 返回 Critical/High 风险 | P0 |
| V6-02 | 自治级别执行 | Low 自治级别下执行高风险命令 | 命令被拒绝 | P0 |
| V6-03 | 路径验证 | PathValidator 检查 `../../../etc/passwd` | 返回路径不安全 | P0 |
| V6-04 | AI Defence 注入检测 | 输入含 prompt injection 的文本 | InjectionDetector 报告威胁 | P0 |
| V6-05 | AI Defence 输出验证 | 输出含敏感信息 | OutputValidator 标记问题 | P1 |
| V6-06 | PII 扫描 | 扫描含邮箱、电话号码的文本 | PiiScanner 检测到 PII | P1 |
| V6-07 | 审计事件记录 | 执行工具调用 | AuditStorage 记录审计事件 | P0 |
| V6-08 | 审计日志查询 | 查询最近的审计记录 | 返回带时间戳的事件列表 | P1 |
| V6-09 | ToolCallInterceptor | 注册拦截器，拦截特定工具调用 | 工具调用被拦截/修改 | P0 |
| V6-10 | TurnGate 并发控制 | 同时发起多个 AgentLoop 轮次 | TurnGate 限制并发数 | P1 |
| V6-11 | Secret 文件访问拒绝 | 尝试读取 `.env` 文件 | 被安全策略拒绝 | P0 |
| V6-12 | ExecPolicy Deny 模式 | Deny 模式下执行任何命令 | 全部被拒绝 | P0 |
| V6-13 | ExecPolicy Allowlist | Allowlist 含 `ls,cat`，执行 `ls` | `ls` 允许 | P0 |
| V6-14 | ExecPolicy Allowlist 未列出 | Allowlist 含 `ls`，执行 `rm` | `rm` 被拒绝 | P0 |
| V6-15 | 操作追踪 | ActionTracker 记录连续操作 | 操作历史正确记录 | P1 |

---

## 8. V7: Session 与配置验证

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V7-01 | Session 创建 | `octo session create` | 返回新 Session ID | P0 |
| V7-02 | Session 列表 | `octo session list` | 返回所有 Session | P0 |
| V7-03 | Session 详情 | `octo session show <id>` | 返回 Session 元数据 | P0 |
| V7-04 | Session 消息存储 | 向 Session 推送消息 | 消息持久化到 SQLite | P0 |
| V7-05 | Session 消息检索 | 检索 Session 的消息历史 | 返回按时间排序的消息 | P0 |
| V7-06 | Config 显示 | `octo config show` | 显示当前配置（YAML 格式） | P0 |
| V7-07 | Config 验证 | `octo config validate` | 验证配置文件合法性 | P0 |
| V7-08 | Config 环境变量覆盖 | 设置 `OCTO_PORT=4000` | 端口配置被环境变量覆盖 | P0 |
| V7-09 | Agent 列表 | `octo agent list` | 返回已注册的 Agent 列表 | P0 |
| V7-10 | Agent 详情 | `octo agent info <id>` | 返回 Agent manifest 信息 | P0 |

---

## 9. V8: Hook 系统验证

> 对标：10 个生命周期 Hook 点、HookAction 控制流

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V8-01 | PreToolUse Hook 触发 | 注册 PreToolUse Handler，执行工具 | Handler 在工具执行前触发 | P0 |
| V8-02 | PostToolUse Hook 触发 | 注册 PostToolUse Handler，执行工具 | Handler 在工具完成后触发 | P0 |
| V8-03 | Hook Abort 动作 | PreToolUse 返回 HookAction::Abort | 工具执行被中止，AgentLoop 停止 | P0 |
| V8-04 | Hook Block 动作 | PreToolUse 返回 HookAction::Block | 工具执行被软拒绝，继续运行 | P0 |
| V8-05 | SessionStart Hook | 创建新 Session | SessionStart Hook 触发 | P1 |
| V8-06 | SessionEnd Hook | 结束 Session | SessionEnd Hook 触发 | P1 |
| V8-07 | ContextDegraded Hook | 上下文超出预算 | ContextDegraded Hook 触发 | P1 |
| V8-08 | PreTask Hook | 开始新任务/轮次 | PreTask Hook 触发 | P1 |
| V8-09 | PostTask Hook | 完成任务/轮次 | PostTask Hook 触发 | P1 |
| V8-10 | 多 Hook 链式执行 | 注册多个 PreToolUse Handler | 按注册顺序依次执行 | P1 |

---

## 10. V9: Context 管理验证

> 对标：4+1 层预算管理、Zone A/B 架构

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| V9-01 | Token 预算计算 | 估算消息列表的 token 数 | 返回合理的 token 估算值 | P0 |
| V9-02 | 上下文降级触发 | 消息超出预算阈值 | 触发 Soft → Auto 降级 | P0 |
| V9-03 | 上下文裁剪 | ContextPruner 处理超长上下文 | 保留关键消息，裁剪冗余 | P0 |
| V9-04 | 系统提示构建 | SystemPromptBuilder 构建提示 | 包含 Zone A（静态）+ Zone B（动态） | P0 |
| V9-05 | 内存冲刷 | MemoryFlusher 在降级时触发 | 关键事实提取并存入长期内存 | P1 |
| V9-06 | Bootstrap 文件注入 | 配置 Bootstrap 文件 | 系统提示包含 Bootstrap 内容 | P1 |
| V9-07 | Emergency 降级 | 上下文极度超出 | Emergency 级别降级 + 激进裁剪 | P1 |
| V9-08 | 动态上下文注入 | loop_steps 的 inject_dynamic_context | 正确注入 Skill instructions + 工具列表 | P1 |

---

## 11. 推荐的 MCP 测试服务器

### Tier 1：必须使用（覆盖所有 MCP 特性，无需 API Key）

| 服务器 | 包名 | 传输 | 特性 | 安装命令 |
|--------|------|------|------|----------|
| **Everything** | `@modelcontextprotocol/server-everything` | stdio/SSE/HTTP | 工具+资源+提示词+采样+日志+分页+订阅 | `npx -y @modelcontextprotocol/server-everything` |
| **Filesystem** | `@modelcontextprotocol/server-filesystem` | stdio | 12 个文件操作工具 | `npx -y @modelcontextprotocol/server-filesystem /tmp` |
| **Memory** | `@modelcontextprotocol/server-memory` | stdio | 知识图谱持久化记忆 | `npx -y @modelcontextprotocol/server-memory` |

### Tier 2：推荐使用（额外覆盖）

| 服务器 | 包名 | 传输 | 特性 | 安装命令 |
|--------|------|------|------|----------|
| **Fetch** (Python) | `mcp-server-fetch` | stdio | URL 获取 + HTML→Markdown | `uvx mcp-server-fetch` |
| **SQLite** (Python) | `mcp-server-sqlite` | stdio | 工具+资源组合 | `uvx mcp-server-sqlite --db-path /tmp/test.db` |

### Tier 3：可选（深度测试）

| 服务器 | 来源 | 传输 | 特性 |
|--------|------|------|------|
| **GitHub** (Go) | `github/github-mcp-server` | stdio | 非 Node.js 服务器，大量工具，需 Token |
| **Sequential Thinking** | `@modelcontextprotocol/server-sequential-thinking` | stdio | 动态思维链 |

### MCP 特性覆盖矩阵

| 特性 | Everything | Filesystem | Memory | Fetch | SQLite |
|------|:---:|:---:|:---:|:---:|:---:|
| Tools | + | + | + | + | + |
| Resources | + | - | - | - | + |
| Prompts | + | - | - | - | - |
| 分页 | + | - | - | - | - |
| 订阅 | + | - | - | - | - |
| Stdio | + | + | + | + | + |
| SSE | + | - | - | - | - |
| Streamable HTTP | + | - | - | - | - |

---

## 12. 主流 Agent Skills 清单

> 以下为可部署到 octo-engine 的主流第三方 Skills。按 agentskills.io 标准（SKILL.md 格式），安装后验证正常加载和执行。

### 12.1 推荐部署验证的 Skills（按优先级）

#### Tier 1：核心验证（验证 Skills 模块基本功能）

| 来源 | Skill 数 | 安装命令 | 验证重点 |
|------|---------|----------|----------|
| **Anthropic 官方** | 17 | `npx skills add anthropics/skills` | SKILL.md 解析、L1/L2 加载、模板变量 |
| **Trail of Bits 安全** | 35 | `npx skills add trailofbits/skills` | allowed-tools 验证、安全审计工作流 |
| **Vercel Labs** | 8+ | `npx skills add vercel-labs/agent-skills` | React/Next.js 最佳实践指令注入 |

#### Tier 2：扩展验证（验证多类别 Skill 兼容性）

| 来源 | Skill 数 | 安装命令 | 验证重点 |
|------|---------|----------|----------|
| **Sentry** | 6+ | `npx skills add getsentry/sentry-agent-skills` | 调试/错误修复工作流 |
| **Cloudflare** | 7 | `npx skills add cloudflare/<repo>` | DevOps/部署类 Skill |
| **HashiCorp** | 3 | `npx skills add hashicorp/<repo>` | IaC 代码生成 Skill |
| **Hugging Face** | 8 | `npx skills add huggingface/<repo>` | ML/AI 训练类 Skill |

#### Tier 3：全面验证（验证大规模 Skill 管理）

| 来源 | Skill 数 | 安装命令 | 验证重点 |
|------|---------|----------|----------|
| **Microsoft/Azure** | 127+ | `npx skills add microsoft/skills` | 大量 Skill 并行加载性能 |
| **Google Workspace** | 26 | `npx skills add googleworkspace/<repo>` | 生产力工具类 Skill |
| **Netlify** | 12 | `npx skills add netlify/<repo>` | Serverless/Edge Skill |
| **GitHub awesome-copilot** | 10+ | `npx skills add github/awesome-copilot` | 含 Rust MCP 生成器 Skill |

### 12.2 完整 Skills 清单（280+ Skills）

#### Anthropic 官方（17 个）

| Skill | 类别 | 描述 |
|-------|------|------|
| `algorithmic-art` | 创意/设计 | 用 p5.js 创建生成式艺术 |
| `brand-guidelines` | 企业/通信 | 应用 Anthropic 品牌规范 |
| `canvas-design` | 创意/设计 | PNG/PDF 视觉设计 |
| `claude-api` | 开发 | Claude API 参考（Python/TS/Java/Go/Ruby/C#/PHP），自动激活 |
| `doc-coauthoring` | 文档 | 协作文档编辑 |
| `docx` | 文档 | Word 文档创建/编辑/分析 |
| `frontend-design` | 设计/UI | 前端设计和 UI/UX 开发 |
| `internal-comms` | 企业/通信 | 状态报告和新闻稿 |
| `mcp-builder` | 开发 | 为外部 API 创建 MCP 服务器 |
| `pdf` | 文档 | PDF 文本提取/创建/表单 |
| `pptx` | 文档 | PowerPoint 创建/编辑/分析 |
| `skill-creator` | 元工具 | 创建新 Skill 的指南 |
| `slack-gif-creator` | 创意/设计 | Slack 优化动图创建 |
| `template` | 元工具 | 新 Skill 基础模板 |
| `theme-factory` | 设计/UI | 专业主题样式 |
| `web-artifacts-builder` | 开发 | 用 React 构建复杂 HTML 工件 |
| `webapp-testing` | 测试 | Playwright 本地 Web 测试 |
| `xlsx` | 数据/分析 | Excel 创建/编辑/分析 |

#### Trail of Bits 安全（35 个）

| Skill | 类别 | 描述 |
|-------|------|------|
| `agentic-actions-auditor` | 安全/审计 | 审计 AI 操作安全性 |
| `audit-context-building` | 安全/审计 | 深度架构代码安全分析 |
| `building-secure-contracts` | 安全/区块链 | 6 个区块链的智能合约安全 |
| `constant-time-analysis` | 安全/密码 | 常量时间执行分析 |
| `differential-review` | 代码审查 | Diff 安全审查 |
| `entry-point-analyzer` | 安全/区块链 | 智能合约入口点分析 |
| `firebase-apk-scanner` | 安全/移动 | APK Firebase 安全扫描 |
| `fp-check` | 安全/分析 | 安全发现误报验证 |
| `gh-cli` | Git/GitHub | GitHub CLI 工作流 |
| `insecure-defaults` | 安全/审计 | 不安全默认配置检测 |
| `modern-python` | 开发 | 现代 Python 最佳实践 |
| `property-based-testing` | 测试 | 属性基测试方法论 |
| `semgrep-rule-creator` | 安全/SAST | 创建 Semgrep 静态分析规则 |
| `sharp-edges` | 安全/审计 | 危险 API 识别 |
| `static-analysis` | 安全/SAST | 静态分析工作流编排 |
| `supply-chain-risk-auditor` | 安全/供应链 | 软件供应链风险审计 |
| `variant-analysis` | 安全/审计 | 已知漏洞变体发现 |
| `yara-authoring` | 安全/恶意软件 | YARA 规则编写 |
| `zeroize-audit` | 安全/密码 | 内存密钥清零审计 |
| *(+ 16 更多)* | | |

#### 其他主要来源汇总

| 来源 | Skill 数 | 主要类别 |
|------|---------|----------|
| Vercel Labs | 8+ | React、Next.js、Web 设计、部署 |
| Cloudflare | 7 | Workers、AI Agents、MCP、性能 |
| Netlify | 12 | Serverless、Edge、数据库、CDN |
| Google Workspace | 26 | Drive、Sheets、Gmail、Calendar、Admin |
| Google Labs (Stitch) | 6 | 设计到代码、UI 组件 |
| Sentry | 6+ | 错误监控、调试、代码审查 |
| Hugging Face | 8 | ML 训练、数据集、评估 |
| HashiCorp | 3 | Terraform IaC |
| Stripe | 2 | 支付 API 集成 |
| Expo | 3 | 移动开发 |
| Microsoft/Azure | 127+ | 云服务、AI、DevOps、身份认证 |
| GitHub awesome-copilot | 10+ | SQL 优化、MCP 生成器、测试 |
| Supabase/Neon/ClickHouse | 3 | 数据库最佳实践 |
| Remotion | 1 | 程序化视频创建 |
| Replicate | 1 | AI 模型运行 |
| Composio | 1+ | 1000+ 外部应用连接 |

### 12.3 Skills 部署验证案例

| ID | 名称 | 操作 | 期望结果 | 优先级 |
|----|------|------|----------|--------|
| VS-01 | Anthropic Skills 批量安装 | `npx skills add anthropics/skills`，加载全部 17 个 | 所有 SKILL.md 解析成功，L1 索引建立 | P0 |
| VS-02 | 单个 Skill 加载 | 加载 `claude-api` Skill | L2 body 加载，template 变量替换正确 | P0 |
| VS-03 | 安全 Skill allowed-tools | 加载 Trail of Bits `static-analysis` | allowed-tools 字段正确解析和执行 | P0 |
| VS-04 | 大规模 Skill 加载 | 同时加载 50+ Skills（Microsoft + Google） | L1 加载 < 200ms，内存占用合理 | P1 |
| VS-05 | 跨平台 Skill 兼容 | 加载 Codex CLI 格式的 Skill | 格式兼容，正确解析 | P1 |
| VS-06 | Skill 热重载 | 修改已加载 Skill 的 SKILL.md | 自动检测变化，重新加载 | P1 |
| VS-07 | Skill 冲突检测 | 加载两个同名 Skill | 报告冲突，按优先级解决 | P2 |
| VS-08 | 无效 Skill 容错 | 批量加载含 1 个无效 Skill | 无效的跳过，其余正常加载 | P0 |

---

## 13. 行业基准实测指南

> 通过运行权威基准测试，量化 octo-engine 与主流自主智能体的差距，确定演示和改进方向。

### 13.1 基准采纳路线图

#### Phase 1：快速验证（1-2 周）

| 基准 | 集成难度 | 目标分数 | 成本 | 意义 |
|------|---------|---------|------|------|
| **HumanEval+** | **EASY** | pass@1 > 85% | ~$5 | 代码生成基础能力 |
| **Tau-bench** | **EASY-MEDIUM** | retail pass^1 > 70% | ~$40 | 工具调用可靠性（与 octo-engine JSON Schema 天然对齐） |

#### Phase 2：竞争力验证（3-6 周）

| 基准 | 集成难度 | 目标分数 | 成本 | 意义 |
|------|---------|---------|------|------|
| **SWE-bench Verified** | **MEDIUM** | > 50% (500 题) | ~$300 | **金标准**，得分即竞争力 |
| **Terminal-Bench 2.0** | **MEDIUM** | > 40% | ~$100 | CLI 智能体定位的直接证明 |

#### Phase 3：差异化验证（2-3 月）

| 基准 | 集成难度 | 目标分数 | 成本 | 意义 |
|------|---------|---------|------|------|
| **Tau2-bench** | **EASY-MEDIUM** | retail pass^1 > 80% | ~$200 | 工具使用一致性 |
| **ToolEmu** | **MEDIUM** | 低于 GPT-4 违规率 | ~$50 | 安全能力（企业定位） |
| **GAIA** | **HARD** | overall > 40% | ~$200 | 通用智能体完整性 |

### 13.2 各基准详细实测方案

#### HumanEval+（最先上手）

**安装与运行**：
```bash
pip install evalplus

# 通过 OpenAI 兼容 API 运行（octo-engine 需暴露兼容端点）
evalplus.evaluate --model "octo-engine" --dataset humaneval --backend openai --greedy

# 或生成 JSONL 后离线评估
evaluate_functional_correctness samples.jsonl
```

**octo-engine 集成**：50 行 Python 适配器，将 prompt 转发到 Rust 引擎，捕获 completion
**集成难度**：EASY — 纯代码生成，无工具调用

**SOTA 对比**：

| 模型 | HumanEval pass@1 | HumanEval+ pass@1 |
|------|-----------------|-------------------|
| o1-mini | 96.2% | ~90%+ |
| Claude Sonnet 4 | ~92% | ~87%+ |
| GPT-4o | 90.2% | ~85%+ |
| **竞争力线** | **> 85%** | **> 80%** |

#### Tau-bench（与 octo-engine 最对齐）

**安装与运行**：
```bash
git clone https://github.com/sierra-research/tau-bench.git
cd tau-bench && pip install -e .

# 最小测试：5 个任务，1 次试验
python run.py --env retail --model your-model --num-trials 1 --num-tasks 5

# Tau2-bench
git clone https://github.com/sierra-research/tau2-bench.git
cd tau2-bench && pip install -e .
tau2 run --num-tasks 5 --num-trials 1
```

**octo-engine 集成**：~100 行 Python，暴露为 LiteLLM 兼容端点（JSON Schema function calling 直接对齐）
**集成难度**：EASY-MEDIUM — 工具定义格式与 octo-engine 的 `Tool` trait 完美匹配

**SOTA 对比**：

| 模型 | Retail pass^1 | Telecom pass^1 |
|------|--------------|----------------|
| Claude Sonnet 4.6 | **91.7%** | **97.9%** |
| Claude Opus 4.6 | 93.5% | 97.9% |
| **竞争力线** | **> 70%** | **> 80%** |

#### SWE-bench Verified（金标准）

**安装与运行**：
```bash
# 方式 A：Cloud 评估（推荐起步）
pip install sb-cli
sb-cli gen-api-key your@email.com
export SWEBENCH_API_KEY=your_key

# 最小测试：5 个实例
sb-cli submit swe-bench_lite dev --predictions_path test.json --instance_ids "instance1,instance2"

# 方式 B：本地 Docker 评估
git clone https://github.com/SWE-bench/SWE-bench.git
cd SWE-bench && pip install -e .
# 需 Docker + 50-100GB 磁盘
```

**octo-engine 集成**：~200 行 Python 适配器
1. 读取 issue 描述 + repo 上下文
2. 调用 Rust 引擎（通过 subprocess 或 HTTP）
3. 引擎使用 bash + file_edit 工具解决问题
4. 输出 unified diff patch → 格式化为 JSON

**集成难度**：MEDIUM — 需要 Python 薄包装层

**SOTA 对比**：

| 模型 + Agent | 分数 |
|-------------|------|
| Claude Opus 4.6 + mini-SWE-agent | **79.2%** |
| Claude Sonnet 4.6 + mini-SWE-agent | 79.6% |
| Gemini 3 Flash + mini-SWE-agent | 75.2% |
| **竞争力线** | **> 50%** |
| **优秀线** | **> 60%** |

#### Terminal-Bench 2.0（CLI 智能体最相关）

**安装与运行**：
```bash
pip install terminal-bench

# 最小测试
tb run --dataset terminal-bench-core==head --agent terminus \
  --model anthropic/claude-sonnet-4-20250514 --task-id hello-world

# 完整测试（需 Docker）
harbor run -d terminal-bench@2.0 -a your-agent --env local -n 32
```

**octo-engine 集成**：~150 行 Python，实现 `BaseAgent` 接口
```python
class OctoAgent(BaseAgent):
    async def run(self, instruction, environment, context):
        # 调用 octo-engine Rust 二进制，通过 shell 执行命令
```

**集成难度**：MEDIUM — 需 Python BaseAgent shim

**SOTA 对比**：

| 模型 | 分数 |
|------|------|
| Claude Opus 4.6 | **62.7%** |
| Claude Sonnet 4.6 | 59.1% |
| GPT-5.2 (high) | 46.7% |
| **竞争力线** | **> 40%** |

#### ToolEmu（安全验证）

**安装与运行**：
```bash
git clone https://github.com/ryoungj/ToolEmu.git
git clone https://github.com/ryoungj/PromptCoder.git
cd PromptCoder && pip install -e . && cd ..
cd ToolEmu && pip install -e .

# 最小测试：单个 toolkit 的 4-5 个案例
python scripts/run.py
```

**octo-engine 集成**：~200 行 Python，暴露为 OpenAI 兼容 API
**集成难度**：MEDIUM — 需工具格式转换
**评估**：LLM-as-Judge 评估安全违规率，目标低于 GPT-4 基线（~20-30% 违规率）

### 13.3 通用集成架构

所有基准共享一个适配器模式：

```
[基准 Harness (Python)]
    ↓
[Python Adapter / Shim] ← 50-200 行
    ↓ HTTP API 或 subprocess
[octo-engine (Rust)]
    ↓ JSON Schema function calling
[LLM Provider (Anthropic/OpenAI)]
```

### 13.4 竞争力分数速查表

| 基准 | "合格" | "竞争力" | "顶尖" |
|------|--------|---------|--------|
| HumanEval+ pass@1 | > 80% | > 88% | > 92% |
| Tau-bench retail pass^1 | > 60% | > 75% | > 85% |
| SWE-bench Verified | > 45% | > 60% | > 70% |
| Terminal-Bench 2.0 | > 35% | > 45% | > 55% |
| GAIA overall | > 40% | > 55% | > 65% |

---

## 14. octo-engine 差距分析

### 对比主流框架的能力差距

| 基准 | 领域 | 与 octo-engine 关联度 | 验证方式 |
|------|------|----------------------|----------|
| **SWE-bench Verified** | 软件工程（500 GitHub Issues） | **高** — 直接测试文件编辑+bash+搜索 | 补丁生成 → 单元测试通过 |
| **Terminal-Bench 2.0** | 终端/CLI 任务 | **高** — 测试 bash 工具可靠性 | Shell 输出状态比较 |
| **Tau-bench** | 客户服务（工具+数据库） | **高** — 测试工具调用可靠性+策略遵循 | 数据库状态比较 |
| **ToolEmu** | 工具使用安全评估 | **高** — 直接测试工具安全边界 | LLM-as-Judge 安全评分 |
| **AgentBench** | 多领域（8 环境） | **中高** — OS/DB 环境直接测试 | 环境特定状态比较 |
| **GAIA** | 通用助手（多模态） | **中** — 测试工具编排 | 精确匹配答案 |
| **HumanEval** | 代码生成 | **低中** — 测试代码生成，非工具使用 | 测试用例通过 |

### 推荐内部基准

基于行业基准和 octo-engine 特点，推荐构建以下内部验证基准：

1. **Tool Accuracy Benchmark**: 50 个工具调用场景，验证正确率
2. **Safety Boundary Benchmark**: 30 个安全边界场景，验证拦截率
3. **Skill Compliance Benchmark**: 25 个 agentskills.io 合规场景
4. **MCP Integration Benchmark**: 20 个 MCP 协议交互场景
5. **Memory Reliability Benchmark**: 12 个存取一致性场景

---

| 差距 | 当前状态 | 目标状态 | 优先级 | 关联重构任务 |
|------|----------|----------|--------|-------------|
| **SkillTool↔SkillRuntimeBridge 断联** | SkillTool.execute() 仅返回 body 文本 | 连接到 SkillRuntimeBridge 执行脚本 | **P0 CRITICAL** | P0-5 |
| **allowed-tools 运行时不执行** | 仅在加载时验证格式 | AgentLoop 中运行时拦截 | **P0 CRITICAL** | P0-7 (ToolCallInterceptor) |
| **NodeJS/Shell/WASM 运行时** | 类型 stub | 完整实现 | P1 | P2 阶段 |
| **SKILL.md spec 字段缺失** | 缺 license, compatibility, metadata | 完整支持 agentskills.io 字段 | P1 | P0-5 |
| **name 正则验证** | 不验证格式规范 | 按 spec 验证（a-z0-9-，≤64字符） | P1 | P0-5 |
| **Grep 输出模式** | 仅 content 模式 | 支持 files_with_matches, count, context lines | P1 | 工具增强 |
| **MultiEdit** | 不支持 | 单次调用多处编辑 | P2 | 工具增强 |
| **背景命令执行** | 不支持 | bash `run_in_background` 参数 | P2 | 工具增强 |
| **语义代码搜索** | 仅正则 grep | 向量嵌入+语义搜索 | P2 | 未来规划 |
| **MCP Streamable HTTP** | 映射到 SSE | 独立支持 Streamable HTTP 传输 | P2 | MCP 增强 |

### 验证案例与重构计划关联

| 重构任务 | 关联验证案例 | 验证数 |
|----------|-------------|--------|
| P0-1 AgentLoopConfig | V4-07 | 1 |
| P0-2 Step Functions | V4-06, V9-08 | 2 |
| P0-3 AgentEvent | V4-08, V8-01~V8-10 | 11 |
| P0-4 SkillDefinition | V1-01~V1-10 | 10 |
| P0-5 SkillTool 修复 | V1-21~V1-25 | 5 |
| P0-6 TrustManager | V1-16~V1-20 | 5 |
| P0-7 ToolCallInterceptor | V6-09, V1-17 | 2 |
| P0-8 TurnGate | V6-10 | 1 |
| P0-9 错误不持久化 | V4-13 | 1 |
| P0-10 ProviderErrorKind | V4-12, V4-15 | 2 |
| P1 ToolOutput/截断 | V3-20, V1-25 | 2 |
| P1 SafetyPipeline | V6-01~V6-06 | 6 |
| P2 Provider Pipeline | V4-09~V4-14 | 6 |
| P2 SkillSelector | V1-11~V1-15 | 5 |

---

## 附录：参考来源

- [agentskills.io Specification](https://agentskills.io/specification)
- [Anthropic Skills GitHub](https://github.com/anthropics/skills)
- [MCP Specification 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25)
- [MCP Servers Repository](https://github.com/modelcontextprotocol/servers)
- [Claude Code System Prompts](https://github.com/Piebald-AI/claude-code-system-prompts)
- [OpenAI Codex CLI](https://developers.openai.com/codex/cli/)
- [Goose (Block)](https://github.com/block/goose)
- [SWE-bench](https://www.swebench.com/)
- [SWE-agent](https://github.com/SWE-agent/SWE-agent)
- [ToolEmu](https://github.com/toolemu/toolemu)
- [Tau-bench](https://github.com/sierra-research/tau-bench)
- [AgentBench](https://github.com/THUDM/AgentBench)
- [Agentic AI Foundation (AAIF)](https://www.linuxfoundation.org/press/linux-foundation-announces-the-formation-of-the-agentic-ai-foundation)
- [skills.sh Directory](https://skills.sh/) — 86,691+ 社区 Skills 索引
- [Trail of Bits Skills](https://github.com/trailofbits/skills) — 35 安全审计 Skills
- [Vercel Agent Skills](https://github.com/vercel-labs/agent-skills)
- [Microsoft Skills](https://github.com/microsoft/skills) — 127+ Azure/AI Skills
- [GitHub awesome-copilot](https://github.com/github/awesome-copilot)
- [Sentry Agent Skills](https://github.com/getsentry/sentry-agent-skills)
- [EvalPlus (HumanEval+)](https://github.com/evalplus/evalplus)
- [sb-cli (SWE-bench CLI)](https://github.com/SWE-bench/sb-cli)
- [Terminal-Bench](https://github.com/harbor-framework/terminal-bench)
- [Tau2-bench](https://github.com/sierra-research/tau2-bench)
- [GAIA Benchmark](https://huggingface.co/datasets/gaia-benchmark/GAIA)
- [mini-SWE-agent](https://github.com/SWE-agent/mini-swe-agent)
