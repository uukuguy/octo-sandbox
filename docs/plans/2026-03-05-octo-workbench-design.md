# octo-workbench 设计方案

> 日期：2026-03-05
> 状态：v1.0 完成，Phase 2.11a 待实施

---

## 一、产品定位

**octo-workbench** 是基于 `octo-engine` 的单用户单 Agent 桌面工作台，面向个人开发者使用。

### 与 octo-platform 的关系

```
octo-types    ← 共享类型（两产品都用）
octo-engine   ← 共享核心引擎（两产品都用，持续完善）
     ↙                    ↘
octo-workbench            octo-platform
（单用户单实例）            （多租户多用户多Agent）
branch: octo-workbench    branch: octo-platform
```

**关键约束**：octo-workbench 是单用户应用，但它依赖的 octo-engine 必须原生支持多租户，以便未来被 octo-platform 复用。

---

## 二、架构总览

### 当前架构

```
octo-workbench
├── crates/
│   ├── octo-types/      ← 共享类型
│   ├── octo-engine/     ← 共享核心引擎
│   │   ├── agent/       ← AgentRuntime (单用户模式)
│   │   ├── memory/      ← Working Memory
│   │   ├── skills/      ← Skill System
│   │   ├── mcp/         ← MCP Client
│   │   └── ...
│   │
│   └── octo-server/     ← REST API 服务器
│       └── api/         ← Agent CRUD, Session 管理
│
└── web/                 ← React 前端
    └── src/
        ├── pages/       ← 主页面
        └── components/ ← UI 组件
```

### AgentRuntime 架构 (当前)

```
AgentRuntime
├── primary_handle: Mutex<Option<AgentExecutorHandle>>  ← 单用户执行器
├── agent_handles: DashMap<AgentId, CancellationToken>  ← 多 Agent 取消
├── catalog: AgentCatalog                                 ← Agent 注册表
├── provider: Arc<dyn Provider>                          ← LLM 提供商
├── tools: Arc<ToolRegistry>                            ← 工具注册表
├── memory: Arc<dyn WorkingMemory>                       ← 工作记忆
└── metering: Arc<Metering>                             ← 计费/监控
```

---

## 三、已完成的功能

### v1.0 冲刺完成 (2026-03-05)

| Phase | 功能 | 状态 |
|-------|------|------|
| Phase 2 | Skills 系统 | ✅ Skill Index, 延迟加载, Agent Skills 标准 |
| Phase 3 | Auth | ✅ API Key 管理, RBAC 4 角色 |
| Phase 4 | Observability | ✅ 结构化日志, Metering |

### 核心组件

| 组件 | 位置 | 说明 |
|------|------|------|
| AgentEntry | `agent/entry.rs` | Agent 定义 (name, role, goal, backstory) |
| AgentCatalog | `agent/catalog.rs` | Agent 注册表 (DashMap 三索引) |
| AgentRuntime | `agent/runtime.rs` | Agent 执行运行时 |
| AgentLoop | `agent/loop_.rs` | Agent 消息循环 |
| ContextInjector | `memory/injector.rs` | Zone B 动态上下文 |

---

## 四、待完成功能

### Phase 2.11a: octo-engine 多租户适配

**目标**：octo-engine 原生支持多租户，为 octo-platform 做好准备

| Task | 内容 | 状态 |
|------|------|------|
| Task 1 | AgentCatalog 添加 TenantId 索引 | ⬜ |
| Task 2 | 新增 TenantContext 类型 | ⬜ |
| Task 3 | AgentRuntime 支持 TenantContext | ⬜ |
| Task 4 | Zone A/B 完整实现 | ⬜ |
| Task 5 | Budget 统一 | ⬜ |
| Task 6 | AppState 集成 + REST API | ⬜ |
| Task 7 | 构建验证 | ⬜ |

**详细计划**：`2026-03-05-phase2-11a-octoe-engine-multi-tenant-implementation.md`

---

## 五、关键设计决策

### 5.1 AgentCatalog 多租户索引

```rust
pub struct AgentCatalog {
    by_id: DashMap<AgentId, AgentEntry>,
    by_name: DashMap<String, AgentId>,           // Key: "{tenant_id}:{name}"
    by_tag: DashMap<String, Vec<AgentId>>,       // Key: "{tenant_id}:{tag}"
    by_tenant_id: DashMap<TenantId, Vec<AgentId>>, // 新增
    store: Option<Arc<AgentStore>>,
}
```

### 5.2 TenantContext

```rust
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

### 5.3 Zone A/B 分离

```
Zone A (System Prompt - 静态)
  优先级: system_prompt > role/goal/backstory > SOUL.md > CORE_INSTRUCTIONS
  不占用 token budget

Zone B (Dynamic Context - 每轮注入)
  <context>
    <datetime>...</datetime>
    <user_profile>...</user_profile>
    <task_context>...</task_context>
    <memory>...</memory>
  </context>
  占用 token budget
```

---

## 六、前端架构

### 页面结构

```
web/src/pages/
├── Home.tsx           ← 首页，Agent 列表
├── Chat.tsx           ← 对话页面
├── Settings.tsx       ← 设置页面
└── ...
```

### 技术栈

- React 18 + TypeScript
- Jotai (状态管理)
- TailwindCSS
- Vite

---

## 七、实施路线图

```
当前状态: v1.0 完成
         │
         ▼
Phase 2.11a: octo-engine 多租户适配
  ├── Task 1-3: 多租户基础 (TenantId, TenantContext, AgentRuntime)
  ├── Task 4-5: 上下文工程 (Zone A/B, Budget)
  ├── Task 6-7: API 集成 + 验证
  │
  ▼
octo-workbench v1.1 (可选)
  ├── 前端优化
  └── 体验改进
  │
  ▼
octo-platform (新分支)
  └── Phase 2.11b: octo-platform-server
```

---

## 八、验收标准

### Phase 2.11a 验收

| 检查项 | 验收标准 |
|--------|---------|
| 编译 | `cargo check --workspace` 0 errors |
| TenantId 支持 | AgentCatalog 有 by_tenant_id 索引 |
| TenantContext | AgentRuntime 可接收 TenantContext |
| Zone A/B | SystemPromptBuilder + ContextInjector 分离 |
| Budget | TokenBudget 与 ContextInjector 对齐 |
| REST API | Agent CRUD 端点可用 |
| 功能测试 | octo-workbench 单用户模式正常工作 |
