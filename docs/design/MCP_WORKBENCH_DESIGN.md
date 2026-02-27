# MCP Workbench 设计方案

**项目**: octo-sandbox
**阶段**: Phase 2.3 - 调试面板完善
**创建日期**: 2026-02-27

---

## 1. 整体架构

MCP Workbench 是一个完整 MCP 服务器管理和调试前端，包含三个子模块：

1. **Server Manager** - MCP servers 列表、添加、配置、启停
2. **Tool Invoker** - 手动调用任意 server 的工具
3. **Log Viewer** - 分级显示 MCP 通信日志

### 1.1 后端架构

```
MCP Workbench (Frontend)
       │
       ▼ WebSocket / REST
┌──────────────────┐
│  octo-server    │
│                  │
│  McpManager     │ ◄── 管理所有 MCP server 生命周期
│  ├─ discovery   │ ◄── .mcp.json 扫描
│  ├─ registry     │ ◄── server 配置存储
│  └─ session     │ ◄── 运行时 session
│                  │
│  LogRecorder    │ ◄── 分级日志记录
│  └─ SQLite      │ ◄── 持久化存储
└──────────────────┘
       │
       ▼ stdio
┌──────────────────┐
│  MCP Server      │
│  (external)     │
└──────────────────┘
```

### 1.2 数据流

- 用户在前端配置 server → REST API 保存到 registry
- 用户点击启动 → WebSocket 通知后端 fork 子进程
- 工具调用 → 通过 stdio 发送 JSON-RPC 请求
- 日志 → 分级缓存 + 持久化到 SQLite

---

## 2. Server Manager 详细设计

### 2.1 Server 配置结构

```typescript
interface McpServerConfig {
  id: string;                    // UUID
  name: string;                  // 显示名称
  source: 'manual' | 'mcp_json' | 'template';
  command: string;               // 执行命令，如 "npx", "uvx"
  args: string[];               // 参数，如 ["-y", "@anthropic/mcp-server-filesystem", "/path"]
  env?: Record<string, string>;  // 环境变量
  enabled: boolean;              // 是否启用
  created_at: string;
  updated_at: string;
}
```

### 2.2 三种添加方式

| 方式 | UI 操作 | 后端逻辑 |
|------|---------|----------|
| **扫描发现** | 点击 "Scan .mcp.json" 按钮 | 扫描项目目录 + 用户 HOME 目录，解析 JSON 数组 |
| **手动配置** | 填写 form: name/command/args/env | 验证 command 存在，测试连接 |
| **模板选择** | 选择模板（filesystem, memory, git 等） | 预填 command + args，用户可修改 |

**常用模板列表**:
- filesystem - 文件系统访问
- memory - 内存存储
- git - Git 操作
- http - HTTP 请求
- slack - Slack 集成
- postgres - PostgreSQL 数据库
- puppeteer - 浏览器自动化
- custom - 自定义模板

### 2.3 运行时状态

```typescript
interface McpServerRuntime {
  config_id: string;
  status: 'stopped' | 'starting' | 'running' | 'error';
  pid?: number;
  started_at?: string;
  error_message?: string;
  tools: McpTool[];  // 发现的所有工具
}
```

### 2.4 Server 列表 UI

```
┌─────────────────────────────────────────────────────────┐
│  MCP Workbench                              [Scan] [Add]│
├─────────────────────────────────────────────────────────┤
│ ┌─────────────────────────────────────────────────────┐ │
│ │ 🟢 filesystem    [Stop]  [Call]   5 tools    📋   │ │
│ │    npx -y @anthropic/mcp-server-filesystem /data   │ │
│ └─────────────────────────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ 🔴 postgres     [Start] [Call]   0 tools    📋   │ │
│ │    docker run -it --rm mcp/postgres ...            │ │
│ └─────────────────────────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ ⚪ slack        [Start] [Call]   0 tools    📋   │ │
│ │    (disabled)                                      │ │
│ └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

- **状态指示器**: 🟢运行中 🔴错误 ⚪已停止 ⏳启动中
- **操作按钮**: Start/Stop, Call Tool, 查看日志

---

## 3. Tool Invoker 详细设计

### 3.1 工具调用 UI

```
┌─────────────────────────────────────────────────────────┐
│  Tool Invoker                                              │
├─────────────────────────────────────────────────────────┤
│  Server: [filesystem ▼]  Tool: [read_file ▼]             │
├─────────────────────────────────────────────────────────┤
│  Parameters (JSON):                                        │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ {                                                  │ │
│  │   "path": "/data/README.md",                      │ │
│  │   "offset": 0,                                    │ │
│  │   "limit": 1000                                   │ │
│  │ }                                                  │ │
│  └─────────────────────────────────────────────────────┘ │
│                                           [Execute]       │
├─────────────────────────────────────────────────────────┤
│  Result:                                                  │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ {                                                  │ │
│  │   "content": [                                     │ │
│  │     {"type": "text", "text": "# Project\n..."}    │ │
│  │   ]                                                │ │
│  │ }                                                  │ │
│  └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### 3.2 参数输入模式

1. **JSON 编辑器** - 直接编辑 JSON，支持语法高亮
2. **Form 模式** - 根据 tool input schema 自动生成表单字段

### 3.3 响应展示

- **Text**: 纯文本显示
- **Image**: 渲染图片（base64）
- **Resource**: 提供下载链接
- **Error**: 红色高亮显示错误信息

### 3.4 调用历史

- 每次执行记录保存到 SQLite
- 可查看历史调用参数和结果
- 支持重新执行（Replay）

---

## 4. Log Viewer 详细设计

### 4.1 日志分级

| 级别 | 颜色 | 内容 |
|------|------|------|
| **🔵 Info** | 蓝色 | 工具调用请求、响应 |
| **🟢 Debug** | 绿色 | 详细协议数据 |
| **🟡 Warn** | 黄色 | 警告信息 |
| **🔴 Error** | 红色 | 错误信息 |
| **⚪ Raw** | 灰色 | 原始 stdin/stdout 数据 |

### 4.2 日志条目结构

```typescript
interface McpLogEntry {
  id: string;
  server_id: string;
  level: 'info' | 'debug' | 'warn' | 'error' | 'raw';
  direction: 'request' | 'response' | 'system';
  timestamp: string;
  method?: string;           // JSON-RPC method
  params?: any;              // 请求参数
  result?: any;              // 响应结果
  raw_data?: string;         // 原始数据
  duration_ms?: number;      // 调用耗时
}
```

### 4.3 Log Viewer UI

```
┌─────────────────────────────────────────────────────────┐
│  Logs: filesystem     [Level: All ▼] [Clear] [Export]  │
├─────────────────────────────────────────────────────────┤
│ 🔵 [12:30:15] → tools/call                              │
│    { "name": "read_file", "arguments": {...} }        │
│ 🟢 [12:30:15] ← 200 OK (23ms)                         │
│    { "content": [{ "type": "text", ... }] }           │
│ 🔵 [12:30:20] → tools/call                             │
│    { "name": "write_file", ... }                       │
│ 🔴 [12:30:21] ← Error: File not found                 │
│    { "code": -32602, "message": "File not found" }    │
│ ⚪ [12:30:21] RAW stdout: {"jsonrpc":"2.0",...}        │
└─────────────────────────────────────────────────────────┘
```

### 4.4 日志过滤

- **按 Server 筛选**
- **按 Level 筛选**
- **按时间范围筛选**
- **搜索关键词**

---

## 5. 数据持久化

### 5.1 SQLite Schema

```sql
-- MCP Server 配置
CREATE TABLE mcp_servers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  source TEXT NOT NULL,
  command TEXT NOT NULL,
  args TEXT NOT NULL,       -- JSON array
  env TEXT,                 -- JSON object
  enabled INTEGER DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- MCP 工具调用历史
CREATE TABLE mcp_executions (
  id TEXT PRIMARY KEY,
  server_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  params TEXT NOT NULL,     -- JSON
  result TEXT,              -- JSON
  error TEXT,
  duration_ms INTEGER,
  executed_at TEXT NOT NULL,
  FOREIGN KEY (server_id) REFERENCES mcp_servers(id)
);

-- MCP 通信日志
CREATE TABLE mcp_logs (
  id TEXT PRIMARY KEY,
  server_id TEXT NOT NULL,
  level TEXT NOT NULL,
  direction TEXT NOT NULL,
  method TEXT,
  params TEXT,
  result TEXT,
  raw_data TEXT,
  duration_ms INTEGER,
  logged_at TEXT NOT NULL,
  FOREIGN KEY (server_id) REFERENCES mcp_servers(id)
);

CREATE INDEX idx_mcp_logs_server_time ON mcp_logs(server_id, logged_at);
CREATE INDEX idx_mcp_executions_server_time ON mcp_executions(server_id, executed_at);
```

### 5.2 导出功能

- **导出格式**: JSON
- **导出内容**: 可选日志/调用历史
- **导出范围**: 全量 / 时间范围 / Server 筛选

---

## 6. API 设计

### 6.1 REST Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/mcp/servers` | 获取所有 server 配置 |
| POST | `/api/mcp/servers` | 添加新 server |
| PUT | `/api/mcp/servers/:id` | 更新 server 配置 |
| DELETE | `/api/mcp/servers/:id` | 删除 server |
| POST | `/api/mcp/servers/:id/start` | 启动 server |
| POST | `/api/mcp/servers/:id/stop` | 停止 server |
| GET | `/api/mcp/servers/:id/tools` | 获取可用工具列表 |
| POST | `/api/mcp/servers/:id/call` | 调用工具 |
| GET | `/api/mcp/logs` | 获取日志（支持分页/筛选） |
| GET | `/api/mcp/logs/export` | 导出日志 |
| GET | `/api/mcp/executions` | 获取调用历史 |

### 6.2 WebSocket Events

| Event | Direction | Description |
|-------|-----------|-------------|
| `mcp:server:status` | Server→Client | Server 状态变化 |
| `mcp:log` | Server→Client | 实时日志推送 |
| `mcp:tool:result` | Server→Client | 工具调用结果 |

---

## 7. 实施计划

| 任务 | 描述 | 预估工作量 |
|------|------|------------|
| 1 | 后端: McpServerConfig CRUD API | 2h |
| 2 | 后端: Server 启动/停止逻辑 | 3h |
| 3 | 后端: 工具发现 (tools/list) | 2h |
| 4 | 后端: 工具调用 (tools/call) | 2h |
| 5 | 后端: 分级日志记录 + SQLite | 3h |
| 6 | 前端: Server 列表页面 | 2h |
| 7 | 前端: 添加/编辑 Server Dialog | 2h |
| 8 | 前端: Tool Invoker 页面 | 2h |
| 9 | 前端: Log Viewer 组件 | 2h |
| 10 | 前端: 模板列表 + Scan 功能 | 1h |
| 11 | 前后端联调 + 测试 | 3h |

**总预估: 约 22 小时**

---

## 8. 已知限制

1. 当前仅支持 stdio 传输方式
2. 模板列表为预设，可能需要更新
3. 日志轮转策略待定（防止 SQLite 过大）
