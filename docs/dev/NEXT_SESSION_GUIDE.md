# Grid Platform 下一会话指南

**最后更新**: 2026-04-07 12:35 GMT+8
**当前分支**: `Grid`
**当前状态**: Phase BG 完成 — 准备 Phase BH

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
- [x] Phase BF — L2 统一资产层 + L1 抽象机制 (7/7, 30 new tests)
- [x] Phase BG — Enterprise SDK 基石 (6/6, 107 new tests @ ea0780c)
- [ ] **Phase BH** — L3 治理层 + L4 基础（待设计）

## Phase BG 成果总结

**目标**: 构建 EAASP Enterprise SDK 的基石层（S1），企业开发者可创作、校验、推演 Skill

| Wave | 内容 | Tests | Commit |
|------|------|-------|--------|
| W1 | specs/ JSON Schema + Pydantic 模型 | 27 | cca61bb |
| W2 | authoring 创作工具链 | 21 | 12e795f |
| W3 | sandbox 核心 + GridCliSandbox | 13 | bd17aa5 |
| W4 | RuntimeSandbox + MultiRuntimeSandbox | 28 | a8f4cf0 |
| W5 | CLI + submit + HR 入职示例 | 18 | ea0780c |
| W6 | 文档收尾 + Makefile + ROADMAP 更新 | — | (本次) |

**Makefile 新增**: `sdk-setup`, `sdk-test`, `sdk-validate`, `sdk-build`

## SDK 长期演进路线

| 阶段 | Phase | 内容 | 状态 |
|------|-------|------|------|
| S1: 基石 | BG | specs + models + authoring + sandbox + CLI | ✅ 完成 |
| S2: 推演增强 | BG-D/BH | GridServerSandbox + test 报告 | 后续 |
| S3: 治理 | BH | Policy DSL + L3 对接 | 后续 |
| S4: 编排 | BH/BI | Playbook DSL + 事件触发 | 后续 |
| S5: 客户端 | BI | 5 REST API + PlatformSandbox | 后续 |
| S6: TypeScript | BI/BJ | TS SDK | 后续 |
| S7: 生态 | BJ+ | MCP Tool + Java/Go | 后续 |

## 关键代码路径

| 组件 | 路径 |
|------|------|
| SDK 设计蓝图 | `docs/design/Grid/EAASP_SDK_DESIGN.md` |
| SDK 源码 | `sdk/python/src/eaasp/` |
| JSON Schema | `sdk/specs/` |
| 示例 Skill | `sdk/examples/hr-onboarding/` |
| Proto 定义 | `proto/eaasp/runtime/v1/runtime.proto` |
| L2 Skill Registry | `tools/eaasp-skill-registry/` |
| grid-runtime gRPC | `crates/grid-runtime/src/service.rs` |
| claude-code-runtime | `lang/claude-code-runtime-python/` |
| certifier blindbox | `tools/eaasp-certifier/src/blindbox.rs` |
| EAASP 路线图 | `docs/design/Grid/EAASP_ROADMAP.md` |

## 建议下一步

1. 设计 Phase BH — L3 治理层 + L4 基础（对应规范 §4, §3）
2. 或处理 BG Deferred 项（BG-D1~D10）中已具备前置条件的
3. 参考 `EAASP_ROADMAP.md` Phase BH 部分了解范围
