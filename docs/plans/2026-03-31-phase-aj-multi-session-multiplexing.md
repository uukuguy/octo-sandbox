# Phase AJ — 多会话复用（Multi-Session Multiplexing）

> **目标**: 让单个 octo 进程能在同一个 Tokio 运行时内同时管理多个独立会话，每个会话拥有完全隔离的上下文、工具和数据环境。
> **设计文档**: `docs/design/MULTI_SESSION_DESIGN.md`

---

## 任务分组

### G1: 会话隔离修复（P0，安全前提）

**T1: ToolRegistry 分层 — 基线/会话两层架构**
- `AgentRuntime.tools` 改为 `base_tools: Arc<ToolRegistry>`（不可变，去掉 Mutex）
- 每个 `AgentExecutor` 持有 `session_tools: Arc<StdMutex<ToolRegistry>>`
- 初始化时从 `base_tools` 做 `snapshot()`
- `mcp_install` / `mcp_remove` 只修改 `session_tools`
- 文件: `runtime.rs`, `executor.rs`, `mcp_manage.rs`

**T2: Knowledge Graph 会话隔离**
- `AgentRuntime` 新增 `session_kgs: DashMap<SessionId, Arc<RwLock<KnowledgeGraph>>>`
- 每 session 创建时初始化独立 KG 实例
- session 结束时 drop KG
- KG 工具注册时传入 session 级 KG 引用
- 文件: `runtime.rs`, `executor.rs`, `tools/mod.rs` (KG 工具注册)

**T3: Memory Search 会话过滤**
- `fts_search()` 新增 `session_id: Option<&str>` 参数
- SQL 加 `AND (?3 IS NULL OR m.session_id = ?3)`
- `vector_search()` 同理
- `search()` 方法将 `SearchOptions.session_id` 传递下去
- memory_search 工具默认传入当前 session_id
- 文件: `sqlite_store.rs`, `tools/memory.rs`

**T4: MCP Manager 会话跟踪**
- `McpManager` 新增 `session_servers: HashMap<String, HashSet<String>>`
- `mcp_install` 需传入 session_id，记录所有权
- executor tool snapshot 只包含该 session 拥有的 MCP tools
- `mcp_remove` 只允许移除自己 session 的 server
- session 结束时自动清理 MCP 连接
- 文件: `mcp/manager.rs`, `tools/mcp_manage.rs`

### G2: 多会话注册表（P0，核心）

**T5: SessionRegistry 数据结构**
- `AgentRuntime` 新增 `sessions: DashMap<SessionId, SessionEntry>`
- `SessionEntry` 包含: handle, user_id, created_at, session_tools
- 保留 `primary_session_id: Mutex<Option<SessionId>>` 兼容字段
- 新增 `max_concurrent_sessions: usize` 配置
- 文件: `runtime.rs`

**T6: `start_session()` / `stop_session()` / `get_session_handle()` API**
- `start_session()` — 构建独立 executor（独立 channels、WorkingMemory、KG、session_tools）
- `stop_session()` — 从 DashMap 移除 + drop handle + 清理 KG/MCP
- `get_session_handle()` / `active_sessions()` / `active_session_count()`
- 提取公共 executor 构建逻辑为 `build_executor()` 私有方法
- 文件: `runtime.rs`

**T7: 重构 `start_primary()` 复用 `start_session()`**
- `start_primary()` 内部调用 `start_session()` + 更新 `primary_session_id`
- `primary()` 方法从 sessions DashMap 查找 primary
- 完全后向兼容
- 文件: `runtime.rs`

### G3: WebSocket 多会话路由 + REST API（P1）

**T8: WS 协议 session_id 路由**
- `/ws?session_id=xxx` — 路由到指定 session
- 无参数 → primary session（后向兼容）
- 不存在的 session_id → 自动创建
- 提取 query param 解析逻辑
- 文件: `ws.rs`

**T9: AppState 解耦 + REST 会话端点**
- AppState 保留 `agent_handle`（兼容），新增 `resolve_session_handle()` helper
- `POST /api/sessions` — 创建新会话
- `DELETE /api/sessions/:id` — 停止会话
- `GET /api/sessions/:id/status` — 会话状态
- `GET /api/sessions/active` — 活跃会话列表
- 文件: `state.rs`, `api/sessions.rs`, `router.rs`

**T10: 配置项 + 会话上限保护**
- `sessions.max_concurrent` (默认 64)
- `sessions.idle_timeout_secs` (默认 3600, 0 = 不超时)
- `sessions.memory_isolation` (strict/relaxed)
- `start_session()` 超限返回错误
- 文件: `config.rs`, `runtime.rs`

### G4: 测试（P1）

**T11: 隔离测试**
- Session A 安装 MCP tool → Session B 看不到
- Session A 写 KG entity → Session B 查不到
- Session A 写 memory → Session B 搜不到（strict 模式）
- Session A mcp_remove → Session B 工具不受影响
- 文件: `crates/octo-engine/tests/session_isolation.rs`

**T12: 多会话生命周期测试**
- start_session × N → active_session_count == N
- stop_session → executor 自然退出
- 并发创建多会话 → 上限保护生效
- primary 兼容性 — start_primary 仍可用
- 文件: `crates/octo-engine/tests/multi_session.rs`

**T13: WebSocket 多会话集成测试**
- 带 session_id 的 WS 连接路由正确
- 两个 WS 到不同 session → 消息互不干扰
- 无 session_id → 使用 primary（后向兼容）
- 文件: `crates/octo-server/tests/ws_multi_session.rs`

---

## 执行顺序与依赖

```
G1 (隔离修复):
  T1 (ToolRegistry 分层) ──┐
  T2 (KG 隔离) ───────────┤
  T3 (Memory 过滤) ────────┤── 可并行
  T4 (MCP 跟踪) ──────────┘

G2 (注册表):
  T5 (SessionRegistry) ← 依赖 T1,T2 完成
  T6 (start/stop API) ← 依赖 T5
  T7 (重构 primary) ← 依赖 T6

G3 (路由):
  T8 (WS 路由) ← 依赖 T6
  T9 (REST API) ← 依赖 T6
  T10 (配置) ← 依赖 T5

G4 (测试):
  T11 (隔离测试) ← 依赖 G1 完成
  T12 (生命周期) ← 依赖 G2 完成
  T13 (WS 集成) ← 依赖 G3 完成
```

**推荐执行流**: G1 (T1-T4 并行) → G2 (T5→T6→T7) → G3 (T8,T9,T10) → G4 (T11,T12,T13)

---

## Deferred

| ID | 描述 | 优先级 | 阻塞 |
|----|------|--------|------|
| AJ-D1 | IPC 健康检查心跳（Unix socket / gRPC） | P3 | 平台 Pod 架构 |
| AJ-D2 | 崩溃恢复 — EventStore 重放恢复会话 | P2 | 本阶段 session registry |
| AJ-D3 | 优雅关闭 — SIGTERM checkpoint 所有会话 | P2 | AJ-D2 |
| AJ-D4 | 会话 idle 超时自动回收 | P2 | ✅ 已补 @ 21a82fc |
| AJ-D5 | 前端多会话 UI（tab 切换器） | P3 | WS 路由完成 |
| AJ-D6 | Event Bus 会话过滤订阅 | P4 | 观测需求 |
| AJ-D7 | KG scope 字段（Global/User/Session） | P3 | 跨 session 知识需求 |

---

## 验收标准

1. ✅ 隔离：Session A 的 MCP 工具、KG、记忆对 Session B 不可见
2. ✅ 并发：单进程同时运行 N 个独立会话（N ≤ max_concurrent_sessions）
3. ✅ 路由：WS `?session_id=` 路由到正确会话
4. ✅ 兼容：不指定 session_id 时行为与当前完全一致
5. ✅ 编译：`cargo check --workspace` 零 warning
6. ✅ 测试：`cargo test --workspace -- --test-threads=1` 全部通过

---

## Baseline

- **Tests**: 2476 (from Phase AI)
- **Commit**: 7c1b27b (HEAD of main)
- **DB Version**: 12 (no migration needed)
