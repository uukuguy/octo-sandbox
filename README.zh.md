# 🦑 Octo Sandbox

**企业级自主智能体平台，沙箱安全执行。**

Octo Sandbox 提供完整的自主智能体能力——长链推理、并行工具执行、结构化多层记忆、MCP 原生工具集成、定时调度——同时内建企业级安全边界：Docker/WASM 沙箱隔离执行、安全策略引擎、操作审计、多租户隔离、密钥管理。

Rust 高性能核心，React 工作台前端。

---

## 为什么选择 Octo Sandbox

大多数自主智能体框架以开发者体验为核心。Octo Sandbox 以企业生产就绪为核心——在这里，可控性、可审计性和隔离性与智能体能力同等重要。

| 能力 | 说明 |
|---|---|
| **沙箱执行** | Docker 容器、WASM 运行时、subprocess 适配器——不可信代码永远不在宿主机上运行 |
| **安全策略** | 每个 Agent 独立自主等级配置、命令风险分级、路径白名单 |
| **审计日志** | 所有工具调用、Agent 行为、会话事件均持久化记录 |
| **MCP 原生** | 完整 Model Context Protocol 支持——stdio 和 SSE 传输，运行时热插拔无需重启 |
| **多层记忆** | 工作记忆（会话内）、会话记忆、持久记忆（全文检索 + 语义搜索）、知识图谱 |
| **定时调度** | Cron 表达式任务调度，附执行历史记录 |
| **多 LLM 提供商** | Anthropic、OpenAI 及兼容接口（DeepSeek、代理等） |
| **并行工具执行** | Semaphore 限流的并发工具调用，可配置并发上限 |
| **Skills 系统** | 文件系统加载的技能模块，热重载，支持 per-agent 工具过滤 |

---

## 架构

```
octo-sandbox（mono-repo）
├── octo-types          共享类型定义
├── octo-engine         核心 Agent 运行时（共享库）
│   ├── agent/          AgentRuntime → AgentExecutor → AgentLoop
│   ├── sandbox/        Docker · WASM · subprocess 适配器
│   ├── security/       策略引擎 · 行为追踪
│   ├── audit/          审计事件存储
│   ├── memory/         工作记忆 · 会话记忆 · 持久记忆 · 知识图谱
│   ├── mcp/            MCP 客户端管理（stdio + SSE）
│   ├── providers/      Anthropic · OpenAI · 重试 · 提供商链
│   ├── scheduler/      Cron 调度 · 执行历史
│   ├── skills/         技能加载 · 注册表
│   ├── tools/          内置工具（bash、文件、搜索…）
│   ├── auth/           认证中间件
│   └── secret/         密钥管理
│
├── octo-server         工作台 API 服务器（Axum，端口 3001）
└── web/                工作台前端（React + TypeScript + Vite，端口 5180）
```

**两个产品，一个引擎：**

- **octo-workbench**（`octo-server` + `web/`）——单用户 Agent 工作台，可直接生产部署
- **octo-platform**（规划中）——多租户企业平台，含租户隔离、用户管理、配额控制、多 Agent 编排

---

## 快速开始

**前置条件：** Rust 1.75+、Node.js 18+、Anthropic 或 OpenAI API Key。

```bash
# 克隆仓库
git clone https://github.com/uukuguy/octo-sandbox.git
cd octo-sandbox

# 配置环境变量
cp .env.example .env
# 编辑 .env，填写 ANTHROPIC_API_KEY 或 OPENAI_API_KEY

# 安装前端依赖
make setup

# 启动（后端 :3001，前端 :5180）
make dev
```

浏览器访问 [http://localhost:5180](http://localhost:5180)。

---

## 配置说明

优先级（低 → 高）：`config.yaml` → CLI 参数 → 环境变量。

常用环境变量：

```bash
# LLM 提供商
LLM_PROVIDER=anthropic          # anthropic | openai
ANTHROPIC_API_KEY=sk-ant-...    # Anthropic 必填
OPENAI_API_KEY=sk-...           # OpenAI 必填
OPENAI_BASE_URL=...             # 可选：代理或兼容接口
OPENAI_MODEL_NAME=deepseek-chat # 可选：指定模型

# 服务器
OCTO_HOST=127.0.0.1
OCTO_PORT=3001
OCTO_DB_PATH=./data/octo.db

# 日志
RUST_LOG=octo_server=info,octo_engine=info
OCTO_LOG_FORMAT=json            # 可选：输出结构化 JSON 日志
```

生成完整注释的配置文件：

```bash
make config-gen   # 写入 config.yaml
```

---

## 开发命令

```bash
make dev          # 启动后端 + 前端（热重载）
make server       # 仅后端
make web          # 仅前端

make build        # 编译 Rust
make check        # 快速编译检查（不生成二进制）
make test         # 运行全部测试
make fmt          # 格式化代码
make lint         # Clippy 静态检查
make verify       # 静态验证：cargo check + tsc + vite build
```

---

## REST API

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/health` | 系统健康状态和组件状态 |
| `GET` | `/api/metrics` | 运行时指标（轮次、工具调用、延迟） |
| `GET/POST` | `/api/sessions` | 会话管理 |
| `GET/POST/DELETE` | `/api/memories` | 持久记忆增删查改和搜索 |
| `GET/POST/DELETE` | `/api/mcp/servers` | MCP 服务器生命周期管理 |
| `GET` | `/api/tools` | 可用工具列表 |
| `GET/POST/DELETE` | `/api/scheduler/tasks` | 定时任务管理 |
| `POST` | `/api/scheduler/tasks/:id/run` | 手动触发任务 |
| `GET/POST/DELETE` | `/api/agents` | Agent 目录 |
| `WS` | `/ws` | 实时 Agent 事件流（聊天、工具事件、Token 用量） |

---

## 技术栈

| 层级 | 技术 |
|---|---|
| Agent 运行时 | Rust、Tokio |
| API 服务器 | Axum、Tower |
| 数据库 | SQLite（rusqlite，WAL 模式）、FTS5 全文检索 |
| MCP | rmcp SDK（stdio + SSE） |
| 沙箱 | Docker API、WASM、subprocess |
| 前端 | React 18、TypeScript、Vite、Jotai、TailwindCSS |
| LLM | Anthropic Claude、OpenAI 及兼容接口 |

---

## 开源协议

MIT
