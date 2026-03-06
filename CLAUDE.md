# Octo Sandbox

An autonomous AI agent workbench built with Rust (backend) + TypeScript/React (frontend). Supports MCP (Model Context Protocol) server management, tool execution, multi-layer memory, session management, sandboxed code execution, and real-time debugging.

**IMPORTANT**: This file is the primary memory for the project. If any information below becomes outdated due to code changes, UPDATE IT IMMEDIATELY. Outdated information is harmful.

---

## Mono-Repo Product Structure

**IMPORTANT**: octo-sandbox is a mono-repo hosting two independent products sharing `octo-engine`.

### Product Boundaries

| Product | Branch | Crates | Frontend | Purpose |
|---------|--------|--------|----------|---------|
| **octo-workbench** | `octo-workbench` | `octo-types`, `octo-sandbox`, `octo-engine`, `octo-server` | `web/` | Single-user single-agent workbench |
| **octo-platform** | `octo-platform` | `octo-types`, `octo-sandbox`, `octo-engine`, `octo-platform-server` | `web-platform/` | Multi-tenant multi-agent platform |

### Rules
- `octo-engine`, `octo-types`, and `octo-sandbox` are **shared libraries** — changes must be backward compatible
- `octo-server` is **workbench-only** — do NOT add platform logic here
- `octo-platform-server` is **platform-only** — separate crate, separate branch
- `web/` is **workbench frontend only** — do NOT add platform pages here
- `web-platform/` is **platform frontend only** — separate app, separate branch

---

## Crate Dependency Graph & Build Order

```
octo-types          (0) — Shared type definitions, no internal deps
    ↓
octo-sandbox        (1) — Sandbox runtime adapters (subprocess, WASM, Docker)
    ↓                      depends on: octo-types
octo-engine         (1) — Core engine: agent, memory, MCP, providers, tools
    ↓                      depends on: octo-types
octo-server         (2) — Workbench HTTP/WS server (Axum)
                           depends on: octo-types, octo-engine, octo-sandbox
octo-platform-server(2) — Platform multi-tenant server (Axum + JWT auth)
                           depends on: octo-types, octo-engine, octo-sandbox
```

**Build order**: `octo-types` → `octo-sandbox` / `octo-engine` (parallel) → `octo-server` or `octo-platform-server`

Cargo workspace handles this automatically. No manual ordering needed for `cargo build`.

---

## Crates & Modules

### `octo-types` — Shared Type Definitions

Zero-dependency type crate shared by all other crates. Defines:
- `message` — LLM message types (user, assistant, tool calls/results)
- `tool` — Tool definition, parameters, execution result types
- `execution` — Tool execution records
- `provider` — Provider configuration types
- `memory` — Memory entry types
- `skill` — Skill manifest types
- `sandbox` — Sandbox configuration types
- `id` — Typed ID wrappers (TenantId, UserId, SessionId, etc.)
- `error` — Shared error types

### `octo-sandbox` — Sandboxed Code Execution

Runtime adapters for executing untrusted code:
- `native` — Native subprocess adapter
- `traits` — Sandbox trait abstraction (`RuntimeAdapter`)

Optional features: `sandbox-wasm` (Wasmtime), `sandbox-docker` (Bollard)

### `octo-engine` — Core Engine (21 modules)

The heart of the system. All agent intelligence lives here.

| Module | Responsibility |
|--------|----------------|
| `agent/` | AgentRuntime, AgentExecutor, AgentLoop, AgentCatalog, AgentStore — full agent lifecycle |
| `context/` | SystemPromptBuilder, ContextBudgetManager, ContextPruner — context engineering |
| `memory/` | WorkingMemory (L0), SessionMemory (L1), MemoryStore (L2), KnowledgeGraph, FtsStore |
| `mcp/` | McpManager, McpClient (stdio/SSE), McpToolBridge, McpStorage |
| `providers/` | Provider trait, Anthropic/OpenAI adapters, ProviderChain (failover/load-balance) |
| `tools/` | ToolRegistry, built-in tools (bash, file_read, file_write, etc.), ToolExecutionRecorder |
| `skills/` | SkillLoader (YAML manifests), SkillRegistry |
| `skill_runtime/` | SkillRuntime, SkillContext — skill execution engine |
| `session/` | SessionStore (SQLite/InMemory), session lifecycle |
| `scheduler/` | Cron-based task scheduler with SQLite storage |
| `event/` | EventBus — pub/sub for observability |
| `db/` | SQLite Database wrapper (tokio-rusqlite) with migrations |
| `auth/` | API key management, role-based auth, middleware |
| `audit/` | AuditEvent, AuditRecord, AuditStorage — audit logging |
| `security/` | SecurityPolicy, CommandRiskLevel, AutonomyLevel, ActionTracker |
| `secret/` | Secret manager (AES-GCM encryption, optional keyring) |
| `sandbox/` | SandboxManager — Subprocess, WASM, Docker adapters |
| `extension/` | Extension system — WASM-based plugin host with hostcall intercept |
| `metrics/` | MetricsRegistry — Counter, Gauge, Histogram |
| `metering/` | Token usage metering and snapshots |
| `logging/` | Structured logging initialization (pretty/JSON) |

### `octo-server` — Workbench API Server (binary)

Single-user Axum HTTP + WebSocket server. Modules:

| Module | Responsibility |
|--------|----------------|
| `main.rs` | Server startup, config loading, AgentRuntime init, scheduler |
| `config.rs` | Layered config (config.yaml < CLI < .env) |
| `router.rs` | Axum router with all REST + WS routes |
| `state.rs` | AppState — shared application state |
| `ws.rs` | WebSocket handler for real-time streaming |
| `middleware.rs` | Request middleware |
| `session.rs` | Session HTTP helpers |
| `api/` | REST endpoints: agents, sessions, memories, tools, executions, MCP servers/tools/logs, budget, config, scheduler, tasks, metrics, audit, providers, user_context |

### `octo-platform-server` — Platform Server (binary, WIP)

Multi-tenant Axum server with JWT authentication. Modules:

| Module | Responsibility |
|--------|----------------|
| `tenant/` | TenantManager, TenantRuntime, quota enforcement, tenant models |
| `auth/` | JWT authentication, OAuth2/OIDC provider integration |
| `api/` | REST endpoints: admin/tenants, users, sessions, MCP |
| `db/` | User database operations |
| `middleware/` | Quota enforcement middleware |
| `agent_pool.rs` | Shared agent pool across tenants |
| `user_runtime.rs` | Per-user runtime isolation |
| `ws.rs` | WebSocket handler |

### `web/` — Workbench Frontend (React + TypeScript)

Single-page app with tab-based navigation:

| Page/Component | Responsibility |
|----------------|----------------|
| `pages/Chat` | Chat interface with streaming AI responses |
| `pages/Tools` | Tool execution history viewer |
| `pages/Memory` | Multi-layer memory explorer |
| `pages/Debug` | Token budget bar, EventBus viewer |
| `pages/McpWorkbench` | MCP server management (add/remove/logs) |
| `pages/Schedule` | Scheduled task management |
| `pages/Tasks` | Task list viewer |
| `components/chat/` | MessageList, MessageBubble, ChatInput, StreamingDisplay |
| `components/mcp/` | ServerList, LogViewer, ToolInvoker |
| `components/tools/` | ExecutionList, ExecutionDetail, JsonViewer, TimelineView |
| `components/debug/` | TokenBudgetBar |
| `atoms/` | Jotai state atoms |
| `ws/` | WebSocket connection manager |

---

## Tech Stack

| Layer | Technologies |
|-------|-------------|
| **Backend runtime** | Rust 1.75+, Tokio 1.42, Axum 0.8 |
| **Database** | SQLite via rusqlite 0.32 (sync) + tokio-rusqlite 0.6 (async) |
| **LLM providers** | Anthropic, OpenAI (via reqwest HTTP client) |
| **MCP SDK** | rmcp 0.16 (stdio + streamable HTTP client) |
| **Sandbox** | Wasmtime 25 (WASM), Bollard 0.18 (Docker), native subprocess |
| **Crypto** | AES-GCM, Argon2, SHA-256, JWT (jsonwebtoken 9) |
| **Frontend** | React 19, TypeScript 5.7, Vite 6, Jotai 2.16, TailwindCSS 4 |

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

### Environment Variables

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

## Build & Test Commands

**ALWAYS update Makefile when adding/removing commands.**

```bash
# ── First-time Setup ──
make setup               # Install frontend dependencies (npm install)
cp .env.example .env     # Then fill in ANTHROPIC_API_KEY

# ── Development ──
make dev                 # Start backend + frontend concurrently
make server              # Backend only (port 3001)
make web                 # Frontend only (port 5180)

# ── Building ──
make check               # Fast Rust check (no binary output)
make build               # Rust debug build
make release             # Rust release build
make web-build           # Frontend production build
make all                 # Full build (backend + frontend)

# ── Testing ──
make test                # Run all workspace tests (cargo test --workspace)
make test-types          # Test octo-types only
make test-engine         # Test octo-engine only
make test-server         # Test octo-server only

# ── Code Quality ──
make fmt                 # Format all Rust code (cargo fmt --all)
make fmt-check           # Format check for CI (cargo fmt --all -- --check)
make lint                # Clippy lint (cargo clippy --workspace -- -D warnings)
make web-check           # TypeScript type check (tsc --noEmit)
make web-lint            # ESLint frontend

# ── Verification ──
make verify              # Static: cargo check + tsc + vite build
make verify-runtime      # Print manual runtime verification checklist
make verify-api          # REST API endpoint smoke test (requires running server)
make verify-api-mcp ID=x # MCP server-specific endpoint check

# ── Configuration ──
make config-gen          # Generate config.yaml from code defaults

# ── Cleanup ──
make clean               # Clean Rust build artifacts
make clean-web           # Clean frontend (node_modules, dist)
make clean-all           # Clean everything
```

---

## Key Design Documents

The `docs/design/` directory contains architectural decision records and design documents (written in Chinese):

| Document | Topic |
|----------|-------|
| `ARCHITECTURE_DESIGN.md` | Overall system architecture |
| `AGENT_RUNTIME_DESIGN.md` | AgentRuntime core design |
| `AGENT_RUNTIME_ARCHITECTURE_AUDIT.md` | Architecture audit of AgentRuntime → AgentExecutor → AgentLoop |
| `AGENT_LOOP_COMPARISON.md` | Agent loop comparison with peer frameworks |
| `CONTEXT_ENGINEERING_DESIGN.md` | Context building, budget management, pruning |
| `MCP_WORKBENCH_DESIGN.md` | MCP server management and tool bridge |
| `MEMORY_PLAN.md` | Multi-layer memory architecture |
| `ENTERPRISE_AGENT_SANDBOX_AUTH_DESIGN.md` | Enterprise sandbox and auth design |
| `D5_SINGLETON_AGENT_CHANNEL_DESIGN.md` | Singleton agent communication channel |
| `COMPETITIVE_ANALYSIS.md` | Competitive framework analysis (Goose, etc.) |
| `PHASE_2_5_DESIGN.md` | Phase 2.5: Sandbox, auth, user isolation |
| `PHASE_2_6_PROVIDER_CHAIN_DESIGN.md` | Provider chain failover/load-balance |
| `PHASE_2_7_METRICS_AUDIT_DESIGN.md` | Metrics and audit subsystem |
| `PHASE_2_8_AGENT_ENHANCEMENT_DESIGN.md` | Agent enhancement (catalog, skills) |
| `RUST_BUILD_OPTIMIZATION.md` | Rust compile-time optimization strategies |

Implementation plans live in `docs/plans/` (date-prefixed).

---

## Key Patterns

### Agent Architecture
- `AgentRuntime` → owns all components (provider, memory, tools, MCP, sessions)
- `AgentExecutor` → per-session agent instance with cancel token
- `AgentLoop` → single conversation turn: context build → LLM call → tool execution → repeat
- `AgentCatalog` → agent registry with state machine (Created → Running → Paused → Stopped)
- `AgentStore` → SQLite persistence for agent manifests

### State Management
- Backend: `Arc<T>` for shared state, `tokio::sync::Mutex` for async locks
- Frontend: Jotai atoms for UI state

### MCP Integration
- `McpManager`: Runtime server lifecycle (start/stop)
- `McpStorage`: SQLite persistence
- `McpToolBridge`: Wraps MCP tools into unified `Tool` trait

### Memory Layers
- Layer 0: Working memory (current conversation)
- Layer 1: Session memory (per-session)
- Layer 2: Persistent memory (long-term)
- Knowledge Graph: Entity-relation graph with FTS search

### Shared Design Tokens (Frontend)
Both frontends share visual design via:
- `design/tailwind.base.ts` — shared Tailwind config (colors, fonts, spacing)
- `design/tokens.css` — CSS variables for theming

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

---

## Behavioral Rules (Always Enforced)

- Do what has been asked; nothing more, nothing less
- NEVER create files unless they're absolutely necessary for achieving your goal
- ALWAYS prefer editing an existing file to creating a new one
- NEVER proactively create documentation files (*.md) or README files unless explicitly requested
- NEVER save working files, text/mds, or tests to the root folder
- Never continuously check status after spawning a swarm — wait for results
- ALWAYS read a file before editing it
- NEVER commit secrets, credentials, or .env files

## File Organization

- NEVER save to root folder — use the directories below
- Use `/src` for source code files
- Use `/tests` for test files
- Use `/docs` for documentation and markdown files
- Use `/config` for configuration files
- Use `/scripts` for utility scripts
- Use `/examples` for example code

## Project Architecture

- Follow Domain-Driven Design with bounded contexts
- Keep files under 500 lines
- Use typed interfaces for all public APIs
- Prefer TDD London School (mock-first) for new code
- Use event sourcing for state changes
- Ensure input validation at system boundaries

### Project Config

- **Topology**: hierarchical-mesh
- **Max Agents**: 15
- **Memory**: hybrid
- **HNSW**: Enabled
- **Neural**: Enabled

## Build & Test

```bash
# Build
npm run build

# Test
npm test

# Lint
npm run lint
```

- ALWAYS run tests after making code changes
- ALWAYS verify build succeeds before committing

## Security Rules

- NEVER hardcode API keys, secrets, or credentials in source files
- NEVER commit .env files or any file containing secrets
- Always validate user input at system boundaries
- Always sanitize file paths to prevent directory traversal
- Run `npx @claude-flow/cli@latest security scan` after security-related changes

## Concurrency: 1 MESSAGE = ALL RELATED OPERATIONS

- All operations MUST be concurrent/parallel in a single message
- Use Claude Code's Task tool for spawning agents, not just MCP
- ALWAYS batch ALL todos in ONE TodoWrite call (5-10+ minimum)
- ALWAYS spawn ALL agents in ONE message with full instructions via Task tool
- ALWAYS batch ALL file reads/writes/edits in ONE message
- ALWAYS batch ALL Bash commands in ONE message

## Swarm Orchestration

- MUST initialize the swarm using CLI tools when starting complex tasks
- MUST spawn concurrent agents using Claude Code's Task tool
- Never use CLI tools alone for execution — Task tool agents do the actual work
- MUST call CLI tools AND Task tool in ONE message for complex work

### 3-Tier Model Routing (ADR-026)

| Tier | Handler | Latency | Cost | Use Cases |
|------|---------|---------|------|-----------|
| **1** | Agent Booster (WASM) | <1ms | $0 | Simple transforms (var→const, add types) — Skip LLM |
| **2** | Haiku | ~500ms | $0.0002 | Simple tasks, low complexity (<30%) |
| **3** | Sonnet/Opus | 2-5s | $0.003-0.015 | Complex reasoning, architecture, security (>30%) |

- Always check for `[AGENT_BOOSTER_AVAILABLE]` or `[TASK_MODEL_RECOMMENDATION]` before spawning agents
- Use Edit tool directly when `[AGENT_BOOSTER_AVAILABLE]`

## Swarm Configuration & Anti-Drift

- ALWAYS use hierarchical topology for coding swarms
- Keep maxAgents at 6-8 for tight coordination
- Use specialized strategy for clear role boundaries
- Use `raft` consensus for hive-mind (leader maintains authoritative state)
- Run frequent checkpoints via `post-task` hooks
- Keep shared memory namespace for all agents

```bash
npx @claude-flow/cli@latest swarm init --topology hierarchical --max-agents 8 --strategy specialized
```

## Swarm Execution Rules

- ALWAYS use `run_in_background: true` for all agent Task calls
- ALWAYS put ALL agent Task calls in ONE message for parallel execution
- After spawning, STOP — do NOT add more tool calls or check status
- Never poll TaskOutput or check swarm status — trust agents to return
- When agent results arrive, review ALL results before proceeding

## V3 CLI Commands

### Core Commands

| Command | Subcommands | Description |
|---------|-------------|-------------|
| `init` | 4 | Project initialization |
| `agent` | 8 | Agent lifecycle management |
| `swarm` | 6 | Multi-agent swarm coordination |
| `memory` | 11 | AgentDB memory with HNSW search |
| `task` | 6 | Task creation and lifecycle |
| `session` | 7 | Session state management |
| `hooks` | 17 | Self-learning hooks + 12 workers |
| `hive-mind` | 6 | Byzantine fault-tolerant consensus |

### Quick CLI Examples

```bash
npx @claude-flow/cli@latest init --wizard
npx @claude-flow/cli@latest agent spawn -t coder --name my-coder
npx @claude-flow/cli@latest swarm init --v3-mode
npx @claude-flow/cli@latest memory search --query "authentication patterns"
npx @claude-flow/cli@latest doctor --fix
```

## Available Agents (60+ Types)

### Core Development
`coder`, `reviewer`, `tester`, `planner`, `researcher`

### Specialized
`security-architect`, `security-auditor`, `memory-specialist`, `performance-engineer`

### Swarm Coordination
`hierarchical-coordinator`, `mesh-coordinator`, `adaptive-coordinator`

### GitHub & Repository
`pr-manager`, `code-review-swarm`, `issue-tracker`, `release-manager`

### SPARC Methodology
`sparc-coord`, `sparc-coder`, `specification`, `pseudocode`, `architecture`

## Memory Commands Reference

```bash
# Store (REQUIRED: --key, --value; OPTIONAL: --namespace, --ttl, --tags)
npx @claude-flow/cli@latest memory store --key "pattern-auth" --value "JWT with refresh" --namespace patterns

# Search (REQUIRED: --query; OPTIONAL: --namespace, --limit, --threshold)
npx @claude-flow/cli@latest memory search --query "authentication patterns"

# List (OPTIONAL: --namespace, --limit)
npx @claude-flow/cli@latest memory list --namespace patterns --limit 10

# Retrieve (REQUIRED: --key; OPTIONAL: --namespace)
npx @claude-flow/cli@latest memory retrieve --key "pattern-auth" --namespace patterns
```

## Quick Setup

```bash
claude mcp add claude-flow -- npx -y @claude-flow/cli@latest
npx @claude-flow/cli@latest daemon start
npx @claude-flow/cli@latest doctor --fix
```

## Claude Code vs CLI Tools

- Claude Code's Task tool handles ALL execution: agents, file ops, code generation, git
- CLI tools handle coordination via Bash: swarm init, memory, hooks, routing
- NEVER use CLI tools as a substitute for Task tool agents

## Support

- Documentation: https://github.com/ruvnet/claude-flow
- Issues: https://github.com/ruvnet/claude-flow/issues
