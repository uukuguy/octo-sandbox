# Grid Platform 下一会话指南

**最后更新**: 2026-04-06 20:30 GMT+8
**当前分支**: `Grid`
**当前状态**: Phase BE 完成 (6/6) — 协议层 + HookBridge + certifier + Python Runtime + 容器化

---

## 完成清单

- [x] Phase A-Z — Core Engine + Eval + TUI + Skills
- [x] Phase AA-AF — Sandbox/Config/Workspace architecture
- [x] Phase AG-AI — Memory/Hooks/WASM enhancement
- [x] Phase AJ-AO — 多会话/安全/前端/服务器
- [x] Phase AP-AV — 追赶 CC-OSS + 安全对齐
- [x] Phase AW-AY — 工具/Agent/SubAgent 体系
- [x] Phase AZ — Cleanup/Transcript/Completion
- [x] Phase BA — Octo to Grid 重命名 + TUI 完善
- [x] Phase BB-BC — TUI 视觉升级 + Deferred 补齐
- [x] Phase BD — grid-runtime EAASP L1 (6/6, 37 tests @ ae4b337)
- [x] Phase BE W1-W3 — 协议层 + HookBridge + certifier (3/3, 54 tests @ 40a231e)
- [x] **Phase BE W4-W6** — claude-code-runtime Python T1 Harness (3/3, 39 Python tests)

## Phase BE W4-W6 产出物

| 组件 | 路径 | 说明 |
|------|------|------|
| 项目骨架 | `lang/claude-code-runtime-python/` | uv + pyproject.toml + build_proto.py |
| 配置 | `src/claude_code_runtime/config.py` | ANTHROPIC_BASE_URL/MODEL_NAME/API_KEY 支持 |
| SDK 封装 | `src/claude_code_runtime/sdk_wrapper.py` | claude-agent-sdk query() 封装 |
| gRPC Service | `src/claude_code_runtime/service.py` | 16 方法 RuntimeService 实现 |
| Session | `src/claude_code_runtime/session.py` | SessionManager + Session 数据类 |
| Hook 执行器 | `src/claude_code_runtime/hook_executor.py` | T1 本地 hook 评估，deny-always-wins |
| 遥测 | `src/claude_code_runtime/telemetry.py` | TelemetryCollector per-session |
| Skill 加载 | `src/claude_code_runtime/skill_loader.py` | SkillContent 解析 + system prompt 注入 |
| 状态管理 | `src/claude_code_runtime/state_manager.py` | JSON 序列化/反序列化 |
| 验证脚本 | `scripts/verify-dual-runtime.sh` | 双 runtime 集成验证 |

## 下一步优先级

### Phase BF — L2 技能资产层 + L1 抽象机制

按路线图 `docs/design/Grid/EAASP_ROADMAP.md` §BF：

1. L2 Skill 仓库 MCP server（7 个 MCP 工具）
2. L2 晋升引擎（draft → tested → reviewed → production）
3. L1 运行时选择器（RuntimeSelector）+ 适配器注册表
4. L1 遥测采集器（统一 schema）
5. 盲盒对比（Grid vs Claude Code）

### 或处理 Deferred Items

| 来源 | ID | 内容 | 前置条件 |
|------|----|----|---------|
| BE | D1 | GrpcHookBridge 端到端集成测试 | HookBridge server 运行 |
| BE | D2 | certifier 端到端测试 | grid-runtime gRPC 运行 |
| BE | D3 | HookBridge 双向流集成测试 | server.rs StreamHooks |
| BE | D4 | common.proto → contract.rs 映射自动化 | 手动同步足够 |
| BE | D5 | certifier mock-l3 子命令 | BH L3 策略引擎 |
| BE | D6 | claude-code-runtime Dockerfile | 基本功能稳定后 |
| BE | D7 | MCP server 真实连接 | claude-agent-sdk MCP 支持 |
| BE | D8 | Skill frontmatter YAML hook 解析 | Skill 规范稳定 |
| BE | D9 | 会话持久化（当前内存） | L4 Session Store |
| BE | D10 | ANTHROPIC_BASE_URL 端到端验证 | 手动测试 |
| BD | D6 | initialize() payload 字段传递 | grid-engine 扩展参数 |
| BD | D7 | emit_telemetry 填充 user_id | session 存储 user_id |

## 关键代码路径

| 组件 | 路径 |
|------|------|
| common.proto | `proto/eaasp/common/v1/common.proto` |
| hook.proto | `proto/eaasp/hook/v1/hook.proto` |
| runtime.proto | `proto/eaasp/runtime/v1/runtime.proto` |
| HookBridge trait | `crates/grid-hook-bridge/src/traits.rs` |
| InProcessHookBridge | `crates/grid-hook-bridge/src/in_process.rs` |
| GrpcHookBridge | `crates/grid-hook-bridge/src/grpc_bridge.rs` |
| HookBridge server | `crates/grid-hook-bridge/src/server.rs` |
| Certifier verifier | `tools/eaasp-certifier/src/verifier.rs` |
| Certifier CLI | `tools/eaasp-certifier/src/main.rs` |
| RuntimeContract | `crates/grid-runtime/src/contract.rs` |
| GridHarness | `crates/grid-runtime/src/harness.rs` |
| gRPC service (Rust) | `crates/grid-runtime/src/service.rs` |
| gRPC service (Python) | `lang/claude-code-runtime-python/src/claude_code_runtime/service.py` |
| Python SDK wrapper | `lang/claude-code-runtime-python/src/claude_code_runtime/sdk_wrapper.py` |
| 双 runtime 验证 | `scripts/verify-dual-runtime.sh` |

## 关键 API 模式

- tonic `extern_path` 必须分步编译：先编译 common.proto，再编译引用方的 proto
- `tests/` 在 .gitignore 中，需 `git add -f`
- HookBridge 双向流使用 `mpsc::channel(32)` + `ReceiverStream`
- Python proto 编译需要 `_fix_imports()` 修正 grpcio 生成的绝对 import 路径
- `claude-agent-sdk` 底层启动 Claude Code CLI 进程，通过 `env` 参数传递 ANTHROPIC_BASE_URL/API_KEY

## Makefile 命令（Python Runtime）

```bash
make claude-runtime-setup    # uv sync --extra dev
make claude-runtime-proto    # 编译 proto → Python stubs
make claude-runtime-test     # pytest tests/
make claude-runtime-start    # 启动 gRPC server :50052
make verify-dual-runtime     # 启动两个 runtime + certifier 验证
```
