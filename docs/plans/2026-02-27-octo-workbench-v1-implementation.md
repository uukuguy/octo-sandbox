# octo-workbench v1.0 实施规划

**项目**：octo-sandbox
**代号**：octo-workbench
**目标**：个人AI智能体模块级调试工作台 v1.0
**分支**：octo-workbench（基于 dev）

---

## 分支策略

```
main ──► dev ──► octo-workbench (Phase 2.1 → 2.2 → 2.3 → 2.4)
                        │
                        └──────────────► octo-platform (Phase 3)
```

---

## v1.0 完整功能清单

### 核心能力
- [x] AI 对话（Provider + Agent Loop）
- [x] 8 个内置工具
- [x] 会话持久化
- [x] 工具执行记录

### 调试面板（6 模块）
- Tool Execution Inspector（Timeline + JsonViewer + Replay）
- MCP Workbench（Server 管理 + 手动调用 + 日志流）
- Skill Studio（编辑 + 测试 + 热重载）
- Memory Explorer（Working/Session/Persistent 可视化）
- Network Interceptor（请求/响应拦截）
- Context Viewer（实时上下文窗口）

### 记忆系统
- Working Memory + Session Memory + Persistent Memory
- 5 memory tools（store/search/update/recall/forget）

---

## 实施路径（4 阶段 + 8 批次）

### 阶段 1：核心闭环可用（1-2 周）

| 批次 | 任务 | 工作量 | 状态 |
|------|------|--------|------|
| **1.1** | 运行时验证：服务器启动 + AI 对话 + 工具执行 | 3-5 天 | ⏳ |
| **1.2** | 执行记录完善：Timeline + JsonViewer | 3-5 天 | ⏳ |

### 阶段 2：记忆系统完整（1-2 周）

| 批次 | 任务 | 工作量 | 状态 |
|------|------|--------|------|
| **2.1** | 5 memory tools：recall + forget 新增 | 3-5 天 | ⏳ |
| **2.2** | Memory Explorer：Working/Session/Persistent 可视化 | 3-5 天 | ⏳ |

### 阶段 3：调试面板完善（1.5-2 周）

| 批次 | 任务 | 工作量 | 状态 |
|------|------|--------|------|
| **3.1** | MCP Workbench：Server 管理 + 手动调用 + 日志流 | 3-5 天 | ⏳ |
| **3.2** | Skill Studio：编辑 + 测试 + 热重载 | 3-5 天 | ⏳ |
| **3.3** | 高级调试：Network Interceptor + Context Viewer | 3-5 天 | ⏳ |

### 阶段 4：v1.0 完成（1-2 周）

| 批次 | 任务 | 工作量 | 状态 |
|------|------|--------|------|
| **4.1** | Bug 修复 + 打磨：稳定性 + 体验优化 | 3-5 天 | ⏳ |
| **4.2** | 测试 + 文档：验收测试 + 用户文档 | 3-5 天 | ⏳ |
| **4.3** | v1.0 Release：合并回 dev + Tag | 1-2 天 | ⏳ |

---

## 交付节奏

| 阶段 | 周期 | 核心交付 |
|------|------|----------|
| Phase 2.1 | 1-2 周 | 核心闭环 + 基础调试 |
| Phase 2.2 | 1-2 周 | 完整记忆系统 |
| Phase 2.3 | 1.5-2 周 | 调试面板全功能 |
| Phase 2.4 | 1-2 周 | v1.0 Release |

**预计总周期：4.5-8 周**

---

## 实施要求

1. 每个批次控制在 3-5 天工作量，最多不超过 1-2 周
2. 每个批次结束时应产出可演示的增量功能
3. 批次之间保持功能连贯性
4. 阶段切换时进行评审

---

**创建日期**：2026-02-27
**最后更新**：2026-02-27
