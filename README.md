# 🦑 Octo Sandbox

**Enterprise-grade autonomous agent platform with sandboxed execution.**

Octo Sandbox delivers the full power of autonomous AI agents — long-horizon reasoning, parallel tool execution, structured multi-layer memory, MCP-native tooling, and cron-based scheduling — inside a security boundary designed for enterprise deployment: Docker/WASM sandboxed execution, security policies, action auditing, multi-tenant isolation, and secret management.

Built on a high-performance Rust core with a React workbench UI.

---

## Why Octo Sandbox

Most autonomous agent frameworks are built for developer experience. Octo Sandbox is built for enterprise production readiness — where control, auditability, and isolation matter as much as capability.

| Capability | Details |
|---|---|
| **Sandboxed execution** | Docker containers, WASM runtime, and subprocess adapters — untrusted code never runs on the host |
| **Security policies** | Per-agent autonomy levels, command risk classification, path allowlists |
| **Audit trail** | Every tool call, agent action, and session event is recorded |
| **MCP-native** | Full Model Context Protocol support — stdio and SSE transports, hot-reload without restart |
| **Multi-layer memory** | Working memory (in-session), session memory, persistent memory with full-text and semantic search, knowledge graph |
| **Scheduled agents** | Cron-based task scheduling with execution history |
| **Multi-provider LLM** | Anthropic, OpenAI, and OpenAI-compatible endpoints (DeepSeek, proxy, etc.) |
| **Parallel tool execution** | Semaphore-controlled concurrent tool calls with configurable limits |
| **Skills system** | Filesystem-loaded skill modules, hot-reloaded, per-agent tool filtering |

---

## Architecture

```
octo-sandbox (mono-repo)
├── octo-types          Shared type definitions
├── octo-engine         Core agent runtime (shared library)
│   ├── agent/          AgentRuntime → AgentExecutor → AgentLoop
│   ├── sandbox/        Docker · WASM · subprocess adapters
│   ├── security/       Policy engine · action tracker
│   ├── audit/          Audit event storage
│   ├── memory/         Working · session · persistent · knowledge graph
│   ├── mcp/            MCP client manager (stdio + SSE)
│   ├── providers/      Anthropic · OpenAI · retry · provider chain
│   ├── scheduler/      Cron scheduler · execution history
│   ├── skills/         Skill loader · registry
│   ├── tools/          Built-in tools (bash, file, search…)
│   ├── auth/           Authentication middleware
│   └── secret/         Secret manager
│
├── octo-server         Workbench API server (Axum, port 3001)
└── web/                Workbench frontend (React + TypeScript + Vite, port 5180)
```

**Two products, one engine:**

- **octo-workbench** (`octo-server` + `web/`) — single-user agent workbench, production-ready for individual or team use
- **octo-platform** (planned) — multi-tenant enterprise platform with per-tenant isolation, user management, quota control, and orchestration

---

## Quick Start

**Prerequisites:** Rust 1.75+, Node.js 18+, an Anthropic or OpenAI API key.

```bash
# Clone
git clone https://github.com/uukuguy/octo-sandbox.git
cd octo-sandbox

# Configure
cp .env.example .env
# Edit .env: set ANTHROPIC_API_KEY or OPENAI_API_KEY

# Install frontend dependencies
make setup

# Start (backend on :3001, frontend on :5180)
make dev
```

Open [http://localhost:5180](http://localhost:5180).

---

## Configuration

Priority order (lowest → highest): `config.yaml` → CLI args → environment variables.

Key environment variables:

```bash
# Provider
LLM_PROVIDER=anthropic          # anthropic | openai
ANTHROPIC_API_KEY=sk-ant-...    # required for Anthropic
OPENAI_API_KEY=sk-...           # required for OpenAI
OPENAI_BASE_URL=...             # optional: proxy or compatible endpoint
OPENAI_MODEL_NAME=deepseek-chat # optional: model override

# Server
OCTO_HOST=127.0.0.1
OCTO_PORT=3001
OCTO_DB_PATH=./data/octo.db

# Logging
RUST_LOG=octo_server=info,octo_engine=info
OCTO_LOG_FORMAT=json            # optional: structured JSON logs
```

Generate a fully-commented config file:

```bash
make config-gen   # writes config.yaml
```

---

## Development

```bash
make dev          # start backend + frontend (hot reload)
make server       # backend only
make web          # frontend only

make build        # compile Rust
make check        # fast compile check (no binary)
make test         # run all tests
make fmt          # format code
make lint         # Clippy lint
make verify       # static: cargo check + tsc + vite build
```

---

## REST API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | System health and component status |
| `GET` | `/api/metrics` | Runtime metrics (turns, tool calls, latency) |
| `GET/POST` | `/api/sessions` | Session management |
| `GET/POST/DELETE` | `/api/memories` | Persistent memory CRUD and search |
| `GET/POST/DELETE` | `/api/mcp/servers` | MCP server lifecycle |
| `GET` | `/api/tools` | Available tools |
| `GET/POST/DELETE` | `/api/scheduler/tasks` | Cron task management |
| `POST` | `/api/scheduler/tasks/:id/run` | Manual task trigger |
| `GET/POST/DELETE` | `/api/agents` | Agent catalog |
| `WS` | `/ws` | Real-time agent stream (chat, tool events, token budget) |

---

## Tech Stack

| Layer | Technology |
|---|---|
| Agent runtime | Rust, Tokio |
| API server | Axum, Tower |
| Database | SQLite (rusqlite, WAL mode), FTS5 |
| MCP | rmcp SDK (stdio + SSE) |
| Sandbox | Docker API, WASM, subprocess |
| Frontend | React 18, TypeScript, Vite, Jotai, TailwindCSS |
| LLM | Anthropic Claude, OpenAI (and compatible) |

---

## License

MIT
