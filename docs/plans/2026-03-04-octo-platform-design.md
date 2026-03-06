# octo-platform 设计方案

> 日期：2026-03-04
> 状态：设计完成（已确认：产品定位、架构总览、目录结构、前端策略、编排层、数据模型、实施路线图）
> 基础：octo-workbench v1.0 冲刺完成后开始实施
> 更新：2026-03-05 添加 Phase 2.11a/b 拆分说明

---

## 0.1 Phase 2.11 拆分说明

由于 octo-engine 需要同时支撑 octo-workbench (单用户) 和 octo-platform (多租户)，将原 Phase 2.11 拆分为：

### Phase 2.11a: octo-engine 多租户适配 (当前分支)

**目标**：octo-engine 原生支持多租户，为 octo-platform 做好准备

**详细计划**：`2026-03-05-phase2-11a-octoe-engine-multi-tenant-implementation.md`

| Task | 内容 |
|------|------|
| Task 1 | AgentCatalog 添加 TenantId 索引 |
| Task 2 | 新增 TenantContext 类型 |
| Task 3 | AgentRuntime 支持 TenantContext |
| Task 4 | Zone A/B 完整实现 |
| Task 5 | Budget 统一 |
| Task 6 | AppState 集成 + REST API |
| Task 7 | 构建验证 |

### Phase 2.11b: octo-platform-server (新分支)

**目标**：实现租户管理层

| Task | 内容 |
|------|------|
| Task 1 | TenantRuntime 实现 |
| Task 2 | UserRuntime 实现 |
| Task 3 | JWT/OAuth2 认证集成 |
| Task 4 | 资源配额管理 |

**依赖**：Phase 2.11a 完成后的 octo-engine

---

## 一、产品定位

**octo-platform** 是基于 `octo-engine` 的企业级多租户多 Agent 平台，与 octo-workbench 独立演进，共享核心引擎。

### 三种部署模式

| 模式 | 场景 | 租户概念 |
|------|------|---------|
| **SaaS 平台** | 多企业订阅，云端部署 | 租户 = 企业/组织 |
| **企业私有部署** | 单企业内网，多团队使用 | 租户 = 部门/团队（可退化为单租户） |
| **开发者 API 平台** | 开发者通过 API Key 调用 | 租户 = 开发者账号 |

### 与 octo-workbench 的关系

```
octo-types    ← 共享类型（两产品都用，不修改）
octo-engine   ← 共享核心引擎（两产品都用，持续完善）
     ↙                    ↘
octo-workbench            octo-platform
（单用户单实例）            （多租户多用户多Agent）
branch: octo-workbench    branch: octo-platform
独立演进                   独立演进
```

---

## 二、两级隔离架构

### 场景对应关系

**场景 A：SaaS 部署**（需两级隔离）
```
octo-platform (单实例)
├── 租户 A（公司甲）—— 独立 DB、独立 MCP 配置、独立配额
│   ├── 用户1：独立 AgentRuntime、记忆、会话
│   └── 用户2：独立 AgentRuntime、记忆、会话
└── 租户 B（公司乙）—— 与租户A完全隔离
```

**场景 B：企业私有部署**（租户退化为单个组织）
```
octo-platform（公司内网）
└── 租户：公司（单一）
    ├── 用户1（工程团队）
    └── 用户2（产品团队）
```

**场景 C：开发者 API 平台**（需两级隔离）
```
octo-platform
├── 开发者 A（租户）→ API Key 调用，终端用户是其用户
└── 开发者 B（租户）→ API Key 调用，终端用户是其用户
```

### 架构总览

```
┌─────────────────────────────────────────────────────┐
│                  octo-platform                       │
│  ┌──────────────┬──────────────┬──────────────────┐  │
│  │  SaaS 模式   │  企业私有部署 │  开发者 API 平台  │  │
│  └──────────────┴──────────────┴──────────────────┘  │
│                                                       │
│  ┌─────────────────────────────────────────────────┐  │
│  │  租户层（TenantRuntime）                         │  │
│  │  ├── 独立 DB schema / SQLite 文件               │  │
│  │  ├── 独立 MCP 服务器配置                        │  │
│  │  ├── 资源配额（Agent数/调用数/存储）             │  │
│  │  └── 租户级 API Key 管理                        │  │
│  └─────────────────────────────────────────────────┘  │
│  ┌─────────────────────────────────────────────────┐  │
│  │  用户层（UserRuntime）                           │  │
│  │  ├── 独立 AgentRuntime（复用 octo-engine）       │  │
│  │  ├── 独立会话历史 + WorkingMemory               │  │
│  │  ├── 独立长期记忆空间                            │  │
│  │  └── 个人 Agent 配置和工具过滤                  │  │
│  └─────────────────────────────────────────────────┘  │
│                                                       │
│  Multi-Agent 编排层（AgentOrchestrator）              │
│  ├── Supervisor + Workers 模式                        │
│  ├── Peer-to-Peer 消息路由                            │
│  └── Pipeline DAG 执行引擎                            │
└─────────────────────────────────────────────────────┘
```

---

## 三、Mono-Repo 目录结构

```
octo-sandbox/                        ← git 仓库根
├── crates/
│   ├── octo-types/                  ← 共享类型（两产品）
│   ├── octo-engine/                 ← 共享核心引擎（两产品）
│   ├── octo-server/                 ← workbench 专用（workbench 分支）
│   └── octo-platform-server/        ← platform 专用（platform 分支，新增）
│       ├── api/                     # REST + WebSocket
│       ├── tenant/                  # TenantManager, TenantRuntime
│       ├── user/                    # UserManager, UserRuntime
│       ├── quota/                   # 资源配额
│       ├── auth/                    # JWT + OAuth2/OIDC
│       ├── orchestrator/            # Multi-Agent DAG 执行
│       └── audit/                   # 操作审计
│
├── web/                             ← workbench 前端（workbench 分支）
├── web-platform/                    ← platform 前端（platform 分支，新增）
│   └── src/
│       ├── admin/                   # 租户/用户/配额管理
│       ├── workspace/               # 用户 Agent 工作空间
│       └── orchestrator/            # DAG 可视化
│
└── design/                          ← 共享前端设计 token（两分支共用）
    ├── tailwind.base.ts             # 共享 Tailwind 基础配置
    └── tokens.css                   # CSS 变量（颜色、字体、间距）
```

---

## 四、认证系统

### 两阶段实现

**开发阶段：** 自建用户系统
- 用户名/密码 + JWT
- 平台自己管理账号
- 简单可控，快速启动

**企业部署阶段：** 插拔式 OAuth2/OIDC
- 对接 Google、GitHub、Okta、Azure AD 等企业 SSO
- 配置式接入，不重复造轮子
- 本地账号 + 外部 SSO 可同时存在

### 权限分层

```
超级管理员（Platform Admin）
  └── 租户管理员（Tenant Admin）
        └── 普通用户（Member）
              └── 只读用户（Viewer）
```

---

## 五、前端策略

### 原则

- **不共享业务组件**：Chat、Memory、Tools 等在两个产品中逻辑有差异，各自独立实现
- **共享设计 token**：颜色、字体、间距通过 `design/` 目录统一
- **复制基础 UI 原语**：Button、Input、Badge、Modal 等无业务逻辑的组件，platform 启动时从 workbench 复制一次，之后独立演进

### 设计 Token 结构

**`design/tailwind.base.ts`（共享）：**
```typescript
export const baseConfig = {
  theme: {
    extend: {
      colors: {
        primary:   { DEFAULT: 'var(--color-primary)' },
        secondary: { DEFAULT: 'var(--color-secondary)' },
        surface:   { DEFAULT: 'var(--color-surface)' },
        border:    { DEFAULT: 'var(--color-border)' },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
      },
      borderRadius: {
        sm: '4px', md: '8px', lg: '12px',
      },
    },
  },
}
```

**`design/tokens.css`（共享）：**
```css
:root {
  --color-primary:   #6366f1;
  --color-secondary: #8b5cf6;
  --color-surface:   #ffffff;
  --color-border:    #e5e7eb;
  --color-text:      #111827;
}
[data-theme="dark"] {
  --color-surface: #1f2937;
  --color-border:  #374151;
  --color-text:    #f9fafb;
}
```

各自前端的 `tailwind.config.ts` extends `design/tailwind.base.ts`，无需 npm 包，相对路径引入。

---

## 六、Multi-Agent 编排层

### 统一抽象：AgentGraph（DAG）

三种模式统一用有向图表达，区别在于拓扑结构和消息路由规则：

```
┌─────────────────────────────────────────────────────┐
│                 AgentOrchestrator                    │
│                                                      │
│  AgentGraph（DAG）                                   │
│  ├── 节点（AgentNode）= 一个 AgentRuntime 实例      │
│  ├── 边（GraphEdge）= 消息/数据流向                 │
│  └── 拓扑类型决定路由规则                           │
│                                                      │
│  ┌────────────┬──────────────┬──────────────┐       │
│  │ Supervisor  │  Peer-to-Peer │   Pipeline   │       │
│  │            │              │              │       │
│  │  Root      │  A ←──→ B   │  A → B → C  │       │
│  │  ├─Worker A │  ↕          │             │       │
│  │  └─Worker B │  C ←──→ D   │  并行分支：  │       │
│  │            │              │  A → B      │       │
│  │  Root 分解 │  任意节点可  │      ↘ D   │       │
│  │  任务分发  │  向任意节点  │  A → C ↗   │       │
│  │  汇总结果  │  发消息      │             │       │
│  └────────────┴──────────────┴──────────────┘       │
└─────────────────────────────────────────────────────┘
```

### 核心数据结构

```rust
pub struct AgentGraph {
    pub id: GraphId,
    pub tenant_id: TenantId,
    pub topology: GraphTopology,       // Supervisor / Peer / Pipeline
    pub nodes: Vec<AgentNode>,
    pub edges: Vec<GraphEdge>,
    pub status: GraphStatus,           // Pending / Running / Done / Failed
}

pub enum GraphTopology {
    Supervisor,   // 树形，有 Root 节点分配任务
    Peer,         // 任意图，Agent 间平等通信
    Pipeline,     // 线性或带分支的 DAG
}

pub struct AgentNode {
    pub id: NodeId,
    pub agent_id: AgentId,
    pub role: NodeRole,                // Root / Worker / Peer / Stage
    pub input_schema: Option<Schema>,
    pub output_schema: Option<Schema>,
}

pub struct GraphEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub condition: Option<EdgeCondition>, // 条件路由（如：仅当输出含 "error"）
}

// Agent 间消息
pub struct AgentInterMessage {
    pub id: MessageId,
    pub graph_id: GraphId,
    pub from: NodeId,
    pub to: NodeId,
    pub content: String,
    pub message_type: InterMessageType,
}

pub enum InterMessageType {
    Task,       // Root → Worker：分配子任务
    Result,     // Worker → Root：返回结果
    Delegate,   // 任意节点委托子任务
    Broadcast,  // 广播给所有节点
}
```

### octo-engine 最小扩展接口

编排逻辑全部在 `octo-platform-server/orchestrator/` 实现，octo-engine 只加最小接口：

```rust
impl AgentRuntime {
    /// 接收来自其他 Agent 的任务，执行后返回结果
    pub async fn execute_agent_task(
        &self,
        task: AgentInterMessage,
    ) -> AgentTaskResult { ... }

    /// 向 Orchestrator 注册消息回调（用于 Peer 模式）
    pub fn register_message_handler(
        &self,
        handler: Arc<dyn AgentMessageHandler>,
    ) { ... }
}
```

**原则**：不向 octo-engine 注入编排概念，保持引擎层纯粹。

---

## 七、核心数据模型

```rust
// ── 租户层 ──────────────────────────────────────────

pub struct Tenant {
    pub id: TenantId,                      // UUID
    pub name: String,
    pub slug: String,                      // URL 友好标识（如 "acme-corp"）
    pub plan: TenantPlan,                  // Free / Pro / Enterprise
    pub quota: ResourceQuota,
    pub mcp_config: Vec<McpServerConfig>,  // 租户级共享 MCP
    pub auth_config: TenantAuthConfig,     // SSO 配置（OIDC issuer 等）
    pub db_path: PathBuf,                  // 租户独立 SQLite 文件
    pub created_at: DateTime<Utc>,
}

pub struct ResourceQuota {
    pub max_agents: u32,                   // 并发 Agent 实例上限
    pub max_sessions_per_user: u32,        // 每用户并发会话上限
    pub max_api_calls_per_day: u64,        // 每日 LLM 调用上限
    pub max_memory_mb: u64,                // 记忆存储上限
    pub max_mcp_servers: u32,             // MCP 服务器数量上限
}

// ── 用户层 ──────────────────────────────────────────

pub struct PlatformUser {
    pub id: UserId,
    pub tenant_id: TenantId,
    pub email: String,
    pub display_name: String,
    pub role: UserRole,                    // TenantAdmin / Member / Viewer
    pub auth_provider: AuthProvider,       // Local / Google / GitHub / Okta
    pub api_keys: Vec<ApiKey>,
    pub created_at: DateTime<Utc>,
}

pub enum UserRole {
    TenantAdmin,  // 管理租户配置、用户、配额
    Member,       // 使用 Agent，创建任务图
    Viewer,       // 只读查看
}

// ── 运行时映射 ───────────────────────────────────────

pub struct PlatformState {
    // 懒加载：首次请求时创建，长期不活跃后回收
    pub tenants: DashMap<TenantId, Arc<TenantRuntime>>,
    pub config: PlatformConfig,
}

pub struct TenantRuntime {
    pub tenant: Tenant,
    // 每用户独立 AgentRuntime（懒加载）
    pub user_runtimes: DashMap<UserId, Arc<AgentRuntime>>,
    pub orchestrator: Arc<AgentOrchestrator>,
}
```

---

## 八、实施路线图

基于 octo-workbench v1.0 完成为起点，分四个阶段（总计约 18 周）：

```
octo-workbench v1.0 → P1（~4周）→ P2（~4周）→ P3（~6周）→ P4（~4周）
  认证+单租户多用户    多租户+配额    Multi-Agent    生产就绪
```

### Platform P1：认证 + 单租户多用户

**目标**：`octo-platform-server` 可运行，多用户登录，每用户独立 AgentRuntime。

| 任务 | 内容 |
|------|------|
| P1-1 | 新建 `octo-platform-server` crate，基础 Axum 服务 |
| P1-2 | 自建用户系统：注册/登录/JWT |
| P1-3 | `PlatformState`：单租户 + `DashMap<UserId, Arc<AgentRuntime>>` |
| P1-4 | 每用户独立 WebSocket + AgentRuntime 懒加载 |
| P1-5 | Admin API：用户 CRUD、角色管理 |
| P1-6 | `web-platform/` 初始化：登录页 + 用户工作空间 |

**验收**：3 用户同时登录，各自独立对话，互不干扰。

### Platform P2：多租户 + 配额 + MCP 隔离

**目标**：完整两级隔离，租户间物理隔离，资源配额生效。

| 任务 | 内容 |
|------|------|
| P2-1 | `TenantManager`：租户 CRUD，独立 SQLite 文件 |
| P2-2 | `QuotaManager`：配额检查中间件，超限返回 429 |
| P2-3 | 租户级 MCP 配置：每租户独立 McpManager |
| P2-4 | 超级管理员控制台：租户管理、配额设置 |
| P2-5 | 插拔式 OAuth2/OIDC（Google、GitHub 优先） |
| P2-6 | 审计日志：所有操作记录（操作人、时间、资源） |

**验收**：两租户数据完全隔离，配额超限 429，SSO 登录正常。

### Platform P3：Multi-Agent 编排

**目标**：三种编排模式可运行，有可视化界面。

| 任务 | 内容 |
|------|------|
| P3-1 | `AgentOrchestrator` 核心：DAG 存储 + 执行引擎 |
| P3-2 | Supervisor 模式：Root 分解任务，Workers 并行执行 |
| P3-3 | Pipeline 模式：节点串并行，条件路由 |
| P3-4 | Peer 模式：Agent 间消息总线，双向通信 |
| P3-5 | octo-engine 最小扩展：`execute_agent_task()` 接口 |
| P3-6 | `web-platform/orchestrator/`：DAG 可视化（节点/边/状态实时更新） |
| P3-7 | 编排执行历史：节点日志、耗时、结果 |

**验收**：三模式各跑通端到端示例，DAG 界面实时显示节点状态。

### Platform P4：生产就绪

**目标**：可对外发布，有监控、容灾、文档。

| 任务 | 内容 |
|------|------|
| P4-1 | Prometheus 指标暴露（租户/用户/Agent 维度） |
| P4-2 | `/health` 涵盖所有组件状态 |
| P4-3 | Docker Compose + K8s Helm Chart |
| P4-4 | OpenAPI 3.1 开发者 API 文档 |
| P4-5 | 压测：100 并发用户，验证配额和隔离稳定性 |
| P4-6 | 数据迁移工具：从 octo-workbench 导入历史数据 |

**验收**：`docker compose up` 一键启动，`helm install` K8s 部署，API 文档完整。

---

## 设计决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| 产品关系 | 独立产品共享 octo-engine | 避免向下兼容负担，各自独立演进 |
| 仓库结构 | Mono-repo，未来可拆 | 引擎未成熟时共同演进更高效 |
| 前端共享策略 | 不共享业务组件，只共享设计 token | 避免 props 爆炸，YAGNI |
| 认证 | 自建 + 插拔式 SSO | 开发快，企业部署灵活 |
| 隔离粒度 | 租户级 + 用户级两层 | 覆盖三种部署模式 |
| 编排模式 | Supervisor/Peer/Pipeline 三种 | 按场景选择，用户自由配置拓扑 |

---

## Deferred（暂缓项）

> 本阶段已知但暂未实现的功能点。每次开始新 Task 前先检查此列表。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| D1 | agent_pool.rs:467 TODO - Session Store 持久化实现 | P1-4 Workspace 持久化完成后，扩展到 Session Store | ✅ 已补 |
| D2 | runtime.rs:86 - AgentExecutor observability 转发 | AgentExecutor observability 集成设计完成后 | ⏳ |
| D3 | middleware.rs:50 - 认证中间件 NOT_IMPLEMENTED 分支 | OAuth2/OIDC 实现完成后 | ✅ 已补 |
