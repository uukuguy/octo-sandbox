# Platform P2: 多租户 + 配额 + MCP 隔离实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标**: 实现两级隔离（租户级 + 用户级），配额管理，OAuth2 认证，审计日志

**架构**: 共享数据库 + tenant_id 隔离，每租户独立 McpManager，滑动窗口配额检查，事件驱动审计

**技术栈**: Rust async/tokio, Axum, DashMap, SQLite (rusqlite)

---

## Task 1: 扩展 User 表添加 tenant_id

**Files:**
- Modify: `crates/octo-platform-server/src/db/users.rs:52-60`
- Modify: `crates/octo-platform-server/src/db/users.rs:127-145`

**Step 1: 修改 User 结构体**

在 `User` 结构体添加 `tenant_id` 字段：

```rust
/// Platform user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub tenant_id: String,  // 新增
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}
```

**Step 2: 修改数据库迁移**

在 `UserDatabase::open` 中添加表迁移：

```rust
pub fn open(data_dir: &Path) -> Result<Self> {
    let db_path = data_dir.join("users.db");
    let conn = Connection::open(&db_path)?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT 'default',
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            display_name TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'member',
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_users_tenant ON users(tenant_id);
        "
    )?;

    Ok(Self { conn: Mutex::new(conn) })
}
```

**Step 3: 修改 register 方法**

```rust
pub fn register(&self, req: &RegisterRequest) -> Result<User> {
    let id = Uuid::new_v4().to_string();
    let tenant_id = "default".to_string();  // 默认租户
    // ... 其他字段
}
```

**Step 4: 修改 UserResponse**

```rust
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub tenant_id: String,  // 新增
    pub email: String,
    pub display_name: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}
```

**Step 5: 验证编译**

```bash
cargo check -p octo-platform-server 2>&1 | grep "^error" | head -10
```

**Step 6: Commit**

```bash
git add crates/octo-platform-server/src/db/users.rs
git commit -m "feat(platform): add tenant_id to User model for multi-tenant support"
```

---

## Task 2: TenantManager + TenantRuntime

**Files:**
- Create: `crates/octo-platform-server/src/tenant/mod.rs`
- Create: `crates/octo-platform-server/src/tenant/manager.rs`
- Create: `crates/octo-platform-server/src/tenant/runtime.rs`
- Create: `crates/octo-platform-server/src/tenant/models.rs`
- Modify: `crates/octo-platform-server/src/lib.rs`

**Step 1: 创建租户数据模型**

```rust
// crates/octo-platform-server/src/tenant/models.rs

use serde::{Deserialize, Serialize};

/// Tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub plan: TenantPlan,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TenantPlan {
    Free,
    Pro,
    Enterprise,
}

impl Default for TenantPlan {
    fn default() -> Self { TenantPlan::Free }
}

/// Resource quota
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuota {
    pub max_agents: u32,
    pub max_sessions_per_user: u32,
    pub max_api_calls_per_day: u64,
    pub max_memory_mb: u64,
    pub max_mcp_servers: u32,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            max_agents: 5,
            max_sessions_per_user: 10,
            max_api_calls_per_day: 1000,
            max_memory_mb: 1024,
            max_mcp_servers: 5,
        }
    }
}
```

**Step 2: 创建 TenantRuntime**

```rust
// crates/octo-platform-server/src/tenant/runtime.rs

use std::sync::Arc;
use dashmap::DashMap;

/// Tenant runtime - isolated per tenant
pub struct TenantRuntime {
    pub tenant_id: String,
    pub quota: super::models::ResourceQuota,
    pub mcp_servers: DashMap<String, serde_json::Value>,
}

impl TenantRuntime {
    pub fn new(tenant_id: String, quota: super::models::ResourceQuota) -> Self {
        Self {
            tenant_id,
            quota,
            mcp_servers: DashMap::new(),
        }
    }
}
```

**Step 3: 创建 TenantManager**

```rust
// crates/octo-platform-server/src/tenant/manager.rs

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use rusqlite::Connection;

use super::models::{ResourceQuota, Tenant, TenantPlan};
use super::runtime::TenantRuntime;

pub struct TenantManager {
    conn: Arc<Mutex<Connection>>,
    runtimes: DashMap<String, Arc<TenantRuntime>>,
    data_dir: PathBuf,
}

impl TenantManager {
    pub fn new(data_dir: PathBuf) -> Result<Self, anyhow::Error> {
        let conn = Connection::open(data_dir.join("platform.db"))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tenants (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                plan TEXT NOT NULL DEFAULT 'free',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tenant_quotas (
                tenant_id TEXT PRIMARY KEY REFERENCES tenants(id),
                max_agents INTEGER NOT NULL DEFAULT 5,
                max_sessions_per_user INTEGER NOT NULL DEFAULT 10,
                max_api_calls_per_day INTEGER NOT NULL DEFAULT 1000,
                max_memory_mb INTEGER NOT NULL DEFAULT 1024,
                max_mcp_servers INTEGER NOT NULL DEFAULT 5
            );
            "
        )?;

        // 创建默认租户
        Self::ensure_default_tenant(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            runtimes: DashMap::new(),
            data_dir,
        })
    }

    fn ensure_default_tenant(conn: &Connection) -> Result<(), anyhow::Error> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tenants WHERE id = 'default'",
            [],
            |row| row.get(0)
        )?;

        if count == 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO tenants (id, name, slug, plan, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
                rusqlite::params!["default", "Default Tenant", "default", "free", now, now],
            )?;

            conn.execute(
                "INSERT INTO tenant_quotas (tenant_id, max_agents, max_sessions_per_user, max_api_calls_per_day, max_memory_mb, max_mcp_servers) VALUES (?, ?, ?, ?, ?, ?)",
                rusqlite::params!["default", 5, 10, 1000, 1024, 5],
            )?;
        }
        Ok(())
    }

    pub fn get_or_create_runtime(&self, tenant_id: &str) -> Arc<TenantRuntime> {
        self.runtimes
            .entry(tenant_id.to_string())
            .or_insert_with(|| {
                let quota = self.get_quota(tenant_id).unwrap_or_default();
                Arc::new(TenantRuntime::new(tenant_id.to_string(), quota))
            })
            .value()
            .clone()
    }

    pub fn get_quota(&self, tenant_id: &str) -> Result<ResourceQuota, anyhow::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT max_agents, max_sessions_per_user, max_api_calls_per_day, max_memory_mb, max_mcp_servers FROM tenant_quotas WHERE tenant_id = ?"
        )?;

        let quota = stmt.query_row(rusqlite::params![tenant_id], |row| {
            Ok(ResourceQuota {
                max_agents: row.get(0)?,
                max_sessions_per_user: row.get(1)?,
                max_api_calls_per_day: row.get(2)?,
                max_memory_mb: row.get(3)?,
                max_mcp_servers: row.get(4)?,
            })
        })?;

        Ok(quota)
    }
}
```

**Step 4: 创建模块入口**

```rust
// crates/octo-platform-server/src/tenant/mod.rs

pub mod models;
pub mod runtime;
pub mod manager;

pub use manager::TenantManager;
pub use models::{ResourceQuota, Tenant, TenantPlan};
pub use runtime::TenantRuntime;
```

**Step 5: 注册模块**

在 `lib.rs` 添加：

```rust
pub mod tenant;
pub use tenant::{TenantManager, TenantRuntime, Tenant, TenantPlan, ResourceQuota};
```

**Step 6: 验证编译**

```bash
cargo check -p octo-platform-server 2>&1 | grep "^error" | head -10
```

**Step 7: Commit**

```bash
git add crates/octo-platform-server/src/tenant/
git add crates/octo-platform-server/src/lib.rs
git commit -m "feat(platform): add TenantManager and TenantRuntime for multi-tenant isolation"
```

---

## Task 3: QuotaManager 滑动窗口

**Files:**
- Create: `crates/octo-platform-server/src/tenant/quota.rs`
- Modify: `crates/octo-platform-server/src/tenant/mod.rs`
- Modify: `crates/octo-platform-server/src/tenant/runtime.rs`

**Step 1: 创建 QuotaManager**

```rust
// crates/octo-platform-server/src/tenant/quota.rs

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::models::ResourceQuota;

const WINDOW_SECONDS: u64 = 86400; // 24 hours

pub struct QuotaManager {
    quota: ResourceQuota,
    daily_api_calls: AtomicU64,
    active_sessions: AtomicU32,
    active_agents: AtomicU32,
    window_start: AtomicU64,
}

impl QuotaManager {
    pub fn new(quota: ResourceQuota) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            quota,
            daily_api_calls: AtomicU64::new(0),
            active_sessions: AtomicU64::new(0) as AtomicU32,
            active_agents: AtomicU32::new(0),
            window_start: AtomicU64::new(now),
        }
    }

    fn check_window(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let window = self.window_start.load(Ordering::Relaxed);
        if now - window >= WINDOW_SECONDS {
            // Reset window
            self.window_start.store(now, Ordering::Relaxed);
            self.daily_api_calls.store(0, Ordering::Relaxed);
        }
    }

    pub fn check_api_call(&self) -> Result<(), QuotaExceeded> {
        self.check_window();

        let used = self.daily_api_calls.load(Ordering::Relaxed);
        let limit = self.quota.max_api_calls_per_day;

        if used >= limit {
            return Err(QuotaExceeded::DailyApiCalls {
                limit,
                used,
            });
        }
        Ok(())
    }

    pub fn consume_api_call(&self) -> Result<(), QuotaExceeded> {
        self.check_api_call()?;
        self.daily_api_calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn check_active_sessions(&self) -> Result<(), QuotaExceeded> {
        let used = self.active_sessions.load(Ordering::Relaxed);
        let limit = self.quota.max_sessions_per_user;

        if used >= limit {
            return Err(QuotaExceeded::ActiveSessions {
                limit,
                used,
            });
        }
        Ok(())
    }

    pub fn acquire_session(&self) -> SessionGuard {
        self.active_sessions.fetch_add(1, Ordering::Relaxed);
        SessionGuard {
            manager: self,
        }
    }
}

pub struct SessionGuard {
    manager: &QuotaManager,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.manager.active_sessions.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub enum QuotaExceeded {
    DailyApiCalls { limit: u64, used: u64 },
    ActiveSessions { limit: u32, used: u32 },
    ActiveAgents { limit: u32, used: u32 },
    McpServers { limit: u32, used: u32 },
}

impl std::fmt::Display for QuotaExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DailyApiCalls { limit, used } =>
                write!(f, "Daily API call limit exceeded: {}/{}", used, limit),
            Self::ActiveSessions { limit, used } =>
                write!(f, "Active session limit exceeded: {}/{}", used, limit),
            Self::ActiveAgents { limit, used } =>
                write!(f, "Active agent limit exceeded: {}/{}", used, limit),
            Self::McpServers { limit, used } =>
                write!(f, "MCP server limit exceeded: {}/{}", used, limit),
        }
    }
}
```

**Step 2: 集成到 TenantRuntime**

```rust
// crates/octo-platform-server/src/tenant/runtime.rs

use super::quota::QuotaManager;

pub struct TenantRuntime {
    pub tenant_id: String,
    pub quota: super::models::ResourceQuota,
    pub quota_manager: QuotaManager,  // 新增
    pub mcp_servers: DashMap<String, serde_json::Value>,
}

impl TenantRuntime {
    pub fn new(tenant_id: String, quota: super::models::ResourceQuota) -> Self {
        Self {
            tenant_id,
            quota: quota.clone(),
            quota_manager: QuotaManager::new(quota),
            mcp_servers: DashMap::new(),
        }
    }
}
```

**Step 3: 验证编译**

```bash
cargo check -p octo-platform-server 2>&1 | grep "^error" | head -10
```

**Step 4: Commit**

```bash
git add crates/octo-platform-server/src/tenant/
git commit -m "feat(platform): add QuotaManager with sliding window"
```

---

## Task 4: QuotaMiddleware 429

**Files:**
- Create: `crates/octo-platform-server/src/middleware/quota.rs`
- Modify: `crates/octo-platform-server/src/main.rs`

**Step 1: 创建配额中间件**

```rust
// crates/octo-platform-server/src/middleware/quota.rs

use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode},
    middleware::Next,
};
use std::sync::Arc;

use crate::tenant::TenantRuntime;

pub async fn quota_middleware(
    tenant: Option<Arc<TenantRuntime>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    if let Some(tenant) = tenant {
        // Check API call quota
        if let Err(e) = tenant.quota_manager.check_api_call() {
            let retry_after = 60; // seconds
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("Retry-After", retry_after.to_string())
                .body(Body::from(e.to_string()))
                .unwrap();
        }
    }

    next.run(request).await
}
```

**Step 2: 注册中间件**

在 `main.rs` 中集成配额检查到路由。

**Step 3: 验证编译**

```bash
cargo check -p octo-platform-server 2>&1 | grep "^error" | head -10
```

**Step 4: Commit**

```bash
git add crates/octo-platform-server/src/middleware/
git commit -m "feat(platform): add QuotaMiddleware returning 429 when exceeded"
```

---

## Task 5: 租户 MCP 配置 API

**Files:**
- Modify: `crates/octo-platform-server/src/tenant/runtime.rs`
- Create: `crates/octo-platform-server/src/api/mcp.rs`
- Modify: `crates/octo-platform-server/src/main.rs`

**Step 1: 扩展 TenantRuntime MCP 支持**

在 `TenantRuntime` 添加 MCP 配置管理方法。

**Step 2: 创建 MCP API 端点**

```rust
// crates/octo-platform-server/src/api/mcp.rs

use axum::{extract::State, Json, Router};
use std::sync::Arc;

use crate::{AppState, ErrorResponse};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/mcp", get(list_mcp).post(add_mcp))
        .route("/api/v1/mcp/:id", delete(delete_mcp))
}

async fn list_mcp(State(state): State<Arc<AppState>>) -> Json<Vec<serde_json::Value>> {
    let runtime = state.tenant_manager.get_or_create_runtime("default");
    Json(runtime.mcp_servers.iter().map(|v| v.value().clone()).collect())
}

async fn add_mcp(
    State(state): State<Arc<AppState>>,
    Json(config): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let runtime = state.tenant_manager.get_or_create_runtime("default");
    let id = uuid::Uuid::new_v4().to_string();
    runtime.mcp_servers.insert(id.clone(), config.clone());
    Ok(Json(config))
}

async fn delete_mcp(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    let runtime = state.tenant_manager.get_or_create_runtime("default");
    if runtime.mcp_servers.remove(&id).is_some() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ErrorResponse { error: "Not found".to_string() })
    }
}
```

**Step 3: 验证编译**

```bash
cargo check -p octo-platform-server 2>&1 | grep "^error" | head -10
```

**Step 4: Commit**

```bash
git add crates/octo-platform-server/src/api/mcp.rs
git commit -m "feat(platform): add tenant MCP config API"
```

---

## Task 6: OAuth2 抽象 + Google/GitHub

**Files:**
- Create: `crates/octo-platform-server/src/auth/providers.rs`
- Modify: `crates/octo-platform-server/src/auth/mod.rs`

**Step 1: 创建 OAuth2 Provider Trait**

```rust
// crates/octo-platform-server/src/auth/providers.rs

use async_trait::async_trait;

pub trait OAuthProvider: Send + Sync {
    fn name(&self) -> &str;
    fn auth_url(&self, state: &str, redirect_uri: &str) -> String;
    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<OAuthUser, OAuthError>;
}

#[derive(Debug)]
pub struct OAuthUser {
    pub provider: String,
    pub provider_user_id: String,
    pub email: String,
    pub name: Option<String>,
}

#[derive(Debug)]
pub enum OAuthError {
    InvalidCode,
    NetworkError(String),
    // ...
}
```

**Step 2: 实现 Google Provider**

```rust
pub struct GoogleOAuthProvider {
    client_id: String,
    client_secret: String,
}

impl GoogleOAuthProvider {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self { client_id, client_secret }
    }
}

#[async_trait]
impl OAuthProvider for GoogleOAuthProvider {
    fn name(&self) -> &str { "google" }

    fn auth_url(&self, state: &str, redirect_uri: &str) -> String {
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}",
            self.client_id, redirect_uri, state
        )
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<OAuthUser, OAuthError> {
        // Exchange code for token, then get user info
        // ...
    }
}
```

**Step 3: 实现 GitHub Provider**

类似 Google Provider 实现。

**Step 4: 验证编译**

```bash
cargo check -p octo-platform-server 2>&1 | grep "^error" | head -10
```

**Step 5: Commit**

```bash
git add crates/octo-platform-server/src/auth/
git commit -m "feat(platform): add OAuth2 abstraction with Google and GitHub providers"
```

---

## Task 7: 审计日志事件驱动

**Files:**
- Create: `crates/octo-platform-server/src/audit/mod.rs`
- Modify: `crates/octo-platform-server/src/tenant/runtime.rs`

**Step 1: 创建审计模块**

```rust
// crates/octo-platform-server/src/audit/mod.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub action: AuditAction,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AuditAction {
    Login,
    Logout,
    CreateAgent,
    DeleteAgent,
    CreateSession,
    DeleteSession,
    UpdateQuota,
    CreateMcp,
    DeleteMcp,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Login => write!(f, "login"),
            Self::Logout => write!(f, "logout"),
            // ...
        }
    }
}
```

**Step 2: 集成到 TenantRuntime**

在 `TenantRuntime` 添加 `publish_audit_event` 方法。

**Step 3: 验证编译**

```bash
cargo check -p octo-platform-server 2>&1 | grep "^error" | head -10
```

**Step 4: Commit**

```bash
git add crates/octo-platform-server/src/audit/
git commit -m "feat(platform): add event-driven audit logging"
```

---

## Task 8: Admin 租户管理界面

**Files:**
- Create: `crates/octo-platform-server/src/api/admin/tenants.rs`
- Modify: `crates/octo-platform-server/src/main.rs`

**Step 1: 创建 Admin API**

```rust
// crates/octo-platform-server/src/api/admin/tenants.rs

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use std::sync::Arc;

use crate::{AppState, ErrorResponse};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/admin/tenants", get(list_tenants).post(create_tenant))
        .route("/api/admin/tenants/:id", get(get_tenant).patch(update_tenant).delete(delete_tenant))
        .route("/api/admin/tenants/:id/quotas", get(get_quotas).patch(update_quotas))
}

// Implement handlers...
```

**Step 2: 验证编译**

```bash
cargo check -p octo-platform-server 2>&1 | grep "^error" | head -10
```

**Step 3: Commit**

```bash
git add crates/octo-platform-server/src/api/admin/
git commit -m "feat(platform): add admin tenant management API"
```

---

## Task 9: 构建验证

**Step 1: 完整编译检查**

```bash
cargo check --workspace 2>&1 | tail -5
```

**Step 2: 运行测试**

```bash
cargo test -p octo-platform-server 2>&1 | tail -20
```

**Step 3: TypeScript 检查**

```bash
cd web-platform && npx tsc --noEmit 2>&1 | tail -10 && cd ..
```

**Step 4: 更新 checkpoint**

更新 `docs/plans/.checkpoint.json` 添加 P2 进度。

**Step 5: Commit**

```bash
git add docs/plans/.checkpoint.json
git commit -m "fix(platform): P2 complete - multi-tenant + quota + MCP isolation"
```

---

## 完成标准

| 检查项 | 验收标准 |
|--------|---------|
| 编译 | `cargo check --workspace` 0 errors |
| 租户隔离 | User 有 tenant_id，TenantRuntime 隔离 |
| 配额检查 | 滑动窗口，429 超限 |
| MCP 隔离 | 每租户 MCP 配置独立 |
| OAuth2 | 可插拔 Provider |
| 审计日志 | 事件驱动 |
| Admin API | 租户 CRUD + 配额管理 |
