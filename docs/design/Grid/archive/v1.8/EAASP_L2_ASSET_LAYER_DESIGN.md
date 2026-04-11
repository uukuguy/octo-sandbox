# EAASP L2 统一资产层设计文档

> **版本**: v1.0
> **创建日期**: 2026-04-07
> **Phase**: BF — L2 统一资产层 + L1 抽象机制
> **基线**: Phase BE 完成 @ d2ae822（EAASP 协议层 + claude-code-runtime）
> **权威参考**: `EAASP_-_企业自主智能体支撑平台设计规范_v1.7_.pdf` §5

---

## 一、概述

### 1.1 目标

L2 统一资产层负责管理企业智能体可复用的资产：Skill（技能）、MCP Server（外部工具连接器）、Ontology（本体/术语表）。本阶段（BF）实现前两者，Ontology Service 留后。

### 1.2 架构定位

```
┌─────────────────────────────────────────────────┐
│ L4 人机协作层（管理控制台、员工门户）               │
├─────────────────────────────────────────────────┤
│ L3 治理层（策略引擎、审批闸门、审计服务）            │
│   RuntimeSelector：选择运行时，下发 skill_ids     │
├─────────────────────────────────────────────────┤
│ L2 统一资产层 ← 本文档                            │
│   ┌──────────────┐  ┌──────────────────┐        │
│   │ Skill Registry│  │ MCP Orchestrator │        │
│   │ (REST API)   │  │ (REST API)       │        │
│   └──────┬───────┘  └───────┬──────────┘        │
├──────────┼──────────────────┼───────────────────┤
│ L1 运行时层                                      │
│   initialize() 时从 L2 拉取 Skill 内容             │
│   connect_mcp() 时直连 MCP Server                 │
└─────────────────────────────────────────────────┘
```

### 1.3 设计原则

1. **物理分离**：Skill Registry 和 MCP Orchestrator 是独立进程，独立部署
2. **REST Only**：L2 对外仅暴露 REST API，不直接提供 MCP 接口（Agent 不直连 L2）
3. **L3 中介**：L3 从 L2 查询资产清单，筛选后通过 SessionPayload 下发给 L1
4. **L1 拉取**：L1 收到 skill_ids + skill_registry_url 后，自行从 L2 REST 拉取内容

---

## 二、数据流全景

```
用户请求
  └─► L4 会话管理器
        └─► L3 RuntimeSelector
              ├─► 查询 L2 Skill Registry（可用 skills）
              ├─► 查询 L2 MCP Orchestrator（可用 MCP servers）
              ├─► 策略筛选：组织范围 + 用户角色 + 任务类型
              └─► 生成 SessionPayload:
                    skill_ids: ["order-mgmt", "logistics"]
                    skill_registry_url: "http://l2-skill:8081"
                    mcp_servers: [{name: "erp-mcp", endpoint: "..."}]
                        │
                        ▼ gRPC Initialize
              L1 Runtime (GridHarness / claude-code-runtime)
                ├─► 遍历 skill_ids
                │     └─► GET /skills/{id}/content → SKILL.md
                │     └─► load_skill() 加载到 Agent 技能系统
                ├─► 遍历 mcp_servers
                │     └─► connect_mcp() 直连 MCP Server
                └─► Agent 使用 skills + MCP tools 自主执行
```

---

## 三、L2 Skill Registry

### 3.1 存储架构

三层存储，各司其职：

| 层 | 技术 | 职责 |
|---|------|------|
| 元数据 | SQLite | id, version, status, tags, author, timestamps |
| 内容 | 文件系统 | `skills/{id}/{version}/SKILL.md` |
| 版本追溯 | Git (git2) | 提交历史，版本回溯（BF-D1 完善） |

### 3.2 数据模型

```rust
// 技能元数据
struct SkillMeta {
    id: String,              // e.g. "org/order-management"
    name: String,
    description: String,
    version: String,         // semver
    status: SkillStatus,     // Draft → Tested → Reviewed → Production
    author: Option<String>,
    tags: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

// 技能完整内容
struct SkillContent {
    meta: SkillMeta,
    frontmatter_yaml: String,  // YAML frontmatter
    prose: String,             // Markdown 正文
}

// 晋升状态
enum SkillStatus {
    Draft,       // 草稿
    Tested,      // 已测试
    Reviewed,    // 已审核
    Production,  // 生产可用
}
```

### 3.3 REST API

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/skills/{id}/content?version=x` | 读取技能完整内容（元数据 + SKILL.md） |
| `GET` | `/skills/{id}/versions` | 列出技能所有版本 |
| `GET` | `/skills/search?tags=erp&q=order&limit=20` | 搜索技能（按标签/关键词） |
| `POST` | `/skills/draft` | 提交草稿（创建/更新） |
| `POST` | `/skills/{id}/promote/{version}` | 晋升状态（需 target_status） |
| `GET` | `/health` | 健康检查 |

### 3.4 晋升流水线

```
Draft ──► Tested ──► Reviewed ──► Production
  │         │           │
  └─────────┴───────────┴── 任何状态可提交新版本（回到 Draft）
```

晋升规则：
- Draft → Tested：开发者自行标记
- Tested → Reviewed：需审核者确认（BF-D6 RBAC 后强制）
- Reviewed → Production：需管理员批准（BF-D6 RBAC 后强制）

### 3.5 启动方式

```bash
# 开发模式
make skill-registry-start
# 等价于:
cargo run -p eaasp-skill-registry -- --data-dir ./data/skill-registry --port 8081
```

---

## 四、L2 MCP Orchestrator

### 4.1 设计目标

管理企业 MCP Server 的生命周期（启动/停止/健康检查），为 L3 提供可用 MCP Server 清单。

### 4.2 运行模式

| 模式 | 说明 | BF 状态 |
|------|------|---------|
| **Shared** | 全局共享子进程，多会话复用 | ✅ 已实现 |
| PerSession | 每会话独立容器（Docker） | BF-D2 Deferred |
| OnDemand | 按需启动，空闲超时关闭 | BF-D3 Deferred |

### 4.3 配置格式 (YAML)

```yaml
# config/mcp-servers.yaml
servers:
  - name: erp-mcp
    command: /opt/mcp/erp-server
    args: ["--port", "8090"]
    transport: streamable-http
    port: 8090
    mode: shared
    tags: [erp, finance]
    env:
      DB_URL: postgres://...
    health_endpoint: /health

  - name: crm-mcp
    command: /opt/mcp/crm-server
    args: []
    transport: stdio
    port: 0
    mode: shared
    tags: [crm, customer]
    env: {}
    health_endpoint: ""
```

### 4.4 REST API

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/mcp-servers` | 列出所有 MCP Server 及运行状态 |
| `GET` | `/mcp-servers/{name}/info` | 单个 Server 详情 |
| `POST` | `/mcp-servers/{name}/start` | 启动 Shared 子进程 |
| `POST` | `/mcp-servers/{name}/stop` | 停止子进程 |
| `GET` | `/mcp-servers/filter?tags=erp` | 按标签过滤 |
| `GET` | `/health` | 健康检查 |

### 4.5 子进程管理

```rust
struct McpManager {
    servers: HashMap<String, McpServerDef>,      // 配置定义
    processes: HashMap<String, tokio::process::Child>,  // 运行中子进程
}
```

- `start(name)`: 启动子进程，记录 PID
- `stop(name)`: 发送 SIGTERM，等待退出
- `list_servers()`: 返回所有 server 状态（包含 running 标志）
- `list_by_tags(tags)`: 过滤返回匹配标签的 server

---

## 五、协议扩展（Proto v1.3）

### 5.1 SessionPayload 新增字段

```protobuf
message SessionPayload {
    // ... existing fields 1-8 ...
    repeated string skill_ids = 9;           // L3 筛选后的 skill ID 列表
    string skill_registry_url = 10;          // L2 Skill Registry REST 端点
    bool allowed_skill_search = 11;          // Agent 是否可搜索更多 skills
    repeated string skill_search_scope = 12; // 搜索范围限制（e.g. "org/erp/*"）
}
```

### 5.2 字段语义

| 字段 | 来源 | 消费者 | 说明 |
|------|------|--------|------|
| `skill_ids` | L3 RuntimeSelector | L1 Runtime | 需预加载的 skill ID 列表 |
| `skill_registry_url` | L3 配置 | L1 Runtime | L2 REST 端点 URL |
| `allowed_skill_search` | L3 策略 | L1 Runtime | 是否允许按需搜索（BF 阶段固定 false） |
| `skill_search_scope` | L3 策略 | L1 Runtime | 搜索范围模式（BF 阶段不使用） |

---

## 六、L1-L2 集成

### 6.1 L2SkillClient

```rust
// crates/grid-runtime/src/l2_client.rs
pub struct L2SkillClient {
    base_url: String,
    http: reqwest::Client,
}

impl L2SkillClient {
    pub async fn fetch_skill(&self, skill_id: &str) -> Result<L2SkillContent>;
    pub async fn fetch_skills(&self, skill_ids: &[String]) -> Vec<(String, Result<L2SkillContent>)>;
}
```

### 6.2 GridHarness initialize 流程

```
initialize(SessionPayload) {
    1. 基础初始化（user_id, quotas, context...）
    2. if skill_ids.is_not_empty() && skill_registry_url.is_some():
         client = L2SkillClient::new(skill_registry_url)
         for id in skill_ids:
             content = client.fetch_skill(id)  // REST GET
             self.load_skill(handle, content)   // 加载到 Agent
    3. 返回 session_id
}
```

### 6.3 Skill 格式转换

L2 交付标准格式 `SKILL.md`（YAML frontmatter + Markdown prose），L1 Runtime 内部负责转换：
- **Grid Runtime**: SKILL.md → `SkillDefinition`（直接加载）
- **Claude Code Runtime**: SKILL.md → `.claude/skills/*.md`（写入临时目录）

转换是 L1 内部的事（BF-KD12），L2 不感知 L1 实现细节。

---

## 七、Mock L3 RuntimeSelector

### 7.1 RuntimePool

运行时池管理所有可用的 L1 Runtime 实例：

```rust
struct RuntimePool {
    entries: Vec<RuntimeEntry>,  // id, name, endpoint, tier, healthy
}

impl RuntimePool {
    fn register(&self, entry: RuntimeEntry);
    fn list(&self) -> Vec<RuntimeEntry>;
    fn healthy(&self) -> Vec<RuntimeEntry>;
    fn get(&self, id: &str) -> Option<RuntimeEntry>;
}
```

### 7.2 SelectionStrategy

```rust
enum SelectionStrategy {
    UserPreference(String),  // 用户指定 runtime ID
    Blindbox,                // 随机选 2 个对比
    Default,                 // 最优（当前=第一个健康的）
}
```

---

## 八、盲盒对比

### 8.1 设计理念

盲盒对比（Blindbox Comparison）让用户在不知道哪个 runtime 产出结果的情况下评判质量，消除品牌偏见。

### 8.2 流程

```
1. 用户选择盲盒模式 + 输入 prompt
2. RuntimeSelector 选择 2 个健康的 runtime
3. 随机分配匿名标签 A / B
4. 并行执行：tokio::join!(execute(A), execute(B))
5. 匿名展示两个结果（只显示 A/B，隐藏 runtime 名称）
6. 用户投票：A Wins / B Wins / Tie
7. 揭示：A = grid-harness, B = claude-code-runtime
```

### 8.3 数据结构

```rust
struct BlindboxRecord {
    prompt: String,
    result_a: BlindboxResult,  // label, response_text, duration_ms, runtime_id(hidden)
    result_b: BlindboxResult,
    vote: Option<BlindboxVote>,  // AWins / BWins / Tie
    revealed: bool,
}
```

### 8.4 使用方式

```bash
make certifier-blindbox PROMPT="请用中文解释什么是 EAASP"

# 或直接:
cargo run -p eaasp-certifier -- blindbox \
    --runtime-a http://localhost:50051 \
    --runtime-b http://localhost:50052 \
    --prompt "Hello"
```

---

## 九、设计决策记录

| # | 决策 | 理由 |
|---|------|------|
| BF-KD1 | L2 存储：SQLite 元数据 + 文件系统内容 + Git 版本追溯 | 与 grid-engine 统一技术栈，三层各司其职 |
| BF-KD2 | L1 ↔ L2 Skill 通信：REST（L1 拉取内容） | Agent 不直连 L2，L3 下发 skill_ids，L1 从 L2 REST 拉取 |
| BF-KD3 | L2 实现语言：Rust（独立 binary） | 复用 grid-types，rmcp 已验证，单 binary 部署 |
| BF-KD4 | RuntimeSelector 属于 L3，BF 在 certifier mock | L3 未来用 Python/TS 实现，certifier mock 是验证基线 |
| BF-KD5 | 盲盒模式：用户主动开启，并行执行，匿名评分 | 实验性功能，成本翻倍，用户自愿触发 |
| BF-KD6 | L2 三个独立服务：Skill Registry / MCP Orchestrator / Ontology | 三种资产本质不同，物理分离 |
| BF-KD7 | MCP Orchestrator 对 L1 不直连 | L3 从 Orchestrator 获取连接信息，筛选后下发给 L1 |
| BF-KD8 | Agent 不需要 skill_search | L3 下发可用 skill 列表，L1 从 L2 拉取内容，Agent 内部使用 |
| BF-KD9 | L2 Skill Registry = REST only | Agent 不直连 L2，纯 REST 足够 |
| BF-KD10 | MCP Server 运行模式：Shared/PerSession/OnDemand | BF 只实现 Shared（子进程），PerSession 留 BH |
| BF-KD11 | Skill 获取 = 预加载 + 按需发现 | BF 只实现预加载，按需发现留 L3 治理后 |
| BF-KD12 | L1 Skill 转换是 Runtime 内部的事 | Grid → SkillDefinition；CC → .claude/skills/；L2 只交付 SKILL.md |

---

## 十、Deferred Items

| ID | 内容 | 前置条件 | 目标阶段 |
|----|------|---------|---------|
| BF-D1 | Git 版本追溯集成到 submit_draft/promote | git2 基础就绪 | BG |
| BF-D2 | MCP Orchestrator PerSession 模式 | Docker API | BH |
| BF-D3 | MCP Orchestrator OnDemand 模式 | 连接计数 + idle 超时 | BH |
| BF-D4 | Agent 按需 skill 发现 (allowed_skill_search) | L3 治理层 | BH |
| BF-D5 | Ontology Service | BH+ | BH+ |
| BF-D6 | Skill Registry RBAC 访问控制 | L3 认证体系 | BH |
| BF-D7 | 盲盒 ELO/win-rate 统计聚合 | 足够评分数据 | BH |
| BF-D8 | RuntimeSelector 成本排序 | CostEstimate 数据收集 | BH |
| BF-D9 | L2 MCP Orchestrator 容器化管理 | Docker API | BH |
| BF-D10 | Skill Registry streamable-http 生产模式 | 部署架构确认 | BG |

---

## 十一、目录结构

```
tools/
├── eaasp-skill-registry/           # L2 Skill Registry
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs                 # CLI + Axum server
│   │   ├── lib.rs
│   │   ├── models.rs               # SkillMeta, SkillContent, SkillStatus
│   │   ├── store.rs                # SQLite + 文件系统存储
│   │   ├── routes.rs               # REST API 路由
│   │   └── git_backend.rs          # Git 版本追溯
│   └── tests/
│       ├── store_test.rs           # 存储层测试 (3 tests)
│       └── api_test.rs             # API 集成测试 (4 tests)
│
├── eaasp-mcp-orchestrator/         # L2 MCP Orchestrator
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs                 # CLI + Axum server
│   │   ├── lib.rs
│   │   ├── config.rs               # McpServerDef, RunMode
│   │   ├── manager.rs              # 子进程生命周期管理
│   │   └── routes.rs               # REST API 路由
│   └── tests/
│       └── manager_test.rs         # 管理器测试 (4 tests)
│
├── eaasp-certifier/src/
│   ├── runtime_pool.rs             # 运行时池 (2 tests)
│   ├── selector.rs                 # Mock L3 选择策略 (3 tests)
│   └── blindbox.rs                 # 盲盒对比 (2 tests)

crates/
├── grid-runtime/src/
│   └── l2_client.rs                # L2 REST 客户端 (4 tests)
```

---

## 十二、验收标准

- [x] `cargo test -p eaasp-skill-registry -- --test-threads=1` 通过
- [x] `cargo test -p eaasp-mcp-orchestrator -- --test-threads=1` 通过
- [x] `cargo test -p eaasp-certifier -- --test-threads=1` 通过
- [x] `cargo test -p grid-runtime -- --test-threads=1` 通过
- [x] SessionPayload proto v1.3 新增 4 个 L2 字段
- [x] Makefile 新增 skill-registry / mcp-orch / blindbox targets
- [x] 设计文档完成
