# Grid Platform 下一会话指南

**最后更新**: 2026-04-07 20:00 GMT+8
**当前分支**: `Grid`
**当前状态**: Phase BH-MVP 设计完成 — 准备 W1 执行

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
- [x] Phase BD — grid-runtime EAASP L1 (6/6, 37 tests)
- [x] Phase BE — EAASP 协议层 + claude-code-runtime (6/6, 93 tests)
- [x] Phase BF — L2 统一资产层 + L1 抽象机制 (7/7, 30 new tests)
- [x] Phase BG — Enterprise SDK 基石 (6/6, 107 new tests)
- [ ] **Phase BH-MVP** — E2E 业务智能体全流程验证 (0/7, ~58 tests planned)

## Phase BH-MVP 设计总结

**目标**: 用 HR 入职智能体走通 L4→L3→L2→L1 完整管道

### 架构要点

- **L3 治理服务** (`eaasp-governance`, :8083): Python FastAPI, 5 API 契约 (§8)
- **L4 会话管理器** (`eaasp-session-manager`, :8084): Python FastAPI, 四平面骨架 (§3)
- **策略 DSL**: Kubernetes 风格 YAML → managed_hooks_json 编译
- **E2E 双模式**: mock-llm (CI) + live-llm (需 API Key)
- **10 个设计决策** (KD-BH1~10): 详见 `EAASP_MVP_E2E_DESIGN.md`

### 7 个 Wave

| Wave | 内容 | Tests | 依赖 |
|------|------|-------|------|
| W1 | 策略 DSL + 编译器 | 8 | 无 |
| W2 | L3 治理服务 5 API 契约 | 12 | W1 |
| W3 | L4 四平面骨架 | 10 | W2 |
| W4 | SDK `eaasp run` + E2E 脚本 | 8 | W1-W3 |
| W5 | E2E 集成测试双模式 | 14 | W1-W4 |
| W6 | HR 示例完善 + 审计 Hook | 6 | W1-W5 |
| W7 | Makefile + 文档 | 0 | W1-W6 |

### 下一步: 执行 W1

W1 产出:
- `tools/eaasp-governance/` Python 包骨架
- PolicyBundle / PolicyRule Pydantic V2 模型
- 策略编译器: YAML DSL → managed_hooks_json
- 层级合并器 (deny-always-wins)
- HR 策略示例: `enterprise.yaml` + `bu_hr.yaml`
- 8 tests

## 关键代码路径

| 组件 | 路径 |
|------|------|
| **设计文档** | `docs/design/Grid/EAASP_MVP_E2E_DESIGN.md` |
| **实施计划** | `docs/plans/2026-04-07-phase-bh-mvp-e2e.md` |
| SDK 源码 | `sdk/python/src/eaasp/` |
| HR 示例 | `sdk/examples/hr-onboarding/` |
| L2 Skill Registry | `tools/eaasp-skill-registry/` |
| L1 grid-runtime | `crates/grid-runtime/src/` |
| L1 claude-code-runtime | `lang/claude-code-runtime-python/` |
| HookBridge | `crates/grid-hook-bridge/src/` |
| Certifier | `tools/eaasp-certifier/src/` |
| Proto | `proto/eaasp/` |
| managed_hooks_json 消费者 (Rust) | `crates/grid-hook-bridge/src/in_process.rs` |
| managed_hooks_json 消费者 (Python) | `lang/claude-code-runtime-python/src/claude_code_runtime/hook_executor.py` |
| EAASP 规范 | `docs/design/Grid/EAASP_-_企业自主智能体支撑平台设计规范_v1.7_.pdf` |
| EAASP 路线图 | `docs/design/Grid/EAASP_ROADMAP.md` |

## 建议下一步

1. **执行 W1**: 策略 DSL + 编译器 + HR 策略示例 (8 tests)
2. 参考 `EAASP_MVP_E2E_DESIGN.md` §三 策略 DSL 规范
3. 编译器输出需兼容 `hook_executor.py` 的 `load_rules()` 格式
