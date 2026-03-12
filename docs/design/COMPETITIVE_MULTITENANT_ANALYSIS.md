# 多租户/平台能力竞品代码分析报告

> 分析日期：2026-03-12
> 分析对象：octo-sandbox vs moltis vs openfang
> 分析维度：租户隔离、认证体系、授权模型、配额管理、审计日志、Token 计量、Agent 池、前端多用户

---

## 一、总览对比矩阵

| 维度 | octo-sandbox | moltis | openfang |
|------|-------------|--------|----------|
| **多租户架构** | 专用 `octo-platform-server` crate，完整租户模型 | 无多租户设计，纯单用户 | 无多租户，仅有 migration 中的遗留字段 |
| **认证体系** | API Key + JWT + OAuth2 (Google/GitHub) | Password + Passkey (WebAuthn) + Session Cookie | 无认证层（仅 API key 环境变量） |
| **授权模型** | 4 级 RBAC (Viewer/User/Admin/Owner) + 7 种 Action | RequireAuth 中间件（二元：认证/未认证） | 无 RBAC（channel_bridge 中有初步 authorize 调用） |
| **配额管理** | 原子计数器 QuotaManager + RAII Guard 模式 | 无配额管理 | 无配额管理（仅 agent 级 cost 限额） |
| **审计日志** | 双层：engine AuditStorage + platform AuditEvent | 无审计日志 | Merkle 哈希链审计（防篡改） |
| **Token 计量** | Metering（原子计数器，input/output/request/error） | 无集中计量（provider 内部有 token 统计） | MeteringEngine（SQLite 持久化 + 美元成本估算） |
| **Agent 池** | AgentPool（软硬限制 + 隔离策略 + 空闲回收） | 无 Agent 池 | 无 Agent 池 |
| **前端多用户** | web-platform/ 独立前端（WIP） | 单用户 Web UI | 单用户 Dashboard (Alpine.js) |

---

## 二、逐维度深度分析

### 2.1 租户隔离设计

#### octo-sandbox（评分：7/10）

octo-sandbox 是三个项目中**唯一**具备真正多租户架构的项目。

**核心实现路径**：
- `crates/octo-platform-server/src/tenant/` — 完整的租户子系统
- `crates/octo-types/src/id.rs` — 强类型 `TenantId`、`UserId`、`SessionId`

**租户模型**（`tenant/models.rs`）：
- `Tenant` 结构体：id, name, slug, plan (Free/Pro/Enterprise), timestamps
- `ResourceQuota`：max_agents, max_sessions_per_user, max_api_calls_per_day, max_memory_mb, max_mcp_servers

**租户管理器**（`tenant/manager.rs`）：
- SQLite 持久化（`tenants` + `tenant_quotas` 两张表）
- 默认租户自动创建（`DEFAULT_TENANT_ID = "default"`）
- `DashMap<String, Arc<TenantRuntime>>` 运行时缓存
- `get_or_create_runtime()` 懒加载租户运行时

**租户运行时**（`tenant/runtime.rs`）：
- 每个租户独立的 `QuotaManager` + `DashMap<String, Value>` MCP 服务器列表
- 审计事件发布（当前仅 tracing::info，预留事件总线扩展点）

**AgentCatalog 租户索引**（`agent/catalog.rs`）：
- `by_tenant_id: DashMap<TenantId, Vec<AgentId>>` — O(1) 按租户查 agent
- `register()` 接受 `Option<TenantId>`，无则归入 default 租户
- `get_by_tenant()` / `unregister()` 维护索引一致性

**UserRuntime**（`user_runtime.rs`）：
- 每用户独立数据库路径（`db_path_template.replace("{user_id}", &user_id)`）
- 会话并发限制（`max_concurrent_agents`）
- TOCTOU 防护的会话创建锁

**差距**：
- 租户间数据隔离依赖应用层逻辑（同一 SQLite 不同 tenant_id），非物理隔离
- 缺少租户级别的加密密钥隔离
- 租户 CRUD API 已有但前端管理界面未完成
- `TenantRuntime.publish_audit_event()` 仅打日志，未持久化

#### moltis（评分：1/10）

moltis 是纯单用户桌面/CLI 应用，无任何多租户设计。代码中出现的 `oauth_tenant` 仅指 MS Teams Bot Framework 的 OAuth 租户段（用于 Teams channel 接入），与多租户平台无关。

#### openfang（评分：1/10）

openfang 同样是单用户 Agent OS。`openclaw.rs` migration 中出现 `tenant_id` / `user_id` 字段，但仅用于从旧格式迁移配置，不参与任何运行时隔离逻辑。`channel_bridge.rs` 中有 `allowed_tenants` 但仅用于 MS Teams 租户白名单过滤。

---

### 2.2 认证体系

#### octo-sandbox（评分：8/10）

**三层认证架构**：

1. **API Key（octo-engine）**：
   - `ApiKeyStorage`：SQLite 持久化，SHA-256 哈希存储
   - 常量时间比较（`subtle::ConstantTimeEq`）防时序攻击
   - 支持过期时间、最后使用时间跟踪
   - `AuthConfig`：HMAC-SHA256 哈希 + 生产环境强制 `OCTO_HMAC_SECRET`

2. **JWT（octo-platform-server）**：
   - `JwtManager`：HS256 签名，access + refresh 双 token
   - Claims 包含 `sub`(user_id), `email`, `role`, `tenant_id`
   - access token 15 分钟，refresh token 7 天
   - 最小 32 字符密钥强制

3. **OAuth2（octo-platform-server）**：
   - `OAuthProvider` trait 抽象
   - `GoogleOAuthProvider`：完整 OAuth2 flow + userinfo 获取
   - `GitHubOAuthProvider`：含 fallback email 获取逻辑
   - 可扩展的 provider 体系

**差距**：
- 无 OIDC（OpenID Connect）标准支持（仅原始 OAuth2）
- 无 MFA / 2FA 支持
- refresh token 无吊销机制（无 blacklist）

#### moltis（评分：6/10）

moltis 有较成熟的**单用户认证**：
- Password + Argon2 哈希
- WebAuthn passkey 完整支持（注册/认证/管理）
- Session cookie 认证
- `CredentialStore` JSON 持久化
- `RequireAuth` 中间件保护 `/api/*`
- API Key 支持
- OAuth callback handler（用于 LLM provider 的 OAuth，非用户认证）

**差距**：无多用户支持，所有认证围绕"设备主人"单一身份。

#### openfang（评分：1/10）

无认证层。API key 通过环境变量直接加载，所有 HTTP 端点公开暴露。`auth_cooldown.rs` 是 provider 级别的熔断器（限制对 LLM provider 的请求），与用户认证无关。

---

### 2.3 授权模型

#### octo-sandbox（评分：8/10）

**4 级 RBAC + 细粒度 Action**：

角色层级（`roles.rs`）：
| 角色 | 优先级 | 权限 |
|------|--------|------|
| Viewer | 1 | Read |
| User | 2 | Read, CreateSession, RunAgent |
| Admin | 3 | +ManageMcp, ManageSkills |
| Owner | 4 | 所有权限 |

- `Role.can(Action)` — 权限检查
- `Role.has_at_least(required)` — 层级比较
- `UserContext` — 请求级别的用户上下文（user_id + permissions + role）
- `RequiredAction` / `RequiredRole` — 路由级守卫
- `Permission` 枚举（Read/Write/Admin）— 与 Role 正交的权限维度

JWT Claims 中携带 `role` 和 `tenant_id`，支持租户级别的角色隔离。

**差距**：
- 无资源级 ACL（不能针对特定 agent/session 设置权限）
- 无权限组/权限模板
- RBAC 规则硬编码在 `can()` 方法中，不可配置

#### moltis（评分：2/10）

二元认证模型：已认证 vs 未认证。`RequireAuth` 中间件仅检查是否有有效 session，无角色区分。

#### openfang（评分：2/10）

`channel_bridge.rs` 中有初步的 `authorize(user_id, &auth_action)` 调用，但仅用于 channel（Teams 等）的消息过滤，非通用 RBAC。

---

### 2.4 配额管理

#### octo-sandbox（评分：8/10）

**双层配额管理**：

1. **租户级**（`tenant/quota.rs`）：
   - `QuotaManager`：全原子计数器，零锁竞争
   - 24 小时滑动窗口 API 调用限额
   - 活跃 session / agent 数量限制
   - **RAII Guard 模式**：`SessionGuard` / `AgentGuard` — `Drop` 时自动释放配额
   - 支持 `QuotaExceeded` 错误类型（DailyApiCalls, ActiveSessions, ActiveAgents, McpServers）

2. **中间件层**（`middleware/quota.rs`）：
   - Axum 中间件，HTTP 429 + Retry-After header
   - 请求级别的配额检查

3. **UserRuntime**（`user_runtime.rs`）：
   - `max_concurrent_agents` 并发限制
   - Mutex 防 TOCTOU 的会话创建

**差距**：
- 配额仅在内存中（重启后 daily_api_calls 归零）
- 无 rate limiting（滑动窗口/令牌桶）
- 无配额用量通知/告警
- 无按 plan 自动设置配额的逻辑（Free/Pro/Enterprise 配额相同）

#### moltis（评分：0/10）

无任何配额管理。

#### openfang（评分：4/10）

虽然无多租户配额，但有**per-agent 成本配额**（`ResourceQuota`）：
- `max_cost_per_hour_usd` / `max_cost_per_day_usd` / `max_cost_per_month_usd`
- 全局预算：`BudgetConfig.max_hourly_usd` / `max_daily_usd` / `max_monthly_usd`
- 告警阈值 `alert_threshold`

这些是单用户场景下的成本控制，非多租户配额。

---

### 2.5 审计日志

#### octo-sandbox（评分：6/10）

**双层审计**：

1. **engine 层**（`audit/storage.rs`）：
   - `AuditStorage`：SQLite 持久化
   - `AuditEvent`：event_type, user_id, session_id, resource_id, action, result, metadata, ip_address
   - 查询支持：按 event_type / user_id 过滤 + 分页

2. **platform 层**（`platform-server/audit/mod.rs`）：
   - `AuditEvent`（含 `tenant_id`）：面向多租户场景
   - 15 种 `AuditAction`：Login, LoginFailed, Logout, CRUD Agent/Session/MCP/User/Tenant, UpdateQuota
   - `AuditEventBuilder` 构建器模式
   - `TenantRuntime.publish_audit_event()` 发布接口

**差距**：
- platform 层的审计事件**仅打日志**（`tracing::info`），未持久化到数据库
- 无审计日志的防篡改机制（哈希链）
- 无审计日志导出/归档
- 无实时审计流（WebSocket/SSE）

#### moltis（评分：0/10）

无审计日志系统。有结构化日志（NDJSON 文件下载），但非审计级别。

#### openfang（评分：9/10）

**Merkle 哈希链审计**（`audit.rs`）— 这是三者中最先进的审计实现：
- 每条审计记录包含前一条的 SHA-256 哈希，形成**防篡改链**
- 12 种 `AuditAction`：ToolInvoke, CapabilityCheck, AgentSpawn/Kill, MemoryAccess, FileAccess, NetworkAccess, ShellExec, AuthAttempt, WireConnect, ConfigChange
- `verify_integrity()` 全链完整性验证
- SQLite 持久化，重启后自动加载并验证链完整性
- 篡改检测测试覆盖

**与 octo 的关键差距**：openfang 的审计是单用户的（`agent_id` 级别），无 `tenant_id` / `user_id`。

---

### 2.6 Token 计量/计费

#### octo-sandbox（评分：4/10）

**基础计量**（`metering/mod.rs`）：
- `Metering`：5 个 AtomicU64 计数器（input_tokens, output_tokens, requests, errors, duration_ms）
- `MeteringSnapshot`：快照 + 衍生指标（total_tokens, avg_tokens_per_request, avg_duration_ms）
- `reset()` 归零

**差距**：
- 无成本估算（不知道一次请求花了多少钱）
- 无持久化（重启后归零）
- 无按租户/用户/agent 的分维度计量
- 无模型定价表
- 无用量告警
- 无 per-session 或 per-agent 的计量隔离

#### moltis（评分：2/10）

无集中计量。provider 内部有 token 统计但不暴露统一接口。

#### openfang（评分：9/10）

**完整计量引擎**（`metering.rs`）：
- `MeteringEngine`：基于 `UsageStore`（SQLite 持久化）
- `UsageRecord`：agent_id, model, input/output_tokens, cost_usd, tool_calls
- 三级时间窗口：hourly / daily / monthly（per-agent + global）
- **模型定价表**：覆盖 40+ 模型系列（Anthropic, OpenAI, Google, DeepSeek, Groq, xAI, Qwen, Mistral 等）
- `estimate_cost()` 静态方法 + `estimate_cost_with_catalog()` 动态定价
- `BudgetStatus`：支出 vs 限额 + 百分比 + 告警阈值
- `UsageSummary` / `ModelUsage` — 按 agent / 按 model 聚合
- `cleanup()` 历史数据清理

---

### 2.7 Agent 池共享 vs 隔离

#### octo-sandbox（评分：7/10）

**唯一实现 Agent 池的项目**（`agent_pool.rs`）：

- `AgentPool`：`DashMap<InstanceId, AgentInstance>` + `Vec<InstanceId>` 空闲队列
- `PoolConfig`：soft_max_total / hard_max_total / min_idle / max_idle / idle_timeout
- **三级隔离策略**：Memory / Process / Session
- `AgentInstance`：runtime (Arc<AgentRuntime>) + workspace + state (Idle/Busy/Releasing)
- `Workspace`：per-user session_ids + context snapshot
- `get_instance()` 空闲池优先 -- 创建新实例 -- Exhausted 错误
- `release_instance()` 持久化 workspace -- 清除上下文 -- 归还空闲池
- `spawn_cleanup_task()` — 60 秒定时清理过期空闲实例
- `persist_workspace()` — 工作区 JSON 文件持久化

**差距**：
- 每个实例创建独立 AgentRuntime + 独立 SQLite DB（开销较大）
- 无实例预热（min_idle 预创建逻辑标注为 TODO）
- 仅 Memory 隔离策略有实际实现，Process/Session 为 placeholder
- 无健康检查/心跳机制

#### moltis / openfang（评分：0/10）

两者均无 Agent 池概念。每个 session 直接绑定一个 agent 实例。

---

### 2.8 前端多用户支持

#### octo-sandbox（评分：3/10）

- `web-platform/` 目录预留（独立于 `web/` workbench 前端）
- 后端 API 已部分就绪（admin/tenants, users, sessions, MCP）
- 实际前端尚未实现

#### moltis（评分：4/10）

成熟的单用户 Web UI（Preact + Tailwind）：
- 聊天、会话管理、MCP 管理、Skills、Docker 镜像管理
- 完善的 OAuth callback、Session 分页
- 无多用户/多租户界面

#### openfang（评分：3/10）

Alpine.js 单页 Dashboard：
- Agent 管理、预算查看、A2A 网络状态
- 单用户，无登录界面

---

## 三、综合评分

| 维度 | 权重 | octo-sandbox | moltis | openfang |
|------|------|-------------|--------|----------|
| 租户隔离 | 20% | **7** | 1 | 1 |
| 认证体系 | 15% | **8** | 6 | 1 |
| 授权模型 | 15% | **8** | 2 | 2 |
| 配额管理 | 15% | **8** | 0 | 4 |
| 审计日志 | 10% | 6 | 0 | **9** |
| Token 计量 | 10% | 4 | 2 | **9** |
| Agent 池 | 10% | **7** | 0 | 0 |
| 前端多用户 | 5% | 3 | 4 | 3 |
| **加权总分** | 100% | **6.85** | 1.90 | 3.05 |

---

## 四、关键发现

### octo-sandbox 的独特优势

1. **唯一的多租户架构**：三个项目中仅 octo-sandbox 设计了完整的租户模型（Tenant -> TenantRuntime -> QuotaManager -> UserRuntime），从类型系统（`TenantId`）到数据库表（`tenants`, `tenant_quotas`）到运行时隔离都有覆盖。

2. **唯一的 Agent 池**：`AgentPool` 是在 Rust AI Agent 框架中罕见的资源池化设计，支持热备、空闲回收、隔离策略选择。

3. **最完整的认证栈**：API Key + JWT + OAuth2 三层，覆盖从开发者 API 到终端用户 Web 登录的全场景。

4. **RAII 配额守卫**：`SessionGuard` / `AgentGuard` 利用 Rust 的 Drop trait 自动释放配额，消除资源泄漏风险。

### octo-sandbox 的真实差距

1. **Token 计量远落后于 openfang**：octo 的 `Metering` 仅是内存计数器，无持久化、无成本估算、无按模型定价。openfang 的 `MeteringEngine` 完全碾压。

2. **审计防篡改能力缺失**：openfang 的 Merkle 哈希链审计是企业级合规要求，octo 的简单 INSERT 日志缺乏完整性保证。

3. **platform 审计未落地**：`octo-platform-server/audit/` 定义了 15 种审计事件和 builder 模式，但 `TenantRuntime.publish_audit_event()` 仅 `tracing::info`，未写入数据库。

4. **配额无持久化**：`QuotaManager` 的 `AtomicU64` 计数器在服务重启后归零，daily 限额形同虚设。

5. **隔离策略仅 Memory 级别**：`IsolationStrategy::Process` / `Session` 定义了枚举值但无实际实现。

6. **前端平台空缺**：`web-platform/` 仅是空壳，后端 API 有了但无配套前端。

### 应从竞品学习的模式

| 来源 | 模式 | 可引入 octo 的位置 |
|------|------|-------------------|
| openfang | Merkle 哈希链审计 | `octo-engine/src/audit/` — 增加 `prev_hash`/`hash` 字段 + `verify_integrity()` |
| openfang | 模型定价表 + 成本估算 | `octo-engine/src/metering/` — 新增 `estimate_cost()` + pricing table |
| openfang | 三级时间窗口预算 (hourly/daily/monthly) | `octo-platform-server/tenant/quota.rs` — 扩展 QuotaManager |
| openfang | SQLite 持久化 UsageStore | `octo-engine/src/metering/` — 增加 SQLite storage |
| openfang | Provider 熔断器 (CircuitBreaker) | `octo-engine/src/providers/` — ProviderChain 已有 failover，可增加熔断 |
| moltis | WebAuthn passkey 认证 | `octo-platform-server/src/auth/` — 增加 passkey provider |
| moltis | CredentialStore 统一凭据管理 | `octo-engine/src/secret/` — 已有 AES-GCM，可扩展 |

---

## 五、成熟度评估

### octo-sandbox 多租户成熟度：**Beta（60%）**

**已完成**：
- 租户数据模型 + SQLite 持久化
- 租户运行时 + 配额管理（内存级）
- JWT + OAuth2 认证
- 4 级 RBAC
- Agent 池 + 隔离策略框架
- 审计事件模型
- 强类型 ID 系统（TenantId, UserId, SessionId）
- AgentCatalog 租户索引
- UserRuntime 用户隔离
- 配额中间件（HTTP 429）

**未完成**：
- 审计事件持久化（仅 tracing::info）
- Token 计量的成本估算 + 持久化
- 配额计数器持久化
- Process/Session 级隔离策略实现
- 租户管理前端
- 用户自注册流程
- 租户间加密隔离
- Rate limiting（令牌桶/滑动窗口）
- 审计防篡改
- 配额用量告警/通知

### 结论

octo-sandbox 在 Rust AI Agent 框架领域的多租户能力处于**明确领先地位**。moltis 和 openfang 本质上是单用户工具，从未将多租户作为设计目标。octo 的主要改进方向不是追赶竞品的多租户能力（竞品没有），而是**将自身已搭建的框架从 Beta 推向生产就绪**，重点补齐计量持久化、审计防篡改、配额持久化三个短板。openfang 的 MeteringEngine 和 AuditLog 实现质量极高，值得参考移植。
