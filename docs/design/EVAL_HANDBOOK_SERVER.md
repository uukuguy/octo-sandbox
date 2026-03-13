# octo-server 评估手册

> **适用版本**: octo-server (Axum HTTP + WebSocket)
> **基础 URL**: `http://127.0.0.1:3001`
> **WebSocket URL**: `ws://127.0.0.1:3001/ws`

---

## 目录

- [概述](#概述)
- [环境准备](#环境准备)
- [S1: Agent 生命周期管理 (Easy)](#s1-agent-生命周期管理)
- [S2: WebSocket 流式对话 (Medium)](#s2-websocket-流式对话)
- [S3: Session 持久化与恢复 (Medium)](#s3-session-持久化与恢复)
- [S4: Token Budget 监控 (Medium)](#s4-token-budget-监控)
- [S5: Provider Chain 故障模拟 (Hard)](#s5-provider-chain-故障模拟)
- [S6: 审计日志完整性 (Easy)](#s6-审计日志完整性)

---

## 概述

本手册定义 6 项 octo-server 端评估任务，覆盖 REST API、WebSocket 流式通信、持久化、可观测性、故障转移和审计日志等核心维度。每项任务包含精确的执行步骤、预期输出和 Pass/Fail 判定标准，可由人工或自动化脚本执行。

### 评估维度矩阵

| 任务 | 难度 | REST API | WebSocket | 持久化 | 可观测性 | 容错 | 安全 |
|------|------|----------|-----------|--------|----------|------|------|
| S1   | Easy   | **主** | - | - | - | - | - |
| S2   | Medium | - | **主** | - | **辅** | - | - |
| S3   | Medium | **主** | **辅** | **主** | - | - | - |
| S4   | Medium | **主** | **辅** | - | **主** | - | - |
| S5   | Hard   | **主** | - | - | **主** | **主** | - |
| S6   | Easy   | **主** | - | **辅** | **主** | - | **辅** |

---

## 环境准备

### 前置条件

1. **Rust 工具链**: 1.75+（`rustup show` 确认）
2. **系统工具**:
   - `curl` (HTTP 请求)
   - `jq` (JSON 解析)
   - `websocat` (WebSocket 客户端，`brew install websocat` 或 `cargo install websocat`)
3. **API Key**: 至少配置一个有效的 LLM Provider API Key

### 环境配置

```bash
# 1. 进入项目根目录
cd /Users/sujiangwen/sandbox/LLM/speechless.ai/Autonomous-Agents/octo-sandbox

# 2. 复制并编辑环境变量（如尚未配置）
cp .env.example .env
# 编辑 .env，填入 ANTHROPIC_API_KEY 或 OPENAI_API_KEY

# 3. 启动 Server
make server
# 或
cargo run -p octo-server
```

### 健康检查（验证 Server 启动成功）

```bash
curl -s http://127.0.0.1:3001/api/health | jq .
```

**预期输出**:

```json
{
  "status": "ok",
  "uptime_secs": 5,
  "provider": "anthropic",
  "mcp_servers": [],
  "version": "0.1.0"
}
```

如果返回 `Connection refused`，请确认 Server 已启动且端口 3001 未被占用。

### 通用变量

本手册中所有命令使用以下变量约定：

```bash
BASE_URL="http://127.0.0.1:3001"
WS_URL="ws://127.0.0.1:3001/ws"
```

---

## S1: Agent 生命周期管理

| 属性 | 值 |
|------|-----|
| **任务 ID** | S1 |
| **名称** | Agent 生命周期管理 |
| **难度** | Easy |
| **评估维度** | REST API 正确性、状态机转换 |
| **预计耗时** | 5 分钟 |

### 前置条件

- Server 已启动并通过健康检查
- 无需 LLM API Key（本任务仅测试 Agent 注册和状态管理，不触发 LLM 调用）

### 评估目标

验证 Agent 从创建到销毁的完整生命周期：`Created -> Running -> Paused -> Running -> Stopped`，每一步状态转换均正确返回。

### 执行步骤

#### 步骤 1: 注册 Agent

```bash
curl -s -X POST ${BASE_URL}/api/v1/agents \
  -H "Content-Type: application/json" \
  -d '{
    "name": "eval-agent-s1",
    "description": "Evaluation test agent for lifecycle management",
    "system_prompt": "You are a test agent.",
    "provider": "anthropic",
    "model": "claude-sonnet-4-20250514"
  }' | jq .
```

**预期**: 返回 HTTP 201，响应体包含 `id`、`name`、`state` 字段。

```json
{
  "id": "<agent-id>",
  "name": "eval-agent-s1",
  "state": "Created",
  ...
}
```

**保存返回的 Agent ID**:

```bash
AGENT_ID=$(curl -s -X POST ${BASE_URL}/api/v1/agents \
  -H "Content-Type: application/json" \
  -d '{
    "name": "eval-agent-s1",
    "description": "Evaluation test agent",
    "system_prompt": "You are a test agent.",
    "provider": "anthropic",
    "model": "claude-sonnet-4-20250514"
  }' | jq -r '.id')
echo "Agent ID: ${AGENT_ID}"
```

#### 步骤 2: 查看 Agent 列表

```bash
curl -s ${BASE_URL}/api/v1/agents | jq .
```

**预期**: 返回数组，包含刚注册的 Agent，`state` 为 `"Created"`。

#### 步骤 3: 启动 Agent

```bash
curl -s -X POST ${BASE_URL}/api/v1/agents/${AGENT_ID}/start | jq .
```

**预期**: HTTP 200，Agent 状态变为 `"Running"`。

#### 步骤 4: 查看单个 Agent 状态

```bash
curl -s ${BASE_URL}/api/v1/agents/${AGENT_ID} | jq '.state'
```

**预期**: 返回 `"Running"`。

#### 步骤 5: 暂停 Agent

```bash
curl -s -X POST ${BASE_URL}/api/v1/agents/${AGENT_ID}/pause | jq .
```

**预期**: HTTP 200，状态变为 `"Paused"`。

#### 步骤 6: 恢复 Agent

```bash
curl -s -X POST ${BASE_URL}/api/v1/agents/${AGENT_ID}/resume | jq .
```

**预期**: HTTP 200，状态恢复为 `"Running"`。

#### 步骤 7: 停止 Agent

```bash
curl -s -X POST ${BASE_URL}/api/v1/agents/${AGENT_ID}/stop | jq .
```

**预期**: HTTP 200，状态变为 `"Stopped"`。

#### 步骤 8: 验证非法转换

```bash
# 对已停止的 Agent 尝试暂停（非法转换）
curl -s -o /dev/null -w "%{http_code}" -X POST ${BASE_URL}/api/v1/agents/${AGENT_ID}/pause
```

**预期**: HTTP 409 (Conflict)，说明状态机拒绝了非法转换。

#### 步骤 9: 删除 Agent

```bash
curl -s -o /dev/null -w "%{http_code}" -X DELETE ${BASE_URL}/api/v1/agents/${AGENT_ID}
```

**预期**: HTTP 200 或 204。

#### 步骤 10: 验证删除

```bash
curl -s -o /dev/null -w "%{http_code}" ${BASE_URL}/api/v1/agents/${AGENT_ID}
```

**预期**: HTTP 404。

### 评判标准

| 条件 | Pass | Fail |
|------|------|------|
| 注册返回 201 + 有效 Agent ID | 必须 | ID 为空或非 201 |
| 状态转换序列 Created->Running->Paused->Running->Stopped | 必须 | 任何一步状态不匹配 |
| 非法转换返回 409 | 必须 | 返回 200 或执行了非法转换 |
| 删除后查询返回 404 | 必须 | 仍能查询到 |

### 故障排除

| 现象 | 可能原因 | 解决方案 |
|------|----------|----------|
| 注册返回 422 | 请求体字段缺失或格式错误 | 检查 JSON 字段名，确认 `name` 和 `system_prompt` 存在 |
| 启动返回 500 | Provider 未配置 | 检查 `.env` 中的 API Key |
| 非法转换返回 200 | 状态机逻辑缺陷 | 属于 Bug，需要检查 `AgentCatalog` 实现 |

---

## S2: WebSocket 流式对话

| 属性 | 值 |
|------|-----|
| **任务 ID** | S2 |
| **名称** | WebSocket 流式对话 |
| **难度** | Medium |
| **评估维度** | WebSocket 协议、事件流完整性、流式输出 |
| **预计耗时** | 10 分钟 |

### 前置条件

- Server 已启动并通过健康检查
- 已配置有效的 LLM Provider API Key（`ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`）
- 安装 `websocat`（`brew install websocat`）

### 评估目标

验证 WebSocket 连接建立、消息发送、事件流接收的完整链路。核心验证事件序列：`session_created -> text_delta(1+) -> text_complete -> done`。

### 执行步骤

#### 步骤 1: 建立 WebSocket 连接并发送消息

```bash
echo '{"type":"send_message","content":"Hello, please respond with exactly: EVAL_OK"}' \
  | websocat ${WS_URL}
```

**预期**: 收到多行 JSON 消息，按时间顺序包含以下事件类型。

#### 步骤 2: 验证事件流

使用脚本捕获完整事件流并验证：

```bash
# 发送消息并收集所有事件（10 秒超时）
echo '{"type":"send_message","content":"Say hello in one word."}' \
  | timeout 30 websocat ${WS_URL} \
  | tee /tmp/ws_events.jsonl
```

逐行检查事件类型：

```bash
# 提取事件类型序列
cat /tmp/ws_events.jsonl | jq -r '.type' 2>/dev/null
```

**预期输出序列**（至少包含以下类型，顺序如下）:

```
session_created
text_delta
...（可能有多个 text_delta）
text_complete
done
```

#### 步骤 3: 验证 session_created 事件

```bash
head -1 /tmp/ws_events.jsonl | jq .
```

**预期**:

```json
{
  "type": "session_created",
  "session_id": "<non-empty-string>"
}
```

#### 步骤 4: 验证 text_delta 事件

```bash
cat /tmp/ws_events.jsonl | jq 'select(.type == "text_delta")' | head -3
```

**预期**: 每个 text_delta 包含 `session_id` 和非空 `text` 字段。

#### 步骤 5: 验证 text_complete 事件

```bash
cat /tmp/ws_events.jsonl | jq 'select(.type == "text_complete")'
```

**预期**: 包含完整回复文本的 `text` 字段。

#### 步骤 6: 验证 done 事件

```bash
tail -1 /tmp/ws_events.jsonl | jq .
```

**预期**:

```json
{
  "type": "done",
  "session_id": "<same-session-id>"
}
```

#### 步骤 7: 验证 Cancel 功能

```bash
# 发送消息后立即发送 cancel
(echo '{"type":"send_message","content":"Write a very long essay about the history of computing."}'; sleep 1; echo '{"type":"cancel"}') \
  | timeout 10 websocat ${WS_URL} \
  | tee /tmp/ws_cancel_events.jsonl
```

**预期**: 事件流应在收到 cancel 后较快终止（可能以 `done` 或 `error` 结束），不会持续输出完整长文。

### 评判标准

| 条件 | Pass | Fail |
|------|------|------|
| WebSocket 连接成功建立 | 必须 | 连接被拒绝或升级失败 |
| 首个事件为 `session_created` 且含非空 `session_id` | 必须 | 缺失或 ID 为空 |
| 收到至少 1 个 `text_delta` 事件 | 必须 | 未收到任何 delta |
| 收到 `text_complete` 事件含完整文本 | 必须 | 缺失 |
| 最终收到 `done` 事件 | 必须 | 事件流无终止信号 |
| 所有事件的 `session_id` 一致 | 必须 | ID 不一致 |
| Cancel 后事件流及时终止 | 必须 | 仍持续输出超过 5 秒 |

### 故障排除

| 现象 | 可能原因 | 解决方案 |
|------|----------|----------|
| `websocat: connection refused` | Server 未启动 | 先执行 `make server` |
| 收到 `error` 事件含 "Invalid message" | JSON 格式错误 | 检查 `type` 字段拼写 |
| 收到 `error` 事件含 provider 错误 | API Key 无效或额度不足 | 检查 `.env` 中的 Key 配置 |
| 仅收到 `session_created` 无后续 | Provider 连接超时 | 检查网络和 API Key |
| `text_delta` 为空字符串 | Provider 返回空 chunk | 通常是 Provider 行为，非 Bug |

---

## S3: Session 持久化与恢复

| 属性 | 值 |
|------|-----|
| **任务 ID** | S3 |
| **名称** | Session 持久化与恢复 |
| **难度** | Medium |
| **评估维度** | 数据持久化、Session 管理、消息历史完整性 |
| **预计耗时** | 15 分钟 |

### 前置条件

- Server 已启动并通过健康检查
- 已配置有效的 LLM Provider API Key
- 数据库路径可写（默认 `./data/octo.db`）

### 评估目标

验证对话消息通过 SQLite 持久化存储，Server 重启后 Session 和消息历史可完整恢复。

### 执行步骤

#### 步骤 1: 通过 WebSocket 进行对话

```bash
echo '{"type":"send_message","content":"Remember this code: EVAL-S3-2026. Please confirm you received it."}' \
  | timeout 30 websocat ${WS_URL} \
  | tee /tmp/ws_s3_events.jsonl
```

#### 步骤 2: 提取 Session ID

```bash
SESSION_ID=$(head -1 /tmp/ws_s3_events.jsonl | jq -r '.session_id')
echo "Session ID: ${SESSION_ID}"
```

#### 步骤 3: 通过 REST API 查询 Session（重启前）

```bash
curl -s ${BASE_URL}/api/sessions/${SESSION_ID} | jq .
```

**预期**: 返回 Session 对象，包含 `id`、`created_at` 和消息列表。

#### 步骤 4: 查询 Session 列表

```bash
curl -s ${BASE_URL}/api/sessions | jq '.[0]'
```

**预期**: 列表中包含上述 Session。

#### 步骤 5: 重启 Server

```bash
# 在另一个终端停止 Server (Ctrl+C)，然后重新启动
make server
# 等待启动完成
sleep 3
curl -s ${BASE_URL}/api/health | jq '.status'
```

**预期**: 返回 `"ok"`。

#### 步骤 6: 重启后查询 Session

```bash
curl -s ${BASE_URL}/api/sessions/${SESSION_ID} | jq .
```

**预期**: Session 仍然存在，包含之前的对话消息。

#### 步骤 7: 验证消息历史完整性

```bash
# 检查 Session 中包含用户消息
curl -s ${BASE_URL}/api/sessions/${SESSION_ID} | jq '.messages[] | select(.role == "user") | .content' 2>/dev/null
```

**预期**: 包含 `"Remember this code: EVAL-S3-2026..."` 内容的用户消息。

#### 步骤 8: 验证 Session 列表持久化

```bash
curl -s ${BASE_URL}/api/sessions | jq 'length'
```

**预期**: 数量 >= 1，包含重启前创建的 Session。

### 评判标准

| 条件 | Pass | Fail |
|------|------|------|
| 对话成功完成并获取 Session ID | 必须 | 对话失败 |
| 重启前 GET Session 返回完整数据 | 必须 | 返回 404 或数据不完整 |
| 重启后 GET Session 返回相同数据 | 必须 | Session 丢失 |
| 消息历史包含用户发送的原始内容 | 必须 | 内容缺失或被截断 |
| Session 列表在重启后仍包含该 Session | 必须 | 列表为空 |

### 故障排除

| 现象 | 可能原因 | 解决方案 |
|------|----------|----------|
| 重启后 Session 丢失 | 使用了 InMemory 存储 | 检查配置确认使用 SQLite (`OCTO_DB_PATH`) |
| Session 存在但消息为空 | 消息未写入 SQLite | 检查 SessionStore 实现 |
| 重启后 Session ID 变化 | Server 创建了新 Session | 确认使用正确的 Session ID 查询 |
| 数据库锁定错误 | 前一个进程未完全退出 | `lsof ./data/octo.db` 检查并 kill 残留进程 |

---

## S4: Token Budget 监控

| 属性 | 值 |
|------|-----|
| **任务 ID** | S4 |
| **名称** | Token Budget 监控 |
| **难度** | Medium |
| **评估维度** | Token 计量、Budget API、可观测性 |
| **预计耗时** | 10 分钟 |

### 前置条件

- Server 已启动并通过健康检查
- 已配置有效的 LLM Provider API Key

### 评估目标

验证 Token Budget API 在对话前后返回合理数据，且对话消耗后各字段正确变化。

### 执行步骤

#### 步骤 1: 对话前查询 Budget 基线

```bash
curl -s ${BASE_URL}/api/budget | jq . | tee /tmp/budget_before.json
```

**预期**: 返回 JSON 对象，包含 Token 相关字段。记录此时的值作为基线。

```json
{
  "max_context_tokens": 200000,
  "used_tokens": 0,
  "remaining_tokens": 200000,
  ...
}
```

#### 步骤 2: 进行一次对话

```bash
echo '{"type":"send_message","content":"Explain what a binary tree is in 3 sentences."}' \
  | timeout 30 websocat ${WS_URL} \
  | tee /tmp/ws_s4_events.jsonl
```

#### 步骤 3: 检查事件流中的 token_budget_update

```bash
cat /tmp/ws_s4_events.jsonl | jq 'select(.type == "token_budget_update")'
```

**预期**: 至少收到 1 个 `token_budget_update` 事件，包含 `budget` 对象。

#### 步骤 4: 对话后查询 Budget

```bash
curl -s ${BASE_URL}/api/budget | jq . | tee /tmp/budget_after.json
```

#### 步骤 5: 对比 Budget 变化

```bash
echo "=== Before ==="
jq '.used_tokens' /tmp/budget_before.json
echo "=== After ==="
jq '.used_tokens' /tmp/budget_after.json
```

**预期**: `used_tokens` 对话后 > 对话前（增加量应为合理范围，一般 > 50 tokens）。

#### 步骤 6: 进行第二次对话并再次检查

```bash
echo '{"type":"send_message","content":"Now explain a hash table in 2 sentences."}' \
  | timeout 30 websocat ${WS_URL} > /dev/null 2>&1

curl -s ${BASE_URL}/api/budget | jq '.used_tokens' | tee /tmp/budget_final.json
```

**预期**: `used_tokens` 进一步增加（累积计算）。

#### 步骤 7: 验证 Metrics 端点

```bash
curl -s ${BASE_URL}/api/metrics | jq .
```

**预期**: 返回 Metrics 数据，可能包含请求计数、延迟等信息。

### 评判标准

| 条件 | Pass | Fail |
|------|------|------|
| Budget API 返回有效 JSON | 必须 | 返回错误或空响应 |
| 对话后 `used_tokens` 增加 | 必须 | 无变化或减少 |
| WebSocket 事件流包含 `token_budget_update` | 推荐 | 缺失不直接 Fail，但降低分数 |
| 多次对话后 Token 累积增长 | 必须 | Token 不累积 |
| 各字段值合理（无负数、不超 max） | 必须 | 出现负数或溢出 |

### 故障排除

| 现象 | 可能原因 | 解决方案 |
|------|----------|----------|
| Budget 全部为 0 | 计量功能未启用 | 检查 config.yaml 中 metering 配置 |
| `used_tokens` 未变化 | Provider 未返回 usage 数据 | 某些 Provider/Model 不返回 token usage，切换到 Anthropic |
| `remaining_tokens` 为负 | 计算逻辑 Bug | 检查 ContextBudgetManager 实现 |
| Metrics 端点返回 404 | 路由未注册 | 检查 `metrics::router()` 是否挂载 |

---

## S5: Provider Chain 故障模拟

| 属性 | 值 |
|------|-----|
| **任务 ID** | S5 |
| **名称** | Provider Chain 故障模拟 |
| **难度** | Hard |
| **评估维度** | Provider Chain 容错、Failover、可观测性 |
| **预计耗时** | 20 分钟 |

### 前置条件

- Server 已启动并通过健康检查
- 至少拥有 **两个不同的** LLM Provider API Key（如 Anthropic + OpenAI）
- 或者：拥有一个有效 Key + 一个无效/过期 Key（用于模拟故障）

### 评估目标

验证 Provider Chain 的多实例管理、健康检查和故障转移能力。当主 Provider 不可用时，系统能自动切换到备用 Provider 并产生可观测的 Failover 事件。

### 执行步骤

#### 步骤 1: 查看当前 Provider 列表

```bash
curl -s ${BASE_URL}/api/providers | jq .
```

**预期**: 返回包含 `policy`、`instances` 的 JSON。

#### 步骤 2: 添加一个高优先级的「故障 Provider」

```bash
curl -s -X POST ${BASE_URL}/api/providers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "broken-primary",
    "provider": "openai",
    "api_key": "sk-INVALID-KEY-FOR-TESTING",
    "model": "gpt-4o",
    "priority": 1,
    "enabled": true
  }' | jq .
```

**预期**: HTTP 200/201，Provider 实例添加成功。

#### 步骤 3: 添加一个低优先级的有效 Provider（备用）

```bash
curl -s -X POST ${BASE_URL}/api/providers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "healthy-backup",
    "provider": "anthropic",
    "api_key": "'${ANTHROPIC_API_KEY}'",
    "model": "claude-sonnet-4-20250514",
    "priority": 10,
    "enabled": true
  }' | jq .
```

**预期**: HTTP 200/201。

#### 步骤 4: 验证 Provider 列表包含两个实例

```bash
curl -s ${BASE_URL}/api/providers | jq '.instances | length'
```

**预期**: >= 2。

#### 步骤 5: 触发对话（触发 Failover）

```bash
echo '{"type":"send_message","content":"Hello, confirm you are working."}' \
  | timeout 60 websocat ${WS_URL} \
  | tee /tmp/ws_s5_events.jsonl
```

**预期**: 系统先尝试 `broken-primary`（失败），然后自动切换到 `healthy-backup`，最终返回有效回复。可能在事件流中看到 `error` 事件（来自首次尝试）后跟正常的 `text_delta` 序列。

#### 步骤 6: 检查事件流

```bash
cat /tmp/ws_s5_events.jsonl | jq -r '.type'
```

**预期**: 事件流最终包含 `text_complete` 和 `done`，说明 Failover 成功。

#### 步骤 7: 检查 Provider 健康状态

```bash
curl -s ${BASE_URL}/api/providers | jq '.instances[] | {id, health}'
```

**预期**: `broken-primary` 的 health 标记为不健康（如 `"Unhealthy"` 或 `"Degraded"`），`healthy-backup` 为 `"Healthy"`。

#### 步骤 8: 检查 Events 端点获取 Failover 记录

```bash
curl -s ${BASE_URL}/api/events | jq .
```

**预期**: 可能包含 Provider 切换相关的事件记录。

#### 步骤 9: 清理 — 移除故障 Provider

```bash
curl -s -X DELETE ${BASE_URL}/api/providers/broken-primary | jq .
curl -s -X DELETE ${BASE_URL}/api/providers/healthy-backup | jq .
```

### 评判标准

| 条件 | Pass | Fail |
|------|------|------|
| 成功添加多个 Provider 实例 | 必须 | API 返回错误 |
| 主 Provider 故障后对话仍能完成 | 必须 | 对话完全失败无回复 |
| 故障 Provider 健康状态标记为不健康 | 推荐 | 仍显示 Healthy（降低分数） |
| 最终事件流包含 `text_complete` + `done` | 必须 | 缺失 |
| 故障 Provider 可删除 | 必须 | 删除失败 |

### 故障排除

| 现象 | 可能原因 | 解决方案 |
|------|----------|----------|
| 添加 Provider 返回 404 | Provider Chain 未启用 | 检查 config.yaml 中 `provider_chain` 配置 |
| 对话直接失败无 Failover | Policy 设置为 `single` 而非 `failover` | 修改配置：`provider_chain.policy: failover` |
| 所有 Provider 都失败 | 备用 Key 也无效 | 检查 `ANTHROPIC_API_KEY` 环境变量 |
| 健康状态未更新 | 健康检查周期较长 | 等待或手动触发健康检查 |
| 删除 Provider 返回 404 | ID 不匹配 | 先 GET `/api/providers` 确认实际 ID |

---

## S6: 审计日志完整性

| 属性 | 值 |
|------|-----|
| **任务 ID** | S6 |
| **名称** | 审计日志完整性 |
| **难度** | Easy |
| **评估维度** | 审计追踪、数据完整性、可观测性 |
| **预计耗时** | 5 分钟 |

### 前置条件

- Server 已启动并通过健康检查
- 已执行至少一次 API 操作（如 S1 的 Agent 注册）

### 评估目标

验证系统自动记录 API 操作的审计日志，日志包含时间戳、事件类型、操作详情等必要字段，且支持查询和过滤。

### 执行步骤

#### 步骤 1: 执行一系列可审计操作

```bash
# 操作 A: 健康检查
curl -s ${BASE_URL}/api/health > /dev/null

# 操作 B: 查询工具列表
curl -s ${BASE_URL}/api/tools > /dev/null

# 操作 C: 查询 Session 列表
curl -s ${BASE_URL}/api/sessions > /dev/null

# 操作 D: 查询内存
curl -s "${BASE_URL}/api/memories?q=test" > /dev/null
```

#### 步骤 2: 查询全部审计日志

```bash
curl -s "${BASE_URL}/api/audit" | jq .
```

**预期**: 返回包含 `logs` 数组和 `total` 计数的 JSON 对象。

```json
{
  "logs": [
    {
      "id": 1,
      "timestamp": "2026-03-13T...",
      "event_type": "api_request",
      "user_id": null,
      "session_id": null,
      "resource_id": null,
      "action": "GET /api/health",
      "result": "200",
      "metadata": null,
      "ip_address": "127.0.0.1"
    },
    ...
  ],
  "total": 4
}
```

#### 步骤 3: 验证审计记录字段完整性

```bash
# 检查第一条记录是否包含所有必要字段
curl -s "${BASE_URL}/api/audit?limit=1" | jq '.logs[0] | keys'
```

**预期**: 包含以下字段：`id`, `timestamp`, `event_type`, `action`, `result`。

#### 步骤 4: 按事件类型过滤

```bash
curl -s "${BASE_URL}/api/audit?event_type=api_request&limit=5" | jq '.logs | length'
```

**预期**: 返回 >= 1 条记录。

#### 步骤 5: 验证分页功能

```bash
# 第一页
curl -s "${BASE_URL}/api/audit?limit=2&offset=0" | jq '.logs | length'
# 第二页
curl -s "${BASE_URL}/api/audit?limit=2&offset=2" | jq '.logs | length'
```

**预期**: 分页返回不同的记录集合，每页 <= 2 条。

#### 步骤 6: 验证时间戳格式

```bash
curl -s "${BASE_URL}/api/audit?limit=1" | jq -r '.logs[0].timestamp'
```

**预期**: 返回 ISO 8601 格式的时间戳（如 `2026-03-13T10:30:00Z`）。

#### 步骤 7: 执行新操作并验证审计记录增长

```bash
# 记录当前总数
BEFORE=$(curl -s "${BASE_URL}/api/audit" | jq '.total')

# 执行操作
curl -s ${BASE_URL}/api/budget > /dev/null

# 检查总数增长
AFTER=$(curl -s "${BASE_URL}/api/audit" | jq '.total')
echo "Before: ${BEFORE}, After: ${AFTER}"
```

**预期**: `AFTER` > `BEFORE`（至少增加 1）。

### 评判标准

| 条件 | Pass | Fail |
|------|------|------|
| 审计 API 返回有效 JSON | 必须 | 返回错误或空响应 |
| 日志包含 `id`, `timestamp`, `action`, `result` 字段 | 必须 | 任何必要字段缺失 |
| `timestamp` 为有效时间格式 | 必须 | 时间戳无法解析 |
| 新操作产生新的审计记录 | 必须 | 记录数无增长 |
| 分页参数 `limit`/`offset` 生效 | 必须 | 忽略分页参数 |
| 过滤参数 `event_type` 生效 | 推荐 | 过滤不生效（降低分数） |

### 故障排除

| 现象 | 可能原因 | 解决方案 |
|------|----------|----------|
| 审计 API 返回 404 | 路由未注册 | 检查 `audit::router()` 是否挂载 |
| `logs` 数组为空 | 审计中间件未生效 | 检查 `audit_middleware` 是否在 router 中注册 |
| `total` 为 0 但有操作 | 数据库写入失败 | 检查 SQLite 数据库权限和路径 |
| 时间戳不是 ISO 8601 | 序列化格式不一致 | 检查 `AuditRecord` 的 timestamp 字段类型 |
| 分页不生效 | Query 参数解析错误 | 检查 `AuditQuery` 的 Deserialize 实现 |

---

## 附录

### A. 评估结果汇总模板

| 任务 | 名称 | 难度 | 结果 | 备注 |
|------|------|------|------|------|
| S1 | Agent 生命周期管理 | Easy | PASS / FAIL | |
| S2 | WebSocket 流式对话 | Medium | PASS / FAIL | |
| S3 | Session 持久化与恢复 | Medium | PASS / FAIL | |
| S4 | Token Budget 监控 | Medium | PASS / FAIL | |
| S5 | Provider Chain 故障模拟 | Hard | PASS / FAIL | |
| S6 | 审计日志完整性 | Easy | PASS / FAIL | |

### B. 自动化脚本提示

上述步骤可封装为 shell 脚本自动执行。建议：

1. 每个任务封装为独立函数
2. 使用 `jq` 的 `-e` 参数实现断言（非零退出码表示失败）
3. 收集所有任务结果后统一输出汇总表

示例断言：

```bash
# 断言 health status 为 "ok"
curl -s ${BASE_URL}/api/health | jq -e '.status == "ok"' > /dev/null \
  && echo "PASS: health check" \
  || echo "FAIL: health check"
```

### C. 工具安装参考

```bash
# macOS
brew install jq websocat

# Linux (Ubuntu/Debian)
sudo apt-get install jq
cargo install websocat

# 验证安装
jq --version
websocat --version
```
