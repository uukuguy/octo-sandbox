# Phase 2.5.3 - 用户隔离 (User Isolation) 实现计划

## 概述

Phase 2.5.3 实现用户资源隔离，确保不同用户的资源（会话、记忆、工具执行等）相互隔离。

## 当前状态分析

### 已实现 user_id 的表
- ✅ `memory_blocks` - 有 user_id 字段
- ✅ `sessions` - 有 user_id 字段
- ✅ `memories` - 有 user_id 字段

### 缺少 user_id 的表
- ❌ `session_messages` - 需要添加 user_id
- ❌ `tool_executions` - 需要添加 user_id
- ❌ `mcp_servers` - 需要添加 user_id
- ❌ `mcp_executions` - 需要添加 user_id
- ❌ `mcp_logs` - 需要添加 user_id

### API 现状
- ❌ `GET /api/sessions` - 返回所有用户的会话，无过滤
- ❌ `GET /api/sessions/:id` - 返回任意会话，无验证
- ❌ `GET /api/memories` - 硬编码 DEFAULT_USER_ID = "default"
- ❌ `POST /api/memories` - 硬编码 DEFAULT_USER_ID
- ❌ `GET /api/mcp/servers` - 返回所有用户的 MCP 服务器
- ❌ 其他 MCP 端点 - 无用户隔离

## 任务列表

### Task 1: 数据库迁移 - 添加 user_id 字段
- [ ] **1.1** 在 `session_messages` 表添加 `user_id` 字段
- [ ] **1.2** 在 `tool_executions` 表添加 `user_id` 字段
- [ ] **1.3** 在 `mcp_servers` 表添加 `user_id` 字段
- [ ] **1.4** 在 `mcp_executions` 表添加 `user_id` 字段
- [ ] **1.5** 在 `mcp_logs` 表添加 `user_id` 字段

### Task 2: 创建用户上下文提取器
- [ ] **2.1** 创建 `user_context.rs` 模块，从请求提取用户上下文
- [ ] **2.2** 定义 `UserContext` 结构体（user_id, sandbox_id）

### Task 3: 修改会话 API 用户隔离
- [ ] **3.1** `GET /api/sessions` - 添加 user_id 过滤
- [ ] **3.2** `GET /api/sessions/:id` - 验证会话属于当前用户

### Task 4: 修改记忆 API 用户隔离
- [ ] **4.1** `GET /api/memories` - 从用户上下文获取 user_id
- [ ] **4.2** `POST /api/memories` - 从用户上下文获取 user_id
- [ ] **4.3** `GET /api/memories/working` - 从用户上下文获取 user_id
- [ ] **4.4** 删除硬编码的 DEFAULT_USER_ID 常量

### Task 5: 修改 MCP 服务器 API 用户隔离
- [ ] **5.1** `GET /api/mcp/servers` - 添加 user_id 过滤
- [ ] **5.2** `POST /api/mcp/servers` - 添加 user_id
- [ ] **5.3** `GET /api/mcp/servers/:id` - 验证属于当前用户
- [ ] **5.4** `PUT /api/mcp/servers/:id` - 验证属于当前用户
- [ ] **5.5** `DELETE /api/mcp/servers/:id` - 验证属于当前用户
- [ ] **5.6** MCP 启动/停止/状态 - 验证属于当前用户

### Task 6: 修改工具执行 API 用户隔离
- [ ] **6.1** `GET /api/executions` - 添加 user_id 过滤

### Task 7: WebSocket 用户隔离
- [ ] **7.1** WebSocket 连接携带用户上下文
- [ ] **7.2** Agent loop 使用正确的 user_id

## 验收标准

| 条件 | 描述 |
|------|------|
| 会话隔离 | 用户只能看到自己的会话 |
| 记忆隔离 | 用户只能看到自己的记忆 |
| MCP 隔离 | 用户只能管理自己的 MCP 服务器 |
| 执行隔离 | 用户只能看到自己的工具执行记录 |
| 数据完整性 | 新创建的记录包含正确的 user_id |

## 技术要点

### 用户标识来源
1. API Key 认证模式下：从 API Key 提取用户 ID
2. 无认证模式下：使用默认用户

### 用户上下文传递
- 通过 `Extension<UserContext>` 在处理器间传递
- 从 auth middleware 注入

## 依赖项
- Phase 2.5.2 认证系统（API Key 验证中间件）

## 预估工作量
- 数据库迁移: 1 小时
- 用户上下文模块: 2 小时
- API 修改: 3 小时
- 测试验证: 2 小时
- **总计: ~8 小时**
