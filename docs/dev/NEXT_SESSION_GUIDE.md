# OpenFang 架构研究阶段指南

## 当前状态

- **Phase**: OpenFang 架构研究已完成
- **分支**: `octo-workbench`
- **研究产出**: `docs/plans/2026-02-27-openfang-architecture-research.md`

## 阶段完成状态

✅ 全部研究任务已完成

- [x] 1. 研究 openfang-kernel: 核心编排引擎
- [x] 2. 研究 openfang-runtime: Agent Loop + MCP 集成
- [x] 3. 研究 openfang-memory: 三层记忆系统
- [x] 4. 研究 openfang-api: Axum 140+ 端点设计
- [ ] 5. 研究 openfang-channels: 40 消息通道适配器 (可选)
- [x] 6. 对比当前 octo-sandbox 架构
- [x] 7. 制定引入计划

## 研究结论

### 高优先级引入模块

| 模块 | 价值 | 实施难度 |
|-----|------|---------|
| MCP Client 完善 | ⭐⭐⭐⭐⭐ | 中 |
| EventBus 事件驱动 | ⭐⭐⭐⭐ | 低 |
| 配置管理 | ⭐⭐⭐⭐ | 低 |

### 建议下一步

1. **启动 Phase 3**: 架构升级
   - Task 1: MCP Client 完善 (参考 OpenFang stdio + SSE)
   - Task 2: EventBus 事件系统
   - Task 3: AgentRegistry 多代理

2. **或继续 Phase 2.4**: v1.0 Release
   - 完善 MCP Workbench 运行时
   - 端到端测试

## 关键代码参考

- **MCP Client**: `github.com/openfang/crates/openfang-runtime/src/mcp.rs`
- **EventBus**: `github.com/openfang/crates/openfang-kernel/src/event_bus.rs`
- **AgentRegistry**: `github.com/openfang/crates/openfang-kernel/src/registry.rs`
- **MemorySubstrate**: `github.com/openfang/crates/openfang-memory/src/substrate.rs`
