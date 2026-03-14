# Phase G — 暂缓项消除（Deferred Resolution）

**日期**: 2026-03-14
**前置**: Phase F COMPLETE @ b4d1cd2 (20/23 tasks, 1962 tests)
**目标**: 消除 Phase F 遗留的 2 个暂缓项，使评估框架达到完整状态

---

## 背景

Phase F 完成后遗留 2 个暂缓项：
1. **F3-T4**: Rust E2E fixtures — `e2e.rs` 仅支持 Python fixture，需扩展支持 Rust/Cargo
2. **F4-T1**: Server HTTP eval mode — octo-server 缺少 REST 端点，无法实现 `EvalTarget::Server`

---

## 一、任务分组

### Phase G1: Rust E2E Fixtures（自包含，无外部依赖）

**分析**: 当前 `e2e.rs` 已经是语言无关的设计：
- `FixtureManifest` 有 `test_cmd` 字段（可以是 `cargo test` 而非 `python3 test.py`）
- `run_test_cmd()` 用 `sh -c` 执行任意命令
- `fix_file` 指定修复文件名（可以是 `.rs` 而非 `.py`）

**结论**: `e2e.rs` 实际上**不需要重构**！只需要：
1. 创建 6 个 Rust fixture 目录（每个含 Cargo.toml + 带 bug 的源码 + 测试 + 修复文件）
2. 将 `fix.py` 硬编码改为使用 `manifest.json` 中的 `fix_file` 字段
3. 更新测试断言（8 → 14 fixtures）

**G1-T1: 修复 e2e.rs 中 fix.py 硬编码**
- 当前 `run_mock_fixture()` 第 127 行硬编码 `fix.py`：`let fix_path = fixture_path.join("fix.py");`
- 改为使用 manifest 中的 `fix_file` 字段来定位修复文件（而非硬编码 `fix.py`）
- 同时需要添加新字段 `source_file` 到 manifest，表示要替换的源文件名
- **文件改动**: `e2e.rs` ~5 行修改

**G1-T2: 创建 6 个 Rust E2E fixtures**
- 每个 fixture 结构:
  ```
  e2e_fixtures/r{n}-{seq}/
    Cargo.toml      # 独立 Rust 项目（不加入 workspace）
    src/lib.rs      # 带 bug 的源码
    tests/test.rs   # 验证测试（cargo test 运行）
    fix.rs          # 正确源码（替换 src/lib.rs）
    manifest.json   # { id, name, test_cmd: "cargo test", fix_file: "src/lib.rs", difficulty }
  ```

| Fixture ID | Bug 类型 | 难度 |
|------------|----------|------|
| e2e-R1-01 | Off-by-one: `.take(n-1)` 应为 `.take(n)` | easy |
| e2e-R1-02 | 错误的 `.unwrap()` 应为 `.unwrap_or_default()` | easy |
| e2e-R2-01 | 缺少 `pub` 关键字导致编译错误 | medium |
| e2e-R2-02 | 生命周期/所有权错误: `&str` 应为 `String` | medium |
| e2e-R3-01 | 多文件: struct 字段改名但调用点未更新 | hard |
| e2e-R3-02 | trait impl 缺少 required method | hard |

- **文件改动**: 6 个新 fixture 目录

**G1-T3: 更新测试断言**
- `test_e2e_suite_runs`: 8 → 14 fixtures
- `test_e2e_all_fixtures_pass`: 8 → 14
- `test_fixtures_dir_exists`: 8 → 14 directories
- **文件改动**: `e2e.rs` tests 模块 ~6 行

**G1-T4: 验证 + Checkpoint**
- `cargo test -p octo-eval -- --test-threads=1` 全量通过
- `cargo check --workspace` 无 warning

---

### Phase G2: Server HTTP Eval Mode（需要 octo-server 端点）

**G2-T1: octo-server 新增 3 个 REST 端点**

在 `crates/octo-server/src/api/sessions.rs` 新增：

1. `POST /api/sessions` — 创建 eval session
   - Request: `{ "agent_id": "default" }` (可选)
   - Response: `{ "session_id": "uuid" }`
   - 实现: 调用 `agent_supervisor.session_store().create_session()`

2. `POST /api/sessions/{id}/messages` — 同步发送消息并等待完整响应
   - Request: `{ "content": "user prompt" }`
   - Response: `{ "text": "...", "tool_calls": [...], "rounds": N, "input_tokens": N, "output_tokens": N, "stop_reason": "..." }`
   - 实现: 通过 `AgentExecutorHandle` 发送消息，订阅 broadcast 收集所有事件直到 `Done`/`Completed`
   - **关键**: 需要为 eval session 创建临时 AgentExecutor（不复用主 executor）

3. `DELETE /api/sessions/{id}` — 清理 session
   - Response: `{ "deleted": true }`
   - 实现: 调用 session_store 删除

- **文件改动**: `sessions.rs` ~120 行, `mod.rs` routes ~5 行

**G2-T2: octo-eval 新增 EvalTarget::Server + ServerConfig**

- 在 `config.rs` 新增:
  ```rust
  EvalTarget::Server(ServerConfig)

  pub struct ServerConfig {
      pub base_url: String,    // default: "http://127.0.0.1:3001"
      pub timeout_secs: u64,   // default: 120
      pub api_key: Option<String>,
  }
  ```

- **文件改动**: `config.rs` ~25 行

**G2-T3: octo-eval runner.rs 新增 run_task_server()**

- 实现 `run_task_server()`:
  1. `POST /api/sessions` → 获取 session_id
  2. `POST /api/sessions/{id}/messages` → body: `{"content": task.prompt()}`
  3. 解析响应 JSON → 转换为 `AgentOutput`
  4. `DELETE /api/sessions/{id}` → 清理
  5. 超时处理: reqwest timeout
- 更新 `run_task()` dispatch match 增加 `EvalTarget::Server` 分支
- 更新 `create_provider_from_config()` 处理 Server 模式

- **文件改动**: `runner.rs` ~80 行

**G2-T4: main.rs CLI 增加 --target server 选项**

- 新增 `--target server --server-url <URL>` CLI 参数
- **文件改动**: `main.rs` ~15 行

**G2-T5: 测试验证**
- 单元测试: mock HTTP server 验证 request/response 格式
- `cargo test --workspace -- --test-threads=1` 全量通过

---

## 二、执行顺序

```
G1 (Rust E2E Fixtures)     — 完全自包含，可先行
  G1-T1 → G1-T2 → G1-T3 → G1-T4
                                    ↘
G2 (Server HTTP)            — 较复杂  G2-T5 (全量验证)
  G2-T1 → G2-T2 → G2-T3 → G2-T4 ↗
```

G1 和 G2 之间无依赖，可并行执行。

---

## 三、验收标准

| Phase | 验收标准 |
|-------|---------|
| G1 | 14 个 E2E fixtures 全部通过 mock 测试; 6 个 Rust fixtures 使用 `cargo test` 验证 |
| G2 | `EvalTarget::Server` 可用; 3 个 REST 端点工作; `--target server` CLI 可用; mock 测试通过 |

---

## 四、风险

| 风险 | 缓解 |
|------|------|
| Rust fixture `cargo test` 在 tempdir 中编译慢 | 保持 fixture 极简（单文件 lib.rs），编译 <5s |
| Server eval 的临时 AgentExecutor 创建复杂 | 参考 ws.rs 的 handle 使用模式，复用 AgentRuntime API |
| Rust fixture 的 Cargo.toml 不应加入 workspace | 使用 `[workspace] exclude` 或放在 datasets/ 下（已在 .gitignore 外） |

---

## Deferred（暂缓项）

> 本阶段暂无暂缓项。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
