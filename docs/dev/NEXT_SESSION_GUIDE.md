# octo-sandbox 下一会话指南

**最后更新**: 2026-03-06 GMT+8
**当前分支**: `dev` (已合并 octo-workbench + octo-platform)
**当前状态**: ✅ v1.0 Release Sprint 完成 + octo-platform P1+P2 完成

---

## 产品背景

octo-sandbox 是一个 mono-repo，包含两个产品：

```
octo-types    ← 共享类型
octo-engine   ← 共享核心引擎
     ↙                    ↘
octo-workbench            octo-platform
（单用户单实例）            （多租户多用户多Agent）
branch: dev              branch: dev (已合并)
```

---

## 阶段进度

| 阶段 | 状态 | 说明 |
|------|------|------|
| Phase 1 核心引擎 | ✅ 完成 | 32 Rust + 16 TS 文件，E2E 验证通过 |
| Phase 2.1–2.11 | ✅ 完成 | 全部子阶段完成（含 AgentRegistry + AgentRuntime 重构） |
| v1.0 Release Sprint | ✅ 完成 | Phase A-D 全部 24 任务完成 |
| octo-platform P1 | ✅ 完成 | Auth + 单租户多用户 |
| octo-platform P2 | ✅ 完成 | 多租户 + 配额 + MCP 隔离 |
| octo-platform P3 | ⏳ 未开始 | Multi-Agent 编排 |
| octo-platform P4 | ⏳ 未开始 | 生产就绪 |

---

## v1.0 Release Sprint 任务清单 (已全部完成)

**计划文档**: `docs/plans/2026-03-04-v1.0-release-sprint-plan.md`

### Phase A — 稳定地基 ✅

| Task | 内容 | 状态 |
|------|------|------|
| A1 | 修复 stop_primary（drop tx 语义） | ✅ |
| A2 | ToolRegistry 版本化共享引用（MCP 热插拔） | ✅ |
| A3 | 修复 Scheduler run_now 真实执行 | ✅ |
| A4 | WorkingMemory per-session 隔离 | ✅ |
| A5 | 优雅关机（MCP shutdown_all） | ✅ |
| A6 | 确认 Provider 重试已实现 | ✅ |

### Phase B — 后端能力 ✅

| Task | 内容 | 状态 |
|------|------|------|
| B1 | 并行工具执行（enable_parallel 生效） | ✅ |
| B2 | 后台任务 API（POST/GET /api/tasks） | ✅ |
| B3 | 增强 /health 端点 | ✅ |
| B4 | 补发 LoopTurnStarted 事件（修复 turns.total 指标） | ✅ |
| B5 | JSON 日志格式支持 | ✅ |
| B6 | 移除 Option<McpManager>（清理噪声） | ✅ |

### Phase C — 前端控制台 ✅

| Task | 内容 | 状态 |
|------|------|------|
| C1 | TabBar 扩展（添加新页面标签） | ✅ |
| C2 | Tasks 页面 | ✅ |
| C3 | Schedule 页面 | ✅ |
| C4 | Tools 页面（MCP + Built-in + Skills） | ✅ |
| C5 | Memory 页面 | ✅ |
| C6 | Debug 页面 | ✅ |
| C7 | Chat 页面完善 | ✅ |

### Phase D — 集成验收 ✅

| Task | 内容 | 状态 |
|------|------|------|
| D1 | 端到端测试脚本 | ✅ |
| D2 | Docker Compose 一键启动 | ✅ |
| D3 | 配置文档完善 | ✅ |
| D4 | 发布 Checklist 验证 | ✅ |

---

## octo-platform 任务清单

### P1: Auth + 单租户多用户 ✅

| Task | 内容 | 状态 |
|------|------|------|
| P1-1 | 新建 `octo-platform-server` crate，基础 Axum 服务 | ✅ |
| P1-2 | 自建用户系统：注册/登录/JWT | ✅ |
| P1-3 | `PlatformState`：单租户 + `DashMap<UserId, Arc<AgentRuntime>>` | ✅ |
| P1-4 | 每用户独立 WebSocket + AgentRuntime 懒加载 + Agent 池 | ✅ |
| P1-5 | Admin API：用户 CRUD、角色管理 | ✅ |
| P1-6 | `web-platform/` 初始化：登录页 + 用户工作空间 | ✅ |

### P2: 多租户 + 配额 + MCP 隔离 ✅

| Task | 内容 | 状态 |
|------|------|------|
| P2-1 | Extend User table with tenant_id | ✅ |
| P2-2 | TenantManager + TenantRuntime | ✅ |
| P2-3 | QuotaManager sliding window | ✅ |
| P2-4 | QuotaMiddleware 429 | ✅ |
| P2-5 | Tenant MCP config API | ✅ |
| P2-6 | OAuth2 abstraction + Google/GitHub | ✅ |
| P2-7 | Audit logging event-driven | ✅ |
| P2-8 | Admin tenant management API | ✅ |
| P2-9 | Build verification | ✅ |

---

## 关键设计决策

1. **两级隔离**：租户（独立 DB/MCP/配额） + 用户（独立 AgentRuntime）
2. **三种部署模式**：SaaS / 企业私有部署 / 开发者 API 平台
3. **认证**：自建 JWT（开发快）+ 插拔式 OIDC（企业部署）
4. **编排**：Supervisor / Peer-to-Peer / Pipeline 三种模式（P3 实现）
5. **前端策略**：独立 `web-platform/`，共享 `design/` tokens，不共享业务组件

---

## 快速启动命令

```bash
# 构建验证（工作区）
cargo check --workspace

# 运行 workbench 服务
cargo run -p octo-server

# 运行 platform 服务
cargo run -p octo-platform-server
```

---

## 下一步操作

```bash
# 开始实施 Platform P3 (Multi-Agent 编排)
/brainstorming
# 或
/writing-plans
```

---

## 重要记忆引用

| claude-mem ID | 内容 |
|---------------|------|
| #3052 | octo-platform 设计完成检查点 |
| #3044 | AgentRuntime 核心架构 |
| #3045 | Agent 注册表数据模型 |
