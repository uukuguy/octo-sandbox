# Wave 6 执行计划 — 生产化加固 + 端到端验证

> **目标**: 将已实现的 24 个引擎模块、21 个 API 端点、50+ CLI 文件打通为可运行的端到端体验，消除生产风险。
>
> **基线**: 1548 tests passing @ commit `d95e468`（Wave 5 完成后）
>
> **前置完成**: Wave 1-5 全部完成（共识、同步、TLS、安全、技能、CLI、前端）
>
> **定位**: 这是 octo-sandbox v1.0 的**最后一个实施阶段**，聚焦于加固而非新功能。

---

## 设计决策记录

| 决策点 | 选项 | 决策 | 理由 |
|--------|------|------|------|
| CLI 架构 | A)嵌入引擎(现状) B)HTTP客户端连server | **维持 A) 嵌入引擎** | CLI 独立运行是合理设计，server 模式作为可选增强 |
| E2E 测试策略 | A)启动真实server B)Mock HTTP层 | **B) Mock HTTP 层** | 避免端口冲突和网络依赖，用 axum::TestServer |
| Docker 前端 | A)Dockerfile内嵌nginx B)compose分离 | **维持 B) compose 分离** | 灵活部署，Caddy/Nginx 可替换 |
| .unwrap() 治理 | A)全部替换 B)仅关键路径 | **B) 仅关键路径** | 测试代码中的 unwrap 可接受，聚焦生产代码 |

---

## 总览

| Wave | 主题 | Tasks | 估计 LOC | 并行策略 |
|------|------|-------|---------|---------|
| **Wave 6a** | Server E2E 测试 | 5 subtasks | ~800 | 可并行 |
| **Wave 6b** | 生产加固 | 5 subtasks | ~400 | 可并行 |
| **Wave 6c** | 配置 & 部署完善 | 5 subtasks | ~300 | 可并行 |

**总计**: 15 tasks

---

## Wave 6a: Server 端到端测试

### 前置条件: 全部满足
- octo-server 有完整的 21 个 API 端点
- octo-engine 1548 tests 全部通过

### 任务分解

#### W6a-T1: Server 测试基础设施

**目标**: 创建 `crates/octo-server/tests/` 测试框架，支持不启动真实端口的 API 测试。

**文件**:
- `crates/octo-server/tests/common/mod.rs` — TestApp 辅助结构

**实现要点**:
```rust
// 使用 axum::Router 直接测试，无需绑定端口
pub struct TestApp {
    pub router: Router,
    pub db_path: PathBuf,  // 临时 SQLite
}

impl TestApp {
    pub async fn new() -> Self { ... }

    // 使用 tower::ServiceExt::oneshot 发送请求
    pub async fn get(&self, uri: &str) -> Response { ... }
    pub async fn post_json(&self, uri: &str, body: Value) -> Response { ... }
}
```

**验收**: `cargo test -p octo-server` 能运行。

---

#### W6a-T2: Health & Config API 测试

**文件**: `crates/octo-server/tests/api_health.rs`

**测试用例** (5 tests):
1. `GET /api/health` → 200, body 含 `status: "ok"`
2. `GET /api/config` → 200, body 含 provider/host/port
3. `GET /api/metrics` → 200, 返回 metrics 结构
4. `GET /api/budget` → 200, 返回 token budget
5. 未认证请求（如果启用 auth）→ 401

**验收**: 5 tests passing。

---

#### W6a-T3: Agent 生命周期 API 测试

**文件**: `crates/octo-server/tests/api_agents.rs`

**测试用例** (6 tests):
1. `POST /api/agents` 创建 agent → 201, 返回 agent_id
2. `GET /api/agents` 列出 agents → 200, 含刚创建的 agent
3. `GET /api/agents/{id}` 获取详情 → 200
4. `POST /api/agents/{id}/start` 启动 → 200, state = "Running"
5. `POST /api/agents/{id}/stop` 停止 → 200, state = "Stopped"
6. `DELETE /api/agents/{id}` 删除 → 200
7. `GET /api/agents/{unknown_id}` → 404

**验收**: 7 tests passing。

---

#### W6a-T4: Session & Memory API 测试

**文件**: `crates/octo-server/tests/api_session_memory.rs`

**测试用例** (6 tests):
1. `GET /api/sessions` → 200, 初始为空或含默认 session
2. `POST /api/memories` 存储记忆 → 201
3. `GET /api/memories` 列出记忆 → 200, 含刚存储的
4. `GET /api/memories?q=keyword` 搜索 → 200, 结果匹配
5. `GET /api/memories/working` 工作记忆 → 200
6. `DELETE /api/memories/{id}` 删除 → 200

**验收**: 6 tests passing。

---

#### W6a-T5: MCP & Scheduler API 测试

**文件**: `crates/octo-server/tests/api_mcp_scheduler.rs`

**测试用例** (6 tests):
1. `GET /api/mcp/servers` → 200, 初始为空
2. `POST /api/mcp/servers` 添加 MCP 配置 → 201
3. `GET /api/tools` → 200, 返回内置工具列表
4. `POST /api/scheduler/tasks` 创建定时任务 → 201
5. `GET /api/scheduler/tasks` 列出任务 → 200
6. `GET /api/audit/events` 审计日志 → 200

**验收**: 6 tests passing。

---

## Wave 6b: 生产加固

### 前置条件: 无（与 Wave 6a 可并行）

#### W6b-T1: skills/loader.rs Mutex unwrap 修复

**目标**: 消除 `skills/loader.rs` 中关键路径的 `.unwrap()` 调用。

**文件**: `crates/octo-engine/src/skills/loader.rs`

**具体修复**:
- Line 48: `NonZeroUsize::new(BODY_CACHE_CAPACITY).unwrap()` → 使用 `const` 确保安全，或 `expect("const > 0")`
- Lines 221, 234: `self.body_cache.lock().unwrap()` → 替换为 `self.body_cache.lock().map_err(|_| OctoError::Internal("cache lock poisoned"))?`
- 扫描同文件其他生产路径的 `.unwrap()`

**验收**: `cargo clippy -p octo-engine` 无新 warning。

---

#### W6b-T2: octo-server .unwrap() 修复

**目标**: 清理 `crates/octo-server/src/` 中的 3 个 `.unwrap()`。

**文件**:
- `crates/octo-server/src/ws.rs:128` — 替换为 `?` 或 `expect`
- `crates/octo-server/src/api/events.rs:91,99` — 如在测试中则保留，否则替换

**验收**: `grep -rn '\.unwrap()' crates/octo-server/src/ --include='*.rs'` 仅剩测试代码中的调用。

---

#### W6b-T3: 关键模块 .unwrap() 扫描与修复

**目标**: 清理 octo-engine 中非测试代码的高风险 `.unwrap()` 调用。

**范围**: 仅修复以下关键路径文件（非测试代码）:
- `agent/runtime.rs`
- `agent/executor.rs`
- `agent/harness.rs`
- `agent/loop_.rs`
- `providers/anthropic.rs`
- `providers/openai.rs`
- `mcp/manager.rs`

**策略**:
- Mutex/RwLock `.unwrap()` → `.map_err()` 或 `.expect("reason")`
- JSON parse `.unwrap()` → `?` propagation
- 保留 `const` 初始化中的 `.unwrap()`（编译期安全）

**验收**: 上述 7 个文件中生产代码无裸 `.unwrap()`。

---

#### W6b-T4: Error Response 统一化

**目标**: 确保所有 API 端点返回统一的错误格式。

**文件**: `crates/octo-server/src/api/mod.rs` 或新建 `error.rs`

**统一格式**:
```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "Agent not found: agent-123"
  }
}
```

**实现**:
- 定义 `ApiError` 枚举，实现 `IntoResponse`
- 确保 404/400/500 都用统一结构
- 检查现有 handler 的错误返回是否一致

**验收**: 所有 API handler 使用统一错误类型。

---

#### W6b-T5: Graceful Shutdown

**目标**: 确保 server 收到 SIGTERM/SIGINT 时优雅关闭。

**文件**: `crates/octo-server/src/main.rs`

**实现**:
- 使用 `tokio::signal::ctrl_c()` + `axum::serve().with_graceful_shutdown()`
- 关闭前: flush EventStore, close DB connections, stop scheduler
- 超时: 30s 后强制退出

**验收**: `kill -TERM <pid>` 后 server 清洁退出，无 "broken pipe" 日志。

---

## Wave 6c: 配置 & 部署完善

### 前置条件: 无（与 Wave 6a/6b 可并行）

#### W6c-T1: config.default.yaml 同步更新

**目标**: 将 Wave 3-5 新增的功能模块配置加入 `config.default.yaml`。

**文件**: `config.default.yaml`

**新增配置项**:
```yaml
# TLS (Wave 5c)
tls:
  enabled: false
  cert_path: ""
  key_path: ""
  self_signed: false

# Sync (Wave 5b)
sync:
  enabled: false
  node_id: ""  # 自动生成 UUID

# Auth
auth:
  mode: "none"  # none | api_key | jwt
  api_key: ""

# Scheduler
scheduler:
  enabled: true
  check_interval_secs: 60
  max_concurrent: 5

# Security
security:
  autonomy_level: "supervised"  # restricted | supervised | autonomous
  max_tool_calls_per_turn: 25
```

**验收**: `make config-gen` 后 `config.default.yaml` 包含所有配置项。

---

#### W6c-T2: Docker 部署验证

**目标**: 确保 Dockerfile + docker-compose.yml 能正确构建和运行。

**文件**:
- `Dockerfile` — 检查多阶段构建正确性
- `docker-compose.yml` — 检查服务配置
- `docker/nginx.conf` — 确保前端代理配置正确

**验证清单**:
1. `docker build -t octo-sandbox .` 能成功构建
2. `docker-compose up` 能启动 server + nginx
3. `curl http://localhost:3001/api/health` 返回 200
4. `curl http://localhost:5180` 能访问前端

**验收**: 提供验证命令和预期输出（用户手动执行）。

---

#### W6c-T3: 部署文档

**目标**: 创建生产部署 checklist。

**文件**: `docs/design/DEPLOYMENT_GUIDE.md`

**内容**:
1. 环境要求（Rust, Node.js, SQLite）
2. 配置文件说明（config.yaml 全字段参考）
3. Docker 部署步骤
4. Caddy/Nginx 反向代理配置
5. TLS 配置（自签名 vs Let's Encrypt via Caddy）
6. 健康检查端点
7. 日志配置
8. 备份策略（SQLite 文件）

**验收**: 文档完整，按步骤可部署。

---

#### W6c-T4: CLI 验证用例执行

**目标**: 验证 CLI 核心命令功能正常。

**验证用例**（来自 `docs/design/CLI_VERIFICATION_CASES.md`）:
1. `octo doctor --fix` — 环境检查通过
2. `octo config show` — 显示配置
3. `octo config validate` — 配置校验通过
4. `octo agent list` — 显示 agent 列表
5. `octo session list` — 显示 session 列表
6. `octo tools list` — 显示内置工具
7. `octo mcp list` — 显示 MCP servers
8. `octo completions zsh` — 生成补全脚本

**验收**: 提供验证命令清单（用户手动执行），记录通过/失败。

---

#### W6c-T5: CHANGELOG 与版本标记

**目标**: 为 v1.0 准备发布材料。

**文件**:
- `CHANGELOG.md` — 从 git log 生成完整变更日志
- `Cargo.toml` (workspace) — 确认版本号一致

**内容**:
- 按 Wave 1-6 组织变更历史
- 标注破坏性变更（如果有）
- 列出已知限制和 Deferred 项

**验收**: CHANGELOG.md 覆盖所有 Wave，版本号统一。

---

## 执行策略

### 并行执行编排

```
Wave 6a (Server E2E)     Wave 6b (加固)          Wave 6c (配置部署)
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│ T1: 测试基础设施  │    │ T1: loader unwrap│    │ T1: config 同步  │
│ T2: Health API   │    │ T2: server unwrap│    │ T2: Docker 验证  │
│ T3: Agent API    │    │ T3: 关键模块 unwrap│   │ T3: 部署文档     │
│ T4: Session API  │    │ T4: Error 统一化  │    │ T4: CLI 验证     │
│ T5: MCP API      │    │ T5: Graceful stop│    │ T5: CHANGELOG    │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                      │                      │
         └──────────────────────┼──────────────────────┘
                                │
                     ┌──────────▼──────────┐
                     │ 最终验证 & 提交       │
                     │ cargo test --workspace│
                     │ cargo clippy          │
                     │ make verify           │
                     └─────────────────────┘
```

**三组可完全并行**:
- Wave 6a: 需要先完成 T1（测试基础设施），然后 T2-T5 并行
- Wave 6b: T1-T5 全部独立，可并行
- Wave 6c: T1-T5 全部独立，可并行

### RuFlo Swarm 配置

```bash
npx @claude-flow/cli@latest swarm init --topology hierarchical --max-agents 8 --strategy specialized
```

**智能体分配**:
- Coordinator: 总控 + checkpoint
- Agent 1-2: Wave 6a (server tests)
- Agent 3-4: Wave 6b (unwrap + error + shutdown)
- Agent 5-6: Wave 6c (config + docker + docs)

---

## 提交策略

```
Wave 6a:
  commit 1: "test(server): W6a-T1+T2 — Server test infrastructure + health API tests"
  commit 2: "test(server): W6a-T3+T4 — Agent lifecycle + session/memory API tests"
  commit 3: "test(server): W6a-T5 — MCP + scheduler API tests"

Wave 6b:
  commit 4: "fix(engine): W6b-T1+T2+T3 — Replace critical .unwrap() with proper error handling"
  commit 5: "refactor(server): W6b-T4+T5 — Unified API errors + graceful shutdown"

Wave 6c:
  commit 6: "chore(config): W6c-T1 — Sync config.default.yaml with Wave 3-5 features"
  commit 7: "docs(deploy): W6c-T3+T5 — Deployment guide + CHANGELOG for v1.0"

Final:
  commit 8: "checkpoint: Wave 6 COMPLETE — production hardening, N tests"
```

---

## 验收标准

### Wave 6 完成标准
- [ ] `cargo check --workspace` 无错误
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过（>= 1548 + 新增测试）
- [ ] `cargo clippy --workspace -- -D warnings` 无 warning
- [ ] Server API 测试覆盖: health, agents, sessions, memories, MCP, scheduler, audit
- [ ] 关键路径无裸 `.unwrap()`（runtime, executor, harness, loop, providers, mcp）
- [ ] `config.default.yaml` 包含 TLS/sync/auth/scheduler/security 配置
- [ ] Docker build 成功（用户手动验证）
- [ ] CLI 核心命令可执行（用户手动验证）
- [ ] CHANGELOG.md 覆盖 Wave 1-6

---

## 风险总览

| 风险 | 等级 | 影响范围 | 缓解措施 |
|------|------|---------|---------|
| Server 测试需要 mock provider | 中 | W6a | 使用 MockProvider（已有实现） |
| .unwrap() 替换可能改变错误传播 | 低 | W6b | 逐文件替换 + 运行全量测试 |
| Docker build 可能因依赖变化失败 | 中 | W6c-T2 | 用户手动验证，提供 troubleshoot |
| config.default.yaml 生成依赖 config.rs | 低 | W6c-T1 | 先更新 config.rs 再 `make config-gen` |

---

## Deferred（仍暂缓）

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| D4-ACME | 内置 ACME 自动证书 | 公网域名 + 生产部署 | ⏳ 用 Caddy 反向代理代替 |
| D6-V2 | CRDT 离线同步 | D6-LWW 完成 + 需求验证 | ⏳ |
| D6-Desktop | Desktop 端同步集成 | D6 核心完成 | ⏳ Wave 6 不含（非加固范畴） |
| D2 | Extension + Hook 系统合并 | 重构评估 | ⏳ |
| D3 | ContentBlock 多模态扩展 | 多模态 Provider 支持 | ⏳ |
| D5 | Tauri 自动更新 | 发布流程 + artifact 托管 | ⏳ |
| D7 | SmartRouting V2 跨 Provider | T10 V1 完成 + 多 Provider 场景 | ⏳ |
| D8 | CLI Server 模式（HTTP 客户端） | 评估需求优先级 | ⏳ 新增 |
| D9 | OpenTelemetry 导出 | 外部监控需求 | ⏳ 新增 |
