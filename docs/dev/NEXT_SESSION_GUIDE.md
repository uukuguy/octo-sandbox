# octo-workbench 下次会话指南

## 当前状态

- **Phase 2.2 (记忆系统完整)** ✅ 已完成
- **分支**: `octo-workbench`
- **提交**: `56cda54 feat(memory): add memory_recall, memory_forget tools and Memory Explorer UI`

## 完成清单

- [x] Phase 1: 核心引擎 (AI 对话 + 工具执行 + WebSocket)
- [x] Phase 2.1: 调试面板基础 (Timeline + JsonViewer + Tool Execution)
- [x] Phase 2.2: 记忆系统完整 (5 memory tools + Memory Explorer UI)
- [ ] Phase 2.3: 调试面板完善
- [ ] Phase 2.4: v1.0 Release

## 下一步优先级

1. **Phase 2.3 调试面板完善**
   - MCP Workbench: Server 管理 + 手动调用 + 日志流
   - Skill Studio: 编辑 + 测试 + 热重载
   - Network Interceptor: 请求/响应拦截
   - Context Viewer: 实时上下文窗口

2. **运行时验证**
   - 需要 `ANTHROPIC_API_KEY` 环境变量
   - 启动服务器: `cargo run -p octo-server`
   - 启动前端: `cd web && pnpm dev`

## 关键代码路径

- **Memory Tools**: `crates/octo-engine/src/tools/memory_*.rs`
- **Memory Store**: `crates/octo-engine/src/memory/`
- **Memory API**: `crates/octo-server/src/router.rs` (搜索 "memories")
- **Memory Page**: `web/src/pages/Memory.tsx`

## 设计文档

- 架构设计: `docs/design/ARCHITECTURE_DESIGN.md`
- 实施规划: `docs/plans/2026-02-27-octo-workbench-v1-implementation.md`
- 检查点: `docs/plans/.checkpoint.json`

## 记忆索引

- MEMORY_INDEX.md: `docs/dev/MEMORY_INDEX.md`
- MCP memory: `claude-mem` 项目 "octo-sandbox"

## 注意事项

1. Phase 2.3 涉及多个前端页面组件开发
2. MCP Workbench 需要连接外部 MCP 服务器
3. 运行时验证需要有效的 API key
