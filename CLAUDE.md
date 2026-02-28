# Octo Sandbox

An AI Agent workbench built with Rust + TypeScript, supporting MCP (Model Context Protocol) server management, tool execution, memory management, and debugging.

**IMPORTANT**: This file is the primary memory for the project. If any information below becomes outdated due to code changes, UPDATE IT IMMEDIATELY. Outdated information is harmful.

---

## Configuration System

### Priority Order (lowest to highest)
1. **config.yaml** - Main configuration file
2. **CLI arguments** - e.g., `--port 4000`
3. **Environment Variables (.env)** - Highest priority, overrides everything

### Configuration Files

| File | Purpose | Version Control |
|------|---------|----------------|
| `.env` | Local dev overrides (NOT committed) | Ignored |
| `.env.example` | Template for .env (committed) | Committed |
| `config.yaml` | Main deployment config (committed) | Committed |
| `config.default.yaml` | Auto-generated from code (re-generated on changes) | Committed |

### Configuration Command

**IMPORTANT**: Run `make config-gen` whenever `crates/octo-server/src/config.rs` is modified to keep `config.default.yaml` in sync.

```bash
# Generate config.yaml from current code defaults
make config-gen
```

### Environment Variables

These are loaded into config system and have lowest priority:

```bash
# Required
ANTHROPIC_API_KEY=sk-ant-xxxxx     # Anthropic API key
OPENAI_API_KEY=sk-xxxxx             # OpenAI API key (if using openai provider)

# Server
OCTO_HOST=127.0.0.1                 # Server host (default: 127.0.0.1)
OCTO_PORT=3001                      # Server port (default: 3001)
OCTO_DB_PATH=./data/octo.db        # Database path

# Provider
LLM_PROVIDER=anthropic              # Provider: anthropic (default) or openai
OPENAI_MODEL_NAME=gpt-4o            # Model override (optional)

# Logging
RUST_LOG=octo_server=debug,octo_engine=debug
```

---

## Service Ports

**DO NOT hardcode ports in code. Use configuration.**

| Port | Service | Config Location |
|------|---------|-----------------|
| 3001 | Backend API | `OCTO_PORT` env or `config.yaml` |
| 5180 | Frontend (Vite) | `web/vite.config.ts` |

### Frontend Proxy Configuration

In `web/vite.config.ts`:

```typescript
server: {
  port: 5180,
  proxy: {
    "/api": { target: "http://127.0.0.1:3001" },
    "/ws":  { target: "ws://127.0.0.1:3001", ws: true },
  },
}
```

---

## Makefile Commands

**ALWAYS update Makefile when adding/removing commands.**

Common commands (check Makefile for full list):

```bash
# Development
make dev              # Start backend + frontend
make server          # Backend only (port 3001)
make web             # Frontend only (port 5180)

# Building
make build           # Build Rust
make check           # Fast Rust check (no binary)
make all             # Full build (backend + frontend)

# Configuration
make config-gen      # Generate config.yaml from code defaults

# Testing
make test            # Run all tests

# Verification
make verify          # Static: cargo check + tsc + vite build
make verify-api      # API endpoints (requires server running)

# Code quality
make fmt             # Format code
make lint            # Clippy lint
```

---

## Project Structure

```
octo-sandbox/
├── crates/
│   ├── octo-engine/          # Core engine (lib)
│   │   ├── agent/            # Agent loop & context
│   │   ├── context/           # Context builder, budget, pruner
│   │   ├── db/                # SQLite connection & migrations
│   │   ├── event/             # Event bus for observability
│   │   ├── memory/            # Working memory, persistent store
│   │   ├── mcp/               # MCP client, server, storage
│   │   ├── providers/         # LLM provider traits
│   │   ├── session/           # Session management
│   │   ├── skills/            # Skill loader & registry
│   │   └── tools/             # Built-in tools
│   │
│   └── octo-server/          # API server (bin)
│       ├── api/               # REST endpoints
│       ├── config.rs           # Configuration module
│       ├── router.rs           # Axum router
│       ├── state.rs            # AppState
│       └── ws.rs               # WebSocket handler
│
├── web/                       # Frontend (React)
│   ├── src/
│   │   ├── atoms/             # Jotai atoms
│   │   ├── components/         # React components
│   │   ├── pages/              # Page components
│   │   ├── stores/             # State stores
│   │   └── ws/                 # WebSocket manager
│   └── vite.config.ts          # Vite config
│
├── config.yaml                # Main config (committed)
├── .env                       # Local overrides (NOT committed)
├── .env.example               # Env template
└── Makefile                   # Commands
```

---

## Module Overview

| Module | Key Files | Responsibility |
|--------|-----------|----------------|
| `agent/` | `loop_.rs` | Agent loop, message handling |
| `context/` | `builder.rs`, `budget.rs` | Context building, token budget |
| `memory/` | `working.rs`, `sqlite_store.rs` | Layer 0-2 memory |
| `mcp/` | `manager.rs`, `stdio.rs`, `sse.rs` | MCP clients |
| `providers/` | `traits.rs` | LLM abstraction |
| `session/` | `mod.rs` | Session persistence |
| `skills/` | `loader.rs`, `registry.rs` | Skill system |
| `tools/` | `mod.rs`, `bash.rs`, `file_*.rs` | Built-in tools |

---

## Tech Stack

- **Backend**: Rust, Tokio, Axum, SQLite (rusqlite)
- **Frontend**: React 18, TypeScript, Vite, Jotai, TailwindCSS
- **LLM**: Anthropic, OpenAI

---

## Key Patterns

### State Management
- Backend: `Arc<T>` for shared state, `tokio::sync::Mutex` for async locks
- Frontend: Jotai atoms for UI state

### MCP Integration
- `McpManager`: Runtime server lifecycle (start/stop)
- `McpStorage`: SQLite persistence
- `McpToolBridge`: Wraps MCP tools in unified Tool trait

### Memory Layers
- Layer 0: Working memory (current conversation)
- Layer 1: Session memory (per-session)
- Layer 2: Persistent memory (long-term)

---

## Git Commit Guidelines

**IMPORTANT**: Commit after meaningful work, not mechanically.

### When to Commit

1. **After Phase Management**
   - After `/end-phase` or `/checkpoint-progress`: **ALWAYS commit**

2. **After Task Completion**
   - After completing a bug fix
   - After implementing a feature or subtask
   - After any code change you might want to reference later

3. **Before Risky Operations**
   - Before major refactoring
   - Before switching branches

### Commit Frequency Rule

- **Minimum**: After each completed task/subtask
- **Trigger**: When `git status` shows >3 modified files that form a logical unit
- **Don't**: Commit mechanically before /clear just for the sake of committing
