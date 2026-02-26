# Octo Sandbox 开发工作日志

## 2026-02-26 — Phase 1 核心引擎实现

### 会话概要

完成 Phase 1 全部 10 个步骤的编码实施。从零搭建 Cargo workspace + React 前端，实现完整的 AI 对话引擎（Provider → AgentLoop → WebSocket → Chat UI）。

### 技术变更

#### 后端 (Rust, 32 个源文件)

**octo-types (8 文件)** — 共享类型定义
- `crates/octo-types/src/id.rs` — UserId, SessionId, SandboxId newtype (宏生成)
- `crates/octo-types/src/message.rs` — MessageRole, ChatMessage, ContentBlock (Text/ToolUse/ToolResult)
- `crates/octo-types/src/provider.rs` — CompletionRequest, CompletionResponse, StreamEvent, TokenUsage, StopReason
- `crates/octo-types/src/tool.rs` — ToolSource, ToolSpec, ToolResult, ToolContext
- `crates/octo-types/src/memory.rs` — MemoryBlock, MemoryBlockKind, TokenBudget
- `crates/octo-types/src/sandbox.rs` — RuntimeType, SandboxConfig, ExecResult
- `crates/octo-types/src/error.rs` — OctoError enum (thiserror)
- `crates/octo-types/src/lib.rs` — 模块声明 + pub re-exports

**octo-engine (12 文件)** — 核心引擎
- `providers/traits.rs` — Provider trait (complete + stream)
- `providers/anthropic.rs` — AnthropicProvider (完整 SSE stream 解析: message_start, content_block_delta, tool_use 积累, message_stop)
- `providers/mod.rs` — create_provider() 工厂
- `tools/traits.rs` — Tool trait (name/desc/params/execute/spec)
- `tools/bash.rs` — BashTool (tokio::process::Command, 30s 超时, env 清理)
- `tools/file_read.rs` — FileReadTool (1MB 限制, 行号显示, offset/limit)
- `tools/mod.rs` — ToolRegistry + default_tools()
- `agent/loop_.rs` — AgentLoop (最大 10 轮, 流式事件, 工具调用循环)
- `agent/context.rs` — ContextBuilder (系统提示词组装, token 估算)
- `memory/traits.rs` — WorkingMemory trait
- `memory/working.rs` — InMemoryWorkingMemory (默认 4 blocks)
- `memory/injector.rs` — ContextInjector (blocks → XML tags)
- `memory/budget.rs` — TokenBudgetManager (chars/4 估算)

**octo-sandbox (3 文件)** — 沙箱运行时
- `traits.rs` — RuntimeAdapter trait
- `native.rs` — NativeRuntime (进程执行 + 超时 + env 清理)
- `lib.rs` — 模块声明

**octo-server (5 文件)** — HTTP/WebSocket 服务
- `main.rs` — Axum 启动, dotenvy, tracing, graceful shutdown
- `router.rs` — build_router() (/api/health + /ws, CORS, TraceLayer)
- `ws.rs` — WebSocket handler (消息解析, AgentLoop 启动, broadcast 事件转发)
- `session.rs` — InMemorySessionStore (DashMap), SessionStore trait
- `state.rs` — AppState (Provider + ToolRegistry + WorkingMemory + AgentLoop)

#### 前端 (TypeScript/React, 16 个源文件)

**基础设施**
- `web/package.json` — React 19 + Jotai 2.16 + Tailwind CSS 4 + Vite 6
- `web/vite.config.ts` — Vite 配置 + API proxy → localhost:3001
- `web/tsconfig.json` — TypeScript 严格模式 + path aliases
- `web/src/main.tsx` — React root + Jotai Provider
- `web/src/globals.css` — Tailwind CSS 基础样式 + CSS 变量主题
- `web/src/lib/utils.ts` — cn() (clsx + tailwind-merge)

**状态管理**
- `web/src/atoms/session.ts` — sessionIdAtom, messagesAtom, isStreamingAtom, streamingTextAtom, toolExecutionsAtom
- `web/src/atoms/ui.ts` — activeTabAtom, sidebarOpenAtom

**WebSocket**
- `web/src/ws/manager.ts` — WsManager 单例 (connect/disconnect/send, 指数退避重连)
- `web/src/ws/types.ts` — ClientMessage, ServerMessage TypeScript 类型
- `web/src/ws/events.ts` — handleWsEvent() 事件分发到 Jotai atoms

**UI 组件**
- `web/src/components/layout/AppLayout.tsx` — NavRail + TabBar + Main
- `web/src/components/layout/NavRail.tsx` — 左侧栏 (Phase 1 占位)
- `web/src/components/layout/TabBar.tsx` — 顶部标签栏
- `web/src/components/chat/MessageList.tsx` — 滚动消息列表 + 自动滚底
- `web/src/components/chat/MessageBubble.tsx` — 单条消息 (用户右蓝/助手左灰)
- `web/src/components/chat/ChatInput.tsx` — Textarea + 发送按钮
- `web/src/components/chat/StreamingDisplay.tsx` — 流式文本 + 工具执行状态

#### 构建配置
- `Cargo.toml` — workspace 定义 + profile 优化 (split-debuginfo, codegen-units=256)
- `.cargo/config.toml` — 编译优化 (jobs=8, dead_strip)
- `Makefile` — dev/build/check/test/fmt/lint 命令
- `.env.example` — ANTHROPIC_API_KEY 模板

### 构建验证结果

| 检查项 | 状态 | 详情 |
|--------|------|------|
| `cargo check --workspace` | ✅ 通过 | 0.25s, 仅 2 个 dead_code warnings |
| `cargo build` | ✅ 通过 | 21s, 13MB binary |
| `npx tsc --noEmit` | ✅ 通过 | 0 errors |
| `npx vite build` | ✅ 通过 | 874ms, 241KB JS bundle |

### 遗留问题

1. **sccache 不可用** — 系统内存压力大时 sccache 进程被 OOM kill。已注释掉配置，待系统空闲时启用。
2. **Dead code warnings** — `AppState.provider/tools/memory` 字段仅被 `ws.rs` 通过 `agent_loop` 间接使用，但 compiler 无法追踪。无需处理。
3. **Cancel 功能未实现** — WebSocket cancel 消息的处理需要 CancellationToken，留待后续实现。

### 下一步

- **运行时验证**: 需要 `ANTHROPIC_API_KEY` 环境变量才能启动 `cargo run -p octo-server` 进行端到端测试
- **前端开发服务器**: `cd web && npm run dev` 启动 Vite dev server
- **端到端测试**: 打开浏览器连接 WebSocket, 发送消息验证流式响应 + 工具调用
