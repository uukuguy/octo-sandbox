# octo-sandbox 下一会话指南

**最后更新**: 2026-03-04 GMT+8
**当前分支**: `octo-platform`
**当前状态**: 🔄 octo-platform Phase 1 - Auth + 单租户多用户（设计完成，待实施）

---

## 产品背景

octo-platform 是基于 `octo-engine` 的企业级多租户多 Agent 平台，与 octo-workbench 独立演进，共享核心引擎。

```
octo-types    ← 共享类型
octo-engine   ← 共享核心引擎
     ↙                    ↘
octo-workbench            octo-platform
（单用户单实例）            （多租户多用户多Agent）
branch: octo-workbench    branch: octo-platform
```

---

## 阶段进度

| 阶段 | 状态 | 说明 |
|------|------|------|
| 设计文档 | ✅ 完成 | `docs/plans/2026-03-04-octo-platform-design.md` |
| P1: Auth + 单租户多用户 | 🔄 启动中 | ~4周，见下方任务清单 |
| P2: 多租户 + 配额 + MCP 隔离 | ⏳ 待开始 | ~4周 |
| P3: Multi-Agent 编排 | ⏳ 待开始 | ~6周 |
| P4: 生产就绪 | ⏳ 待开始 | ~4周 |

---

## Platform P1 任务清单

**目标**：`octo-platform-server` 可运行，多用户登录，每用户独立 AgentRuntime。

| Task | 内容 | 状态 |
|------|------|------|
| P1-1 | 新建 `octo-platform-server` crate，基础 Axum 服务 | ⏳ |
| P1-2 | 自建用户系统：注册/登录/JWT | ⏳ |
| P1-3 | `PlatformState`：单租户 + `DashMap<UserId, Arc<AgentRuntime>>` | ⏳ |
| P1-4 | 每用户独立 WebSocket + AgentRuntime 懒加载 | ⏳ |
| P1-5 | Admin API：用户 CRUD、角色管理 | ⏳ |
| P1-6 | `web-platform/` 初始化：登录页 + 用户工作空间 | ⏳ |

**验收**：3 用户同时登录，各自独立对话，互不干扰。

---

## 关键设计决策

1. **两级隔离**：租户（独立 DB/MCP/配额） + 用户（独立 AgentRuntime）
2. **三种部署模式**：SaaS / 企业私有部署 / 开发者 API 平台
3. **认证**：自建 JWT（开发快）+ 插拔式 OIDC（企业部署）
4. **编排**：Supervisor / Peer-to-Peer / Pipeline 三种模式（P3 实现）
5. **前端策略**：独立 `web-platform/`，共享 `design/` tokens，不共享业务组件

---

## 核心数据模型（P1 相关）

```rust
pub struct PlatformState {
    // 懒加载：首次请求时创建
    pub tenants: DashMap<TenantId, Arc<TenantRuntime>>,
    pub config: PlatformConfig,
}

pub struct TenantRuntime {
    pub tenant: Tenant,
    // 每用户独立 AgentRuntime（懒加载）
    pub user_runtimes: DashMap<UserId, Arc<AgentRuntime>>,
}

pub struct PlatformUser {
    pub id: UserId,
    pub tenant_id: TenantId,
    pub email: String,
    pub role: UserRole,  // TenantAdmin / Member / Viewer
}
```

---

## 关键代码路径

| 组件 | 路径 |
|------|------|
| octo-engine AgentRuntime | `crates/octo-engine/src/agent/runtime.rs` |
| octo-engine AgentRuntimeConfig | `crates/octo-engine/src/agent/runtime.rs` |
| octo-server AppState（参考） | `crates/octo-server/src/state.rs` |
| octo-server main.rs（参考） | `crates/octo-server/src/main.rs` |
| octo-server router.rs（参考） | `crates/octo-server/src/router.rs` |
| Cargo.toml（workspace） | `Cargo.toml` |
| 平台设计文档 | `docs/plans/2026-03-04-octo-platform-design.md` |

---

## 快速启动命令

```bash
# 构建验证（工作区）
cargo check --workspace

# 新建 crate 后验证
cargo build -p octo-platform-server

# 运行平台服务（P1-1 完成后）
cargo run -p octo-platform-server
```

---

## 下一步操作

```bash
# 开始实施 Platform P1
# 第一步：新建 octo-platform-server crate
/brainstorming  # 讨论 P1-1 实现方案
# 或直接开始
/writing-plans  # 细化 P1 实施计划
```

---

## 重要记忆引用

| claude-mem ID | 内容 |
|---------------|------|
| #3052 | octo-platform 设计完成检查点（产品定位、架构、路线图） |
| #3044 | AgentRuntime 核心架构（自主智能体统一运行时） |
| #3045 | Agent 注册表数据模型（AgentEntry/AgentManifest） |

---
