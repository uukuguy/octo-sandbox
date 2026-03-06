# Phase 2.11a: octo-engine 多租户适配实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标**: octo-engine 原生支持多租户，为 octo-platform 做好准备

**前置条件**: Phase 2.10 (Knowledge Graph) 完成

---

## 实施任务总览

| Task | 任务 | 估算 | 状态 |
|------|------|------|------|
| Task 1 | AgentCatalog 添加 TenantId 索引 | 100 LOC | ⬜ |
| Task 2 | 新增 TenantContext 类型 | 80 LOC | ⬜ |
| Task 3 | AgentRuntime 支持 TenantContext | 150 LOC | ⬜ |
| Task 4 | Zone A/B 完整实现 | 200 LOC | ⬜ |
| Task 5 | Budget 统一 | 100 LOC | ⬜ |
| Task 6 | AppState 集成 + REST API | 150 LOC | ⬜ |
| Task 7 | 构建验证 | - | ⬜ |

---

## Task 1: AgentCatalog 添加 TenantId 索引

**目标**: AgentCatalog 原生支持多租户隔离

### Step 1: 修改 `catalog.rs`

```rust
// crates/octo-engine/src/agent/catalog.rs

use octo_types::TenantId;

// 添加 TenantId 到 AgentEntry 存储
pub struct AgentCatalog {
    by_id: DashMap<AgentId, AgentEntry>,
    by_name: DashMap<String, AgentId>,           // Key: "{tenant_id}:{name}"
    by_tag: DashMap<String, Vec<AgentId>>,       // Key: "{tenant_id}:{tag}"
    by_tenant_id: DashMap<TenantId, Vec<AgentId>>, // 新增
    store: Option<Arc<AgentStore>>,
}

impl AgentCatalog {
    /// 按租户注册 Agent
    pub fn register(&self, tenant_id: TenantId, manifest: AgentManifest) -> AgentId {
        let entry = AgentEntry::new(tenant_id, manifest);
        // ... 更新所有索引
    }

    /// 获取租户下所有 Agent
    pub fn get_by_tenant(&self, tenant_id: &TenantId) -> Vec<AgentEntry> {
        self.by_tenant_id
            .get(tenant_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.by_id.get(id).map(|e| e.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
}
```

### Step 2: 修改 `entry.rs` - 添加 TenantId

```rust
// crates/octo-engine/src/agent/entry.rs

use octo_types::TenantId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    pub id: AgentId,
    pub tenant_id: TenantId,  // 新增
    pub manifest: AgentManifest,
    pub state: AgentStatus,
    pub created_at: i64,
}

impl AgentEntry {
    pub fn new(tenant_id: TenantId, manifest: AgentManifest) -> Self {
        Self {
            id: AgentId::new(),
            tenant_id,
            manifest,
            state: AgentStatus::Created,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        }
    }
}
```

### Step 3: 编译验证

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 4: Commit

```bash
git add crates/octo-engine/src/agent/catalog.rs
git add crates/octo-engine/src/agent/entry.rs
git commit -m "feat(agent): add TenantId to AgentCatalog and AgentEntry"
```

---

## Task 2: 新增 TenantContext 类型

**目标**: 定义租户上下文，用于资源隔离

### Step 1: 创建 `tenant.rs`

```rust
// crates/octo-engine/src/agent/tenant.rs

use octo_types::{TenantId, UserId};

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Viewer,
    User,
    Admin,
    Owner,
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

    /// 验证用户有权限执行操作
    pub fn can(&self, action: Action) -> bool {
        match (self.roles.as_slice(), action) {
            ([Role::Owner | Role::Admin], _) => true,
            ([Role::User], Action::RunAgent | Action::CreateSession) => true,
            ([Role::Viewer], Action::Read) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Read,
    CreateSession,
    RunAgent,
    ManageAgent,
    ManageMcp,
}
```

### Step 2: 更新 `mod.rs` 导出

```rust
// crates/octo-engine/src/agent/mod.rs

pub mod tenant;  // 新增

pub use tenant::{TenantContext, Role, Action};
```

### Step 3: Commit

```bash
git add crates/octo-engine/src/agent/tenant.rs
git add crates/octo-engine/src/agent/mod.rs
git commit -m "feat(agent): add TenantContext for multi-tenant isolation"
```

---

## Task 3: AgentRuntime 支持 TenantContext

**目标**: AgentRuntime 可按租户/用户隔离资源

### Step 1: 修改 `runtime.rs` - 添加 TenantContext 字段

```rust
// crates/octo-engine/src/agent/runtime.rs

pub struct AgentRuntime {
    // ... 现有字段
    tenant_context: Option<TenantContext>,  // 新增
}

impl AgentRuntime {
    /// 为指定租户创建 AgentRuntime
    pub async fn new(
        catalog: Arc<AgentCatalog>,
        config: AgentRuntimeConfig,
        tenant_context: Option<TenantContext>,  // 新增参数
    ) -> Result<Self, AgentError> {
        // ... 现有初始化逻辑
        Ok(Self {
            // ... 现有字段
            tenant_context,
        })
    }

    /// 验证当前请求的租户权限
    pub fn verify_tenant_access(&self, tenant_id: &TenantId) -> Result<()> {
        if let Some(ctx) = &self.tenant_context {
            if &ctx.tenant_id != tenant_id {
                return Err(AgentError::PermissionDenied(
                    "Tenant mismatch".into()
                ));
            }
        }
        Ok(())
    }
}
```

### Step 2: 更新 octo-server 调用处

```rust
// crates/octo-server/src/state.rs 或 server.rs

// 单用户场景 (octo-workbench)
let tenant_context = TenantContext::for_single_user(
    TenantId::from_string("default"),
    UserId::from_string("local-user"),
);

let runtime = AgentRuntime::new(catalog, config, Some(tenant_context)).await?;
```

### Step 3: 编译验证

```bash
cargo check -p octo-engine -p octo-server 2>&1 | grep "^error" | head -20
```

### Step 4: Commit

```bash
git add crates/octo-engine/src/agent/runtime.rs
git add crates/octo-server/src/
git commit -m "feat(agent): add TenantContext support to AgentRuntime"
```

---

## Task 4: Zone A/B 完整实现

**目标**: System Prompt (Zone A) 和 Dynamic Context (Zone B) 完全分离

### Step 1: 创建 SystemPromptBuilder

```rust
// crates/octo-engine/src/context/system_prompt.rs (新增)

use crate::agent::AgentManifest;

pub struct SystemPromptBuilder {
    manifest: AgentManifest,
    core_instructions: Option<String>,
    bootstrap_files: Vec<BootstrapFile>,
}

pub struct BootstrapFile {
    pub name: String,
    pub content: String,
}

impl SystemPromptBuilder {
    /// 构建 Zone A - System Prompt
    ///
    /// 优先级: system_prompt > role/goal/backstory > SOUL.md > CORE_INSTRUCTIONS
    pub fn build(&self) -> String {
        let mut parts = Vec::new();

        // 1. 最高优先级: 直接覆盖
        if let Some(sp) = &self.manifest.system_prompt {
            parts.push(sp.clone());
            return parts.join("\n\n");
        }

        // 2. role/goal/backstory (CrewAI 模式)
        if let Some(role) = &self.manifest.role {
            parts.push(format!("## Role\n{}", role));
        }
        if let Some(goal) = &self.manifest.goal {
            parts.push(format!("## Goal\n{}", goal));
        }
        if let Some(backstory) = &self.manifest.backstory {
            parts.push(format!("## Backstory\n{}", backstory));
        }

        // 3. Bootstrap 文件
        for file in &self.bootstrap_files {
            parts.push(format!("## {}\n{}", file.name, file.content));
        }

        // 4. Core Instructions (最低优先级)
        if let Some(instructions) = &self.core_instructions {
            parts.push(instructions.clone());
        }

        parts.join("\n\n")
    }
}
```

### Step 2: 修改 ContextInjector - Zone B

```rust
// crates/octo-engine/src/memory/injector.rs (已存在，检查完整性)

impl ContextInjector {
    /// 构建 Zone B - Dynamic Context (注入首条 Human Message)
    ///
    /// 输出格式:
    /// <context>
    ///   <datetime>2026-03-05T10:30:00Z</datetime>
    ///   <user_profile>...</user_profile>
    ///   <task_context>...</task_context>
    ///   <memory>...</memory>
    /// </context>
    pub fn build_zone_b(&self, memory: &dyn WorkingMemory) -> String {
        // 已有实现，检查是否输出 <context> 标签
    }
}
```

### Step 3: 修改 AgentLoop - Zone A/B 分离

```rust
// crates/octo-engine/src/agent/loop_.rs

impl AgentLoop {
    pub async fn run(&mut self, ctx: &mut AgentContext) -> Result<AgentResponse, AgentError> {
        // Zone A: 构建 System Prompt (静态，不占用 token budget)
        let system_prompt = SystemPromptBuilder::new(&self.manifest)
            .with_core_instructions(CORE_INSTRUCTIONS)
            .build();

        // Zone B: 动态上下文 (注入首条 Human Message)
        let context_block = self.injector.build_zone_b(self.memory.as_ref());

        // 组合消息
        let mut messages = vec![
            Message::system(system_prompt),
            Message::user(context_block),
        ];

        // ... 现有 LLM 调用逻辑
    }
}
```

### Step 4: Commit

```bash
git add crates/octo-engine/src/context/
git add crates/octo-engine/src/memory/injector.rs
git add crates/octo-engine/src/agent/loop_.rs
git commit -m "feat(context): implement Zone A/B separation in AgentLoop"
```

---

## Task 5: Budget 统一

**目标**: TokenBudget 与 ContextInjector 对齐

### Step 1: 检查现有实现

```rust
// crates/octo-engine/src/context/budget.rs

pub struct TokenBudget {
    pub system_prompt: usize,  // Zone A
    pub context: usize,        // Zone B
    pub conversation: usize,   // Zone C
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            system_prompt: 16_000,  // Zone A: 16K
            context: 12_000,        // Zone B: 12K
            conversation: 32_000,  // Zone C: 32K
        }
    }
}
```

### Step 2: 对齐 ContextInjector budget

```rust
// crates/octo-engine/src/memory/injector.rs

impl ContextInjector {
    const DEFAULT_BUDGET: usize = 12_000;  // 与 TokenBudget.context 对齐
}
```

### Step 3: Commit

```bash
git add crates/octo-engine/src/context/budget.rs
git add crates/octo-engine/src/memory/injector.rs
git commit -m "fix(context): align TokenBudget with ContextInjector"
```

---

## Task 6: AppState 集成 + REST API

**目标**: octo-server 正确集成 AgentRuntime with TenantContext

### Step 1: 更新 octo-server 的 AgentRuntime 创建

```rust
// crates/octo-server/src/state.rs

impl AppState {
    pub async fn new(config: Config) -> Result<Self, anyhow::Error> {
        // 从配置获取租户信息 (单用户场景用默认值)
        let tenant_context = TenantContext::for_single_user(
            TenantId::from_string("default"),
            UserId::from_string("local-user"),
        );

        let runtime = AgentRuntime::new(
            catalog,
            config.into(),
            Some(tenant_context),  // 传入租户上下文
        ).await?;

        Ok(Self { runtime, ... })
    }
}
```

### Step 2: 验证 REST API

```bash
# 启动服务器
cargo run -p octo-server &

# 测试 Agent 端点
curl http://127.0.0.1:3001/api/v1/agents
curl -X POST http://127.0.0.1:3001/api/v1/agents \
  -H "Content-Type: application/json" \
  -d '{"name": "test-agent", "role": "assistant"}'
```

### Step 3: Commit

```bash
git add crates/octo-server/src/state.rs
git commit -m "fix(server): integrate TenantContext into AppState"
```

---

## Task 7: 构建验证

### Step 1: 完整编译检查

```bash
cargo check --workspace 2>&1 | tail -5
```

### Step 2: 运行测试

```bash
cargo test -p octo-engine 2>&1 | tail -20
```

### Step 3: TypeScript 检查

```bash
cd web && npx tsc --noEmit 2>&1 | tail -10 && cd ..
```

### Step 4: 功能验证

```bash
# 启动服务器
cargo run -p octo-server &

# 验证 Agent 创建
curl -X POST http://127.0.0.1:3001/api/v1/agents \
  -H "Content-Type: application/json" \
  -d '{"name": "test", "role": "assistant"}' | jq .

# 验证 Agent 列表
curl http://127.0.0.1:3001/api/v1/agents | jq .
```

### Step 5: Commit

```bash
git add .
git commit -m "fix: Phase 2.11a complete - multi-tenant support in octo-engine"
```

---

## 完成标准

| 检查项 | 验收标准 |
|--------|---------|
| 编译 | `cargo check --workspace` 0 errors |
| TenantId 支持 | AgentCatalog 有 by_tenant_id 索引 |
| TenantContext | AgentRuntime 可接收 TenantContext |
| Zone A/B | SystemPromptBuilder + ContextInjector 分离 |
| Budget | TokenBudget 与 ContextInjector 对齐 |
| REST API | Agent CRUD 端点可用 |
| 功能测试 | octo-workbench 单用户模式正常工作 |

---

## 预估工作量

| Task | LOC | 复杂度 |
|------|-----|--------|
| Task 1 | 100 | 低 |
| Task 2 | 80 | 低 |
| Task 3 | 150 | 中 |
| Task 4 | 200 | 中 |
| Task 5 | 100 | 低 |
| Task 6 | 150 | 中 |
| Task 7 | - | 低 |
| **Total** | **~780 LOC** | |
