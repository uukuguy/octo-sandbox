# Octo-Sandbox 全面代码审查报告

**审查日期**: 2026-03-06
**审查分支**: `dev`
**审查方法**: RuFlo Swarm (hierarchical topology, 5 specialized agents)
**审查范围**: 全部 5 个 crate + 前端 (`web/`)

---

## 审查 Agent 分工

| Agent ID | 角色 | 模型 | 审查范围 |
|----------|------|------|----------|
| review-security | security-auditor | opus | 全项目安全审计 |
| review-engine | reviewer | opus | `crates/octo-engine/` 核心引擎 |
| review-server | reviewer | sonnet | `crates/octo-server/` API 层 |
| review-architecture | analyst | opus | 类型/架构/多租户 |
| review-frontend | reviewer | sonnet | `web/` 前端 |

---

## 一、问题统计总览

| 严重程度 | 后端 | 前端 | 合计 |
|----------|------|------|------|
| **P0 严重** | 15 | 3 | **18** |
| **P1 重要** | 28 | 8 | **36** |
| **P2 建议** | 24 | 9 | **33** |
| **合计** | 67 | 20 | **87** |

---

## 二、P0 严重问题（需立即修复）

### 安全类 P0

#### P0-SEC-1: CORS 默认配置完全开放
- **文件**: `crates/octo-server/src/router.rs:110-114`
- **问题**: `cors_origins` 为空时使用 `allow_origin(Any) + allow_methods(Any) + allow_headers(Any)`，任何网站可跨域调用所有 API
- **风险**: 结合 AuthMode::None 可实现远程代码执行
- **修复**: 默认仅允许 `localhost:5180`，显式列出所需 headers

#### P0-SEC-2: MCP Server update 绕过命令白名单
- **文件**: `crates/octo-server/src/api/mcp_servers.rs:264-342`
- **问题**: `create_server` 有 `ALLOWED_MCP_COMMANDS` 白名单，但 `update_server` 完全无检查。攻击者可先创建合法 server 再通过 PUT 改为恶意命令
- **补充**: args 和 env 也未校验，`-e "..."` 或 `LD_PRELOAD` 可注入
- **修复**: 提取命令校验函数，create/update 共用；过滤危险 args 和 env keys

#### P0-SEC-3: MCP 工具调用接口无用户鉴权
- **文件**: `crates/octo-server/src/api/mcp_tools.rs:40-51`
- **问题**: `call_tool`、`list_tools`、`list_executions` 均不提取 `UserContext`，任何用户可调用任何 MCP server 的任何工具
- **修复**: 添加 `Extension(ctx)` 并验证 server 所有权

#### P0-SEC-4: Provider API 明文接收 API Key 且无 RBAC
- **文件**: `crates/octo-server/src/api/providers.rs:128-151`
- **问题**: REST API 明文传输 LLM API key；provider 管理端点无角色检查
- **修复**: API key 通过 secret manager 引用；添加 Admin 角色检查

#### P0-SEC-5: 硬编码 HMAC 默认密钥
- **文件**: `crates/octo-engine/src/auth/config.rs:22`
- **代码**: `const DEFAULT_HMAC_SECRET: &str = "octo-default-hmac-secret-change-in-production";`
- **风险**: 攻击者可伪造任意 API Key hash
- **修复**: 未设置环境变量时生产模式拒绝启动，或自动生成随机密钥持久化

#### P0-SEC-6: LlmInstance 明文存储 API Key 且可被序列化
- **文件**: `crates/octo-engine/src/providers/chain.rs:17-25`
- **问题**: `api_key: String` 带 `Serialize`，`list_instances()` 返回包含明文 key 的完整结构
- **修复**: 使用 `#[serde(skip_serializing)]`，对外暴露脱敏版本

#### P0-SEC-7: SubprocessAdapter 无安全检查直接执行任意代码
- **文件**: `crates/octo-engine/src/sandbox/subprocess.rs:87-89`
- **问题**: `sh -c code` 无命令白名单、无路径校验、无沙箱隔离
- **修复**: 集成 `SecurityPolicy` 或 `ExecPolicy` 进行命令验证

#### P0-SEC-8: GrepTool 缺少路径验证 — 可读取任意系统文件
- **文件**: `crates/octo-engine/src/tools/grep.rs:71-78`
- **问题**: `search_path` 参数缺少 `path_validator` 检查（对比 `FindTool` 有此检查），用户可指定 `/etc/passwd` 等绝对路径读取敏感文件
- **修复**: 添加 `ctx.path_validator.check_path(&search_path)` 验证

#### P0-SEC-9: BashTool 沙箱失败时静默回退到直接执行 (fail-open)
- **文件**: `crates/octo-engine/src/tools/bash.rs:276-279`
- **问题**: sandbox-wasm 执行失败时静默 fallback 到无沙箱执行，攻击者可故意触发沙箱失败绕过保护
- **修复**: 采用 fail-closed 策略，添加 `sandbox_fallback_allowed` 配置，默认 false

### 架构类 P0

#### P0-ARCH-1: Sandbox 核心类型三处重复定义 — crate 边界名存实亡
- **文件**: `octo-types/src/sandbox.rs` + `octo-engine/src/sandbox/traits.rs` + `octo-sandbox/src/traits.rs`
- **问题**: 三个 crate 各自定义同名但结构不同的 `ExecResult`、`SandboxConfig`、`RuntimeAdapter` trait。`octo-engine` 实际不依赖 `octo-sandbox` crate
- **修复**: 统一 sandbox 抽象到 `octo-sandbox`，engine 通过 trait object 使用

#### P0-ARCH-2: octo-engine 为 God Crate — 40+ 依赖、21 模块、24000+ 行
- **文件**: `crates/octo-engine/Cargo.toml`
- **问题**: 密码学(aes-gcm/argon2)、JWT、文件监控(notify)、WASM(wasmtime)、Docker(bollard) 全在一个 crate，严重违反单一职责
- **修复**: 拆分 `octo-secret`、`octo-auth`、`octo-scheduler` 独立 crate

### 多租户类 P0

#### P0-MT-1: QuotaManager consume_api_call 从未被调用
- **文件**: `crates/octo-platform-server/src/middleware/quota.rs:18`
- **问题**: 中间件仅调用 `check_api_call()`（只读），不调用 `consume_api_call()`（递增），配额形同虚设
- **修复**: 替换为 `consume_api_call()` 或在 next.run() 后调用

#### P0-MT-2: QuotaManager check_window TOCTOU 竞态条件
- **文件**: `crates/octo-platform-server/src/tenant/quota.rs:32-44`
- **问题**: `Relaxed` ordering + 非原子的 check-then-store，并发下多线程可同时 reset 计数器
- **修复**: 使用 `compare_exchange` + `Acquire/Release` ordering

#### P0-MT-3: acquire_session/acquire_agent TOCTOU 竞态
- **文件**: `crates/octo-platform-server/src/tenant/quota.rs:74-94`
- **问题**: `check` 和 `fetch_add` 不是原子操作，并发请求可超限
- **修复**: 使用 `fetch_update` 或 CAS 循环

### 沙箱类 P0

#### P0-SB-1: NativeRuntime 无输出大小限制可 OOM
- **文件**: `crates/octo-sandbox/src/native.rs:35-47`
- **问题**: `Command::output()` 等待全部输出缓冲到内存，`yes` 等命令可产生无限输出
- **修复**: 限制 stdout/stderr 最大读取字节数（如 1MB）

#### P0-SB-2: SubprocessAdapter 无超时机制
- **文件**: `crates/octo-engine/src/sandbox/subprocess.rs:73-126`
- **问题**: 无 `tokio::time::timeout` 包裹，死循环命令永远阻塞
- **修复**: 使用 `tokio::time::timeout` 包裹执行

### 前端 P0

#### P0-FE-1: WebSocket 消息缺乏运行时验证
- **文件**: `web/src/ws/manager.ts:57-63`
- **问题**: `JSON.parse` 后直接 `as ServerMessage` 类型断言，无运行时验证
- **修复**: 添加类型守卫函数验证 `msg.type` 是否为已知枚举值

#### P0-FE-2: 模块级可变状态导致泄漏风险
- **文件**: `web/src/ws/events.ts:16-17`
- **问题**: `streamBuffer`/`thinkingBuffer` 为模块级全局变量，不受组件生命周期管理
- **修复**: 移入 `WsEventBridge` 组件通过 `useRef` 管理

#### P0-FE-3: MCP Server 表单缺少命令注入防护
- **文件**: `web/src/components/mcp/ServerList.tsx:98-145`
- **问题**: 前端不验证 command 输入的字符集，无基本清理
- **修复**: 添加 `SAFE_COMMAND_REGEX` 验证

---

## 三、P1 重要问题

### 安全/鉴权 P1

| ID | 文件 | 问题 | 修复建议 |
|----|------|------|----------|
| P1-SEC-1 | `router.rs:62-73` | X-Forwarded-For 可伪造绕过速率限制 | 增加 `trust_proxy` 配置，默认 false |
| P1-SEC-2 | `rate_limit.rs:15-18` | 速率限制器 HashMap 永不清理 key，可 DoS | 添加定期清理 + key 数量上限 |
| P1-SEC-3 | 多个 API handler | 12+ 端点缺少 UserContext/RBAC 检查 | 统一添加鉴权中间件 |
| P1-SEC-4 | `audit.rs:57-60` | 审计日志 API 无访问控制 | 要求 Admin 角色 |
| P1-SEC-5 | `metrics.rs:42` | Metrics API 无访问控制 | 要求 Admin 角色 |
| P1-SEC-6 | `scheduler.rs:172-177` | 任务所有权检查在匿名用户时被跳过 | 严格检查，匿名用户禁止访问有主任务 |
| P1-SEC-7 | `auth/config.rs:22` | HMAC Secret 硬编码默认值 | 生产模式强制要求配置 |
| P1-SEC-8 | `bash.rs:232-236` | 路径遍历检查仅匹配 `../`，易绕过 | 改用 `SecurityPolicy::check_path()` |
| P1-SEC-9 | `mcp/stdio.rs:44` | MCP stdio 客户端启动外部进程无命令校验 | 实施 command 白名单 |
| P1-SEC-10 | `memory/sqlite_store.rs:432` | FTS5 搜索未转义用户输入 | 对 token 使用双引号转义 |
| P1-SEC-11 | `tools/file_read.rs:59` | 文件工具缺少符号链接解析，可绕过路径验证 | 对已存在文件先 `canonicalize` 再校验 |
| P1-SEC-12 | `ws.rs:103-255` | WebSocket 连接无消息速率限制 | 添加每分钟消息数限制（如 20/min） |
| P1-SEC-13 | `auth/api_key.rs:76-79` | API Key 哈希使用 SHA-256 但无盐值 | 添加随机盐值或统一使用 HMAC-SHA256 |
| P1-SEC-14 | `tools/bash.rs:42-43` | ExecPolicy 白名单含 python/node 等高风险命令 | 从默认白名单移除，放入需显式启用的扩展白名单 |

### 多租户 P1

| ID | 文件 | 问题 | 修复建议 |
|----|------|------|----------|
| P1-MT-1 | `auth/jwt.rs:60-97` | Access Token 和 Refresh Token 无法区分 | Claims 添加 `token_type` 字段 |
| P1-MT-2 | `user_runtime.rs:83` | user_id 拼接文件路径有路径遍历风险 | 验证 user_id 不含 `/`, `..`, `\0` |
| P1-MT-3 | `agent_pool.rs:399` | AgentPool 硬编码读取 ANTHROPIC_API_KEY 环境变量 | 从租户配置获取 |
| P1-MT-4 | `id.rs:15-16` | ID 类型 `from_string` 无输入验证 | 添加非空、长度、字符集验证 |

### 代码质量 P1

| ID | 文件 | 问题 | 修复建议 |
|----|------|------|----------|
| P1-CQ-1 | `loop_.rs:178-180` | `assert!` 可导致生产 panic | 改为 `Err(AgentError::ConfigError)` |
| P1-CQ-2 | `loop_.rs:31,242-246` | MAX_ROUNDS 常量与实际使用不一致 | 日志使用 `max_rounds` 局部变量 |
| P1-CQ-3 | `sqlite_working.rs:15-16` | 异步上下文使用 `std::sync::RwLock` | 改用 `tokio::sync::RwLock` 或添加注释 |
| P1-CQ-4 | `chain.rs:264-291` | ProviderChain 健康检查始终返回 true | 实现真正的健康检查 |
| P1-CQ-5 | `auth/api_key.rs:96-98` | ApiKeyStorage 用同步 rusqlite，不支持并发 | 改用 `Arc<Mutex<Connection>>` 或 tokio_rusqlite |
| P1-CQ-6 | `db/users.rs:多处` | `Mutex::lock().unwrap()` 毒化风险 | 使用 `parking_lot::Mutex` 或 `.map_err()` |
| P1-CQ-7 | `chain.rs:235-261` | ProviderChain 健康检查 task 无法终止，内存泄漏 | 返回 JoinHandle 或接受 CancellationToken |
| P1-CQ-8 | `session/sqlite.rs:53,186` | Session DB 写入结果被 `let _` 静默忽略 | 至少记录 warn 日志 |
| P1-CQ-9 | `mcp/bridge.rs:55-63` | McpToolBridge 假设 content 为 string，MCP 规范允许 array | 支持 content 作为 array 的情况 |
| P1-CQ-10 | `catalog.rs:60-81` | AgentCatalog 四个 DashMap 索引更新非原子 | 用事务性操作或锁保护所有索引更新 |
| P1-CQ-11 | 11 个文件 | 超过 500 行文件违反项目规范（最大 loader.rs 1111 行） | 拆分为子模块 |

### 类型设计 P1

| ID | 文件 | 问题 | 修复建议 |
|----|------|------|----------|
| P1-TY-1 | `provider.rs:12-16` | `TokenUsage` 使用 `u32` 可能溢出 | 改为 `u64` |
| P1-TY-2 | `memory.rs:72-74` | `char_count()` 返回字节数而非字符数 | 使用 `.chars().count()` |
| P1-TY-3 | `lib.rs:11-19` | Glob re-export 污染命名空间 | 改为显式 re-export |
| P1-TY-4 | `memory.rs:171,200` | `from_str` 命名阴影标准库 | 实现 `std::str::FromStr` trait |

### 前端 P1

| ID | 文件 | 问题 | 修复建议 |
|----|------|------|----------|
| P1-FE-1 | `Memory.tsx:54-56` 等 | API 响应未检查 `res.ok` 状态 | 统一检查 `if (!res.ok) throw` |
| P1-FE-2 | `manager.ts:95-97` | WebSocket 单一 handler 模式不可扩展 | 改为发布-订阅模式 |
| P1-FE-3 | 全部 POST/DELETE | 缺少 CSRF 防护 | 添加 CSRF token 或 SameSite cookie |
| P1-FE-4 | `MessageList.tsx` | 消息列表无虚拟化，长会话性能差 | 使用 `@tanstack/react-virtual` |
| P1-FE-5 | `events.ts:109-118` | executionRecordsAtom 无界增长 | 添加数量上限 |
| P1-FE-6 | `events.ts:82-97` | 错误消息可能泄露内部信息 | 分类处理，仅显示用户友好描述 |
| P1-FE-7 | `Memory.tsx:39-42` 等 | useEffect 缺少依赖项 | 使用 `useCallback` 或在 effect 内定义 |

---

## 四、P2 建议改进（摘要）

### 后端 P2
1. `octo-types` 依赖 `anyhow` 不合理 — 应移除
2. `octo-engine` 同时依赖 `rusqlite` 和 `sqlx` — 应统一（sqlx 实际未使用）
3. `ToolExecution` ID 用裸 String 而非类型化 ID
4. `SandboxConfig.working_dir` 用 String 而非 PathBuf
5. `MemoryId` 与 `newtype_id!` 宏重复实现
6. `SecurityPolicy.forbidden_paths` 的 `~` 前缀运行时展开不可靠
7. McpStorage 大量重复的 row_to_entry 映射代码
8. EventBus history 写锁粒度可优化
9. 数据库迁移不在事务中执行
10. CredentialVault 的 salt 使用不一致
11. 审计中间件每次请求创建新 DB 连接
12. MCP env 序列化格式不一致（create 用 JSON，update 用 KEY=VALUE）
13. `list_servers` 中 runtime_status 判断逻辑错误（所有 server 共享状态）
14. Budget API 返回硬编码假数据
15. `-h` 参数与 `--help` 冲突
16. `AgentLoop::run()` 约 490 行，远超 50 行函数限制 — 应拆分
17. OpenAI/Anthropic SSE 解析代码高度重复 — 抽取通用框架
18. `AgentExecutor::new()` 有 17 个参数 — 使用 Builder 模式
19. `LoopGuard::recent_calls` 使用 `Vec::remove(0)` O(n) — 改用 VecDeque
20. `vector_search` 加载全表到内存计算余弦相似度 — 引入 ANN 索引
21. `ContextBudgetManager` 默认 200k tokens 不适用所有模型 — 从模型配置获取
22. Tenant 模型使用 `String` 而非 `TenantId` 类型 — 丧失类型安全
23. `octo-engine` default features 启用 wasmtime+bollard — 应默认关闭
24. NativeRuntime 硬编码 `bash` 和 PATH — 不可移植

### 前端 P2
1. Tab 切换使用条件卸载导致状态丢失
2. ToolInvoker/LogViewer 使用硬编码 mock 数据
3. MCP 组件混用硬编码颜色和设计 token
4. UI 文本中英文混用
5. LogViewer "导出"按钮无功能
6. ChatInput 不支持自动高度调整
7. config.ts fallback URL 硬编码 HTTP
8. 未启用 React StrictMode

---

## 五、架构层面总结

### 5.1 依赖关系评估 ✅
```
octo-types (0 内部依赖)
  ├── octo-sandbox (依赖 octo-types)
  └── octo-engine (依赖 octo-types)
        ├── octo-server (依赖 types + engine + sandbox)
        └── octo-platform-server (依赖 types + engine + sandbox)
```
无循环依赖，分层清晰。**但 `octo-engine` 不依赖 `octo-sandbox`，CLAUDE.md 中的依赖图与代码不符**。`octo-engine` 过于庞大（21 模块、40+ 依赖），需要拆分。

### 5.2 多租户隔离评估

| 维度 | 当前状态 | 风险等级 |
|------|---------|---------|
| 数据隔离 | 通过 tenant_id 字段过滤 | 中 |
| 运行时隔离 | TenantRuntime 内存级隔离 | 低 |
| 配额隔离 | **不生效** (P0-MT-1) | **高** |
| Agent 池隔离 | 共享池、共享 API Key (P1-MT-3) | 高 |
| 文件系统隔离 | 路径遍历风险 (P1-MT-2) | 高 |

### 5.3 整体代码质量评价

**优点**:
1. 清晰的三层 Agent 架构 (Runtime - Executor - Loop)
2. 数据库层面 SQL 注入防护做得好（一致使用参数化查询）
3. LoopGuard 循环检测设计精良（多层级防护：哈希计数、结果感知、ping-pong 检测、全局断路器）
4. API Key 使用 SHA-256 + 常量时间比较（subtle::ConstantTimeEq）防止时序攻击
5. AES-256-GCM 加密 + Argon2id 密钥派生 + Zeroize 内存清理
6. Taint Tracking 数据污点追踪防止敏感数据泄露
7. BashTool 环境变量白名单（env_clear + 白名单）
8. SSRF 防护拦截私有 IP 和云元数据端点
9. 前端零 any 使用，TypeScript 类型覆盖完整，无 XSS 风险
10. 前端依赖极度精简（仅 7 个运行时依赖）

**关键风险领域**:
1. **命令执行安全**：BashTool/SubprocessAdapter/MCP stdio/GrepTool 检查不够严密，沙箱 fail-open
2. **API 鉴权缺失**：12+ 端点缺少 UserContext/RBAC
3. **多租户配额**：完全不生效 + TOCTOU 竞态条件
4. **密钥管理**：HMAC 硬编码 + API Key 明文存储和传输
5. **架构完整性**：Sandbox 类型三处重复、octo-engine God Crate、CLAUDE.md 依赖图与代码不符

---

## 六、修复优先级建议

### 第一优先级（立即修复，预计 4h）
1. P0-SEC-1 CORS 默认限制为 localhost
2. P0-SEC-2 MCP update 命令注入（提取共用校验函数）
3. P0-SEC-5 HMAC 硬编码密钥（生产模式拒绝启动）
4. P0-SEC-8 GrepTool 添加路径验证
5. P0-SEC-9 沙箱 fail-closed 策略
6. P0-SB-1/2 沙箱 OOM 限制和超时

### 第二优先级（本迭代内，预计 8h）
1. P0-SEC-3/4/6 MCP 工具、Provider 鉴权和 API Key 脱敏
2. P0-MT-1/2/3 配额消费 + 竞态条件修复
3. P1-SEC-1/2 速率限制绕过和内存泄漏
4. P1-SEC-3 所有端点添加 RBAC
5. P1-SEC-12 WebSocket 消息速率限制
6. P1-MT-1 JWT Token 类型区分

### 第三优先级（下一里程碑）
1. P0-ARCH-1/2 Sandbox 统一 + octo-engine 拆分
2. P1-CQ-* 代码质量改进（健康检查、session DB 写入、索引原子性）
3. P1-TY-* 类型设计统一
4. P1-FE-* 前端改进
5. P2-* 所有建议改进

---

## 七、安全亮点（正面发现）

审计过程中也发现项目已实现的优秀安全实践：

1. **API Key 常量时间比较**: `subtle::ConstantTimeEq` 防止时序攻击
2. **AES-256-GCM 加密**: Argon2id 密钥派生 + 每次加密生成新 nonce
3. **Zeroize 内存清理**: 敏感数据 drop 时自动清除
4. **Taint Tracking**: 数据污点追踪防止敏感数据流向不安全 sink
5. **BashTool Shell 元字符过滤**: 拦截 `;`, `|`, `&&`, `||`, `$(`, `` ` ``, `>`, `<`, `\n`, `\0`
6. **环境变量白名单**: `env_clear()` + 白名单避免泄露
7. **参数化 SQL**: 绝大部分使用 `?1, ?2` 占位符
8. **SSRF 防护**: 拦截私有 IP、云元数据端点
9. **完整 RBAC**: 5 级角色 + 细粒度动作权限
10. **ContextPruner 4+1 阶段降级策略**: 优雅的上下文管理

---

*报告由 RuFlo Swarm (swarm-1772801688449) 生成，5 个专业 agent 并行审查*
*总审查 token 消耗: ~610k tokens | 审查耗时: ~3 分钟*
