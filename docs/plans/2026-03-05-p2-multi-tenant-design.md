# Platform P2: 多租户 + 配额 + MCP 隔离设计方案

> 日期：2026-03-05
> 状态：设计完成

## 一、架构总览

```
┌─────────────────────────────────────────────────────────────┐
│                   octo-platform-server                       │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐  │
│  │  TenantManager                                       │  │
│  │  ├── Tenant CRUD                                     │  │
│  │  ├── TenantRuntime (per-tenant)                     │  │
│  │  │   ├── McpManager (独立)                         │  │
│  │  │   ├── QuotaManager (滑动窗口)                   │  │
│  │  │   └── EventBus (审计)                           │  │
│  │  └── PlatformUser (per-user)                        │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐  │
│  │  QuotaMiddleware                                    │  │
│  │  └── 429 Too Many Requests                          │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐  │
│  │  OAuth2/Auth Module (可插拔)                        │  │
│  │  ├── Local (用户名/密码)                           │  │
│  │  ├── Google OAuth2                                  │  │
│  │  └── GitHub OAuth2                                  │  │
│  └─────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## 二、核心组件设计

### 2.1 TenantManager

```rust
// 租户管理器
pub struct TenantManager {
    conn: Arc<Mutex<Connection>>,           // 平台级共享数据库
    tenant_runtimes: DashMap<TenantId, Arc<TenantRuntime>>,
    config: PlatformConfig,
}

impl TenantManager {
    /// 创建租户
    pub async fn create_tenant(&self, req: CreateTenantRequest) -> Result<Tenant> {
        // 1. 生成 TenantId
        // 2. 创建租户数据目录
        // 3. 初始化租户 SQLite (可选)
        // 4. 写入平台数据库
    }

    /// 获取租户运行时（懒加载）
    pub fn get_or_create_runtime(&self, tenant_id: &TenantId) -> Arc<TenantRuntime> {
        self.tenant_runtimes
            .entry(tenant_id.clone())
            .or_insert_with(|| Arc::new(TenantRuntime::new(...)))
            .value()
            .clone()
    }
}
```

### 2.2 TenantRuntime

```rust
// 租户运行时 - 完全隔离
pub struct TenantRuntime {
    pub tenant_id: TenantId,
    pub mcp_manager: Arc<McpManager>,        // 独立 MCP 管理器
    pub quota_manager: Arc<QuotaManager>,    // 配额管理器
    pub event_bus: Arc<EventBus>,           // 审计事件
    pub db: Arc<Mutex<Connection>>,         // 租户独立数据库
}

impl TenantRuntime {
    pub fn new(tenant_id: TenantId, config: TenantConfig) -> Self {
        Self {
            tenant_id: tenant_id.clone(),
            mcp_manager: Arc::new(McpManager::new(tenant_id.to_string())),
            quota_manager: Arc::new(QuotaManager::new(config.quota)),
            event_bus: Arc::new(EventBus::new()),
            db: Self::open_tenant_db(&tenant_id),
        }
    }
}
```

### 2.3 QuotaManager (滑动窗口)

```rust
// 配额管理器 - 滑动窗口计数器
pub struct QuotaManager {
    quota: ResourceQuota,
    // 滑动窗口计数器
    daily_api_calls: AtomicU64,
    active_sessions: AtomicU32,
    active_agents: AtomicU32,
    window_start: AtomicU64,  // UTC 秒数
}

impl QuotaManager {
    /// 检查配额
    pub fn check(&self, resource: Resource) -> Result<(), QuotaExceeded> {
        // 1. 检查并重置窗口（如需要）
        // 2. 比较当前使用量与配额
        // 3. 返回结果
    }

    /// 消耗配额
    pub fn consume(&self, resource: Resource, amount: u32) -> Result<(), QuotaExceeded> {
        // 1. 检查配额
        // 2. 原子递增计数器
    }
}

#[derive(Debug)]
pub enum QuotaExceeded {
    DailyApiCalls { limit: u64, used: u64 },
    ActiveSessions { limit: u32, used: u32 },
    ActiveAgents { limit: u32, used: u32 },
    McpServers { limit: u32, used: u32 },
}
```

### 2.4 OAuth2 抽象

```rust
// 可插拔认证 Provider
pub trait AuthProvider: Send + Sync {
    fn name(&self) -> &str;
    fn auth_url(&self, state: &str) -> String;
    fn exchange_code(&self, code: &str) -> Result<AuthUser, AuthError>;
    fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse, AuthError>;
}

// Provider 注册表
pub struct AuthModule {
    providers: DashMap<String, Box<dyn AuthProvider>>,
    jwt: Arc<JwtManager>,
}

impl AuthModule {
    pub fn register(&self, provider: Box<dyn AuthProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    pub fn get(&self, name: &str) -> Option<&Box<dyn AuthProvider>> {
        self.providers.get(name)
    }
}

// 内置 Providers
pub struct GoogleOAuth2Provider { client_id: String, client_secret: String, redirect_uri: String }
pub struct GitHubOAuth2Provider { client_id: String, client_secret: String, redirect_uri: String }
```

### 2.5 审计日志（事件驱动）

```rust
// 审计事件
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub tenant_id: TenantId,
    pub user_id: Option<UserId>,
    pub action: AuditAction,
    pub resource: ResourceRef,
    pub timestamp: DateTime<Utc>,
    pub ip_address: Option<IpAddr>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AuditAction {
    Login,
    Logout,
    CreateAgent,
    DeleteAgent,
    CreateSession,
    DeleteSession,
    UpdateQuota,
    UpdateTenantConfig,
    // ...
}

// TenantRuntime 初始化时设置审计处理器
impl TenantRuntime {
    pub fn new(...) -> Self {
        let event_bus = Arc::new(EventBus::new());

        // 注册审计处理器
        event_bus.subscribe(AuditHandler::new(self.db.clone()));

        Self { event_bus, ... }
    }
}

// 审计处理器
struct AuditHandler {
    conn: Arc<Mutex<Connection>>,
}

impl EventHandler for AuditHandler {
    fn handle(&self, event: Event) {
        if let Ok(audit) = serde_json::from_value::<AuditEvent>(event.payload) {
            self.write_audit(&audit);
        }
    }
}
```

---

## 三、数据模型

### 3.1 租户表（平台数据库）

```sql
CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    plan TEXT NOT NULL DEFAULT 'free',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE tenant_quotas (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants(id),
    max_agents INTEGER NOT NULL DEFAULT 5,
    max_sessions_per_user INTEGER NOT NULL DEFAULT 10,
    max_api_calls_per_day INTEGER NOT NULL DEFAULT 1000,
    max_memory_mb INTEGER NOT NULL DEFAULT 1024,
    max_mcp_servers INTEGER NOT NULL DEFAULT 5
);

CREATE TABLE tenant_mcp_configs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    name TEXT NOT NULL,
    config TEXT NOT NULL,  -- JSON
    created_at INTEGER NOT NULL
);
```

### 3.2 用户表（扩展现有）

```sql
ALTER TABLE users ADD COLUMN tenant_id TEXT NOT NULL REFERENCES tenants(id);
ALTER TABLE users ADD COLUMN auth_provider TEXT NOT NULL DEFAULT 'local';
```

### 3.3 审计日志表（租户数据库）

```sql
CREATE TABLE audit_logs (
    id TEXT PRIMARY KEY,
    user_id TEXT,
    action TEXT NOT NULL,
    resource_type TEXT,
    resource_id TEXT,
    details TEXT,  -- JSON
    ip_address TEXT,
    created_at INTEGER NOT NULL,
    INDEX idx_tenant_action (tenant_id, action, created_at)
);
```

---

## 四、API 端点设计

### 4.1 租户管理（Platform Admin）

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | /api/admin/tenants | 创建租户 |
| GET | /api/admin/tenants | 列表 |
| GET | /api/admin/tenants/:id | 详情 |
| PATCH | /api/admin/tenants/:id | 更新 |
| DELETE | /api/admin/tenants/:id | 删除 |
| GET | /api/admin/tenants/:id/quotas | 配额详情 |
| PATCH | /api/admin/tenants/:id/quotas | 更新配额 |

### 4.2 租户 MCP 配置

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | /api/v1/mcp | 列表 |
| POST | /api/v1/mcp | 添加 |
| DELETE | /api/v1/mcp/:id | 删除 |

### 4.3 OAuth2 认证

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | /auth/:provider | 开始 OAuth 流程 |
| GET | /auth/:provider/callback | OAuth 回调 |

---

## 五、实施任务拆分

| Task | 内容 | 依赖 |
|------|------|------|
| Task 1 | 扩展 User 表添加 tenant_id | P1 |
| Task 2 | TenantManager + TenantRuntime | - |
| Task 3 | QuotaManager 滑动窗口 | Task 2 |
| Task 4 | QuotaMiddleware 429 | Task 3 |
| Task 5 | 租户 MCP 配置 API | Task 2 |
| Task 6 | OAuth2 抽象 + Google/GitHub | - |
| Task 7 | 审计日志事件驱动 | Task 2 |
| Task 8 | Admin 租户管理界面 | Task 2 |
| Task 9 | 构建验证 | - |

---

## 六、与 Phase 2.11 的关系

| Phase 2.11 (octo-engine) | P2 (octo-platform) |
|---------------------------|-------------------|
| TenantId (Agent 索引) | TenantManager (租户管理) |
| TenantContext (user_id) | 用户已有 tenant_id |
| AgentRuntime per user | TenantRuntime 包含 AgentRuntime |

**结论**：P2 可以与 Phase 2.11 平行实施，P2 关注租户级别隔离，Phase 2.11 关注 Agent 级别隔离。

---

## 七、验收标准

| 检查项 | 验收标准 |
|--------|---------|
| 租户隔离 | 两个租户数据完全隔离，跨租户访问返回 403 |
| 配额检查 | 超限返回 429，包含 Retry-After 头 |
| MCP 隔离 | 每租户 MCP 配置独立，互不影响 |
| OAuth2 | Google/GitHub 登录正常，token 有效 |
| 审计日志 | 所有操作记录可查，包含操作人/时间/资源 |
| Admin | 租户 CRUD 正常，配额设置生效 |
