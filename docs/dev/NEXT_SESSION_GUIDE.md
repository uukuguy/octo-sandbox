# Grid Platform 下一会话指南

**最后更新**: 2026-04-07 04:30 GMT+8
**当前分支**: `Grid`
**当前状态**: Phase BF 完成 — 准备 Phase BG

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
- [x] Phase BE — EAASP 协议层 + claude-code-runtime (6/6, 93 tests)
- [x] **Phase BF** — L2 统一资产层 + L1 抽象机制 (7/7, 30 new tests)
- [ ] **Phase BG** — Enterprise SDK（多语言）

## Phase BF 完成总结

**计划文档**: `docs/plans/2026-04-06-phase-bf-l2-asset-layer.md`
**设计文档**: `docs/design/Grid/EAASP_L2_ASSET_LAYER_DESIGN.md`

| Wave | 内容 | 状态 | Commit |
|------|------|------|--------|
| W1 | 协议扩展（SessionPayload L2 字段, proto v1.3） | ✅ | `1a54f95` |
| W2 | L2 Skill Registry crate (REST + SQLite + Git) | ✅ | `9e8bac5` |
| W3 | L2 MCP Orchestrator crate (YAML + subprocess) | ✅ | `9e8bac5` |
| W4 | L1 Runtime L2 集成 (GridHarness → L2 REST) | ✅ | `b6af473` |
| W5 | Mock L3 RuntimeSelector + 运行时池 | ✅ | `9e982e0` |
| W6 | 盲盒对比 (并行执行 + 匿名评分) | ✅ | `59bb58e` |
| W7 | 集成验证 + 设计文档 + Makefile | ✅ | — |

**Deferred**: BF-D1~D10（Git 版本追溯、PerSession/OnDemand 模式、RBAC、ELO 统计等）

## BF 新增组件

| 组件 | 路径 | 说明 |
|------|------|------|
| Skill Registry | `tools/eaasp-skill-registry/` | L2 Skill 仓库 REST API (SQLite + fs + git2) |
| MCP Orchestrator | `tools/eaasp-mcp-orchestrator/` | L2 MCP Server 管理 (YAML + 子进程) |
| L2 Client | `crates/grid-runtime/src/l2_client.rs` | L1 从 L2 REST 拉取 Skill |
| RuntimePool | `tools/eaasp-certifier/src/runtime_pool.rs` | 运行时池管理 |
| RuntimeSelector | `tools/eaasp-certifier/src/selector.rs` | Mock L3 选择策略 |
| Blindbox | `tools/eaasp-certifier/src/blindbox.rs` | 盲盒对比 |

## 关键代码路径

| 组件 | 路径 |
|------|------|
| SessionPayload (proto) | `proto/eaasp/runtime/v1/runtime.proto` |
| SessionPayload (Rust) | `crates/grid-runtime/src/contract.rs` |
| GridHarness | `crates/grid-runtime/src/harness.rs` |
| gRPC service | `crates/grid-runtime/src/service.rs` |
| L2 Skill Registry | `tools/eaasp-skill-registry/` |
| L2 MCP Orchestrator | `tools/eaasp-mcp-orchestrator/` |
| certifier CLI | `tools/eaasp-certifier/src/main.rs` |
| HookBridge trait | `crates/grid-hook-bridge/src/traits.rs` |
| Python runtime | `lang/claude-code-runtime-python/` |

## Makefile 新增 Targets

```bash
make skill-registry-build    # 编译 L2 Skill Registry
make skill-registry-start    # 启动 (port 8081)
make skill-registry-test     # 运行测试
make mcp-orch-build          # 编译 L2 MCP Orchestrator
make mcp-orch-start          # 启动 (port 8082)
make mcp-orch-test           # 运行测试
make certifier-blindbox      # 运行盲盒对比
```

## 建议下一步

1. Phase BG — Enterprise SDK（Python/TypeScript 多语言 SDK）
2. 或处理 BF Deferred Items（BF-D1~D10）
3. 参考路线图：`docs/design/Grid/EAASP_ROADMAP.md`
