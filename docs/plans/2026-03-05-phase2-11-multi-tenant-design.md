# Phase 2.11: AgentRegistry + 多租户适配重构

> **状态**: 待更新设计 (2026-03-05)
> **基于**: octo-platform 设计方案 (2026-03-04)

## 背景更新

**重要架构决策** (2026-03-05)：

octo-workbench 是单用户应用，但它依赖的 **octo-engine 是为多租户 octo-platform 设计的**。因此 Phase 2.11 的设计需要调整：

```
octo-platform (多用户平台)
    │
    └── octo-engine (共享核心，支撑两产品)
            ├── AgentRegistry (需支持多租户)
            ├── 租户隔离 (TenantId)
            │
            ▼
octo-workbench (单用户前端)
    │
    └── 使用 octo-engine 的单租户视图
```

---

## 调整后的架构设计

### 核心变化

| 原设计 | 调整后 | 原因 |
|--------|--------|------|
| AgentRegistry (全局) | AgentRegistry + TenantId 索引 | 多租户隔离 |
| AgentRuntime (全局单例) | AgentRuntime per-user | octo-platform 需要 |
| 无租户上下文 | TenantContext 注入 | 资源隔离 |

### 调整后的文件结构

```
crates/octo-engine/src/agent/
├── entry.rs              ✅ 已存在 (AgentEntry, AgentManifest, AgentStatus)
├── catalog.rs           ⚠️ 需修改 (添加 TenantId 索引)
├── runtime.rs           ✅ 已存在 (AgentRuntime)
│
├── registry/            ❌ 整合到 catalog.rs
│   ├── mod.rs
│   ├── entry.rs
│   └── store.rs
│
├── runner.rs            ❌ 整合到 runtime.rs
└── tenant.rs            🆕 新增 (TenantContext)
```

### AgentCatalog 多租户索引

```rust
// crates/octo-engine/src/agent/catalog.rs 调整后

use octo_types::TenantId;

pub struct AgentCatalog {
    by_id: DashMap<AgentId, AgentEntry>,
    by_name: DashMap<String, AgentId>,           // Key: "{tenant_id}:{name}"
    by_tag: DashMap<String, Vec<AgentId>>,       // Key: "{tenant_id}:{tag}"
    by_tenant_id: DashMap<TenantId, Vec<AgentId>>, // 新增
    store: Option<Arc<AgentStore>>,
}
```

### TenantContext 注入

```rust
// crates/octo-engine/src/agent/tenant.rs (新增)

use octo_types::{TenantId, UserId};

pub struct TenantContext {
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub roles: Vec<Role>,
}

impl TenantContext {
    /// 单用户场景 (octo-workbench)
    pub fn for_single_user(tenant_id: TenantId, user_id: UserId) -> Self {
        Self {
            tenant_id,
            user_id,
            roles: vec![Role::Owner],
        }
    }
}
```

---

## Task 拆分

### Phase 2.11a: octo-engine 多租户适配 (本计划)

**目标**: octo-engine 原生支持多租户，为 octo-platform 做好准备

| Task | 内容 |
|------|------|
| Task 1 | AgentCatalog 添加 TenantId 索引 |
| Task 2 | 新增 TenantContext 类型 |
| Task 3 | AgentRuntime 支持 TenantContext |
| Task 4 | Zone A/B 完整实现 |
| Task 5 | Budget 统一 |
| Task 6 | AppState 集成 + REST API |
| Task 7 | 构建验证 |

### Phase 2.11b: octo-platform-server (后续计划)

**目标**: 实现租户管理层

| Task | 内容 |
|------|------|
| Task 1 | TenantRuntime 实现 |
| Task 2 | UserRuntime 实现 |
| Task 3 | JWT/OAuth2 认证集成 |
| Task 4 | 资源配额管理 |

---

## 依赖关系

```
Phase 2.11a (octo-engine)
    │
    ├── Task 1-3: 多租户基础 (AgentCatalog + TenantContext + AgentRuntime)
    ├── Task 4-5: 上下文工程 (Zone A/B + Budget)
    └── Task 6-7: API 集成 + 验证
            │
            ▼
Phase 2.11b (octo-platform-server)
    │
    ├── Task 1-2: 租户/用户管理层
    ├── Task 3: 认证集成
    └── Task 4: 配额管理
```

---

## 验收标准 (Phase 2.11a)

| 检查项 | 验收标准 |
|--------|---------|
| 编译 | `cargo check --workspace` 0 errors |
| TenantId 支持 | AgentCatalog 有 by_tenant_id 索引 |
| TenantContext | AgentRuntime 可接收 TenantContext |
| AgentManifest | role/goal/backstory/system_prompt/tool_filter 字段完整 |
| Zone A/B | system prompt 静态，working memory 注入首条 Human Message |
| REST API | Agent CRUD 端点可用 |
| octo-workbench | 单用户模式正常工作 |
