# ADR：MCP INTEGRATION 架构决策记录

**项目**：octo-sandbox
**版本**：v1.0
**日期**：2026-03-07
**状态**：已完成

---

## 目录

- [ADR-013：MCP Manager 生命周期管理](#adr-013mcp-manager-生命周期管理)
- [ADR-017：MCP Client 多传输协议支持](#adr-017mcp-client-多传输协议支持)
- [ADR-018：MCP Tool Bridge 统一工具接口](#adr-018mcp-tool-bridge-统一工具接口)

---

## ADR-013：MCP Manager 生命周期管理

### 状态

**已完成** — 2026-03-07

### 上下文

MCP (Model Context Protocol) 服务器需要在运行时动态管理：
- 启动和停止 MCP 服务器进程
- 进程健康监控
- 日志收集
- 工具发现和同步

### 决策

实现 `McpManager` 负责 MCP 服务器全生命周期：

```rust
pub struct McpManager {
    processes: HashMap<SandboxId, Child>,
    storage: Arc<McpStorage>,
}

impl McpManager {
    pub async fn start_server(&self, config: &McpServerConfig) -> Result<()>;
    pub async fn stop_server(&self, sandbox_id: &SandboxId) -> Result<()>;
    pub async fn get_tools(&self, sandbox_id: &SandboxId) -> Result<Vec<Tool>>;
}
```

### 涉及文件

| 文件 | 变更 |
|------|------|
| `src/mcp/manager.rs` | McpManager 主实现 |
| `src/mcp/storage.rs` | McpStorage SQLite 持久化 |

---

## ADR-017：MCP Client 多传输协议支持

### 状态

**已完成** — 2026-03-07

### 上下文

MCP 支持多种传输协议：stdio（本地进程）和 SSE（远程 HTTP）。

### 决策

实现统一的 `McpClient` 接口支持多种传输：

```rust
pub enum McpClient {
    Stdio(McpStdioClient),
    Sse(McpSseClient),
}

pub trait McpTransport: Send {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;
    async fn subscribe(&self, handler: Box<dyn NotificationHandler>) -> Result<()>;
}
```

### 涉及文件

| 文件 | 变更 |
|------|------|
| `src/mcp/stdio.rs` | stdio 传输实现 |
| `src/mcp/sse.rs` | SSE 传输实现 |
| `src/mcp/client.rs` | McpClient 统一接口 |

---

## ADR-018：MCP Tool Bridge 统一工具接口

### 状态

**已完成** — 2026-03-07

### 上下文

MCP 服务器暴露的工具需要统一接入系统工具注册表。

### 决策

实现 `McpToolBridge` 将 MCP 工具转换为系统工具：

```rust
pub struct McpToolBridge {
    clients: HashMap<SandboxId, McpClient>,
}

impl ToolBridge for McpToolBridge {
    fn get_tools(&self, sandbox_id: &SandboxId) -> Result<Vec<Tool>>;
}
```

### 涉及文件

| 文件 | 变更 |
|------|------|
| `src/mcp/bridge.rs` | McpToolBridge 实现 |
