# Grid Platform 下一会话指南

**最后更新**: 2026-04-06 17:00 GMT+8
**当前分支**: `Grid`
**当前状态**: Phase BE W1-W3 完成 — 协议层 + HookBridge + certifier

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
- [x] **Phase BE W1-W3** — 协议层 + HookBridge + certifier (3/3, 54 tests @ 40a231e)

## Phase BE W1-W3 产出物

| 组件 | 路径 | 说明 |
|------|------|------|
| common.proto | `proto/eaasp/common/v1/common.proto` | 共享类型（HookDecision, TelemetryEvent 等） |
| hook.proto | `proto/eaasp/hook/v1/hook.proto` | HookBridge 双向流协议（StreamHooks + EvaluateHook） |
| runtime.proto | `proto/eaasp/runtime/v1/runtime.proto` | 已重构，import common.proto |
| grid-hook-bridge | `crates/grid-hook-bridge/` | HookBridge trait + InProcess + gRPC（11 tests） |
| eaasp-certifier | `tools/eaasp-certifier/` | 16 方法契约验证工具 + CLI（6 tests） |

## 下一步优先级

### Phase BE W4-W6（可选 — claude-code-runtime Python）

按路线图 `docs/design/Grid/EAASP_ROADMAP.md` §三 Phase BE 定义：

1. **W4** — claude-code-runtime Python W1：骨架 + claude-agent-sdk 封装 + gRPC service
2. **W5** — claude-code-runtime Python W2：hook 执行 + 遥测 + Skill
3. **W6** — 集成验证：certifier 验证 grid-runtime + claude-code-runtime

**前置条件**：需要先调研 claude-agent-sdk API 稳定性。

### 或直接进入 Phase BF（L2 技能资产层）

如果决定跳过 Python runtime，可直接进入 BF：
- L2 Skill 仓库 MCP server
- RuntimeSelector + AdapterRegistry
- 盲盒对比

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
| gRPC service | `crates/grid-runtime/src/service.rs` |

## 关键 API 模式（BE 中发现）

- tonic `extern_path` 必须分步编译：先编译 common.proto（无 extern_path），再编译引用方的 proto（有 extern_path 指向 `crate::common_proto`）
- 单次 build.rs 中可以调用两次 `tonic_build::configure().compile_protos()`
- `tests/` 在 .gitignore 中，需 `git add -f`
- HookBridge 双向流使用 `mpsc::channel(32)` + `ReceiverStream` 作为 server response stream

## Deferred 未清项

| 来源 | ID | 内容 | 前置条件 |
|------|----|----|---------|
| BE | D1 | GrpcHookBridge 端到端集成测试 | HookBridge server 运行 |
| BE | D2 | certifier 端到端测试 | grid-runtime gRPC 运行 |
| BE | D3 | HookBridge 双向流集成测试 | server.rs StreamHooks |
| BE | D4 | common.proto → contract.rs 映射自动化 | 手动同步足够 |
| BE | D5 | certifier mock-l3 子命令 | BH L3 策略引擎 |
| BD | D1 | grid-hook-bridge crate（T2/3 sidecar） | **BE W2 已完成，可关闭** |
| BD | D6 | initialize() payload 字段传递 | grid-engine 扩展参数 |
| BD | D7 | emit_telemetry 填充 user_id | session 存储 user_id |
